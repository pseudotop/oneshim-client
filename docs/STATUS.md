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

### Rust Tests (2026-03-20)

| Crate | Tests | Status |
|-------|------:|--------|
| oneshim-core | 341 | pass |
| oneshim-network | 258 + 28 | pass |
| oneshim-suggestion | 17 | pass |
| oneshim-storage | 188 | pass (3 ignored) |
| oneshim-monitor | 78 | pass |
| oneshim-vision | 151 | pass (2 ignored) |
| oneshim-web | 221 | pass |
| oneshim-automation | 202 + 1 | pass |
| oneshim-analysis | 484 | pass |
| oneshim-embedding | 1 | pass (2 ignored) |
| oneshim-api-contracts | 42 | pass |
| oneshim-app (unit) | 209 | pass |
| oneshim-app (integration) | 55 | pass (3 ignored) |
| language-check | 4 | pass |
| **Total** | **2,281** | **0 failed** |

### Build & Lint

- `cargo check --workspace`: pass
- `cargo clippy --workspace --tests`: pass (0 warnings)
- `cargo fmt --check`: pass
- E2E tests: See latest Playwright CI report for exact count

### CI/CD

- Latest CI workflow run: success (`CI`) — [Run 22820191743](https://github.com/pseudotop/oneshim-client/actions/runs/22820191743) (2026-03-08)
- Latest Release workflow run: pending (`Release`, tag `v0.3.1`) (2026-03-09)
- UI/UX QA run records: `docs/qa/runs/2026-02-23-uiux-qa-rc3.md` (latest tracked run evidence)

### Recent Changes (Agent Review Batch 1-5, 2026-03-08)

- **Batch 1**: Config warn stub fix, CI expression injection hardening, script permissions, STATUS update
- **Batch 2**: Added missing derives — `Serialize`/`Deserialize` on `ConsentStatus`/`SessionCreateResponse`/`SseEvent`, `PartialEq`/`Eq` on `AutomationAction`/`AutomationIntent`
- **Batch 3**: `CoreError::RequestTimeout` variant, `map_reqwest_error()` timeout detection, `Swatinem/rust-cache@v2` in release.yml
- **Batch 4**: Vision port traits `&mut self` → `&self` with interior mutability (`Mutex`), Scheduler DI simplified from `Arc<Mutex<Box<dyn T>>>` → `Arc<dyn T>`
- **Batch 5** (v0.2.0): Clippy `needless_borrows_for_generic_args` fix in `oneshim-storage/src/encryption.rs`; E2E `replay-scene` mock added for `/api/ai/providers/presets`; Linux smoke `tauri::generate_context!()` fixed via `frontendDist` stub creation in `scripts/release-reliability-smoke.sh` (pre-existing since v0.1.6)

### GUI V2 Milestone Status (ADR-002)

| Milestone | Description | Status |
|-----------|-------------|--------|
| M1 | Handler Integration Tests + Contract Documentation | done (75 tests, `001fc4f`) |
| M2-P1 | Execution Reliability — focus drift retry, overlay cleanup, timeout | done (10 tests, `a6e7a1a`) |
| M2-P2 | Ticket Expiry Grace Period + Partial Execution Tracking | done (10 tests, `411cd60`) |
| M2-P3 | Execution Reliability Tracing | done (`933bfba`) |
| M3 | SSE Event Stream Integration | done (10 tests, `b700804`) |
| M4 | End-to-End Workflow Tests | done (10 tests, `0b0880e`) |

### GUI V2 Performance Baselines

| Operation | Target | Instrumented |
|-----------|--------|-------------|
| `create_session` (scene analysis + candidate ranking) | <50ms | `#[tracing::instrument]` |
| `highlight_session` (overlay render) | <16ms | `#[tracing::instrument]` |
| `confirm_candidate` (focus validation + ticket signing) | <10ms | `#[tracing::instrument]` |
| `prepare_execution` (focus revalidation + ticket verify) | <10ms | `#[tracing::instrument]` |
| `build_candidates` (200 elements) | <5ms | `#[tracing::instrument]` |
| `sign_ticket` / `verify_ticket` (HMAC-SHA256) | <1ms | `#[tracing::instrument]` |
| Accessibility tree query (depth 3) | <30ms | per-platform impl |

Baselines measured via `RUST_LOG=oneshim_automation=debug` tracing spans. QA template: `docs/qa/runs/TEMPLATE-adr-002-gui-smoke-matrix.md`.

## Notes

- Historical numbers may remain in CHANGELOG entries because they describe past releases.
- For current status communication, always link this file.
- `docs/qa/runs/TEMPLATE-uiux-qa-run.md` is a template and is excluded from latest-run references.
