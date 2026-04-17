# Phase 2 — Config Change Bus + Telemetry Exporter Wiring: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a runtime config-change broadcast bus in `oneshim-core` and a feature-gated OpenTelemetry OTLP exporter wired to it in `src-tauri`.

**Architecture:** `ConfigManager` stores its source of truth inside `tokio::sync::watch::Sender<Arc<AppConfig>>` plus a `parking_lot::Mutex<()>` writer-lock. The only consumer wired in this phase is a Tokio task that forwards `TelemetryConfig` changes to a `TelemetryHandle` built around `tracing_subscriber::reload::Layer<Option<OtelLayer>, _>` — so the OTel layer can be attached, swapped, or detached live without restarting the process. The OTel pipeline, its deps, and all runtime export machinery live behind a `telemetry` Cargo feature on the `src-tauri` binary crate; default builds pay zero cost.

**Tech Stack:** Rust 2021, tokio 1 (watch/broadcast), parking_lot 0.12, tracing 0.1 + tracing-subscriber 0.3 (+ reload), tracing-appender 0.2, tracing-opentelemetry 0.28 (feature-gated), opentelemetry / opentelemetry_sdk / opentelemetry-otlp 0.27 (feature-gated), reqwest 0.13 (workspace pin), Axum 0.8 (for mock OTLP collector test harness), serde_json 1.

**Authoritative spec:** [`docs/reviews/2026-04-17-phase2-config-telemetry-spec.md`](./2026-04-17-phase2-config-telemetry-spec.md).

---

## Ground rules

- **TDD.** Every behavioural change lands as: failing test → minimum code → passing test → commit.
- **Plan tasks are finer-grained than spec §6 commits.** The spec bundles all X1 tests into one commit; this plan splits them across Tasks 1–4 so each test drives a discrete code change. The resulting 12 task-commits may be left as-is or squashed into the spec's 7 commits before opening the PR — behaviour and tests are equivalent. Engineer's preference.
- **Keep `cargo check --workspace`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` green on every commit.** If a step leaves the tree red, back out before committing.
- **Do NOT migrate any existing scheduler loop to `subscribe()` in this PR.** Spec §2.6 forbids it — audit-coalescing hazard.
- **If you need to use `block_in_place`, `tokio::task::spawn_blocking`, or `tokio::runtime::Handle::block_on`, stop and re-read this plan.** The only blocking calls in this PR are inside tests (`#[tokio::test]`) and the sync `parking_lot::Mutex` used for writer-lock — never hold that lock across an `.await`.

---

## Test placement convention

`src-tauri` has no `[lib]` target (only `[[bin]] name = "oneshim"`). Integration tests in `src-tauri/tests/*.rs` therefore compile as separate crates and cannot `use oneshim_app::telemetry::*`. All telemetry tests in this plan live **inline** as `#[cfg(test)] mod tests` blocks inside `src-tauri/src/telemetry/mod.rs` (and sibling modules when they fit better there). The mock OTLP collector is a `#[cfg(all(test, feature = "telemetry"))] mod mock_otlp;` inside `src/telemetry/`.

Every test invocation in this plan uses the form:

```bash
cargo test -p oneshim-app --features telemetry -- telemetry::tests::<test_name>
```

(or `--features telemetry` omitted for feature-off tests). The `--test <integration_target>` form from the Task-7 draft does NOT apply.

Running a feature-off test with the telemetry feature enabled is harmless — `#[cfg(not(feature = "telemetry"))]` attributes make them inert. Use `cargo test -p oneshim-app -- telemetry::tests` for the default path.

## File structure (new + modified)

### Created

| Path | Responsibility |
|------|----------------|
| `docs/architecture/ADR-016-config-change-bus.md` | Records the watch-channel + subscribe decision and the audit-coalescing hazard. |
| `docs/architecture/ADR-016-config-change-bus.ko.md` | Korean companion per `docs/DOCUMENTATION_POLICY.md`. |
| `docs/guides/telemetry.md` | End-user doc: what is collected, how to enable, collector endpoint, opt-out. |
| `docs/guides/telemetry.ko.md` | Korean companion. |
| `src-tauri/src/telemetry/mod.rs` | `TelemetryHandle`, `TelemetryLayer`, public API; feature-off no-op path. |
| `src-tauri/src/telemetry/otlp.rs` | `OtelLayer` alias, `OtlpPipeline`, `build`, `shutdown`, `resolve_endpoint`. `#[cfg(feature = "telemetry")]`. |
| `src-tauri/src/telemetry/instance_id.rs` | `telemetry_instance_id` file lifecycle (§3.7 state table). `#[cfg(feature = "telemetry")]`. |
| `src-tauri/src/telemetry/mock_otlp.rs` | Tiny Axum route on `127.0.0.1:<random>` that records POST bodies for T-X2-3 / T-X2-10. Gated `#[cfg(all(test, feature = "telemetry"))]` — lives inside `src/` because `src-tauri` has no `[lib]` target and integration tests in `tests/` cannot reach the `telemetry` module. |

### Modified

| Path | Changes |
|------|---------|
| `crates/oneshim-core/src/config_manager.rs` | Replace `Arc<RwLock<AppConfig>>` with `watch::Sender<Arc<AppConfig>>` + `parking_lot::Mutex<()>`; add `subscribe()` + `snapshot()`. |
| `crates/oneshim-core/src/config/sections/storage.rs` | Extend `TelemetryConfig` with `otlp_endpoint`, `sample_rate`, `service_name` + serde defaults. |
| `crates/oneshim-core/CLAUDE.md` | Document new public API surface. |
| `src-tauri/Cargo.toml` | Declare `telemetry` feature + optional OTel deps. |
| `src-tauri/src/main.rs` | Build `TelemetryHandle` + layer, attach to subscriber, spawn bus-driven toggle task. |
| `src-tauri/CLAUDE.md` | Note the new `telemetry/` module and `telemetry` feature. |
| `.github/workflows/ci.yml` (or whichever workflow file owns CI) | Add path-gated `--features telemetry` matrix cell. |
| `docs/STATUS.md` | Final commit only: update test totals and add "Telemetry feature binary size" row. |

---

## Dependency order between tasks

```
Task 0 (spike)  ──┐
                  ├── Task 1 ── Task 2 ── Task 3 ── Task 4 ── Task 5 (ADR)
                  │                                              │
                  └────────── Task 6 ── Task 7 ── Task 8 ── Task 9 ── Task 10 ── Task 11 ── Task 12 (docs)
```

Tasks 1–5 are X1 (ConfigChangeBus). Tasks 6–11 are X2 (telemetry). Task 12 is user docs. Task 0 is a throwaway verification spike that gates Task 7 (the first Cargo.toml change that actually pulls OTel crates).

---

## Task 0: Commit-0 verification spike (NOT committed)

**Purpose:** Confirm `opentelemetry` 0.27 + `opentelemetry-otlp` 0.27 with `http-proto` + `reqwest-client` features actually resolve against the workspace reqwest 0.13 / tonic 0.14 pins. If not, switch the plan to `grpc-tonic` before starting Task 7.

**Files:** None. This runs on a throwaway worktree or stash and is discarded.

**Steps:**

- [ ] **Step 1: Create a throwaway worktree.**

```bash
git stash --include-untracked
git worktree add /tmp/phase2-spike HEAD
cd /tmp/phase2-spike
```

- [ ] **Step 2: Attempt to add the OTel deps with HTTP/proto transport.**

Edit `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
opentelemetry = { version = "0.27", default-features = false, features = ["trace"] }
opentelemetry_sdk = { version = "0.27", default-features = false, features = ["rt-tokio", "trace"] }
opentelemetry-otlp = { version = "0.27", default-features = false, features = ["http-proto", "reqwest-client", "trace"] }
tracing-opentelemetry = "0.28"
```

Then run:

```bash
cargo update -p opentelemetry -p opentelemetry_sdk -p opentelemetry-otlp -p tracing-opentelemetry
cargo check -p oneshim-app 2>&1 | tee /tmp/phase2-spike-result.log
```

Expected: compile success, OR dep resolution failure. Record which.

- [ ] **Step 3: If Step 2 failed, try the `grpc-tonic` path.**

Replace the `opentelemetry-otlp` line with:

```toml
opentelemetry-otlp = { version = "0.27", default-features = false, features = ["grpc-tonic", "trace"] }
```

Re-run `cargo check`. Record outcome.

- [ ] **Step 4: Record the verified versions and transport in the plan.**

Back in the main worktree (exit the spike one), if the HTTP path succeeded append a line to this plan under Task 7: "Commit-0 verified HTTP/proto compat on YYYY-MM-DD; versions pinned as above."

If only grpc-tonic worked, update **every mention** of `http-proto` / `reqwest-client` in Task 7 to `grpc-tonic`, and note the pivot here.

