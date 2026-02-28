[English](./STATUS.md) | [한국어](./STATUS.ko.md)

# Project Status (Single Source of Truth)

This document is the canonical source for mutable project quality metrics.

## Scope

The following values must be tracked only in this file:

- Rust test totals and pass/fail status
- E2E test totals and pass/fail status
- Lint/build status
- Known flaky test status

Other documents should link to this file instead of hard-coding mutable counts.

## Update Policy

Update this document whenever CI quality metrics change.

Recommended verification commands:

```bash
cargo test --workspace
cd crates/oneshim-web/frontend && pnpm test:e2e
cargo clippy --workspace
cargo fmt --check
```

## Current Snapshot

### Rust Tests (2026-02-27)

| Crate | Tests | Status |
|-------|------:|--------|
| oneshim-core | 74 | pass |
| oneshim-network | 87 | pass |
| oneshim-suggestion | 17 | pass |
| oneshim-storage | 42 | pass |
| oneshim-monitor | 39 | pass |
| oneshim-vision | 84 | pass |
| oneshim-ui | 37 | pass |
| oneshim-web | 119 | pass |
| oneshim-automation | 193 | pass |
| oneshim-app (unit) | 99 | pass |
| oneshim-app (integration) | 32 | pass (3 ignored) |
| oneshim-api-contracts | 8 | pass |
| language-check | 4 | pass |
| **Total** | **831** | **0 failed** |

### Build & Lint

- `cargo check --workspace`: pass
- `cargo clippy --workspace --tests`: pass (2 pre-existing `type_complexity` warnings in `oneshim-app`)
- `cargo fmt --check`: pass
- E2E tests: See latest Playwright CI report for exact count

### CI/CD

- Latest CI workflow run: failure (`CI`, stale HTTP interface manifest) — [Run 22489514315](https://github.com/pseudotop/oneshim-client/actions/runs/22489514315) (2026-02-27)
- Latest Release workflow run: success (`Release`, tag `v0.1.1`) — [Run 22489557639](https://github.com/pseudotop/oneshim-client/actions/runs/22489557639) (2026-02-27)
- Latest Notarization workflow run: in progress (`Notarize macOS Release Assets`) — [Run 22512298797](https://github.com/pseudotop/oneshim-client/actions/runs/22512298797) (started 2026-02-28)
- UI/UX QA run records: `docs/qa/runs/2026-02-23-uiux-qa-rc3.md` (latest tracked run evidence)

### GUI V2 Milestone Status (ADR-002)

| Milestone | Description | Status |
|-----------|-------------|--------|
| M1 | Handler Integration Tests + Contract Documentation | done (75 tests, `001fc4f`) |
| M2-P1 | Execution Reliability — focus drift retry, overlay cleanup, timeout | done (10 tests, `a6e7a1a`) |
| M2-P2 | Ticket Expiry Grace Period + Partial Execution Tracking | done (10 tests, `411cd60`) |
| M2-P3 | Execution Reliability Tracing | done (`933bfba`) |
| M3 | SSE Event Stream Integration | pending |
| M4 | End-to-End Workflow Tests | pending |

## Notes

- Historical numbers may remain in CHANGELOG entries because they describe past releases.
- For current status communication, always link this file.
- `docs/qa/runs/TEMPLATE-uiux-qa-run.md` is a template and is excluded from latest-run references.
