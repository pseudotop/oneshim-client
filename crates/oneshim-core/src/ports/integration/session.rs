//! Integration session management ports.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::integration::{
    IntegrationAckCursor, IntegrationCapabilityScope, IntegrationRuntimeTelemetry,
    IntegrationSessionState,
};

#[async_trait]
pub trait IntegrationSessionPort: Send + Sync {
    /// Connect or resume an outbound integration session for the requested scopes.
    async fn connect(
        &self,
        requested_scopes: Vec<IntegrationCapabilityScope>,
    ) -> Result<IntegrationSessionState, CoreError>;

    /// Return the current session state, if one exists.
    async fn current_session(&self) -> Result<Option<IntegrationSessionState>, CoreError>;

    /// Send a liveness heartbeat and return the refreshed session state.
    async fn heartbeat(&self, session_id: &str) -> Result<IntegrationSessionState, CoreError>;

    /// Persist the latest acknowledged cursor for the active session.
    async fn store_ack_cursor(
        &self,
        session_id: &str,
        cursor: IntegrationAckCursor,
    ) -> Result<IntegrationSessionState, CoreError>;

    /// Disconnect an established integration session.
    async fn disconnect(&self, session_id: &str) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationSessionStorePort: Send + Sync {
    /// Load the last persisted integration session state, if one exists.
    async fn load(&self) -> Result<Option<IntegrationSessionState>, CoreError>;

    /// Persist the latest integration session state snapshot.
    async fn store(&self, state: IntegrationSessionState) -> Result<(), CoreError>;

    /// Clear any persisted integration session state.
    async fn clear(&self) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationRuntimeTelemetryPort: Send + Sync {
    /// Return the latest runtime telemetry snapshot for integration background loops.
    async fn snapshot(&self) -> Result<IntegrationRuntimeTelemetry, CoreError>;
}