- [ ] **Step 5: Clean up the spike.**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/features
git worktree remove /tmp/phase2-spike --force
git stash pop   # restore the unstaged progress file etc.
```

No commit. Delete `/tmp/phase2-spike-result.log` after recording outcome.

---

## Task 1: `ConfigManager` — swap in `watch::Sender` without changing public API

**Files:**
- Modify: `crates/oneshim-core/src/config_manager.rs`

Goal: internal storage becomes `watch::Sender<Arc<AppConfig>>` + `parking_lot::Mutex<()>`. `get()`, `update()`, `update_with()`, `reload()` keep the same signatures and visible behaviour. No new public API yet.

- [ ] **Step 1: Run baseline.**

```bash
cargo test -p oneshim-core config_manager 2>&1 | tail -20
```

Expected: all existing `config_manager` tests pass. Note the count.

Also confirm the clone-site inventory is still accurate before starting:

```bash
grep -rn "config_manager\.clone()\|ConfigManager::clone" \
  src-tauri/src crates/oneshim-web/src | wc -l
```

Expected: > 0 (the count is informational; the fix in Step 2 keeps `derive(Clone)` working at all of them).

- [ ] **Step 2: Replace the struct fields and `new`/`with_path` constructors.**

> **Why the outer Arc:** 20+ call sites across `src-tauri`, `oneshim-web`, and the scheduler loops already call `.clone()` on owned `ConfigManager` values (e.g., `crates/oneshim-web/src/web_contexts/mod.rs:72,93,120,145,202`, `src-tauri/src/app_runtime_launch.rs:383,532,733,745`, `src-tauri/src/scheduler/loops/{intelligence,monitor,system}.rs`). The pre-change struct was `#[derive(Clone)]` because `Arc<RwLock<AppConfig>>` clones cheaply. `watch::Sender` is not `Clone` and `parking_lot::Mutex<()>` is not `Clone`, so we move all state into a private `Arc<Inner>` and derive `Clone` on the outer shell. Every existing call site continues to work with zero edits.

In `crates/oneshim-core/src/config_manager.rs`, replace the top of the file:

```rust
use crate::config::AppConfig;
use crate::error::CoreError;
use parking_lot::Mutex;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{debug, info, warn};

const CONFIG_FILE_NAME: &str = "config.json";
const APP_DIR_NAME: &str = "oneshim";

#[derive(Debug, Clone)]
pub struct ConfigManager {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    sender: watch::Sender<Arc<AppConfig>>,
    writer_lock: Mutex<()>,
    config_path: PathBuf,
}
```

(`Inner` itself is neither `Clone` nor needs to be — the `Arc` around it is what every clone shares.)

- [ ] **Step 3: Replace `with_path` body.**

```rust
pub fn with_path(config_path: PathBuf) -> Result<Self, CoreError> {
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                CoreError::Config(format!(
                    "Failed to create config directory: {}: {}",
                    parent.display(),
                    e
                ))
            })?;
            info!("settings create: {}", parent.display());
        }
    }

    let initial = if config_path.exists() {
        match Self::load_from_file(&config_path) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    path = %config_path.display(),
                    error = %e,
                    "config file corrupted, falling back to defaults"
                );
                let default_config = AppConfig::default_config();
                if let Err(e) = Self::save_to_file(&config_path, &default_config) {
                    debug!("save_to_file failed: {e}");
                }
                default_config
            }
        }
    } else {
        let default_config = AppConfig::default_config();
        Self::save_to_file(&config_path, &default_config)?;
        info!("default settings file create: {}", config_path.display());
        default_config
    };

    let (sender, _rx) = watch::channel(Arc::new(initial));
    // Dropping `_rx` here is fine — `Sender` does not require any receivers to exist.
    // `subscribe()` lazily creates them.

    Ok(Self {
        inner: Arc::new(Inner {
            sender,
            writer_lock: Mutex::new(()),
            config_path,
        }),
    })
}
```

- [ ] **Step 4: Rewrite `get`, `update`, `update_with`, `reload`.**

Each method now deref-walks through `self.inner`.

```rust
pub fn get(&self) -> AppConfig {
    (*self.inner.sender.borrow()).clone()
}

pub fn update(&self, new_config: AppConfig) -> Result<(), CoreError> {
    let _guard = self.inner.writer_lock.lock();
    Self::save_to_file(&self.inner.config_path, &new_config)?;
    self.inner.sender.send_replace(Arc::new(new_config));
    debug!("settings save complete: {}", self.inner.config_path.display());
    Ok(())
}

pub fn update_with<F>(&self, updater: F) -> Result<AppConfig, CoreError>
where
    F: FnOnce(&mut AppConfig) -> Result<(), String>,
{
    let _guard = self.inner.writer_lock.lock();
    let mut new_cfg = (**self.inner.sender.borrow()).clone();
    updater(&mut new_cfg).map_err(CoreError::Config)?;
    Self::save_to_file(&self.inner.config_path, &new_cfg)?;
    let snapshot = new_cfg.clone();
    self.inner.sender.send_replace(Arc::new(new_cfg));
    debug!("settings save complete: {}", self.inner.config_path.display());
    Ok(snapshot)
}

pub fn config_path(&self) -> &PathBuf {
    &self.inner.config_path
}

pub fn reload(&self) -> Result<(), CoreError> {
    let _guard = self.inner.writer_lock.lock();
    let reloaded = Self::load_from_file(&self.inner.config_path)?;
    self.inner.sender.send_replace(Arc::new(reloaded));
    info!("settings load complete");
    Ok(())
}
```

Leave `load_from_file`, `save_to_file`, `config_dir`, `data_dir`, `default_config_path` unchanged.

- [ ] **Step 5: Run the existing tests.**

```bash
cargo test -p oneshim-core config_manager 2>&1 | tail -20
```

Expected: same count of pre-existing tests pass. If any fail, the mutation-path rewrite is wrong; fix before continuing.

- [ ] **Step 6: `cargo check` + clippy workspace.**

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: green.

- [ ] **Step 7: Commit.**

```bash
git add crates/oneshim-core/src/config_manager.rs
git commit -m "refactor(core): back ConfigManager with tokio::sync::watch

Internal storage moves from Arc<RwLock<AppConfig>> to
watch::Sender<Arc<AppConfig>> + parking_lot::Mutex<()> writer-lock.

Public API is unchanged. No consumers affected. Subscribe/snapshot
API added in the next commit.

Part of Phase 2 (X1 ConfigChangeBus).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 2: `subscribe()` + `snapshot()` public API (T-X1-1, T-X1-5, T-X1-6)

**Files:**
- Modify: `crates/oneshim-core/Cargo.toml` (add tokio to dev-dependencies for `#[tokio::test]`)
- Modify: `crates/oneshim-core/src/config_manager.rs` (add methods + tests)

- [ ] **Step 0: Add tokio + macros to `[dev-dependencies]`.**

Edit `crates/oneshim-core/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
criterion = { workspace = true }
tokio = { workspace = true, features = ["macros", "rt", "rt-multi-thread", "time"] }
```

(`workspace = true` picks up the root-Cargo pin of tokio 1 with `features = ["full"]`; the dev-deps line explicitly requests the subset we need for tests. Since the workspace line is `tokio = { version = "1", features = ["full"] }` with no `workspace.dependencies`, the simpler alternative is `tokio = { version = "1", features = ["macros", "rt", "rt-multi-thread", "time"] }`. Pick whichever matches how other crates declare tokio in dev-deps.)

Run `cargo check -p oneshim-core --tests 2>&1 | tail -5` — expect green.

- [ ] **Step 1: Write failing test `subscribe_sees_initial_value` (T-X1-1).**

Add to the `#[cfg(test)] mod tests` block in `config_manager.rs`:

```rust
#[test]
fn subscribe_sees_initial_value() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path).unwrap();

    let rx = mgr.subscribe();
    let snapshot = rx.borrow();
    // After a fresh ConfigManager, the first snapshot must equal the current get().
    let current = mgr.get();
    assert_eq!(snapshot.telemetry.enabled, current.telemetry.enabled);
}
```

- [ ] **Step 2: Run — expect compile error (`subscribe` does not exist).**

```bash
cargo test -p oneshim-core config_manager::tests::subscribe_sees_initial_value 2>&1 | tail -10
```

Expected: `error[E0599]: no method named 'subscribe'`.

- [ ] **Step 3: Add `subscribe()` and `snapshot()`.**

Insert after `get()`:

```rust
/// Subscribe to whole-config change notifications.
///
/// The receiver starts at the current config. `changed().await` resolves after
/// the next `update` / `update_with` / `reload`. Dropping a receiver does not
/// affect any other subscriber.
pub fn subscribe(&self) -> watch::Receiver<Arc<AppConfig>> {
    self.inner.sender.subscribe()
}

/// Cheap read-only snapshot of the current config.
///
/// Equivalent to `subscribe().borrow().clone()` without registering a subscriber.
pub fn snapshot(&self) -> Arc<AppConfig> {
    self.inner.sender.borrow().clone()
}
```

- [ ] **Step 4: Run the test — expect pass.**

```bash
cargo test -p oneshim-core config_manager::tests::subscribe_sees_initial_value 2>&1 | tail -10
```

Expected: 1 passed.

- [ ] **Step 5: Add T-X1-5 `dropped_receiver_does_not_block_sender`.**

