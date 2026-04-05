//! Integration authentication ports.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::integration::{
    IntegrationAuthContext, IntegrationAuthStatus, IntegrationCapabilityScope,
    IntegrationDeviceAuthorizationFlow,
};

#[async_trait]
pub trait IntegrationAuthPort: Send + Sync {
    /// Resolve outbound session auth material for the requested scopes and resource.
    async fn resolve_session_auth(
        &self,
        requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationAuthContext, CoreError>;

    /// Return the current runtime status of the integration auth profile.
    async fn current_auth_status(&self) -> Result<IntegrationAuthStatus, CoreError>;

    /// Start a device authorization flow if the auth profile supports it.
    async fn start_device_authorization(
        &self,
        requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationDeviceAuthorizationFlow, CoreError>;

    /// Poll a pending device authorization flow.
    async fn poll_device_authorization(
        &self,
        flow_id: &str,
    ) -> Result<IntegrationAuthStatus, CoreError>;

    /// Cancel a pending device authorization flow.
    async fn cancel_device_authorization(&self, flow_id: &str) -> Result<(), CoreError>;

    /// Clear locally persisted auth material and pending bootstrap state so the
    /// client can recover from a broken or stale device authorization flow.
    async fn reset_auth_state(&self) -> Result<(), CoreError>;
}
