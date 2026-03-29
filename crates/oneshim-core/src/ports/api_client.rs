//! Server API client port — defines the contract for all HTTP/gRPC
//! communication with the ONESHIM server (session, batch upload, heartbeat).
//! Implemented by `HttpApiClient` and `UnifiedClient` in `oneshim-network`.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::event::EventBatch;
use crate::models::frame::ContextUpload;
use crate::models::suggestion::{Suggestion, SuggestionFeedback};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionCreateResponse {
    pub session_id: String,
    pub user_id: String,
    pub client_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[async_trait]
pub trait ApiClient: Send + Sync {
    /// Create a server session for the given client.
    ///
    /// # Errors
    /// Returns `CoreError::Network` on connection failure, `CoreError::Auth` on
    /// authentication failure, `CoreError::ServiceUnavailable` when the server is down.
    async fn create_session(&self, client_id: &str) -> Result<SessionCreateResponse, CoreError>;

    /// End an active server session.
    ///
    /// # Errors
    /// Returns `CoreError::Network` on connection failure.
    async fn end_session(&self, session_id: &str) -> Result<(), CoreError>;

    /// Upload an event batch to the server.
    ///
    /// # Errors
    /// Returns `CoreError::Network` on connection failure, `CoreError::RateLimit`
    /// when the server throttles uploads.
    async fn upload_batch(&self, batch: &EventBatch) -> Result<(), CoreError>;

    /// Upload context data (frames + metadata) to the server.
    ///
    /// # Errors
    /// Returns `CoreError::Network` on connection failure.
    async fn upload_context(&self, upload: &ContextUpload) -> Result<(), CoreError>;

    /// Send suggestion feedback (accept/reject) to the server.
    ///
    /// # Errors
    /// Returns `CoreError::Network` on connection failure.
    async fn send_feedback(&self, feedback: &SuggestionFeedback) -> Result<(), CoreError>;

    /// Send a session heartbeat to keep the server session alive.
    ///
    /// # Errors
    /// Returns `CoreError::Network` on connection failure.
    async fn send_heartbeat(&self, session_id: &str) -> Result<(), CoreError>;
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum SseEvent {
    Connected {
        session_id: String,
    },
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
    /// Connect to the server SSE stream for real-time suggestion delivery.
    ///
    /// # Errors
    /// Returns `CoreError::Network` on connection failure, `CoreError::Auth`
    /// on invalid session.
    async fn connect(
        &self,
        session_id: &str,
        tx: tokio::sync::mpsc::Sender<SseEvent>,
    ) -> Result<(), CoreError>;
}