```rust
#[tokio::test]
async fn dropped_receiver_does_not_block_sender() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path).unwrap();

    let rx_a = mgr.subscribe();
    let rx_b = mgr.subscribe();
    drop(rx_a);

    // Should return Ok — rx_b is still alive.
    mgr.update_with(|c| {
        c.telemetry.enabled = !c.telemetry.enabled;
        Ok(())
    })
    .expect("update_with must not fail when one receiver was dropped");

    // The survivor sees the new value on demand.
    let mut rx_b = rx_b;
    // `changed().await` would fire here; we synchronously check via borrow_and_update.
    let _ = rx_b.borrow_and_update();
}
```

- [ ] **Step 6: Add T-X1-6 `snapshot_matches_latest_update`.**

```rust
#[test]
fn snapshot_matches_latest_update() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path).unwrap();

    mgr.update_with(|c| {
        c.telemetry.enabled = true;
        Ok(())
    })
    .unwrap();

    let via_snapshot = mgr.snapshot();
    let via_subscribe = mgr.subscribe();
    let via_borrow = via_subscribe.borrow();

    // They must be pointer-equal (same Arc) or at least value-equal.
    assert_eq!(via_snapshot.telemetry.enabled, via_borrow.telemetry.enabled);
    assert!(via_snapshot.telemetry.enabled);
}
```

- [ ] **Step 7: Run the new tests.**

```bash
cargo test -p oneshim-core config_manager::tests -- \
  subscribe_sees_initial_value \
  dropped_receiver_does_not_block_sender \
  snapshot_matches_latest_update 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 8: Clippy + commit.**

```bash
cargo clippy -p oneshim-core --all-targets -- -D warnings
git add crates/oneshim-core/src/config_manager.rs
git commit -m "feat(core): add ConfigManager::subscribe and snapshot APIs

Additive API on top of the watch-backed ConfigManager landed in the
previous commit. Existing callers are unchanged.

Tests: T-X1-1, T-X1-5, T-X1-6.

Part of Phase 2 (X1 ConfigChangeBus).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 3: Notification tests T-X1-2 / T-X1-3 / T-X1-4 / T-X1-7

**Files:**
- Modify: `crates/oneshim-core/src/config_manager.rs`

Every mutation path is already sending on the channel (Task 1). These tests lock that behaviour in.

- [ ] **Step 1: Add T-X1-2, T-X1-3, T-X1-4.**

```rust
#[tokio::test]
async fn update_notifies_subscribers() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path).unwrap();

    let mut rx = mgr.subscribe();
    let before = rx.borrow_and_update().telemetry.enabled;

    let mut new_cfg = mgr.get();
    new_cfg.telemetry.enabled = !before;
    mgr.update(new_cfg).unwrap();

    rx.changed().await.expect("changed() must resolve after update()");
    assert_ne!(rx.borrow().telemetry.enabled, before);
}

#[tokio::test]
async fn update_with_notifies_subscribers() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path).unwrap();

    let mut rx = mgr.subscribe();
    let before = rx.borrow_and_update().telemetry.enabled;

    mgr.update_with(|c| {
        c.telemetry.enabled = !before;
        Ok(())
    })
    .unwrap();

    rx.changed().await.expect("changed() must resolve after update_with()");
    assert_ne!(rx.borrow().telemetry.enabled, before);
}

#[tokio::test]
async fn reload_notifies_subscribers() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path.clone()).unwrap();

    let mut rx = mgr.subscribe();
    let before = rx.borrow_and_update().telemetry.enabled;

    // Rewrite the file out-of-band so reload() observes a different value.
    let mut forced = AppConfig::default_config();
    forced.telemetry.enabled = !before;
    let json = serde_json::to_string_pretty(&forced).unwrap();
    std::fs::write(&cfg_path, json).unwrap();

    mgr.reload().unwrap();
    rx.changed().await.expect("changed() must resolve after reload()");
    assert_ne!(rx.borrow().telemetry.enabled, before);
}
```

- [ ] **Step 2: Add T-X1-7 `no_spurious_wakeup_when_content_identical`.**

```rust
#[tokio::test]
async fn each_update_fires_even_for_identical_content() {
    // Pins `watch` semantics: consumers are responsible for diffing.
    // See ADR-016 for the audit-coalescing hazard.
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path).unwrap();

    let mut rx = mgr.subscribe();
    rx.borrow_and_update(); // consume initial

    let cfg = mgr.get();
    mgr.update(cfg.clone()).unwrap();
    rx.changed().await.expect("first identical update still fires");
    rx.borrow_and_update();

    mgr.update(cfg).unwrap();
    rx.changed().await.expect("second identical update still fires");
}
```

- [ ] **Step 3: Run the four new tests.**

```bash
cargo test -p oneshim-core config_manager::tests -- \
  update_notifies_subscribers \
  update_with_notifies_subscribers \
  reload_notifies_subscribers \
  each_update_fires_even_for_identical_content 2>&1 | tail -10
```

Expected: 4 passed.

- [ ] **Step 4: Clippy + commit.**

```bash
cargo clippy -p oneshim-core --all-targets -- -D warnings
git add crates/oneshim-core/src/config_manager.rs
git commit -m "test(core): notification semantics for ConfigChangeBus

Tests: T-X1-2, T-X1-3, T-X1-4, T-X1-7.

T-X1-7 pins the latest-wins semantics: identical-content updates still
fire a notification. Consumer-side diffing is the caller's job (see
ADR-016).

Part of Phase 2 (X1 ConfigChangeBus).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 4: Reentrancy + sender-drop tests (T-X1-9, T-X1-10)

**Files:**
- Modify: `crates/oneshim-core/src/config_manager.rs`

- [ ] **Step 1: Add T-X1-9 `update_with_does_not_reenter`.**

```rust
#[test]
fn update_with_does_not_reenter() {
    // The update_with closure must be able to call get()/snapshot() without
    // deadlocking. writer_lock must not be held across those calls.
    use std::sync::atomic::{AtomicBool, Ordering};
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path).unwrap();

    let saw_snapshot = AtomicBool::new(false);

    mgr.update_with(|c| {
        // These calls read the `watch` value, which uses its own internal
        // synchronisation separate from writer_lock.
        let _ = mgr.snapshot();
        let _ = mgr.get();
        saw_snapshot.store(true, Ordering::SeqCst);
        c.telemetry.enabled = true;
        Ok(())
    })
    .unwrap();

    assert!(saw_snapshot.load(Ordering::SeqCst));
    assert!(mgr.get().telemetry.enabled);
}
```

- [ ] **Step 2: Add T-X1-10 `receiver_changed_returns_err_after_manager_dropped`.**

```rust
#[tokio::test]
async fn receiver_changed_returns_err_after_manager_dropped() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = ConfigManager::with_path(cfg_path).unwrap();

    let mut rx = mgr.subscribe();

    let handle = tokio::spawn(async move {
        // `changed()` resolves to Err once the sender is dropped.
        rx.changed().await
    });

    drop(mgr);
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), handle)
        .await
        .expect("task must not hang")
        .expect("task must not panic");

    assert!(result.is_err(), "expected Err from changed() after sender dropped");
}
```

- [ ] **Step 3: Run.**

```bash
cargo test -p oneshim-core config_manager::tests -- \
  update_with_does_not_reenter \
  receiver_changed_returns_err_after_manager_dropped 2>&1 | tail -10
```

Expected: 2 passed.

- [ ] **Step 4: Clippy + commit.**

```bash
cargo clippy -p oneshim-core --all-targets -- -D warnings
git add crates/oneshim-core/src/config_manager.rs
git commit -m "test(core): writer_lock non-reentrancy and sender-drop shutdown

Tests: T-X1-9, T-X1-10.

T-X1-9 proves get()/snapshot() inside an update_with closure do not
deadlock against writer_lock. T-X1-10 pins the bus-shutdown story for
the telemetry bootstrap task.

Part of Phase 2 (X1 ConfigChangeBus).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 5: ADR-016

**Files:**
- Create: `docs/architecture/ADR-016-config-change-bus.md`
- Create: `docs/architecture/ADR-016-config-change-bus.ko.md`

- [ ] **Step 1: Write ADR-016 (English).**

Copy the template from an existing ADR (e.g., `ADR-014-tauri-managed-state-boundary.md`) and replace the body. Required sections: Status, Context, Decision, Consequences, Alternatives, References.

Use this body verbatim (add project-specific examples if useful, but every section below must ship):

