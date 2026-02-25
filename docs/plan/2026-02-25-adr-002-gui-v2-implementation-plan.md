[English](./2026-02-25-adr-002-gui-v2-implementation-plan.md) | [한국어](./2026-02-25-adr-002-gui-v2-implementation-plan.ko.md)

# ADR-002 GUI V2 Implementation Plan

**Date**: 2026-02-25
**Status**: Active
**Source ADR**: `docs/architecture/ADR-002-os-gui-interaction-boundary.md`
**Detailed Plan**: [`2026-02-25-adr-002-phase3-delivery-plan.md`](./2026-02-25-adr-002-phase3-delivery-plan.md)

## 1. Objective

Ship the ADR-002 interaction model end-to-end:

1. `propose -> highlight -> confirm -> execute`
2. signed ticket integrity via fixed env secret (`ONESHIM_GUI_TICKET_HMAC_SECRET`)
3. dedicated session SSE stream (`/api/automation/gui/sessions/:id/events`)
4. strict separation: web control plane vs OS execution plane

## 2. Scope by Crate

- `oneshim-core`: GUI contracts (models + ports)
- `oneshim-automation`: session state machine, ticket signing/verification, orchestration
- `oneshim-web`: V2 API handlers + token enforcement + dedicated SSE
- `oneshim-app`: composition wiring (FocusProbe adapter, Overlay driver)
- `oneshim-ui` / OS adapters: native overlay and accessibility hardening (Phase 3)

## 3. Milestones

## M0: Contracts and Baseline Runtime (Done)

- Add `FocusProbe`, `OverlayDriver` ports.
- Add GUI models (session, candidate, focus snapshot, ticket, event).
- Add in-memory `GuiInteractionService` with:
  - session capability token validation
  - HMAC signing/verification
  - TTL cleanup task
  - state transitions and event publication
- Add controller orchestration methods.
- Add web V2 endpoints and dedicated session SSE.

## M1: Compatibility and Handler Hardening (In Progress)

- Add handler-level integration tests for:
  - missing/invalid `X-Gui-Session-Token`
  - `409/422/503` mappings
  - session-scoped SSE filtering
- Add audit event coverage for every transition and denial path.
- Add explicit docs for request/response payload examples in `docs/contracts`.

## M2: Execution Reliability (Next)

- Enforce atomic focus revalidation during execute path with richer diagnostics.
- Add nonce replay tests and boundary tests (expired session/ticket, race windows).
- Add retry semantics for recoverable execution failures.

## M3: Native Overlay and Accessibility (Phase 3 Core)

### macOS
- Accessibility adapter (AXUIElement) for focused-window-first candidate discovery.
- Native overlay adapter (always-on-top, click-through NSWindow).
- Permission gating + failure UX when AX permission is missing.

### Windows
- Accessibility adapter (UIA COM).
- Native overlay adapter (`WS_EX_LAYERED`, transparent/click-through handling).
- Foreground window and DPI-aware coordinate normalization.

### Linux
- Accessibility adapter (AT-SPI via D-Bus).
- Overlay adapter for X11/Wayland fallback strategy.
- Compositor-specific compatibility handling.

## M4: Production Hardening

- Performance profiling for candidate ranking and overlay refresh latency.
- Security review: session hijack, replay, spoofing checks.
- E2E smoke scenarios per OS and headless fallback paths.

## 4. API Contract (V2)

- `POST /api/automation/gui/sessions`
- `GET /api/automation/gui/sessions/:id`
- `POST /api/automation/gui/sessions/:id/highlight`
- `POST /api/automation/gui/sessions/:id/confirm`
- `POST /api/automation/gui/sessions/:id/execute`
- `DELETE /api/automation/gui/sessions/:id`
- `GET /api/automation/gui/sessions/:id/events`

All `:id` routes require `X-Gui-Session-Token`.

## 5. Security Gates

- Ticket integrity: HMAC signature over ticket payload.
- Session capability token required for all session routes and SSE.
- Execution blocked on focus drift (`409`) and invalid/expired ticket (`422`).
- Missing HMAC secret is fail-closed (`503`).

## 6. Definition of Done

- All milestones M0-M2 merged and tested in CI.
- At least one native overlay + accessibility adapter shipped behind feature flag on each target OS in M3.
- Dedicated GUI SSE used as source of truth for session lifecycle in frontend.
- Legacy scene endpoints remain functional during rollout.

## 7. Current Implementation Snapshot (2026-02-25)

Implemented now:
- Core GUI contracts + ports
- Automation GUI interaction service and controller orchestration
- Web V2 routes + dedicated session SSE
- App wiring with `ProcessMonitor`-backed `FocusProbe`
- HMAC secret fixed to `ONESHIM_GUI_TICKET_HMAC_SECRET`
- Phase 3 starter adapters shipped for all target OS:
  - macOS: accessibility probe via System Events script + Python overlay border renderer
  - Windows: UIA probe via PowerShell + WinForms overlay border renderer
  - Linux: active-window accessibility fallback via `xdotool` + Python overlay border renderer

Remaining priority:
1. Handler/service integration tests and contract docs (M1)
2. execution-race hardening and replay boundary tests (M2)
3. real native overlay + accessibility adapters per OS (M3)
