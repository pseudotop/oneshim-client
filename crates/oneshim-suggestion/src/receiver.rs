use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::Suggestion;
use oneshim_core::ports::api_client::{SseClient, SseEvent};
use oneshim_core::ports::notifier::DesktopNotifier;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use crate::queue::SuggestionQueue;

pub struct SuggestionReceiver {
    sse_client: Arc<dyn SseClient>,
    notifier: Option<Arc<dyn DesktopNotifier>>,
    queue: Arc<Mutex<SuggestionQueue>>,
    suggestion_tx: mpsc::Sender<Suggestion>,
}

impl SuggestionReceiver {
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

    pub async fn run(&self, session_id: &str) -> Result<(), CoreError> {
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
        {
            let mut queue = self.queue.lock().await;
            queue.push(suggestion.clone());
        }

        if let Some(notifier) = &self.notifier {
            if let Err(e) = notifier.show_suggestion(&suggestion).await {
                warn!("notification display failure: {e}");
            }
        }

        let _ = self.suggestion_tx.send(suggestion).await;
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

    #[test]
    fn suggestion_queue_default_size() {
        let queue = SuggestionQueue::new(50);
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }
}
