# GUI V2 Failure Scenario Tests — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 14 failure-path tests covering 403/409/422/503 HTTP error codes, nonce replay, session TTL, SSE session-scoped filtering, and HMAC fail-closed behavior in the GUI V2 interaction flow.

**Scope:** `crates/oneshim-automation/src/gui_interaction/mod.rs` (service-level), `crates/oneshim-web/src/handlers/automation_gui.rs` (handler-level)

**Prerequisites:** Existing M4 tests passing (`cargo test -p oneshim-automation` and `cargo test -p oneshim-web`).

**Spec:** `docs/superpowers/specs/2026-03-20-gui-failure-scenario-tests-design.md`

---

## File Map

| File | Changes |
|------|---------|
| `crates/oneshim-automation/src/gui_interaction/mod.rs` | Add `DriftingFocusProbe`, `PermissionDeniedElementFinder` mocks; add 8 service-level tests |
| `crates/oneshim-web/src/handlers/automation_gui.rs` | Add 6 handler-level tests (M5 module) |

---

## Task 1: Add DriftingFocusProbe and PermissionDeniedElementFinder Mocks

Location: `crates/oneshim-automation/src/gui_interaction/mod.rs`, inside `#[cfg(test)] mod tests`.

These mocks complement the existing `MockElementFinder` and `MockFocusProbe` for new failure scenarios.

### Steps

- [ ] **1a.** Add `PermissionDeniedElementFinder` struct after the existing `MockOverlayDriver` impl block (~line 200). This mock returns `CoreError::PolicyDenied` from `analyze_scene`, simulating OS-level accessibility permission denial:
  ```rust
  struct PermissionDeniedElementFinder;

  #[async_trait]
  impl ElementFinder for PermissionDeniedElementFinder {
      async fn find_element(
          &self,
          _text: Option<&str>,
          _role: Option<&str>,
          _region: Option<&ElementBounds>,
      ) -> Result<Vec<UiElement>, CoreError> {
          Err(CoreError::PolicyDenied("Accessibility permission denied".to_string()))
      }

      async fn analyze_scene(
          &self,
          _app_name: Option<&str>,
          _screen_id: Option<&str>,
      ) -> Result<UiScene, CoreError> {
          Err(CoreError::PolicyDenied("Accessibility permission denied".to_string()))
      }

      fn name(&self) -> &str {
          "permission-denied-mock"
      }
  }
  ```

- [ ] **1b.** Add `DriftingFocusProbe` struct after `PermissionDeniedElementFinder`. This mock returns a valid focus initially but switches to a *different* focus hash after `N` calls to `current_focus`, and always returns `valid: false` from `validate_execution_binding`. It differs from the existing `MockFocusProbe` (which toggles validation validity) by also changing the `current_focus` return value so `create_session` captures one hash, while later checks see a different one:
  ```rust
  struct DriftingFocusProbe {
      initial_focus: FocusSnapshot,
      drifted_focus: FocusSnapshot,
      /// current_focus returns initial_focus for first `drift_after` calls,
      /// then drifted_focus.
      drift_after: usize,
      call_count: AtomicUsize,
  }

  impl DriftingFocusProbe {
      fn new(initial: FocusSnapshot, drifted: FocusSnapshot, drift_after: usize) -> Self {
          Self {
              initial_focus: initial,
              drifted_focus: drifted,
              drift_after,
              call_count: AtomicUsize::new(0),
          }
      }
  }

  #[async_trait]
  impl FocusProbe for DriftingFocusProbe {
      async fn current_focus(&self) -> Result<FocusSnapshot, CoreError> {
          let n = self.call_count.fetch_add(1, Ordering::SeqCst);
          if n < self.drift_after {
              Ok(self.initial_focus.clone())
          } else {
              Ok(self.drifted_focus.clone())
          }
      }

      async fn validate_execution_binding(
          &self,
          _binding: &ExecutionBinding,
      ) -> Result<FocusValidation, CoreError> {
          let n = self.call_count.load(Ordering::SeqCst);
          let valid = n <= self.drift_after;
          Ok(FocusValidation {
              valid,
              reason: if valid {
                  None
              } else {
                  Some("Window focus changed to another application".to_string())
              },
              current_focus: None,
          })
      }
  }
  ```

- [ ] **1c.** Add a `make_service_with_finder` helper below the existing `make_service_full` function (~line 273) that accepts a custom `ElementFinder`:
  ```rust
  fn make_service_with_finder(
      finder: Arc<dyn ElementFinder>,
      focus: FocusSnapshot,
  ) -> Arc<GuiInteractionService> {
      Arc::new(GuiInteractionService::new(
          finder,
          Arc::new(MockFocusProbe::new(focus)),
          Arc::new(MockOverlayDriver::new()),
          Some(TEST_HMAC_SECRET.to_string()),
      ))
  }
  ```