```markdown
# ADR-016: Config Change Bus

## Status
Accepted — 2026-04-17

## Context
Before this ADR, `ConfigManager` (in `oneshim-core`) held `Arc<RwLock<AppConfig>>` and exposed only polled reads via `get()`. Every consumer that needed to react to a user-driven settings change cached its own previous snapshot and re-read on its own tick. The seven scheduler loops in `src-tauri/src/scheduler/loops/` each reimplemented the dirty-check pattern differently; some consumers (`oneshim-vision::privacy`, `oneshim-analysis::regime_manager`) cached sections at init and never saw later changes at all. A toggle in the settings UI took 1–30 s to reach each consumer. See `docs/reviews/2026-04-17-phase2-config-telemetry-spec.md §1.1` for the full inventory.

This coupling also blocked the telemetry exporter work (X2): the OTel layer lifecycle has to swap on a runtime `telemetry.enabled` change, and polling every second from inside `main.rs` was ugly.

## Decision
`ConfigManager` now owns `watch::Sender<Arc<AppConfig>>` and exposes `subscribe()` returning `watch::Receiver<Arc<AppConfig>>`. Read-only snapshots are available via `snapshot()`. Mutations are serialised by a `parking_lot::Mutex<()>` writer-lock. Existing callers that used `get()`/`update()`/`update_with()`/`reload()` are unchanged.

## Consequences
### Positive
- Subscribers wake on mutation; no per-consumer polling scaffold.
- Readers never block writers.

### Negative — audit-coalescing hazard
`watch` has latest-wins semantics. Rapid A→B→A updates may collapse into a
single wake-up where the subscriber sees only the final state. **Consumers
whose correctness depends on observing every intermediate transition
(compliance audit logs, metrics counters) must keep their existing
poll-and-diff structure, or adopt `broadcast` instead.** The one production
example today is
`src-tauri/src/scheduler/loops/helpers.rs::audit_consent_and_pii_changes`,
which is deliberately NOT migrated in Phase 2.

### Neutral
- Subscribers who only need the latest state can just call `snapshot()`.

## Alternatives considered
- `tokio::sync::broadcast` — rejected, adds Lagged handling and per-subscriber
  queue sizing for no capability gain.
- Per-section channels — rejected, explodes API surface; diffing in consumer
  is cheap.
- `arc_swap::ArcSwap` polling — rejected, gives no wake-up signal.

## References
- Spec: `docs/reviews/2026-04-17-phase2-config-telemetry-spec.md`
- Feature gap analysis: `docs/reviews/2026-04-16-feature-gaps-analysis.md` X1
- ADR-001 (Rust architecture patterns) — Hexagonal boundary compliance.
```

- [ ] **Step 2: Write the Korean companion.**

Translate the English doc to Korean in the same folder with suffix `.ko.md`. Keep structure identical per `docs/DOCUMENTATION_POLICY.md`. If any English technical term has no good Korean equivalent (e.g., "watch channel"), keep the English in parens after the Korean.

- [ ] **Step 3: Commit both files.**

```bash
git add docs/architecture/ADR-016-config-change-bus.md docs/architecture/ADR-016-config-change-bus.ko.md
git commit -m "docs(arch): ADR-016 for ConfigChangeBus

Records the watch-backed ConfigManager decision and the audit-coalescing
hazard that blocks Phase 3 consumer migrations without per-consumer review.

Part of Phase 2 (X1 ConfigChangeBus).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 6: TelemetryConfig field extension (T-X1-8, T-X2-6)

**Files:**
- Modify: `crates/oneshim-core/src/config/sections/storage.rs`
- Modify: `crates/oneshim-core/src/config_manager.rs` (tests only)

- [ ] **Step 1: Write failing T-X1-8 (still in `config_manager.rs` tests).**

```rust
#[test]
fn deserialises_legacy_config_json_without_new_telemetry_fields() {
    // A config.json predating Phase 2 will not have otlp_endpoint,
    // sample_rate, or service_name. Serde defaults must cover them.
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let legacy = r#"{
      "telemetry": {
        "enabled": false,
        "crash_reports": false,
        "usage_analytics": false,
        "performance_metrics": false
      }
    }"#;
    std::fs::write(&cfg_path, legacy).unwrap();

    let mgr = ConfigManager::with_path(cfg_path).expect("legacy JSON must deserialise");
    let cfg = mgr.get();
    assert_eq!(cfg.telemetry.otlp_endpoint, None);
    assert!((cfg.telemetry.sample_rate - 1.0).abs() < f64::EPSILON);
    assert_eq!(cfg.telemetry.service_name, "oneshim-client");
}
```

- [ ] **Step 2: Write failing T-X2-6.**

```rust
#[test]
fn telemetry_enabled_defaults_to_false() {
    let cfg = AppConfig::default_config();
    assert!(!cfg.telemetry.enabled);
}
```

- [ ] **Step 3: Run — expect compile errors on the three new fields.**

```bash
cargo test -p oneshim-core config_manager::tests::deserialises_legacy_config_json_without_new_telemetry_fields 2>&1 | tail -10
```

Expected: `error[E0609]: no field 'otlp_endpoint'`.

- [ ] **Step 4: Extend `TelemetryConfig`.**

In `crates/oneshim-core/src/config/sections/storage.rs` replace the existing block:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub crash_reports: bool,
    #[serde(default)]
    pub usage_analytics: bool,
    #[serde(default)]
    pub performance_metrics: bool,
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
    #[serde(default = "default_service_name")]
    pub service_name: String,
}

fn default_sample_rate() -> f64 {
    1.0
}

fn default_service_name() -> String {
    "oneshim-client".to_string()
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            crash_reports: false,
            usage_analytics: false,
            performance_metrics: false,
            otlp_endpoint: None,
            sample_rate: default_sample_rate(),
            service_name: default_service_name(),
        }
    }
}
```

(The previous `#[derive(Default)]` is removed because three fields need non-trivial defaults.)

- [ ] **Step 5: Run all tests.**

```bash
cargo test -p oneshim-core 2>&1 | tail -15
```

Expected: all pass, including T-X1-8 and T-X2-6.

- [ ] **Step 6: Clippy + commit.**

```bash
cargo clippy --workspace --all-targets -- -D warnings
git add crates/oneshim-core/src/config/sections/storage.rs crates/oneshim-core/src/config_manager.rs
git commit -m "feat(core): extend TelemetryConfig with otlp_endpoint/sample_rate/service_name

Serde defaults preserve backward compatibility with existing config.json
files on upgrade.

Tests: T-X1-8 (legacy JSON), T-X2-6 (opt-in default false).

Part of Phase 2 (X2 Telemetry exporter wiring).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 7: Add `telemetry` feature, OTel deps, module skeleton (T-X2-1)

> **Precondition:** Task 0 spike outcome is recorded. Use the verified versions and transport.

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/telemetry/mod.rs`
- Modify: `src-tauri/src/main.rs` (add `mod telemetry;`)

- [ ] **Step 1: Declare the feature and optional deps.**

In `src-tauri/Cargo.toml`, find the `[features]` block (create if missing) and add:

```toml
[features]
default = []
telemetry = [
    "dep:opentelemetry",
    "dep:opentelemetry_sdk",
    "dep:opentelemetry-otlp",
    "dep:tracing-opentelemetry",
]
```

Under `[dependencies]`, append:

```toml
opentelemetry = { version = "0.27", optional = true, default-features = false, features = ["trace"] }
opentelemetry_sdk = { version = "0.27", optional = true, default-features = false, features = ["rt-tokio", "trace"] }
opentelemetry-otlp = { version = "0.27", optional = true, default-features = false, features = ["http-proto", "reqwest-client", "trace"] }
tracing-opentelemetry = { version = "0.28", optional = true }
```

(If Task 0 forced the grpc-tonic path, swap `http-proto, reqwest-client` for `grpc-tonic`.)

- [ ] **Step 2: Write failing T-X2-1 `feature_off_init_is_noop` inline.**

Append to `src-tauri/src/telemetry/mod.rs` (end of file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::sections::storage::TelemetryConfig;

    #[test]
    #[cfg(not(feature = "telemetry"))]
    fn feature_off_construction_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TelemetryConfig {
            enabled: true,
            ..Default::default()
        };
        let (_layer, handle) = Handle::new_with_layer(&cfg, tmp.path())
            .expect("feature-off construction is infallible in practice");
        handle.apply(&cfg).expect("apply is a no-op when feature is off");
    }
}
```

- [ ] **Step 3: Run — expect compile error (the `telemetry` module is not declared yet in `main.rs`).**

```bash
cargo test -p oneshim-app -- telemetry::tests::feature_off_construction_is_noop 2>&1 | tail -10
```

Expected: `error[E0583]: file not found for module 'telemetry'` or similar, until Step 5 registers the module.

- [ ] **Step 4: Create the skeleton module (feature-off branch only).**

`src-tauri/src/telemetry/mod.rs`:

```rust
//! Telemetry bootstrap.
//!
//! Feature-off (`default`): zero-cost no-op layer and handle.
//! Feature-on (`telemetry`): OpenTelemetry OTLP pipeline behind a
//! `tracing_subscriber::reload::Layer`. See docs/guides/telemetry.md.

use oneshim_core::config::sections::storage::TelemetryConfig;

#[cfg(feature = "telemetry")]
mod instance_id;
#[cfg(feature = "telemetry")]
mod otlp;

/// Public handle for runtime toggle. Construct via `Handle::new_with_layer`.
pub struct Handle {
    #[cfg(feature = "telemetry")]
    inner: parking_lot::Mutex<otlp::Inner>,
}

/// Layer attached to the tracing subscriber. Type alias keeps the `.with()`
/// call site monomorphic across feature states.
#[cfg(feature = "telemetry")]
pub type Layer = tracing_subscriber::reload::Layer<
    Option<otlp::OtelLayer>,
    tracing_subscriber::Registry,
>;

#[cfg(not(feature = "telemetry"))]
pub type Layer = NoopLayer;

