use axum::extract::{Query, State};
use axum::Json;
use oneshim_api_contracts::coaching::{
    CoachingEventResponse, CoachingHistoryQuery, GoalProgressResponse, UpdateGoalsRequest,
};

use crate::error::ApiError;
use crate::AppState;

/// GET /api/coaching/history
pub async fn get_coaching_history(
    State(state): State<AppState>,
    Query(params): Query<CoachingHistoryQuery>,
) -> Result<Json<Vec<CoachingEventResponse>>, ApiError> {
    let events = state
        .storage
        .query_coaching_events(params.limit.unwrap_or(50), params.offset.unwrap_or(0))
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(
        events
            .into_iter()
            .map(CoachingEventResponse::from)
            .collect(),
    ))
}

/// GET /api/coaching/goals
pub async fn get_goals(
    State(state): State<AppState>,
) -> Result<Json<Vec<GoalProgressResponse>>, ApiError> {
    if let Some(ref engine) = state.coaching_engine {
        let progress = engine.all_goal_progress_blocking();
        Ok(Json(
            progress
                .into_iter()
                .map(GoalProgressResponse::from)
                .collect(),
        ))
    } else {
        Ok(Json(vec![]))
    }
}

/// PUT /api/coaching/goals
pub async fn update_goals(
    State(state): State<AppState>,
    Json(body): Json<UpdateGoalsRequest>,
) -> Result<Json<()>, ApiError> {
    if let Some(ref engine) = state.coaching_engine {
        engine.update_regime_goals_blocking(&body.goals);
    }
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::{Request, StatusCode};
    use oneshim_core::config::CredentialBackendKind;
    use oneshim_core::models::coaching::GoalProgressView;
    use oneshim_core::ports::coaching::CoachingPort;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    struct MockCoachingEngine;

    #[async_trait]
    impl CoachingPort for MockCoachingEngine {
        fn all_goal_progress_blocking(&self) -> Vec<GoalProgressView> {
            vec![GoalProgressView {
                regime_label: "deep_work".to_string(),
                current_minutes: 90,
                target_minutes: 120,
                percentage: 75,
                display_color: "#4CAF50".to_string(),
            }]
        }

        fn update_regime_goals_blocking(&self, _goals: &HashMap<String, u32>) {
            // no-op for test
        }

        async fn snooze_profile(&self, _profile: &str, _duration_secs: u64) {}
        async fn record_feedback(&self, _message_id: &str, _positive: bool) {}
        async fn all_goal_progress(&self) -> Vec<GoalProgressView> {
            self.all_goal_progress_blocking()
        }
        async fn update_regime_goals(&self, _goals: &HashMap<String, u32>) {}
    }

    fn test_app_state() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
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
            latest_bug_report: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn test_app_state_with_coaching() -> AppState {
        let mut state = test_app_state();
        state.coaching_engine = Some(Arc::new(MockCoachingEngine));
        state
    }

    fn loopback_app(state: AppState) -> axum::Router {
        crate::WebServer::build_router(state)
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
    }

    #[tokio::test]
    async fn get_coaching_history_returns_events() {
        let app = loopback_app(test_app_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/coaching/history")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Empty database returns an empty JSON array
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_goals_returns_empty_without_engine() {
        let app = loopback_app(test_app_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/coaching/goals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn get_goals_returns_progress_with_engine() {
        let app = loopback_app(test_app_state_with_coaching());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/coaching/goals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let goals = parsed.as_array().unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0]["regime_label"], "deep_work");
        assert_eq!(goals[0]["current_minutes"], 90);
        assert_eq!(goals[0]["target_minutes"], 120);
        assert_eq!(goals[0]["percentage"], 75);
    }

    #[tokio::test]
    async fn update_goals_succeeds() {
        let app = loopback_app(test_app_state_with_coaching());
        let body = r#"{"goals":{"deep_work":180}}"#.to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/coaching/goals")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn update_goals_succeeds_without_engine() {
        let app = loopback_app(test_app_state());
        let body = r#"{"goals":{"deep_work":180}}"#.to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/coaching/goals")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn coaching_event_response_serializes() {
        let resp = CoachingEventResponse {
            event_id: "evt-001".to_string(),
            trigger_type: "regime_change".to_string(),
            profile_name: "focus_boost".to_string(),
            regime_id: Some("regime-abc".to_string()),
            message_template: "Time to focus!".to_string(),
            personalized_message: Some("You've been in meetings for 2h.".to_string()),
            shown_at: "2026-03-21T10:00:00Z".to_string(),
            dismissed_at: None,
            dismiss_action: None,
            feedback_type: None,
            feedback_score: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("focus_boost"));
        assert!(json.contains("regime_change"));
    }

    #[test]
    fn goal_progress_response_serializes() {
        let resp = GoalProgressResponse {
            regime_label: "deep_work".to_string(),
            current_minutes: 90,
            target_minutes: 120,
            percentage: 75,
            display_color: "#4CAF50".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("deep_work"));
        assert!(json.contains("\"percentage\":75"));
    }
}