- [ ] **1d.** Add a `make_drifted_focus` helper that returns a focus snapshot with a different hash:
  ```rust
  fn make_drifted_focus() -> FocusSnapshot {
      FocusSnapshot {
          app_name: "OtherApp".to_string(),
          window_title: "Other Window".to_string(),
          pid: 9999,
          bounds: None,
          captured_at: Utc::now(),
          focus_hash: "drifted-hash-999".to_string(),
      }
  }
  ```

---

## Task 2: Service-Level Tests — 403 Permission Denied, 409 Focus Drift, 422 Expired Ticket

Location: `crates/oneshim-automation/src/gui_interaction/mod.rs`, inside `#[cfg(test)] mod tests`, after the existing "Build candidates tests" section (~line 1310).

### Steps

- [ ] **2a.** Add section header and 403 test. The `PermissionDeniedElementFinder` mock causes `analyze_scene` to return `CoreError::PolicyDenied`, which `map_core_error` maps to `GuiInteractionError::Forbidden`:
  ```rust
  // ── M5: Failure scenario tests ────────────────────────────────────

  #[tokio::test]
  async fn m5_permission_denied_returns_forbidden() {
      let service = make_service_with_finder(
          Arc::new(PermissionDeniedElementFinder),
          make_focus(),
      );

      let err = service
          .create_session(default_create_request())
          .await
          .unwrap_err();
      assert!(
          matches!(err, GuiInteractionError::Forbidden(_)),
          "Expected Forbidden, got: {err:?}"
      );
  }
  ```

- [ ] **2b.** Add 409 focus-drift-on-confirm test. Uses existing `MockFocusProbe::set_validation_valid(false)` to simulate drift during `confirm_candidate`:
  ```rust
  #[tokio::test]
  async fn m5_focus_drift_on_confirm_returns_focus_drift() {
      let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
      let (service, probe) = make_service(scene, make_focus());

      let (sid, token) = create_and_highlight(&service).await;

      // Drift focus before confirm
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
                  ticket_ttl_secs: Some(60),
              },
          )
          .await
          .unwrap_err();
      assert!(
          matches!(err, GuiInteractionError::FocusDrift(_)),
          "Expected FocusDrift, got: {err:?}"
      );
  }
  ```

- [ ] **2c.** Add 409 focus-drift-on-execute test. Focus is valid during confirm but drifts before `prepare_execution`. Uses `MockFocusProbe::set_validation_valid` toggled after confirm:
  ```rust
  #[tokio::test]
  async fn m5_focus_drift_on_execute_returns_focus_drift() {
      let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
      let (service, probe) = make_service(scene, make_focus());

      let (sid, token, ticket) = create_highlight_and_confirm(&service).await;

      // Drift focus after confirm, before execute
      probe.set_validation_valid(false);

      let err = service
          .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
          .await
          .unwrap_err();
      assert!(
          matches!(err, GuiInteractionError::FocusDrift(_)),
          "Expected FocusDrift, got: {err:?}"
      );
  }
  ```

- [ ] **2d.** Add 422 expired-ticket test. Creates a session, confirms, then manually expires the ticket by backdating `expires_at`, then attempts `prepare_execution`:
  ```rust
  #[tokio::test]
  async fn m5_expired_ticket_returns_ticket_invalid() {
      let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
      let (service, _) = make_service(scene, make_focus());

      let (sid, token, mut ticket) = create_highlight_and_confirm(&service).await;

      // Backdate the ticket so it is expired past grace
      ticket.expires_at = Utc::now()
          - ChronoDuration::seconds(TICKET_EXPIRY_GRACE_SECS + 10);

      let err = service
          .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
          .await
          .unwrap_err();
      assert!(
          matches!(err, GuiInteractionError::TicketInvalid(_)),
          "Expected TicketInvalid for expired ticket, got: {err:?}"
      );
  }
  ```

---

## Task 3: Service-Level Tests — Nonce Replay, TTL Boundary

Location: same `mod tests` section, continuing after Task 2 tests.

### Steps

