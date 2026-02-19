//! HTTP REST API 클라이언트.
//!
//! `ApiClient` 포트 구현. JWT 인증 헤더 자동 주입 + 재시도 로직.

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::models::event::EventBatch;
use oneshim_core::models::frame::ContextUpload;
use oneshim_core::models::suggestion::SuggestionFeedback;
use oneshim_core::ports::api_client::{ApiClient, SessionCreateResponse};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

use crate::auth::TokenManager;

/// 기본 재시도 횟수
const DEFAULT_MAX_RETRIES: u32 = 3;

/// 재시도 가능한 에러인지 판별
fn is_retryable(error: &CoreError) -> bool {
    matches!(
        error,
        CoreError::Network(_) | CoreError::ServiceUnavailable(_) | CoreError::RateLimit { .. }
    )
}

/// REST API 클라이언트 — `ApiClient` 포트 구현
///
/// Phase 34: 재시도 로직 + 에러 세분화 + 세션 관리
pub struct HttpApiClient {
    client: reqwest::Client,
    base_url: String,
    token_manager: Arc<TokenManager>,
    max_retries: u32,
}

impl HttpApiClient {
    /// 새 HTTP API 클라이언트 생성
    pub fn new(
        base_url: &str,
        token_manager: Arc<TokenManager>,
        timeout: Duration,
    ) -> Result<Self, CoreError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| CoreError::Network(format!("HTTP 클라이언트 빌드 실패: {}", e)))?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token_manager,
            max_retries: DEFAULT_MAX_RETRIES,
        })
    }

    /// 재시도 횟수 설정
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Authorization 헤더가 포함된 요청 빌더 반환
    async fn authorized_request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, CoreError> {
        let token = self.token_manager.get_token().await?;
        let url = format!("{}{}", self.base_url, path);
        Ok(self.client.request(method, &url).bearer_auth(token))
    }

    /// 응답 상태 코드 확인 및 에러 매핑
    ///
    /// Phase 34: 429, 503 등 상태 코드별 에러 타입 반환
    async fn check_response(
        &self,
        resp: reqwest::Response,
    ) -> Result<reqwest::Response, CoreError> {
        let status = resp.status();

        if status.is_success() {
            return Ok(resp);
        }

        let status_code = status.as_u16();
        let text = resp.text().await.unwrap_or_else(|e| {
            tracing::warn!("응답 본문 읽기 실패: {e}");
            String::new()
        });

        match status_code {
            401 => Err(CoreError::Auth(format!("인증 실패: {text}"))),
            404 => Err(CoreError::NotFound {
                resource_type: "API".to_string(),
                id: text,
            }),
            429 => {
                // Rate Limit — Retry-After 헤더 파싱 (기본 60초)
                let retry_after = 60;
                Err(CoreError::RateLimit {
                    retry_after_secs: retry_after,
                })
            }
            503 => Err(CoreError::ServiceUnavailable(text)),
            _ => Err(CoreError::Internal(format!("API 에러 ({status}): {text}"))),
        }
    }

    /// 재시도가 포함된 요청 실행
    ///
    /// exponential backoff: 1s → 2s → 4s
    async fn execute_with_retry<F, Fut, T>(&self, operation: F) -> Result<T, CoreError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, CoreError>>,
    {
        let mut last_error = CoreError::Internal("요청 실패".to_string());
        let mut delay = Duration::from_secs(1);

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !is_retryable(&e) || attempt == self.max_retries {
                        return Err(e);
                    }

                    warn!(
                        "요청 실패 (시도 {}/{}): {e}, {delay:?} 후 재시도",
                        attempt + 1,
                        self.max_retries + 1
                    );

                    // RateLimit의 경우 서버 지정 대기 시간 사용
                    if let CoreError::RateLimit { retry_after_secs } = &e {
                        delay = Duration::from_secs(*retry_after_secs);
                    }

                    last_error = e;
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(30));
                }
            }
        }

        Err(last_error)
    }
}

#[async_trait]
impl ApiClient for HttpApiClient {
    async fn create_session(&self, client_id: &str) -> Result<SessionCreateResponse, CoreError> {
        debug!("세션 생성 요청: client_id={client_id}");

        self.execute_with_retry(|| async {
            let req = self
                .authorized_request(reqwest::Method::POST, "/user_context/sessions/")
                .await?;

            let body = serde_json::json!({ "client_id": client_id });
            let resp = req
                .json(&body)
                .send()
                .await
                .map_err(|e| CoreError::Network(format!("세션 생성 요청 실패: {e}")))?;

            let resp = self.check_response(resp).await?;
            let session: SessionCreateResponse = resp
                .json()
                .await
                .map_err(|e| CoreError::Internal(format!("세션 응답 파싱 실패: {e}")))?;

            debug!("세션 생성 성공: session_id={}", session.session_id);
            Ok(session)
        })
        .await
    }

