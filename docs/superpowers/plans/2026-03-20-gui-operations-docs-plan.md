# GUI V2 Operations Documentation — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create operator-facing documentation for ADR-002 GUI V2 (troubleshooting runbook, API contract examples, security review) and wire the GUI event broadcast channel into `AuditLogger` so every state transition produces an audit entry.

**Architecture:** Tasks 1-3 are pure documentation. Task 4 is a code change: subscribe to `GuiInteractionService.subscribe()` (tokio broadcast) in `src-tauri` and forward each `GuiSessionEvent` to `AuditLogger::log_event()`. Task 5 adds a test that exercises the full create-highlight-confirm-execute flow and asserts audit entries exist for every state transition.

**Tech Stack:** Rust, oneshim-automation (audit.rs, gui_interaction/), oneshim-core (models/gui.rs), src-tauri (web_server_runtime.rs, automation_controller_builder.rs)

**Prerequisite:** Native Platform Adapters spec (ADR-002 GUI V2 endpoints already implemented).

---

## File Map

### Task 1 — Troubleshooting runbook

| File | Change |
|------|--------|
| `docs/guides/adr-002-gui-troubleshooting-runbook.md` | New file |

### Task 2 — API contract examples

| File | Change |
|------|--------|
| `docs/contracts/gui-interaction-v2-examples.md` | New file |

### Task 3 — Security review document

| File | Change |
|------|--------|
| `docs/security/adr-002-gui-security-review.md` | New file |

### Task 4 — Audit logger integration (code)

| File | Change |
|------|--------|
| `src-tauri/src/web_server_runtime.rs` | Spawn audit-forwarding task that subscribes to `GuiInteractionService.subscribe()` and calls `AuditLogger::log_event()` |

### Task 5 — Audit logger test

| File | Change |
|------|--------|
| `crates/oneshim-automation/src/gui_interaction/service.rs` | Add test in `#[cfg(test)] mod tests` that exercises state transitions and asserts broadcast events are received |
| `crates/oneshim-automation/src/audit.rs` | Add test that feeds `GuiSessionEvent` data into `AuditLogger` and verifies entries |

---

## Task 1: Write troubleshooting runbook

**Why:** Operators need a single reference for diagnosing GUI V2 permission and runtime failures across macOS, Windows, and Linux. The spec defines the exact sections and failure signatures.

**File:** `docs/guides/adr-002-gui-troubleshooting-runbook.md` (new)

- [ ] **Step 1.1: Create runbook file**

Write `docs/guides/adr-002-gui-troubleshooting-runbook.md` with the following structure (use the style from `docs/guides/automation-playbook-templates.md` -- short, table-driven, actionable):

```
# ADR-002 GUI Troubleshooting Runbook

## Prerequisites
- HMAC secret: `ONESHIM_GUI_TICKET_HMAC_SECRET` env var
- OS accessibility permissions granted

## macOS Permission Flow
System Preferences → Privacy & Security → Accessibility → grant ONESHIM
Reset command: `tccutil reset Accessibility com.oneshim.app`

## Windows Permission Flow
UIA works without elevation by default.
For protected apps (admin processes): run ONESHIM elevated, or use Accessibility Insights.
Check Event Viewer → Windows Logs → Application for UIA errors.

## Linux Permission Flow
1. Verify AT-SPI: `busctl --user introspect org.a11y.Bus /org/a11y/bus`
2. Enable if missing: `gsettings set org.gnome.desktop.interface toolkit-accessibility true`
3. Verify DBUS_SESSION_BUS_ADDRESS is set

## Common Failure Signatures
| Symptom | Cause | Action |
|---------|-------|--------|
| 503 on session create | Missing HMAC secret | Set ONESHIM_GUI_TICKET_HMAC_SECRET |
| 503 on session create | GUI feature disabled | Enable in config |
| 403 on scene analysis | Accessibility denied | Grant OS permission (see above) |
| 409 on confirm/execute | Focus changed | Retry from propose step |
| 422 on execute | Ticket expired (>30s) | Re-confirm candidate |
| 422 on execute | Nonce replay | Create new session |
| Empty element list | OCR+Accessibility both failed | Check screen capture permissions |

## Diagnostic Commands
- macOS: `tccutil reset Accessibility com.oneshim.app`
- Linux: `busctl --user introspect org.a11y.Bus /org/a11y/bus`
- Windows: Event Viewer → Windows Logs → Application (filter UIA)

## Log Level Guide
RUST_LOG=oneshim_automation=debug cargo run -p oneshim-app
```

