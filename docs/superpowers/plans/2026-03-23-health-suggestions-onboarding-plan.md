# Runtime Health + Suggestions + Onboarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete three features: runtime health check with tray icon updates, SSE/gRPC suggestion reception, and first-run onboarding page.

**Architecture:** Health check uses passive AtomicBool flag reading from adapters (no network calls in the loop). Suggestions wire the existing SuggestionReceiver into the scheduler with shutdown integration. Onboarding adds a SQLite V19 migration + React onboarding page with IPC gate.

**Tech Stack:** Rust (Tauri v2, tokio, image, rusqlite), React 18 (react-router-dom, Tailwind CSS, i18n)

**Spec:** `docs/superpowers/specs/2026-03-23-runtime-health-suggestions-onboarding-design.md`

---

## Feature 1: Runtime Health Check

### Task 1: Add last_request_ok flags to adapters

**Files:**
- Modify: `crates/oneshim-network/src/batch_uploader.rs`
- Modify: `crates/oneshim-network/src/ai_llm_client/mod.rs`
- Modify: `crates/oneshim-automation/src/controller/mod.rs`

- [ ] **Step 1: Add `last_upload_ok` to BatchUploader**

In `batch_uploader.rs`, add field to struct:
```rust
    last_upload_ok: Option<Arc<AtomicBool>>,
```

Add builder method:
```rust
    pub fn with_health_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.last_upload_ok = Some(flag);
        self
    }
```

In `flush()`, after successful upload set `true`, on final failure set `false`:
```rust
// After successful upload:
if let Some(ref flag) = self.last_upload_ok {
    flag.store(true, Ordering::Relaxed);
}

// On final failure (after retries exhausted):
if let Some(ref flag) = self.last_upload_ok {
    flag.store(false, Ordering::Relaxed);
}
```

Initialize as `None` in `new()`.

- [ ] **Step 2: Add `last_request_ok` to RemoteLlmProvider**

In `ai_llm_client/mod.rs`, add field:
```rust
    last_request_ok: Option<Arc<AtomicBool>>,
```

Add method:
```rust
    pub fn with_health_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.last_request_ok = Some(flag);
        self
    }
```

In `send_and_parse()` (the internal method called by all LLM requests), set flag on success/failure.

- [ ] **Step 3: Add `last_command_ok` to AutomationController**

In `controller/mod.rs`, add field:
```rust
    pub(super) last_command_ok: Option<Arc<AtomicBool>>,
```

Add public method:
```rust
    pub fn with_health_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.last_command_ok = Some(flag);
        self
    }
```

In `execute_command()` (or the primary dispatch method), set flag on success/failure.

- [ ] **Step 4: Verify compilation**