#[cfg(not(feature = "telemetry"))]
#[derive(Clone, Copy, Default)]
pub struct NoopLayer;

#[cfg(not(feature = "telemetry"))]
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for NoopLayer {}

impl Handle {
    /// Feature-off: infallible, `data_dir` ignored.
    /// Feature-on: fallible, uses `data_dir` to resolve/create `telemetry_instance_id`.
    ///
    /// Signature is the same across feature states — callers pass `data_dir` in both,
    /// and the feature-off branch drops it. This keeps `main.rs` and tests identical
    /// regardless of which build they compile under.
    pub fn new_with_layer(
        _cfg: &TelemetryConfig,
        _data_dir: &std::path::Path,
    ) -> anyhow::Result<(Layer, Self)> {
        #[cfg(not(feature = "telemetry"))]
        {
            Ok((NoopLayer, Handle {}))
        }
        #[cfg(feature = "telemetry")]
        {
            otlp::build_initial_handle(_cfg, _data_dir)
        }
    }

    pub fn apply(&self, _cfg: &TelemetryConfig) -> anyhow::Result<()> {
        #[cfg(not(feature = "telemetry"))]
        {
            Ok(())
        }
        #[cfg(feature = "telemetry")]
        {
            self.inner.lock().apply(_cfg)
        }
    }
}
```

Also create a **stub** `src-tauri/src/telemetry/otlp.rs`. Task 8 replaces this stub in full — the only job of this version is to make `cargo check --features telemetry` compile. No feature-on test runs against this stub.

```rust
#![cfg(feature = "telemetry")]

//! Stub — Task 8 replaces this with the real OTLP pipeline.
//!
//! The type aliases and function signatures here are pinned by mod.rs; Task 8
//! must preserve them (`Inner::apply`, `OtelLayer`, `build_initial_handle`).

use crate::telemetry::{Handle, Layer};
use oneshim_core::config::sections::storage::TelemetryConfig;

pub(super) struct Inner;
// Cheap stand-in satisfying `Layer<Registry>` until the real OtelLayer lands.
pub(super) type OtelLayer = tracing_subscriber::fmt::Layer<tracing_subscriber::Registry>;

impl Inner {
    pub(super) fn apply(&mut self, _cfg: &TelemetryConfig) -> anyhow::Result<()> {
        Ok(())
    }
}

pub(super) fn build_initial_handle(
    _cfg: &TelemetryConfig,
    _data_dir: &std::path::Path,
) -> anyhow::Result<(Layer, Handle)> {
    unimplemented!("Task 8 wires the real pipeline")
}
```

And a stub `src-tauri/src/telemetry/instance_id.rs`:

```rust
#![cfg(feature = "telemetry")]
// Implementation lands in Task 9.
```

- [ ] **Step 5: Register the module in `main.rs`.**

Add near the other `mod` declarations at the top of `src-tauri/src/main.rs`:

```rust
pub mod telemetry;
```

- [ ] **Step 6: Run T-X2-1 with default features.**

```bash
cargo test -p oneshim-app -- telemetry::tests::feature_off_construction_is_noop 2>&1 | tail -10
```

Expected: 1 passed.

- [ ] **Step 7: Verify the feature gate compiles.**

```bash
cargo check -p oneshim-app --features telemetry 2>&1 | tail -10
```

Expected: green. (The `unimplemented!()` body compiles; it only panics at runtime.)

- [ ] **Step 8: Clippy both feature states.**

```bash
cargo clippy -p oneshim-app --all-targets -- -D warnings
cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings
```

Expected: both green.

- [ ] **Step 9: Commit.**

```bash
git add src-tauri/Cargo.toml src-tauri/src/telemetry/ src-tauri/src/main.rs Cargo.lock
git commit -m "feat(app): introduce telemetry feature + module skeleton

Adds a Cargo feature 'telemetry' on the oneshim-app binary crate,
optional OTel deps, and a no-op feature-off path that proves the
feature gate compiles cleanly.

Real OTLP pipeline and reload-layer wiring land in the next commits.

Tests: T-X2-1.

Part of Phase 2 (X2 Telemetry exporter wiring).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 8: OTLP pipeline + reload-layer wiring (T-X2-2, T-X2-3, T-X2-4, T-X2-7)

**Files:**
- Modify: `src-tauri/src/telemetry/otlp.rs` (replace the stub with real implementation)
- Modify: `src-tauri/src/telemetry/mod.rs` (append feature-on tests to the existing `#[cfg(test)] mod tests`)
- Create: `src-tauri/src/telemetry/mock_otlp.rs` (test-only + feature-gated — see Test placement convention)

- [ ] **Step 1: Write failing T-X2-2 `feature_on_config_off_installs_empty_reload_wrapper` inline.**

Append inside the existing `mod tests` in `src-tauri/src/telemetry/mod.rs`:

```rust
    #[test]
    #[cfg(feature = "telemetry")]
    fn feature_on_config_off_installs_empty_reload_wrapper() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = TelemetryConfig {
            enabled: false,
            ..Default::default()
        };
        let (_layer, _handle) = Handle::new_with_layer(&cfg, tmp.path())
            .expect("disabled-at-boot never fails");
        // No panic, no network activity. Inner `Option<OtelLayer>` is None.
    }
```

- [ ] **Step 2: Implement `otlp::build_initial_handle` and `Inner::apply` for real.**

Replace `src-tauri/src/telemetry/otlp.rs`:

```rust
#![cfg(feature = "telemetry")]

use crate::telemetry::{Handle, Layer};
use oneshim_core::config::sections::storage::TelemetryConfig;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{trace as sdktrace, Resource};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::reload;

pub(super) type OtelLayer =
    OpenTelemetryLayer<tracing_subscriber::Registry, sdktrace::Tracer>;

pub(super) struct Inner {
    reload_handle: reload::Handle<Option<OtelLayer>, tracing_subscriber::Registry>,
    active: Option<sdktrace::SdkTracerProvider>,
    last_cfg: TelemetryConfig,
}

impl Inner {
    pub(super) fn apply(&mut self, cfg: &TelemetryConfig) -> anyhow::Result<()> {
        use std::cmp::Ordering;
        let transition = match (self.last_cfg.enabled, cfg.enabled) {
            (false, true) => 1,
            (true, false) => -1,
            _ => 0,
        };
        match transition.cmp(&0) {
            Ordering::Greater => {
                // off -> on: build a new pipeline and swap in.
                let (provider, layer) = build_pipeline(cfg, &self.data_dir)?;
                self.reload_handle
                    .modify(|opt| *opt = Some(layer))
                    .map_err(|e| anyhow::anyhow!("reload modify failed: {e:?}"))?;
                self.active = Some(provider);
            }
            Ordering::Less => {
                // on -> off: detach and shut down.
                self.reload_handle
                    .modify(|opt| *opt = None)
                    .map_err(|e| anyhow::anyhow!("reload modify failed: {e:?}"))?;
                if let Some(provider) = self.active.take() {
                    shutdown(provider);
                }
            }
            Ordering::Equal => {}
        }
        self.last_cfg = cfg.clone();
        Ok(())
    }
}

pub(super) fn build_initial_handle(
    cfg: &TelemetryConfig,
    data_dir: &std::path::Path,
) -> anyhow::Result<(Layer, Handle)> {
    // Build the pipeline at most ONCE at boot. The provider lives in Inner.active
    // so we can shut it down on toggle-off; the layer is moved into the reload
    // wrapper so it is attached to the subscriber. `OtelLayer` is not Clone and
    // doesn't need to be — we own exactly one instance.
    let (initial_layer, active) = if cfg.enabled {
        let (provider, layer) = build_pipeline(cfg, data_dir)?;
        (Some(layer), Some(provider))
    } else {
        (None, None)
    };

    let (reload_layer, reload_handle) = reload::Layer::new(initial_layer);

    let handle = Handle {
        inner: parking_lot::Mutex::new(Inner {
            reload_handle,
            active,
            last_cfg: cfg.clone(),
        }),
    };

    Ok((reload_layer, handle))
}

fn build_pipeline(
    cfg: &TelemetryConfig,
    data_dir: &std::path::Path,
) -> anyhow::Result<(sdktrace::SdkTracerProvider, OtelLayer)> {
    let endpoint = resolve_endpoint(cfg);
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()?;

    let instance_id = crate::telemetry::instance_id::ensure_instance_id(data_dir)?;

    let resource = Resource::builder()
        .with_attribute(KeyValue::new("service.name", cfg.service_name.clone()))
        .with_attribute(KeyValue::new("service.instance.id", instance_id))
        .build();

    let provider = sdktrace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    use opentelemetry::trace::TracerProvider as _;
    let tracer = provider.tracer("oneshim");
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    Ok((provider, layer))
}

pub(super) fn resolve_endpoint(cfg: &TelemetryConfig) -> String {
    if let Some(ref explicit) = cfg.otlp_endpoint {
        return explicit.clone();
    }
    if let Ok(env) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        if !env.is_empty() {
            return env;
        }
    }
    "http://localhost:4318".to_string()
}

fn shutdown(provider: sdktrace::SdkTracerProvider) {
    // Put the shutdown onto a separate blocking thread with a hard deadline so
    // a wedged exporter cannot hang the app (see T-X2-8).
    let join = std::thread::spawn(move || {
        let _ = provider.shutdown();
    });
    let _ = join.join();
}
```