    async fn end_session(&self, session_id: &str) -> Result<(), CoreError> {
        debug!("세션 종료 요청: session_id={session_id}");

        self.execute_with_retry(|| async {
            let path = format!("/user_context/sessions/{session_id}");
            let req = self
                .authorized_request(reqwest::Method::DELETE, &path)
                .await?;

            let resp = req
                .send()
                .await
                .map_err(|e| CoreError::Network(format!("세션 종료 요청 실패: {e}")))?;

            self.check_response(resp).await?;
            debug!("세션 종료 성공");
            Ok(())
        })
        .await
    }

    async fn upload_batch(&self, batch: &EventBatch) -> Result<(), CoreError> {
        debug!("배치 업로드: {} 이벤트", batch.events.len());

        self.execute_with_retry(|| async {
            // 업계 표준 경로: POST /user_context/batches
            let req = self
                .authorized_request(reqwest::Method::POST, "/user_context/batches")
                .await?;

            let resp = req
                .json(batch)
                .send()
                .await
                .map_err(|e| CoreError::Network(format!("배치 업로드 요청 실패: {e}")))?;

            self.check_response(resp).await?;
            debug!("배치 업로드 성공");
            Ok(())
        })
        .await
    }

    async fn upload_context(&self, upload: &ContextUpload) -> Result<(), CoreError> {
        debug!("컨텍스트 업로드: {}", upload.metadata.app_name);

        self.execute_with_retry(|| async {
            // 업계 표준 경로: POST /user_context/contexts
            let req = self
                .authorized_request(reqwest::Method::POST, "/user_context/contexts")
                .await?;

            let resp = req
                .json(upload)
                .send()
                .await
                .map_err(|e| CoreError::Network(format!("컨텍스트 업로드 실패: {e}")))?;

            self.check_response(resp).await?;
            Ok(())
        })
        .await
    }

    async fn send_feedback(&self, feedback: &SuggestionFeedback) -> Result<(), CoreError> {
        debug!(
            "피드백 전송: {} → {:?}",
            feedback.suggestion_id, feedback.feedback_type
        );

        self.execute_with_retry(|| async {
            let req = self
                .authorized_request(reqwest::Method::POST, "/user_context/suggestions/feedback")
                .await?;

            let resp = req
                .json(feedback)
                .send()
                .await
                .map_err(|e| CoreError::Network(format!("피드백 전송 실패: {e}")))?;

            self.check_response(resp).await?;
            Ok(())
        })
        .await
    }

