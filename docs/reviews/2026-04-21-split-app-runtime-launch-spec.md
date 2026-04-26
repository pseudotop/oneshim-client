# Split: `app_runtime_launch.rs` — Spec

**Date**: 2026-04-21
**Parent**: [`2026-04-21-p2-large-files-triage.md`](2026-04-21-p2-large-files-triage.md) #1 (must-split)
**Status**: SPEC (Loop 1)

## Scope correction from triage doc

The triage doc proposed a 3-phase split (`health_probe_phase`, `web_server_phase`, `scheduler_phase`). **That is infeasible.** Analysis of the actual control flow reveals the file is a single monolithic composition root, not a 3-phase pipeline. See analysis below.

## Narrowed scope — this PR

**Extract 1 phase module**:
- `src-tauri/src/app_runtime_launch/health_probe_phase.rs` — self-contained startup health probe + rolled-back-notification scan + spawn_healthy_writer helper.

**Expected net LOC reduction**: `app_runtime_launch.rs` 1068 → ~810 (~24% smaller).

## Why not the full 3-phase split

Mapping the actual file contents:

| Lines | Block | Category |
|-------|-------|----------|
| 47-75 | Destructure + installation_id | setup |
| 77-154 | **Health probe + rollback** | **self-contained** |
| 156-240 | core_resources + **rolled_back_notification scan** | setup + self-contained |
| 241-283 | Atomic flags + shared_capture_services | shared state |
| 285-554 | **Composition root** (regime, coaching, suggestion_manager) | shared state |
| 556-658 | agent_runtime wiring (25 `with_*` calls) | consumer |
| 660-740 | session_manager + idle reaper | consumer |
| 742-791 | Web server + gRPC dashboard | consumer |
| 793-956 | Audio/STT/model_downloader | setup |
| 958-1047 | AppState + ManagedStateBuilder composition | aggregator |
| 1049-1061 | **spawn_healthy_writer** (probe reuse) | **self-contained** |

The ~500 lines in the middle (241-740) construct ~40 `Arc`s that are read by _every_ downstream block (agent_runtime, web_server, session_manager, AppState). There is no "web_server phase" cleanly separable from "scheduler phase" — they are siblings consuming shared state.

**Only health_probe has a true sequential handoff**: line 89 (build probe) → line 1056 (spawn healthy writer). Everything between consumes intermediate state that cannot be phase-packaged without a wide input struct.

## Extraction A — `health_probe_phase.rs`

### Input
- `&AppHandle` (for `current_exe` context)
- `&runtime::Handle` (for rolled_back scan spawn)
- `Arc<UpdateControl>` (for rolled_back scan → UI broadcast)

### Output
```rust
pub(crate) struct HealthProbePhaseResult {
    pub(crate) probe: Option<crate::updater::HealthProbe>,
    // On RollbackRequired path, execute_rollback panics via process::exit.
    // On Normal + None paths, we return the probe (or None) for spawn_healthy_writer.
}
```

### Public API
```rust
pub(crate) fn execute_startup_probe(
    handle: &tokio::runtime::Handle,
    update_control: Arc<UpdateControl>,
) -> HealthProbePhaseResult;

pub(crate) fn spawn_healthy_writer(
    probe: Option<crate::updater::HealthProbe>,
    handle: &tokio::runtime::Handle,
);
```

### Extracts
- Lines 77-154: `execute_startup_probe` body (probe construction + `check_startup_state` + rollback execution on `RollbackRequired`)
- Lines 170-240: nested — the `rolled_back_notification_*` scan. Spawned inside `execute_startup_probe` (both need `current_exe` + `install_dir`).
- Lines 1049-1061: `spawn_healthy_writer` (keeps probe alive + 30s uptime marker writer).

## Acceptance

| Check | Expected |
|-------|----------|
| `cargo check -p oneshim-app` | clean |
| `cargo clippy -p oneshim-app --bin oneshim --no-deps -- -D warnings` | clean |
| `cargo test -p oneshim-app --lib updater::health_probe` | all pass |
| `cargo fmt --check` | clean |
| `wc -l src-tauri/src/app_runtime_launch.rs` | < 850 |
| `wc -l src-tauri/src/app_runtime_launch/health_probe_phase.rs` | 150-220 |
| `build_and_spawn` function behavior unchanged | observable via existing tests + manual launch |

## Non-goals

- Web server phase extraction (medium confidence, wide input struct, net gain not worth the churn per analysis)
- Scheduler phase extraction (does not exist as a coherent phase — composition root)
- Refactoring AgentRuntimeBuilder wiring (separate scope)

## Risk

- **LOW**: pure I/O + logging, no Arc threading beyond `UpdateControl`.
- Existing 13 tests in `updater::health_probe::tests` cover probe behavior; extraction should not affect them.

## Rollback

Single-commit revert.

## Loop 1 self-review

- [x] Scope correction honest (3-phase claim refuted)
- [x] Input/output contracts defined
- [x] Blockers identified and avoided (rolled_back scan consolidated inside probe phase)
- [x] Acceptance criteria command-verifiable
- [x] Risk bound: LOW (pure I/O)
- [x] No placeholders

**Gate passed.** Proceed to Loop 2 (Plan).