Run: `cargo check --workspace`
Expected: 0 errors (new fields are `Option`, no callers break)

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-network/src/batch_uploader.rs crates/oneshim-network/src/ai_llm_client/mod.rs crates/oneshim-automation/src/controller/mod.rs
git commit -m "feat(health): add last_request_ok health flags to adapters"
```

---

### Task 2: Create health check scheduler loop

**Files:**
- Create: `src-tauri/src/scheduler/loops/health.rs`
- Modify: `src-tauri/src/scheduler/loops/mod.rs`

- [ ] **Step 1: Create health.rs**

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tracing::info;

/// Health flags read from adapters (source of truth).
pub(crate) struct AdapterHealthFlags {
    pub server_ok: Arc<AtomicBool>,
    pub llm_ok: Arc<AtomicBool>,
    pub cli_ok: Arc<AtomicBool>,
}

/// AppState connection flags (write target for tray display).
pub(crate) struct ConnectionFlags {
    pub server: Arc<AtomicBool>,
    pub llm: Arc<AtomicBool>,
    pub cli: Arc<AtomicBool>,
}

/// Note: Uses concrete `AppHandle` (Wry runtime), not generic `<R: Runtime>`,
/// consistent with how `Scheduler` stores `app_handle: Option<AppHandle>`.
pub(crate) fn spawn_health_check_loop(
    interval: Duration,
    adapter_flags: AdapterHealthFlags,
    connection_flags: ConnectionFlags,
    app_handle: AppHandle,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let srv = adapter_flags.server_ok.load(Ordering::Relaxed);
                    let llm = adapter_flags.llm_ok.load(Ordering::Relaxed);
                    let cli = adapter_flags.cli_ok.load(Ordering::Relaxed);

                    let prev_srv = connection_flags.server.swap(srv, Ordering::Relaxed);
                    let prev_llm = connection_flags.llm.swap(llm, Ordering::Relaxed);
                    let prev_cli = connection_flags.cli.swap(cli, Ordering::Relaxed);

                    // Only update tray + emit event on state change
                    if prev_srv != srv || prev_llm != llm || prev_cli != cli {
                        let payload = serde_json::json!({
                            "server": srv, "llm": llm, "cli": cli
                        });
                        let _ = app_handle.emit_to("magic-overlay", "overlay:connection-changed", &payload);
                        let _ = app_handle.emit_to("tracking-panel", "overlay:connection-changed", &payload);

                        if let Some(state) = app_handle.try_state::<crate::runtime_state::AppState>() {
                            let paused = state.capture_paused.load(Ordering::Relaxed);
                            let visible = state.indicator_visible.load(Ordering::Relaxed);
                            let _ = crate::tray::sync_tray_state(&app_handle, paused, visible);
                        }
                        info!(server = srv, llm = llm, cli = cli, "connection status changed");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("health check loop shutdown");
                    break;
                }
            }
        }
    })
}
```

- [ ] **Step 2: Add re-export in loops/mod.rs**

Add `pub(crate) mod health;` to `src-tauri/src/scheduler/loops/mod.rs`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-app`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/scheduler/loops/health.rs src-tauri/src/scheduler/loops/mod.rs
git commit -m "feat(health): spawn_health_check_loop with passive flag reading"
```

---

### Task 3: Wire health flags through Scheduler + AppRuntimeLaunch

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/app_runtime_launch.rs`

- [ ] **Step 1: Add health flag fields to Scheduler**

In `scheduler/mod.rs`, add to Scheduler struct:
```rust
    server_health_flag: Option<Arc<AtomicBool>>,
    llm_health_flag: Option<Arc<AtomicBool>>,
    cli_health_flag: Option<Arc<AtomicBool>>,
    server_connected: Option<Arc<AtomicBool>>,
    llm_connected: Option<Arc<AtomicBool>>,
    cli_connected: Option<Arc<AtomicBool>>,
    app_handle: Option<AppHandle>,
```

Add builder methods:
```rust
    pub fn with_health_flags(
        &mut self,
        server_flag: Arc<AtomicBool>,
        llm_flag: Arc<AtomicBool>,
        cli_flag: Arc<AtomicBool>,
    ) -> &mut Self {
        self.server_health_flag = Some(server_flag);
        self.llm_health_flag = Some(llm_flag);
        self.cli_health_flag = Some(cli_flag);
        self
    }

    pub fn with_connection_flags(
        &mut self,
        server: Arc<AtomicBool>,
        llm: Arc<AtomicBool>,
        cli: Arc<AtomicBool>,
    ) -> &mut Self {
        self.server_connected = Some(server);
        self.llm_connected = Some(llm);
        self.cli_connected = Some(cli);
        self
    }

    pub fn with_app_handle(&mut self, handle: AppHandle) -> &mut Self {
        self.app_handle = Some(handle);
        self
    }
```

- [ ] **Step 2: Spawn health loop in run_scheduler_loops**

In `run_scheduler_loops()`, after the last existing named task spawn, add a new named variable (the scheduler uses `let name_task = spawn_*()` pattern, NOT `handles.push()`):

```rust
    // Health check loop (passive flag reading, 30s interval)
    let health_task = if let (Some(s_flag), Some(l_flag), Some(c_flag), Some(s_conn), Some(l_conn), Some(c_conn), Some(ref handle)) = (
        &self.server_health_flag, &self.llm_health_flag, &self.cli_health_flag,
        &self.server_connected, &self.llm_connected, &self.cli_connected,
        &self.app_handle,
    ) {
        Some(loops::health::spawn_health_check_loop(
            Duration::from_secs(30),
            loops::health::AdapterHealthFlags {
                server_ok: s_flag.clone(),
                llm_ok: l_flag.clone(),
                cli_ok: c_flag.clone(),
            },
            loops::health::ConnectionFlags {
                server: s_conn.clone(),
                llm: l_conn.clone(),
                cli: c_conn.clone(),
            },
            handle.clone(),
            shutdown_rx.clone(),
        ))
    } else {
        None
    };