The `Inner::apply` transition arms call `build_pipeline(cfg, data_dir)?` with the same `data_dir` captured at boot. Store `data_dir` in `Inner`:

```rust
pub(super) struct Inner {
    reload_handle: reload::Handle<Option<OtelLayer>, tracing_subscriber::Registry>,
    active: Option<sdktrace::SdkTracerProvider>,
    last_cfg: TelemetryConfig,
    data_dir: std::path::PathBuf,
}
```

`build_initial_handle` captures `data_dir.to_path_buf()` into the new field. `apply`'s off→on arm calls `build_pipeline(cfg, &self.data_dir)?`.

- [ ] **Step 3: Adjust `mod.rs` to expose what the tests need.**

The existing declarations in `mod.rs` already use `otlp::Inner` and `otlp::OtelLayer`; nothing else to add.

- [ ] **Step 4: Create the mock OTLP collector helper.**

Create `src-tauri/src/telemetry/mock_otlp.rs`:

```rust
#![cfg(all(test, feature = "telemetry"))]

use axum::{http::StatusCode, routing::post, Router};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct MockCollector {
    pub endpoint: String,
    pub rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

pub async fn start() -> MockCollector {
    let (tx, rx) = mpsc::unbounded_channel();
    let tx = Arc::new(tx);

    let app = Router::new()
        .route(
            "/v1/traces",
            post({
                let tx = Arc::clone(&tx);
                move |body: axum::body::Bytes| async move {
                    let _ = tx.send(body.to_vec());
                    StatusCode::OK
                }
            }),
        )
        .with_state(());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    MockCollector {
        endpoint: format!("http://{addr}"),
        rx,
    }
}
```

Register the helper at the top of `src-tauri/src/telemetry/mod.rs` (right after the existing `mod instance_id;` and `mod otlp;` declarations):

```rust
#[cfg(test)]
mod mock_otlp;
```

- [ ] **Step 5: Add T-X2-3 `feature_on_config_on_builds_pipeline` using the mock.**

Append inside `mod tests` in `src-tauri/src/telemetry/mod.rs`:

```rust
    #[tokio::test]
    #[cfg(feature = "telemetry")]
    async fn feature_on_config_on_builds_pipeline() {
        let tmp = tempfile::tempdir().unwrap();
        let mock = mock_otlp::start().await;

        let cfg = TelemetryConfig {
            enabled: true,
            otlp_endpoint: Some(mock.endpoint.clone()),
            ..Default::default()
        };
        let (_layer, handle) = Handle::new_with_layer(&cfg, tmp.path())
            .expect("pipeline builds against the mock");

        // apply() with an unchanged cfg is idempotent.
        handle.apply(&cfg).expect("apply is idempotent for unchanged cfg");
    }
```

- [ ] **Step 6: Add T-X2-4 `apply_disables_and_reenables_live`.**

```rust
    #[tokio::test]
    #[cfg(feature = "telemetry")]
    async fn apply_disables_and_reenables_live() {
        let tmp = tempfile::tempdir().unwrap();
        let mock = mock_otlp::start().await;

        let mut cfg_on = TelemetryConfig {
            enabled: true,
            otlp_endpoint: Some(mock.endpoint.clone()),
            ..Default::default()
        };
        let (_layer, handle) = Handle::new_with_layer(&cfg_on, tmp.path())
            .expect("pipeline builds against the mock");

        let cfg_off = TelemetryConfig { enabled: false, ..cfg_on.clone() };
        handle.apply(&cfg_off).expect("toggle off");

        cfg_on.sample_rate = 1.0;
        handle.apply(&cfg_on).expect("toggle back on");
    }
```

- [ ] **Step 7: Add T-X2-7 `endpoint_precedence`.**

```rust
#[test]
fn env_endpoint_overrides_default_but_not_explicit_config() {
    // Explicit config wins over env.
    std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://from-env:4318");
    let cfg_explicit = TelemetryConfig {
        otlp_endpoint: Some("http://from-config:4318".into()),
        ..Default::default()
    };
    assert_eq!(
        oneshim_app::telemetry_test_helpers::resolve_endpoint(&cfg_explicit),
        "http://from-config:4318"
    );

    // Env wins over default when config is None.
    let cfg_default = TelemetryConfig::default();
    assert_eq!(
        oneshim_app::telemetry_test_helpers::resolve_endpoint(&cfg_default),
        "http://from-env:4318"
    );

    std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
    assert_eq!(
        oneshim_app::telemetry_test_helpers::resolve_endpoint(&TelemetryConfig::default()),
        "http://localhost:4318"
    );
}
```

Expose `resolve_endpoint` via a test-helpers module in `src-tauri/src/telemetry/mod.rs`:

```rust
#[cfg(feature = "telemetry")]
pub mod telemetry_test_helpers {
    pub use super::otlp::resolve_endpoint;
}
```

- [ ] **Step 8: Run the four tests.**

```bash
cargo test -p oneshim-app --features telemetry telemetry::tests 2>&1 | tail -15
```

Expected: 4 passed.

- [ ] **Step 9: Clippy.**

```bash
cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings
```

Expected: green.

- [ ] **Step 10: Commit.**

```bash
git add src-tauri/src/telemetry/ src-tauri/src/telemetry/mod.rs src-tauri/src/telemetry/mock_otlp.rs
git commit -m "feat(app): wire OTLP pipeline behind tracing reload::Layer

Adds the real otlp module: SpanExporter over HTTP/proto (or gRPC per
Commit-0 spike), SdkTracerProvider, reload-wrapped OpenTelemetryLayer.

Handle::apply honours the off->on and on->off transitions in-place; the
'on' arm builds a new pipeline and swaps into the reload wrapper, the
'off' arm detaches and shuts the provider down on a watchdogged thread.

Tests: T-X2-2, T-X2-3, T-X2-4, T-X2-7 + a mock OTLP collector harness
under tests/mock_otlp.rs.

Part of Phase 2 (X2 Telemetry exporter wiring).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 9: `telemetry_instance_id` file lifecycle (T-X2-9)

**Files:**
- Modify: `src-tauri/src/telemetry/instance_id.rs`
- Modify: `src-tauri/src/telemetry/otlp.rs` (attach the UUID as a Resource attribute)
- Modify: `src-tauri/src/telemetry/mod.rs`

- [ ] **Step 1: Write failing T-X2-9 covering every state-table row (§3.7).**

Append inside the existing `mod tests` block in `src-tauri/src/telemetry/mod.rs`:

```rust
use oneshim_app::telemetry::instance_id_test_helpers as iid;

#[test]
fn instance_id_file_lifecycle_matches_state_table() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();

    // Row 1: first opt-in creates file with 0600 perms.
    let first = iid::ensure_instance_id(&data_dir).unwrap();
    let path = data_dir.join("telemetry_instance_id");
    assert!(path.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "instance_id perms must be 0600");
    }

    // Row 2: boot with enabled=true + file exists -> reuse same UUID.
    let second = iid::ensure_instance_id(&data_dir).unwrap();
    assert_eq!(first, second, "UUID must be stable across opt-in cycles");

    // Rows 4 + 5 ("enabled=false + file exists -> untouched" and "opt-out -> file
    // still present") collapse into a single meaningful assertion: after opt-out
    // the file is still present and still holds the same UUID. The earlier
    // reuse-same-UUID assertion (Row 2) already proved `ensure_instance_id`
    // doesn't rewrite; here we assert nothing else erases it either.
    assert!(path.exists());
    let contents = std::fs::read_to_string(&path).unwrap();
    assert_eq!(contents.trim(), first);

    // Row 6: explicit reset -> file deleted, next ensure regenerates different UUID.
    iid::reset_instance_id(&data_dir).unwrap();
    assert!(!path.exists());
    let third = iid::ensure_instance_id(&data_dir).unwrap();
    assert_ne!(first, third, "reset must regenerate UUID");
}
```

- [ ] **Step 2: Implement `instance_id.rs`.**

Replace `src-tauri/src/telemetry/instance_id.rs`:

```rust
#![cfg(feature = "telemetry")]

use std::fs;
use std::io::Write;
use std::path::Path;

const FILE_NAME: &str = "telemetry_instance_id";

pub(super) fn ensure_instance_id(data_dir: &Path) -> anyhow::Result<String> {
    let path = data_dir.join(FILE_NAME);
    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let uuid = format!("{}", uuid::Uuid::new_v4());
    fs::create_dir_all(data_dir)?;
    write_with_owner_only(&path, &uuid)?;
    Ok(uuid)
}

pub(super) fn reset_instance_id(data_dir: &Path) -> anyhow::Result<()> {
    let path = data_dir.join(FILE_NAME);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn write_with_owner_only(path: &Path, contents: &str) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(contents.as_bytes())?;
    Ok(())
}

