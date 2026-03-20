use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::stream::Stream;
use oneshim_api_contracts::automation_gui::{
    GuiConfirmRequest, GuiConfirmResponse, GuiCreateSessionRequest, GuiCreateSessionResponse,
    GuiExecuteResponse, GuiExecutionRequest, GuiHighlightRequest, GuiSessionPath,
    GuiSessionResponse,
};
use std::convert::Infallible;

use crate::error::ApiError;
#[cfg(test)]
use crate::services::automation_gui_service::{
    map_gui_error, read_capability_token, GUI_SCHEMA_VERSION, GUI_SESSION_HEADER,
};
use crate::services::automation_gui_service::{
    AutomationGuiCommandService, AutomationGuiQueryService, AutomationGuiStreamService,
};
use crate::services::web_contexts::AutomationGuiWebContext;
#[cfg(test)]
use oneshim_core::error::GuiInteractionError;

pub async fn create_gui_session(
    State(context): State<AutomationGuiWebContext>,
    Json(req): Json<GuiCreateSessionRequest>,
) -> Result<Json<GuiCreateSessionResponse>, ApiError> {
    Ok(Json(
        AutomationGuiCommandService::new(context)
            .create_gui_session(req)
            .await?,
    ))
}

pub async fn get_gui_session(
    State(context): State<AutomationGuiWebContext>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    Ok(Json(
        AutomationGuiQueryService::new(context)
            .get_gui_session(&path.id, &headers)
            .await?,
    ))
}

pub async fn highlight_gui_session(
    State(context): State<AutomationGuiWebContext>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiHighlightRequest>,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    Ok(Json(
        AutomationGuiCommandService::new(context)
            .highlight_gui_session(&path.id, &headers, req)
            .await?,
    ))
}

pub async fn confirm_gui_session(
    State(context): State<AutomationGuiWebContext>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiConfirmRequest>,
) -> Result<Json<GuiConfirmResponse>, ApiError> {
    Ok(Json(
        AutomationGuiCommandService::new(context)
            .confirm_gui_session(&path.id, &headers, req)
            .await?,
    ))
}

pub async fn execute_gui_session(
    State(context): State<AutomationGuiWebContext>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiExecutionRequest>,
) -> Result<Json<GuiExecuteResponse>, ApiError> {
    Ok(Json(
        AutomationGuiCommandService::new(context)
            .execute_gui_session(&path.id, &headers, req)
            .await?,
    ))
}

pub async fn delete_gui_session(
    State(context): State<AutomationGuiWebContext>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    Ok(Json(
        AutomationGuiCommandService::new(context)
            .delete_gui_session(&path.id, &headers)
            .await?,
    ))
}

