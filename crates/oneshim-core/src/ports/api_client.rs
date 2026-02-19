//! API 클라이언트 포트.
//!
//! 구현: `oneshim-network` crate (reqwest, eventsource-client)

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::event::EventBatch;
use crate::models::frame::ContextUpload;
use crate::models::suggestion::{Suggestion, SuggestionFeedback};

/// 세션 생성 응답
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionCreateResponse {
    /// 서버에서 할당한 세션 ID
    pub session_id: String,
    /// 사용자 ID
    pub user_id: String,
    /// 클라이언트 ID
    pub client_id: String,
    /// 지원 기능 목록
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// HTTP API 클라이언트
#[async_trait]
pub trait ApiClient: Send + Sync {
    /// 서버 세션 생성
    ///
    /// 클라이언트 시작 시 호출하여 서버에 세션을 등록한다.
    async fn create_session(&self, client_id: &str) -> Result<SessionCreateResponse, CoreError>;

    /// 서버 세션 종료
    ///
    /// 클라이언트 종료 시 호출하여 서버에 세션 종료를 알린다.
    async fn end_session(&self, session_id: &str) -> Result<(), CoreError>;

    /// 이벤트 배치 업로드
    async fn upload_batch(&self, batch: &EventBatch) -> Result<(), CoreError>;

    /// 컨텍스트(메타+이미지) 업로드
    async fn upload_context(&self, upload: &ContextUpload) -> Result<(), CoreError>;

    /// 제안 피드백 전송
    async fn send_feedback(&self, feedback: &SuggestionFeedback) -> Result<(), CoreError>;

    /// 하트비트 전송
    async fn send_heartbeat(&self, session_id: &str) -> Result<(), CoreError>;
}

/// SSE(Server-Sent Events) 이벤트
#[derive(Debug, Clone)]
pub enum SseEvent {
    /// 연결 수립
    Connected { session_id: String },
    /// 제안 수신
    Suggestion(Suggestion),
    /// 일반 업데이트 (JSON)
    Update(serde_json::Value),
    /// 하트비트
    Heartbeat {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    /// 서버 에러
    Error(String),
    /// 연결 종료
    Close,
}

/// SSE 스트림 클라이언트
#[async_trait]
pub trait SseClient: Send + Sync {
    /// SSE 스트림 연결 및 이벤트 수신
    ///
    /// 수신된 이벤트를 `tx` 채널로 전송한다.
    /// 연결이 끊기면 자동 재연결을 시도한다.
    async fn connect(
        &self,
        session_id: &str,
        tx: tokio::sync::mpsc::Sender<SseEvent>,
    ) -> Result<(), CoreError>;
}
