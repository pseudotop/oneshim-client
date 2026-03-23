# Runtime Health Check + Suggestion Reception + Onboarding Design

Three independent features completing the ONESHIM desktop client's tracking indicator and AI integration layer.

---

## 1. Runtime Health Check

### Problem

Connection status flags (`server_connected`, `llm_connected`, `cli_connected`) are set once at startup and never updated. If the server goes down or LLM becomes unreachable mid-session, the tray icon and menu show stale state.

### Design

**New scheduler loop**: `spawn_health_check_loop()` at 30-second intervals.

**Per-service check strategy**:

| Service | Check Method | Source |
|---------|-------------|--------|
| Server API | Read `server_last_ok: Arc<AtomicBool>` — set by `BatchUploader` on upload success/failure | `oneshim-network` |
| Local LLM | Read `llm_last_ok: Arc<AtomicBool>` — set by `RemoteLlmProvider` / subprocess on request success/failure | `oneshim-network` / `src-tauri` |
| CLI Bridge | Read `cli_last_ok: Arc<AtomicBool>` — set by `AutomationController` on command execution success/failure | `oneshim-automation` |

**Key principle**: The health check loop makes NO network calls. It only reads `Arc<AtomicBool>` flags that are updated by the adapters themselves during normal operation. This avoids mutex contention and keeps the loop lightweight.

**Adapter-side tracking**: Each adapter already processes requests. Add a `last_request_ok: Arc<AtomicBool>` constructor parameter to:
- `BatchUploader` — set on `flush()` success/failure
- `RemoteLlmProvider` — set on `send_and_parse()` success/failure
- `AutomationController` — set on `execute_command()` success/failure

The `Arc<AtomicBool>` flags are created in `app_runtime_launch.rs` and passed to both the adapter constructors AND the health check loop independently. This avoids trait modifications — the flags bypass the port trait layer entirely.

The health check loop reads these flags and updates `AppState.{server,llm,cli}_connected`. On state change:
1. Call `sync_tray_state()` to update tray icon (shape) + menu (✓/✗)
2. Emit `overlay:connection-changed` event to overlay/panel windows

**Event payload schema** for `overlay:connection-changed`:
```json
{ "server": true, "llm": false, "cli": true }
```

**Parameters passed to loop**:
- `Arc<AtomicBool>` × 3 (AppState connection flags — write target)
- `Arc<AtomicBool>` × 3 (adapter last_request_ok flags — read source)
- `AppHandle` (for tray sync + event emission)
- `shutdown_rx: tokio::sync::watch::Receiver<bool>`

### Files

| Action | File |
|--------|------|
| Create | `src-tauri/src/scheduler/loops/health.rs` |
| Modify | `src-tauri/src/scheduler/loops/mod.rs` (re-export) |
| Modify | `src-tauri/src/scheduler/mod.rs` (add health flag fields, spawn loop) |
| Modify | `crates/oneshim-network/src/batch_uploader.rs` (accept `last_upload_ok: Arc<AtomicBool>` in constructor) |
| Modify | `crates/oneshim-network/src/ai_llm_client/mod.rs` (accept `last_request_ok: Arc<AtomicBool>` in constructor) |
| Modify | `crates/oneshim-automation/src/controller/mod.rs` (accept `last_command_ok: Arc<AtomicBool>` in constructor) |
| Modify | `src-tauri/src/app_runtime_launch.rs` (create flags, pass to adapters + scheduler) |

Note: Health flags bypass `AgentRuntimeBuilder` — they are passed directly from `app_runtime_launch.rs` to the scheduler via a new `with_health_flags()` builder method. The `agent_runtime.rs` file is not modified.

### Edge Cases

- **Startup**: All adapter flags AND AppState connection flags start `false`. The existing optimistic initialization in `app_runtime_launch.rs` (setting flags `true` based on config) must be removed — the health loop is now the single source of truth. First successful adapter request sets the adapter flag to `true`, next health tick propagates to AppState.
- **Server feature disabled**: `server_last_ok` never gets set. `server_connected` stays `false`, tray shows ✗. Correct behavior.
- **LLM not configured**: `llm_last_ok` never gets set. `llm_connected` stays `false`. Correct behavior.
- **Rapid oscillation**: Health loop runs at 30s intervals — prevents rapid icon flickering.
- **Adapter never called**: If a service is configured but no requests are made (e.g., LLM enabled but no analysis triggers), the flag stays `false`. This is acceptable — "connected" means "last request succeeded", not "configured".

---

## 2. SSE/gRPC Suggestion Reception

### Problem

