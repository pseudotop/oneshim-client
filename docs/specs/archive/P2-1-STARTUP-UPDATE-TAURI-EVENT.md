# P2-1: Startup Update Tauri Event

## Problem Statement

`UpdateRuntimeBuilder.build_and_spawn()` performs a startup update check (3s timeout, fire-and-forget) but only logs the result. The Tauri frontend cannot know about available updates at startup — it must poll via IPC or wait for the periodic coordinator re-check.

**Current behavior** (`update_runtime.rs:39-57`):
```rust
self.runtime_handle.spawn(async move {
    match tokio::time::timeout(Duration::from_secs(3), updater.check_for_updates()).await {
        Ok(Ok(UpdateCheckResult::Available { latest, .. })) => {
            info!("startup update check: v{latest} available");  // ← log only
        }
        // ...
    }
});
```

**Desired behavior**: When `Available` is detected, emit a Tauri event so the frontend can display an update banner immediately.

## Architecture Context

```
UpdateRuntimeBuilder         (no AppHandle — Tauri-agnostic)
  ├── startup check          (fire-and-forget, 3s timeout, logs only)
  └── coordinator loop       (writes state, publishes to broadcast::Sender<UpdateStatus>)

LaunchCoreResourcesBuilder   (has AppHandle)
  └── creates UpdateRuntimeBuilder

BackgroundRuntimeCoordinator (has AppHandle + runtime_handle)
  └── spawn_runtime_bridges()
      └── spawn_realtime_event_bridge()  ← existing pattern

UpdateControl                (broadcast channel + shared state)
  ├── event_tx: broadcast::Sender<UpdateStatus>  ← coordinator publishes here
  ├── state: Arc<RwLock<UpdateStatus>>
  └── subscribe() → broadcast::Receiver<UpdateStatus>
```

**Key fact**: The coordinator already publishes `UpdateStatus` to `event_tx` on every state change. The SSE stream handler (`oneshim-web`) subscribes to this. The startup check does NOT publish to this channel.

## Design Decision

**Approach: Broadcast bridge** (follows `spawn_realtime_event_bridge` pattern)

1. Startup check publishes to existing `event_tx` broadcast channel (+ writes to shared state)
2. New `spawn_update_event_bridge()` subscribes to broadcast and emits Tauri events
3. Bridge is wired in `app_runtime_launch.rs`

**Why not pass `AppHandle` to `UpdateRuntimeBuilder`?**
`update_runtime.rs` has zero Tauri imports. Keeping it Tauri-agnostic preserves testability and separation. The bridge pattern is already established.

**Why not replace startup check with `CheckNow` action?**
Sending `CheckNow` to the coordinator after spawn could double-check if `should_check_for_updates()` also runs. The standalone startup check with 3s timeout is intentional — it's a fast, unconditional probe that doesn't interfere with the coordinator's throttle-based schedule.

## Implementation Design

### 1. Startup check → publish to broadcast + write state

**File**: `src-tauri/src/update_runtime.rs`

The startup task clones `event_tx` and `state` from `UpdateControl`. On `Available`, it writes `PendingApproval` to shared state and publishes to broadcast — matching coordinator's `run_check()` behavior.

New imports needed: `UpdatePhase`, `PendingUpdateInfo` (re-exported from `oneshim_web::update_control`).

```rust
if self.config.enabled {
    let startup_config = self.config.clone();
    let startup_event_tx = update_control.event_tx.clone();
    let startup_state = update_control.state.clone();
    self.runtime_handle.spawn(async move {
        let updater = Updater::new(startup_config);
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            updater.check_for_updates(),
        ).await {
            Ok(Ok(UpdateCheckResult::Available { current, latest, release, download_url })) => {
                info!("startup update check: v{latest} available");
                let mut guard = startup_state.write().await;
                guard.phase = UpdatePhase::PendingApproval;
                guard.message = Some(format!("{current} -> {latest}"));
                guard.pending = Some(PendingUpdateInfo {
                    current_version: current.to_string(),
                    latest_version: latest.to_string(),
                    release_url: release.html_url.clone(),
                    release_name: release.name.clone(),
                    published_at: release.published_at.clone(),
                    download_url,
                });
                guard.touch();
                let _ = startup_event_tx.send(guard.clone());
            }
            Ok(Ok(UpdateCheckResult::UpToDate { .. })) => {
                debug!("startup update check: up to date");
            }
            _ => {
                debug!("startup update check: skipped");
            }
        }
    });
    // ... coordinator spawn unchanged
}
```