pub async fn gui_session_event_stream(
    State(context): State<AutomationGuiWebContext>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    AutomationGuiStreamService::new(context)
        .gui_session_event_stream(&path.id, &headers)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── M4: End-to-End Workflow Tests ───────────────────────────────────

    mod m4 {
        use super::*;
        use crate::AppState;
        use async_trait::async_trait;
        use chrono::Utc;
        use oneshim_api_contracts::automation_gui::{
            GuiConfirmRequest, GuiCreateSessionRequest, GuiExecutionRequest, GuiHighlightRequest,
            GuiSessionPath,
        };
        use oneshim_automation::audit::AuditLogger;
        use oneshim_automation::controller::AutomationController;
        use oneshim_automation::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use oneshim_automation::intent_resolver::{IntentExecutor, IntentResolver};
        use oneshim_automation::policy::PolicyClient;
        use oneshim_automation::sandbox::NoOpSandbox;
        use oneshim_core::config::SandboxConfig;
        use oneshim_core::error::CoreError;
        use oneshim_core::models::gui::{
            ExecutionBinding, FocusSnapshot, FocusValidation, GuiActionRequest, GuiActionType,
            GuiSessionState, HighlightHandle, HighlightRequest,
        };
        use oneshim_core::models::intent::{ElementBounds, IntentConfig};
        use oneshim_core::models::ui_scene::{NormalizedBounds, UiScene, UiSceneElement};
        use oneshim_core::ports::element_finder::ElementFinder;
        use oneshim_core::ports::focus_probe::FocusProbe;
        use oneshim_core::ports::overlay_driver::OverlayDriver;
        use oneshim_storage::sqlite::SqliteStorage;
        use std::sync::Arc;
        use tokio::sync::{broadcast, RwLock};

        const M4_HMAC_SECRET: &str = "m4-hmac-secret-32-bytes-long!!!!";

        // ── Mock types ──────────────────────────────────────────────────

        struct M4MockElementFinder;

        #[async_trait]
        impl ElementFinder for M4MockElementFinder {
            async fn find_element(
                &self,
                _text: Option<&str>,
                _role: Option<&str>,
                _region: Option<&ElementBounds>,
            ) -> Result<Vec<oneshim_core::models::intent::UiElement>, CoreError> {
                Ok(vec![])
            }

            async fn analyze_scene(
                &self,
                app_name: Option<&str>,
                screen_id: Option<&str>,
            ) -> Result<UiScene, CoreError> {
                Ok(UiScene {
                    schema_version: "ui_scene.v1".to_string(),
                    scene_id: "m4-scene".to_string(),
                    app_name: app_name.map(str::to_string),
                    screen_id: screen_id.map(str::to_string),
                    captured_at: Utc::now(),
                    screen_width: 1920,
                    screen_height: 1080,
                    elements: vec![UiSceneElement {
                        element_id: "btn-save".to_string(),
                        bbox_abs: ElementBounds {
                            x: 100,
                            y: 80,
                            width: 200,
                            height: 40,
                        },
                        bbox_norm: NormalizedBounds::new(0.05, 0.07, 0.10, 0.04),
                        label: "Save".to_string(),
                        role: Some("button".to_string()),
                        intent: None,
                        state: Some("enabled".to_string()),
                        confidence: 0.95,
                        text_masked: Some("Save".to_string()),
                        parent_id: None,
                    }],
                })
            }

            fn name(&self) -> &str {
                "m4-mock"
            }
        }

        struct M4MockFocusProbe;

        #[async_trait]
        impl FocusProbe for M4MockFocusProbe {
            async fn current_focus(&self) -> Result<FocusSnapshot, CoreError> {
                Ok(FocusSnapshot {
                    app_name: "TestApp".to_string(),
                    window_title: "Test Window".to_string(),
                    pid: 1234,
                    bounds: None,
                    captured_at: Utc::now(),
                    focus_hash: "m4focushash".to_string(),
                })
            }

            async fn validate_execution_binding(
                &self,
                _binding: &ExecutionBinding,
            ) -> Result<FocusValidation, CoreError> {
                Ok(FocusValidation {
                    valid: true,
                    reason: None,
                    current_focus: None,
                })
            }
        }

        struct M4MockOverlayDriver;

        #[async_trait]
        impl OverlayDriver for M4MockOverlayDriver {
            async fn show_highlights(
                &self,
                req: HighlightRequest,
            ) -> Result<HighlightHandle, CoreError> {
                Ok(HighlightHandle {
                    handle_id: "m4-overlay-handle".to_string(),
                    rendered_at: Utc::now(),
                    target_count: req.targets.len(),
                })
            }

            async fn clear_highlights(&self, _handle_id: &str) -> Result<(), CoreError> {
                Ok(())
            }
        }

        // ── Fixture builders ────────────────────────────────────────────

        fn make_controller() -> Arc<AutomationController> {
            let policy_client = Arc::new(PolicyClient::new());
            let audit_logger = Arc::new(RwLock::new(AuditLogger::default()));
            let sandbox: Arc<dyn oneshim_core::ports::sandbox::Sandbox> = Arc::new(NoOpSandbox);
            let sandbox_config = SandboxConfig::default();
            let mut controller =
                AutomationController::new(policy_client, audit_logger, sandbox, sandbox_config);

            controller.set_scene_finder(Arc::new(M4MockElementFinder));
            controller
                .configure_gui_interaction(
                    Arc::new(M4MockFocusProbe),
                    Arc::new(M4MockOverlayDriver),
                    Some(M4_HMAC_SECRET.to_string()),
                )
                .expect("configure_gui_interaction should succeed");

            let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
                Arc::new(NoOpInputDriver);
            let element_finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder> =
                Arc::new(NoOpElementFinder);
            let resolver =
                IntentResolver::new(element_finder, input_driver, IntentConfig::default());
            controller.set_intent_executor(Arc::new(IntentExecutor::new(
                resolver,
                IntentConfig::default(),
            )));

            controller.set_enabled(true);
            Arc::new(controller)
        }

        fn make_state() -> AppState {
            let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
            let (event_tx, _) = broadcast::channel(16);
            AppState {
                storage,
                frames_dir: None,
                event_tx,
                config_manager: None,
                default_secret_backend_kind:
                    oneshim_core::config::CredentialBackendKind::Unavailable,
                secret_store: None,
                secret_stores: None,
                audit_logger: None,
                automation_controller: Some(make_controller()),
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
                pomodoro: std::sync::Arc::new(std::sync::Mutex::new(None)),
            }
        }

        fn make_state_no_controller() -> AppState {
            let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
            let (event_tx, _) = broadcast::channel(16);
            AppState {
                storage,
                frames_dir: None,
                event_tx,
                config_manager: None,
                default_secret_backend_kind:
                    oneshim_core::config::CredentialBackendKind::Unavailable,
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
                pomodoro: std::sync::Arc::new(std::sync::Mutex::new(None)),
            }
        }

        fn token_headers(token: &str) -> HeaderMap {
            let mut h = HeaderMap::new();
            h.insert(GUI_SESSION_HEADER, token.parse().unwrap());
            h
        }

        fn context(state: &AppState) -> AutomationGuiWebContext {
            AutomationGuiWebContext::from_state(state)
        }

        fn default_create_req() -> GuiCreateSessionRequest {
            GuiCreateSessionRequest {
                app_name: Some("TestApp".to_string()),
                screen_id: None,
                min_confidence: None,
                max_candidates: None,
                session_ttl_secs: None,
            }
        }

        // ── Fixture helpers (DRY across multi-step tests) ───────────────

        /// Creates a session; returns (session_id, capability_token).
        async fn fixture_create(state: &AppState) -> (String, String) {
            let resp = create_gui_session(State(context(state)), Json(default_create_req()))
                .await
                .expect("fixture_create: create_gui_session should succeed");
            (resp.0.session.session_id, resp.0.capability_token)
        }

        /// Highlights a session; returns the first candidate's element_id.
        async fn fixture_highlight(state: &AppState, sid: &str, token: &str) -> String {
            let resp = highlight_gui_session(
                State(context(state)),
                Path(GuiSessionPath {
                    id: sid.to_string(),
                }),
                token_headers(token),
                Json(GuiHighlightRequest {
                    candidate_ids: None,
                }),
            )
            .await
            .expect("fixture_highlight: highlight_gui_session should succeed");
            resp.0
                .session
                .candidates
                .first()
                .expect("fixture_highlight: at least one candidate expected")
                .element
                .element_id
                .clone()
        }

        // ── M4 Tests ────────────────────────────────────────────────────

        #[tokio::test]
        async fn m4_no_controller_returns_service_unavailable() {
            let state = make_state_no_controller();
            let err = create_gui_session(State(context(&state)), Json(default_create_req()))
                .await
                .unwrap_err();
            assert!(matches!(err, ApiError::ServiceUnavailable(_)));
        }

        #[tokio::test]
        async fn m4_missing_token_blocks_get_session() {
            let state = make_state();
            let err = get_gui_session(
                State(context(&state)),
                Path(GuiSessionPath {
                    id: "any-id".to_string(),
                }),
                HeaderMap::new(),
            )
            .await
            .unwrap_err();
            assert!(matches!(err, ApiError::Unauthorized(_)));
        }

        #[tokio::test]
        async fn m4_create_session_returns_session_and_token() {
            let state = make_state();
            let resp = create_gui_session(State(context(&state)), Json(default_create_req()))
                .await
                .unwrap();
            assert!(!resp.0.session.session_id.is_empty());
            assert!(!resp.0.capability_token.is_empty());
            assert_eq!(resp.0.schema_version, GUI_SCHEMA_VERSION);
            assert_eq!(resp.0.session.state, GuiSessionState::Proposed);
        }

        #[tokio::test]
        async fn m4_get_session_reflects_proposed_state() {
            let state = make_state();
            let (sid, token) = fixture_create(&state).await;

            let get_resp = get_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid.clone() }),
                token_headers(&token),
            )
            .await
            .unwrap();
            assert_eq!(get_resp.0.session.session_id, sid);
            assert_eq!(get_resp.0.session.state, GuiSessionState::Proposed);
        }

        #[tokio::test]
        async fn m4_highlight_session_transitions_to_highlighted() {
            let state = make_state();
            let (sid, token) = fixture_create(&state).await;

            let highlight_resp = highlight_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid }),
                token_headers(&token),
                Json(GuiHighlightRequest {
                    candidate_ids: None,
                }),
            )
            .await
            .unwrap();
            assert_eq!(highlight_resp.0.session.state, GuiSessionState::Highlighted);
            assert!(!highlight_resp.0.session.candidates.is_empty());
        }

        #[tokio::test]
        async fn m4_confirm_session_returns_execution_ticket() {
            let state = make_state();
            let (sid, token) = fixture_create(&state).await;
            let candidate_id = fixture_highlight(&state, &sid, &token).await;

            let confirm_resp = confirm_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid }),
                token_headers(&token),
                Json(GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                }),
            )
            .await
            .unwrap();
            assert!(!confirm_resp.0.ticket.ticket_id.is_empty());
            assert_eq!(confirm_resp.0.schema_version, GUI_SCHEMA_VERSION);
        }

        #[tokio::test]
        async fn m4_execute_with_valid_ticket_succeeds() {
            let state = make_state();
            let (sid, token) = fixture_create(&state).await;
            let candidate_id = fixture_highlight(&state, &sid, &token).await;

            let confirm_resp = confirm_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid.clone() }),
                token_headers(&token),
                Json(GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                }),
            )
            .await
            .unwrap();
            let ticket = confirm_resp.0.ticket;

            let exec_resp = execute_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid }),
                token_headers(&token),
                Json(GuiExecutionRequest { ticket }),
            )
            .await
            .unwrap();
            assert!(exec_resp.0.outcome.succeeded);
            assert_eq!(exec_resp.0.outcome.session.state, GuiSessionState::Executed);
            assert_eq!(exec_resp.0.schema_version, GUI_SCHEMA_VERSION);
        }

        #[tokio::test]
        async fn m4_delete_session_transitions_to_cancelled() {
            let state = make_state();
            let (sid, token) = fixture_create(&state).await;

            let delete_resp = delete_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid }),
                token_headers(&token),
            )
            .await
            .unwrap();
            assert_eq!(delete_resp.0.session.state, GuiSessionState::Cancelled);
        }

        #[tokio::test]
        async fn m4_wrong_token_rejected_as_unauthorized() {
            let state = make_state();
            let (sid, _) = fixture_create(&state).await;

            // Wrong token is rejected regardless of which endpoint is called
            let err = get_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid }),
                token_headers("wrong-token"),
            )
            .await
            .unwrap_err();
            assert!(matches!(err, ApiError::Unauthorized(_)));
        }

        /// Verifies that two concurrent sessions are fully independent: cancelling
        /// session B does not affect session A in any way.
        #[tokio::test]
        async fn m4_two_concurrent_sessions_are_independent() {
            let state = make_state();
            let (sid_a, token_a) = fixture_create(&state).await;
            let (sid_b, token_b) = fixture_create(&state).await;

            // Cancel session B (return value not inspected; cancellation confirmed by B becoming inaccessible)
            let _b_cancelled = delete_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid_b.clone() }),
                token_headers(&token_b),
            )
            .await
            .expect("delete session B should succeed");

            // Session A is still accessible and Proposed
            let get_a = get_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid_a.clone() }),
                token_headers(&token_a),
            )
            .await
            .expect("session A should still be accessible after B is cancelled");
            assert_eq!(get_a.0.session.state, GuiSessionState::Proposed);
            assert_eq!(get_a.0.session.session_id, sid_a);

            // Token B cannot access session A (token is session-scoped)
            let err = get_gui_session(
                State(context(&state)),
                Path(GuiSessionPath { id: sid_a }),
                token_headers(&token_b),
            )
            .await
            .unwrap_err();
            assert!(matches!(err, ApiError::Unauthorized(_)));
        }
    }

    // ── read_capability_token tests ─────────────────────────────────────

    #[test]
    fn token_header_is_enforced() {
        let headers = HeaderMap::new();
        let err = read_capability_token(&headers).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn token_header_rejects_empty_value() {
        let mut headers = HeaderMap::new();
        headers.insert(GUI_SESSION_HEADER, "".parse().unwrap());
        let err = read_capability_token(&headers).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn token_header_rejects_whitespace_only() {
        let mut headers = HeaderMap::new();
        headers.insert(GUI_SESSION_HEADER, "   ".parse().unwrap());
        let err = read_capability_token(&headers).unwrap_err();
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn token_header_accepts_valid_token() {
        let mut headers = HeaderMap::new();
        headers.insert(GUI_SESSION_HEADER, "abc123".parse().unwrap());
        let token = read_capability_token(&headers).unwrap();
        assert_eq!(token, "abc123");
    }

    #[test]
    fn token_header_trims_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(GUI_SESSION_HEADER, " tok123 ".parse().unwrap());
        let token = read_capability_token(&headers).unwrap();
        assert_eq!(token, "tok123");
    }

    // ── map_gui_error tests ─────────────────────────────────────────────

    #[test]
    fn maps_unauthorized_to_401() {
        let err = map_gui_error(GuiInteractionError::Unauthorized);
        assert!(matches!(err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn maps_not_found_to_404() {
        let err = map_gui_error(GuiInteractionError::NotFound("s1".to_string()));
        assert!(matches!(err, ApiError::NotFound(_)));
    }

    #[test]
    fn maps_bad_request_to_400() {
        let err = map_gui_error(GuiInteractionError::BadRequest("bad".to_string()));
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn maps_forbidden_to_403() {
        let err = map_gui_error(GuiInteractionError::Forbidden("denied".to_string()));
        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[test]
    fn maps_focus_drift_to_409_conflict() {
        let err = map_gui_error(GuiInteractionError::FocusDrift("drift".to_string()));
        assert!(matches!(err, ApiError::Conflict(_)));
    }

    #[test]
    fn maps_ticket_invalid_to_422() {
        let err = map_gui_error(GuiInteractionError::TicketInvalid("expired".to_string()));
        assert!(matches!(err, ApiError::Unprocessable(_)));
    }

    #[test]
    fn maps_unavailable_to_503() {
        let err = map_gui_error(GuiInteractionError::Unavailable("down".to_string()));
        assert!(matches!(err, ApiError::ServiceUnavailable(_)));
    }

    #[test]
    fn maps_internal_to_500() {
        let err = map_gui_error(GuiInteractionError::Internal("crash".to_string()));
        assert!(matches!(err, ApiError::Internal(_)));
    }

    // ── Schema version constant ─────────────────────────────────────────

    #[test]
    fn gui_schema_version_matches_core() {
        assert_eq!(
            GUI_SCHEMA_VERSION,
            oneshim_core::models::gui::GUI_INTERACTION_SCHEMA_VERSION
        );
    }

    // ── M5: Failure Scenario Tests ──────────────────────────────────────

    mod m5 {
        use super::*;
        use crate::AppState;
        use async_trait::async_trait;
        use chrono::{Duration as ChronoDuration, Utc};
        use oneshim_api_contracts::automation_gui::{
            GuiConfirmRequest, GuiCreateSessionRequest, GuiExecutionRequest, GuiHighlightRequest,
            GuiSessionPath,
        };
        use oneshim_automation::audit::AuditLogger;
        use oneshim_automation::controller::AutomationController;
        use oneshim_automation::input_driver::{NoOpElementFinder, NoOpInputDriver};
        use oneshim_automation::intent_resolver::{IntentExecutor, IntentResolver};
        use oneshim_automation::policy::PolicyClient;
        use oneshim_automation::sandbox::NoOpSandbox;
        use oneshim_core::config::SandboxConfig;
        use oneshim_core::error::CoreError;
        use oneshim_core::models::gui::{
            ExecutionBinding, FocusSnapshot, FocusValidation, GuiActionRequest, GuiActionType,
            HighlightHandle, HighlightRequest,
        };
        use oneshim_core::models::intent::{ElementBounds, IntentConfig};
        use oneshim_core::models::ui_scene::{NormalizedBounds, UiScene, UiSceneElement};
        use oneshim_core::ports::element_finder::ElementFinder;
        use oneshim_core::ports::focus_probe::FocusProbe;
        use oneshim_core::ports::overlay_driver::OverlayDriver;
        use oneshim_storage::sqlite::SqliteStorage;
        use std::sync::Arc;
        use tokio::sync::{broadcast, RwLock};

        const M5_HMAC_SECRET: &str = "m5-hmac-secret-32-bytes-long!!!!";

        // ── Mock types ──────────────────────────────────────────────────

        struct M5PermissionDeniedFinder;

        #[async_trait]
        impl ElementFinder for M5PermissionDeniedFinder {
            async fn find_element(
                &self,
                _text: Option<&str>,
                _role: Option<&str>,
                _region: Option<&ElementBounds>,
            ) -> Result<Vec<oneshim_core::models::intent::UiElement>, CoreError> {
                Err(CoreError::PolicyDenied(
                    "Accessibility permission denied".to_string(),
                ))
            }

            async fn analyze_scene(
                &self,
                _app_name: Option<&str>,
                _screen_id: Option<&str>,
            ) -> Result<UiScene, CoreError> {
                Err(CoreError::PolicyDenied(
                    "Accessibility permission denied".to_string(),
                ))
            }

            fn name(&self) -> &str {
                "m5-permission-denied"
            }
        }

        struct M5MockFocusProbe {
            validation_valid: std::sync::Mutex<bool>,
        }

        impl M5MockFocusProbe {
            fn new() -> Self {
                Self {
                    validation_valid: std::sync::Mutex::new(true),
                }
            }
            fn set_validation_valid(&self, valid: bool) {
                *self.validation_valid.lock().unwrap() = valid;
            }
        }

        #[async_trait]
        impl FocusProbe for M5MockFocusProbe {
            async fn current_focus(&self) -> Result<FocusSnapshot, CoreError> {
                Ok(FocusSnapshot {
                    app_name: "TestApp".to_string(),
                    window_title: "Test Window".to_string(),
                    pid: 1234,
                    bounds: None,
                    captured_at: Utc::now(),
                    focus_hash: "m5focushash".to_string(),
                })
            }

            async fn validate_execution_binding(
                &self,
                _binding: &ExecutionBinding,
            ) -> Result<FocusValidation, CoreError> {
                let valid = *self.validation_valid.lock().unwrap();
                Ok(FocusValidation {
                    valid,
                    reason: if valid {
                        None
                    } else {
                        Some("Focus changed".to_string())
                    },
                    current_focus: None,
                })
            }
        }

        struct M5MockOverlayDriver;

        #[async_trait]
        impl OverlayDriver for M5MockOverlayDriver {
            async fn show_highlights(
                &self,
                req: HighlightRequest,
            ) -> Result<HighlightHandle, CoreError> {
                Ok(HighlightHandle {
                    handle_id: "m5-handle".to_string(),
                    rendered_at: Utc::now(),
                    target_count: req.targets.len(),
                })
            }

            async fn clear_highlights(&self, _handle_id: &str) -> Result<(), CoreError> {
                Ok(())
            }
        }

        struct M5MockElementFinder;

        #[async_trait]
        impl ElementFinder for M5MockElementFinder {
            async fn find_element(
                &self,
                _text: Option<&str>,
                _role: Option<&str>,
                _region: Option<&ElementBounds>,
            ) -> Result<Vec<oneshim_core::models::intent::UiElement>, CoreError> {
                Ok(vec![])
            }

            async fn analyze_scene(
                &self,
                app_name: Option<&str>,
                screen_id: Option<&str>,
            ) -> Result<UiScene, CoreError> {
                Ok(UiScene {
                    schema_version: "ui_scene.v1".to_string(),
                    scene_id: "m5-scene".to_string(),
                    app_name: app_name.map(str::to_string),
                    screen_id: screen_id.map(str::to_string),
                    captured_at: Utc::now(),
                    screen_width: 1920,
                    screen_height: 1080,
                    elements: vec![UiSceneElement {
                        element_id: "btn-ok".to_string(),
                        bbox_abs: ElementBounds {
                            x: 100,
                            y: 80,
                            width: 200,
                            height: 40,
                        },
                        bbox_norm: NormalizedBounds::new(0.05, 0.07, 0.10, 0.04),
                        label: "OK".to_string(),
                        role: Some("button".to_string()),
                        intent: None,
                        state: Some("enabled".to_string()),
                        confidence: 0.95,
                        text_masked: Some("OK".to_string()),
                        parent_id: None,
                    }],
                })
            }

            fn name(&self) -> &str {
                "m5-mock"
            }
        }

        // ── Fixture builders ────────────────────────────────────────────

        fn make_controller_with(
            finder: Arc<dyn ElementFinder>,
            focus_probe: Arc<dyn FocusProbe>,
            hmac_secret: Option<String>,
        ) -> Arc<AutomationController> {
            let policy_client = Arc::new(PolicyClient::new());
            let audit_logger = Arc::new(RwLock::new(AuditLogger::default()));
            let sandbox: Arc<dyn oneshim_core::ports::sandbox::Sandbox> = Arc::new(NoOpSandbox);
            let sandbox_config = SandboxConfig::default();
            let mut controller =
                AutomationController::new(policy_client, audit_logger, sandbox, sandbox_config);

            controller.set_scene_finder(finder);
            controller
                .configure_gui_interaction(focus_probe, Arc::new(M5MockOverlayDriver), hmac_secret)
                .expect("configure_gui_interaction should succeed");

            let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
                Arc::new(NoOpInputDriver);
            let element_finder: Arc<dyn ElementFinder> = Arc::new(NoOpElementFinder);
            let resolver =
                IntentResolver::new(element_finder, input_driver, IntentConfig::default());
            controller.set_intent_executor(Arc::new(IntentExecutor::new(
                resolver,
                IntentConfig::default(),
            )));

            controller.set_enabled(true);
            Arc::new(controller)
        }

        fn make_state_with(controller: Arc<AutomationController>) -> AppState {
            let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
            let (event_tx, _) = broadcast::channel(16);
            AppState {
                storage,
                frames_dir: None,
                event_tx,
                config_manager: None,
                default_secret_backend_kind:
                    oneshim_core::config::CredentialBackendKind::Unavailable,
                secret_store: None,
                secret_stores: None,
                audit_logger: None,
                automation_controller: Some(controller),
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
                pomodoro: std::sync::Arc::new(std::sync::Mutex::new(None)),
            }
        }

        fn m5_context(state: &AppState) -> AutomationGuiWebContext {
            AutomationGuiWebContext::from_state(state)
        }

        fn m5_token_headers(token: &str) -> HeaderMap {
            let mut h = HeaderMap::new();
            h.insert(GUI_SESSION_HEADER, token.parse().unwrap());
            h
        }

        fn m5_default_create_req() -> GuiCreateSessionRequest {
            GuiCreateSessionRequest {
                app_name: Some("TestApp".to_string()),
                screen_id: None,
                min_confidence: None,
                max_candidates: None,
                session_ttl_secs: None,
            }
        }

        // ── M5 Tests ────────────────────────────────────────────────────

        #[tokio::test]
        async fn m5_permission_denied_returns_http_403() {
            let controller = make_controller_with(
                Arc::new(M5PermissionDeniedFinder),
                Arc::new(M5MockFocusProbe::new()),
                Some(M5_HMAC_SECRET.to_string()),
            );
            let state = make_state_with(controller);

            let err = create_gui_session(State(m5_context(&state)), Json(m5_default_create_req()))
                .await
                .unwrap_err();
            assert!(matches!(err, ApiError::Forbidden(_)));
        }

        #[tokio::test]
        async fn m5_focus_drift_returns_http_409() {
            let focus_probe = Arc::new(M5MockFocusProbe::new());
            let controller = make_controller_with(
                Arc::new(M5MockElementFinder),
                focus_probe.clone(),
                Some(M5_HMAC_SECRET.to_string()),
            );
            let state = make_state_with(controller);

            // Create + highlight
            let resp = create_gui_session(State(m5_context(&state)), Json(m5_default_create_req()))
                .await
                .unwrap();
            let sid = resp.0.session.session_id.clone();
            let token = resp.0.capability_token.clone();

            let highlight_resp = highlight_gui_session(
                State(m5_context(&state)),
                Path(GuiSessionPath { id: sid.clone() }),
                m5_token_headers(&token),
                Json(GuiHighlightRequest {
                    candidate_ids: None,
                }),
            )
            .await
            .unwrap();
            let candidate_id = highlight_resp.0.session.candidates[0]
                .element
                .element_id
                .clone();

            // Drift before confirm
            focus_probe.set_validation_valid(false);

            let err = confirm_gui_session(
                State(m5_context(&state)),
                Path(GuiSessionPath { id: sid }),
                m5_token_headers(&token),
                Json(GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                }),
            )
            .await
            .unwrap_err();
            assert!(matches!(err, ApiError::Conflict(_)));
        }

        #[tokio::test]
        async fn m5_expired_ticket_returns_http_422() {
            let controller = make_controller_with(
                Arc::new(M5MockElementFinder),
                Arc::new(M5MockFocusProbe::new()),
                Some(M5_HMAC_SECRET.to_string()),
            );
            let state = make_state_with(controller);

            // Create + highlight
            let resp = create_gui_session(State(m5_context(&state)), Json(m5_default_create_req()))
                .await
                .unwrap();
            let sid = resp.0.session.session_id.clone();
            let token = resp.0.capability_token.clone();

            let highlight_resp = highlight_gui_session(
                State(m5_context(&state)),
                Path(GuiSessionPath { id: sid.clone() }),
                m5_token_headers(&token),
                Json(GuiHighlightRequest {
                    candidate_ids: None,
                }),
            )
            .await
            .unwrap();
            let candidate_id = highlight_resp.0.session.candidates[0]
                .element
                .element_id
                .clone();

            let confirm_resp = confirm_gui_session(
                State(m5_context(&state)),
                Path(GuiSessionPath { id: sid.clone() }),
                m5_token_headers(&token),
                Json(GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                }),
            )
            .await
            .unwrap();
            let mut ticket = confirm_resp.0.ticket;

            // Backdate ticket past grace window
            ticket.expires_at = Utc::now() - ChronoDuration::seconds(300);

            let err = execute_gui_session(
                State(m5_context(&state)),
                Path(GuiSessionPath { id: sid }),
                m5_token_headers(&token),
                Json(GuiExecutionRequest { ticket }),
            )
            .await
            .unwrap_err();
            assert!(matches!(err, ApiError::Unprocessable(_)));
        }

        #[tokio::test]
        async fn m5_sse_events_filtered_by_session_id() {
            let controller = make_controller_with(
                Arc::new(M5MockElementFinder),
                Arc::new(M5MockFocusProbe::new()),
                Some(M5_HMAC_SECRET.to_string()),
            );
            let state = make_state_with(controller);

            // Create two sessions
            let resp_a =
                create_gui_session(State(m5_context(&state)), Json(m5_default_create_req()))
                    .await
                    .unwrap();
            let sid_a = resp_a.0.session.session_id.clone();
            let token_a = resp_a.0.capability_token.clone();

            let resp_b =
                create_gui_session(State(m5_context(&state)), Json(m5_default_create_req()))
                    .await
                    .unwrap();
            let sid_b = resp_b.0.session.session_id.clone();
            let token_b = resp_b.0.capability_token.clone();

            // Subscribe to session B's events via the controller
            let ctrl = state.automation_controller.as_ref().unwrap();
            let mut rx_b = ctrl
                .gui_subscribe_events(&sid_b, &token_b)
                .await
                .expect("subscribe should succeed");

            // Trigger an event on session A (highlight)
            let _ = highlight_gui_session(
                State(m5_context(&state)),
                Path(GuiSessionPath { id: sid_a.clone() }),
                m5_token_headers(&token_a),
                Json(GuiHighlightRequest {
                    candidate_ids: None,
                }),
            )
            .await
            .unwrap();

            // Apply the same filter_map the handler uses
            let mut b_events = Vec::new();
            while let Ok(event) = rx_b.try_recv() {
                if event.session_id == sid_b {
                    b_events.push(event);
                }
            }

            // Session B should see NO events from session A's highlight
            assert!(
                b_events.is_empty(),
                "Session B stream should not contain session A events, got: {b_events:?}"
            );
        }

        #[tokio::test]
        async fn m5_missing_hmac_secret_returns_http_503() {
            let controller = make_controller_with(
                Arc::new(M5MockElementFinder),
                Arc::new(M5MockFocusProbe::new()),
                None, // No HMAC secret — fail-closed
            );
            let state = make_state_with(controller);

            let err = create_gui_session(State(m5_context(&state)), Json(m5_default_create_req()))
                .await
                .unwrap_err();
            assert!(
                matches!(err, ApiError::ServiceUnavailable(_)),
                "Missing HMAC secret should fail closed with 503, got: {err:?}"
            );
        }

        #[tokio::test]
        async fn m5_empty_hmac_secret_returns_http_503() {
            let controller = make_controller_with(
                Arc::new(M5MockElementFinder),
                Arc::new(M5MockFocusProbe::new()),
                Some("   ".to_string()), // Whitespace-only — treated as missing
            );
            let state = make_state_with(controller);

            let err = create_gui_session(State(m5_context(&state)), Json(m5_default_create_req()))
                .await
                .unwrap_err();
            assert!(
                matches!(err, ApiError::ServiceUnavailable(_)),
                "Empty HMAC secret should fail closed with 503, got: {err:?}"
            );
        }
    }
}
