# GUI V2 Failure Scenario Tests — Design Spec

> Created: 2026-03-20
> Status: Proposed
> Scope: oneshim-automation (service tests), oneshim-web (handler tests)
> Prerequisite: ADR-002 GUI V2 M0-M2 tests complete

## 1. Goal

Add comprehensive failure path test coverage for all HTTP error codes (403/409/422/503) in the GUI V2 interaction flow. Verify nonce replay protection, session TTL boundaries, and SSE session-scoped event filtering.

## 2. Current Test Coverage

### What's Tested (M4 suite in automation_gui.rs)
- 401 Unauthorized: 5 token validation tests ✅
- 503 ServiceUnavailable: no controller test ✅
- Happy path: create → get → highlight → confirm → execute → delete ✅
- Two concurrent sessions independent ✅
- Error mapping (7 tests) ✅

### What's Missing
- 403 Forbidden (permission/policy denied) — ZERO tests
- 409 Conflict (focus drift during confirm/execute) — ZERO tests
- 422 Unprocessable (expired/invalid ticket, nonce replay) — ZERO tests
- Session TTL boundary (expiry, cleanup) — ZERO tests
- SSE session-scoped event filtering — ZERO tests
- HMAC secret missing fail-closed — ZERO integration tests

## 3. Test Design

### 3.1 Permission Denied (403 Forbidden)

Test the path where accessibility permission is denied by the OS:

```rust
#[tokio::test]
async fn m5_permission_denied_returns_403() {
    // Setup: GuiInteractionService with a mock ElementFinder that returns PermissionDenied
    // Action: POST /api/automation/gui/sessions
    // Assert: 403 Forbidden response
}
```

### 3.2 Focus Drift (409 Conflict)

Test the path where the window focus changes between confirm and execute:

```rust
#[tokio::test]
async fn m5_focus_drift_on_confirm_returns_409() {
    // Setup: Create session → highlight → change mock focus → confirm
    // Assert: 409 Conflict with "focus drift" message
}

#[tokio::test]
async fn m5_focus_drift_on_execute_returns_409() {
    // Setup: Create → highlight → confirm → change mock focus → execute
    // Assert: 409 Conflict
}
```

### 3.3 Expired/Invalid Ticket (422 Unprocessable)

```rust
#[tokio::test]
async fn m5_expired_ticket_returns_422() {
    // Setup: Create → confirm with 1s TTL → wait 2s → execute
    // Assert: 422 Unprocessable "ticket expired"
}

#[tokio::test]
async fn m5_invalid_ticket_signature_returns_422() {
    // Setup: Create → confirm → tamper with ticket signature → execute
    // Assert: 422 Unprocessable "invalid signature"
}
```

### 3.4 Nonce Replay Protection (422)

```rust
#[tokio::test]
async fn m5_nonce_replay_returns_422() {
    // Setup: Create → confirm → execute (success) → execute same ticket again
    // Assert: 422 Unprocessable "nonce replay detected"
}
```

### 3.5 Session TTL Boundary

```rust
#[tokio::test]
async fn m5_session_expires_after_ttl() {
    // Setup: Create session with 1s TTL → wait 2s
    // Assert: GET session returns 404 (cleaned up)
}

#[tokio::test]
async fn m5_overlay_cleared_on_session_expire() {
    // Setup: Create → highlight → wait for TTL
    // Assert: Overlay clear_highlights() called
}
```

### 3.6 SSE Session-Scoped Filtering

```rust
#[tokio::test]
async fn m5_sse_events_scoped_to_session() {
    // Setup: Create session A and session B
    // Action: Highlight session A
    // Assert: SSE stream for session B receives NO events
    // Assert: SSE stream for session A receives Highlighted event
}
```

### 3.7 HMAC Secret Fail-Closed

```rust
#[tokio::test]
async fn m5_missing_hmac_secret_returns_503() {
    // Setup: No ONESHIM_GUI_TICKET_HMAC_SECRET env var
    // Action: POST /api/automation/gui/sessions
    // Assert: 503 ServiceUnavailable
}
```

## 4. Mock Strategy

Use existing mock patterns from M4 tests:
- `MockElementFinder` — returns configurable UiScene/elements
- `MockFocusProbe` — returns configurable FocusSnapshot, can simulate drift
- `MockOverlayDriver` — records calls for assertion
- `MockInputDriver` — no-op
- New: `DriftingFocusProbe` — returns different focus after N calls (for 409 tests)
- New: `PermissionDeniedElementFinder` — always returns CoreError::PermissionDenied

## 5. File Locations

| Test Suite | File | New Tests |
|------------|------|-----------|
| Service-level | `crates/oneshim-automation/src/gui_interaction/mod.rs` | 403, 409, 422, TTL, nonce |
| Handler-level | `crates/oneshim-web/src/handlers/automation_gui.rs` | HTTP status mapping |
| SSE filtering | `crates/oneshim-web/src/handlers/automation_gui.rs` | Event scoping |

## 6. Acceptance Criteria

- All 7 error paths tested at both service and handler level
- Nonce replay blocked deterministically
- Session TTL cleanup verified within 2× TTL tolerance
- SSE events isolated per session
- Every denied path emits traceable log entry (tracing::warn or error)
- `cargo test --workspace` passes with 0 failures
