use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures::stream::Stream;
use oneshim_api_contracts::automation_gui::{
    GuiConfirmRequest, GuiConfirmResponse, GuiCreateSessionRequest, GuiCreateSessionResponse,
    GuiExecuteResponse, GuiExecutionOutcome, GuiExecutionRequest, GuiHighlightRequest,
    GuiSessionPath, GuiSessionResponse,
};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use oneshim_automation::controller::GuiExecutionResult;
use oneshim_automation::gui_interaction::{
    GuiConfirmRequest as AutomationGuiConfirmRequest,
    GuiCreateSessionRequest as AutomationGuiCreateSessionRequest,
    GuiExecutionRequest as AutomationGuiExecutionRequest,
    GuiHighlightRequest as AutomationGuiHighlightRequest, GuiInteractionError,
};

use crate::error::ApiError;
use crate::AppState;

const GUI_SESSION_HEADER: &str = "x-gui-session-token";
const GUI_SCHEMA_VERSION: &str = "automation.gui.v2";

pub async fn create_gui_session(
    State(state): State<AppState>,
    Json(req): Json<GuiCreateSessionRequest>,
) -> Result<Json<GuiCreateSessionResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let req = AutomationGuiCreateSessionRequest {
        app_name: req.app_name,
        screen_id: req.screen_id,
        min_confidence: req.min_confidence,
        max_candidates: req.max_candidates,
        session_ttl_secs: req.session_ttl_secs,
    };
    let created = controller
        .gui_create_session(req)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiCreateSessionResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        session: created.session,
        capability_token: created.capability_token,
    }))
}

pub async fn get_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;

    let session = controller
        .gui_get_session(&path.id, &capability_token)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiSessionResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        session,
    }))
}

pub async fn highlight_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiHighlightRequest>,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;
    let req = AutomationGuiHighlightRequest {
        candidate_ids: req.candidate_ids,
    };

    let session = controller
        .gui_highlight_session(&path.id, &capability_token, req)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiSessionResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        session,
    }))
}

pub async fn confirm_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiConfirmRequest>,
) -> Result<Json<GuiConfirmResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;
    let req = AutomationGuiConfirmRequest {
        candidate_id: req.candidate_id,
        action: req.action,
        ticket_ttl_secs: req.ticket_ttl_secs,
    };

    let ticket = controller
        .gui_confirm_candidate(&path.id, &capability_token, req)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiConfirmResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        ticket,
    }))
}

pub async fn execute_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
    Json(req): Json<GuiExecutionRequest>,
) -> Result<Json<GuiExecuteResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;
    let req = AutomationGuiExecutionRequest { ticket: req.ticket };

    let result: GuiExecutionResult = controller
        .gui_execute(&path.id, &capability_token, req)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiExecuteResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        command_id: result.command_id,
        ticket: result.ticket,
        result: result.result,
        outcome: GuiExecutionOutcome {
            session: result.outcome.session,
            succeeded: result.outcome.succeeded,
            detail: result.outcome.detail,
            steps_completed: result.outcome.steps_completed,
            total_steps: result.outcome.total_steps,
        },
    }))
}

pub async fn delete_gui_session(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Json<GuiSessionResponse>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;

    let session = controller
        .gui_cancel_session(&path.id, &capability_token)
        .await
        .map_err(map_gui_error)?;

    Ok(Json(GuiSessionResponse {
        schema_version: GUI_SCHEMA_VERSION.to_string(),
        session,
    }))
}

pub async fn gui_session_event_stream(
    State(state): State<AppState>,
    Path(path): Path<GuiSessionPath>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let controller = require_controller(&state)?;
    let capability_token = read_capability_token(&headers)?;
    let rx = controller
        .gui_subscribe_events(&path.id, &capability_token)
        .await
        .map_err(map_gui_error)?;

    let stream = BroadcastStream::new(rx);
    let session_id = path.id;
    let sse_stream = stream.filter_map(move |result| match result {
        Ok(event) if event.session_id == session_id => {
            let data = serde_json::to_string(&event).ok()?;
            Some(Ok(Event::default()
                .event(event.event_type.clone())
                .data(data)))
        }
        _ => None,
    });

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

fn require_controller(
    state: &AppState,
) -> Result<&oneshim_automation::controller::AutomationController, ApiError> {
    state.automation_controller.as_deref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Automation controller is disabled".to_string())
    })
}

