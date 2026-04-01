//! Port for persisting AI conversation sessions and messages to local storage.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::ai_session::{MessageRecord, SessionRecord, SessionState};

#[async_trait]
pub trait SessionStoragePort: Send + Sync {
    /// Persist a new session record.
    async fn save_session(&self, record: &SessionRecord) -> Result<(), CoreError>;

    /// Update session state. Bumps `last_active` to now, except on Terminated
    /// which sets `terminated_at` instead.
    async fn update_session_state(
        &self,
        session_id: &str,
        state: &SessionState,
    ) -> Result<(), CoreError>;

    /// Mark session as terminated with current timestamp.
    async fn terminate_session(&self, session_id: &str) -> Result<(), CoreError>;

    /// Update cumulative token usage and turn count.
    async fn update_session_usage(
        &self,
        session_id: &str,
        input_tokens: u64,
        output_tokens: u64,
        turn_count: u32,
    ) -> Result<(), CoreError>;

    /// List sessions ordered by last_active DESC.
    async fn list_sessions(&self, limit: u32) -> Result<Vec<SessionRecord>, CoreError>;

    /// Persist a batch of messages for a session.
    async fn save_messages(
        &self,
        session_id: &str,
        messages: &[MessageRecord],
    ) -> Result<(), CoreError>;

    /// Load messages for a session, paginated, ordered by seq ASC.
    async fn load_messages(
        &self,
        session_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<MessageRecord>, CoreError>;

    /// Delete a session and all its messages (CASCADE).
    async fn delete_session(&self, session_id: &str) -> Result<(), CoreError>;

    /// Purge terminated sessions older than `retention_days` and orphaned active
    /// sessions older than `2 * retention_days` (crash recovery).
    async fn purge_expired(&self, retention_days: u32) -> Result<u32, CoreError>;

    /// Get the next seq value for a session (`MAX(seq) + 1`, or 0 if empty).
    async fn next_seq(&self, session_id: &str) -> Result<i64, CoreError>;
}