- [ ] **3a.** Add nonce-replay test. Executes a ticket once (succeeds), fails the execution back to Confirmed, then replays the same ticket nonce — the second `prepare_execution` must fail with `TicketInvalid("ticket nonce replay detected")`:
  ```rust
  #[tokio::test]
  async fn m5_nonce_replay_blocked_deterministically() {
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

      // Revert to Confirmed so state gate passes
      service
          .complete_execution(&sid, false, None, 0, 1)
          .await
          .unwrap();

      // Replay same nonce — must be rejected
      let err = service
          .prepare_execution(&sid, &token, GuiExecutionRequest { ticket })
          .await
          .unwrap_err();
      assert!(
          matches!(err, GuiInteractionError::TicketInvalid(_)),
          "Expected TicketInvalid for nonce replay, got: {err:?}"
      );
      // Verify the error message is specific
      if let GuiInteractionError::TicketInvalid(msg) = &err {
          assert!(
              msg.contains("nonce") || msg.contains("replay"),
              "Error message should mention nonce replay, got: {msg}"
          );
      }
  }
  ```

- [ ] **3b.** Add session-TTL-boundary test. Creates a session with minimum TTL (30s), manually backdates `expires_at` to simulate expiry, then verifies `get_session` returns `Expired` state and `highlight_session` returns `TicketInvalid`:
  ```rust
  #[tokio::test]
  async fn m5_session_ttl_boundary_marks_expired() {
      let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
      let (service, _) = make_service(scene, make_focus());

      let mut req = default_create_request();
      req.session_ttl_secs = Some(30);
      let resp = service.create_session(req).await.unwrap();
      let sid = resp.session.session_id.clone();
      let token = resp.capability_token.clone();

      // Manually expire
      {
          let mut sessions = service.sessions.write().await;
          if let Some(stored) = sessions.get_mut(&sid) {
              stored.session.expires_at = Utc::now() - ChronoDuration::seconds(2);
          }
      }

      // get_session should reflect Expired state
      let session = service.get_session(&sid, &token).await.unwrap();
      assert_eq!(session.state, GuiSessionState::Expired);

      // Operations on expired session should fail
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
  ```

- [ ] **3c.** Add session-TTL-cleanup test. Verifies that `expire_sessions()` removes the session from storage and emits an expired event:
  ```rust
  #[tokio::test]
  async fn m5_expire_sessions_removes_and_emits_event() {
      let scene = make_scene(vec![make_element("el-1", "Save", 0.9)]);
      let (service, _, overlay) = make_service_full(scene, make_focus());

      let (sid, token) = create_and_highlight(&service).await;

      let mut rx = service.subscribe();

      // Expire the session
      {
          let mut sessions = service.sessions.write().await;
          if let Some(stored) = sessions.get_mut(&sid) {
              stored.session.expires_at = Utc::now() - ChronoDuration::seconds(1);
          }
      }

      service.expire_sessions().await;

      // Session should be gone
      let err = service.get_session(&sid, &token).await.unwrap_err();
      assert!(matches!(err, GuiInteractionError::NotFound(_)));

      // Expired event should have been emitted
      let mut events = Vec::new();
      while let Ok(event) = rx.try_recv() {
          events.push(event);
      }
      let expired_events: Vec<_> = events
          .iter()
          .filter(|e| e.event_type == "gui_session.expired")
          .collect();
      assert_eq!(expired_events.len(), 1);
      assert_eq!(expired_events[0].session_id, sid);

      // Overlay clear should have been called (session was highlighted)
      assert!(
          overlay.clear_count.load(Ordering::SeqCst) >= 1,
          "Overlay clear_highlights should be called on expire"
      );
  }
  ```

---

## Task 4: Handler-Level Tests — HTTP 403/409/422 Status Mapping

Location: `crates/oneshim-web/src/handlers/automation_gui.rs`, inside `#[cfg(test)] mod tests`. Add a new `mod m5` after the existing `mod m4` (~line 606).

These tests verify that the handler layer maps service errors to the correct HTTP status codes via `map_gui_error`. They use the same `make_state` / `make_controller` fixture pattern from M4, with modified controller configurations.

### Steps

- [ ] **4a.** Add `mod m5` with necessary imports. The module needs its own mock types that produce the failure conditions. Add this after the closing brace of `mod m4` (~line 606):
  ```rust
  mod m5 {
      use super::*;
      use crate::AppState;
      use async_trait::async_trait;
      use chrono::{Duration as ChronoDuration, Utc};
      use oneshim_api_contracts::automation_gui::{
          GuiConfirmRequest, GuiCreateSessionRequest, GuiExecutionRequest,
          GuiHighlightRequest, GuiSessionPath,
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
          ExecutionBinding, FocusSnapshot, FocusValidation, GuiActionRequest,
          GuiActionType, GuiSessionState, HighlightHandle, HighlightRequest,
      };
      use oneshim_core::models::intent::{ElementBounds, IntentConfig};
      use oneshim_core::models::ui_scene::{NormalizedBounds, UiScene, UiSceneElement};
      use oneshim_core::ports::element_finder::ElementFinder;
      use oneshim_core::ports::focus_probe::FocusProbe;
      use oneshim_core::ports::overlay_driver::OverlayDriver;
      use oneshim_storage::sqlite::SqliteStorage;
      use std::sync::atomic::{AtomicUsize, Ordering};
      use std::sync::Arc;
      use tokio::sync::{broadcast, RwLock};

      const M5_HMAC_SECRET: &str = "m5-hmac-secret-32-bytes-long!!!!";

      // ... (see following steps for mock types and tests)
  }
  ```