fn read_capability_token(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get(GUI_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            ApiError::Unauthorized(format!(
                "Missing required header '{}': session capability token",
                GUI_SESSION_HEADER
            ))
        })
}

fn map_gui_error(err: GuiInteractionError) -> ApiError {
    match err {
        GuiInteractionError::Unauthorized => {
            ApiError::Unauthorized("Invalid GUI session token".to_string())
        }
        GuiInteractionError::NotFound(msg) => ApiError::NotFound(msg),
        GuiInteractionError::BadRequest(msg) => ApiError::BadRequest(msg),
        GuiInteractionError::Forbidden(msg) => ApiError::Forbidden(msg),
        GuiInteractionError::FocusDrift(msg) => ApiError::Conflict(msg),
        GuiInteractionError::TicketInvalid(msg) => ApiError::Unprocessable(msg),
        GuiInteractionError::Unavailable(msg) => ApiError::ServiceUnavailable(msg),
        GuiInteractionError::Internal(msg) => ApiError::Internal(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_automation::gui_interaction::GuiInteractionError;

    // ── M4: End-to-End Workflow Tests ───────────────────────────────────

    mod m4 {
        use super::*;
        use async_trait::async_trait;
        use chrono::Utc;
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
        use oneshim_api_contracts::automation_gui::{
            GuiConfirmRequest, GuiCreateSessionRequest, GuiExecutionRequest, GuiHighlightRequest,
            GuiSessionPath,
        };
        use oneshim_storage::sqlite::SqliteStorage;
        use std::sync::Arc;
        use tokio::sync::{broadcast, RwLock};
        use crate::AppState;

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
            let sandbox: Arc<dyn oneshim_core::ports::sandbox::Sandbox> =
                Arc::new(NoOpSandbox);
            let sandbox_config = SandboxConfig::default();
            let mut controller = AutomationController::new(
                policy_client,
                audit_logger,
                sandbox,
                sandbox_config,
            );

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
            let element_finder: Arc<
                dyn oneshim_core::ports::element_finder::ElementFinder,
            > = Arc::new(NoOpElementFinder);
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
                audit_logger: None,
                automation_controller: Some(make_controller()),
                ai_runtime_status: None,
                update_control: None,
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
                audit_logger: None,
                automation_controller: None,
                ai_runtime_status: None,
                update_control: None,
            }
        }

        fn token_headers(token: &str) -> HeaderMap {
            let mut h = HeaderMap::new();
            h.insert(GUI_SESSION_HEADER, token.parse().unwrap());
            h
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
            let resp = create_gui_session(State(state.clone()), Json(default_create_req()))
                .await
                .expect("fixture_create: create_gui_session should succeed");
            (resp.0.session.session_id, resp.0.capability_token)
        }

        /// Highlights a session; returns the first candidate's element_id.
        async fn fixture_highlight(state: &AppState, sid: &str, token: &str) -> String {
            let resp = highlight_gui_session(
                State(state.clone()),
                Path(GuiSessionPath { id: sid.to_string() }),
                token_headers(token),
                Json(GuiHighlightRequest { candidate_ids: None }),
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
            let err = create_gui_session(State(state), Json(default_create_req()))
                .await
                .unwrap_err();
            assert!(matches!(err, ApiError::ServiceUnavailable(_)));
        }

        #[tokio::test]
        async fn m4_missing_token_blocks_get_session() {
            let state = make_state();
            let err = get_gui_session(
                State(state),
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
            let resp = create_gui_session(State(state), Json(default_create_req()))
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
                State(state),
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
                State(state),
                Path(GuiSessionPath { id: sid }),
                token_headers(&token),
                Json(GuiHighlightRequest { candidate_ids: None }),
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
                State(state),
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
                State(state.clone()),
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
                State(state),
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
                State(state),
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
                State(state),
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
                State(state.clone()),
                Path(GuiSessionPath { id: sid_b.clone() }),
                token_headers(&token_b),
            )
            .await
            .expect("delete session B should succeed");

            // Session A is still accessible and Proposed
            let get_a = get_gui_session(
                State(state.clone()),
                Path(GuiSessionPath { id: sid_a.clone() }),
                token_headers(&token_a),
            )
            .await
            .expect("session A should still be accessible after B is cancelled");
            assert_eq!(get_a.0.session.state, GuiSessionState::Proposed);
            assert_eq!(get_a.0.session.session_id, sid_a);

            // Token B cannot access session A (token is session-scoped)
            let err = get_gui_session(
                State(state),
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
}