```

And in the shutdown sequence (after `shutdown_rx.changed().await`), add:
```rust
    if let Some(t) = health_task { t.abort(); }
```

- [ ] **Step 3: Pass flags through AgentRuntimeBuilder**

> **Note**: The spec says health flags "bypass AgentRuntimeBuilder". This is incorrect — the Scheduler is constructed inside `AgentRuntimeBundle.run()`, so flags must flow through the builder chain. This plan overrides the spec on this point.

In `agent_runtime.rs`, add fields and builder methods to `AgentRuntimeBuilder`:
```rust
    server_health_flag: Option<Arc<AtomicBool>>,
    llm_health_flag: Option<Arc<AtomicBool>>,
    cli_health_flag: Option<Arc<AtomicBool>>,
    server_connected: Option<Arc<AtomicBool>>,
    llm_connected: Option<Arc<AtomicBool>>,
    cli_connected: Option<Arc<AtomicBool>>,
```

Add `with_health_flags()` and `with_connection_flags()` methods. In `build()`, forward to Scheduler via its builder methods.

- [ ] **Step 4: Create and wire flags in app_runtime_launch.rs**

Create 3 adapter health flags:
```rust
    let server_health_flag = Arc::new(AtomicBool::new(false));
    let llm_health_flag = Arc::new(AtomicBool::new(false));
    let cli_health_flag = Arc::new(AtomicBool::new(false));
```

Remove the existing optimistic initialization of AppState connection flags (the `#[cfg(feature = "server")] server_connected.store(true, ...)` etc. lines). The health loop is now the single source of truth.

Pass health flags to adapter constructors via `with_health_flag()`.
Pass both health flags and AppState connection flags to `AgentRuntimeBuilder`.
Pass `app_handle.clone()` to the builder.

- [ ] **Step 5: Verify compilation and tests**

Run: `cargo check -p oneshim-app && cargo test -p oneshim-app`
Expected: 0 errors, all tests pass

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(health): wire health flags through scheduler + app launch"
```

---

## Feature 3: First-Run Onboarding

### Task 4: SQLite V19 migration — app_meta table

**Files:**
- Modify: `crates/oneshim-storage/src/migration/mod.rs`
- Modify: `crates/oneshim-storage/src/migration/v09_v18.rs` (add `migrate_v19` function — keep in same file, consistent with grouped-range convention)

- [ ] **Step 1: Write test for V19 migration**

In the migration test module (or `sqlite.rs` tests), add:
```rust
#[test]
fn migrate_v19_creates_app_meta_table() {
    let conn = Connection::open_in_memory().unwrap();
    // Run all migrations up to V19
    // ...
    conn.execute("INSERT INTO app_meta (key, value) VALUES ('test', 'hello')", []).unwrap();
    let val: String = conn.query_row("SELECT value FROM app_meta WHERE key = 'test'", [], |r| r.get(0)).unwrap();
    assert_eq!(val, "hello");
}
```

- [ ] **Step 2: Implement V19 migration**

Bump `CURRENT_VERSION` to 19 in `migration/mod.rs`.

Add `migrate_v19` to `v09_v18.rs` (same file as V16-V18, grouped convention):
```rust
pub(super) fn migrate_v19(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        INSERT INTO schema_version (version) VALUES (19);"
    )?;
    Ok(())
}
```

Add dispatch call in `run_migrations()`.

- [ ] **Step 3: Add get_meta/set_meta/delete_meta to SqliteStorage**

In `crates/oneshim-storage/src/sqlite.rs`:
```rust
    pub fn get_meta(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            [key],
            |row| row.get(0),
        ).ok()
    }

    pub fn set_meta(&self, key: &str, value: &str) {
        let conn = self.conn.lock();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO app_meta (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        );
    }

    pub fn delete_meta(&self, key: &str) {
        let conn = self.conn.lock();
        let _ = conn.execute("DELETE FROM app_meta WHERE key = ?1", [key]);
    }
