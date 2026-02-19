//! 배치 업로더.
//!
//! 이벤트를 배치로 모아 서버에 업로드. 재시도 + exponential backoff.
//! Phase 32 최적화: Lock-free 큐 + 스트림 압축.

use crossbeam::queue::SegQueue;
use oneshim_core::error::CoreError;
use oneshim_core::models::event::{Event, EventBatch};
use oneshim_core::ports::api_client::ApiClient;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, warn};

/// 배치 업로더 — Lock-free 이벤트 큐 → 배치 전송
///
/// Phase 32 최적화:
/// - crossbeam::SegQueue: Lock-free MPSC 큐로 enqueue 무경합
/// - AtomicUsize: 락 없이 큐 크기 추적
/// - 동적 배치 크기: 큐 크기에 따라 배치 크기 조절
pub struct BatchUploader {
    api_client: Arc<dyn ApiClient>,
    /// Lock-free 큐 — 여러 producer에서 동시 push 가능
    queue: Arc<SegQueue<Event>>,
    /// 큐 크기 (lock-free 카운터)
    queue_size: AtomicUsize,
    session_id: String,
    max_batch_size: usize,
    max_retries: u32,
    /// 동적 배치 크기 활성화
    dynamic_batch: bool,
}

impl BatchUploader {
    /// 새 배치 업로더 생성
    pub fn new(
        api_client: Arc<dyn ApiClient>,
        session_id: String,
        max_batch_size: usize,
        max_retries: u32,
    ) -> Self {
        Self {
            api_client,
            queue: Arc::new(SegQueue::new()),
            queue_size: AtomicUsize::new(0),
            session_id,
            max_batch_size,
            max_retries,
            dynamic_batch: true,
        }
    }

    /// 동적 배치 크기 설정
    pub fn with_dynamic_batch(mut self, enabled: bool) -> Self {
        self.dynamic_batch = enabled;
        self
    }

    /// 이벤트를 큐에 추가 (Lock-free)
    ///
    /// Phase 32: SegQueue.push()는 CAS 기반으로 락 없이 동작
    pub fn enqueue(&self, event: Event) {
        self.queue.push(event);
        let size = self.queue_size.fetch_add(1, Ordering::Relaxed) + 1;
        debug!("이벤트 큐 추가 (lock-free), 현재 크기: {size}");
    }

    /// 여러 이벤트를 한번에 큐에 추가 (Lock-free)
    pub fn enqueue_many(&self, events: Vec<Event>) {
        let count = events.len();
        for event in events {
            self.queue.push(event);
        }
        let size = self.queue_size.fetch_add(count, Ordering::Relaxed) + count;
        debug!("이벤트 {count}개 큐 추가 (lock-free), 현재 크기: {size}");
    }

    /// 동적 배치 크기 계산
    ///
    /// 큐 크기에 따라 배치 크기 조절:
    /// - 큐 크기 < 10: 즉시 전송 (min_batch = 1)
    /// - 큐 크기 10-50: 기본 배치 크기
    /// - 큐 크기 > 50: 2배 배치 (빠른 처리)
    fn compute_batch_size(&self, queue_len: usize) -> usize {
        if !self.dynamic_batch {
            return self.max_batch_size;
        }

        if queue_len < 10 {
            queue_len // 즉시 전송
        } else if queue_len > 50 {
            (self.max_batch_size * 2).min(queue_len) // 2배 배치
        } else {
            self.max_batch_size
        }
    }

    /// 큐에서 배치를 가져와 서버에 업로드
    pub async fn flush(&self) -> Result<usize, CoreError> {
        let current_size = self.queue_size.load(Ordering::Relaxed);

        if current_size == 0 {
            return Ok(0);
        }

        // 동적 배치 크기 계산
        let batch_size = self.compute_batch_size(current_size);
        let drain_count = current_size.min(batch_size);

        // Lock-free 큐에서 이벤트 추출
        let mut events = Vec::with_capacity(drain_count);
        for _ in 0..drain_count {
            if let Some(event) = self.queue.pop() {
                events.push(event);
            } else {
                break;
            }
        }

        let actual_count = events.len();
        if actual_count == 0 {
            return Ok(0);
        }

        // 카운터 갱신
        self.queue_size.fetch_sub(actual_count, Ordering::Relaxed);

        let batch = EventBatch {
            session_id: self.session_id.clone(),
            events,
            created_at: chrono::Utc::now(),
        };

        // exponential backoff 재시도
        let mut retry_delay = Duration::from_secs(1);
        for attempt in 0..=self.max_retries {
            match self.api_client.upload_batch(&batch).await {
                Ok(()) => {
                    debug!("배치 업로드 성공: {actual_count}개 이벤트");
                    return Ok(actual_count);
                }
                Err(e) => {
                    if attempt < self.max_retries {
                        warn!(
                            "배치 업로드 실패 (시도 {}/{}): {e}",
                            attempt + 1,
                            self.max_retries + 1
                        );
                        tokio::time::sleep(retry_delay).await;
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                    } else {
                        error!("배치 업로드 최종 실패: {e}");
                        // 실패한 이벤트를 다시 큐에 넣기
                        self.requeue_failed_events(batch.events);
                        return Err(e);
                    }
                }
            }
        }

        Ok(0)
    }

