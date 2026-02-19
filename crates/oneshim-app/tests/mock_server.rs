//! Mock 서버 모듈
//!
//! 클라이언트 통합 테스트를 위한 경량 mock 서버.
//! Axum 기반으로 실제 서버 API를 모의합니다.

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{atomic::AtomicU64, Arc};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Mock 서버 상태
#[derive(Debug, Default)]
pub struct MockServerState {
    /// 수신된 요청 수
    pub request_count: AtomicU64,
    /// 저장된 컨텍스트 (세션별)
    pub contexts: RwLock<HashMap<String, Vec<ContextUploadRecord>>>,
    /// 활성 세션
    pub sessions: RwLock<HashMap<String, SessionInfo>>,
    /// 발급된 토큰
    pub tokens: RwLock<HashMap<String, TokenInfo>>,
}

/// 컨텍스트 업로드 기록
#[derive(Debug, Clone, Serialize)]
pub struct ContextUploadRecord {
    pub timestamp: String,
    pub app_name: String,
    pub window_title: String,
}

/// 세션 정보
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub user_id: String,
    pub client_id: String,
    pub created_at: String,
}

/// 토큰 정보
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub access_token: String,
    pub expires_at: i64,
}

/// 로그인 요청 (identifier 필드 지원, email은 하위 호환용)
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// 새 형식: identifier (email, username, phone)
    #[serde(alias = "email")]
    pub identifier: String,
    pub password: String,
    /// 조직 ID (선택)
    #[serde(default)]
    pub organization_id: Option<String>,
}

/// 로그인 응답
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// 세션 생성 요청
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SessionCreateRequest {
    pub client_id: String,
    pub metadata: Option<HashMap<String, String>>,
}

/// 세션 생성 응답
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionCreateResponse {
    pub session_id: String,
    pub user_id: String,
    pub client_id: String,
    pub permissions: Vec<String>,
}

/// 컨텍스트 업로드 요청 (실제 클라이언트 형식)
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ContextUploadRequest {
    pub session_id: String,
    pub timestamp: String,
    pub metadata: ContextMetadata,
    pub ocr_text: Option<String>,
    pub image: Option<serde_json::Value>,
}

/// 컨텍스트 메타데이터
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ContextMetadata {
    pub timestamp: String,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub resolution: (u32, u32),
    pub importance: f32,
}

/// 배치 동기화 요청
#[derive(Debug, Deserialize)]
pub struct BatchSyncRequest {
    pub events: Vec<serde_json::Value>,
}

/// 배치 동기화 응답
#[derive(Debug, Serialize, Deserialize)]
pub struct BatchSyncResponse {
    pub synced_count: usize,
    pub failed_count: usize,
}

/// 헬스체크 응답
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub session_active: bool,
}

/// Mock 서버 핸들
pub struct MockServer {
    pub addr: String,
    pub port: u16,
    pub state: Arc<MockServerState>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl MockServer {
    /// 새 mock 서버 시작
    pub async fn start() -> Self {
        Self::start_on_port(0).await // 0 = 자동 포트 할당
    }

    /// 지정 포트에서 mock 서버 시작
    pub async fn start_on_port(port: u16) -> Self {
        let state = Arc::new(MockServerState::default());
        let app = create_router(state.clone());

        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .expect("포트 바인딩 실패");

        let local_addr = listener.local_addr().unwrap();
        let actual_port = local_addr.port();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // 서버 태스크 시작
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("서버 실행 실패");
        });

