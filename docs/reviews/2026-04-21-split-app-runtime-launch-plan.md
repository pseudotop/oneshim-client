# Split: `app_runtime_launch.rs` — Plan

**Companion**: [Spec](2026-04-21-split-app-runtime-launch-spec.md) (Loop 1)
**Status**: PLAN (Loop 2)
**Target effort**: ~1.5 hours

## Structure

Create a directory module:

```
src-tauri/src/
  app_runtime_launch.rs           # orchestrator (~810 LOC)
  app_runtime_launch/
    mod.rs                        # re-exports (replaces app_runtime_launch.rs in module tree)
    health_probe_phase.rs         # new — 180 LOC
```

Actually simpler: keep `app_runtime_launch.rs` as the orchestrator and add a sibling `app_runtime_launch_health_probe.rs` (underscore, not nested directory). Matches existing workspace convention (`app_runtime_launch.rs` is not yet a dir module).

## Tasks

### Task 1 — Create `app_runtime_launch_health_probe.rs`

Write the following content:

```rust
//! Health probe + rollback startup phase for `app_runtime_launch::build_and_spawn`.
//!
//! Runs BEFORE any scheduler loop spawns. If the probe escalates to
//! `RollbackRequired` (two consecutive failed boots on this version without a
//! self-healthy marker), `execute_rollback` spawns the restored binary and this
//! process terminates via `ROLLBACK_EXIT_CODE`. On any error, logs and returns
//! `None` — the current (failing) binary keeps running; the next boot retries.

use std::sync::Arc;

use crate::updater::{HealthProbe, RollbackReason, StartupAction, Updater, CURRENT_VERSION};
use crate::update_control::UpdateControl;

/// Execute the startup health probe + rolled-back-notification scan.
///
/// Returns the live `HealthProbe` handle (kept alive until the scheduler is
/// ready to call `spawn_healthy_writer`). `None` on any error in the probe
/// setup chain — downstream code must handle the no-probe case.
pub(crate) fn execute_startup_probe(
    handle: &tokio::runtime::Handle,
    update_control: Arc<UpdateControl>,
) -> Option<HealthProbe> {
    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("health probe skipped: std::env::current_exe() failed: {e}");
            return None;
        }
    };
    let install_dir = match current_exe.parent().map(std::path::Path::to_path_buf) {
        Some(dir) => dir,
        None => {
            tracing::warn!("health probe skipped: current_exe has no parent");
            return None;
        }
    };

    // Spawn the rolled_back_notification scan. This is fire-and-forget; the
    // scan consumes `.rolled_back_notification_<version>` markers and feeds
    // the RolledBack state to UpdateControl for UI broadcast.
    spawn_rolled_back_scan(handle, install_dir.clone(), update_control);

    let probe = HealthProbe::new(install_dir, CURRENT_VERSION.to_string());
    match probe.check_startup_state() {
        StartupAction::Normal => {
            tracing::debug!("health probe: Normal — proceeding with startup");
            Some(probe)
        }
        StartupAction::RollbackRequired {
            from_version,
            to_version,
            backup_path,
            reason,
        } => {
            tracing::error!(
                "health probe escalated to rollback: {from_version} -> {to_version} ({:?})",
                reason
            );
            let contract_reason = match reason {
                RollbackReason::RepeatedStartupFailure => {
                    oneshim_api_contracts::update::RollbackReason::RepeatedStartupFailure
                }
            };
            match Updater::execute_rollback(
                &backup_path,
                &current_exe,
                &from_version,
                &to_version,
                contract_reason,
                |info| {
                    tracing::warn!(
                        "rollback event: {} -> {} ({:?})",
                        info.from_version,
                        info.to_version,
                        info.reason
                    );
                },
            ) {
                Ok(_never) => unreachable!("Infallible success path"),
                Err(e) => {
                    tracing::error!("rollback failed: {e}");
                    None
                }
            }
        }
    }
}

/// Scan for `.rolled_back_notification_<version>` markers written by a
/// previous (failing) binary just before the rollback swap. The restored
/// binary surfaces the RolledBack state in UI on next boot.
///
/// Holistic-review I-2: files whose `to_version` matches the current binary
/// are OUR rollback — consume + delete. Files whose `to_version` does not
/// match are stale from a prior rollback cycle — delete without surfacing UI.
fn spawn_rolled_back_scan(
    handle: &tokio::runtime::Handle,
    install_dir: std::path::PathBuf,
    update_control: Arc<UpdateControl>,
) {
    handle.spawn(async move {
        let entries = match std::fs::read_dir(&install_dir) {
            Ok(it) => it,
            Err(e) => {
                tracing::warn!("rolled_back_notification scan failed ({:?}): {e}", install_dir);
                return;
            }
        };
        let current_version = CURRENT_VERSION;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with(".rolled_back_notification_") {
                continue;
            }
            let path = entry.path();
            match std::fs::read(&path) {
                Ok(bytes) => match serde_json::from_slice::<
                    oneshim_api_contracts::update::RollbackInfo,
                >(&bytes)
                {
                    Ok(info) => {
                        if info.to_version == current_version {
                            tracing::warn!(
                                "consuming rolled_back_notification: {} -> {}",
                                info.from_version,
                                info.to_version
                            );
                            let _ = update_control.set_rolled_back(info).await;
                        } else {
                            tracing::debug!(
                                "sweeping stale rolled_back_notification (to_version={}, current={})",
                                info.to_version,
                                current_version
                            );
                        }
                    }
                    Err(e) => tracing::warn!("rolled_back_notification parse failed: {e}"),
                },
                Err(e) => tracing::warn!("rolled_back_notification read failed: {e}"),
            }
            let _ = std::fs::remove_file(&path);
        }
    });
}

/// Spawn the self-healthy marker writer. After `healthy_threshold` of
/// continuous uptime without a crash, records `.self_healthy_{VERSION}` +
/// cleans rollback state. Fire-and-forget; the task survives past
/// `build_and_spawn` return.
pub(crate) fn spawn_healthy_writer(probe: Option<&HealthProbe>) {
    if let Some(probe) = probe {
        let _join_handle = probe.spawn_healthy_writer();
        tracing::debug!("health probe: spawn_healthy_writer dispatched");
    }
}
```