All content is defined in the spec (section 2.1). Adapt formatting to match the concise, table-driven style of existing guides.

- [ ] **Step 1.2: Verify**

Confirm the document covers all 3 OS permission flows, all 7 failure signatures from the spec, and diagnostic commands.

- [ ] **Step 1.3: Commit**

```
git add docs/guides/adr-002-gui-troubleshooting-runbook.md
git commit -m "docs(guides): add ADR-002 GUI V2 troubleshooting runbook"
```

---

## Task 2: Write API contract examples

**Why:** The existing `docs/contracts/gui-interaction-contract.md` defines schemas but lacks concrete request/response examples. The spec requires cURL examples for all 7 endpoints with success and error cases.

**File:** `docs/contracts/gui-interaction-v2-examples.md` (new)

- [ ] **Step 2.1: Create examples file**

Write `docs/contracts/gui-interaction-v2-examples.md` referencing the schema definitions in `gui-interaction-contract.md`. For each of the 7 endpoints provide:

1. **cURL command** with realistic values (localhost:10090 base URL)
2. **Request body JSON** (where applicable)
3. **Success response JSON** (200)
4. **Error response JSON** for each applicable error code

Endpoints to cover (from `gui-interaction-contract.md`):

**1. POST /api/automation/gui/sessions** (create)
- Success 200: `GuiCreateSessionResponse` with `capability_token`, session state `proposed`, candidate list
- Error 503: Missing HMAC secret / GUI disabled
- No auth header required

**2. GET /api/automation/gui/sessions/{id}** (read)
- Header: `x-gui-session-token: {token}`
- Success 200: `GuiSessionResponse`
- Error 401: Missing/invalid token
- Error 404: Session not found

**3. POST /api/automation/gui/sessions/{id}/highlight**
- Header: `x-gui-session-token: {token}`
- Request: `{"candidate_ids": ["elem-001", "elem-003"]}` or `{"candidate_ids": null}`
- Success 200: `GuiSessionResponse` with state `highlighted`
- Error 404, 401

**4. POST /api/automation/gui/sessions/{id}/confirm**
- Header: `x-gui-session-token: {token}`
- Request: `{"candidate_id": "elem-001", "action": {"action_type": "click"}, "ticket_ttl_secs": 30}`
- Success 200: `GuiConfirmResponse` with `GuiExecutionTicket`
- Error 400: Missing candidate_id
- Error 404: Session or candidate not found

**5. POST /api/automation/gui/sessions/{id}/execute**
- Header: `x-gui-session-token: {token}`
- Request: `{"ticket": {...ticket object from confirm...}}`
- Success 200: `GuiExecuteResponse` with `IntentResult`
- Error 409: Focus drift
- Error 422: Ticket expired or nonce replay

**6. DELETE /api/automation/gui/sessions/{id}**
- Header: `x-gui-session-token: {token}`
- Success 200: `GuiSessionResponse` with state `cancelled`
- Error 404

**7. GET /api/automation/gui/sessions/{id}/events** (SSE)
- Header: `x-gui-session-token: {token}`
- SSE event format with `event:` and `data:` lines
- Keep-alive ping every 15s

Use realistic UUIDs, ISO 8601 timestamps, and field values from the contract spec models (`GuiInteractionSession`, `FocusSnapshot`, `GuiCandidate`, `GuiExecutionTicket`, `IntentResult`). Reference the error mapping table from the contract spec.

- [ ] **Step 2.2: Verify**

Confirm all 7 endpoints are covered with at least one success and one error example each.

- [ ] **Step 2.3: Commit**

```
git add docs/contracts/gui-interaction-v2-examples.md
git commit -m "docs(contracts): add GUI V2 API contract examples with cURL commands"
```

---

## Task 3: Write security review document

**Why:** The spec requires a formal security review covering 6 threat categories with mitigations, verification status, and residual risk analysis.

**File:** `docs/security/adr-002-gui-security-review.md` (new)

- [ ] **Step 3.1: Create security review file**

Write `docs/security/adr-002-gui-security-review.md` following the style of `docs/security/standalone-integrity-baseline.md` (objective-driven, structured, checklist-based). Sections:

