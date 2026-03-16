use crate::models::integration::{IntegrationAuthScheme, IntegrationTransportKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IntegrationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bootstrap_url: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    #[serde(default)]
    pub resource_indicator: Option<String>,
    #[serde(default)]
    pub auth_token_env_var: Option<String>,
    #[serde(default = "default_integration_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_integration_preferred_transports")]
    pub preferred_transports: Vec<IntegrationTransportKind>,
    #[serde(default = "default_integration_supported_auth_schemes")]
    pub supported_auth_schemes: Vec<IntegrationAuthScheme>,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bootstrap_url: None,
            device_id: None,
            device_label: None,
            resource_indicator: None,
            auth_token_env_var: None,
            request_timeout_secs: default_integration_request_timeout_secs(),
            preferred_transports: default_integration_preferred_transports(),
            supported_auth_schemes: default_integration_supported_auth_schemes(),
        }
    }
}

pub(crate) fn default_integration_request_timeout_secs() -> u64 {
    15
}

fn default_integration_preferred_transports() -> Vec<IntegrationTransportKind> {
    vec![
        IntegrationTransportKind::WebSocket,
        IntegrationTransportKind::HttpsSse,
        IntegrationTransportKind::HttpsLongPoll,
    ]
}

fn default_integration_supported_auth_schemes() -> Vec<IntegrationAuthScheme> {
    vec![IntegrationAuthScheme::BearerToken]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn integration_config_defaults_are_safe() {
        let config = IntegrationConfig::default();
        assert!(!config.enabled);
        assert!(config.bootstrap_url.is_none());
        assert_eq!(config.request_timeout_secs, 15);
        assert_eq!(
            config.preferred_transports,
            vec![
                IntegrationTransportKind::WebSocket,
                IntegrationTransportKind::HttpsSse,
                IntegrationTransportKind::HttpsLongPoll,
            ]
        );
        assert_eq!(
            config.supported_auth_schemes,
            vec![IntegrationAuthScheme::BearerToken]
        );
    }

    #[test]
    fn integration_config_deserializes_custom_values() {
        let parsed: IntegrationConfig = serde_json::from_value(json!({
            "enabled": true,
            "bootstrap_url": "https://integration.example.com/bootstrap",
            "device_id": "device-001",
            "device_label": "workstation",
            "resource_indicator": "https://integration.example.com",
            "auth_token_env_var": "ONESHIM_INTEGRATION_TOKEN",
            "request_timeout_secs": 20,
            "preferred_transports": ["web_socket", "https_long_poll"],
            "supported_auth_schemes": ["bearer_token"]
        }))
        .unwrap();

        assert!(parsed.enabled);
        assert_eq!(
            parsed.bootstrap_url.as_deref(),
            Some("https://integration.example.com/bootstrap")
        );
        assert_eq!(parsed.device_id.as_deref(), Some("device-001"));
        assert_eq!(
            parsed.preferred_transports,
            vec![
                IntegrationTransportKind::WebSocket,
                IntegrationTransportKind::HttpsLongPoll
            ]
        );
    }
}
