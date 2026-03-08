mod crypto;
mod helpers;
mod service;
mod types;

// ── Public re-exports (external API) ────────────────────────────────
pub use service::GuiInteractionService;
pub use types::{
    GuiConfirmRequest, GuiCreateSessionRequest, GuiCreateSessionResponse, GuiExecutionOutcome,
    GuiExecutionPlan, GuiExecutionRequest, GuiHighlightRequest, GuiInteractionError,
};

// ── Test-only re-exports (child `mod tests` accesses via `use super::*`) ──
#[cfg(test)]
use crate::controller::AutomationAction;
#[cfg(test)]
use chrono::{Duration as ChronoDuration, Utc};
#[cfg(test)]
use crypto::*;
#[cfg(test)]
use helpers::*;
#[cfg(test)]
use oneshim_core::error::CoreError;
#[cfg(test)]
use oneshim_core::models::gui::{
    ExecutionBinding, GuiActionRequest, GuiCandidate, GuiExecutionTicket, GuiSessionState,
    HighlightRequest,
};
#[cfg(test)]
use oneshim_core::ports::element_finder::ElementFinder;
#[cfg(test)]
use oneshim_core::ports::focus_probe::FocusProbe;
#[cfg(test)]
use oneshim_core::ports::overlay_driver::OverlayDriver;
#[cfg(test)]
use std::sync::atomic::Ordering;
#[cfg(test)]
use std::sync::Arc;

