# PR-B2 3-Loop Phase State

**Task**: D13 V2b PR-B2 — SubscribeMetrics RPC implementation
**Branch**: `feature/d13-v2b-pr-b2-subscribe-metrics` off `origin/main` @ `529ec547`
**Ralph-loop completion promise**: `__RALPH_LOOP_PR_B2_ALL_PHASES_CONVERGED_2026_04_22__`

## Current phase: 1 (Spec)

Substep: **1.3 — Iter-1 converged with findings; Spec rev 2 applied; iter-2 dispatch next**

### Iter-1 review results (2026-04-22)

5 reviewers completed. Aggregate: **15 Critical + 31 Important + 24 Minor**. Saved at `.claude/pr-b2-review/iter1/00-aggregate-findings.md`.

Spec revision 2 written (applies all 15 CRIT + 31 IMP fixes). 10 decision gates D1-D10 resolved. Commit: _pending_.

### Iter-2 plan

Dispatch same 5 dimensions again, but with the revised spec + iter-1 aggregate findings as reference. Goal: return zero Critical + zero Important.

Convergence criterion: **two consecutive rounds with zero Crit + zero Important**. Iter-2 is round 1 of potentially two.

## Phase checklist

### Phase 1 — Spec (Critical/Important → 0)

- [x] Read PR-B1 merged state (`grpc/mod.rs`)
- [x] Read full design spec (`2026-04-21-d13-v2b-streaming-design.md`, both RPCs)
- [x] Read PR-B2 section of plan doc (B2-1..B2-9 full)
- [x] Verify fact base (WebConfig.integration_auth_token exists, SystemMonitor port at `oneshim-core/src/ports/monitor.rs`, RealtimeEvent at `oneshim-api-contracts/src/stream.rs`)
- [x] Write initial draft: `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md`
- [ ] Deep review round 1 (parallel subagents: API contract / server semantics / security / test strategy / stale-assumption audit) → classify findings Critical/Important/Minor
- [ ] Fix Critical + Important → re-review
- [ ] Converge (two consecutive zero-Crit/zero-Important rounds, OR one round with no open threads)
- [ ] Mark Phase 1 complete

### Phase 2 — Plan (Critical/Important → 0)

- [ ] Draft `docs/reviews/2026-04-22-d13-v2b-pr-b2-plan.md` from validated spec
- [ ] Deep review (sequencing, stale-assumption audit per PR-B1 lesson, cross-consumer impact, test coverage)
- [ ] Converge
- [ ] Mark Phase 2 complete

### Phase 3 — Impl (issues → 0) + PR

- [ ] Dispatch B2-1..B2-9 via subagent-driven-development (impl + spec reviewer + code reviewer)
- [ ] `cargo check --workspace` clean
- [ ] `cargo test -p oneshim-core --lib sections::network` + `cargo test -p oneshim-web --features grpc-dashboard` green
- [ ] `cargo clippy --workspace -- -D warnings` clean (check clippy 1.95 patterns)
- [ ] `cargo fmt --check` clean
- [ ] PR opened + CI green
- [ ] Emit completion promise `__RALPH_LOOP_PR_B2_ALL_PHASES_CONVERGED_2026_04_22__`
- [ ] `rm .claude/ralph-loop.local.md` at the features worktree (stop-hook bug workaround)

## Stub files (created this iter)

- `src-tauri/oneshim-sandbox-worker-aarch64-apple-darwin` (CI externalBin Tauri stub)
- `crates/oneshim-web/frontend/dist/index.html` (rust-embed SPA stub)

## Verified fact base (for spec/plan)

| Fact | Location | Confirmed |
|---|---|---|
| `WebConfig.integration_auth_token: Option<String>` | `crates/oneshim-core/src/config/sections/network.rs:102` | ✓ |
| `SystemMonitor::collect_metrics() -> SystemMetrics` | `crates/oneshim-core/src/ports/monitor.rs:20` | ✓ |
| `RealtimeEvent` enum | `crates/oneshim-api-contracts/src/stream.rs:13` | ✓ |
| `subscribe_metrics` stub returns `Unimplemented` | `crates/oneshim-web/src/grpc/mod.rs:319-326` | ✓ |
| `to_proto_ts` helper `pub(super)` | `crates/oneshim-web/src/grpc/mod.rs:59-64` | ✓ |
| `serve/serve_optional` take `(port, storage)` today | `crates/oneshim-web/src/grpc/mod.rs:342,370` | ✓ (PR-B2 migrates to GrpcSpawnConfig) |

Need to verify during Phase 2 review: `SystemMetrics` fields (cpu_usage, memory_total, memory_used, …), `tests/common/` dir existence, `app_runtime_launch.rs` structure, PiiSanitizer port path.
