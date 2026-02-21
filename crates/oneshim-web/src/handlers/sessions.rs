//! 세션 API 핸들러.

use axum::extract::{Path, State};
use axum::Json;
use oneshim_core::ports::storage::MetricsStorage;
use serde::Serialize;

use crate::error::ApiError;
use crate::AppState;

/// 세션 응답 DTO
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    /// 세션 ID
    pub session_id: String,
    /// 시작 시각 (RFC3339)
    pub started_at: String,
    /// 종료 시각 (RFC3339, null이면 진행 중)
    pub ended_at: Option<String>,
    /// 총 이벤트 수
    pub total_events: u64,
    /// 총 프레임 수
    pub total_frames: u64,
    /// 총 유휴 시간 (초)
    pub total_idle_secs: u64,
    /// 활동 시간 (초, 시작~종료 - 유휴)
    pub active_duration_secs: Option<u64>,
}

/// 세션 목록 조회
///
/// GET /api/sessions
pub async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionResponse>>, ApiError> {
    let sessions: Vec<SessionResponse> = state
        .storage
        .list_session_stats(50)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|session| {
            let active_duration_secs = session.ended_at.map(|end| {
                let total_secs = (end - session.started_at).num_seconds() as u64;
                total_secs.saturating_sub(session.total_idle_secs)
            });

            SessionResponse {
                session_id: session.session_id,
                started_at: session.started_at.to_rfc3339(),
                ended_at: session.ended_at.map(|dt| dt.to_rfc3339()),
                total_events: session.total_events,
                total_frames: session.total_frames,
                total_idle_secs: session.total_idle_secs,
                active_duration_secs,
            }
        })
        .collect();

    Ok(Json(sessions))
}

/// 세션 상세 조회
///
/// GET /api/sessions/:id
pub async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, ApiError> {
    let session = state
        .storage
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("세션 '{session_id}'")))?;

    // 활동 시간 계산
    let active_duration_secs = session.ended_at.map(|end| {
        let total_secs = (end - session.started_at).num_seconds() as u64;
        total_secs.saturating_sub(session.total_idle_secs)
    });

    Ok(Json(SessionResponse {
        session_id: session.session_id,
        started_at: session.started_at.to_rfc3339(),
        ended_at: session.ended_at.map(|dt| dt.to_rfc3339()),
        total_events: session.total_events,
        total_frames: session.total_frames,
        total_idle_secs: session.total_idle_secs,
        active_duration_secs,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_response_serializes() {
        let session = SessionResponse {
            session_id: "test_123".to_string(),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            ended_at: None,
            total_events: 100,
            total_frames: 50,
            total_idle_secs: 300,
            active_duration_secs: None,
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("test_123"));
    }
}