#[cfg(windows)]
fn write_with_owner_only(path: &Path, contents: &str) -> anyhow::Result<()> {
    // Windows ACLs default to user-profile private. `CREATE_NEW` prevents
    // truncating a squatted file.
    use std::os::windows::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        // FILE_FLAG_WRITE_THROUGH = 0x80000000, belt-and-braces for durability.
        .custom_flags(0x8000_0000)
        .open(path)?;
    f.write_all(contents.as_bytes())?;
    Ok(())
}

pub mod instance_id_test_helpers {
    pub use super::{ensure_instance_id, reset_instance_id};
}
```

Expose the test helpers in `mod.rs`:

```rust
#[cfg(feature = "telemetry")]
pub use instance_id::instance_id_test_helpers;
```

`uuid` is already a workspace dep and already listed unconditionally in `src-tauri/Cargo.toml` (`uuid = { workspace = true }` line ~52), so no dep change is needed. Do NOT add `dep:uuid` to the `telemetry` feature line — that would re-declare the same crate as optional and Cargo will reject the combination.

- [ ] **Step 3: Wire the UUID into the Resource attributes in `otlp::build_pipeline`.**

Modify `build_pipeline` to accept a `data_dir` (or a pre-resolved instance_id) and add the attribute to the Resource:

```rust
let instance_id = crate::telemetry::instance_id::ensure_instance_id(data_dir)?;
let resource = Resource::builder()
    .with_attribute(KeyValue::new("service.name", cfg.service_name.clone()))
    .with_attribute(KeyValue::new("service.instance.id", instance_id))
    .build();
```

Propagate `data_dir` through `build_initial_handle` and `Inner::apply` — use `oneshim_core::config_manager::ConfigManager::data_dir()` at the call site in `main.rs`, not inside these helpers.

- [ ] **Step 4: Run T-X2-9.**

```bash
cargo test -p oneshim-app --features telemetry -- telemetry::tests::\
  instance_id_file_lifecycle_matches_state_table 2>&1 | tail -10
```

Expected: 1 passed.

- [ ] **Step 5: Run the whole telemetry suite.**

```bash
cargo test -p oneshim-app --features telemetry 2>&1 | tail -15
```

Expected: no regressions.

- [ ] **Step 6: Clippy + commit.**

```bash
cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings
git add src-tauri/Cargo.toml src-tauri/src/telemetry/ src-tauri/src/telemetry/mod.rs Cargo.lock
git commit -m "feat(app): telemetry_instance_id lifecycle

Creates the UUIDv4 instance file with 0600 perms on first opt-in,
reuses it across opt-in cycles, leaves it untouched while disabled,
and regenerates on explicit reset.

Attached as service.instance.id on OTel Resource.

Tests: T-X2-9 (covers all seven rows of the §3.7 state table).

Part of Phase 2 (X2 Telemetry exporter wiring).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 10: Shutdown-with-unreachable-collector watchdog (T-X2-8)

**Files:**
- Modify: `src-tauri/src/telemetry/mod.rs`
- Modify: `src-tauri/src/telemetry/otlp.rs` (already has thread-isolated shutdown — tighten deadline)

- [ ] **Step 1: Add T-X2-8.**

Append inside the existing `mod tests` block in `src-tauri/src/telemetry/mod.rs`:

```rust
    #[tokio::test]
    #[cfg(feature = "telemetry")]
    async fn shutdown_completes_when_collector_unreachable() {
        let tmp = tempfile::tempdir().unwrap();
        // Pick a port that is guaranteed unreachable.
        let cfg_on = TelemetryConfig {
            enabled: true,
            otlp_endpoint: Some("http://127.0.0.1:1".into()),
            ..Default::default()
        };
        let (_layer, handle) = Handle::new_with_layer(&cfg_on, tmp.path())
            .expect("exporter builds even though the endpoint is unreachable");

    // Emit 5 spans so the exporter has queue pressure.
    for i in 0..5 {
        tracing::info_span!("t_x2_8_span", i).in_scope(|| {});
    }

    // Toggle off — this drives apply() -> shutdown().
    let cfg_off = TelemetryConfig { enabled: false, ..cfg_on };
    let start = std::time::Instant::now();
    handle.apply(&cfg_off).expect("apply off must not hang");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "shutdown watchdog exceeded 5s"
    );
}
```

- [ ] **Step 2: Tighten the shutdown watchdog in `otlp.rs`.**

Replace `shutdown` with:

```rust
fn shutdown(provider: sdktrace::SdkTracerProvider) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = provider.shutdown();
        let _ = tx.send(());
    });
    if rx.recv_timeout(std::time::Duration::from_secs(4)).is_err() {
        tracing::warn!(
            "otel provider shutdown exceeded 4s; continuing without waiting"
        );
    }
}
```

- [ ] **Step 3: Run T-X2-8.**

```bash
cargo test -p oneshim-app --features telemetry -- telemetry::tests::\
  shutdown_completes_when_collector_unreachable 2>&1 | tail -10
```

Expected: 1 passed, within 5 s.

- [ ] **Step 4: Clippy + commit.**

```bash
cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings
git add src-tauri/src/telemetry/otlp.rs src-tauri/src/telemetry/mod.rs
git commit -m "fix(app): watchdogged OTel shutdown when collector unreachable

Provider shutdown now runs on a dedicated thread with a 4s deadline.
Past the deadline we log a warning and proceed — the SDK may retain a
zombie I/O task but the app is never blocked on exit.

Tests: T-X2-8.

Part of Phase 2 (X2 Telemetry exporter wiring).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 11: Bus-driven toggle task in `main.rs` + spine test (T-X2-5, T-X2-10)

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/telemetry/mod.rs`

- [ ] **Step 1: Write failing T-X2-10 (spine test).**

```rust
    #[tokio::test]
    #[cfg(feature = "telemetry")]
    async fn mock_collector_receives_span() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mock = mock_otlp::start().await;
        let cfg = TelemetryConfig {
            enabled: true,
            otlp_endpoint: Some(mock.endpoint.clone()),
            ..Default::default()
        };

        // Build the subscriber the same way main.rs will.
        let (layer, handle) = Handle::new_with_layer(&cfg, tmp.path())
            .expect("pipeline builds against the mock");
        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info_span!("t_x2_10_span").in_scope(|| {
        tracing::info!("body");
    });

    // Wait up to 15s for the exporter to flush.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        if let Ok(body) = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            mock.rx.recv(),
        )
        .await
        {
            if body.is_some() {
                return; // pass
            }
        }
    }
    panic!("no OTLP POST reached the mock collector within 15s");
}
```

- [ ] **Step 2: Run — expect fail because the subscriber does not actually flush.**

```bash
cargo test -p oneshim-app --features telemetry -- telemetry::tests::\
  mock_collector_receives_span 2>&1 | tail -10
```

If it already passes, the infrastructure is sufficient — skip Step 3.

- [ ] **Step 3: Force-flush the exporter for deterministic tests.**

`SdkTracerProvider::force_flush()` exists on opentelemetry_sdk 0.27+. Use it instead of time-based sleep.

This requires `Handle::new_with_layer` to expose the provider to the test. Extend `TelemetryHandle` with a test-only helper:

```rust
#[cfg(all(test, feature = "telemetry"))]
impl Handle {
    pub(crate) fn force_flush_for_tests(&self) {
        let inner = self.inner.lock();
        if let Some(ref provider) = inner.active {
            let _ = provider.force_flush();
        }
    }
}
```

Then in the test, replace the `sleep(1s)` with:

```rust
handle.force_flush_for_tests();
```

`_handle` in the original T-X2-10 snippet therefore becomes `handle` (keep the reference instead of `_`) — already shown as `Handle::new_with_layer(&cfg, tmp.path())?` in Step 1 of this task.

- [ ] **Step 4: Add T-X2-5 `config_bus_delivers_telemetry_toggle` as an integration test against `ConfigManager`.**

```rust
#[tokio::test]
async fn config_bus_delivers_telemetry_toggle() {
    use oneshim_core::config_manager::ConfigManager;

    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.json");
    let mgr = std::sync::Arc::new(ConfigManager::with_path(cfg_path).unwrap());

    let handle = {
        let cfg = mgr.get();
        let (_layer, h) = Handle::new_with_layer(&cfg.telemetry, tmp.path())
            .expect("feature-on construction must succeed for the test");
        std::sync::Arc::new(h)
    };

    // Spawn the toggle task (copy of the production wiring in Task 11 Step 6).
    let observed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed_for_task = observed.clone();
    let handle_for_task = handle.clone();
    let mut rx = mgr.subscribe();
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().telemetry.clone();
        while rx.changed().await.is_ok() {
            let current = rx.borrow_and_update().telemetry.clone();
            if current != prev {
                let _ = handle_for_task.apply(&current);
                observed_for_task.store(true, std::sync::atomic::Ordering::SeqCst);
                prev = current;
            }
        }
    });

    mgr.update_with(|c| {
        c.telemetry.enabled = true;
        Ok(())
    })
    .unwrap();

    // Deterministic wait: poll the atomic with a short timeout budget.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        if observed.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("toggle task did not observe the update within 1s");
}
```

- [ ] **Step 5: Run T-X2-5.**

