use async_trait::async_trait;
use chrono::{DateTime, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::IntegrationCapabilityScope;

#[derive(Debug, Clone)]
pub struct IntegrationTransportConnectRequest {
    pub device_id: String,
    pub requested_scopes: Vec<IntegrationCapabilityScope>,
}

#[derive(Debug, Clone)]
pub struct IntegrationTransportConnectResponse {
    pub session_id: String,
    pub connected_at: DateTime<Utc>,
    pub granted_scopes: Vec<IntegrationCapabilityScope>,
}

#[async_trait]
pub trait IntegrationTransportClient: Send + Sync {
    async fn connect(
        &self,
        request: IntegrationTransportConnectRequest,
    ) -> Result<IntegrationTransportConnectResponse, CoreError>;

    async fn heartbeat(&self, session_id: &str) -> Result<DateTime<Utc>, CoreError>;

    async fn disconnect(&self, session_id: &str) -> Result<(), CoreError>;
}