### Task 2 — Wire in `main.rs` or sibling module list

Add `mod app_runtime_launch_health_probe;` to the module list in `src-tauri/src/main.rs` (matches existing `mod app_runtime_launch;`).

### Task 3 — Replace lines 77-154 in `app_runtime_launch.rs`

**Before** (lines 77-154 — 78 lines):
```rust
// Phase 4 D11: post-install self-healthy probe.
// ... 20 lines of doc comments ...
let health_probe: Option<crate::updater::HealthProbe> = match std::env::current_exe() {
    // ... 65 lines of nested match/execute_rollback ...
};
```

**After** (3 lines):
```rust
// Phase 4 D11: post-install self-healthy probe + rolled-back notification
// scan. Runs BEFORE any scheduler loop spawns. On RollbackRequired, the
// helper calls execute_rollback which terminates this process.
let health_probe = crate::app_runtime_launch_health_probe::execute_startup_probe(
    &handle,
    update_control.clone(),
);
```

**Net delta**: -75 lines.

Note: `execute_startup_probe` takes `update_control`, which is built at line 167. The call must move to just after `let update_control = core_resources.update_runtime.update_control.clone();` — reorder the probe call to line ~168.

### Task 4 — Replace lines 170-240 in `app_runtime_launch.rs`

Delete the rolled_back_notification scan (now inside `spawn_rolled_back_scan` called by `execute_startup_probe`). **Net delta**: -70 lines.

Also delete the early probe call at original line 89 (moved in Task 3).

### Task 5 — Replace lines 1049-1061 in `app_runtime_launch.rs`

**Before** (13 lines):
```rust
// Phase 4 D11: scheduler is now fully up — ...
if let Some(probe) = health_probe.as_ref() {
    let _join_handle = probe.spawn_healthy_writer();
    tracing::debug!("health probe: spawn_healthy_writer dispatched");
}
```

**After** (1 line):
```rust
crate::app_runtime_launch_health_probe::spawn_healthy_writer(health_probe.as_ref());
```

**Net delta**: -12 lines.

### Task 6 — Verify

```bash
cargo check -p oneshim-app                                           # expect clean
cargo clippy -p oneshim-app --bin oneshim --no-deps -- -D warnings   # expect clean
cargo test -p oneshim-app --lib updater::health_probe                # expect pass
cargo fmt --check                                                    # expect clean
wc -l src-tauri/src/app_runtime_launch.rs                            # expect ~900
wc -l src-tauri/src/app_runtime_launch_health_probe.rs               # expect 160-200
```

## Anticipated failure modes

| Failure | Diagnosis | Fix |
|---------|-----------|-----|
| `execute_startup_probe` needs `update_control` but probe is called before `core_resources` | Move probe call to after `update_control` is extracted (line 168) | Reorder in Task 3 |
| `UpdateControl` not public from its module | Check `crate::update_control::UpdateControl` visibility | Promote if needed |
| `CURRENT_VERSION` not re-exported from `updater` mod | Already re-exported per `updater/mod.rs` | No fix needed |
| Rolled-back scan's tokio::spawn runs before UpdateControl is ready | The scan IS set_rolled_back's consumer; UpdateControl must exist when scan spawns. Currently it does — scan spawns after `update_control = ...` at line 167. | Preserve order in Task 3 |

## Self-review (Loop 2 gate)

- [x] Each step has exact line numbers + before/after
- [x] Failure modes anticipated
- [x] Verification commands with expected output
- [x] Reordering constraint (probe call moves after `update_control`) documented
- [x] Net LOC delta tallied: -75 -70 -12 = -157 lines from main file

**Gate passed.** Proceed to Loop 3 (Implementation).