```bash
cargo test -p oneshim-app --features telemetry -- telemetry::tests::\
  config_bus_delivers_telemetry_toggle 2>&1 | tail -10
```

Expected: 1 passed.

- [ ] **Step 6: Wire the subscriber + bus-driven toggle task in `main.rs`.**

Three integration points in `src-tauri/src/main.rs`:

**(a) Build the telemetry layer + handle before the subscriber is initialised.** Do this right after the `file_layer` is constructed (around the existing line that builds `tracing_subscriber::registry().with(env_filter)...`):

```rust
use telemetry::Handle as TelemetryHandle;

let data_dir = oneshim_core::config_manager::ConfigManager::data_dir()
    .unwrap_or_else(|_| std::path::PathBuf::from("."));
std::fs::create_dir_all(&data_dir).ok();

// ConfigManager is already constructed earlier; we only need the initial TelemetryConfig.
let initial_telemetry = config_manager.get().telemetry.clone();
let (telemetry_layer, telemetry_handle) = match TelemetryHandle::new_with_layer(
    &initial_telemetry,
    &data_dir,
) {
    Ok(x) => x,
    Err(e) => {
        tracing::warn!(error = %e, "telemetry init failed; continuing without OTLP export");
        // Fall back to a disabled pipeline so the subscriber still takes a placeholder layer.
        TelemetryHandle::new_with_layer(
            &oneshim_core::config::sections::storage::TelemetryConfig::default(),
            &data_dir,
        )
        .expect("disabled-at-boot construction is infallible")
    }
};
```

**(b) Attach the layer to the subscriber.** In the existing `tracing_subscriber::registry()` chain, append `.with(telemetry_layer)` before `.init()`:

```rust
tracing_subscriber::registry()
    .with(env_filter)
    .with(console_layer)
    .with(file_layer)
    .with(telemetry_layer)
    .init();
```

**(c) Wrap in `Arc` and spawn the bus-driven toggle task.** `TelemetryHandle` itself does not implement `Clone` (the inner `parking_lot::Mutex` is not cloneable), so we share it via `Arc`:

```rust
let telemetry_handle = std::sync::Arc::new(telemetry_handle);
// Stash a clone in Tauri managed state so commands can reach it later.
let telemetry_handle_for_state = telemetry_handle.clone();
// Separate clone for the toggle task.
let handle_for_task = telemetry_handle.clone();

let mut rx = config_manager.subscribe();
tokio::spawn(async move {
    let mut prev = rx.borrow_and_update().telemetry.clone();
    while rx.changed().await.is_ok() {
        let current = rx.borrow_and_update().telemetry.clone();
        if current != prev {
            if let Err(e) = handle_for_task.apply(&current) {
                tracing::warn!(error = %e, "telemetry apply failed");
            }
            prev = current;
        }
    }
});
```

The `#[cfg(not(feature = "telemetry"))]` path still needs (a) and (c) above — the `Layer` is a `NoopLayer`, and the toggle task is dead code that compiles cleanly. No `#[cfg]` gate on the wiring itself; the feature gate inside `TelemetryHandle` handles the dispatch.

- [ ] **Step 7: Run the whole test suite on both feature states.**

```bash
cargo test --workspace 2>&1 | tail -5
cargo test -p oneshim-app --features telemetry 2>&1 | tail -5
```

Expected: both green.

- [ ] **Step 8: Clippy both feature states.**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings
```

Expected: both green.

- [ ] **Step 9: Commit.**

```bash
git add src-tauri/src/main.rs src-tauri/src/telemetry/mod.rs
git commit -m "feat(app): bus-driven telemetry toggle + mock-collector spine test

ConfigChangeBus subscriber in main.rs forwards TelemetryConfig changes
to Handle::apply. A span emitted with telemetry enabled reaches the mock
OTLP collector within 15s.

Tests: T-X2-5, T-X2-10.

Part of Phase 2 (X2 Telemetry exporter wiring).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 12: User documentation + CI matrix + STATUS update

**Files:**
- Create: `docs/guides/telemetry.md`
- Create: `docs/guides/telemetry.ko.md`
- Modify: `.github/workflows/ci.yml` (add path-gated telemetry matrix cell)
- Modify: `docs/STATUS.md`
- Modify: `crates/oneshim-core/CLAUDE.md`
- Modify: `src-tauri/CLAUDE.md`

- [ ] **Step 1: Write the user guide.**

`docs/guides/telemetry.md` sections (each non-trivial):
- What is collected (spans + events; no PII; `service.instance.id` UUIDv4).
- How to enable (toggle in Preferences > Privacy, or edit `config.json`).
- How to point at a custom collector (`otlp_endpoint` or `OTEL_EXPORTER_OTLP_ENDPOINT`).
- How to opt out (toggle off — stops new exports within one tick).
- How to fully erase the instance identity (`telemetry reset-instance-id` CLI command, wired in Phase 3).

- [ ] **Step 2: Write the Korean companion `telemetry.ko.md`.**

Same structure, in Korean.

- [ ] **Step 3: Update the CI workflow.**

The repo gates Rust work via `changes.outputs.rust == 'true'` computed by `./scripts/gha-detect-changes.sh`. That script already emits `rust=true` whenever `src-tauri/**`, `crates/**`, `Cargo.toml`, or `Cargo.lock` change, so no extra `paths:` filter is needed — a PR that touches telemetry code already trips the Rust rebuild. The spec's "path-gated" wording means "the change detection already covers telemetry-relevant paths" — not a new matrix cell.

In `.github/workflows/ci.yml` there are existing steps for `--features server` and `--features grpc`. Append telemetry variants after them.

Clippy (append after the existing `Run clippy (grpc features)` step around line 228):

```yaml
      - name: Run clippy (telemetry features)
        run: ./scripts/cargo-cache.sh clippy -p oneshim-app --all-targets --features telemetry -- -D warnings -A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity
```

Tests (append in the Tests job, alongside whatever other `--features` test runs exist; if the job has none, add as the first feature-flag test):

```yaml
      - name: Run tests (telemetry features)
        run: ./scripts/cargo-cache.sh test -p oneshim-app --features telemetry
```

Notes:
- Use `./scripts/cargo-cache.sh` (the repo's wrapper) — never raw `cargo` in ci.yml.
- Keep the `-A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity` allow-list consistent with the other clippy invocations; do not drop it.
- Do NOT add a new `if:` gate or a new matrix cell — the `needs: [changes]` gating already covers "only when Rust changed," and `workflow_dispatch` / main push runs it unconditionally via `emit_all_true` in `scripts/gha-detect-changes.sh`.
- If the Tests job uses `./scripts/cargo-cache.sh test --workspace` with no feature flags, still add an explicit `-p oneshim-app --features telemetry` step (not `--workspace --features telemetry` — that would try to apply the feature to every crate that doesn't declare it and fail).

- [ ] **Step 4: Bump `docs/STATUS.md`.**

Add a row under the test-count section: "Rust tests: +20 (10 X1 + 10 X2)."
Add a row under the binary-size section: "Telemetry feature: +X.Y MB (measured 2026-04-XX)."
Run `cargo build --release -p oneshim-app` with and without `--features telemetry`, record the sizes.

- [ ] **Step 5: Update CLAUDE.md per-crate notes.**

`crates/oneshim-core/CLAUDE.md`: add a line under the public API bullet listing `subscribe()`, `snapshot()`, and the audit-coalescing hazard (cite ADR-016).

`src-tauri/CLAUDE.md`: add a line to the module summary — "`telemetry/` — feature-gated OTel OTLP exporter behind `reload::Layer`."

- [ ] **Step 6: Commit.**

```bash
git add docs/guides/telemetry.md docs/guides/telemetry.ko.md .github/workflows/ci.yml docs/STATUS.md crates/oneshim-core/CLAUDE.md src-tauri/CLAUDE.md
git commit -m "docs(phase2): telemetry user guide, CI matrix, STATUS bump

Closes Phase 2.

- docs/guides/telemetry.md + .ko.md: end-user guide.
- CI: path-gated --features telemetry matrix cell; unconditional on
  scheduled main + release.
- STATUS: +20 tests, binary size delta.
- CLAUDE.md: note the new public APIs and telemetry module.

Part of Phase 2 (X1 ConfigChangeBus + X2 Telemetry exporter wiring).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Self-review checklist (run before opening a PR)

- [ ] All 7 commits keep `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` green.
- [ ] `cargo check -p oneshim-app --features telemetry`, `cargo test -p oneshim-app --features telemetry`, `cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings` all green.
- [ ] All 20 tests named in the spec exist (T-X1-1..10, T-X2-1..10) and pass.
- [ ] `docs/architecture/ADR-016-config-change-bus.md` + `.ko.md` present.
- [ ] `docs/guides/telemetry.md` + `.ko.md` present.
- [ ] `docs/STATUS.md` updated with the measured binary-size delta and test count.
- [ ] `TelemetryConfig::default().enabled == false` — manual sanity check.
- [ ] No scheduler loop migrated to `subscribe()` (spec §2.6).
- [ ] Commit-0 spike outcome recorded in commit message of Task 7 if transport pivoted.

When all boxes are ticked, push the branch and open the PR. Use the spec as the PR description seed.
