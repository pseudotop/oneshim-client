use axum::extract::{Path, State};
use axum::Json;
use oneshim_api_contracts::sessions::SessionResponse;

use crate::error::ApiError;
use crate::services::sessions_service::SessionsQueryService;
use crate::services::web_contexts::StorageWebContext;

/// GET /api/sessions
pub async fn list_sessions(
    State(context): State<StorageWebContext>,
) -> Result<Json<Vec<SessionResponse>>, ApiError> {
    Ok(Json(SessionsQueryService::new(context).list_sessions()?))
}

/// GET /api/sessions/:id
pub async fn get_session(
    State(context): State<StorageWebContext>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, ApiError> {
    Ok(Json(
        SessionsQueryService::new(context)
            .get_session(&session_id)
            .await?,
    ))
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