```

- [ ] **Step 4: Write tests for get_meta/set_meta**

```rust
#[test]
fn meta_roundtrip() {
    let storage = open_test_db();
    assert_eq!(storage.get_meta("onboarding"), None);
    storage.set_meta("onboarding", "true");
    assert_eq!(storage.get_meta("onboarding"), Some("true".to_string()));
    storage.delete_meta("onboarding");
    assert_eq!(storage.get_meta("onboarding"), None);
}
```

- [ ] **Step 5: Verify**

Run: `cargo test -p oneshim-storage`
Expected: all pass

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(storage): V19 migration app_meta table + get/set/delete_meta"
```

---

### Task 5: Onboarding IPC commands

**Files:**
- Create: `src-tauri/src/commands/onboarding.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Create onboarding.rs**

```rust
use serde::Serialize;
use tauri::{command, State};

use crate::runtime_state::AppState;

#[derive(Serialize)]
pub struct OnboardingStatus {
    pub completed: bool,
}

#[command]
pub async fn get_onboarding_status(
    state: State<'_, AppState>,
) -> Result<OnboardingStatus, String> {
    let completed = state
        .storage
        .get_meta("onboarding_completed")
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(OnboardingStatus { completed })
}

#[command]
pub async fn complete_onboarding(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.storage.set_meta("onboarding_completed", "true");
    Ok(())
}

#[command]
pub async fn reset_onboarding(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.storage.delete_meta("onboarding_completed");
    Ok(())
}
```

- [ ] **Step 2: Register module and commands**

Add `pub(crate) mod onboarding;` to `commands/mod.rs`.

Add to `main.rs` invoke_handler:
```rust
    commands::onboarding::get_onboarding_status,
    commands::onboarding::complete_onboarding,
    commands::onboarding::reset_onboarding,
```

- [ ] **Step 3: Verify**

Run: `cargo check -p oneshim-app`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(onboarding): IPC commands for onboarding status"
```

---

### Task 6: Onboarding React page + route

**Files:**
- Create: `crates/oneshim-web/frontend/src/pages/Onboarding.tsx`
- Modify: `crates/oneshim-web/frontend/src/App.tsx`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/en.json`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/ko.json`

- [ ] **Step 1: Create Onboarding.tsx**

Step-based single-page component with 4 steps (Intro → Permissions → Features → Start). Uses `invoke('complete_onboarding')` on final step, then navigates to `/`.

Key requirements:
- Uses `useTranslation()` for all text (i18n keys under `onboarding.*`)
- Uses Tailwind CSS classes consistent with existing pages
- Step indicator at top (dots or progress bar)
- "Skip" button available (does NOT complete onboarding)
- "Complete" button on final step → calls IPC → navigates to dashboard

- [ ] **Step 2: Add onboarding route + redirect gate in App.tsx**

The current App.tsx renders a shell layout (TitleBar, ActivityBar, SidePanel, StatusBar) around `<Routes>`. The onboarding page must render as a **full-page layout WITHOUT the shell**, so it needs special handling.

**Approach**: The current `App.tsx` renders the shell (TitleBar, ActivityBar, SidePanel, StatusBar) directly — it does NOT use a `<Route element={<ShellLayout />}>` wrapper. Therefore, render the Onboarding page BEFORE the shell when onboarding is incomplete:

```tsx
const Onboarding = lazy(() => import('./pages/Onboarding'))

// Add onboarding state at the top of App component:
const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null)
useEffect(() => {
  invoke<{ completed: boolean }>('get_onboarding_status')
    .then(r => setOnboardingDone(r.completed))
    .catch(() => setOnboardingDone(true)) // standalone/dev mode
}, [])

// Gate: show full-page onboarding instead of the shell
if (onboardingDone === null) return null // loading
if (!onboardingDone) return <Onboarding onComplete={() => setOnboardingDone(true)} />

// else: render the normal shell + routes (existing code unchanged)
```

