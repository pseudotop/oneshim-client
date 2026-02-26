[English](./07-code-sketches.md) | [한국어](./07-code-sketches.ko.md)

# 7. Core Rust Code Sketches

[← UI Framework](./06-ui-framework.md) | [Edge Vision →](./08-edge-vision.md)

---

## Model Example (oneshim-core)

```rust
// models/suggestion.rs
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub suggestion_id: String,
    pub suggestion_type: SuggestionType,
    pub content: String,
    pub priority: Priority,
    pub confidence_score: f64,
    pub relevance_score: f64,
    pub is_actionable: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SuggestionType {
    WorkGuidance,
    EmailDraft,
    ProductivityTip,
    WorkflowOptimization,
    ContextBased,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}
```

## SSE Client Sketch (oneshim-network)

```rust
// sse_client.rs
use eventsource_client as es;
use futures::StreamExt;
use crate::auth::TokenManager;

pub struct SseClient {
    base_url: String,
    token_manager: Arc<TokenManager>,
}

impl SseClient {
    /// SSE stream connection — receives suggestions, heartbeats, and update events
    pub async fn connect(
        &self,
        session_id: &str,
        tx: tokio::sync::mpsc::Sender<SseEvent>,
    ) -> Result<(), ClientError> {
        let url = format!("{}/user_context/sessions/stream?session_id={}",
            self.base_url, session_id);
        let token = self.token_manager.get_token().await?;

        let client = es::ClientBuilder::for_url(&url)?
            .header("Authorization", &format!("Bearer {}", token))?
            .reconnect(
                es::ReconnectOptions::reconnect(true)
                    .retry_initial(true)
                    .delay(std::time::Duration::from_secs(1))
                    .backoff_factor(2)
                    .delay_max(std::time::Duration::from_secs(30))
                    .build(),
            )
            .build();

        let mut stream = client.stream();

        while let Some(event) = stream.next().await {
            match event {
                Ok(es::SSE::Event(ev)) => {
                    let sse_event = parse_sse_event(&ev)?;
                    tx.send(sse_event).await?;
                }
                Ok(es::SSE::Comment(_)) => {}
                Err(e) => {
                    tracing::warn!("SSE error: {}", e);
                    // Auto-reconnect is handled by eventsource-client
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum SseEvent {
    Connection { session_id: String },
    Suggestion(Suggestion),
    Update(serde_json::Value),
    Heartbeat { timestamp: DateTime<Utc> },
    Error(String),
    Close,
}
```

## Suggestion Reception → Notification Pipeline Sketch

```rust
// oneshim-suggestion/receiver.rs
pub struct SuggestionReceiver {
    sse_client: Arc<SseClient>,
    notifier: Arc<dyn DesktopNotifier>,
    feedback_sender: Arc<FeedbackSender>,
    queue: Arc<SuggestionQueue>,
}

impl SuggestionReceiver {
    /// Receives suggestions from SSE and delivers them to UI/notifications
    pub async fn run(&self, session_id: &str) -> Result<(), ClientError> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);

        // SSE stream connection (separate task)
        let sse = self.sse_client.clone();
        let sid = session_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = sse.connect(&sid, tx).await {
                tracing::error!("SSE connection failed: {}", e);
            }
        });

        // Event processing loop
        while let Some(event) = rx.recv().await {
            match event {
                SseEvent::Suggestion(suggestion) => {
                    // Add to queue
                    self.queue.push(suggestion.clone()).await;

                    // Desktop notification
                    self.notifier.show_suggestion(&suggestion).await?;

                    tracing::info!(
                        "Suggestion received: [{}] {}",
                        suggestion.suggestion_type,
                        &suggestion.content[..80.min(suggestion.content.len())]
                    );
                }
                SseEvent::Heartbeat { timestamp } => {
                    tracing::trace!("Heartbeat: {}", timestamp);
                }
                SseEvent::Error(msg) => {
                    tracing::warn!("Server error: {}", msg);
                }
                _ => {}
            }
        }
        Ok(())
    }
}
```