    /// 실패한 이벤트를 다시 큐에 추가
    fn requeue_failed_events(&self, events: Vec<Event>) {
        let count = events.len();
        for event in events {
            self.queue.push(event);
        }
        self.queue_size.fetch_add(count, Ordering::Relaxed);
        warn!("실패한 이벤트 {count}개 재큐잉");
    }

    /// 현재 큐 크기 (Lock-free)
    pub fn queue_size(&self) -> usize {
        self.queue_size.load(Ordering::Relaxed)
    }

    /// 배치 통계
    pub fn stats(&self) -> BatchStats {
        BatchStats {
            queue_size: self.queue_size(),
            max_batch_size: self.max_batch_size,
            dynamic_batch_enabled: self.dynamic_batch,
        }
    }
}

/// 배치 업로더 통계
#[derive(Debug, Clone)]
pub struct BatchStats {
    /// 현재 큐 크기
    pub queue_size: usize,
    /// 최대 배치 크기
    pub max_batch_size: usize,
    /// 동적 배치 활성화 여부
    pub dynamic_batch_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::event::{ContextEvent, Event};

    struct MockApiClient {
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl ApiClient for MockApiClient {
        async fn create_session(
            &self,
            client_id: &str,
        ) -> Result<oneshim_core::ports::api_client::SessionCreateResponse, CoreError> {
            Ok(oneshim_core::ports::api_client::SessionCreateResponse {
                session_id: format!("sess_{client_id}"),
                user_id: "user_1".to_string(),
                client_id: client_id.to_string(),
                capabilities: vec![],
            })
        }
        async fn end_session(&self, _session_id: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn upload_batch(&self, _batch: &EventBatch) -> Result<(), CoreError> {
            if self.should_fail {
                Err(CoreError::Internal("mock 실패".to_string()))
            } else {
                Ok(())
            }
        }
        async fn upload_context(
            &self,
            _upload: &oneshim_core::models::frame::ContextUpload,
        ) -> Result<(), CoreError> {
            Ok(())
        }
        async fn send_feedback(
            &self,
            _feedback: &oneshim_core::models::suggestion::SuggestionFeedback,
        ) -> Result<(), CoreError> {
            Ok(())
        }
        async fn send_heartbeat(&self, _session_id: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    fn make_test_event() -> Event {
        Event::Context(ContextEvent {
            app_name: "test".to_string(),
            window_title: "Test".to_string(),
            prev_app_name: None,
            timestamp: chrono::Utc::now(),
        })
    }

    #[tokio::test]
    async fn enqueue_and_flush() {
        let client = Arc::new(MockApiClient { should_fail: false });
        let uploader = BatchUploader::new(client, "sess_1".to_string(), 100, 3);

        uploader.enqueue(make_test_event());
        uploader.enqueue(make_test_event());
        assert_eq!(uploader.queue_size(), 2);

        let sent = uploader.flush().await.unwrap();
        assert_eq!(sent, 2);
        assert_eq!(uploader.queue_size(), 0);
    }

    #[tokio::test]
    async fn flush_empty_queue() {
        let client = Arc::new(MockApiClient { should_fail: false });
        let uploader = BatchUploader::new(client, "sess_1".to_string(), 100, 3);
        let sent = uploader.flush().await.unwrap();
        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn max_batch_size_limit() {
        let client = Arc::new(MockApiClient { should_fail: false });
        let uploader =
            BatchUploader::new(client, "sess_1".to_string(), 2, 3).with_dynamic_batch(false); // 동적 배치 비활성화

        for _ in 0..5 {
            uploader.enqueue(make_test_event());
        }
        assert_eq!(uploader.queue_size(), 5);

        let sent = uploader.flush().await.unwrap();
        assert_eq!(sent, 2);
        assert_eq!(uploader.queue_size(), 3);
    }

    #[tokio::test]
    async fn dynamic_batch_small_queue() {
        let client = Arc::new(MockApiClient { should_fail: false });
        let uploader = BatchUploader::new(client, "sess_1".to_string(), 100, 3);

        // 작은 큐 — 즉시 전송
        for _ in 0..5 {
            uploader.enqueue(make_test_event());
        }

        let sent = uploader.flush().await.unwrap();
        assert_eq!(sent, 5); // 전부 전송
    }

    #[tokio::test]
    async fn dynamic_batch_large_queue() {
        let client = Arc::new(MockApiClient { should_fail: false });
        let uploader = BatchUploader::new(client, "sess_1".to_string(), 20, 3);

        // 큰 큐 — 2배 배치
        for _ in 0..60 {
            uploader.enqueue(make_test_event());
        }

        let sent = uploader.flush().await.unwrap();
        assert_eq!(sent, 40); // 20 * 2 = 40
    }

    /// 1회 실패 후 성공하는 FlakeyApiClient
    struct FlakeyApiClient {
        call_count: std::sync::atomic::AtomicU32,
        fail_until: u32,
    }

    #[async_trait::async_trait]
    impl ApiClient for FlakeyApiClient {
        async fn create_session(
            &self,
            client_id: &str,
        ) -> Result<oneshim_core::ports::api_client::SessionCreateResponse, CoreError> {
            Ok(oneshim_core::ports::api_client::SessionCreateResponse {
                session_id: format!("sess_{client_id}"),
                user_id: "user_1".to_string(),
                client_id: client_id.to_string(),
                capabilities: vec![],
            })
        }
        async fn end_session(&self, _session_id: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn upload_batch(&self, _batch: &EventBatch) -> Result<(), CoreError> {
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            if count <= self.fail_until {
                Err(CoreError::Internal("일시적 실패".to_string()))
            } else {
                Ok(())
            }
        }
        async fn upload_context(
            &self,
            _upload: &oneshim_core::models::frame::ContextUpload,
        ) -> Result<(), CoreError> {
            Ok(())
        }
        async fn send_feedback(
            &self,
            _feedback: &oneshim_core::models::suggestion::SuggestionFeedback,
        ) -> Result<(), CoreError> {
            Ok(())
        }
        async fn send_heartbeat(&self, _session_id: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn retry_on_transient_failure() {
        // 1회 실패 후 성공
        let client = Arc::new(FlakeyApiClient {
            call_count: std::sync::atomic::AtomicU32::new(0),
            fail_until: 1,
        });
        let uploader = BatchUploader::new(client, "sess_retry".to_string(), 100, 3);

        uploader.enqueue(make_test_event());
        let result = uploader.flush().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[tokio::test]
    async fn max_retries_exhaustion() {
        // 항상 실패 → max_retries 초과 후 Err
        let client = Arc::new(MockApiClient { should_fail: true });
        let uploader = BatchUploader::new(client, "sess_fail".to_string(), 100, 0); // 0 retries

        uploader.enqueue(make_test_event());
        let result = uploader.flush().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn failed_events_requeued() {
        // 실패 시 이벤트가 큐에 복원되는지 확인
        let client = Arc::new(MockApiClient { should_fail: true });
        let uploader = BatchUploader::new(client, "sess_requeue".to_string(), 100, 0);

        uploader.enqueue(make_test_event());
        uploader.enqueue(make_test_event());
        assert_eq!(uploader.queue_size(), 2);

        let result = uploader.flush().await;
        assert!(result.is_err());
        // 실패한 이벤트가 다시 큐에 들어감
        assert_eq!(uploader.queue_size(), 2);
    }

    #[test]
    fn lock_free_concurrent_enqueue() {
        use std::thread;

        let client = Arc::new(MockApiClient { should_fail: false });
        let uploader = Arc::new(BatchUploader::new(
            client,
            "sess_concurrent".to_string(),
            100,
            3,
        ));

        // 10개 스레드에서 동시에 100개씩 enqueue
        let mut handles = vec![];
        for _ in 0..10 {
            let uploader = Arc::clone(&uploader);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    uploader.enqueue(make_test_event());
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // 1000개 이벤트가 모두 큐에 들어가야 함
        assert_eq!(uploader.queue_size(), 1000);
    }

    #[test]
    fn batch_stats() {
        let client = Arc::new(MockApiClient { should_fail: false });
        let uploader = BatchUploader::new(client, "sess_stats".to_string(), 50, 3);

        for _ in 0..25 {
            uploader.enqueue(make_test_event());
        }

        let stats = uploader.stats();
        assert_eq!(stats.queue_size, 25);
        assert_eq!(stats.max_batch_size, 50);
        assert!(stats.dynamic_batch_enabled);
    }
}
