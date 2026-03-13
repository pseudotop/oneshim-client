//! OAuth token exchange — authorization code → access + refresh tokens.

use oneshim_core::error::CoreError;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::provider_config::OAuthProviderConfig;

/// Successful token exchange result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenExchangeResult {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
}

/// Error response from the token endpoint.
#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// Exchange an authorization code for tokens.
pub async fn exchange_code(
    http: &reqwest::Client,
    config: &OAuthProviderConfig,
    code: &str,
    pkce_verifier: &str,
) -> Result<TokenExchangeResult, CoreError> {
    let redirect_uri = config.redirect_uri();

    debug!(
        "exchanging authorization code for tokens (provider: {})",
        config.provider_id
    );

    let resp = http
        .post(&config.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", config.client_id.as_str()),
            ("code_verifier", pkce_verifier),
        ])
        .send()
        .await
        .map_err(|e| CoreError::OAuthError {
            provider: config.provider_id.clone(),
            message: format!("token exchange request failed: {e}"),
        })?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| CoreError::OAuthError {
        provider: config.provider_id.clone(),
        message: format!("failed to read token response: {e}"),
    })?;

    if !status.is_success() {
        if let Ok(err) = serde_json::from_str::<TokenErrorResponse>(&body) {
            let desc = err.error_description.unwrap_or_default();
            warn!(
                "token exchange failed: {} — {} (status: {})",
                err.error, desc, status
            );
            return Err(CoreError::OAuthError {
                provider: config.provider_id.clone(),
                message: format!("{}: {desc}", err.error),
            });
        }
        return Err(CoreError::OAuthError {
            provider: config.provider_id.clone(),
            message: format!("token exchange returned {status}"),
        });
    }

    let result: TokenExchangeResult =
        serde_json::from_str(&body).map_err(|e| CoreError::OAuthError {
            provider: config.provider_id.clone(),
            message: format!("failed to parse token response: {e}"),
        })?;

    debug!(
        "token exchange successful (expires_in: {:?}, has_refresh: {})",
        result.expires_in,
        result.refresh_token.is_some()
    );

    Ok(result)
}

/// Refresh an access token using a refresh token.
pub async fn refresh_token(
    http: &reqwest::Client,
    config: &OAuthProviderConfig,
    refresh_tok: &str,
) -> Result<TokenExchangeResult, CoreError> {
    debug!("refreshing access token (provider: {})", config.provider_id);

    let resp = http
        .post(&config.token_endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_tok),
            ("client_id", config.client_id.as_str()),
        ])
        .send()
        .await
        .map_err(|e| CoreError::OAuthError {
            provider: config.provider_id.clone(),
            message: format!("token refresh request failed: {e}"),
        })?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| CoreError::OAuthError {
        provider: config.provider_id.clone(),
        message: format!("failed to read refresh response: {e}"),
    })?;

    if !status.is_success() {
        if let Ok(err) = serde_json::from_str::<TokenErrorResponse>(&body) {
            let desc = err.error_description.unwrap_or_default();
            return Err(CoreError::OAuthError {
                provider: config.provider_id.clone(),
                message: format!("refresh failed: {}: {desc}", err.error),
            });
        }
        return Err(CoreError::OAuthError {
            provider: config.provider_id.clone(),
            message: format!("token refresh returned {status}"),
        });
    }

    serde_json::from_str(&body).map_err(|e| CoreError::OAuthError {
        provider: config.provider_id.clone(),
        message: format!("failed to parse refresh response: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_result_deserializes_minimal() {
        let json = r#"{"access_token": "at_123", "token_type": "Bearer"}"#;
        let result: TokenExchangeResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.access_token, "at_123");
        assert!(result.refresh_token.is_none());
        assert!(result.expires_in.is_none());
    }

    #[test]
    fn token_result_deserializes_full() {
        let json = r#"{
            "access_token": "at_abc",
            "refresh_token": "rt_xyz",
            "expires_in": 3600,
            "scope": "openid profile",
            "token_type": "Bearer"
        }"#;
        let result: TokenExchangeResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.access_token, "at_abc");
        assert_eq!(result.refresh_token.as_deref(), Some("rt_xyz"));
        assert_eq!(result.expires_in, Some(3600));
        assert_eq!(result.scope.as_deref(), Some("openid profile"));
    }

    #[test]
    fn error_response_deserializes() {
        let json = r#"{"error": "invalid_grant", "error_description": "code expired"}"#;
        let err: TokenErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err.error, "invalid_grant");
        assert_eq!(err.error_description.as_deref(), Some("code expired"));
    }
}