This avoids modifying the routing structure. The Onboarding component is a full-page layout with no shell chrome.

- [ ] **Step 3: Add i18n keys**

`en.json`:
```json
"onboarding": {
    "step1Title": "Welcome to ONESHIM",
    "step1Desc": "ONESHIM monitors your desktop activity and provides intelligent suggestions to boost productivity.",
    "step2Title": "Permissions",
    "step2Desc": "ONESHIM needs some permissions to work effectively.",
    "step2Accessibility": "Accessibility",
    "step2ScreenCapture": "Screen Capture",
    "step2Notifications": "Notifications",
    "step3Title": "Features",
    "step3Desc": "Here's what ONESHIM can do for you.",
    "step3Capture": "Real-time context capture",
    "step3Analysis": "AI-powered analysis",
    "step3Suggestions": "Smart suggestions",
    "step4Title": "Ready to Go",
    "step4Desc": "You're all set. Start using ONESHIM now.",
    "next": "Next",
    "skip": "Skip",
    "complete": "Get Started"
}
```

`ko.json`: Korean translations for all keys above.

- [ ] **Step 4: Verify**

Run: `cd crates/oneshim-web/frontend && npx tsc --noEmit`
Expected: exit 0

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(onboarding): first-run onboarding page with 4-step guide"
```

---

### Task 7: Settings — "View Guide" button

**Files:**
- Modify: `crates/oneshim-web/frontend/src/pages/Settings.tsx`

- [ ] **Step 1: Add "View Setup Guide" button to Settings**

Find the settings page layout. Add a button at an appropriate location (e.g., in a "General" section or at the bottom):

```tsx
<button
  onClick={async () => {
    await invoke('reset_onboarding')
    navigate('/onboarding')
  }}
  className="... existing button styles ..."
>
  {t('settings.viewSetupGuide')}
</button>
```

- [ ] **Step 2: Add i18n key**

`en.json`: `"viewSetupGuide": "View Setup Guide"`
`ko.json`: `"viewSetupGuide": "설정 가이드 다시 보기"`

(Add under existing `settings` namespace)

- [ ] **Step 3: Verify and commit**

```bash
npx tsc --noEmit
git commit -m "feat(settings): add View Setup Guide button"
```

---

## Feature 2: Suggestion Reception

### Task 8: Add SuggestionConfig to oneshim-core

**Files:**
- Create: `crates/oneshim-core/src/config/sections/suggestion.rs`
- Modify: `crates/oneshim-core/src/config/sections/mod.rs`
- Modify: `crates/oneshim-core/src/config/mod.rs`

- [ ] **Step 1: Create suggestion.rs**

```rust
use serde::{Deserialize, Serialize};

/// Configuration for real-time suggestion reception.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionConfig {
    /// Enable real-time suggestion reception from server.
    #[serde(default)]
    pub enabled: bool,
}

impl Default for SuggestionConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}
```

- [ ] **Step 2: Add re-export and AppConfig field**

In `sections/mod.rs`: `mod suggestion; pub use suggestion::*;`

In `config/mod.rs`: Add `pub suggestions: SuggestionConfig` to `AppConfig` struct and `suggestions: SuggestionConfig::default()` to `default_config()`.

- [ ] **Step 3: Verify**

Run: `cargo check --workspace`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(config): add SuggestionConfig for real-time suggestion reception"
```

---

### Task 9: Create suggestion scheduler loop

**Files:**
- Create: `src-tauri/src/scheduler/loops/suggestions.rs`
- Modify: `src-tauri/src/scheduler/loops/mod.rs`

- [ ] **Step 1: Create suggestions.rs**

