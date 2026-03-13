//! OAuth port — runtime abstraction for provider-managed credential flows.
//!
//! The OAuthPort trait defines the contract for starting, polling, cancelling,
//! and revoking OAuth flows against external providers.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Handle returned when an OAuth flow is initiated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthFlowHandle {
    pub flow_id: String,
    pub auth_url: String,
}

/// Status of a pending or completed OAuth flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum OAuthFlowStatus {
    Pending,
    Completed,
    Failed { error: String },
    Cancelled,
}

/// Connection status for a provider's managed credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConnectionStatus {
    pub provider_id: String,
    pub connected: bool,
    pub expires_at: Option<String>,
    pub scopes: Vec<String>,
    /// API base URL for authenticated requests (e.g., `chatgpt.com/backend-api/codex`).
    #[serde(default)]
    pub api_base_url: Option<String>,
}

/// OAuth runtime port.
///
/// Implementations coordinate PKCE, callback server, token exchange,
/// and secure storage.
#[async_trait]
pub trait OAuthPort: Send + Sync {
    /// Start an OAuth authorization flow. Returns a handle with the auth URL
    /// that should be opened in the user's browser.
    async fn start_flow(&self, provider_id: &str) -> Result<OAuthFlowHandle, CoreError>;

    /// Check the status of a pending flow.
    async fn flow_status(&self, flow_id: &str) -> Result<OAuthFlowStatus, CoreError>;

    /// Cancel a pending flow and shut down the callback server.
    async fn cancel_flow(&self, flow_id: &str) -> Result<(), CoreError>;

    /// Get an active access token for a provider, refreshing if needed.
    /// Returns `None` if not authenticated.
    async fn get_access_token(&self, provider_id: &str) -> Result<Option<String>, CoreError>;

    /// Revoke stored credentials and disconnect.
    async fn revoke(&self, provider_id: &str) -> Result<(), CoreError>;

    /// Get connection status for a provider.
    async fn connection_status(
        &self,
        provider_id: &str,
    ) -> Result<OAuthConnectionStatus, CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_handle_serialization() {
        let handle = OAuthFlowHandle {
            flow_id: "abc-123".to_string(),
            auth_url: "https://auth.openai.com/oauth/authorize?foo=bar".to_string(),
        };
        let json = serde_json::to_string(&handle).unwrap();
        let parsed: OAuthFlowHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.flow_id, "abc-123");
    }

    #[test]
    fn flow_status_serialization() {
        let statuses = vec![
            OAuthFlowStatus::Pending,
            OAuthFlowStatus::Completed,
            OAuthFlowStatus::Failed {
                error: "timeout".to_string(),
            },
            OAuthFlowStatus::Cancelled,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let parsed: OAuthFlowStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, status);
        }
    }

    #[test]
    fn connection_status_serialization() {
        let status = OAuthConnectionStatus {
            provider_id: "openai".to_string(),
            connected: true,
            expires_at: Some("2026-03-14T00:00:00Z".to_string()),
            scopes: vec!["openid".to_string(), "offline_access".to_string()],
            api_base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: OAuthConnectionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider_id, "openai");
        assert!(parsed.connected);
        assert_eq!(parsed.scopes.len(), 2);
    }
}