    async fn send_heartbeat(&self, session_id: &str) -> Result<(), CoreError> {
        debug!("하트비트 전송: {session_id}");

        self.execute_with_retry(|| async {
            // 업계 표준 경로: POST /user_context/sessions/{session_id}/heartbeat
            let path = format!("/user_context/sessions/{}/heartbeat", session_id);
            let req = self
                .authorized_request(reqwest::Method::POST, &path)
                .await?;

            let resp = req
                .send()
                .await
                .map_err(|e| CoreError::Network(format!("하트비트 전송 실패: {e}")))?;

            self.check_response(resp).await?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_client_creation() {
        let tm = Arc::new(TokenManager::new("http://localhost:8000"));
        let client =
            HttpApiClient::new("http://localhost:8000", tm, Duration::from_secs(30)).unwrap();
        assert_eq!(client.base_url, "http://localhost:8000");
        assert_eq!(client.max_retries, DEFAULT_MAX_RETRIES);
    }

    #[test]
    fn with_max_retries() {
        let tm = Arc::new(TokenManager::new("http://localhost:8000"));
        let client = HttpApiClient::new("http://localhost:8000", tm, Duration::from_secs(30))
            .unwrap()
            .with_max_retries(5);
        assert_eq!(client.max_retries, 5);
    }

    #[test]
    fn is_retryable_errors() {
        assert!(is_retryable(&CoreError::Network("test".to_string())));
        assert!(is_retryable(&CoreError::ServiceUnavailable(
            "test".to_string()
        )));
        assert!(is_retryable(&CoreError::RateLimit {
            retry_after_secs: 60
        }));
        assert!(!is_retryable(&CoreError::Auth("test".to_string())));
        assert!(!is_retryable(&CoreError::Internal("test".to_string())));
    }

    /// 로그인된 TokenManager를 생성하는 헬퍼
    async fn setup_authed_client(
        server: &mut mockito::ServerGuard,
    ) -> (HttpApiClient, mockito::Mock) {
        let login_mock = server
            .mock("POST", "/api/v1/auth/tokens")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"test_jwt","refresh_token":"ref","expires_in":3600}"#)
            .create_async()
            .await;

        let tm = Arc::new(TokenManager::new(&server.url()));
        tm.login("test@test.com", "pass").await.unwrap();

        let client = HttpApiClient::new(&server.url(), tm, Duration::from_secs(5)).unwrap();
        (client, login_mock)
    }

    #[tokio::test]
    async fn create_session_success() {
        let mut server = mockito::Server::new_async().await;
        let (client, _login_mock) = setup_authed_client(&mut server).await;

        let mock = server
            .mock("POST", "/user_context/sessions/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"session_id":"sess_123","user_id":"user_1","client_id":"cli_1","capabilities":["streaming"]}"#)
            .create_async()
            .await;

        let result = client.create_session("cli_1").await;
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.session_id, "sess_123");
        assert_eq!(session.user_id, "user_1");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn end_session_success() {
        let mut server = mockito::Server::new_async().await;
        let (client, _login_mock) = setup_authed_client(&mut server).await;

        let mock = server
            .mock("DELETE", "/user_context/sessions/sess_123")
            .with_status(200)
            .create_async()
            .await;

        let result = client.end_session("sess_123").await;
        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn upload_context_success() {
        let mut server = mockito::Server::new_async().await;
        let (client, _login_mock) = setup_authed_client(&mut server).await;

        // 업계 표준 경로: POST /user_context/contexts
        let mock = server
            .mock("POST", "/user_context/contexts")
            .with_status(200)
            .create_async()
            .await;

        let upload = oneshim_core::models::frame::ContextUpload {
            session_id: "sess_1".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: oneshim_core::models::frame::FrameMetadata {
                timestamp: chrono::Utc::now(),
                trigger_type: "test".to_string(),
                app_name: "Test".to_string(),
                window_title: "Test Window".to_string(),
                resolution: (1920, 1080),
                importance: 0.5,
            },
            ocr_text: None,
            image: None,
        };

        let result = client.upload_context(&upload).await;
        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn upload_context_server_error() {
        let mut server = mockito::Server::new_async().await;
        let (client, _login_mock) = setup_authed_client(&mut server).await;

        // 업계 표준 경로: POST /user_context/contexts
        let mock = server
            .mock("POST", "/user_context/contexts")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let upload = oneshim_core::models::frame::ContextUpload {
            session_id: "sess_1".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: oneshim_core::models::frame::FrameMetadata {
                timestamp: chrono::Utc::now(),
                trigger_type: "test".to_string(),
                app_name: "Test".to_string(),
                window_title: "Test Window".to_string(),
                resolution: (1920, 1080),
                importance: 0.5,
            },
            ocr_text: None,
            image: None,
        };

        let result = client.upload_context(&upload).await;
        assert!(result.is_err());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn upload_batch_401() {
        let mut server = mockito::Server::new_async().await;
        let (client, _login_mock) = setup_authed_client(&mut server).await;

        // 업계 표준 경로: POST /user_context/batches
        let mock = server
            .mock("POST", "/user_context/batches")
            .with_status(401)
            .with_body("Unauthorized")
            .create_async()
            .await;

        let batch = oneshim_core::models::event::EventBatch {
            session_id: "sess_1".to_string(),
            events: vec![],
            created_at: chrono::Utc::now(),
        };

        let result = client.upload_batch(&batch).await;
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("인증"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn rate_limit_429() {
        let mut server = mockito::Server::new_async().await;
        let (client, _login_mock) = setup_authed_client(&mut server).await;
        let client = client.with_max_retries(0); // 재시도 없이 즉시 실패

        // 업계 표준 경로: POST /user_context/contexts
        let mock = server
            .mock("POST", "/user_context/contexts")
            .with_status(429)
            .with_body("Too Many Requests")
            .create_async()
            .await;

        let upload = oneshim_core::models::frame::ContextUpload {
            session_id: "sess_1".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: oneshim_core::models::frame::FrameMetadata {
                timestamp: chrono::Utc::now(),
                trigger_type: "test".to_string(),
                app_name: "Test".to_string(),
                window_title: "Test Window".to_string(),
                resolution: (1920, 1080),
                importance: 0.5,
            },
            ocr_text: None,
            image: None,
        };

        let result = client.upload_context(&upload).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CoreError::RateLimit { .. }));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn service_unavailable_503() {
        let mut server = mockito::Server::new_async().await;
        let (client, _login_mock) = setup_authed_client(&mut server).await;
        let client = client.with_max_retries(0);

        // 업계 표준 경로: POST /user_context/sessions/{session_id}/heartbeat
        let mock = server
            .mock("POST", "/user_context/sessions/sess_1/heartbeat")
            .with_status(503)
            .with_body("Service Unavailable")
            .create_async()
            .await;

        let result = client.send_heartbeat("sess_1").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CoreError::ServiceUnavailable(_)));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn heartbeat_success() {
        let mut server = mockito::Server::new_async().await;
        let (client, _login_mock) = setup_authed_client(&mut server).await;

        // 업계 표준 경로: POST /user_context/sessions/{session_id}/heartbeat
        let mock = server
            .mock("POST", "/user_context/sessions/sess_test/heartbeat")
            .with_status(200)
            .create_async()
            .await;

        let result = client.send_heartbeat("sess_test").await;
        assert!(result.is_ok());
        mock.assert_async().await;
    }
}