`SuggestionReceiver`, `SseStreamClient`, and gRPC `subscribe_suggestions()` are fully implemented but not wired into the scheduler. The client captures context and uploads it but never receives suggestions back.

### Design

**New scheduler loop**: `spawn_suggestion_loop()`.

**Flow**:
```
Scheduler starts
  → Check config: suggestions.enabled && server feature
  → SSE mode: SseStreamClient.connect(session_id) → event stream
  → gRPC mode: UnifiedClient.subscribe_suggestions(session_id) → tonic::Streaming
  → For each received suggestion:
      1. Push to SuggestionQueue (existing, max 50, priority-sorted)
      2. Show desktop notification via DesktopNotifier
      3. Emit overlay:show-suggestion to MagicOverlay
      4. Store in SQLite local_suggestions table
  → On disconnect: auto-reconnect (SSE has built-in exponential backoff)
```

**Shutdown integration**: The suggestion loop wraps `receiver.run()` inside `tokio::select!` against the `shutdown_rx` watch channel. When shutdown is signaled, the loop drops the SSE/gRPC stream and exits cleanly.

```rust
tokio::select! {
    _ = receiver.run(&session_id) => { info!("suggestion stream ended"); }
    _ = shutdown_rx.changed() => { info!("suggestion loop shutdown"); }
}
```

**Event payload schema** for `overlay:show-suggestion`:
```json
{
  "suggestion_id": "uuid",
  "title": "Take a break",
  "content": "You've been coding for 2 hours...",
  "priority": "medium",
  "auto_dismiss_secs": 30
}
```

**Wiring**: The `SuggestionReceiver` struct already handles steps 1-2. We need to:
1. Instantiate `SuggestionReceiver` in `AgentRuntimeBuilder`
2. Pass it to the scheduler
3. Create `spawn_suggestion_loop()` that calls `receiver.run(session_id)` with shutdown integration
4. Add overlay emission for received suggestions

**Config**: Add `SuggestionConfig` to `oneshim-core`:

```rust
/// Configuration for real-time suggestion reception.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionConfig {
    /// Enable real-time suggestion reception from server.
    pub enabled: bool,
}

impl Default for SuggestionConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}
```

Add `pub suggestions: SuggestionConfig` to `AppConfig` and its `Default` impl.

Gate: `#[cfg(feature = "server")]` + `config.suggestions.enabled`

### Files

| Action | File |
|--------|------|
| Create | `src-tauri/src/scheduler/loops/suggestions.rs` |
| Modify | `src-tauri/src/scheduler/loops/mod.rs` (re-export) |
| Modify | `src-tauri/src/scheduler/mod.rs` (add SuggestionReceiver field, spawn loop) |
| Modify | `src-tauri/src/agent_runtime.rs` (create + pass SuggestionReceiver) |
| Create | `crates/oneshim-core/src/config/sections/suggestion.rs` |
| Modify | `crates/oneshim-core/src/config/sections/mod.rs` (add `mod suggestion; pub use suggestion::*;`) |
| Modify | `crates/oneshim-core/src/config/mod.rs` (add `pub suggestions: SuggestionConfig` to `AppConfig` struct + `default_config()`) |

### Edge Cases

- **Server unreachable**: SSE client has built-in reconnect with exponential backoff (1s → 30s max). gRPC mode falls back to REST SSE.
- **Session not created**: Suggestion loop waits for session initialization before connecting.
- **Queue full**: `SuggestionQueue` drops lowest-priority items (existing behavior).
- **Shutdown during reconnect backoff**: `tokio::select!` against shutdown_rx ensures clean exit even during backoff sleep.

---

## 3. First-Run Onboarding

### Problem

New users see the dashboard immediately with no context about what ONESHIM does, what permissions it needs, or how to use it.

### Design

**First-run detection**: SQLite `app_meta` table (V19 migration).

```sql
CREATE TABLE IF NOT EXISTS app_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Bump `CURRENT_VERSION` to 19 in `migration/mod.rs` and add `run_migration_step(conn, 19, migrate_v19)` to the dispatch chain.

Check: `SELECT value FROM app_meta WHERE key = 'onboarding_completed'`. Missing or `"false"` → show onboarding.

**Onboarding route**: `/onboarding` in React Router. Step-based single page:

| Step | Content | Action |
|------|---------|--------|
| 1. Intro | App purpose, what it monitors | Next |
| 2. Permissions | Accessibility, screen capture, notifications | Grant / Skip |
| 3. Features | Capture, analysis, suggestions overview | Next |
| 4. Start | Ready to go, link to settings | Complete |

**Entry point — frontend-driven**: Instead of driving navigation from Rust (`desktop_startup.rs`), the React `App.tsx` handles onboarding routing on mount:

```tsx
// In App.tsx, at the top level inside the Router:
const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null)