**Threat Model:**
- Scope: local-only HTTP API (localhost:10090), no network exposure
- Attacker model: malicious local process with knowledge of the API

**Mitigations Table:**

| Threat | Mitigation | Implementation | Verified |
|--------|-----------|----------------|----------|
| Session hijack | Random UUID capability token per session | `GuiInteractionService::new()` generates via `new_capability_token()` in `crypto.rs` | Pending |
| Ticket replay | Single-use nonce per ticket, tracked in `HashSet` | `prepare_execution()` checks `used_nonces` in `StoredSession` | Pending |
| Ticket forgery | HMAC-SHA256 signature on `session_id\|scene_id\|element_id\|action_hash\|focus_hash\|nonce` | `sign_ticket()` / `verify_ticket()` in `crypto.rs` | Pending |
| Focus spoofing | Atomic revalidation: re-capture focus at execute time, compare `focus_hash` | `validate_execution_binding()` in `service.rs` | Pending |
| Stale sessions | TTL-based cleanup every 30s (default session TTL: 300s) | `expire_sessions()` spawned by `ensure_cleanup_task()` | Pending |
| HMAC key leak | Env var only (`ONESHIM_GUI_TICKET_HMAC_SECRET`), never logged, fail-closed on missing | `require_hmac_secret()` returns `Unavailable` | Pending |

**Verification Checklist:**
- Each row gets a manual verification checkbox referencing the source file and line
- Code references: `crates/oneshim-automation/src/gui_interaction/crypto.rs`, `service.rs`, `helpers.rs`

**Residual Risks:**
- Local-only attack surface: agent runs as the logged-in user; any process running as the same user can call the API
- No TLS on localhost (acceptable for local-only)
- Overlay rendering trusts the OS compositor

- [ ] **Step 3.2: Verify**

Confirm all 6 threat categories from the spec are covered, each with mitigation, implementation pointer, and verification status.

- [ ] **Step 3.3: Commit**

```
git add docs/security/adr-002-gui-security-review.md
git commit -m "docs(security): add ADR-002 GUI V2 security review with threat model"
```

---

## Task 4: Implement audit logger integration

**Why:** The spec requires that every GUI session state transition, denied path, and ticket operation produces an audit entry. `GuiInteractionService` already publishes `GuiSessionEvent` via `tokio::broadcast`. `AuditLogger` already has `log_event()`, `log_denied()`, `log_start()` methods. The missing piece is a subscriber task that bridges the two.

**Files:**
- Modify: `src-tauri/src/web_server_runtime.rs`

- [ ] **Step 4.1: Add GUI audit forwarding task in `web_server_runtime.rs`**

In `src-tauri/src/web_server_runtime.rs`, after the `AutomationController` and web `AuditLogger` are constructed (around the existing `web_audit_logger` setup near line 257-293), add a function that spawns a background task:

```rust
/// Subscribes to GUI session events and forwards them to the audit logger.
fn spawn_gui_audit_forwarder(
    automation_controller: &Arc<AutomationController>,
    audit_logger: Arc<tokio::sync::RwLock<AuditLogger>>,
) {
    // Access the gui_service from the controller to get a broadcast receiver
    let Some(gui_service) = automation_controller.gui_service() else {
        tracing::debug!("GUI service not configured; skipping audit forwarder");
        return;
    };

    let mut rx = gui_service.subscribe();

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let action_type = format!("gui.session.{}", event.event_type);
                    let details = event.message.unwrap_or_default();
                    let mut logger = audit_logger.write().await;
                    logger.log_event(&action_type, &event.session_id, &details);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("GUI audit forwarder lagged by {} events", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!("GUI event channel closed; audit forwarder exiting");
                    break;
                }
            }
        }
    });
}
```

**Note:** `AutomationController` currently stores `gui_service` as `pub(super)`. If there is no public accessor `gui_service()`, add one:

In `crates/oneshim-automation/src/controller/mod.rs`, add:

```rust
/// Returns a reference to the GUI interaction service, if configured.
pub fn gui_service(&self) -> Option<&Arc<GuiInteractionService>> {
    self.gui_service.as_ref()
}
```

Then call `spawn_gui_audit_forwarder()` in `web_server_runtime.rs` after the automation controller is built and the web audit logger exists. Find the location where `automation_controller` and `web_audit_logger` are both available (near line 280-293) and add:

