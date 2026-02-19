//! 제안 수신기.
//!
//! SSE 이벤트 → Suggestion 변환 → 큐 추가 → 알림 트리거.

use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::Suggestion;
use oneshim_core::ports::api_client::{SseClient, SseEvent};
use oneshim_core::ports::notifier::DesktopNotifier;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use crate::queue::SuggestionQueue;

/// 제안 수신기 — SSE 스트림 → 제안 큐 + 알림
pub struct SuggestionReceiver {
    sse_client: Arc<dyn SseClient>,
    notifier: Option<Arc<dyn DesktopNotifier>>,
    queue: Arc<Mutex<SuggestionQueue>>,
    /// 외부에서 새 제안을 구독할 수 있는 채널
    suggestion_tx: mpsc::Sender<Suggestion>,
}

impl SuggestionReceiver {
    /// 새 수신기 생성
    pub fn new(
        sse_client: Arc<dyn SseClient>,
        notifier: Option<Arc<dyn DesktopNotifier>>,
        queue: Arc<Mutex<SuggestionQueue>>,
        suggestion_tx: mpsc::Sender<Suggestion>,
    ) -> Self {
        Self {
            sse_client,
            notifier,
            queue,
            suggestion_tx,
        }
    }

    /// SSE 스트림 수신 시작 (블로킹)
    pub async fn run(&self, session_id: &str) -> Result<(), CoreError> {
        let (tx, mut rx) = mpsc::channel::<SseEvent>(64);

        // SSE 연결 태스크 시작
        let sse = self.sse_client.clone();
        let sid = session_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = sse.connect(&sid, tx).await {
                error!("SSE 연결 에러: {e}");
            }
        });

        info!("제안 수신 대기 시작");

        // 이벤트 처리 루프
        while let Some(event) = rx.recv().await {
            match event {
                SseEvent::Connected { session_id } => {
                    info!("SSE 연결 성공: {session_id}");
                }
                SseEvent::Suggestion(suggestion) => {
                    debug!(
                        "제안 수신: {} ({:?})",
                        suggestion.suggestion_id, suggestion.priority
                    );
                    self.handle_suggestion(suggestion).await;
                }
                SseEvent::Update(data) => {
                    debug!("업데이트 수신: {data}");
                }
                SseEvent::Heartbeat { timestamp } => {
                    debug!("하트비트: {timestamp}");
                }
                SseEvent::Error(msg) => {
                    warn!("SSE 에러: {msg}");
                }
                SseEvent::Close => {
                    info!("SSE 연결 종료");
                    break;
                }
            }
        }

        Ok(())
    }

    /// 제안 처리: 큐 추가 + 알림 + 외부 채널 전송
    async fn handle_suggestion(&self, suggestion: Suggestion) {
        // 큐에 추가
        {
            let mut queue = self.queue.lock().await;
            queue.push(suggestion.clone());
        }

        // 데스크톱 알림
        if let Some(notifier) = &self.notifier {
            if let Err(e) = notifier.show_suggestion(&suggestion).await {
                warn!("알림 표시 실패: {e}");
            }
        }

        // 외부 구독자에게 전달
        let _ = self.suggestion_tx.send(suggestion).await;
    }

    /// 현재 큐 크기
    pub async fn queue_size(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// 가장 높은 우선순위 제안 조회
    pub async fn peek_top(&self) -> Option<Suggestion> {
        self.queue.lock().await.peek().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestion_queue_default_size() {
        let queue = SuggestionQueue::new(50);
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }
}
