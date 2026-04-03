use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::Json;
use oneshim_api_contracts::frames::FrameResponse;

use crate::error::ApiError;
use crate::services::frames_service::FramesQueryService;
use crate::services::web_contexts::StorageWebContext;

use super::{PaginatedResponse, TimeRangeQuery};

/// GET /api/frames?from=&to=&limit=&offset=
pub async fn get_frames(
    State(context): State<StorageWebContext>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<PaginatedResponse<FrameResponse>>, ApiError> {
    Ok(Json(FramesQueryService::new(context).get_frames(&params)?))
}

/// GET /api/frames/:id/image
pub async fn get_frame_image(
    State(context): State<StorageWebContext>,
    Path(frame_id): Path<i64>,
) -> Response {
    FramesQueryService::new(context).get_frame_image(frame_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::{Request, StatusCode};
    use oneshim_core::config::CredentialBackendKind;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    fn test_app_state() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(16);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            default_secret_backend_kind: CredentialBackendKind::Unavailable,
            secret_store: None,
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            integration_runtime_status: None,
            integration_auth: None,
            integration_session: None,
            integration_outbox: None,
            integration_inbox: None,
            integration_inbox_store: None,
            integration_audit: None,
            integration_runtime_telemetry: None,
            update_control: None,
            vector_store: None,
            embedding_provider: None,
            text_search: None,
            override_store: None,
            recluster_requested: None,
            coaching_engine: None,
            session_manager: None,
            pomodoro: Arc::new(std::sync::Mutex::new(None)),
            pii_sanitizer: None,
            latest_bug_report: std::sync::Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    fn loopback_app(state: AppState) -> axum::Router {
        crate::WebServer::build_router(state)
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
    }

    #[test]
    fn frame_response_serializes() {
        let frame = FrameResponse {
            id: 1,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            trigger_type: "AppSwitch".to_string(),
            app_name: "Code".to_string(),
            window_title: "main.rs".to_string(),
            importance: 0.85,
            resolution: "1920x1080".to_string(),
            file_path: Some("frames/123.webp".to_string()),
            ocr_text: None,
            image_url: Some("/api/frames/1/image".to_string()),
            tag_ids: vec![1, 3],
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("Code"));
        assert!(json.contains("tag_ids"));
    }

    #[tokio::test]
    async fn get_frame_image_returns_not_found_for_nonexistent() {
        let app = loopback_app(test_app_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/frames/99999/image")
                    .body(Body::empty())
                    .expect("request build"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
