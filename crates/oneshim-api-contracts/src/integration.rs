use crate::stream::AiRuntimeStatus;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationStatus {
    pub schema_version: String,
    pub external_access_enabled: bool,
    pub automation_controller_configured: bool,
    pub ai_runtime_status: Option<AiRuntimeStatus>,
}
