# GUI V2 Operations Documentation — Design Spec

> Created: 2026-03-20
> Status: Proposed
> Scope: docs/guides, docs/contracts, docs/security
> Prerequisite: Native Platform Adapters spec

## 1. Goal

Create operator-facing documentation for ADR-002 GUI V2: troubleshooting runbook, API contract examples, security review, and audit logger integration.

## 2. Documents to Create

### 2.1 Operator Troubleshooting Runbook
**File**: `docs/guides/adr-002-gui-troubleshooting-runbook.md`

Sections:
- **Prerequisites**: HMAC secret setup, accessibility permissions per OS
- **macOS Permission Flow**: System Preferences → Privacy & Security → Accessibility → grant to ONESHIM
- **Windows Permission Flow**: UIA generally works without elevation; troubleshooting for protected apps
- **Linux Permission Flow**: AT-SPI daemon check, enable AT-SPI, DBUS_SESSION_BUS_ADDRESS verification
- **Common Failure Signatures**:
  | Symptom | Cause | Action |
  |---------|-------|--------|
  | 503 on session create | Missing HMAC secret | Set `ONESHIM_GUI_TICKET_HMAC_SECRET` env var |
  | 503 on session create | GUI feature disabled | Enable in config |
  | 403 on scene analysis | Accessibility permission denied | Grant OS permission |
  | 409 on confirm/execute | Focus changed during session | Retry from propose step |
  | 422 on execute | Ticket expired (>30s) | Re-confirm candidate |
  | 422 on execute | Nonce replay | Create new session |
  | Empty element list | OCR/Accessibility both failed | Check screen capture permissions |
- **Diagnostic Commands**:
  - macOS: `tccutil reset Accessibility com.oneshim.app`
  - Linux: `busctl --user introspect org.a11y.Bus /org/a11y/bus`
  - Windows: check Event Viewer for UIA errors
- **Log Level Guide**: `RUST_LOG=oneshim_automation=debug` for GUI session tracing

### 2.2 API Contract Examples
**File**: `docs/contracts/gui-interaction-v2-examples.md`

For each of the 7 V2 endpoints, provide:
- cURL command example
- Request body JSON (where applicable)
- Response body JSON (success case)
- Error response JSON (for each applicable error code)
- Required headers (X-Gui-Session-Token)

Endpoints:
1. POST /api/automation/gui/sessions (create)
2. GET /api/automation/gui/sessions/{id} (read)
3. POST /api/automation/gui/sessions/{id}/highlight
4. POST /api/automation/gui/sessions/{id}/confirm
5. POST /api/automation/gui/sessions/{id}/execute
6. DELETE /api/automation/gui/sessions/{id}
7. GET /api/automation/gui/sessions/{id}/events (SSE)

### 2.3 Security Review Document
**File**: `docs/security/adr-002-gui-security-review.md`

Sections:
- **Threat Model**: Session hijack, ticket replay, focus spoofing, overlay manipulation
- **Mitigations**:
  | Threat | Mitigation | Implementation |
  |--------|-----------|----------------|
  | Session hijack | Random UUID capability token per session | `create_session()` generates token |
  | Ticket replay | Single-use nonce per ticket, tracked in HashSet | `prepare_execution()` checks |
  | Ticket forgery | HMAC-SHA256 signature on session+scene+element+focus | `crypto.rs` |
  | Focus spoofing | Atomic revalidation at execute time | `validate_execution_binding()` |
  | Stale sessions | TTL-based cleanup (default 300s) | `expire_sessions()` task |
  | HMAC key leak | Env var only, never logged, fail-closed on missing | `require_hmac_secret()` |
- **Verification Checklist**: ✅/❌ for each mitigation
- **Residual Risks**: Local-only attack surface (agent runs as user), no network exposure

### 2.4 Audit Logger Integration
**File**: (code change, not just doc)

Connect `GuiInteractionService` event stream to the existing `AuditLogger` in `oneshim-automation`:
- Every state transition → audit entry
- Every denied path → audit entry with reason
- Every ticket operation (sign/verify/replay) → audit entry

Implementation: Subscribe to the service's broadcast channel in the scheduler, forward events to `AuditLogger::log()`.

## 3. Acceptance Criteria

- All 3 documents written, reviewed, committed
- Runbook covers all 3 OS permission flows
- Contract examples cover all 7 endpoints with success + error cases
- Security review covers all 6 threat categories
- Audit logger emits entries for all state transitions (verified by test)