        // 서버 시작 대기
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        Self {
            addr: format!("http://127.0.0.1:{}", actual_port),
            port: actual_port,
            state,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// 서버 주소 반환
    pub fn url(&self) -> &str {
        &self.addr
    }

    /// 요청 수 조회
    pub fn request_count(&self) -> u64 {
        self.state
            .request_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 저장된 컨텍스트 수 조회
    pub fn context_count(&self) -> usize {
        self.state.contexts.read().values().map(|v| v.len()).sum()
    }

    /// 활성 세션 수 조회
    pub fn session_count(&self) -> usize {
        self.state.sessions.read().len()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// 라우터 생성
fn create_router(state: Arc<MockServerState>) -> Router {
    Router::new()
        // ==================== REST 표준 경로 ====================
        // 인증
        .route("/api/v1/auth/tokens", post(handle_login)) // 로그인
        .route("/api/v1/auth/tokens/refresh", post(handle_refresh)) // 토큰 갱신
        // 세션
        .route("/user_context/sessions/", post(handle_create_session))
        .route(
            "/user_context/sessions/:session_id/heartbeat",
            post(handle_health),
        )
        // 컨텍스트
        .route("/user_context/contexts", post(handle_context_upload))
        .route("/user_context/batches", post(handle_batch_sync))
        // 피드백
        .route("/user_context/suggestions/feedback", post(handle_feedback))
        // SSE 스트림 (제안)
        .route("/user_context/suggestions/stream", get(handle_sse_stream))
        .with_state(state)
}

/// 로그인 핸들러
async fn handle_login(
    State(state): State<Arc<MockServerState>>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // 간단한 검증
    if req.identifier.is_empty() || req.password.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Invalid credentials"})),
        )
            .into_response();
    }

    let access_token = format!("mock_access_{}", uuid::Uuid::new_v4());
    let refresh_token = format!("mock_refresh_{}", uuid::Uuid::new_v4());

    // 토큰 저장
    state.tokens.write().insert(
        access_token.clone(),
        TokenInfo {
            access_token: access_token.clone(),
            expires_at: chrono::Utc::now().timestamp() + 3600,
        },
    );

    Json(LoginResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 3600,
    })
    .into_response()
}

/// 토큰 갱신 핸들러
async fn handle_refresh(State(state): State<Arc<MockServerState>>) -> impl IntoResponse {
    state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let access_token = format!("mock_access_{}", uuid::Uuid::new_v4());

    Json(LoginResponse {
        access_token,
        refresh_token: format!("mock_refresh_{}", uuid::Uuid::new_v4()),
        token_type: "Bearer".to_string(),
        expires_in: 3600,
    })
}

/// 세션 생성 핸들러
async fn handle_create_session(
    State(state): State<Arc<MockServerState>>,
    Json(req): Json<SessionCreateRequest>,
) -> impl IntoResponse {
    state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let session_id = format!("session_{}", uuid::Uuid::new_v4());
    let user_id = format!("user_{}", uuid::Uuid::new_v4());

    let session = SessionInfo {
        session_id: session_id.clone(),
        user_id: user_id.clone(),
        client_id: req.client_id.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state.sessions.write().insert(session_id.clone(), session);

    Json(SessionCreateResponse {
        session_id,
        user_id,
        client_id: req.client_id,
        permissions: vec!["read".to_string(), "write".to_string()],
    })
}

/// 헬스체크 핸들러
async fn handle_health(State(state): State<Arc<MockServerState>>) -> impl IntoResponse {
    state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    Json(HealthResponse {
        status: "ok".to_string(),
        session_active: true,
    })
}

/// 컨텍스트 업로드 핸들러
async fn handle_context_upload(
    State(state): State<Arc<MockServerState>>,
    Json(req): Json<ContextUploadRequest>,
) -> impl IntoResponse {
    state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let record = ContextUploadRecord {
        timestamp: req.timestamp,
        app_name: req.metadata.app_name,
        window_title: req.metadata.window_title,
    };

    // 세션 ID로 저장
    state
        .contexts
        .write()
        .entry(req.session_id)
        .or_default()
        .push(record);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"status": "ok"})),
    )
}