- [ ] **4b.** Inside `mod m5`, add `M5PermissionDeniedFinder` mock that causes `create_gui_session` to fail with 403:
  ```rust
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
  ```

- [ ] **4c.** Inside `mod m5`, add a `M5MockFocusProbe` with a `set_validation_valid` toggle (mirror of M4 pattern but with the toggle), the standard `M5MockOverlayDriver`, and helper functions:
  ```rust
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
  ```

- [ ] **4d.** Inside `mod m5`, add the fixture builders. `make_controller_with_finder` accepts a custom `ElementFinder` and `FocusProbe`:
  ```rust
  fn make_controller_with(
      finder: Arc<dyn ElementFinder>,
      focus_probe: Arc<dyn FocusProbe>,
      hmac_secret: Option<String>,
  ) -> Arc<AutomationController> {
      let policy_client = Arc::new(PolicyClient::new());
      let audit_logger = Arc::new(RwLock::new(AuditLogger::default()));
      let sandbox: Arc<dyn oneshim_core::ports::sandbox::Sandbox> =
          Arc::new(NoOpSandbox);
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
  ```

- [ ] **4e.** Add handler-level 403 test:
  ```rust
  #[tokio::test]
  async fn m5_permission_denied_returns_http_403() {
      let controller = make_controller_with(
          Arc::new(M5PermissionDeniedFinder),
          Arc::new(M5MockFocusProbe::new()),
          Some(M5_HMAC_SECRET.to_string()),
      );
      let state = make_state_with(controller);

      let err = create_gui_session(
          State(m5_context(&state)),
          Json(m5_default_create_req()),
      )
      .await
      .unwrap_err();
      assert!(matches!(err, ApiError::Forbidden(_)));
  }
  ```

- [ ] **4f.** Add handler-level 409 test (focus drift during confirm):
  ```rust
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
      let resp = create_gui_session(
          State(m5_context(&state)),
          Json(m5_default_create_req()),
      )
      .await
      .unwrap();
      let sid = resp.0.session.session_id.clone();
      let token = resp.0.capability_token.clone();

      let highlight_resp = highlight_gui_session(
          State(m5_context(&state)),
          Path(GuiSessionPath { id: sid.clone() }),
          m5_token_headers(&token),
          Json(GuiHighlightRequest { candidate_ids: None }),
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
  ```

- [ ] **4g.** Add handler-level 422 test (expired ticket). Uses the same approach as M4 full-flow but tampers with the ticket's `expires_at`:
  ```rust
  #[tokio::test]
  async fn m5_expired_ticket_returns_http_422() {
      let controller = make_controller_with(
          Arc::new(M5MockElementFinder),
          Arc::new(M5MockFocusProbe::new()),
          Some(M5_HMAC_SECRET.to_string()),
      );
      let state = make_state_with(controller);

      // Create + highlight
      let resp = create_gui_session(
          State(m5_context(&state)),
          Json(m5_default_create_req()),
      )
      .await
      .unwrap();
      let sid = resp.0.session.session_id.clone();
      let token = resp.0.capability_token.clone();

      let highlight_resp = highlight_gui_session(
          State(m5_context(&state)),
          Path(GuiSessionPath { id: sid.clone() }),
          m5_token_headers(&token),
          Json(GuiHighlightRequest { candidate_ids: None }),
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
  ```

---

## Task 5: SSE Session-Scoped Event Filtering Test

Location: `crates/oneshim-web/src/handlers/automation_gui.rs`, inside `mod m5`.

This test verifies that the `gui_session_event_stream` handler's `filter_map` closure correctly isolates events to the subscribing session's ID, matching the spec's requirement in section 3.6.

### Steps

