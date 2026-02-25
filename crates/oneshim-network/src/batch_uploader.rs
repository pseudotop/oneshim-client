//!

use crossbeam::queue::SegQueue;
use oneshim_core::error::CoreError;
use oneshim_core::models::event::{Event, EventBatch};
use oneshim_core::ports::api_client::ApiClient;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, warn};

///
pub struct BatchUploader {
    api_client: Arc<dyn ApiClient>,
    queue: Arc<SegQueue<Event>>,
    queue_size: AtomicUsize,
    session_id: String,
    max_batch_size: usize,
    max_retries: u32,
    dynamic_batch: bool,
}

impl BatchUploader {
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

    pub fn with_dynamic_batch(mut self, enabled: bool) -> Self {
        self.dynamic_batch = enabled;
        self
    }

    ///
    pub fn enqueue(&self, event: Event) {
        self.queue.push(event);
        let size = self.queue_size.fetch_add(1, Ordering::Relaxed) + 1;
        debug!("event add (lock-free), current size: {size}");
    }

    pub fn enqueue_many(&self, events: Vec<Event>) {
        let count = events.len();
        for event in events {
            self.queue.push(event);
        }
        let size = self.queue_size.fetch_add(count, Ordering::Relaxed) + count;
        debug!("event {count}items add (lock-free), current size: {size}");
    }

    ///
    fn compute_batch_size(&self, queue_len: usize) -> usize {
        if !self.dynamic_batch {
            return self.max_batch_size;
        }

        if queue_len < 10 {
            queue_len // send all when queue is small
        } else if queue_len > 50 {
            (self.max_batch_size * 2).min(queue_len) // 2x batch when queue is large
        } else {
            self.max_batch_size
        }
    }

    pub async fn flush(&self) -> Result<usize, CoreError> {
        let current_size = self.queue_size.load(Ordering::Relaxed);

        if current_size == 0 {
            return Ok(0);
        }

        let batch_size = self.compute_batch_size(current_size);
        let drain_count = current_size.min(batch_size);

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

        self.queue_size.fetch_sub(actual_count, Ordering::Relaxed);

        let batch = EventBatch {
            session_id: self.session_id.clone(),
            events,
            created_at: chrono::Utc::now(),
        };

        let mut retry_delay = Duration::from_secs(1);
        for attempt in 0..=self.max_retries {
            match self.api_client.upload_batch(&batch).await {
                Ok(()) => {
                    debug!("batch upload success: {actual_count}items event");
                    return Ok(actual_count);
                }
                Err(e) => {
                    if attempt < self.max_retries {
                        warn!(
                            "batch upload failure (attempt {}/{}): {e}",
                            attempt + 1,
                            self.max_retries + 1
                        );
                        tokio::time::sleep(retry_delay).await;
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                    } else {
                        error!("batch upload final failure: {e}");
                        self.requeue_failed_events(batch.events);
                        return Err(e);
                    }
                }
            }
        }

        Ok(0)
    }

    fn requeue_failed_events(&self, events: Vec<Event>) {
        let count = events.len();
        for event in events {
            self.queue.push(event);
        }
        self.queue_size.fetch_add(count, Ordering::Relaxed);
        warn!("failure event {count}items");
    }

    pub fn queue_size(&self) -> usize {
        self.queue_size.load(Ordering::Relaxed)
    }

    pub fn stats(&self) -> BatchStats {
        BatchStats {
            queue_size: self.queue_size(),
            max_batch_size: self.max_batch_size,
            dynamic_batch_enabled: self.dynamic_batch,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BatchStats {
    pub queue_size: usize,
    pub max_batch_size: usize,
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
                Err(CoreError::Internal("mock failure".to_string()))
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
            BatchUploader::new(client, "sess_1".to_string(), 2, 3).with_dynamic_batch(false); // batch disabled
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

        for _ in 0..5 {
            uploader.enqueue(make_test_event());
        }

        let sent = uploader.flush().await.unwrap();
        assert_eq!(sent, 5); // all sent
    }

    #[tokio::test]
    async fn dynamic_batch_large_queue() {
        let client = Arc::new(MockApiClient { should_fail: false });
        let uploader = BatchUploader::new(client, "sess_1".to_string(), 20, 3);

        for _ in 0..60 {
            uploader.enqueue(make_test_event());
        }

        let sent = uploader.flush().await.unwrap();
        assert_eq!(sent, 40); // 20 * 2 = 40
    }

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
                Err(CoreError::Internal("Temporary failure".to_string()))
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
        let client = Arc::new(MockApiClient { should_fail: true });
        let uploader = BatchUploader::new(client, "sess_fail".to_string(), 100, 0); // 0 retries

        uploader.enqueue(make_test_event());
        let result = uploader.flush().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn failed_events_requeued() {
        let client = Arc::new(MockApiClient { should_fail: true });
        let uploader = BatchUploader::new(client, "sess_requeue".to_string(), 100, 0);

        uploader.enqueue(make_test_event());
        uploader.enqueue(make_test_event());
        assert_eq!(uploader.queue_size(), 2);

        let result = uploader.flush().await;
        assert!(result.is_err());
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
