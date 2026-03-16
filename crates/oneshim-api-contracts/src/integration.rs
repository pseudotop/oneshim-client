use crate::stream::AiRuntimeStatus;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationStatus {
    pub schema_version: String,
    pub external_access_enabled: bool,
    pub automation_controller_configured: bool,
    pub ai_runtime_status: Option<AiRuntimeStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationBootstrapRequest {
    pub client_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_label: Option<String>,
    #[serde(default)]
    pub requested_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationBootstrapResponse {
    pub schema_version: String,
    #[serde(default)]
    pub supported_scopes: Vec<String>,
    #[serde(default)]
    pub transport_hints: Vec<String>,
    pub session_required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_request_roundtrip() {
        let request = IntegrationBootstrapRequest {
            client_version: "0.3.8".to_string(),
            device_label: Some("macbook".to_string()),
            requested_scopes: vec!["insight:write".to_string(), "prompt:read".to_string()],
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: IntegrationBootstrapRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.client_version, "0.3.8");
        assert_eq!(parsed.requested_scopes.len(), 2);
    }

    #[test]
    fn bootstrap_response_roundtrip() {
        let response = IntegrationBootstrapResponse {
            schema_version: "integration.bootstrap.v1".to_string(),
            supported_scopes: vec!["insight:write".to_string()],
            transport_hints: vec!["websocket".to_string()],
            session_required: true,
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: IntegrationBootstrapResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schema_version, "integration.bootstrap.v1");
        assert!(parsed.session_required);
    }
}