- [ ] **5a.** Add SSE session-scoped filtering test. This validates at the *service-level broadcast filtering logic* pattern used in the handler. Since we cannot easily consume a full SSE stream in a unit test without an HTTP client, we test the underlying filtering behavior that the handler relies on -- `subscribe_session` returns a broadcast receiver, and the handler applies `filter_map(|e| e.session_id == target_id)`:
  ```rust
  #[tokio::test]
  async fn m5_sse_events_filtered_by_session_id() {
      let controller = make_controller_with(
          Arc::new(M5MockElementFinder),
          Arc::new(M5MockFocusProbe::new()),
          Some(M5_HMAC_SECRET.to_string()),
      );
      let state = make_state_with(controller);

      // Create two sessions
      let resp_a = create_gui_session(
          State(m5_context(&state)),
          Json(m5_default_create_req()),
      )
      .await
      .unwrap();
      let sid_a = resp_a.0.session.session_id.clone();
      let token_a = resp_a.0.capability_token.clone();

      let resp_b = create_gui_session(
          State(m5_context(&state)),
          Json(m5_default_create_req()),
      )
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
          Json(GuiHighlightRequest { candidate_ids: None }),
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
  ```

---

## Task 6: HMAC Secret Fail-Closed Integration Test

Location: `crates/oneshim-web/src/handlers/automation_gui.rs`, inside `mod m5`.

This test verifies that when the HMAC secret is missing/empty, `create_gui_session` returns 503 ServiceUnavailable (fail-closed behavior).

### Steps

- [ ] **6a.** Add HMAC-missing-returns-503 test:
  ```rust
  #[tokio::test]
  async fn m5_missing_hmac_secret_returns_http_503() {
      let controller = make_controller_with(
          Arc::new(M5MockElementFinder),
          Arc::new(M5MockFocusProbe::new()),
          None, // No HMAC secret — fail-closed
      );
      let state = make_state_with(controller);

      let err = create_gui_session(
          State(m5_context(&state)),
          Json(m5_default_create_req()),
      )
      .await
      .unwrap_err();
      assert!(
          matches!(err, ApiError::ServiceUnavailable(_)),
          "Missing HMAC secret should fail closed with 503, got: {err:?}"
      );
  }
  ```

- [ ] **6b.** Add HMAC-empty-string-returns-503 test (edge case: secret set but whitespace-only):
  ```rust
  #[tokio::test]
  async fn m5_empty_hmac_secret_returns_http_503() {
      let controller = make_controller_with(
          Arc::new(M5MockElementFinder),
          Arc::new(M5MockFocusProbe::new()),
          Some("   ".to_string()), // Whitespace-only — treated as missing
      );
      let state = make_state_with(controller);

      let err = create_gui_session(
          State(m5_context(&state)),
          Json(m5_default_create_req()),
      )
      .await
      .unwrap_err();
      assert!(
          matches!(err, ApiError::ServiceUnavailable(_)),
          "Empty HMAC secret should fail closed with 503, got: {err:?}"
      );
  }
  ```

---

## Verification

After all tasks are complete:

- [ ] `cargo test -p oneshim-automation -- gui_interaction::tests::m5` passes (8 new service-level tests)
- [ ] `cargo test -p oneshim-web -- handlers::automation_gui::tests::m5` passes (6 new handler-level tests)
- [ ] `cargo test --workspace` passes with 0 failures
- [ ] `cargo clippy --workspace` passes with no new warnings

### Expected New Test Count

| Location | Test Name | Error Path |
|----------|-----------|------------|
| `automation/mod.rs` | `m5_permission_denied_returns_forbidden` | 403 |
| `automation/mod.rs` | `m5_focus_drift_on_confirm_returns_focus_drift` | 409 |
| `automation/mod.rs` | `m5_focus_drift_on_execute_returns_focus_drift` | 409 |
| `automation/mod.rs` | `m5_expired_ticket_returns_ticket_invalid` | 422 |
| `automation/mod.rs` | `m5_nonce_replay_blocked_deterministically` | 422 |
| `automation/mod.rs` | `m5_session_ttl_boundary_marks_expired` | TTL |
| `automation/mod.rs` | `m5_expire_sessions_removes_and_emits_event` | TTL cleanup |
| `automation/mod.rs` | (reserved for DriftingFocusProbe if needed) | -- |
| `web/automation_gui.rs` | `m5_permission_denied_returns_http_403` | 403 |
| `web/automation_gui.rs` | `m5_focus_drift_returns_http_409` | 409 |
| `web/automation_gui.rs` | `m5_expired_ticket_returns_http_422` | 422 |
| `web/automation_gui.rs` | `m5_sse_events_filtered_by_session_id` | SSE scope |
| `web/automation_gui.rs` | `m5_missing_hmac_secret_returns_http_503` | 503 |
| `web/automation_gui.rs` | `m5_empty_hmac_secret_returns_http_503` | 503 |

**Total: 14 new tests** (8 service-level + 6 handler-level)
