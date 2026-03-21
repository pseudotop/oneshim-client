# GUI V2 E2E Smoke Tests & QA — Design Spec

> Created: 2026-03-20
> Status: Proposed
> Scope: Cross-crate integration tests, QA templates
> Prerequisite: Native Platform Adapters spec, Failure Scenario Tests spec

## 1. Goal

Create a repeatable E2E smoke test matrix for the complete GUI interaction flow (propose → highlight → confirm → execute) across macOS, Windows, and Linux. Include QA run artifact templates and performance profiling baselines.

## 2. Smoke Test Matrix

### 8 Scenarios

| # | Scenario | Expected Outcome | HTTP Status |
|---|----------|-----------------|-------------|
| 1 | Happy path (full flow) | Action executed successfully | 200 |
| 2 | Permission denied | Graceful error, fallback to OCR | 403 |
| 3 | Focus drift mid-session | Session invalidated | 409 |
| 4 | Expired ticket | Ticket rejected | 422 |
| 5 | Overlay render failure | Coaching fallback notification | 503 |
| 6 | Session timeout (TTL) | Auto-cleanup, Expired event | — |
| 7 | Nonce replay | Ticket rejected on reuse | 422 |
| 8 | Headless/no display | Graceful degrade, no crash | 503 |

### Per-OS Matrix

Each scenario tested on:
- macOS (AXUIElement + MagicOverlay)
- Windows (UIA + MagicOverlay)
- Linux X11 (AT-SPI + MagicOverlay)
- Linux Wayland (AT-SPI, limited overlay)

## 3. Test Infrastructure

### Integration Test File
`crates/oneshim-app/tests/gui_smoke_e2e.rs`

Uses mock adapters wired through the full DI chain:
- MockSystemMonitor, MockProcessMonitor → FocusProbe
- MockElementFinder or PlatformAccessibilityElementFinder (for real OS tests)
- NoOpOverlayDriver or MagicOverlayDriver (for real overlay tests)
- NoOpInputDriver (safety: never execute real actions in tests)

### Real-OS Integration (Manual)
For OS-specific testing that can't be automated:
- macOS: requires accessibility permission grant (manual step)
- Windows: requires active desktop session
- Linux: requires running AT-SPI daemon

## 4. QA Run Artifact Template

File: `docs/qa/runs/TEMPLATE-adr-002-gui-smoke-matrix.md`

```markdown
# ADR-002 GUI V2 Smoke Test Run

**Date**: YYYY-MM-DD
**Tester**: [name]
**Version**: [git hash]
**OS**: [macOS version / Windows version / Linux distro+compositor]

## Environment
- Display: [resolution, DPI scale]
- Accessibility: [permission granted? Y/N]
- HMAC Secret: [configured? Y/N]

## Results

| # | Scenario | Status | Notes | Duration |
|---|----------|--------|-------|----------|
| 1 | Happy path | PASS/FAIL | | ms |
| 2 | Permission denied | PASS/FAIL | | ms |
| ... | ... | ... | ... | ... |

## Artifacts
- [ ] Screenshot of overlay highlight
- [ ] Log excerpt for failure scenarios
- [ ] Performance timing (P50/P95/P99)

## Issues Found
- (list any bugs or unexpected behaviors)
```

## 5. Performance Profiling

### Metrics to Capture

| Operation | Target | Method |
|-----------|--------|--------|
| Candidate ranking (`build_candidates`) | <5ms for 200 elements | `tracing::instrument` span timing |
| Overlay highlight render | <16ms (1 frame) | Tauri event → React render timing |
| Focus validation (`validate_execution_binding`) | <10ms | Span timing |
| Full E2E flow (propose → execute) | <500ms | Test harness timing |
| Accessibility tree query (depth 3) | <30ms | `extract_window_elements` span |

### Profiling Approach
- Add `#[tracing::instrument]` to key functions
- Use `tracing-subscriber` with timing layer in test harness
- Report P50/P95/P99 over 100 iterations

## 6. Acceptance Criteria

- 8 smoke scenarios pass on at least 1 OS (mock-based)
- QA template exists and is usable
- Performance baselines documented in `docs/STATUS.md`
- Known failure signatures mapped to operator actions in runbook
