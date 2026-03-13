//! Credential source abstraction for provider authentication.
//!
//! Replaces direct `String api_key` dependency with a resolution-time
//! abstraction that supports both BYOK API keys and managed OAuth tokens.

use std::sync::Arc;

use crate::error::CoreError;
use crate::ports::oauth::OAuthPort;

/// Source of authentication credentials for AI provider requests.
#[derive(Clone)]
pub enum CredentialSource {
    /// User-provided API key (current default, BYOK).
    ApiKey(String),

    /// Managed OAuth token resolved at request time via OAuthPort.
    ///
    /// `api_base_url` is the provider's API endpoint for OAuth-authenticated
    /// requests. For OpenAI ChatGPT OAuth this is `chatgpt.com/backend-api/codex`,
    /// NOT the standard `api.openai.com/v1`.
    ManagedOAuth {
        provider_id: String,
        oauth_port: Arc<dyn OAuthPort>,
        /// API base URL for authenticated requests (differs per auth mode).
        api_base_url: String,
    },
}

impl CredentialSource {
    /// Resolve to a bearer token string at request time.
    ///
    /// For `ApiKey`, returns the key directly.
    /// For `ManagedOAuth`, calls `OAuthPort::get_access_token` (may trigger refresh).
    pub async fn resolve_bearer_token(&self) -> Result<String, CoreError> {
        match self {
            Self::ApiKey(key) => Ok(key.clone()),
            Self::ManagedOAuth {
                provider_id,
                oauth_port,
                ..
            } => oauth_port
                .get_access_token(provider_id)
                .await?
                .ok_or_else(|| CoreError::OAuthError {
                    provider: provider_id.clone(),
                    message: "not authenticated — please connect via OAuth".into(),
                }),
        }
    }

    /// Whether this source is a managed OAuth credential.
    pub fn is_managed(&self) -> bool {
        matches!(self, Self::ManagedOAuth { .. })
    }

    /// API base URL for the credential source.
    ///
    /// Returns `Some(url)` for `ManagedOAuth`, `None` for `ApiKey`
    /// (API key users configure their own endpoint).
    pub fn api_base_url(&self) -> Option<&str> {
        match self {
            Self::ApiKey(_) => None,
            Self::ManagedOAuth { api_base_url, .. } => Some(api_base_url),
        }
    }
}

impl std::fmt::Debug for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey(_) => write!(f, "CredentialSource::ApiKey(****)"),
            Self::ManagedOAuth { provider_id, .. } => {
                write!(f, "CredentialSource::ManagedOAuth({provider_id})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn api_key_resolves_directly() {
        let source = CredentialSource::ApiKey("sk-test-key".to_string());
        let token = source.resolve_bearer_token().await.unwrap();
        assert_eq!(token, "sk-test-key");
        assert!(!source.is_managed());
    }

    #[test]
    fn debug_redacts_api_key() {
        let source = CredentialSource::ApiKey("sk-secret".to_string());
        let debug = format!("{source:?}");
        assert!(!debug.contains("sk-secret"));
        assert!(debug.contains("****"));
    }
}