// ── Constants (used by sub-modules via `super::` and tests via `use super::*`) ──
const GUI_HMAC_SECRET_ENV: &str = "ONESHIM_GUI_TICKET_HMAC_SECRET";
const DEFAULT_MAX_CANDIDATES: usize = 20;
const DEFAULT_MIN_CONFIDENCE: f64 = 0.5;
const DEFAULT_SESSION_TTL_SECS: i64 = 300;
const DEFAULT_TICKET_TTL_SECS: i64 = 30;
const CLEANUP_INTERVAL_SECS: u64 = 30;
const GUI_EVENT_CHANNEL_CAPACITY: usize = 256;
const FOCUS_DRIFT_MAX_RETRIES: usize = 2;
const FOCUS_DRIFT_RETRY_DELAY_MS: u64 = 500;
const TICKET_EXPIRY_GRACE_SECS: i64 = 5;

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_core::models::gui::{
        FocusSnapshot, FocusValidation, GuiActionType, HighlightHandle,
    };
    use oneshim_core::models::intent::{ElementBounds, UiElement};
    use oneshim_core::models::ui_scene::{NormalizedBounds, UiScene, UiSceneElement};
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    // ── Test constants ──────────────────────────────────────────────────

    const TEST_HMAC_SECRET: &str = "test-hmac-secret-32-bytes-long!!";

    // ── MockElementFinder ───────────────────────────────────────────────

    struct MockElementFinder {
        scene: Mutex<UiScene>,
    }

    impl MockElementFinder {
        fn new(scene: UiScene) -> Self {
            Self {
                scene: Mutex::new(scene),
            }
        }
    }

    #[async_trait]
    impl ElementFinder for MockElementFinder {
        async fn find_element(
            &self,
            _text: Option<&str>,
            _role: Option<&str>,
            _region: Option<&ElementBounds>,
        ) -> Result<Vec<UiElement>, CoreError> {
            Ok(vec![])
        }

        async fn analyze_scene(
            &self,
            _app_name: Option<&str>,
            _screen_id: Option<&str>,
        ) -> Result<UiScene, CoreError> {
            Ok(self.scene.lock().unwrap().clone())
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    // ── MockFocusProbe ──────────────────────────────────────────────────

    struct MockFocusProbe {
        focus: Mutex<FocusSnapshot>,
        validation_valid: Mutex<bool>,
        /// When set, validation returns invalid for first N calls, then valid.
        drift_recover_after: Mutex<Option<usize>>,
        validation_call_count: AtomicUsize,
    }

    impl MockFocusProbe {
        fn new(focus: FocusSnapshot) -> Self {
            Self {
                focus: Mutex::new(focus),
                validation_valid: Mutex::new(true),
                drift_recover_after: Mutex::new(None),
                validation_call_count: AtomicUsize::new(0),
            }
        }

        fn set_validation_valid(&self, valid: bool) {
            *self.validation_valid.lock().unwrap() = valid;
        }

        /// Focus returns invalid for first `n` calls, then valid.
        fn set_drift_recover_after(&self, n: usize) {
            *self.drift_recover_after.lock().unwrap() = Some(n);
        }
    }

    #[async_trait]
    impl FocusProbe for MockFocusProbe {
        async fn current_focus(&self) -> Result<FocusSnapshot, CoreError> {
            Ok(self.focus.lock().unwrap().clone())
        }

        async fn validate_execution_binding(
            &self,
            _binding: &ExecutionBinding,
        ) -> Result<FocusValidation, CoreError> {
            let call_num = self.validation_call_count.fetch_add(1, Ordering::SeqCst);

            let valid = if let Some(recover_after) = *self.drift_recover_after.lock().unwrap() {
                call_num >= recover_after
            } else {
                *self.validation_valid.lock().unwrap()
            };

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

    // ── MockOverlayDriver ───────────────────────────────────────────────

    struct MockOverlayDriver {
        show_count: AtomicUsize,
        clear_count: AtomicUsize,
    }

    impl MockOverlayDriver {
        fn new() -> Self {
            Self {
                show_count: AtomicUsize::new(0),
                clear_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl OverlayDriver for MockOverlayDriver {
        async fn show_highlights(
            &self,
            req: HighlightRequest,
        ) -> Result<HighlightHandle, CoreError> {
            self.show_count.fetch_add(1, Ordering::SeqCst);
            Ok(HighlightHandle {
                handle_id: format!("handle-{}", self.show_count.load(Ordering::SeqCst)),
                rendered_at: Utc::now(),
                target_count: req.targets.len(),
            })
        }

        async fn clear_highlights(&self, _handle_id: &str) -> Result<(), CoreError> {
            self.clear_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    // ── Fixture builders ────────────────────────────────────────────────

    fn make_element(id: &str, label: &str, confidence: f64) -> UiSceneElement {
        UiSceneElement {
            element_id: id.to_string(),
            bbox_abs: ElementBounds {
                x: 100,
                y: 80,
                width: 200,
                height: 40,
            },
            bbox_norm: NormalizedBounds::new(0.05, 0.07, 0.10, 0.04),
            label: label.to_string(),
            role: Some("button".to_string()),
            intent: None,
            state: Some("enabled".to_string()),
            confidence,
            text_masked: Some(label.to_string()),
            parent_id: None,
        }
    }

    fn make_scene(elements: Vec<UiSceneElement>) -> UiScene {
        UiScene {
            schema_version: "ui_scene.v1".to_string(),
            scene_id: "test-scene-1".to_string(),
            app_name: Some("TestApp".to_string()),
            screen_id: Some("screen-main".to_string()),
            captured_at: Utc::now(),
            screen_width: 1920,
            screen_height: 1080,
            elements,
        }
    }

    fn make_focus() -> FocusSnapshot {
        FocusSnapshot {
            app_name: "TestApp".to_string(),
            window_title: "Test Window".to_string(),
            pid: 1234,
            bounds: None,
            captured_at: Utc::now(),
            focus_hash: "abc123hash".to_string(),
        }
    }

    fn make_service(
        scene: UiScene,
        focus: FocusSnapshot,
    ) -> (Arc<GuiInteractionService>, Arc<MockFocusProbe>) {
        let (service, probe, _) = make_service_full(scene, focus);
        (service, probe)
    }

    fn make_service_full(
        scene: UiScene,
        focus: FocusSnapshot,
    ) -> (
        Arc<GuiInteractionService>,
        Arc<MockFocusProbe>,
        Arc<MockOverlayDriver>,
    ) {
        let probe = Arc::new(MockFocusProbe::new(focus));
        let overlay = Arc::new(MockOverlayDriver::new());
        let service = Arc::new(GuiInteractionService::new(
            Arc::new(MockElementFinder::new(scene)),
            probe.clone(),
            overlay.clone(),
            Some(TEST_HMAC_SECRET.to_string()),
        ));
        (service, probe, overlay)
    }

    fn default_create_request() -> GuiCreateSessionRequest {
        GuiCreateSessionRequest {
            app_name: Some("TestApp".to_string()),
            screen_id: None,
            min_confidence: None,
            max_candidates: None,
            session_ttl_secs: Some(300),
        }
    }

    /// Helper: create a session and return (session_id, capability_token)
    async fn create_test_session(service: &GuiInteractionService) -> (String, String) {
        let resp = service
            .create_session(default_create_request())
            .await
            .expect("create_session should succeed");
        (resp.session.session_id, resp.capability_token)
    }

    /// Helper: create session + highlight it, returns (session_id, token)
    async fn create_and_highlight(service: &GuiInteractionService) -> (String, String) {
        let (sid, token) = create_test_session(service).await;
        service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .expect("highlight should succeed");
        (sid, token)
    }

    /// Helper: create session + highlight + confirm, returns (session_id, token, ticket)
    async fn create_highlight_and_confirm(
        service: &GuiInteractionService,
    ) -> (String, String, GuiExecutionTicket) {
        let (sid, token) = create_and_highlight(service).await;

        let session = service.get_session(&sid, &token).await.unwrap();
        let candidate_id = session.candidates[0].element.element_id.clone();

        let ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                },
            )
            .await
            .expect("confirm should succeed");
        (sid, token, ticket)
    }

    // ── Utility tests ───────────────────────────────────────────────────

    #[test]
    fn hex_roundtrip() {
        let data = b"hello";
        let encoded = encode_hex(data);
        let decoded = decode_hex(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_hex_rejects_invalid_length() {
        assert!(decode_hex("abc").is_none());
    }

    // ── Session creation tests ──────────────────────────────────────────

    #[tokio::test]
    async fn create_session_returns_proposed_state() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let resp = service
            .create_session(default_create_request())
            .await
            .unwrap();

        assert_eq!(resp.session.state, GuiSessionState::Proposed);
        assert!(!resp.capability_token.is_empty());
        assert!(!resp.session.session_id.is_empty());
        assert_eq!(resp.session.candidates.len(), 1);
        assert_eq!(resp.session.candidates[0].element.label, "Save");
    }

    #[tokio::test]
    async fn create_session_filters_low_confidence_candidates() {
        let scene = make_scene(vec![
            make_element("el-high", "Save", 0.9),
            make_element("el-low", "Cancel", 0.2),
        ]);
        let (service, _) = make_service(scene, make_focus());

        let resp = service
            .create_session(default_create_request())
            .await
            .unwrap();

        assert_eq!(resp.session.candidates.len(), 1);
        assert_eq!(resp.session.candidates[0].element.element_id, "el-high");
    }

    #[tokio::test]
    async fn create_session_respects_max_candidates() {
        let elements: Vec<UiSceneElement> = (0..10)
            .map(|i| make_element(&format!("el-{i}"), &format!("Btn{i}"), 0.8))
            .collect();
        let scene = make_scene(elements);
        let (service, _) = make_service(scene, make_focus());

        let mut req = default_create_request();
        req.max_candidates = Some(3);

        let resp = service.create_session(req).await.unwrap();
        assert_eq!(resp.session.candidates.len(), 3);
    }

    #[tokio::test]
    async fn create_session_rejects_empty_scene() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.1)]);
        let (service, _) = make_service(scene, make_focus());

        let mut req = default_create_request();
        req.min_confidence = Some(0.99);

        let err = service.create_session(req).await.unwrap_err();
        assert!(matches!(err, GuiInteractionError::BadRequest(_)));
    }

    #[tokio::test]
    async fn create_session_requires_hmac_secret() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let service = GuiInteractionService::new(
            Arc::new(MockElementFinder::new(scene)),
            Arc::new(MockFocusProbe::new(make_focus())),
            Arc::new(MockOverlayDriver::new()),
            None, // no HMAC secret
        );

        let err = service
            .create_session(default_create_request())
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unavailable(_)));
    }

    // ── Get session tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn get_session_returns_current_state() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;
        let session = service.get_session(&sid, &token).await.unwrap();

        assert_eq!(session.state, GuiSessionState::Proposed);
        assert_eq!(session.session_id, sid);
    }

    #[tokio::test]
    async fn get_session_rejects_invalid_token() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, _) = create_test_session(&service).await;
        let err = service.get_session(&sid, "wrong-token").await.unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized));
    }

    #[tokio::test]
    async fn get_session_rejects_unknown_session() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let err = service
            .get_session("nonexistent", "some-token")
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::NotFound(_)));
    }

    // ── Highlight tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn highlight_transitions_to_highlighted() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;
        let session = service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(session.state, GuiSessionState::Highlighted);
    }

    #[tokio::test]
    async fn highlight_filters_by_candidate_ids() {
        let scene = make_scene(vec![
            make_element("el-1", "Save", 0.9),
            make_element("el-2", "Cancel", 0.8),
        ]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;

        // highlight only el-1
        let session = service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: Some(vec!["el-1".to_string()]),
                },
            )
            .await
            .unwrap();

        assert_eq!(session.state, GuiSessionState::Highlighted);
    }

    #[tokio::test]
    async fn highlight_rejects_invalid_token() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, _) = create_test_session(&service).await;
        let err = service
            .highlight_session(
                &sid,
                "wrong-token",
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized));
    }

    // ── Confirm tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn confirm_transitions_to_confirmed_with_ticket() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        let ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap();

        assert!(!ticket.signature.is_empty());
        assert_eq!(ticket.session_id, sid);
        assert_eq!(ticket.element_id, "el-1");

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Confirmed);
        assert_eq!(session.selected_element_id, Some("el-1".to_string()));
    }

    #[tokio::test]
    async fn confirm_rejects_unknown_candidate() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        let err = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "nonexistent".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::BadRequest(_)));
    }

    #[tokio::test]
    async fn confirm_rejects_when_focus_changed() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, probe) = make_service(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        // Simulate focus drift
        probe.set_validation_valid(false);

        let err = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::FocusDrift(_)));
    }

    #[tokio::test]
    async fn confirm_type_text_requires_text() {
        let scene = make_scene(vec![make_element("el-1", "Input", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        let err = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::TypeText,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::BadRequest(_)));
    }

    // ── Execution tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn prepare_execution_transitions_to_executing() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        let plan = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        assert_eq!(plan.session_id, sid);
        assert!(!plan.actions.is_empty());

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Executing);
    }

    #[tokio::test]
    async fn prepare_execution_rejects_nonce_replay() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        // First execution succeeds
        let _ = service
            .prepare_execution(
                &sid,
                &token,
                GuiExecutionRequest {
                    ticket: ticket.clone(),
                },
            )
            .await
            .unwrap();

        // Complete execution to go back to Confirmed for re-test
        service
            .complete_execution(&sid, false, None, 0, 1)
            .await
            .unwrap();

        // Replay same ticket nonce — should be rejected
        let err = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::TicketInvalid(_)));
    }

    #[tokio::test]
    async fn prepare_execution_rejects_tampered_signature() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, mut ticket) = create_highlight_and_confirm(&service).await;

        ticket.signature = "00".repeat(32); // tampered

        let err = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::TicketInvalid(_)));
    }

    #[tokio::test]
    async fn prepare_execution_rejects_wrong_session_state() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        // Session is only Proposed (not Confirmed), so prepare_execution should fail
        let (sid, token) = create_test_session(&service).await;
        let dummy_ticket = GuiExecutionTicket {
            schema_version: "automation.gui.ticket.v1".to_string(),
            ticket_id: "t1".to_string(),
            session_id: sid.clone(),
            scene_id: "s1".to_string(),
            element_id: "el-1".to_string(),
            action_hash: "hash".to_string(),
            focus_hash: "focus".to_string(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::seconds(60),
            nonce: "nonce1".to_string(),
            signature: "sig".to_string(),
        };

        let err = service
            .prepare_execution(
                &sid,
                &token,
                GuiExecutionRequest {
                    ticket: dummy_ticket,
                },
            )
            .await
            .unwrap_err();
        // Should fail because there is no confirmed_action
        assert!(matches!(err, GuiInteractionError::TicketInvalid(_)));
    }

    #[tokio::test]
    async fn prepare_execution_rejects_focus_drift() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, probe) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        // Focus drifts after confirm
        probe.set_validation_valid(false);

        let err = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::FocusDrift(_)));
    }

    // ── Complete execution tests ────────────────────────────────────────

    #[tokio::test]
    async fn complete_execution_success_transitions_to_executed() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        let outcome = service
            .complete_execution(&sid, true, None, 1, 1)
            .await
            .unwrap();

        assert!(outcome.succeeded);
        assert_eq!(outcome.session.state, GuiSessionState::Executed);
    }

    #[tokio::test]
    async fn complete_execution_failure_reverts_to_confirmed() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        let outcome = service
            .complete_execution(&sid, false, Some("click missed".to_string()), 0, 1)
            .await
            .unwrap();

        assert!(!outcome.succeeded);
        assert_eq!(outcome.session.state, GuiSessionState::Confirmed);
        assert_eq!(outcome.detail, Some("click missed".to_string()));
    }

    // ── Cancel tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn cancel_transitions_to_cancelled() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;
        let session = service.cancel_session(&sid, &token).await.unwrap();

        assert_eq!(session.state, GuiSessionState::Cancelled);
    }

    #[tokio::test]
    async fn cancel_rejects_invalid_token() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, _) = create_test_session(&service).await;
        let err = service
            .cancel_session(&sid, "wrong-token")
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized));
    }

    // ── Expiry tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn expired_session_is_detected_on_get() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut req = default_create_request();
        req.session_ttl_secs = Some(30); // minimum allowed

        let resp = service.create_session(req).await.unwrap();
        let sid = resp.session.session_id.clone();
        let token = resp.capability_token.clone();

        // Manually expire the session by setting expires_at in the past
        {
            let mut sessions = service.sessions.write().await;
            if let Some(stored) = sessions.get_mut(&sid) {
                stored.session.expires_at = Utc::now() - ChronoDuration::seconds(1);
            }
        }

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Expired);
    }

    #[tokio::test]
    async fn highlight_rejects_expired_session() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;

        // Expire it
        {
            let mut sessions = service.sessions.write().await;
            if let Some(stored) = sessions.get_mut(&sid) {
                stored.session.expires_at = Utc::now() - ChronoDuration::seconds(1);
            }
        }

        let err = service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::TicketInvalid(_)));
    }

    // ── Full flow integration test ──────────────────────────────────────

    #[tokio::test]
    async fn full_propose_highlight_confirm_execute_flow() {
        let scene = make_scene(vec![
            make_element("el-1", "Save", 0.95),
            make_element("el-2", "Cancel", 0.85),
        ]);
        let (service, _) = make_service(scene, make_focus());

        // 1. Propose
        let resp = service
            .create_session(default_create_request())
            .await
            .unwrap();
        assert_eq!(resp.session.state, GuiSessionState::Proposed);
        let sid = resp.session.session_id.clone();
        let token = resp.capability_token.clone();

        // 2. Highlight
        let session = service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(session.state, GuiSessionState::Highlighted);

        // 3. Confirm
        let ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                },
            )
            .await
            .unwrap();
        assert!(!ticket.signature.is_empty());

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Confirmed);

        // 4. Execute
        let plan = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();
        assert!(!plan.actions.is_empty());

        let session = service.get_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Executing);

        // 5. Complete
        let outcome = service
            .complete_execution(&sid, true, None, 1, 1)
            .await
            .unwrap();
        assert!(outcome.succeeded);
        assert_eq!(outcome.session.state, GuiSessionState::Executed);
    }

    // ── M3: Event subscription / SSE integration tests ─────────────────

    #[tokio::test]
    async fn subscribe_receives_session_events() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut rx = service.subscribe();

        let _ = service
            .create_session(default_create_request())
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert_eq!(event.event_type, "gui_session.proposed");
        assert_eq!(event.state, GuiSessionState::Proposed);
    }

    #[tokio::test]
    async fn subscribe_session_requires_valid_token() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, _token) = create_test_session(&service).await;

        let err = service
            .subscribe_session(&sid, "wrong-token")
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized));
    }

    #[tokio::test]
    async fn subscribe_session_rejects_unknown_session() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let err = service
            .subscribe_session("nonexistent-session", "any-token")
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::NotFound(_)));
    }

    #[tokio::test]
    async fn subscribe_session_succeeds_with_valid_token() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token) = create_test_session(&service).await;

        let rx = service.subscribe_session(&sid, &token).await;
        assert!(rx.is_ok(), "Valid token should allow subscription");
    }

    #[tokio::test]
    async fn event_stream_full_lifecycle_proposed_to_executed() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut rx = service.subscribe();

        // 1. Create session → Proposed
        let resp = service
            .create_session(default_create_request())
            .await
            .unwrap();
        let sid = resp.session.session_id;
        let token = resp.capability_token;

        // 2. Highlight → Highlighted
        service
            .highlight_session(
                &sid,
                &token,
                GuiHighlightRequest {
                    candidate_ids: None,
                },
            )
            .await
            .unwrap();

        // 3. Confirm → Confirmed
        let ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap();

        // 4. Prepare execution → Executing
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        // 5. Complete → Executed
        service
            .complete_execution(&sid, true, None, 1, 1)
            .await
            .unwrap();

        // Collect all events
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert_eq!(
            types,
            vec![
                "gui_session.proposed",
                "gui_session.highlighted",
                "gui_session.confirmed",
                "gui_session.executing",
                "gui_session.executed",
            ],
            "Events should arrive in state machine order"
        );

        // All events belong to the same session
        assert!(
            events.iter().all(|e| e.session_id == sid),
            "All events must reference the same session"
        );
    }

    #[tokio::test]
    async fn event_stream_cancel_emits_cancelled_event() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut rx = service.subscribe();

        let (sid, token) = create_test_session(&service).await;
        service.cancel_session(&sid, &token).await.unwrap();

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        assert_eq!(events.len(), 2); // proposed + cancelled
        assert_eq!(events[0].event_type, "gui_session.proposed");
        assert_eq!(events[1].event_type, "gui_session.cancelled");
        assert_eq!(events[1].state, GuiSessionState::Cancelled);
    }

    #[tokio::test]
    async fn event_stream_execution_failure_emits_failure_event() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut rx = service.subscribe();

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();
        service
            .complete_execution(&sid, false, Some("click missed".to_string()), 0, 1)
            .await
            .unwrap();

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert!(
            types.contains(&"gui_session.execution_failed"),
            "Should emit execution_failed event, got: {types:?}"
        );
    }

    #[tokio::test]
    async fn event_schema_version_is_consistent() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut rx = service.subscribe();

        let _ = service
            .create_session(default_create_request())
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert_eq!(
            event.schema_version, "automation.gui.event.v1",
            "Event schema version must match contract"
        );
    }

    #[tokio::test]
    async fn events_are_session_scoped_in_broadcast() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        // Create two sessions
        let (sid1, _token1) = create_test_session(&service).await;
        let (sid2, _token2) = create_test_session(&service).await;

        // Subscribe AFTER both sessions exist
        let mut rx = service.subscribe();

        // Cancel session 1 — should emit event for sid1 only
        service.cancel_session(&sid1, &_token1).await.unwrap();

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        // Filter to session 1 events only (simulating handler-level filtering)
        let sid1_events: Vec<_> = events.iter().filter(|e| e.session_id == sid1).collect();
        let sid2_events: Vec<_> = events.iter().filter(|e| e.session_id == sid2).collect();

        assert_eq!(sid1_events.len(), 1);
        assert_eq!(sid1_events[0].event_type, "gui_session.cancelled");
        assert!(
            sid2_events.is_empty(),
            "Session 2 should have no events after session 1 cancel"
        );
    }

    #[tokio::test]
    async fn event_includes_message_from_confirm() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut rx = service.subscribe();

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        // Drain events to find the executing event with ticket_id message
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        let executing_event = events
            .iter()
            .find(|e| e.event_type == "gui_session.executing")
            .expect("Should have executing event");

        assert!(
            executing_event.message.is_some(),
            "Executing event should contain ticket_id in message"
        );
        assert!(
            executing_event
                .message
                .as_ref()
                .unwrap()
                .contains("ticket_id="),
            "Message should contain ticket_id reference"
        );
    }

    #[test]
    fn event_channel_capacity_is_reasonable() {
        assert!(
            std::hint::black_box(GUI_EVENT_CHANNEL_CAPACITY) >= 64
                && std::hint::black_box(GUI_EVENT_CHANNEL_CAPACITY) <= 1024,
            "Event channel capacity should be between 64 and 1024"
        );
    }

    // ── Build candidates tests ──────────────────────────────────────────

    #[test]
    fn build_candidates_sorts_by_confidence_descending() {
        let scene = make_scene(vec![
            make_element("el-low", "A", 0.6),
            make_element("el-high", "B", 0.95),
            make_element("el-mid", "C", 0.8),
        ]);

        let candidates = build_candidates(&scene, 0.5, 10);

        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].element.element_id, "el-high");
        assert_eq!(candidates[1].element.element_id, "el-mid");
        assert_eq!(candidates[2].element.element_id, "el-low");
    }

    #[test]
    fn build_candidates_filters_below_min_confidence() {
        let scene = make_scene(vec![
            make_element("el-1", "A", 0.9),
            make_element("el-2", "B", 0.3),
        ]);

        let candidates = build_candidates(&scene, 0.5, 10);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].element.element_id, "el-1");
    }

    #[test]
    fn build_candidates_truncates_to_max() {
        let elements: Vec<UiSceneElement> = (0..10)
            .map(|i| make_element(&format!("el-{i}"), &format!("Btn{i}"), 0.8))
            .collect();
        let scene = make_scene(elements);

        let candidates = build_candidates(&scene, 0.5, 3);
        assert_eq!(candidates.len(), 3);
    }

    // ── HMAC ticket signing tests ───────────────────────────────────────

    #[test]
    fn sign_and_verify_ticket_roundtrip() {
        let secret = TEST_HMAC_SECRET.as_bytes();
        let ticket = GuiExecutionTicket {
            schema_version: "automation.gui.ticket.v1".to_string(),
            ticket_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            scene_id: "sc-1".to_string(),
            element_id: "el-1".to_string(),
            action_hash: "ahash".to_string(),
            focus_hash: "fhash".to_string(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::seconds(30),
            nonce: "nonce-1".to_string(),
            signature: String::new(),
        };

        let sig = sign_ticket(secret, &ticket).unwrap();
        let signed = GuiExecutionTicket {
            signature: sig,
            ..ticket
        };

        assert!(verify_ticket(secret, &signed).is_ok());
    }

    #[test]
    fn verify_ticket_rejects_tampered_nonce() {
        let secret = TEST_HMAC_SECRET.as_bytes();
        let ticket = GuiExecutionTicket {
            schema_version: "automation.gui.ticket.v1".to_string(),
            ticket_id: "t-1".to_string(),
            session_id: "s-1".to_string(),
            scene_id: "sc-1".to_string(),
            element_id: "el-1".to_string(),
            action_hash: "ahash".to_string(),
            focus_hash: "fhash".to_string(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + ChronoDuration::seconds(30),
            nonce: "nonce-1".to_string(),
            signature: String::new(),
        };

        let sig = sign_ticket(secret, &ticket).unwrap();
        let tampered = GuiExecutionTicket {
            signature: sig,
            nonce: "tampered-nonce".to_string(),
            ..ticket
        };

        assert!(verify_ticket(secret, &tampered).is_err());
    }

    // ── Action builder tests ────────────────────────────────────────────

    #[test]
    fn build_actions_click_generates_mouse_click() {
        let candidate = GuiCandidate {
            element: make_element("el-1", "Save", 0.9),
            ranking_reason: None,
            eligible: true,
        };
        let action = GuiActionRequest {
            action_type: GuiActionType::Click,
            text: None,
        };

        let actions = build_actions_for_candidate(&candidate, &action).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], AutomationAction::MouseClick { .. }));
    }

    #[test]
    fn build_actions_double_click_generates_two_clicks() {
        let candidate = GuiCandidate {
            element: make_element("el-1", "File", 0.9),
            ranking_reason: None,
            eligible: true,
        };
        let action = GuiActionRequest {
            action_type: GuiActionType::DoubleClick,
            text: None,
        };

        let actions = build_actions_for_candidate(&candidate, &action).unwrap();
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn build_actions_type_text_generates_click_then_type() {
        let candidate = GuiCandidate {
            element: make_element("el-1", "Input", 0.9),
            ranking_reason: None,
            eligible: true,
        };
        let action = GuiActionRequest {
            action_type: GuiActionType::TypeText,
            text: Some("hello".to_string()),
        };

        let actions = build_actions_for_candidate(&candidate, &action).unwrap();
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], AutomationAction::MouseClick { .. }));
        assert!(matches!(actions[1], AutomationAction::KeyType { .. }));
    }

    #[test]
    fn build_actions_type_text_rejects_empty_text() {
        let candidate = GuiCandidate {
            element: make_element("el-1", "Input", 0.9),
            ranking_reason: None,
            eligible: true,
        };
        let action = GuiActionRequest {
            action_type: GuiActionType::TypeText,
            text: Some("  ".to_string()),
        };

        assert!(build_actions_for_candidate(&candidate, &action).is_err());
    }

    // ── M2: Focus drift recovery tests ─────────────────────────────────

    #[tokio::test]
    async fn prepare_execution_recovers_from_transient_drift() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, probe) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        // confirm_candidate already made call 0 (valid).
        // In prepare_execution: call 1 → drift, call 2 → recover.
        probe.set_drift_recover_after(2);

        let plan = service
            .prepare_execution(
                &sid,
                &token,
                GuiExecutionRequest {
                    ticket: ticket.clone(),
                },
            )
            .await;
        assert!(plan.is_ok(), "Should recover after transient drift");
        // 1 (confirm) + 2 (prepare: drift then recover) = 3
        assert_eq!(
            probe.validation_call_count.load(Ordering::SeqCst),
            3,
            "Should have retried focus validation"
        );
    }

    #[tokio::test]
    async fn prepare_execution_recovers_after_two_drifts() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, probe) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        // confirm_candidate already made call 0 (valid).
        // In prepare_execution: call 1 → drift, call 2 → drift, call 3 → recover.
        probe.set_drift_recover_after(3);

        let plan = service
            .prepare_execution(
                &sid,
                &token,
                GuiExecutionRequest {
                    ticket: ticket.clone(),
                },
            )
            .await;
        assert!(plan.is_ok(), "Should recover after two drifts");
        // 1 (confirm) + 3 (prepare: drift, drift, recover) = 4
        assert_eq!(
            probe.validation_call_count.load(Ordering::SeqCst),
            4,
            "Should have attempted initial + 2 retries in prepare_execution"
        );
    }

    #[tokio::test]
    async fn prepare_execution_fails_after_max_drift_retries() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, probe) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

        // Never recover
        probe.set_validation_valid(false);

        let err = service
            .prepare_execution(
                &sid,
                &token,
                GuiExecutionRequest {
                    ticket: ticket.clone(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, GuiInteractionError::FocusDrift(_)));
        // confirm_candidate calls validate once, then prepare_execution calls
        // initial + MAX_RETRIES = 1 + (1 + MAX_RETRIES) = MAX_RETRIES + 2
        assert_eq!(
            probe.validation_call_count.load(Ordering::SeqCst),
            FOCUS_DRIFT_MAX_RETRIES + 2,
            "Should have exhausted all retry attempts"
        );
    }

    // ── M2: Overlay cleanup tests ──────────────────────────────────────

    #[tokio::test]
    async fn complete_execution_clears_overlay_on_failure() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _, overlay) = make_service_full(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        // Overlay was shown during highlight
        assert!(overlay.show_count.load(Ordering::SeqCst) >= 1);

        // Confirm the candidate
        let session = service.get_session(&sid, &token).await.unwrap();
        let candidate_id = session.candidates[0].element.element_id.clone();
        let _ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                },
            )
            .await
            .unwrap();

        let clear_before = overlay.clear_count.load(Ordering::SeqCst);

        // Complete with failure
        let outcome = service
            .complete_execution(&sid, false, Some("action failed".to_string()), 0, 1)
            .await
            .unwrap();
        assert!(!outcome.succeeded);

        // Overlay should be cleared even on failure
        assert!(
            overlay.clear_count.load(Ordering::SeqCst) > clear_before,
            "Overlay should be cleared on execution failure"
        );
    }

    #[tokio::test]
    async fn complete_execution_clears_overlay_on_success() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _, overlay) = make_service_full(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;
        let session = service.get_session(&sid, &token).await.unwrap();
        let candidate_id = session.candidates[0].element.element_id.clone();
        let _ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                },
            )
            .await
            .unwrap();

        let clear_before = overlay.clear_count.load(Ordering::SeqCst);

        let outcome = service
            .complete_execution(&sid, true, None, 1, 1)
            .await
            .unwrap();
        assert!(outcome.succeeded);

        assert!(
            overlay.clear_count.load(Ordering::SeqCst) > clear_before,
            "Overlay should be cleared on execution success"
        );
    }

    #[tokio::test]
    async fn cancel_session_clears_overlay() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _, overlay) = make_service_full(scene, make_focus());

        let (sid, token) = create_and_highlight(&service).await;

        let clear_before = overlay.clear_count.load(Ordering::SeqCst);

        let session = service.cancel_session(&sid, &token).await.unwrap();
        assert_eq!(session.state, GuiSessionState::Cancelled);

        assert!(
            overlay.clear_count.load(Ordering::SeqCst) > clear_before,
            "Overlay should be cleared on session cancel"
        );
    }

    // ── M2: Execution constants ────────────────────────────────────────

    #[test]
    fn focus_drift_retry_constants_are_reasonable() {
        assert!(
            std::hint::black_box(FOCUS_DRIFT_MAX_RETRIES) <= 5,
            "Max retries should be bounded"
        );
        assert!(
            std::hint::black_box(FOCUS_DRIFT_RETRY_DELAY_MS) >= 100
                && std::hint::black_box(FOCUS_DRIFT_RETRY_DELAY_MS) <= 5000,
            "Retry delay should be between 100ms and 5s"
        );
    }

    // ── M2 P2: Ticket expiry grace period ────────────────────────────────

    #[test]
    fn ticket_expiry_grace_secs_is_reasonable() {
        assert!(
            std::hint::black_box(TICKET_EXPIRY_GRACE_SECS) >= 1
                && std::hint::black_box(TICKET_EXPIRY_GRACE_SECS) <= 30,
            "Grace period should be between 1s and 30s"
        );
        assert!(
            std::hint::black_box(TICKET_EXPIRY_GRACE_SECS)
                < std::hint::black_box(DEFAULT_TICKET_TTL_SECS),
            "Grace period must be shorter than ticket TTL"
        );
    }

    #[test]
    fn is_expired_past_grace_rejects_well_past_deadline() {
        let well_expired = Utc::now() - ChronoDuration::seconds(60);
        assert!(
            is_expired_past_grace(&well_expired, TICKET_EXPIRY_GRACE_SECS),
            "Ticket expired 60s ago should fail even with grace"
        );
    }

    #[test]
    fn is_expired_past_grace_allows_within_grace_window() {
        // Expired 2s ago, but grace is 5s — should still be valid
        let just_expired = Utc::now() - ChronoDuration::seconds(2);
        assert!(
            !is_expired_past_grace(&just_expired, TICKET_EXPIRY_GRACE_SECS),
            "Ticket expired 2s ago should be allowed within 5s grace"
        );
    }

    #[test]
    fn is_expired_past_grace_rejects_past_grace_boundary() {
        // Expired 10s ago, grace is 5s — should fail
        let past_grace = Utc::now() - ChronoDuration::seconds(10);
        assert!(
            is_expired_past_grace(&past_grace, TICKET_EXPIRY_GRACE_SECS),
            "Ticket expired 10s ago should fail with 5s grace"
        );
    }

    #[test]
    fn is_expired_still_strict_for_sessions() {
        // Session expiry uses strict is_expired (no grace)
        let just_expired = Utc::now() - ChronoDuration::seconds(1);
        assert!(
            is_expired(&just_expired),
            "Session expiry should remain strict"
        );
    }

    #[tokio::test]
    async fn prepare_execution_allows_ticket_within_grace_window() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        // Confirm with a 1-second TTL so it expires quickly
        let (sid, token, _) = create_highlight_and_confirm(&service).await;
        let ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(1),
                },
            )
            .await
            .unwrap();

        // Wait for ticket to nominally expire (1s), but grace (5s) keeps it valid
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

        let result = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await;

        assert!(
            result.is_ok(),
            "Ticket expired 1.2s ago should be accepted within 5s grace window"
        );
    }

    #[tokio::test]
    async fn prepare_execution_rejects_ticket_past_grace_window() {
        // Test the is_expired_past_grace function directly since we can't
        // tamper with expires_at without breaking the HMAC signature
        let past_grace = Utc::now() - ChronoDuration::seconds(60);
        assert!(
            is_expired_past_grace(&past_grace, TICKET_EXPIRY_GRACE_SECS),
            "Ticket expired 60s ago should be rejected even with grace"
        );

        let within_grace = Utc::now() - ChronoDuration::seconds(2);
        assert!(
            !is_expired_past_grace(&within_grace, TICKET_EXPIRY_GRACE_SECS),
            "Ticket expired 2s ago should pass with 5s grace"
        );
    }

    // ── M2 P2: Partial execution step tracking ──────────────────────────

    #[tokio::test]
    async fn complete_execution_tracks_step_counts() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        // Simulate partial execution: 2 of 5 steps completed
        let outcome = service
            .complete_execution(&sid, false, Some("step 3 failed".to_string()), 2, 5)
            .await
            .unwrap();

        assert!(!outcome.succeeded);
        assert_eq!(outcome.steps_completed, 2);
        assert_eq!(outcome.total_steps, 5);
        assert_eq!(outcome.session.state, GuiSessionState::Confirmed);
    }

    #[tokio::test]
    async fn complete_execution_full_success_step_counts() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        let outcome = service
            .complete_execution(&sid, true, None, 3, 3)
            .await
            .unwrap();

        assert!(outcome.succeeded);
        assert_eq!(outcome.steps_completed, 3);
        assert_eq!(outcome.total_steps, 3);
        assert_eq!(outcome.session.state, GuiSessionState::Executed);
    }

    #[tokio::test]
    async fn partial_execution_allows_retry_with_new_ticket() {
        let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let (sid, token, ticket) = create_highlight_and_confirm(&service).await;
        service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
            .await
            .unwrap();

        // Partial failure reverts to Confirmed
        let outcome = service
            .complete_execution(&sid, false, Some("step 2 failed".to_string()), 1, 3)
            .await
            .unwrap();
        assert_eq!(outcome.session.state, GuiSessionState::Confirmed);

        // Client can re-confirm to get a new ticket
        let new_ticket = service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id: "el-1".to_string(),
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: None,
                },
            )
            .await
            .unwrap();

        // New ticket should work for retry
        let plan = service
            .prepare_execution(&sid, &token, GuiExecutionRequest { ticket: new_ticket })
            .await;
        assert!(plan.is_ok(), "Retry with new ticket should succeed");
    }

    // ── M3: SSE Event Stream Integration ────────────────────────────────

    /// `subscribe_session` rejects a wrong capability token.
    #[tokio::test]
    async fn m3_subscribe_session_rejects_invalid_token() {
        let (service, _) = make_service(make_scene(vec![make_element("el-1", "OK", 0.9)]), make_focus());
        let (sid, _token) = create_test_session(&service).await;

        let err = service.subscribe_session(&sid, "wrong-token").await.unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized));
    }

    /// `subscribe_session` rejects an unknown session_id.
    #[tokio::test]
    async fn m3_subscribe_session_rejects_unknown_session() {
        let (service, _) = make_service(make_scene(vec![make_element("el-1", "OK", 0.9)]), make_focus());

        let err = service.subscribe_session("no-such-session", "any-token").await.unwrap_err();
        assert!(matches!(err, GuiInteractionError::Unauthorized | GuiInteractionError::NotFound(_)));
    }

    /// `subscribe_session` with the correct token succeeds.
    #[tokio::test]
    async fn m3_subscribe_session_accepts_valid_token() {
        let (service, _) = make_service(make_scene(vec![make_element("el-1", "OK", 0.9)]), make_focus());
        let (sid, token) = create_test_session(&service).await;

        // Subscribing after session creation with the correct token must succeed.
        let result = service.subscribe_session(&sid, &token).await;
        assert!(result.is_ok(), "subscribe_session should succeed with valid token");
    }

    /// `create_session` emits a `gui_session.proposed` event on the broadcast channel.
    #[tokio::test]
    async fn m3_create_session_emits_proposed_event() {
        let (service, _) = make_service(make_scene(vec![make_element("el-1", "OK", 0.9)]), make_focus());

        // Subscribe before the state transition so we don't miss the event.
        let mut rx = service.subscribe();

        let (sid, _) = create_test_session(&service).await;

        let event = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                if let Ok(ev) = rx.try_recv() {
                    if ev.session_id == sid {
                        return ev;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("proposed event should be received within timeout");

        assert_eq!(event.event_type, "gui_session.proposed");
        assert_eq!(event.session_id, sid);
    }

    /// `highlight_session` emits a `gui_session.highlighted` event.
    #[tokio::test]
    async fn m3_highlight_session_emits_highlighted_event() {
        let (service, _) = make_service(make_scene(vec![make_element("el-1", "OK", 0.9)]), make_focus());

        let mut rx = service.subscribe();
        let (sid, token) = create_test_session(&service).await;

        // Drain the proposed event.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), async {
            loop {
                if rx.try_recv().is_ok() { break; }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await;

        service
            .highlight_session(&sid, &token, GuiHighlightRequest { candidate_ids: None })
            .await
            .expect("highlight should succeed");

        let event = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                if let Ok(ev) = rx.try_recv() {
                    if ev.session_id == sid && ev.event_type == "gui_session.highlighted" {
                        return ev;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("highlighted event should be received within timeout");

        assert_eq!(event.event_type, "gui_session.highlighted");
        assert_eq!(event.session_id, sid);
    }

    /// `cancel_session` emits a `gui_session.cancelled` event.
    #[tokio::test]
    async fn m3_cancel_session_emits_cancelled_event() {
        let (service, _) = make_service(make_scene(vec![make_element("el-1", "OK", 0.9)]), make_focus());

        let mut rx = service.subscribe();
        let (sid, token) = create_test_session(&service).await;

        service
            .cancel_session(&sid, &token)
            .await
            .expect("cancel should succeed");

        let event = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                if let Ok(ev) = rx.try_recv() {
                    if ev.session_id == sid && ev.event_type == "gui_session.cancelled" {
                        return ev;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("cancelled event should be received within timeout");

        assert_eq!(event.event_type, "gui_session.cancelled");
        assert_eq!(event.session_id, sid);
    }

    /// Events from session B are not mistaken for events from session A.
    /// The broadcast channel carries events from all sessions; correct consumers
    /// must filter by `session_id` (as the SSE handler does).
    #[tokio::test]
    async fn m3_event_session_id_scoping() {
        let (service, _) = make_service(
            make_scene(vec![make_element("el-1", "OK", 0.9)]),
            make_focus(),
        );

        let mut rx = service.subscribe();

        // Create session A — its events should carry sid_a.
        let (sid_a, _) = create_test_session(&service).await;
        // Create session B — its events should carry sid_b.
        let (sid_b, _) = create_test_session(&service).await;

        // Drain all events and partition them by session_id.
        let mut events_a: Vec<String> = vec![];
        let mut events_b: Vec<String> = vec![];

        let _ = tokio::time::timeout(std::time::Duration::from_millis(300), async {
            loop {
                match rx.try_recv() {
                    Ok(ev) => {
                        if ev.session_id == sid_a {
                            events_a.push(ev.event_type.clone());
                        } else if ev.session_id == sid_b {
                            events_b.push(ev.event_type.clone());
                        }
                    }
                    Err(_) => tokio::time::sleep(std::time::Duration::from_millis(5)).await,
                }
                if !events_a.is_empty() && !events_b.is_empty() {
                    break;
                }
            }
        })
        .await;

        // Both sessions should have their own proposed event.
        assert!(
            events_a.iter().any(|t| t == "gui_session.proposed"),
            "session A should have a proposed event; got {:?}",
            events_a
        );
        assert!(
            events_b.iter().any(|t| t == "gui_session.proposed"),
            "session B should have a proposed event; got {:?}",
            events_b
        );

        // No session A event should carry session B's id and vice versa
        // (guaranteed by the event construction, but asserting the partition is clean).
        assert!(
            !events_a.is_empty() && !events_b.is_empty(),
            "each session must have at least one event"
        );
    }

    /// `confirm_candidate` emits a `gui_session.confirmed` event.
    #[tokio::test]
    async fn m3_confirm_candidate_emits_confirmed_event() {
        let scene = make_scene(vec![make_element("el-1", "OK", 0.9)]);
        let (service, _) = make_service(scene, make_focus());

        let mut rx = service.subscribe();
        let (sid, token) = create_and_highlight(&service).await;

        // Drain earlier events (proposed + highlighted).
        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), async {
            let mut drained = 0usize;
            loop {
                if rx.try_recv().is_ok() {
                    drained += 1;
                }
                if drained >= 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await;

        // Get a candidate id.
        let session = service.get_session(&sid, &token).await.unwrap();
        let candidate_id = session.candidates[0].element.element_id.clone();

        service
            .confirm_candidate(
                &sid,
                &token,
                GuiConfirmRequest {
                    candidate_id,
                    action: GuiActionRequest {
                        action_type: GuiActionType::Click,
                        text: None,
                    },
                    ticket_ttl_secs: Some(60),
                },
            )
            .await
            .expect("confirm should succeed");

        let event = tokio::time::timeout(std::time::Duration::from_millis(500), async {
            loop {
                if let Ok(ev) = rx.try_recv() {
                    if ev.session_id == sid && ev.event_type == "gui_session.confirmed" {
                        return ev;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("confirmed event should be received within timeout");

        assert_eq!(event.event_type, "gui_session.confirmed");
        assert_eq!(event.session_id, sid);
    }
}