```rust
// Note: SuggestionReceiver is NOT re-exported from lib.rs. Use full path,
// or add `pub use receiver::SuggestionReceiver;` to oneshim-suggestion/src/lib.rs.
use oneshim_suggestion::receiver::SuggestionReceiver;
use std::sync::Arc;
use tracing::info;

#[cfg(feature = "server")]
pub(crate) fn spawn_suggestion_loop(
    receiver: Arc<SuggestionReceiver>,
    session_id: String,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("suggestion reception loop started");
        tokio::select! {
            result = receiver.run(&session_id) => {
                match result {
                    Ok(()) => info!("suggestion stream ended normally"),
                    Err(e) => tracing::warn!("suggestion stream error: {e}"),
                }
            }
            _ = shutdown_rx.changed() => {
                info!("suggestion loop shutdown");
            }
        }
    })
}
```

- [ ] **Step 2: Add re-export**

In `loops/mod.rs`: `#[cfg(feature = "server")] pub(crate) mod suggestions;`

- [ ] **Step 3: Verify and commit**

```bash
cargo check -p oneshim-app
git commit -m "feat(suggestions): spawn_suggestion_loop with shutdown integration"
```

---

### Task 10: Wire SuggestionReceiver into Scheduler

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/app_runtime_launch.rs`

- [ ] **Step 1: Add SuggestionReceiver to Scheduler**

Add field:
```rust
    #[cfg(feature = "server")]
    suggestion_receiver: Option<Arc<oneshim_suggestion::receiver::SuggestionReceiver>>,
```

Add builder method:
```rust
    #[cfg(feature = "server")]
    pub fn with_suggestion_receiver(&mut self, receiver: Arc<oneshim_suggestion::SuggestionReceiver>) -> &mut Self {
        self.suggestion_receiver = Some(receiver);
        self
    }
```

In `run_scheduler_loops()`, after health check spawn. Note: `Scheduler.config` is `SchedulerConfig`, not `AppConfig`. Store `suggestions_enabled: bool` on the Scheduler directly (set via builder). Use named variable pattern (not `handles.push()`):

```rust
    #[cfg(feature = "server")]
    let suggestion_task = if self.suggestions_enabled {
        self.suggestion_receiver.as_ref().map(|receiver| {
            loops::suggestions::spawn_suggestion_loop(
                receiver.clone(),
                session_id.clone(),
                shutdown_rx.clone(),
            )
        })
    } else {
        None
    };
```

And in the shutdown sequence:
```rust
    #[cfg(feature = "server")]
    if let Some(t) = suggestion_task { t.abort(); }
```

- [ ] **Step 2: Wire through AgentRuntimeBuilder**

Add field + builder method + forward in `build()`.

- [ ] **Step 3: Create SuggestionReceiver in app_runtime_launch.rs**

```rust
    #[cfg(feature = "server")]
    {
        let (suggestion_tx, _suggestion_rx) = tokio::sync::mpsc::channel(100);
        let suggestion_queue = Arc::new(tokio::sync::Mutex::new(
            oneshim_suggestion::queue::SuggestionQueue::new(50),
        ));
        let suggestion_receiver = Arc::new(oneshim_suggestion::SuggestionReceiver::new(
            sse_client.clone(),  // or create new SseStreamClient
            notifier.clone(),
            suggestion_queue,
            suggestion_tx,
        ));
        builder = builder.with_suggestion_receiver(suggestion_receiver);
    }
```

Note: The exact wiring depends on which SSE/gRPC client is available. Use the existing `SseStreamClient` that the agent runtime creates, or create a new one from config. Check `agent_runtime.rs` for the SSE client creation pattern and follow it.

- [ ] **Step 4: Verify**

Run: `cargo check -p oneshim-app`
Expected: 0 errors

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(suggestions): wire SuggestionReceiver into scheduler"
```

---

## Final

### Task 11: Final verification + cleanup

- [ ] **Step 1: Full workspace check**

Run: `cargo check --workspace`

- [ ] **Step 2: Clippy**

Run: `cargo clippy -p oneshim-app -- -D warnings`

- [ ] **Step 3: All tests**

Run: `cargo test -p oneshim-app`

- [ ] **Step 4: TypeScript**

Run: `cd crates/oneshim-web/frontend && npx tsc --noEmit`

- [ ] **Step 5: Remove stale optimistic initialization**

Verify that `app_runtime_launch.rs` no longer has the optimistic `server_connected.store(true, ...)` lines. The health loop is the single source of truth.
