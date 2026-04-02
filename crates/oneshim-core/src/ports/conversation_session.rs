//! Conversation session ports — defines contracts for AI conversation
//! session management across CLI subprocess, HTTP API, and local LLM backends.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;

use crate::error::CoreError;
use crate::models::ai_session::{
    ConversationSessionInfo, OutboundMessage, SessionConfig, SessionMessage, SessionState,
};

/// Streaming response from a conversation session.
pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<OutboundMessage, CoreError>> + Send>>;

#[async_trait]
pub trait ConversationSession: Send + Sync {
    /// Send a message with optional attachments, receive streaming response.
    ///
    /// # Errors
    /// Returns `CoreError::Internal` if the provider subprocess or connection
    /// fails, `CoreError::Network` on HTTP/streaming errors.
    async fn send_message(&self, message: &SessionMessage) -> Result<ResponseStream, CoreError>;

    /// Current session info (synchronous — returns locally held state).
    fn info(&self) -> ConversationSessionInfo;

    /// Unique session identifier.
    fn session_id(&self) -> &str;

    /// Provider display name.
    fn provider_name(&self) -> &str;

    /// Gracefully terminate the session, releasing provider resources.
    /// Default implementation is a no-op for backwards compatibility.
    async fn terminate(&self) {}
}

#[async_trait]
pub trait SessionManager: Send + Sync {
    /// Create a new session with the given provider.
    ///
    /// # Errors
    /// Returns `CoreError::Internal` if max concurrent sessions reached or
    /// provider detection fails, `CoreError::InvalidArguments` if required
    /// config fields are missing, `CoreError::Auth` if credentials unavailable.
    async fn create_session(
        &self,
        config: SessionConfig,
    ) -> Result<Arc<dyn ConversationSession>, CoreError>;

    /// Terminate a session.
    ///
    /// # Errors
    /// Returns `CoreError::Internal` if the session ID is not found.
    async fn kill_session(&self, session_id: &str) -> Result<(), CoreError>;

    /// List active sessions.
    async fn list_sessions(&self) -> Vec<ConversationSessionInfo>;

    /// Retrieve a session by ID.
    ///
    /// # Errors
    /// Returns `CoreError::Internal` if the session ID is not found.
    async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn ConversationSession>, CoreError>;

    /// Recover a failed session when retry budget remains.
    async fn recover_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn ConversationSession>, CoreError>;

    /// Reset idle timer for a session (keeps it alive during active use).
    async fn touch_session(&self, session_id: &str);

    /// Report an adapter-level failure. Returns the resulting session state.
    async fn report_failure(&self, session_id: &str, error: &CoreError) -> SessionState;

    /// Terminate all active sessions during app shutdown.
    async fn shutdown_all(&self);
}
