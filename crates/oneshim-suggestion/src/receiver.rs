use oneshim_core::models::suggestion::Suggestion;
use oneshim_core::ports::api_client::{SseClient, SseEvent};
use oneshim_core::ports::notifier::DesktopNotifier;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use crate::error::SuggestionError;

use crate::queue::SuggestionQueue;

pub struct SuggestionReceiver {
    sse_client: Arc<dyn SseClient>,
    notifier: Option<Arc<dyn DesktopNotifier>>,
    queue: Arc<Mutex<SuggestionQueue>>,
}

impl SuggestionReceiver {
    pub fn new(
        sse_client: Arc<dyn SseClient>,
        notifier: Option<Arc<dyn DesktopNotifier>>,
        queue: Arc<Mutex<SuggestionQueue>>,
    ) -> Self {
        Self {
            sse_client,
            notifier,
            queue,
        }
    }

    pub async fn run(&self, session_id: &str) -> Result<(), SuggestionError> {
        let (tx, mut rx) = mpsc::channel::<SseEvent>(64);

        let sse = self.sse_client.clone();
        let sid = session_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = sse.connect(&sid, tx).await {
                error!("SSE connection error: {e}");
            }
        });

        info!("suggestion received waiting started");

        while let Some(event) = rx.recv().await {
            match event {
                SseEvent::Connected { session_id } => {
                    info!("SSE connection success: {session_id}");
                }
                SseEvent::Suggestion(suggestion) => {
                    debug!(
                        "suggestion received: {} ({:?})",
                        suggestion.suggestion_id, suggestion.priority
                    );
                    self.handle_suggestion(suggestion).await;
                }
                SseEvent::Update(data) => {
                    debug!("update received: {data}");
                }
                SseEvent::Heartbeat { timestamp } => {
                    debug!("heartbeat: {timestamp}");
                }
                SseEvent::Error(msg) => {
                    warn!("SSE error: {msg}");
                }
                SseEvent::Close => {
                    info!("SSE connection ended");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_suggestion(&self, suggestion: Suggestion) {
        let accepted = {
            let mut queue = self.queue.lock().await;
            queue.push(suggestion.clone())
        };

        if !accepted {
            // Queue full and this suggestion has lower priority — skip notification
            return;
        }

        if let Some(notifier) = &self.notifier {
            if let Err(e) = notifier.show_suggestion(&suggestion).await {
                warn!("notification display failure: {e}");
            }
        }
    }

    pub async fn queue_size(&self) -> usize {
        self.queue.lock().await.len()
    }

    pub async fn peek_top(&self) -> Option<Suggestion> {
        self.queue.lock().await.peek().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::error::CoreError;
    use oneshim_core::models::suggestion::{Priority, SuggestionSource, SuggestionType};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn suggestion_queue_default_size() {
        let queue = SuggestionQueue::new(50);
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    struct MockSseClient;
    #[async_trait::async_trait]
    impl SseClient for MockSseClient {
        async fn connect(
            &self,
            _session_id: &str,
            _tx: tokio::sync::mpsc::Sender<SseEvent>,
        ) -> Result<(), CoreError> {
            Ok(())
        }
    }

    struct CountingNotifier {
        count: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl DesktopNotifier for CountingNotifier {
        async fn show_suggestion(&self, _suggestion: &Suggestion) -> Result<(), CoreError> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn show_notification(&self, _title: &str, _body: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn show_error(&self, _message: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    fn make_suggestion() -> Suggestion {
        Suggestion {
            suggestion_id: "test-1".to_string(),
            suggestion_type: SuggestionType::WorkGuidance,
            content: "Test suggestion content".to_string(),
            priority: Priority::Medium,
            confidence_score: 0.8,
            relevance_score: 0.9,
            is_actionable: true,
            created_at: chrono::Utc::now(),
            expires_at: None,
            source: SuggestionSource::RuleBased,
            reasoning: None,
        }
    }

    #[tokio::test]
    async fn handle_suggestion_calls_notifier() {
        let notifier = Arc::new(CountingNotifier {
            count: AtomicUsize::new(0),
        });
        let queue = Arc::new(Mutex::new(SuggestionQueue::new(50)));
        let receiver = SuggestionReceiver::new(
            Arc::new(MockSseClient) as Arc<dyn SseClient>,
            Some(notifier.clone() as Arc<dyn DesktopNotifier>),
            queue.clone(),
        );

        receiver.handle_suggestion(make_suggestion()).await;

        assert_eq!(notifier.count.load(Ordering::SeqCst), 1);
        assert_eq!(queue.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn handle_suggestion_works_without_notifier() {
        let queue = Arc::new(Mutex::new(SuggestionQueue::new(50)));
        let receiver = SuggestionReceiver::new(
            Arc::new(MockSseClient) as Arc<dyn SseClient>,
            None,
            queue.clone(),
        );

        receiver.handle_suggestion(make_suggestion()).await;

        assert_eq!(queue.lock().await.len(), 1);
    }
}
