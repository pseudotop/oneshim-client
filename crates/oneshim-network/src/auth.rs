//! JWT 인증 토큰 관리.
//!
//! 서버 로그인, 토큰 갱신, 자동 만료 관리를 담당한다.

use chrono::{DateTime, Duration, Utc};
use oneshim_core::error::CoreError;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// 서버 응답 — 로그인/리프레시
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

/// 내부 토큰 상태
#[derive(Debug, Clone)]
struct TokenState {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: DateTime<Utc>,
}

/// JWT 토큰 매니저 — 로그인/갱신/만료 관리
#[derive(Clone)]
pub struct TokenManager {
    base_url: String,
    client: reqwest::Client,
    state: Arc<RwLock<Option<TokenState>>>,
}

impl TokenManager {
    /// 새 토큰 매니저 생성
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            state: Arc::new(RwLock::new(None)),
        }
    }

    /// 이메일/비밀번호 로그인 → JWT 토큰 획득
    ///
    /// # Arguments
    /// * `email` - 이메일 주소 (identifier로 사용)
    /// * `password` - 비밀번호
    pub async fn login(&self, email: &str, password: &str) -> Result<(), CoreError> {
        // organization_id가 필요한 경우 환경변수에서 가져오거나 기본값 사용
        let organization_id =
            std::env::var("ONESHIM_ORGANIZATION_ID").unwrap_or_else(|_| "default".to_string());
        self.login_with_org(email, password, &organization_id).await
    }

    /// 조직 ID 포함 로그인
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
            .map_err(|e| CoreError::Auth(format!("로그인 요청 실패: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CoreError::Auth(format!("로그인 실패 ({status}): {text}")));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| CoreError::Auth(format!("토큰 파싱 실패: {e}")))?;

        let expires_at = Utc::now() + Duration::seconds(token_resp.expires_in.unwrap_or(3600));

        let mut state = self.state.write().await;
        *state = Some(TokenState {
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token,
            expires_at,
        });

        debug!("로그인 성공, 토큰 만료: {expires_at}");
        Ok(())
    }

    /// 토큰 갱신 (refresh_token 사용)
    pub async fn refresh(&self) -> Result<(), CoreError> {
        let current = {
            let state = self.state.read().await;
            state.clone()
        };

        let current = current.ok_or_else(|| CoreError::Auth("인증되지 않음".to_string()))?;
        let refresh_token = current
            .refresh_token
            .ok_or_else(|| CoreError::Auth("리프레시 토큰 없음".to_string()))?;

        // REST 표준 경로: POST /api/v1/auth/tokens/refresh
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
            .map_err(|e| CoreError::Auth(format!("토큰 갱신 요청 실패: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CoreError::Auth(format!(
                "토큰 갱신 실패 ({status}): {text}"
            )));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| CoreError::Auth(format!("갱신 토큰 파싱 실패: {e}")))?;

        let expires_at = Utc::now() + Duration::seconds(token_resp.expires_in.unwrap_or(3600));

        let mut state = self.state.write().await;
        *state = Some(TokenState {
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token.or(Some(refresh_token)),
            expires_at,
        });

        debug!("토큰 갱신 성공, 새 만료: {expires_at}");
        Ok(())
    }

    /// 유효한 액세스 토큰 반환 (만료 임박 시 자동 갱신)
    pub async fn get_token(&self) -> Result<String, CoreError> {
        // 만료 5분 전이면 자동 갱신
        let needs_refresh = {
            let state = self.state.read().await;
            match &*state {
                Some(s) => Utc::now() + Duration::minutes(5) >= s.expires_at,
                None => return Err(CoreError::Auth("인증되지 않음".to_string())),
            }
        };

        if needs_refresh {
            if let Err(e) = self.refresh().await {
                warn!("자동 토큰 갱신 실패: {e}");
            }
        }

        let state = self.state.read().await;
        state
            .as_ref()
            .map(|s| s.access_token.clone())
            .ok_or_else(|| CoreError::Auth("인증되지 않음".to_string()))
    }

    /// 토큰 유효성 검증 (REST 표준: GET /api/v1/auth/tokens/verify)
    pub async fn verify(&self) -> Result<bool, CoreError> {
        let token = self.get_token().await?;
        let url = format!("{}/api/v1/auth/tokens/verify", self.base_url);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| CoreError::Auth(format!("토큰 검증 요청 실패: {e}")))?;

        Ok(resp.status().is_success())
    }

    /// 로그아웃 (REST 표준: DELETE /api/v1/auth/tokens)
    pub async fn logout(&self) -> Result<(), CoreError> {
        let token = self.get_token().await.ok();

        if let Some(token) = token {
            // REST 표준 경로 사용 (DELETE /tokens)
            let url = format!("{}/api/v1/auth/tokens", self.base_url);
            let _ = self.client.delete(&url).bearer_auth(&token).send().await;
        }

        let mut state = self.state.write().await;
        *state = None;
        debug!("로그아웃 완료");
        Ok(())
    }

    /// 현재 인증 상태 확인
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
        assert!(err.contains("로그인 실패"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn login_failure_network() {
        // 도달 불가 URL → 네트워크 에러
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
        assert!(err.contains("인증"));
    }

    #[tokio::test]
    async fn refresh_success() {
        let mut server = mockito::Server::new_async().await;

        // 먼저 로그인
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
}
