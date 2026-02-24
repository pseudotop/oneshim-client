//!

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::event::EventBatch;
use crate::models::frame::ContextUpload;
use crate::models::suggestion::{Suggestion, SuggestionFeedback};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionCreateResponse {
    pub session_id: String,
    pub user_id: String,
    pub client_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[async_trait]
pub trait ApiClient: Send + Sync {
    ///
    async fn create_session(&self, client_id: &str) -> Result<SessionCreateResponse, CoreError>;

    ///
    async fn end_session(&self, session_id: &str) -> Result<(), CoreError>;

    async fn upload_batch(&self, batch: &EventBatch) -> Result<(), CoreError>;

    async fn upload_context(&self, upload: &ContextUpload) -> Result<(), CoreError>;

    async fn send_feedback(&self, feedback: &SuggestionFeedback) -> Result<(), CoreError>;

    async fn send_heartbeat(&self, session_id: &str) -> Result<(), CoreError>;
}

#[derive(Debug, Clone)]
pub enum SseEvent {
    Connected { session_id: String },
    Suggestion(Suggestion),
    Update(serde_json::Value),
    Heartbeat {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Error(String),
    Close,
}

#[async_trait]
pub trait SseClient: Send + Sync {
    ///
    async fn connect(
        &self,
        session_id: &str,
        tx: tokio::sync::mpsc::Sender<SseEvent>,
    ) -> Result<(), CoreError>;
}
