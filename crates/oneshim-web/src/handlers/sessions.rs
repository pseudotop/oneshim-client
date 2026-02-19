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
    // session_stats 테이블에서 모든 세션 조회
    let conn = state.storage.conn_ref();
    let conn = conn
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let mut stmt = conn
        .prepare(
            "SELECT session_id, started_at, ended_at, total_events, total_frames, total_idle_secs
             FROM session_stats
             ORDER BY started_at DESC
             LIMIT 50",
        )
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let sessions: Vec<SessionResponse> = stmt
        .query_map([], |row| {
            let session_id: String = row.get(0)?;
            let started_at: String = row.get(1)?;
            let ended_at: Option<String> = row.get(2)?;
            let total_events: i64 = row.get(3)?;
            let total_frames: i64 = row.get(4)?;
            let total_idle_secs: i64 = row.get(5)?;

            // 활동 시간 계산
            let active_duration_secs = if let Some(ref end) = ended_at {
                use chrono::DateTime;
                let start = DateTime::parse_from_rfc3339(&started_at).ok();
                let end = DateTime::parse_from_rfc3339(end).ok();
                match (start, end) {
                    (Some(s), Some(e)) => {
                        let total_secs = (e - s).num_seconds() as u64;
                        Some(total_secs.saturating_sub(total_idle_secs as u64))
                    }
                    _ => None,
                }
            } else {
                None
            };

            Ok(SessionResponse {
                session_id,
                started_at,
                ended_at,
                total_events: total_events as u64,
                total_frames: total_frames as u64,
                total_idle_secs: total_idle_secs as u64,
                active_duration_secs,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
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