useEffect(() => {
  invoke<{ completed: boolean }>('get_onboarding_status')
    .then(r => setOnboardingDone(r.completed))
    .catch(() => setOnboardingDone(true)) // standalone/dev mode — skip
}, [])

if (onboardingDone === null) return null // loading
if (!onboardingDone) return <Navigate to="/onboarding" />
```

This keeps routing logic in the frontend where `react-router-dom` controls navigation. No Rust-side navigation needed.

**IPC commands**:
- `get_onboarding_status() → { completed: bool }`
- `complete_onboarding()` — sets `app_meta.onboarding_completed = "true"`
- `reset_onboarding()` — deletes the key (for "View guide again" in Settings)

**Storage**: Add methods to `SqliteStorage`:
- `pub fn get_meta(&self, key: &str) -> Option<String>` (sync, called via `block_in_place`)
- `pub fn set_meta(&self, key: &str, value: &str)` (sync)
- `pub fn delete_meta(&self, key: &str)` (sync)

**Settings integration**: Add "View Setup Guide" button in Settings page that calls `reset_onboarding()` + navigates to `/onboarding`. i18n keys: `settings.viewSetupGuide` (en: "View Setup Guide", ko: "설정 가이드 다시 보기").

### Files

| Action | File |
|--------|------|
| Modify | `crates/oneshim-storage/src/migration/mod.rs` (V19: app_meta table, bump CURRENT_VERSION to 19, add dispatch) |
| Create or Modify | `crates/oneshim-storage/src/migration/v19.rs` (or append to existing v09_v18.rs — follow codebase convention) |
| Modify | `crates/oneshim-storage/src/sqlite.rs` (get_meta/set_meta/delete_meta methods) |
| Create | `src-tauri/src/commands/onboarding.rs` |
| Modify | `src-tauri/src/commands/mod.rs` (add module) |
| Modify | `src-tauri/src/main.rs` (register IPC commands) |
| Create | `crates/oneshim-web/frontend/src/pages/Onboarding.tsx` |
| Modify | `crates/oneshim-web/frontend/src/App.tsx` (add route + onboarding gate) |
| Modify | `crates/oneshim-web/frontend/src/pages/Settings.tsx` (add "View Guide" button) |
| Modify | `crates/oneshim-web/frontend/src/i18n/locales/en.json` (onboarding + settings keys) |
| Modify | `crates/oneshim-web/frontend/src/i18n/locales/ko.json` (onboarding + settings keys) |

### Edge Cases

- **Upgrade from existing install**: `app_meta` table doesn't exist → V19 migration creates it → `onboarding_completed` key missing → shows onboarding. This is intentional — existing users get to see the guide once.
- **Skip onboarding**: User can close the window or click "Skip" → does NOT set completed → shows again next launch. Only "Complete" button sets the flag.
- **Permissions already granted**: Step 2 checks current permission state and shows ✓ for already-granted permissions.
- **Standalone/dev mode**: IPC invoke fails → `catch(() => setOnboardingDone(true))` — skips onboarding gracefully.

---

## Dependencies Between Features

```
Feature 1 (Health Check) ← independent
Feature 2 (Suggestions)  ← requires server connection (health check provides status visibility)
Feature 3 (Onboarding)   ← independent (frontend-only + SQLite)
```

Features 1 and 3 are fully independent. Feature 2 benefits from Feature 1's health status visibility but doesn't depend on it — it has its own reconnect logic.

**Implementation order**: 1 → 3 → 2 (health check first establishes the adapter tracking pattern, onboarding is isolated frontend work, suggestions is the most complex integration).

---

## Testing Strategy

| Feature | Test Type | What |
|---------|-----------|------|
| Health Check | Unit | Loop tick reads flags correctly, state change detection |
| Health Check | Unit | Adapter `last_request_ok` set on success/failure paths |
| Suggestions | Unit | SuggestionReceiver already tested — new tests for loop spawn/shutdown |
| Suggestions | Unit | `SuggestionConfig` default + serde roundtrip |
| Onboarding | Unit | `get_meta`/`set_meta`/`delete_meta` SQLite roundtrip |
| Onboarding | Unit | V19 migration creates `app_meta` table |
| Onboarding | Unit | IPC command responses (get/complete/reset) |
| Onboarding | Frontend | Component render, step navigation, completion flow |
