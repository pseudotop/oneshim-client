# PR-B2 3-Loop Phase State

**Task**: D13 V2b PR-B2 — SubscribeMetrics RPC implementation
**Branch**: `feature/d13-v2b-pr-b2-subscribe-metrics` off `origin/main` @ `529ec547`
**Ralph-loop completion promise**: `__RALPH_LOOP_PR_B2_ALL_PHASES_CONVERGED_2026_04_22__`

## Current phase: 3 (Impl) — Phase 1+2 DONE ✅✅

### Phase 2 summary

- Iter-1: 4 Critical + 6 Important + 2 Minor from 4 parallel reviewers → rev 2 (commit `3bb62d7f`)
- Iter-2: **CONVERGED** — zero new Critical/Important; all 11 findings verified applied (verdict at `.claude/pr-b2-plan-review/iter2/00-verdict.md`)

Final plan: `docs/reviews/2026-04-22-d13-v2b-pr-b2-plan.md` @ commit `3bb62d7f`

### Phase 3 entry

Substep: **3.9 — B2-0..B2-8 done; B2-9 subscribe_metrics handler next**

### B2 progress

| Task | Status | Commit | Notes |
|---|---|---|---|
| B2-0 | ✅ verified | no commit | remote_addr=Some(127.0.0.1:…) |
| B2-1 | ✅ done | `5e2954e9` | async-stream + subtle + test-support features |
| B2-2 | ✅ done | `6bcb6a56` | WebConfig LoadThresholds/streaming_enabled/max_concurrent_streams + 4 tests |
| B2-3 | ✅ done | `2ea8b3b7` | LoadPolicy + 9 tests |
| B2-4 | ✅ done | `d574f569` | HintEmitter + force_emit_degraded + 6 tests |
| B2-5 | ✅ done | `3ab0abe6` | auth_gate (honor_opt_out + validate_authority) + 13 tests |
| B2-6 | ✅ done | `726ed8ce` | StreamCounterGuard + 4 tests |
| B2-7 | ✅ done | (B2-7 commit) | GrpcSpawnConfig + MockSystemMonitor + Debug redact test |
| B2-8 | ✅ done | `785ccf9f` | serve migration + 10 v2a tests pass + app_runtime_launch wired |
| B2-9 | pending | — | subscribe_metrics handler impl (~250 LoC) |
| B2-10 | pending | — | 7-8 integration tests |
| B2-11 | pending | — | CI grep script + grpc-dashboard CI steps |
| B2-12 | pending | — | Acceptance + PR open |

### Test count so far

- network config: 4 pass (oneshim-core)
- LoadPolicy: 9 pass
- HintEmitter: 6 pass
- auth_gate: 13 pass (11 honor_opt_out + 2 authority OR 10+3)
- StreamCounterGuard: 4 pass
- GrpcSpawnConfig debug redact: 1 pass
- v2a integration: 10 pass (now via GrpcSpawnConfig)

**Total passing: 47 tests across B2-1..B2-8.**

### B2-0 result (retained for reference)

Smoke probe confirmed `tonic::Request::remote_addr()` returns `Some(127.0.0.1:<port>)` under default `Server::builder().add_service(...).serve(addr)` wiring. No `TcpConnectInfo` tower layer needed. Spec §10 rows 18/19 **CONTINGENCY RESOLVED**.

### Commit hygiene note

cargo-fmt hook occasionally reformats generated code (long signatures, multi-line expressions). Workflow: write code → `cargo fmt -p <crate>` → re-add → commit. Don't fight rustfmt line-by-line.

### Next iteration entry point

1. Read state file (this doc)
2. Start at B2-4: create `crates/oneshim-web/src/grpc/hint_emitter.rs` per plan §"Task B2-4"
3. `LoadThresholds` imported via `oneshim_core::config::LoadThresholds` (NOT via `sections::network::`)
4. `Level::LoadLevel{Low|Medium|High|Critical}` for proto enum values (prost camelCase from SCREAMING_SNAKE)

### Phase 3 execution pattern

For each task B2-N:
1. Pre-dispatch: verify relevant Gate (V1..V8) if applicable
2. Impl subagent: implements per plan (writes code + tests)
3. Spec-conformance reviewer: verifies code matches spec + plan
4. Code-quality reviewer: clippy, fmt, patterns, clarity
5. Commit when all 3 agree
6. Proceed to B2-(N+1)

### Phase 3 open items (carry forward from spec/plan phases)

- Plan nit MIN-N1: `AtomicUsize`/`Arc` imports in pseudocode — implementer adds during B2-8 real impl
- Spec NIT-R3-A: `force_emit_degraded` Option<> return type — final impl may drop Option wrapper (always Some)
- Spec NIT-R3-C: internal API mismatch in HintEmitter build_hint reason_tag — tighten during B2-4 impl

### Phase 3 convergence criterion