### 2. Add `spawn_update_event_bridge()`

**File**: `src-tauri/src/runtime_bridges.rs`

```rust
pub(crate) fn spawn_update_event_bridge(
    handle: &Handle,
    app_handle: &AppHandle,
    update_control: &UpdateControl,
) {
    let app = app_handle.clone();
    let mut rx = update_control.subscribe();
    handle.spawn(async move {
        while let Ok(status) = rx.recv().await {
            if let Err(e) = app.emit_to("main", "update:status-changed", &status) {
                tracing::debug!("update event emit error (window may be hidden): {e}");
            }
        }
    });
}
```

New import: `use oneshim_web::update_control::UpdateControl;`

**Event name**: `update:status-changed` — covers ALL update status changes (startup, coordinator, user actions). Not just startup-specific.

**Payload**: `UpdateStatus` (already `Serialize + Clone`).

### 3. Wire bridge in `app_runtime_launch.rs`

**File**: `src-tauri/src/app_runtime_launch.rs`

Call directly after `spawn_runtime_bridges()` at line 382. All required references are available:
- `handle` (tokio runtime, from bootstrap destructure at line 48)
- `self.app_handle` (field on `AppRuntimeLaunchBuilder`)
- `update_control` (cloned at line 67)

```rust
core_resources.background_runtime.spawn_runtime_bridges();

// Forward update status changes to Tauri frontend
RuntimeBridgeSpawner::spawn_update_event_bridge(
    &handle,
    &self.app_handle,
    &update_control,
);
```

New import: `use crate::runtime_bridges::RuntimeBridgeSpawner;`

**Not** modifying `BackgroundRuntimeCoordinator` — it doesn't own `UpdateControl` and shouldn't. The bridge call sits naturally alongside `spawn_runtime_bridges()` at the same call site.

## Race Condition Analysis

The startup check (3s timeout) and coordinator initial check (`should_check_for_updates()`) run concurrently:

| Coordinator initial | Startup check | Result |
|---|---|---|
| Throttled (skips) | Available | Startup publishes → bridge emits → frontend shows banner |
| Throttled (skips) | Timeout/Error | No event — acceptable |
| Runs check | Available | Both publish `PendingApproval`. Frontend gets 2 events. Idempotent — `revision` increases monotonically |
| Runs check | Available | Both write to `state`. `RwLock` serializes writes. Last writer wins. Same data → no conflict |

**Double API call**: When both run, two GitHub API calls occur. This happens once at startup and is negligible.

**State consistency**: The `RwLock<UpdateStatus>` ensures atomic writes. Both write identical `PendingApproval` status. The `revision` counter (via `touch()`) distinguishes events.

## Files Changed

| File | Change | Lines |
|------|--------|-------|
| `src-tauri/src/update_runtime.rs` | Startup check publishes to broadcast + writes state | ~20 |
| `src-tauri/src/runtime_bridges.rs` | Add `spawn_update_event_bridge()` | ~15 |
| `src-tauri/src/app_runtime_launch.rs` | Wire bridge call + import | ~5 |

## Testing Strategy

1. **Existing coordinator tests** (3 tests in `update_coordinator.rs`) — must pass unchanged.
2. **New test in `update_runtime.rs`**: verify startup check with `Available` result publishes to `event_tx`. Create `UpdateControl`, spawn the startup check logic, assert `subscribe().recv()` yields `PendingApproval`.
3. **Bridge function**: requires Tauri runtime — verified manually (follows proven `spawn_realtime_event_bridge` pattern).

## Out of Scope

- Frontend TypeScript `listen('update:status-changed')` handler
- Tray menu changes (approve/defer already exist)
- OS notification for update available