/// 배치 동기화 핸들러
async fn handle_batch_sync(
    State(state): State<Arc<MockServerState>>,
    Json(req): Json<BatchSyncRequest>,
) -> impl IntoResponse {
    state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let count = req.events.len();

    Json(BatchSyncResponse {
        synced_count: count,
        failed_count: 0,
    })
}

/// 피드백 핸들러
async fn handle_feedback(State(state): State<Arc<MockServerState>>) -> impl IntoResponse {
    state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    Json(serde_json::json!({"status": "ok"}))
}

/// SSE 스트림 핸들러
async fn handle_sse_stream(State(state): State<Arc<MockServerState>>) -> impl IntoResponse {
    state
        .request_count
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // SSE 응답 (간단한 ping만 전송)
    let body = "event: ping\ndata: {\"type\":\"ping\"}\n\n";

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_starts() {
        let server = MockServer::start().await;
        assert!(!server.url().is_empty());
        assert!(server.port > 0);
    }

    #[tokio::test]
    async fn test_login_endpoint() {
        let server = MockServer::start().await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/api/v1/auth/tokens", server.url()))
            .json(&serde_json::json!({
                "email": "test@example.com",
                "password": "test-password-placeholder"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);

        let body: LoginResponse = resp.json().await.unwrap();
        assert!(body.access_token.starts_with("mock_access_"));
        assert_eq!(body.token_type, "Bearer");
    }

    #[tokio::test]
    async fn test_session_creation() {
        let server = MockServer::start().await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/user_context/sessions/", server.url()))
            .json(&serde_json::json!({
                "client_id": "test_client_123"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);

        let body: SessionCreateResponse = resp.json().await.unwrap();
        assert!(body.session_id.starts_with("session_"));
        assert_eq!(body.client_id, "test_client_123");
        assert_eq!(server.session_count(), 1);
    }

    #[tokio::test]
    async fn test_context_upload() {
        let server = MockServer::start().await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/user_context/contexts", server.url()))
            .json(&serde_json::json!({
                "session_id": "test_session_123",
                "timestamp": "2024-01-15T10:30:00Z",
                "metadata": {
                    "timestamp": "2024-01-15T10:30:00Z",
                    "trigger_type": "AppSwitch",
                    "app_name": "VSCode",
                    "window_title": "main.rs - oneshim",
                    "resolution": [1920, 1080],
                    "importance": 0.8
                }
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 201);
        assert_eq!(server.context_count(), 1);
    }

    #[tokio::test]
    async fn test_batch_sync() {
        let server = MockServer::start().await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/user_context/batches", server.url()))
            .json(&serde_json::json!({
                "events": [
                    {"type": "window_change", "app": "Chrome"},
                    {"type": "window_change", "app": "Slack"}
                ]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);

        let body: BatchSyncResponse = resp.json().await.unwrap();
        assert_eq!(body.synced_count, 2);
        assert_eq!(body.failed_count, 0);
    }

    #[tokio::test]
    async fn test_health_check() {
        let server = MockServer::start().await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!(
                "{}/user_context/sessions/test_session_1/heartbeat",
                server.url()
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);

        let body: HealthResponse = resp.json().await.unwrap();
        assert_eq!(body.status, "ok");
        assert!(body.session_active);
    }

    #[tokio::test]
    async fn test_request_counting() {
        let server = MockServer::start().await;
        assert_eq!(server.request_count(), 0);

        let client = reqwest::Client::new();

        // 3개 요청 (heartbeat 엔드포인트)
        let _ = client
            .post(format!(
                "{}/user_context/sessions/s1/heartbeat",
                server.url()
            ))
            .send()
            .await;
        let _ = client
            .post(format!(
                "{}/user_context/sessions/s2/heartbeat",
                server.url()
            ))
            .send()
            .await;
        let _ = client
            .post(format!(
                "{}/user_context/sessions/s3/heartbeat",
                server.url()
            ))
            .send()
            .await;

        assert_eq!(server.request_count(), 3);
    }
}