- All B2-0..B2-12 complete
- `cargo check --workspace` clean across macOS + CI Linux/Windows
- 4 config tests + 30 unit tests + 18 integration tests green
- `--no-default-features --features grpc-dashboard` smoke build clean
- `scripts/ci/grep-no-instrument-on-sensitive-fns.sh` exits 0
- `cargo clippy --workspace -- -D warnings` clean (1.95 patterns audited)
- `cargo fmt --check` clean
- PR opened and CI green
- Emit completion promise `__RALPH_LOOP_PR_B2_ALL_PHASES_CONVERGED_2026_04_22__`
- `rm .claude/ralph-loop.local.md` at features worktree

### Phase 1 summary

- Iter-1: 15 Critical + 31 Important + 24 Minor → rev 2 applied all fixes (commit `41252400`)
- Iter-2: 1 Critical + 8 Important + 4 Minor → rev 3 applied all fixes (commit `abc16ca7`)
- Iter-3: **CONVERGED** — zero Crit + zero Important across all 5 dimensions (verdict at `.claude/pr-b2-review/iter3/00-verdict.md`)

Final spec: `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md` @ commit `abc16ca7`

### Phase 2 entry

Substep: **2.5 — plan rev 2 committed (`3bb62d7f`); iter-2 convergence reviewer dispatched (single consolidated pass); awaiting notification.**

Plan doc: `docs/reviews/2026-04-22-d13-v2b-pr-b2-plan.md` (commit `b135d272`)

### Iter-1 plan review results

| # | Dim | Verdict | Counts | Top findings |
|---|---|---|---|---|
| P1 | Sequencing & completeness | NOT CONVERGED | 2C / 3I / 2Minor | `is_local_loopback` pub(super); `active_streams` accessor; B2-7 test-support scope; B2-0 rollback blocker; B2-8 granularity |
| P2 | Stale audit | STALE | 2 stale / 15 | **`integration_auth_token` NOT in app_runtime_launch.rs** — lives in `services/settings_service/mod.rs:14`; CI uses `--features grpc` umbrella, not `grpc-dashboard` |
| P3 | Spec traceability | FULL COVERAGE | 2 minor gaps | Same as P1 C1/C2 (pub(super) + field init) |
| P4 | Cross-cutting | _pending_ | — | — |

### Consolidated findings to apply in rev 2 (this iter)

**Critical**:
- **CRIT-P-1**: B2-5 must export `pub(super) fn is_local_loopback` (P1 C1 + P3 gap 1)
- **CRIT-P-2**: B2-10 test #7 — pick direct accessor on DashboardServiceImpl OR indirect cap+1 probe; if direct, B2-8 adds `pub(crate) fn active_stream_count() -> usize` (P1 C2 + P3 gap 2)
- **CRIT-P-3**: `integration_auth_token` wiring path WRONG — not in `app_runtime_launch.rs`. Lives in `services/settings_service/mod.rs:14`. B2-8 must thread token from WebServerRuntime config, not from `core_resources` (P2 #15)

**Important**:
- **IMP-P-1**: CI feature flag naming — plan references `grpc-dashboard` (crate feature); CI uses `--features grpc` (workspace umbrella). B2-11 grep script + acceptance commands must target both or verify alias. Check workspace `Cargo.toml` for `grpc = ["oneshim-web/grpc-dashboard", ...]` (P2 #13)
- **IMP-P-2**: B2-7 scope inconsistency — `test-support = []` is owned by B2-1; remove dup from B2-7 title/scope (P1 I1)
- **IMP-P-3**: B2-0 rollback note must identify downstream blocker — B2-10 test #6 `honors_opt_out_on_localhost` fails if `remote_addr() == None` (P1 I2)
- **IMP-P-4**: B2-8 commit granularity — add "atomic refactor" justification OR split into B2-8a (sig change) + B2-8b (test/caller updates) (P1 I3)
- **IMP-P-5**: `MetricBucketRecord` path — plan uses bare name; actual full path is `crate::models::dashboard_streaming::MetricBucketRecord`. B2-9 handler + B2-3 use sites must disambiguate (P2 nit)

**Minor**:
- M1: B2-0 idempotency note (same commit contains revert OR layer addition)
- M2: B2-8 rollback wording "fully revert, not partial"

### P1 findings to apply (after aggregating all 4)

**Critical**:
- **C1**: `is_local_loopback` must be `pub(super)` in B2-5 file structure (B2-9 handler imports it)
- **C2**: B2-10 test #7 needs explicit strategy: direct `active_stream_count()` accessor on DashboardServiceImpl (add in B2-8) OR indirect "cap+1 probe" approach

**Important**:
- **I1**: B2-7 scope inconsistency — `test-support = []` is owned by B2-1; remove from B2-7 title
- **I2**: B2-0 rollback note must identify downstream blocker (B2-10 test #6 `honors_opt_out_on_localhost` fails if remote_addr returns None)
- **I3**: B2-8 commit granularity — add "atomic refactor" justification note OR split into B2-8a/B2-8b

**Minor**: M1 B2-0 idempotency; M2 B2-8 "fully revert, not partial" rollback language

Waiting on P2, P3, P4 before applying fixes (avoid double-edit).

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