```rust
if let Some(ref controller) = runtime_context.automation_controller {
    spawn_gui_audit_forwarder(controller, web_audit_logger.clone());
}
```

- [ ] **Step 4.2: Verify compilation**

```bash
cargo check -p oneshim-automation
cargo check --workspace
```

- [ ] **Step 4.3: Commit**

```
git add src-tauri/src/web_server_runtime.rs crates/oneshim-automation/src/controller/mod.rs
git commit -m "feat(automation): wire GUI session events to AuditLogger via broadcast subscriber"
```

---

## Task 5: Add audit logger test

**Why:** The spec acceptance criteria require that audit entries are verified for all state transitions. This test exercises the `GuiInteractionService` through create/highlight/confirm/execute and asserts that the broadcast produces events that, when forwarded to `AuditLogger`, produce the expected entries.

**File:** `crates/oneshim-automation/src/audit.rs`

- [ ] **Step 5.1: Add integration test in `audit.rs`**

In `crates/oneshim-automation/src/audit.rs`, add a test in the existing `#[cfg(test)] mod tests` block. The test should:

1. Create an `AuditLogger`
2. Simulate the GUI event stream by calling `log_event()` for each state transition that `publish_event()` emits: `proposed`, `highlighted`, `confirmed`, `executing`, `executed`
3. Also simulate denied paths: `log_denied()` for a 403 (accessibility denied)
4. Also simulate ticket operations: `log_event()` for `gui.ticket.signed`, `gui.ticket.verified`, `log_denied()` for `gui.ticket.replay_rejected`
5. Assert `pending_count()` matches the expected number of entries
6. Assert `entries_by_status(&AuditStatus::Completed, ...)` contains the state transition entries
7. Assert `entries_by_status(&AuditStatus::Denied, ...)` contains the denied entries
8. Assert `stats()` totals are correct

```rust
#[test]
fn gui_state_transitions_emit_audit_entries() {
    let mut logger = AuditLogger::new(100, 50);
    let session_id = "gui-sess-001";

    // State transitions (forwarded from GuiSessionEvent broadcast)
    logger.log_event("gui.session.proposed", session_id, "Session created");
    logger.log_event("gui.session.highlighted", session_id, "3 candidates highlighted");
    logger.log_event("gui.session.confirmed", session_id, "Element elem-001 confirmed");
    logger.log_event("gui.session.executing", session_id, "Executing click on elem-001");
    logger.log_event("gui.session.executed", session_id, "Action completed successfully");

    // Denied path
    logger.log_denied("gui-deny-001", session_id, "gui.accessibility_denied");

    // Ticket operations
    logger.log_event("gui.ticket.signed", session_id, "Ticket ticket-001 issued");
    logger.log_event("gui.ticket.verified", session_id, "Ticket ticket-001 verified");
    logger.log_denied("gui-deny-002", session_id, "gui.ticket.replay_rejected");

    assert_eq!(logger.pending_count(), 9);

    let completed = logger.entries_by_status(&AuditStatus::Completed, 20);
    assert_eq!(completed.len(), 7); // 5 state transitions + 2 ticket ops

    let denied = logger.entries_by_status(&AuditStatus::Denied, 20);
    assert_eq!(denied.len(), 2); // accessibility + replay

    let stats = logger.stats();
    assert_eq!(stats.completed, 7);
    assert_eq!(stats.denied, 2);
    assert_eq!(stats.total, 9);
}
```

- [ ] **Step 5.2: Verify test passes**

```bash
cargo test -p oneshim-automation -- audit::tests::gui_state_transitions
```

- [ ] **Step 5.3: Commit**

```
git add crates/oneshim-automation/src/audit.rs
git commit -m "test(automation): verify GUI state transitions emit audit entries"
```

---

## Verification

```bash
# All code compiles
cargo check --workspace

# All automation tests pass
cargo test -p oneshim-automation

# Lint clean
cargo clippy --workspace -- -D warnings

# Format clean
cargo fmt --check
```

---

## Execution Order

```
Task 1 (runbook)              — independent, can run in parallel
Task 2 (API examples)         — independent, can run in parallel
Task 3 (security review)      — independent, can run in parallel
Task 4 (audit integration)    — code change, depends on understanding service.rs
Task 5 (audit test)           — depends on Task 4 (same crate, tests the integration pattern)
```

Recommended: Tasks 1-3 in parallel, then Task 4, then Task 5.
