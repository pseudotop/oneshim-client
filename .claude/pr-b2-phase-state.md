# PR-B2 3-Loop Phase State

**Task**: D13 V2b PR-B2 — SubscribeMetrics RPC implementation
**Branch**: `feature/d13-v2b-pr-b2-subscribe-metrics` off `origin/main` @ `529ec547`
**Ralph-loop completion promise**: `__RALPH_LOOP_PR_B2_ALL_PHASES_CONVERGED_2026_04_22__`

## Current phase: 2 (Plan) — Phase 1 DONE ✅

### Phase 1 summary

- Iter-1: 15 Critical + 31 Important + 24 Minor → rev 2 applied all fixes (commit `41252400`)
- Iter-2: 1 Critical + 8 Important + 4 Minor → rev 3 applied all fixes (commit `abc16ca7`)
- Iter-3: **CONVERGED** — zero Crit + zero Important across all 5 dimensions (verdict at `.claude/pr-b2-review/iter3/00-verdict.md`)

Final spec: `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md` @ commit `abc16ca7`

### Phase 2 entry

Substep: **2.1 — draft plan doc `docs/reviews/2026-04-22-d13-v2b-pr-b2-plan.md`**

Next iteration: draft plan derived from spec §4 components + §7 testing + §8 acceptance. Task structure (12 tasks):

- **B2-0**: `remote_addr()` smoke test + `TcpConnectInfo` layer if needed (CRIT-7 gate — blocks everything)
- **B2-1**: `async-stream` + `subtle` dep additions (Cargo.toml workspace + oneshim-web)
- **B2-2**: Config fields `LoadThresholds` + `grpc_load_thresholds` + `grpc_streaming_enabled` + `grpc_max_concurrent_streams` + 4 unit tests
- **B2-3**: `LoadPolicy` component + 9 unit tests
- **B2-4**: `HintEmitter` (with `force_emit_degraded`) + 5 unit tests
- **B2-5**: `auth_gate` (`honor_opt_out` + `validate_authority`) + 8+3 unit tests
- **B2-6**: `StreamCounterGuard` + 4 unit tests
- **B2-7**: `GrpcSpawnConfig` + `test_support/mock_system_monitor.rs` + `test-support = []` feature + `Debug` redaction test
- **B2-8**: `serve`/`serve_optional` → takes `GrpcSpawnConfig` + v2a 10 integration tests updated in-PR
- **B2-9**: `subscribe_metrics` handler impl (realtime + interval branches)
- **B2-10**: 7-8 SubscribeMetrics integration tests
- **B2-11**: `scripts/ci/grep-no-instrument-on-sensitive-fns.sh` + code-review checklist note
- **B2-12**: Final acceptance runs + PR open

Plan review dimensions (iter-1 plan review):
- Dim P1: Task sequencing & dependencies (does B2-N truly depend on B2-(N-1)?)
- Dim P2: Per-task completeness (file paths, commands, assertions, rollback)
- Dim P3: Stale-assumption verification pass per PR-B1 lesson
- Dim P4: Cross-consumer audit (config defaults, test-support feature isolation)
- Dim P5: Spec-to-plan traceability (does plan cover 100% of spec §4-§8 requirements?)

Plan convergence criterion: same as spec — zero Critical + zero Important.

Plan-phase opening tasks (Must Verify Before Task Dispatch):
1. `grpc-dashboard` feature flag deps in `oneshim-web/Cargo.toml` (plan-phase NIT-R3-B)
2. `tonic::Request::remote_addr()` behavior smoke test (spec §10 row 18/19)
3. `app_runtime_launch.rs` grpc spawn call site — exact variable chain
4. `integration_auth_token` config field wiring to `app_runtime_launch.rs`

### Iter-1 review results (2026-04-22)

5 reviewers completed. Aggregate: **15 Critical + 31 Important + 24 Minor**. Saved at `.claude/pr-b2-review/iter1/00-aggregate-findings.md`.

Spec revision 2 written (commit 41252400) applies all 15 CRIT + 31 IMP fixes. 10 decision gates D1-D10 resolved.

### Iter-2 review results (so far)

| # | Dim | Verdict | New issues |
|---|---|---|---|
| 01 | API contract | **CONVERGED** (0C/0I) | none |
| 02 | Server state/concurrency | _pending_ | — |
| 03 | Security | CONVERGED with 2 Important | IMP-SEC-A `:authority` IPv6 bracket parsing bug; IMP-SEC-B grep enforcement fragility |
| 04 | Test strategy | _pending_ | — |
| 05 | Stale audit | _pending_ | — |

### Known iter-2 Important findings (to fix before iter-3)

- **IMP-SEC-A**: `:authority` validator splits on `':'` first — breaks for bracketed IPv6 `[::1]:port` (returns `"["` as host). Use `url::Host::parse` OR manually detect bracket form.
- **IMP-SEC-B**: `#[tracing::instrument]` CI grep is bypassable (multi-line attrs, renamed re-export, new sensitive fns outside allowlist). Downgrade grep claim from "enforcement" to "first-line heuristic"; rely on `GrpcSpawnConfig::Debug` redaction test as actual invariant guard.

### Iter-2 convergence status

Waiting for Dim 02, 04, 05. After all 5 report, aggregate → apply fixes if any Crit/Important remain → iter-3 if needed. Convergence requires two consecutive rounds with zero Crit + zero Important; iter-2 has 2 Important so at minimum an iter-3 is needed.

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
