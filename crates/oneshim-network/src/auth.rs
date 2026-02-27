use chrono::{DateTime, Duration, Utc};
use oneshim_core::error::CoreError;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

#[derive(Debug, Clone)]
struct TokenState {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct TokenManager {
    base_url: String,
    client: reqwest::Client,
    state: Arc<RwLock<Option<TokenState>>>,
}

impl TokenManager {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            state: Arc::new(RwLock::new(None)),
        }
    }

    /// # Arguments
    pub async fn login(&self, email: &str, password: &str) -> Result<(), CoreError> {
        let organization_id =
            std::env::var("ONESHIM_ORGANIZATION_ID").unwrap_or_else(|_| "default".to_string());
        self.login_with_org(email, password, &organization_id).await
    }

    pub async fn login_with_org(
        &self,
        email: &str,
        password: &str,
        organization_id: &str,
    ) -> Result<(), CoreError> {
        let url = format!("{}/api/v1/auth/tokens", self.base_url);
        let body = serde_json::json!({
            "identifier": email,
            "password": password,
            "organization_id": organization_id,
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Auth(format!("login request failure: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CoreError::Auth(format!("login failure ({status}): {text}")));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| CoreError::Auth(format!("Token parsing failed: {e}")))?;

        let expires_at = Utc::now() + Duration::seconds(token_resp.expires_in.unwrap_or(3600));

        let mut state = self.state.write().await;
        *state = Some(TokenState {
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token,
            expires_at,
        });

        debug!("login success, token: {expires_at}");
        Ok(())
    }

    pub async fn refresh(&self) -> Result<(), CoreError> {
        let current = {
            let state = self.state.read().await;
            state.clone()
        };

        let current = current.ok_or_else(|| CoreError::Auth("Not authenticated".to_string()))?;
        let refresh_token = current
            .refresh_token
            .ok_or_else(|| CoreError::Auth("Refresh token is missing".to_string()))?;

        let url = format!("{}/api/v1/auth/tokens/refresh", self.base_url);
        let body = serde_json::json!({
            "refresh_token": refresh_token,
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Auth(format!("token refresh request failure: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CoreError::Auth(format!(
                "token refresh failure ({status}): {text}"
            )));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| CoreError::Auth(format!("refresh Token parsing failed: {e}")))?;

        let expires_at = Utc::now() + Duration::seconds(token_resp.expires_in.unwrap_or(3600));

        let mut state = self.state.write().await;
        *state = Some(TokenState {
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token.or(Some(refresh_token)),
            expires_at,
        });

        debug!("token refresh success,: {expires_at}");
        Ok(())
    }

    pub async fn get_token(&self) -> Result<String, CoreError> {
        let needs_refresh = {
            let state = self.state.read().await;
            match &*state {
                Some(s) => Utc::now() + Duration::minutes(5) >= s.expires_at,
                None => return Err(CoreError::Auth("Not authenticated".to_string())),
            }
        };

        if needs_refresh {
            self.refresh().await.map_err(|e| {
                warn!("token refresh failure: {e}");
                CoreError::Auth(format!("Automatic token refresh failed: {e}"))
            })?;
        }

        let state = self.state.read().await;
        state
            .as_ref()
            .map(|s| s.access_token.clone())
            .ok_or_else(|| CoreError::Auth("Not authenticated".to_string()))
    }

    pub async fn verify(&self) -> Result<bool, CoreError> {
        let token = self.get_token().await?;
        let url = format!("{}/api/v1/auth/tokens/verify", self.base_url);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| CoreError::Auth(format!("token validation request failure: {e}")))?;

        Ok(resp.status().is_success())
    }

    pub async fn logout(&self) -> Result<(), CoreError> {
        let token = self.get_token().await.ok();

        if let Some(token) = token {
            let url = format!("{}/api/v1/auth/tokens", self.base_url);
            let _ = self.client.delete(&url).bearer_auth(&token).send().await;
        }

        let mut state = self.state.write().await;
        *state = None;
        debug!("logout completed");
        Ok(())
    }

    pub async fn is_authenticated(&self) -> bool {
        let state = self.state.read().await;
        state.as_ref().is_some_and(|s| Utc::now() < s.expires_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_manager_creation() {
        let tm = TokenManager::new("http://localhost:8000");
        assert_eq!(tm.base_url, "http://localhost:8000");
    }

    #[test]
    fn token_manager_trailing_slash() {
        let tm = TokenManager::new("http://localhost:8000/");
        assert_eq!(tm.base_url, "http://localhost:8000");
    }

    #[tokio::test]
    async fn unauthenticated_get_token_fails() {
        let tm = TokenManager::new("http://localhost:8000");
        assert!(tm.get_token().await.is_err());
    }

    #[tokio::test]
    async fn unauthenticated_state() {
        let tm = TokenManager::new("http://localhost:8000");
        assert!(!tm.is_authenticated().await);
    }

    #[tokio::test]
    async fn login_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/v1/auth/tokens")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"jwt_abc","refresh_token":"ref_xyz","expires_in":3600}"#)
            .create_async()
            .await;

        let tm = TokenManager::new(&server.url());
        let result = tm.login("user@test.com", "test-password-placeholder").await;
        assert!(result.is_ok());
        assert!(tm.is_authenticated().await);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn login_failure_401() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/api/v1/auth/tokens")
            .with_status(401)
            .with_body("Unauthorized")
            .create_async()
            .await;

        let tm = TokenManager::new(&server.url());
        let result = tm.login("bad@test.com", "wrong").await;
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("login failure"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn login_failure_network() {
        let tm = TokenManager::new("http://127.0.0.1:1");
        let result = tm.login("user@test.com", "pass").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn refresh_without_auth_fails() {
        let tm = TokenManager::new("http://localhost:9999");
        let result = tm.refresh().await;
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Not authenticated"));
    }

    #[tokio::test]
    async fn refresh_success() {
        let mut server = mockito::Server::new_async().await;

        let login_mock = server
            .mock("POST", "/api/v1/auth/tokens")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"old_jwt","refresh_token":"ref_tok","expires_in":3600}"#)
            .create_async()
            .await;

        let refresh_mock = server
            .mock("POST", "/api/v1/auth/tokens/refresh")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"new_jwt","refresh_token":"new_ref","expires_in":7200}"#)
            .create_async()
            .await;

        let tm = TokenManager::new(&server.url());
        tm.login("user@test.com", "pass").await.unwrap();
        login_mock.assert_async().await;

        let result = tm.refresh().await;
        assert!(result.is_ok());
        refresh_mock.assert_async().await;
    }

    #[tokio::test]
    async fn get_token_propagates_refresh_failure() {
        let mut server = mockito::Server::new_async().await;

        let login_mock = server
            .mock("POST", "/api/v1/auth/tokens")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"old_jwt","refresh_token":"ref_tok","expires_in":1}"#)
            .create_async()
            .await;

        let refresh_mock = server
            .mock("POST", "/api/v1/auth/tokens/refresh")
            .with_status(401)
            .with_body("Unauthorized")
            .create_async()
            .await;

        let tm = TokenManager::new(&server.url());
        tm.login("user@test.com", "pass").await.unwrap();
        login_mock.assert_async().await;

        let result = tm.get_token().await;
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("Automatic token refresh failed"));
        refresh_mock.assert_async().await;
    }
}
