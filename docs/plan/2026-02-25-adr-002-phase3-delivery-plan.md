[English](./2026-02-25-adr-002-phase3-delivery-plan.md) | [한국어](./2026-02-25-adr-002-phase3-delivery-plan.ko.md)

# ADR-002 Phase3 Delivery Plan

**Date**: 2026-02-25  
**Status**: Active  
**Source ADR**: `docs/architecture/ADR-002-os-gui-interaction-boundary.md`  
**Companion Baseline Plan**: [`2026-02-25-adr-002-gui-v2-implementation-plan.md`](./2026-02-25-adr-002-gui-v2-implementation-plan.md)

## 1. Goal

Complete production-grade OS GUI interaction delivery for ADR-002 with:

1. reliable session/ticket security gates
2. dedicated session SSE lifecycle correctness
3. hardened native accessibility + overlay adapters across macOS/Windows/Linux

## 2. Current Baseline (already implemented as of 2026-02-25)

- GUI V2 contracts, session flow, and dedicated SSE endpoint
- HMAC ticket signing and validation with `ONESHIM_GUI_TICKET_HMAC_SECRET`
- starter accessibility and overlay adapters for all target OS
- composition wiring in `oneshim-app`

## 3. Workstreams

## WS-1: Control Plane and Contract Hardening

Target crates:
- `oneshim-automation`
- `oneshim-web`
- `docs/contracts`

Implementation items:
1. Add handler-level integration coverage for `401/409/422/503` edge paths on GUI V2 endpoints.
2. Add API contract examples for all seven V2 endpoints and the dedicated SSE event payload.
3. Add session token + ticket expiry boundary tests and nonce replay tests.
4. Add explicit audit assertions for every state transition and denied execute path.

Acceptance criteria:
1. Contract examples are versioned under `docs/contracts/`.
2. GUI session API negative-path tests are deterministic and pass in CI.
3. Every failed execute path emits a traceable audit entry.

## WS-2: Execution Plane Hardening (Per OS)

Target crate:
- `oneshim-app`

Implementation items:
1. macOS: replace script fallback path with AXUIElement + NSWindow overlay adapter behind a feature flag.
2. Windows: replace script fallback path with UIA COM + layered transparent overlay adapter.
3. Linux: add AT-SPI probe path and keep X11/Wayland fallback strategy explicit.
4. Normalize coordinate conversions and focus checks with DPI/compositor-aware logic.
5. Keep no-op fallback path for unsupported/headless environments.

Acceptance criteria:
1. Each OS has at least one native adapter path selectable by config/feature.
2. Overlay cleanup is deterministic on session cancel/expire/execute.
3. Focus drift at execute time is blocked consistently with `409`.

## WS-3: Reliability and Operational Readiness

Target crates/docs:
- `oneshim-app`
- `oneshim-automation`
- `docs/guides`
- `docs/qa`

Implementation items:
1. Add end-to-end smoke scenarios for `propose -> highlight -> confirm -> execute`.
2. Add failure scenarios: permission denied, focus drift, expired ticket, overlay render failure.
3. Add runbook updates for operator troubleshooting (permissions, OS feature toggles, fallback behavior).
4. Add QA run artifact metadata templates for cross-OS verification.

Acceptance criteria:
1. A repeatable smoke matrix exists for macOS/Windows/Linux.
2. Known failure signatures map to clear operator actions.
3. CI includes at least contract + service-level regression checks for GUI V2.

## 4. Execution Timeline (planned)

1. 2026-02-26 to 2026-03-03: WS-1 complete (contracts/tests/audit paths).
2. 2026-03-04 to 2026-03-14: WS-2 native adapter hardening by OS.
3. 2026-03-15 to 2026-03-21: WS-3 runbooks + QA matrix + release gate review.

## 5. Release Gates

1. Security: HMAC secret required, nonce replay blocked, session token enforced on all `:id` routes and SSE.
2. Architecture: no direct adapter-to-adapter coupling outside approved composition paths.
3. Quality: `cargo check` for touched crates and relevant `cargo test` suites pass.
4. Documentation: `docs/plan/README.md` and `docs/README.md` stay synchronized with plan status.
