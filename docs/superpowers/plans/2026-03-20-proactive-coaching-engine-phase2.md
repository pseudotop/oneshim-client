# Proactive Coaching Engine Phase 2 — MagicOverlay + React UI Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the MagicOverlay transparent WebView window and its React frontend, wire Tauri IPC commands between the coaching engine (Phase 1) and the overlay, enable background LLM personalization of coaching messages, and add coaching history and regime goal settings pages to the dashboard.

**Prerequisites:** Phase 1 must be complete. The following must exist and pass tests:
- `CoachingEngine` in `oneshim-analysis` (trigger evaluation, template messaging, feedback tracking)
- `CoachingConfig` in `oneshim-core/config` (including `overlay_mode`, `overlay_hotkey`, `regime_goals`)
- Coaching models in `oneshim-core/models/coaching.rs` (`CoachingMessage`, `DismissAction`, `GoalProgress`, `GoalProgressView`, etc.)
- V17 storage migration (`coaching_events`, `regime_goals`, `coaching_effectiveness` tables)
- `CoachingTemplateRegistry` with 50+ templates
- `RegimeGoalTracker` and `FeedbackTracker`
- `coaching_loop` in the scheduler + `notify_coaching()` in `NotificationManager`

**Architecture:** The MagicOverlay is a second Tauri WebView window (transparent, always-on-top, click-through) that renders a lightweight React app. The main dashboard window communicates with it via Tauri events. The Rust backend exposes IPC commands that the overlay React app calls for dismissal/feedback, and that the scheduler calls (via `AppHandle.emit()`) to push coaching messages. The LLM personalization spawns a background `tokio::spawn` task using the existing `AnalysisProvider` port.

**Tech Stack:**
- Rust: Tauri v2 (`WebviewWindowBuilder`, `AppHandle.emit`, `#[tauri::command]`), tokio, serde
- Frontend: React 18, TypeScript, Tailwind CSS, `@tauri-apps/api/core` (invoke), `@tauri-apps/api/event` (listen), Recharts (charts)
- Existing patterns: `PomodoroTimer.tsx` (floating widget), `DashboardDay.tsx` (page layout), `useRecalibration.ts` (hooks), `tauriInvoke()` (IPC wrapper)

**Spec:** `docs/superpowers/specs/2026-03-20-proactive-coaching-engine-design.md` (Sections 4.7, 4.8)
**Phase 1:** `docs/superpowers/plans/2026-03-20-proactive-coaching-engine-phase1.md`

---

## File Map

### New files

| File | Content |
|------|---------|
| `src-tauri/src/magic_overlay.rs` | `MagicOverlayHandle` — Tauri WebView window creation, event emission, state management |
| `src-tauri/capabilities/overlay.json` | Tauri v2 capability permissions for the overlay window (events, cursor, show/hide) |
| `crates/oneshim-web/frontend/src/overlay/main.tsx` | Overlay React app entry point (separate from dashboard) |
| `crates/oneshim-web/frontend/src/overlay/App.tsx` | Overlay root component — layout, event listeners, mode state |
| `crates/oneshim-web/frontend/src/overlay/components/CoachingPopup.tsx` | Coaching message bubble with OK/Later/thumbs actions |
| `crates/oneshim-web/frontend/src/overlay/components/FocusHighlight.tsx` | Translucent border around focused element |
| `crates/oneshim-web/frontend/src/overlay/components/GoalProgressBar.tsx` | Bottom regime goal progress bar (Rich mode) |
| `crates/oneshim-web/frontend/src/overlay/components/HeatmapGhost.tsx` | Attention heatmap ghost layer (Rich mode, optional) |
| `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts` | Tauri event listener hook for overlay commands |
| `crates/oneshim-web/frontend/src/overlay/hooks/useAutoDissmiss.ts` | 15-second auto-dismiss timer hook |
| `crates/oneshim-web/frontend/src/overlay/types.ts` | TypeScript interfaces for overlay IPC payloads |
| `crates/oneshim-web/frontend/src/overlay/index.css` | Overlay-specific Tailwind CSS with transparent background |
| `crates/oneshim-web/frontend/overlay.html` | HTML entry point for the overlay window |
| `crates/oneshim-web/frontend/src/pages/Coaching.tsx` | Coaching history page (`/coaching` route) |
| `crates/oneshim-web/frontend/src/pages/settingSections/CoachingGoalsTab.tsx` | Regime goal settings tab |
| `crates/oneshim-web/frontend/src/hooks/useCoaching.ts` | React Query hooks for coaching API endpoints |
| `crates/oneshim-web/frontend/src/api/coaching.ts` | API client functions for coaching REST + IPC endpoints |

### Modified files

| File | Change |
|------|--------|
| `src-tauri/src/main.rs` | Add `mod magic_overlay;` declaration |
| `src-tauri/src/commands.rs` | Add 8 coaching IPC commands |
| `src-tauri/src/runtime_state.rs` | Add `MagicOverlayHandle` + `coaching_engine` to `AppState` |
| `src-tauri/src/scheduler/loops.rs` | Wire `MagicOverlayHandle` + LLM spawn into coaching evaluation |
| `src-tauri/src/scheduler/mod.rs` | Store `MagicOverlayHandle` reference; pass to coaching loop |
| `src-tauri/tauri.conf.json` | No static window needed (overlay created at runtime via `WebviewWindowBuilder`) |
| `crates/oneshim-web/frontend/vite.config.ts` | Add `overlay` entry point to multi-page build |
| `crates/oneshim-web/frontend/src/App.tsx` | Add `/coaching` route |
| `crates/oneshim-web/frontend/src/pages/Settings.tsx` | Add coaching goals tab |
| `crates/oneshim-web/frontend/src/components/shell/TreeView.tsx` | Add "Coaching" nav entry |
| `crates/oneshim-web/frontend/src/i18n/index.ts` | Add coaching i18n keys (en/ko) |
| `crates/oneshim-web/src/routes.rs` | Add `/api/coaching/*` REST endpoints |
| `crates/oneshim-web/src/handlers/mod.rs` | Add `pub mod coaching;` |
| `src-tauri/Cargo.toml` | Add `tauri-plugin-global-shortcut` dependency |

---

## Task 1: MagicOverlayHandle — Tauri window manager

**Why:** The overlay is a second WebView window managed entirely from Rust. It must be created at runtime (not in `tauri.conf.json`) because it needs transparent background and always-on-top, which require programmatic `WebviewWindowBuilder` configuration. This is the foundational infrastructure all overlay UI depends on.

**Files:**
- Create: `src-tauri/src/magic_overlay.rs`
- Create: `src-tauri/capabilities/overlay.json`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/runtime_state.rs`

- [ ] **Step 1.1: Create `MagicOverlayHandle` struct**

Create `src-tauri/src/magic_overlay.rs` with:

```rust
use oneshim_core::config::OverlayMode;
use oneshim_core::models::coaching::{CoachingMessage, DismissAction, GoalProgressView};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

const OVERLAY_LABEL: &str = "magic-overlay";
const OVERLAY_URL: &str = "overlay.html";

/// Serializable payload emitted to the overlay WebView via Tauri events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayCoachingPayload {
    pub message_id: String,
    pub profile: String,
    pub trigger_type: String,
    pub text: String,
    pub auto_dismiss_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayUpgradePayload {
    pub message_id: String,
    pub personalized_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayFocusPayload {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub border_color: String,
    pub opacity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayGoalPayload {
    pub goals: Vec<GoalProgressView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayModePayload {
    pub mode: OverlayMode,
}

#[derive(Debug)]
struct OverlayState {
    mode: OverlayMode,
    visible: bool,
    current_message_id: Option<String>,
}

/// Handle for managing the MagicOverlay Tauri WebView window.
///
/// Created once during app setup. The overlay window is lazily created
/// on the first `show_coaching()` call and kept alive (hidden when idle).
#[derive(Clone)]
pub struct MagicOverlayHandle {
    app_handle: AppHandle,
    state: Arc<RwLock<OverlayState>>,
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 1.2: Implement window lifecycle methods**

Add the following methods to `MagicOverlayHandle`:

- `pub fn new(app_handle: AppHandle, initial_mode: OverlayMode) -> Self`
- `async fn ensure_window(&self) -> Result<(), String>` — creates the overlay window if it does not exist. Uses `WebviewWindowBuilder::new()` with:
  - `label`: `OVERLAY_LABEL`
  - `url`: `WebviewUrl::App(OVERLAY_URL.into())`
  - `transparent(true)`
  - `always_on_top(true)`
  - `decorations(false)`
  - `resizable(false)`
  - `visible(false)` initially
  - `skip_taskbar(true)`
  - Full-screen dimensions from primary monitor (`app_handle.primary_monitor()`)
  - macOS: `.title_bar_style(tauri::TitleBarStyle::Transparent)` and `.hidden_title(true)`
- `pub async fn show_coaching(&self, message: &CoachingMessage)` — calls `ensure_window()`, emits `overlay:show-coaching` event with `OverlayCoachingPayload`, sets window visible, updates `current_message_id`.
- `pub async fn upgrade_message(&self, message_id: &str, personalized_text: &str)` — only emits if `current_message_id` matches and window is visible. Emits `overlay:upgrade-message` event.
- `pub async fn dismiss(&self, message_id: &str, action: DismissAction)` — emits `overlay:dismiss` event, clears `current_message_id`. Hides window if no other content is displayed.
- `pub async fn update_focus_highlight(&self, highlight: OverlayFocusPayload)` — emits `overlay:update-focus` event.
- `pub async fn update_goal_progress(&self, goals: Vec<GoalProgressView>)` — emits `overlay:update-goals` event.
- `pub async fn set_mode(&self, mode: OverlayMode)` — updates internal state and emits `overlay:set-mode` event.
- `pub async fn get_mode(&self) -> OverlayMode` — reads current mode.
- `pub async fn is_visible(&self) -> bool` — reads visibility state.
- `pub async fn toggle_mode(&self)` — cycles Minimal -> Rich -> Adaptive -> Minimal.

Window creation pattern (reference `WebviewWindowBuilder` from Tauri v2):

```rust
async fn ensure_window(&self) -> Result<(), String> {
    // Check if window already exists
    if self.app_handle.get_webview_window(OVERLAY_LABEL).is_some() {
        return Ok(());
    }

    let monitor = self.app_handle
        .primary_monitor()
        .map_err(|e| format!("Failed to get primary monitor: {e}"))?
        .ok_or_else(|| "No primary monitor found".to_string())?;

    let size = monitor.size();

    let _window = WebviewWindowBuilder::new(
        &self.app_handle,
        OVERLAY_LABEL,
        WebviewUrl::App(OVERLAY_URL.into()),
    )
    .title("ONESHIM Overlay")
    .inner_size(size.width as f64, size.height as f64)
    .position(0.0, 0.0)
    .transparent(true)
    .always_on_top(true)
    .decorations(false)
    .resizable(false)
    .visible(false)
    .skip_taskbar(true)
    .build()
    .map_err(|e| format!("Failed to create overlay window: {e}"))?;

    info!("MagicOverlay window created");
    Ok(())
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 1.3: Register module and add to AppState**

In `src-tauri/src/main.rs`, add `mod magic_overlay;`.

In `src-tauri/src/runtime_state.rs`, add to `AppState`:

```rust
pub magic_overlay: Option<magic_overlay::MagicOverlayHandle>,
```

Initialize it during app setup (after `AppHandle` is available):

```rust
let overlay_handle = magic_overlay::MagicOverlayHandle::new(
    app.handle().clone(),
    config.coaching.overlay_mode,
);
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 1.4: Create Tauri capability permissions for overlay**

The overlay window requires explicit Tauri v2 capability permissions to use event listening, cursor passthrough, and window show/hide APIs. Without this file, IPC calls from the overlay React app will be rejected at runtime.

Create `src-tauri/capabilities/overlay.json`:

```json
{
  "identifier": "overlay",
  "windows": ["magic-overlay"],
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "core:window:allow-set-ignore-cursor-events",
    "core:window:allow-show",
    "core:window:allow-hide"
  ]
}
```

Tauri v2 auto-discovers capability files in the `src-tauri/capabilities/` directory. No additional registration is needed in `tauri.conf.json`.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 1.5: Add click-through behavior**

The overlay window must be click-through except for interactive elements (the coaching popup, buttons). Tauri v2 supports `ignore_cursor_events(true)` on the window level. However, we need the popup to be interactive.

Strategy: use Tauri's `set_ignore_cursor_events` API. The overlay React app calls `set_ignore_cursor_events(false)` when the mouse enters an interactive element and `set_ignore_cursor_events(true)` when it leaves. This requires a Tauri IPC command.

Add to `magic_overlay.rs`:

```rust
/// Set whether the overlay window ignores cursor events.
/// Called from React when mouse enters/leaves interactive elements.
pub async fn set_cursor_passthrough(&self, passthrough: bool) {
    if let Some(window) = self.app_handle.get_webview_window(OVERLAY_LABEL) {
        let _ = window.set_ignore_cursor_events(passthrough);
    }
}
```

Default state: `ignore_cursor_events(true)` set during window creation.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 1.6: Add tests**

Add `#[cfg(test)] mod tests` at the bottom of `magic_overlay.rs`:
- `overlay_state_default_mode`: create `OverlayState` with `Minimal`, assert mode and visibility.
- `overlay_coaching_payload_serde_roundtrip`: serialize/deserialize `OverlayCoachingPayload`.
- `overlay_upgrade_payload_serde_roundtrip`: serialize/deserialize `OverlayUpgradePayload`.
- `overlay_focus_payload_serde_roundtrip`: serialize/deserialize `OverlayFocusPayload`.
- `overlay_goal_payload_serde_roundtrip`: serialize/deserialize `OverlayGoalPayload`.

Note: `MagicOverlayHandle` methods that require a real `AppHandle` cannot be tested in unit tests. They are covered by manual testing and integration tests.

```
cargo test -p oneshim-tauri -- magic_overlay
```

---

## Task 2: Tauri IPC commands for coaching

**Why:** The React overlay and dashboard apps need to communicate with the Rust backend. IPC commands bridge the gap. These commands are called by the overlay UI (dismiss, feedback, cursor passthrough) and by the dashboard pages (get history, update goals).

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/runtime_state.rs` (promote `coaching_engine` to `AppState`)

- [ ] **Step 2.1: Promote `coaching_engine` to `AppState`**

The IPC commands in this task reference `state.coaching_engine` for goal progress, feedback recording, and snooze control. In Phase 1, `CoachingEngine` is only held inside the `Scheduler`. It must also be available in `AppState` (or a shared `CoachingState` struct) so that the Tauri IPC commands can access it.

In `src-tauri/src/runtime_state.rs`, add:

```rust
pub coaching_engine: Option<Arc<CoachingEngine>>,
```

During app setup, create the `CoachingEngine` instance once with `Arc`, then share it with both the `Scheduler` and `AppState`:

```rust
let coaching_engine = Arc::new(CoachingEngine::new(/* ... */));

// Pass to Scheduler
let scheduler = Scheduler::new(/* ..., coaching_engine: Some(coaching_engine.clone()), ... */);

// Store in AppState for IPC access
let app_state = AppState {
    // ... existing fields ...
    coaching_engine: Some(coaching_engine.clone()),
    // ...
};
```

This ensures that IPC commands (`dismiss_coaching_message`, `submit_coaching_feedback`, `get_goal_progress`, `update_regime_goals`) can access the engine directly from `AppState`, while the scheduler's coaching loop continues to use the same engine instance.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 2.2: Add `dismiss_coaching_message` command**

```rust
#[command]
pub async fn dismiss_coaching_message(
    state: tauri::State<'_, AppState>,
    message_id: String,
    action: String, // "ok" | "later" | "timeout"
) -> Result<(), String> {
    let dismiss_action = match action.as_str() {
        "ok" => DismissAction::Ok,
        "later" => DismissAction::Later,
        "timeout" => DismissAction::Timeout,
        _ => return Err(format!("Invalid dismiss action: {action}")),
    };

    if let Some(ref overlay) = state.magic_overlay {
        overlay.dismiss(&message_id, dismiss_action).await;
    }

    // If "Later", snooze the profile for 15 minutes via CoachingEngine
    if dismiss_action == DismissAction::Later {
        if let Some(ref engine) = state.coaching_engine {
            engine.snooze_current_profile(std::time::Duration::from_secs(900)).await;
        }
    }

    Ok(())
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 2.3: Add `submit_coaching_feedback` command**

```rust
#[command]
pub async fn submit_coaching_feedback(
    state: tauri::State<'_, AppState>,
    message_id: String,
    positive: bool,
) -> Result<(), String> {
    if let Some(ref engine) = state.coaching_engine {
        engine.record_explicit_feedback(&message_id, positive).await;
    }
    Ok(())
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 2.4: Add `set_overlay_mode` and `get_overlay_state` commands**

```rust
#[command]
pub async fn set_overlay_mode(
    state: tauri::State<'_, AppState>,
    mode: String, // "minimal" | "rich" | "adaptive"
) -> Result<(), String> {
    let overlay_mode = match mode.as_str() {
        "minimal" => OverlayMode::Minimal,
        "rich" => OverlayMode::Rich,
        "adaptive" => OverlayMode::Adaptive,
        _ => return Err(format!("Invalid overlay mode: {mode}")),
    };

    if let Some(ref overlay) = state.magic_overlay {
        overlay.set_mode(overlay_mode).await;
    }
    Ok(())
}

#[command]
pub async fn get_overlay_state(
    state: tauri::State<'_, AppState>,
) -> Result<OverlayStateResponse, String> {
    let (mode, visible) = if let Some(ref overlay) = state.magic_overlay {
        (overlay.get_mode().await, overlay.is_visible().await)
    } else {
        (OverlayMode::Minimal, false)
    };

    Ok(OverlayStateResponse {
        mode: format!("{:?}", mode).to_lowercase(),
        visible,
    })
}

#[derive(Serialize)]
pub struct OverlayStateResponse {
    pub mode: String,
    pub visible: bool,
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 2.5: Add `set_overlay_cursor_passthrough` command**

```rust
#[command]
pub async fn set_overlay_cursor_passthrough(
    state: tauri::State<'_, AppState>,
    passthrough: bool,
) -> Result<(), String> {
    if let Some(ref overlay) = state.magic_overlay {
        overlay.set_cursor_passthrough(passthrough).await;
    }
    Ok(())
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 2.6: Add `get_coaching_history` command**

Returns coaching events from SQLite for the history page.

```rust
#[command]
pub async fn get_coaching_history(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<CoachingEventRow>, String> {
    let storage = state.storage.as_ref()
        .ok_or("Storage not available")?;

    storage.query_coaching_events(
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    ).await.map_err(|e| e.to_string())
}
```

This requires adding a `query_coaching_events()` method to the storage adapter (see Task 3).

```
cargo check -p oneshim-tauri
```

- [ ] **Step 2.7: Add `get_goal_progress` and `update_regime_goals` commands**

```rust
#[command]
pub async fn get_goal_progress(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<GoalProgressView>, String> {
    if let Some(ref engine) = state.coaching_engine {
        Ok(engine.all_goal_progress().await)
    } else {
        Ok(vec![])
    }
}

#[command]
pub async fn update_regime_goals(
    state: tauri::State<'_, AppState>,
    goals: std::collections::HashMap<String, u32>,
) -> Result<(), String> {
    if let Some(ref engine) = state.coaching_engine {
        engine.update_regime_goals(&goals).await;
    }

    // Persist to config file
    if let Some(ref config_manager) = state.config_manager {
        let mut config = config_manager.load().map_err(|e| e.to_string())?;
        config.coaching.regime_goals = goals;
        config_manager.save(&config).map_err(|e| e.to_string())?;
    }

    Ok(())
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 2.8: Register all new commands in Tauri builder**

In `src-tauri/src/setup.rs` (or wherever `tauri::Builder` is configured), add the new commands to `.invoke_handler(tauri::generate_handler![...])`:

```rust
dismiss_coaching_message,
submit_coaching_feedback,
set_overlay_mode,
get_overlay_state,
set_overlay_cursor_passthrough,
get_coaching_history,
get_goal_progress,
update_regime_goals,
```

```
cargo check -p oneshim-tauri
```

> **Phase 1 API surface note:** Steps 2.2 and 2.7 reference `CoachingEngine` methods that may not exist from Phase 1. If the following methods are not already present on `CoachingEngine`, they must be added as part of this task:
>
> - **`snooze_current_profile(duration: Duration)`** — Sets a temporary cooldown override on the currently active coaching profile. While snoozed, `evaluate()` skips that profile's triggers. The snooze expires after `duration` elapses. Implementation: store `(profile_name, Instant::now() + duration)` in the engine state; check it at the start of `evaluate()`.
> - **`all_goal_progress() -> Vec<GoalProgressView>`** — Returns goal progress for all configured regimes. Delegates to `RegimeGoalTracker::all_progress()`, which iterates over the `regime_goals` config map and computes `current_minutes` from today's tracked time per regime label.
>
> If these methods already exist from Phase 1, no action is needed here.

---

## Task 3: Storage adapter — coaching query methods

**Why:** The coaching history page and goal settings UI need to read and write coaching data from SQLite. The V17 tables already exist (from Phase 1), but query methods on the storage adapter are needed.

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite.rs` (or appropriate storage adapter file)
- Modify: `crates/oneshim-core/src/ports/storage.rs` (if adding port methods)

- [ ] **Step 3.1: Add `CoachingEventRow` response type**

In `crates/oneshim-core/src/models/coaching.rs`, add:

```rust
/// Storage query result for coaching history display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachingEventRow {
    pub event_id: String,
    pub trigger_type: String,
    pub profile_name: String,
    pub regime_id: Option<String>,
    pub message_template: String,
    pub personalized_message: Option<String>,
    pub shown_at: String,
    pub dismissed_at: Option<String>,
    pub dismiss_action: Option<String>,
    pub feedback_type: Option<String>,
    pub feedback_score: Option<f64>,
}
```

```
cargo check -p oneshim-core
```

- [ ] **Step 3.2: Add coaching query methods to storage**

Add to the storage adapter (either directly on `SqliteStorage` or as an extension trait):

```rust
/// Query coaching events, newest first, with pagination.
pub async fn query_coaching_events(
    &self,
    limit: u32,
    offset: u32,
) -> Result<Vec<CoachingEventRow>, CoreError>

/// Insert a coaching event record.
pub async fn insert_coaching_event(
    &self,
    event: &CoachingEventRow,
) -> Result<(), CoreError>

/// Update coaching event with dismiss/feedback data.
pub async fn update_coaching_event_feedback(
    &self,
    event_id: &str,
    dismiss_action: Option<&str>,
    dismissed_at: Option<&str>,
    feedback_type: Option<&str>,
    feedback_score: Option<f64>,
) -> Result<(), CoreError>

/// Get/set regime goals from the regime_goals table.
pub async fn get_regime_goals(&self) -> Result<HashMap<String, u32>, CoreError>
pub async fn set_regime_goal(&self, regime_label: &str, target_minutes: u32) -> Result<(), CoreError>
pub async fn delete_regime_goal(&self, regime_label: &str) -> Result<(), CoreError>

/// Get/upsert coaching effectiveness scores.
pub async fn get_coaching_effectiveness(&self) -> Result<Vec<EffectivenessRow>, CoreError>
pub async fn upsert_coaching_effectiveness(
    &self,
    profile_name: &str,
    trigger_type: &str,
    score: &EffectivenessScore,
) -> Result<(), CoreError>
```

Implement each using rusqlite queries against the V17 tables:
- `query_coaching_events`: `SELECT * FROM coaching_events ORDER BY shown_at DESC LIMIT ? OFFSET ?`
- `insert_coaching_event`: `INSERT INTO coaching_events (...) VALUES (...)`
- `update_coaching_event_feedback`: `UPDATE coaching_events SET ... WHERE event_id = ?`
- `get_regime_goals`: `SELECT regime_label, daily_target_minutes FROM regime_goals`
- `set_regime_goal`: `INSERT OR REPLACE INTO regime_goals (...) VALUES (...)`
- `delete_regime_goal`: `DELETE FROM regime_goals WHERE regime_label = ?`
- `get_coaching_effectiveness`: `SELECT * FROM coaching_effectiveness`
- `upsert_coaching_effectiveness`: `INSERT OR REPLACE INTO coaching_effectiveness (...) VALUES (...)`

```
cargo check -p oneshim-storage
```

- [ ] **Step 3.3: Add storage query tests**

Add tests in `#[cfg(test)]`:
- `insert_and_query_coaching_event`: insert a record, query it back, verify fields.
- `query_coaching_events_pagination`: insert 5 records, query with limit=2 offset=2, verify 2 results.
- `set_and_get_regime_goals`: set 3 goals, get all, verify count and values.
- `upsert_coaching_effectiveness`: upsert twice, verify update (not duplicate).

```
cargo test -p oneshim-storage -- coaching
```

---

## Task 4: Wire LLM personalization into scheduler

**Why:** Phase 1 delivers template-only messages. Phase 2 upgrades them with LLM personalization. After the template message is shown on the overlay, a background task calls the `AnalysisProvider` and, if the overlay is still visible, upgrades the message text.

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs`
- Modify: `src-tauri/src/scheduler/mod.rs`

- [ ] **Step 4.1: Add `MagicOverlayHandle` to scheduler**

In `src-tauri/src/scheduler/mod.rs`, add:

```rust
magic_overlay: Option<Arc<magic_overlay::MagicOverlayHandle>>,
```

Pass it in from the `Scheduler` constructor.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 4.2: Replace desktop notification with overlay delivery**

In the coaching evaluation section of `spawn_monitor_loop()` (or `spawn_coaching_loop()`), replace the Phase 1 notification-only delivery with overlay-primary delivery:

```rust
if let Some(message) = coaching.evaluate(/* ... */).await {
    // 1. Show on MagicOverlay (primary)
    if let Some(ref overlay) = overlay_ref {
        overlay.show_coaching(&message).await;
    }

    // 2. Also send desktop notification (fallback)
    if let Some(ref notif) = notif1 {
        notif.notify_coaching(&message.template_text).await;
    }

    // 3. Persist coaching event to storage
    if let Some(ref storage) = storage_ref {
        let _ = storage.insert_coaching_event(&CoachingEventRow::from(&message)).await;
    }

    // 4. Register for feedback tracking
    coaching.register_pending_feedback(/* ... */).await;

    // 5. Spawn background LLM personalization
    let msg_clone = message.clone();
    let provider = analysis_provider.clone();
    let overlay_clone = overlay_ref.clone();
    let storage_clone = storage_ref.clone();
    tokio::spawn(async move {
        let prompt = build_personalization_prompt(
            &msg_clone.template_text,
            &regime_label,
            &history_summary,
            goal_progress.as_ref(),
            tone,
        );
        match provider.analyze(&prompt, COACHING_SYSTEM_PROMPT).await {
            Ok(suggestions) if !suggestions.is_empty() => {
                let personalized = &suggestions[0].content;
                // Upgrade overlay if still visible
                if let Some(ref overlay) = overlay_clone {
                    overlay.upgrade_message(&msg_clone.message_id, personalized).await;
                }
                // Persist personalized text to storage
                if let Some(ref storage) = storage_clone {
                    let _ = storage.update_coaching_event_personalized(
                        &msg_clone.message_id,
                        personalized,
                    ).await;
                }
            }
            Ok(_) => { /* No suggestions returned — template remains */ }
            Err(e) => {
                debug!("LLM coaching personalization failed: {e}");
            }
        }
    });
}
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 4.3: Implement `build_personalization_prompt()` function**

Add to `src-tauri/src/scheduler/loops.rs` (or a dedicated `coaching_support.rs` file):

```rust
const COACHING_SYSTEM_PROMPT: &str =
    "You are a concise productivity coach. Rewrite the given message \
     to be more personalized and contextual. Keep the same intent. \
     Respond with ONLY the rewritten message, no preamble.";

fn build_personalization_prompt(
    template_text: &str,
    regime_label: &str,
    history_summary: &str,
    goal_progress: Option<&GoalProgress>,
    tone: CoachingTone,
) -> String {
    let goal_section = match goal_progress {
        Some(gp) => format!(
            "Goal progress: {}min / {}min ({}%)\n",
            gp.current_minutes, gp.target_minutes, gp.percentage
        ),
        None => String::new(),
    };

    format!(
        "Rewrite this productivity coaching message to be more personalized \
         and contextual. Keep the same intent and information, but make it \
         feel natural.\n\n\
         Original: {template_text}\n\
         Current regime: {regime_label}\n\
         Recent history: {history_summary}\n\
         {goal_section}\
         Tone: {tone:?}\n\
         Respond with ONLY the rewritten message, no preamble.",
    )
}
```

```
cargo check -p oneshim-tauri
```

---

## Task 5: Overlay React app — entry point and build config

**Why:** The overlay is a separate React app from the main dashboard. It needs its own HTML entry point, CSS (transparent background), and Vite build entry. This is the foundation for all overlay React components.

**Files:**
- Create: `crates/oneshim-web/frontend/overlay.html`
- Create: `crates/oneshim-web/frontend/src/overlay/main.tsx`
- Create: `crates/oneshim-web/frontend/src/overlay/index.css`
- Modify: `crates/oneshim-web/frontend/vite.config.ts`

- [ ] **Step 5.1: Create overlay HTML entry**

Create `crates/oneshim-web/frontend/overlay.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>ONESHIM Overlay</title>
</head>
<body class="overlay-body">
  <div id="overlay-root"></div>
  <script type="module" src="/src/overlay/main.tsx"></script>
</body>
</html>
```

- [ ] **Step 5.2: Create overlay CSS**

Create `crates/oneshim-web/frontend/src/overlay/index.css`:

```css
@import 'tailwindcss';

/* Overlay-specific: transparent background, no scrollbars */
.overlay-body {
  margin: 0;
  padding: 0;
  background: transparent;
  overflow: hidden;
  user-select: none;
  pointer-events: none;  /* Default: click-through */
}

/* Interactive elements opt back in to pointer events */
.overlay-interactive {
  pointer-events: auto;
}
```

- [ ] **Step 5.3: Create overlay React entry point**

Create `crates/oneshim-web/frontend/src/overlay/main.tsx`:

```tsx
import React from 'react'
import ReactDOM from 'react-dom/client'
import OverlayApp from './App'
import './index.css'

ReactDOM.createRoot(document.getElementById('overlay-root')!).render(
  <React.StrictMode>
    <OverlayApp />
  </React.StrictMode>,
)
```

- [ ] **Step 5.4: Update Vite config for multi-page build**

Modify `crates/oneshim-web/frontend/vite.config.ts` to add the overlay entry point. **Important:** The existing config already has `build.rollupOptions.output.manualChunks` for vendor chunk splitting. You must merge the new `input` entries into the existing `rollupOptions` without overwriting `output.manualChunks`.

Add `input` alongside the existing `output`:

```ts
import { resolve } from 'path'

export default defineConfig({
  // ... existing config ...
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        overlay: resolve(__dirname, 'overlay.html'),
      },
      output: {
        manualChunks: { /* preserve existing manualChunks entries */ },
      },
    },
  },
})
```

In practice, this means inserting the `input` block into the existing `rollupOptions` object rather than replacing it. The existing `manualChunks` configuration (e.g., vendor splitting for `react`, `recharts`, `@tanstack`) must be preserved exactly as-is.

This ensures both the dashboard and overlay HTML files are built and included in the Tauri bundle, while maintaining the existing chunk splitting optimization.

```
cd crates/oneshim-web/frontend && pnpm build
```

---

## Task 6: Overlay TypeScript types and event hooks

**Why:** The overlay React app receives commands from Rust via Tauri events. TypeScript interfaces and a hook that listens for these events are needed before building UI components.

**Files:**
- Create: `crates/oneshim-web/frontend/src/overlay/types.ts`
- Create: `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts`
- Create: `crates/oneshim-web/frontend/src/overlay/hooks/useAutoDissmiss.ts`

- [ ] **Step 6.1: Define TypeScript interfaces**

Create `crates/oneshim-web/frontend/src/overlay/types.ts`:

```ts
export type OverlayMode = 'minimal' | 'rich' | 'adaptive'
export type DismissAction = 'ok' | 'later' | 'timeout'

export interface CoachingPayload {
  message_id: string
  profile: string
  trigger_type: string
  text: string
  auto_dismiss_secs: number
}

export interface UpgradePayload {
  message_id: string
  personalized_text: string
}

export interface FocusHighlightPayload {
  x: number
  y: number
  width: number
  height: number
  border_color: string
  opacity: number
}

export interface GoalProgressItem {
  regime_label: string
  current_minutes: number
  target_minutes: number
  percentage: number
  display_color: string
}

export interface GoalPayload {
  goals: GoalProgressItem[]
}

export interface ModePayload {
  mode: OverlayMode
}

export interface OverlayState {
  mode: OverlayMode
  coaching: CoachingPayload | null
  focusHighlight: FocusHighlightPayload | null
  goals: GoalProgressItem[]
}
```

- [ ] **Step 6.2: Implement `useOverlayEvents` hook**

Create `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts`:

```ts
import { useEffect, useReducer } from 'react'
import type {
  CoachingPayload,
  FocusHighlightPayload,
  GoalProgressItem,
  OverlayMode,
  OverlayState,
} from '../types'

type OverlayAction =
  | { type: 'show-coaching'; payload: CoachingPayload }
  | { type: 'upgrade-message'; payload: { message_id: string; personalized_text: string } }
  | { type: 'dismiss' }
  | { type: 'update-focus'; payload: FocusHighlightPayload }
  | { type: 'update-goals'; payload: GoalProgressItem[] }
  | { type: 'set-mode'; payload: OverlayMode }

const initialState: OverlayState = {
  mode: 'minimal',
  coaching: null,
  focusHighlight: null,
  goals: [],
}

function reducer(state: OverlayState, action: OverlayAction): OverlayState {
  switch (action.type) {
    case 'show-coaching':
      return { ...state, coaching: action.payload }
    case 'upgrade-message':
      if (state.coaching?.message_id === action.payload.message_id) {
        return {
          ...state,
          coaching: { ...state.coaching, text: action.payload.personalized_text },
        }
      }
      return state
    case 'dismiss':
      return { ...state, coaching: null }
    case 'update-focus':
      return { ...state, focusHighlight: action.payload }
    case 'update-goals':
      return { ...state, goals: action.payload }
    case 'set-mode':
      return { ...state, mode: action.payload }
    default:
      return state
  }
}

export function useOverlayEvents() {
  const [state, dispatch] = useReducer(reducer, initialState)

  useEffect(() => {
    let unlisten: Array<() => void> = []

    async function setup() {
      const { listen } = await import('@tauri-apps/api/event')

      const u1 = await listen<CoachingPayload>('overlay:show-coaching', (e) => {
        dispatch({ type: 'show-coaching', payload: e.payload })
      })
      const u2 = await listen<{ message_id: string; personalized_text: string }>(
        'overlay:upgrade-message',
        (e) => {
          dispatch({ type: 'upgrade-message', payload: e.payload })
        },
      )
      const u3 = await listen('overlay:dismiss', () => {
        dispatch({ type: 'dismiss' })
      })
      const u4 = await listen<FocusHighlightPayload>('overlay:update-focus', (e) => {
        dispatch({ type: 'update-focus', payload: e.payload })
      })
      const u5 = await listen<{ goals: GoalProgressItem[] }>('overlay:update-goals', (e) => {
        dispatch({ type: 'update-goals', payload: e.payload.goals })
      })
      const u6 = await listen<{ mode: OverlayMode }>('overlay:set-mode', (e) => {
        dispatch({ type: 'set-mode', payload: e.payload.mode })
      })

      unlisten = [u1, u2, u3, u4, u5, u6]
    }

    setup()
    return () => {
      for (const fn of unlisten) fn()
    }
  }, [])

  return { state, dispatch }
}
```

- [ ] **Step 6.3: Implement `useAutoDissmiss` hook**

Create `crates/oneshim-web/frontend/src/overlay/hooks/useAutoDissmiss.ts`:

```ts
import { useEffect, useRef } from 'react'

/**
 * Auto-dismiss timer. Calls `onDismiss` after `seconds` unless reset or cancelled.
 * Returns a `reset()` function to restart the timer (e.g., when LLM upgrade arrives).
 */
export function useAutoDismiss(
  active: boolean,
  seconds: number,
  onDismiss: () => void,
): { reset: () => void } {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const clear = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current)
      timerRef.current = null
    }
  }

  const start = () => {
    clear()
    timerRef.current = setTimeout(onDismiss, seconds * 1000)
  }

  useEffect(() => {
    if (active) {
      start()
    } else {
      clear()
    }
    return clear
  }, [active, seconds])

  return {
    reset: () => {
      if (active) start()
    },
  }
}
```

---

## Task 7: Overlay React components

**Why:** The visual overlay consists of four main components: the coaching popup bubble, focus area highlight, goal progress bar, and attention heatmap ghost. Each component consumes overlay state from the `useOverlayEvents` hook and calls Tauri IPC for interactions.

**Files:**
- Create: `crates/oneshim-web/frontend/src/overlay/App.tsx`
- Create: `crates/oneshim-web/frontend/src/overlay/components/CoachingPopup.tsx`
- Create: `crates/oneshim-web/frontend/src/overlay/components/FocusHighlight.tsx`
- Create: `crates/oneshim-web/frontend/src/overlay/components/GoalProgressBar.tsx`
- Create: `crates/oneshim-web/frontend/src/overlay/components/HeatmapGhost.tsx`

- [ ] **Step 7.1: Create `OverlayApp` root component**

Create `crates/oneshim-web/frontend/src/overlay/App.tsx`:

```tsx
import { useOverlayEvents } from './hooks/useOverlayEvents'
import CoachingPopup from './components/CoachingPopup'
import FocusHighlight from './components/FocusHighlight'
import GoalProgressBar from './components/GoalProgressBar'
import HeatmapGhost from './components/HeatmapGhost'

export default function OverlayApp() {
  const { state } = useOverlayEvents()
  const isRich = state.mode === 'rich' || state.mode === 'adaptive'

  return (
    <div className="relative h-screen w-screen overflow-hidden">
      {/* Focus area highlight (always shown when available) */}
      {state.focusHighlight && (
        <FocusHighlight highlight={state.focusHighlight} />
      )}

      {/* Coaching popup (shown when a message is active) */}
      {state.coaching && (
        <CoachingPopup
          message={state.coaching}
          autoDismissSecs={state.coaching.auto_dismiss_secs}
        />
      )}

      {/* Rich mode: goal progress bar at bottom */}
      {isRich && state.goals.length > 0 && (
        <GoalProgressBar goals={state.goals} />
      )}

      {/* Rich mode: attention heatmap ghost */}
      {isRich && <HeatmapGhost />}
    </div>
  )
}
```

- [ ] **Step 7.2: Create `CoachingPopup` component**

Create `crates/oneshim-web/frontend/src/overlay/components/CoachingPopup.tsx`:

Key behavior:
- Positioned in the top-right corner (configurable via a `position` prop in future)
- Shows message text, OK button, Later button, subtle thumbs-up/down icons
- OK dismisses immediately, Later snoozes for 15 minutes
- Thumbs icons are low opacity (0.3), full opacity (1.0) on hover
- When LLM upgrade arrives (text changes), smooth CSS transition
- Auto-dismisses after `autoDismissSecs` (default 15) via `useAutoDismiss` hook
- On mouse enter: calls `set_overlay_cursor_passthrough(false)` IPC
- On mouse leave: calls `set_overlay_cursor_passthrough(true)` IPC

```tsx
import { useCallback, useEffect, useRef, useState } from 'react'
import { useAutoDismiss } from '../hooks/useAutoDissmiss'
import type { CoachingPayload, DismissAction } from '../types'

async function tauriInvoke(cmd: string, args?: Record<string, unknown>): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke(cmd, args)
}

interface CoachingPopupProps {
  message: CoachingPayload
  autoDismissSecs: number
}

export default function CoachingPopup({ message, autoDismissSecs }: CoachingPopupProps) {
  const [text, setText] = useState(message.text)
  const [transitioning, setTransitioning] = useState(false)
  const prevTextRef = useRef(message.text)

  // Detect text upgrade (LLM personalization)
  useEffect(() => {
    if (message.text !== prevTextRef.current) {
      setTransitioning(true)
      const timer = setTimeout(() => {
        setText(message.text)
        setTransitioning(false)
      }, 300) // fade duration
      prevTextRef.current = message.text
      return () => clearTimeout(timer)
    }
    setText(message.text)
  }, [message.text])

  const dismiss = useCallback(async (action: DismissAction) => {
    await tauriInvoke('dismiss_coaching_message', {
      messageId: message.message_id,
      action,
    })
  }, [message.message_id])

  const { reset } = useAutoDismiss(true, autoDismissSecs, () => dismiss('timeout'))

  // Reset auto-dismiss when LLM upgrade arrives
  useEffect(() => {
    if (message.text !== prevTextRef.current) {
      reset()
    }
  }, [message.text, reset])

  const feedback = useCallback(async (positive: boolean) => {
    await tauriInvoke('submit_coaching_feedback', {
      messageId: message.message_id,
      positive,
    })
  }, [message.message_id])

  // Cursor passthrough management
  const onMouseEnter = () => tauriInvoke('set_overlay_cursor_passthrough', { passthrough: false })
  const onMouseLeave = () => tauriInvoke('set_overlay_cursor_passthrough', { passthrough: true })

  return (
    <div
      className="overlay-interactive fixed right-4 top-4 z-50"
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      <div className="w-80 rounded-xl border border-white/10 bg-gray-900/90 p-4 shadow-2xl backdrop-blur-md">
        {/* Message text with transition */}
        <p
          className={`mb-3 text-sm leading-relaxed text-gray-100 transition-opacity duration-300 ${
            transitioning ? 'opacity-0' : 'opacity-100'
          }`}
        >
          {text}
        </p>

        {/* Actions row */}
        <div className="flex items-center justify-between">
          <div className="flex gap-2">
            <button
              onClick={() => dismiss('ok')}
              className="rounded-md bg-white/10 px-3 py-1 text-xs font-medium text-gray-200 transition-colors hover:bg-white/20"
            >
              OK
            </button>
            <button
              onClick={() => dismiss('later')}
              className="rounded-md bg-white/5 px-3 py-1 text-xs font-medium text-gray-400 transition-colors hover:bg-white/10"
            >
              Later
            </button>
          </div>

          {/* Thumbs feedback — subtle by default */}
          <div className="flex gap-1">
            <button
              onClick={() => feedback(true)}
              className="rounded p-1 text-gray-500 opacity-30 transition-opacity hover:text-green-400 hover:opacity-100"
              aria-label="Helpful"
            >
              <ThumbsUpIcon />
            </button>
            <button
              onClick={() => feedback(false)}
              className="rounded p-1 text-gray-500 opacity-30 transition-opacity hover:text-red-400 hover:opacity-100"
              aria-label="Not helpful"
            >
              <ThumbsDownIcon />
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

function ThumbsUpIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M7 10v12" /><path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2h0a3.13 3.13 0 0 1 3 3.88Z" />
    </svg>
  )
}

function ThumbsDownIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M17 14V2" /><path d="M9 18.12 10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.76a2 2 0 0 0-1.79 1.11L12 22h0a3.13 3.13 0 0 1-3-3.88Z" />
    </svg>
  )
}
```

- [ ] **Step 7.3: Create `FocusHighlight` component**

Create `crates/oneshim-web/frontend/src/overlay/components/FocusHighlight.tsx`:

```tsx
import type { FocusHighlightPayload } from '../types'

interface FocusHighlightProps {
  highlight: FocusHighlightPayload
}

export default function FocusHighlight({ highlight }: FocusHighlightProps) {
  const { x, y, width, height, border_color, opacity } = highlight

  return (
    <div
      className="pointer-events-none fixed transition-all duration-200 ease-out"
      style={{
        left: x,
        top: y,
        width,
        height,
        border: `2px solid ${border_color}`,
        borderRadius: '4px',
        opacity,
        boxShadow: `0 0 12px ${border_color}40`,
      }}
    />
  )
}
```

- [ ] **Step 7.4: Create `GoalProgressBar` component**

Create `crates/oneshim-web/frontend/src/overlay/components/GoalProgressBar.tsx`:

```tsx
import type { GoalProgressItem } from '../types'

interface GoalProgressBarProps {
  goals: GoalProgressItem[]
}

export default function GoalProgressBar({ goals }: GoalProgressBarProps) {
  return (
    <div className="overlay-interactive fixed inset-x-0 bottom-0 z-40">
      <div className="mx-auto max-w-3xl rounded-t-lg border border-b-0 border-white/10 bg-gray-900/80 px-4 py-2 backdrop-blur-md"
        onMouseEnter={async () => {
          const { invoke } = await import('@tauri-apps/api/core')
          invoke('set_overlay_cursor_passthrough', { passthrough: false })
        }}
        onMouseLeave={async () => {
          const { invoke } = await import('@tauri-apps/api/core')
          invoke('set_overlay_cursor_passthrough', { passthrough: true })
        }}
      >
        <div className="flex flex-wrap gap-3">
          {goals.map((goal) => (
            <div key={goal.regime_label} className="flex min-w-[180px] flex-1 items-center gap-2">
              <span className="w-20 truncate text-xs text-gray-300">{goal.regime_label}</span>
              <div className="relative h-2 flex-1 overflow-hidden rounded-full bg-white/10">
                <div
                  className="absolute inset-y-0 left-0 rounded-full transition-all duration-500"
                  style={{
                    width: `${Math.min(goal.percentage, 100)}%`,
                    backgroundColor: goal.display_color,
                  }}
                />
              </div>
              <span className="w-16 text-right text-xs text-gray-400">
                {goal.current_minutes}/{goal.target_minutes}m
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 7.5: Create `HeatmapGhost` component (placeholder)**

Create `crates/oneshim-web/frontend/src/overlay/components/HeatmapGhost.tsx`:

The attention heatmap ghost is an optional Rich-mode feature. For Phase 2, implement a minimal placeholder that can be filled in later.

```tsx
/**
 * HeatmapGhost — semi-transparent attention heatmap overlay.
 *
 * Phase 2 placeholder: renders nothing. Full implementation will consume
 * aggregated attention data from the monitor loop and render colored
 * regions over the screen.
 */
export default function HeatmapGhost() {
  // Future: receive heatmap data via Tauri events
  // and render semi-transparent colored regions
  return null
}
```

---

## Task 8: Coaching history page

**Why:** Users need to see their coaching event history, effectiveness scores, and how their behavior changed over time. This is a new page in the main dashboard.

**Files:**
- Create: `crates/oneshim-web/frontend/src/pages/Coaching.tsx`
- Create: `crates/oneshim-web/frontend/src/hooks/useCoaching.ts`
- Create: `crates/oneshim-web/frontend/src/api/coaching.ts`
- Modify: `crates/oneshim-web/frontend/src/App.tsx`
- Modify: `crates/oneshim-web/frontend/src/components/shell/TreeView.tsx`

- [ ] **Step 8.1: Create coaching API client**

Create `crates/oneshim-web/frontend/src/api/coaching.ts`:

```ts
async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

export interface CoachingEvent {
  event_id: string
  trigger_type: string
  profile_name: string
  regime_id: string | null
  message_template: string
  personalized_message: string | null
  shown_at: string
  dismissed_at: string | null
  dismiss_action: string | null
  feedback_type: string | null
  feedback_score: number | null
}

export interface GoalProgress {
  regime_label: string
  current_minutes: number
  target_minutes: number
  percentage: number
  display_color: string
}

export async function fetchCoachingHistory(
  limit = 50,
  offset = 0,
): Promise<CoachingEvent[]> {
  return tauriInvoke<CoachingEvent[]>('get_coaching_history', { limit, offset })
}

export async function fetchGoalProgress(): Promise<GoalProgress[]> {
  return tauriInvoke<GoalProgress[]>('get_goal_progress')
}

export async function updateRegimeGoals(
  goals: Record<string, number>,
): Promise<void> {
  return tauriInvoke<void>('update_regime_goals', { goals })
}
```

- [ ] **Step 8.2: Create coaching React Query hooks**

Create `crates/oneshim-web/frontend/src/hooks/useCoaching.ts`:

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { fetchCoachingHistory, fetchGoalProgress, updateRegimeGoals } from '../api/coaching'
import { addToast } from './useToast'

export function useCoachingHistory(limit = 50, offset = 0) {
  return useQuery({
    queryKey: ['coaching-history', limit, offset],
    queryFn: () => fetchCoachingHistory(limit, offset),
    staleTime: 30_000,
  })
}

export function useGoalProgress() {
  return useQuery({
    queryKey: ['goal-progress'],
    queryFn: fetchGoalProgress,
    refetchInterval: 30_000, // refresh every 30s
  })
}

export function useUpdateGoals() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (goals: Record<string, number>) => updateRegimeGoals(goals),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['goal-progress'] })
      addToast('success', 'Goals updated')
    },
    onError: (err: Error) => {
      addToast('error', err.message)
    },
  })
}
```

- [ ] **Step 8.3: Create `Coaching` page**

Create `crates/oneshim-web/frontend/src/pages/Coaching.tsx`:

Layout follows the `DashboardDay.tsx` pattern with:
- Page title "Coaching History"
- Goal progress summary cards at the top (from `useGoalProgress`)
- Coaching event timeline list (from `useCoachingHistory`)
- Each event card shows: profile badge, trigger type, message text, timestamp, feedback indicator
- Effectiveness score summary section (percentage breakdown per profile)
- Loading/error states using `Skeleton` and `Card` from UI components

```tsx
import { useQuery } from '@tanstack/react-query'
import { MessageCircle, Target, TrendingUp } from 'lucide-react'
import { useCoachingHistory, useGoalProgress } from '../hooks/useCoaching'
import { Card, CardContent, CardHeader, CardTitle, Skeleton } from '../components/ui'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

export default function Coaching() {
  const { data: history, isLoading: histLoading } = useCoachingHistory()
  const { data: goals, isLoading: goalsLoading } = useGoalProgress()

  return (
    <div className="min-h-full p-6">
      <h1 className={cn(typography.h1, colors.text.pageTitle, 'mb-6')}>Coaching History</h1>

      {/* Goal progress summary */}
      <section className="mb-6">
        <h2 className={cn(typography.h3, 'mb-3')}>Today's Goals</h2>
        {goalsLoading ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
            {[1, 2, 3, 4].map((i) => <Skeleton key={i} className="h-20 w-full" />)}
          </div>
        ) : goals && goals.length > 0 ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
            {goals.map((g) => (
              <Card key={g.regime_label} variant="default" padding="sm">
                <CardContent>
                  <div className="flex items-center gap-2">
                    <Target className="h-4 w-4" style={{ color: g.display_color }} />
                    <span className="text-sm font-medium">{g.regime_label}</span>
                  </div>
                  <div className="mt-2 flex items-baseline gap-1">
                    <span className="text-2xl font-bold">{g.percentage}%</span>
                    <span className="text-xs text-content-secondary">
                      {g.current_minutes}/{g.target_minutes}m
                    </span>
                  </div>
                  <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-surface-muted">
                    <div
                      className="h-full rounded-full transition-all duration-500"
                      style={{
                        width: `${Math.min(g.percentage, 100)}%`,
                        backgroundColor: g.display_color,
                      }}
                    />
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        ) : (
          <p className={cn('text-sm', colors.text.secondary)}>No goals configured.</p>
        )}
      </section>

      {/* Coaching event timeline */}
      <section>
        <h2 className={cn(typography.h3, 'mb-3')}>Recent Events</h2>
        {histLoading ? (
          <div className="space-y-3">
            {[1, 2, 3].map((i) => <Skeleton key={i} className="h-16 w-full" />)}
          </div>
        ) : history && history.length > 0 ? (
          <div className="space-y-3">
            {history.map((evt) => (
              <Card key={evt.event_id} variant="default" padding="sm">
                <CardContent>
                  <div className="flex items-start justify-between">
                    <div className="flex items-center gap-2">
                      <MessageCircle className="h-4 w-4 text-brand-text" />
                      <span className="rounded-md bg-surface-elevated px-2 py-0.5 text-xs font-medium">
                        {evt.profile_name}
                      </span>
                      <span className="text-xs text-content-secondary">{evt.trigger_type}</span>
                    </div>
                    <span className="text-xs text-content-secondary">
                      {new Date(evt.shown_at).toLocaleTimeString()}
                    </span>
                  </div>
                  <p className="mt-1 text-sm text-content">
                    {evt.personalized_message || evt.message_template}
                  </p>
                  {evt.feedback_type && (
                    <div className="mt-1 flex items-center gap-1">
                      <TrendingUp className="h-3 w-3" />
                      <span className="text-xs text-content-secondary">{evt.feedback_type}</span>
                    </div>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>
        ) : (
          <p className={cn('text-sm', colors.text.secondary)}>No coaching events yet.</p>
        )}
      </section>
    </div>
  )
}
```

- [ ] **Step 8.4: Add `/coaching` route and navigation**

In `crates/oneshim-web/frontend/src/App.tsx`, add:

```tsx
import Coaching from './pages/Coaching'
// ...
<Route path="/coaching" element={<Coaching />} />
```

In `crates/oneshim-web/frontend/src/components/shell/TreeView.tsx`, add a "Coaching" navigation entry after "Focus":

```tsx
{ path: '/coaching', label: 'Coaching', icon: MessageCircleIcon }
```

---

## Task 9: Regime goal settings UI

**Why:** Users need a way to configure per-regime daily minute targets. This is added as a section in the existing Settings page.

**Files:**
- Create: `crates/oneshim-web/frontend/src/pages/settingSections/CoachingGoalsTab.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Settings.tsx`

- [ ] **Step 9.1: Create `CoachingGoalsTab` component**

Create `crates/oneshim-web/frontend/src/pages/settingSections/CoachingGoalsTab.tsx`:

Features:
- List of current regime goals (regime label + target minutes) from `useGoalProgress`
- "Add Goal" form: text input for regime label, number input for target minutes
- Edit existing goal: inline edit of target minutes
- Delete goal: remove button per row
- Enable/disable coaching toggle (calls `update_setting` IPC to toggle `coaching.enabled`)
- Profile enable/disable toggles per coaching profile
- Overlay mode selector (Minimal / Rich / Adaptive)
- Tone selector (Direct / Gentle / Data-Driven)
- Quiet hours configuration (start/end time pickers)

```tsx
import { useCallback, useState } from 'react'
import { useGoalProgress, useUpdateGoals } from '../../hooks/useCoaching'
import { Button, Card, CardContent, CardHeader, CardTitle, Input } from '../../components/ui'

export default function CoachingGoalsTab() {
  const { data: goals } = useGoalProgress()
  const updateGoalsMutation = useUpdateGoals()
  const [newLabel, setNewLabel] = useState('')
  const [newMinutes, setNewMinutes] = useState(60)

  const handleAdd = useCallback(() => {
    if (!newLabel.trim()) return
    const current: Record<string, number> = {}
    for (const g of goals ?? []) {
      current[g.regime_label] = g.target_minutes
    }
    current[newLabel.trim()] = newMinutes
    updateGoalsMutation.mutate(current)
    setNewLabel('')
    setNewMinutes(60)
  }, [newLabel, newMinutes, goals, updateGoalsMutation])

  const handleDelete = useCallback((label: string) => {
    const current: Record<string, number> = {}
    for (const g of goals ?? []) {
      if (g.regime_label !== label) {
        current[g.regime_label] = g.target_minutes
      }
    }
    updateGoalsMutation.mutate(current)
  }, [goals, updateGoalsMutation])

  return (
    <div className="space-y-6">
      <Card variant="default" padding="md">
        <CardHeader>
          <CardTitle>Regime Goals</CardTitle>
        </CardHeader>
        <CardContent>
          {/* Existing goals list */}
          {goals && goals.length > 0 ? (
            <div className="mb-4 space-y-2">
              {goals.map((g) => (
                <div key={g.regime_label} className="flex items-center gap-3">
                  <span className="w-32 truncate text-sm font-medium">{g.regime_label}</span>
                  <span className="text-sm text-content-secondary">{g.target_minutes} min/day</span>
                  <Button variant="ghost" size="sm" onClick={() => handleDelete(g.regime_label)}>
                    Remove
                  </Button>
                </div>
              ))}
            </div>
          ) : (
            <p className="mb-4 text-sm text-content-secondary">
              No goals set. Add a regime goal below.
            </p>
          )}

          {/* Add new goal form */}
          <div className="flex items-end gap-2">
            <div>
              <label className="mb-1 block text-xs text-content-secondary">Regime Label</label>
              <Input
                value={newLabel}
                onChange={(e) => setNewLabel(e.target.value)}
                placeholder="e.g. Deep Coding"
                className="w-40"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs text-content-secondary">Target (min)</label>
              <Input
                type="number"
                value={newMinutes}
                onChange={(e) => setNewMinutes(Number(e.target.value))}
                min={1}
                max={1440}
                className="w-24"
              />
            </div>
            <Button variant="primary" size="sm" onClick={handleAdd}>
              Add Goal
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
```

- [ ] **Step 9.2: Register tab in Settings page**

In `crates/oneshim-web/frontend/src/pages/Settings.tsx`, add the coaching goals tab to the settings tabs array.

Follow the existing pattern used by other settings sections (e.g., `AiAutomationTab`, `DataStorageTab`):

```tsx
import CoachingGoalsTab from './settingSections/CoachingGoalsTab'

// In the tabs configuration array:
{ id: 'coaching', label: 'Coaching Goals', component: CoachingGoalsTab }
```

---

## Task 10: REST API endpoints for coaching data

**Why:** The web dashboard (when running in standalone mode or via Axum) also needs REST endpoints for coaching data, not just Tauri IPC. This maintains the dual-access pattern used by all other dashboard features.

**Files:**
- Create: `crates/oneshim-web/src/handlers/coaching.rs`
- Modify: `crates/oneshim-web/src/handlers/mod.rs`
- Modify: `crates/oneshim-web/src/routes.rs`

- [ ] **Step 10.1: Create coaching REST handlers**

Create `crates/oneshim-web/src/handlers/coaching.rs` with:

```rust
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CoachingHistoryQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// GET /api/coaching/history
pub async fn get_coaching_history(
    State(state): State<AppState>,
    Query(params): Query<CoachingHistoryQuery>,
) -> Result<Json<Vec<CoachingEventResponse>>, ApiError> {
    let storage = state.storage_service();
    let events = storage
        .query_coaching_events(params.limit.unwrap_or(50), params.offset.unwrap_or(0))
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(events.into_iter().map(CoachingEventResponse::from).collect()))
}

/// GET /api/coaching/goals
pub async fn get_goals(
    State(state): State<AppState>,
) -> Result<Json<Vec<GoalProgressResponse>>, ApiError> {
    // Delegate to coaching engine for live progress
    // Fallback to storage for persisted goals
    todo!("Wire to CoachingEngine or storage adapter")
}

/// PUT /api/coaching/goals
pub async fn update_goals(
    State(state): State<AppState>,
    Json(goals): Json<UpdateGoalsRequest>,
) -> Result<Json<()>, ApiError> {
    todo!("Wire to config manager + coaching engine")
}
```

- [ ] **Step 10.2: Register routes**

In `crates/oneshim-web/src/routes.rs`, add:

```rust
.route("/api/coaching/history", get(handlers::coaching::get_coaching_history))
.route("/api/coaching/goals", get(handlers::coaching::get_goals).put(handlers::coaching::update_goals))
```

In `crates/oneshim-web/src/handlers/mod.rs`, add:

```rust
pub mod coaching;
```

```
cargo check -p oneshim-web
```

---

## Task 11: i18n keys for coaching

**Why:** The dashboard follows an i18n pattern with en/ko translation keys. Coaching pages need their own key set.

**Files:**
- Modify: `crates/oneshim-web/frontend/src/i18n/index.ts` (or the appropriate translation resource files)

- [ ] **Step 11.1: Add coaching i18n keys**

Add to the English translation:

```json
{
  "coaching": {
    "title": "Coaching History",
    "goalsTitle": "Today's Goals",
    "noGoals": "No goals configured.",
    "recentEvents": "Recent Events",
    "noEvents": "No coaching events yet.",
    "goalAdded": "Goal added",
    "goalRemoved": "Goal removed",
    "goalsUpdated": "Goals updated",
    "settingsTitle": "Coaching Goals",
    "regimeLabel": "Regime Label",
    "targetMinutes": "Target (min)",
    "addGoal": "Add Goal",
    "remove": "Remove",
    "perDay": "min/day",
    "profiles": {
      "FocusGuard": "Focus Guard",
      "TimeAware": "Time Aware",
      "DeepWorkCoach": "Deep Work Coach",
      "ContextRestore": "Context Restore",
      "GoalTracker": "Goal Tracker"
    }
  }
}
```

Add corresponding Korean translations:

```json
{
  "coaching": {
    "title": "코칭 히스토리",
    "goalsTitle": "오늘의 목표",
    "noGoals": "설정된 목표가 없습니다.",
    "recentEvents": "최근 이벤트",
    "noEvents": "아직 코칭 이벤트가 없습니다.",
    "goalAdded": "목표 추가됨",
    "goalRemoved": "목표 삭제됨",
    "goalsUpdated": "목표 업데이트됨",
    "settingsTitle": "코칭 목표",
    "regimeLabel": "레짐 라벨",
    "targetMinutes": "목표 (분)",
    "addGoal": "목표 추가",
    "remove": "삭제",
    "perDay": "분/일",
    "profiles": {
      "FocusGuard": "집중 가드",
      "TimeAware": "시간 인식",
      "DeepWorkCoach": "딥워크 코치",
      "ContextRestore": "컨텍스트 복원",
      "GoalTracker": "목표 추적"
    }
  }
}
```

---

## Task 12: Hotkey integration for overlay mode toggle

**Why:** The spec defines Cmd+Shift+O (macOS) / Ctrl+Shift+O (Windows/Linux) as the overlay mode toggle hotkey. Tauri v2 provides global shortcut registration.

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `tauri-plugin-global-shortcut` dependency)
- Modify: `src-tauri/src/main.rs` (or `setup.rs` — register plugin in Tauri builder)
- Modify: `src-tauri/capabilities/overlay.json` (add `global-shortcut:allow-register` permission)

- [ ] **Step 12.1: Add `tauri-plugin-global-shortcut` dependency**

Tauri v2 global shortcuts require the `tauri-plugin-global-shortcut` plugin. Add it to `src-tauri/Cargo.toml`:

```toml
[dependencies]
tauri-plugin-global-shortcut = "2"
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 12.2: Register the plugin in the Tauri builder**

In `src-tauri/src/main.rs` (or `setup.rs`), register the global shortcut plugin in the Tauri builder chain:

```rust
tauri::Builder::default()
    // ... existing plugins ...
    .plugin(tauri_plugin_global_shortcut::init())
    // ... rest of builder ...
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 12.3: Add capability permission for global shortcuts**

Add the `global-shortcut:allow-register` permission. Either add it to an existing capability file (e.g., `src-tauri/capabilities/default.json`) or create a new one. If adding to the overlay capability file (`src-tauri/capabilities/overlay.json`), append it to the permissions array:

```json
{
  "identifier": "overlay",
  "windows": ["magic-overlay"],
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "core:window:allow-set-ignore-cursor-events",
    "core:window:allow-show",
    "core:window:allow-hide",
    "global-shortcut:allow-register"
  ]
}
```

Alternatively, if global shortcuts should be available to the main window too, add `"global-shortcut:allow-register"` to the main window's capability file instead.

- [ ] **Step 12.4: Register global shortcut for overlay toggle**

During app setup, register the hotkey using the `tauri_plugin_global_shortcut` API:

```rust
use tauri_plugin_global_shortcut::ShortcutState;

// In setup function, after MagicOverlayHandle is created:
let overlay_for_hotkey = overlay_handle.clone();
let hotkey = "CommandOrControl+Shift+O";

app.global_shortcut().on_shortcut(hotkey, move |_app, _shortcut, event| {
    if event.state == ShortcutState::Pressed {
        let overlay = overlay_for_hotkey.clone();
        tauri::async_runtime::spawn(async move {
            overlay.toggle_mode().await;
        });
    }
});
```

Note: `CommandOrControl` maps to Cmd on macOS and Ctrl on Windows/Linux.

```
cargo check -p oneshim-tauri
```

---

## Task 13: Workspace verification and cleanup

**Why:** Ensure the entire workspace builds, all tests pass, and lint is clean.

- [ ] **Step 13.1: Run `cargo check --workspace`**

```
cargo check --workspace
```

Fix any compilation errors.

- [ ] **Step 13.2: Run all Rust tests**

```
cargo test --workspace
```

Fix any test failures.

- [ ] **Step 13.3: Run clippy**

```
cargo clippy --workspace
```

Fix any warnings (except allowed `dead_code` on future-use variants).

- [ ] **Step 13.4: Run format check**

```
cargo fmt --check
```

Fix any formatting issues.

- [ ] **Step 13.5: Build frontend**

```
cd crates/oneshim-web/frontend && pnpm build
```

Verify both `index.html` and `overlay.html` are in the `dist/` output.

- [ ] **Step 13.6: Manual testing checklist**

1. Enable coaching (`coaching.enabled: true` in config)
2. Set a regime goal (e.g., "Deep Coding": 120 min)
3. Trigger a regime change — verify MagicOverlay appears with coaching popup
4. Verify popup has OK, Later, thumbs-up/down buttons
5. Click OK — verify overlay dismisses
6. Trigger another message — click Later — verify 15-minute snooze
7. Trigger message — wait 15 seconds — verify auto-dismiss
8. Press Cmd+Shift+O — verify overlay toggles between Minimal and Rich mode
9. In Rich mode, verify bottom goal progress bar appears
10. Open dashboard `/coaching` page — verify coaching event timeline
11. Open Settings > Coaching Goals — add/remove goals
12. If LLM is available: verify template text upgrades to personalized text within 3 seconds
13. Click thumbs-down on several messages — verify reduced frequency
14. Verify click-through: click on areas outside the popup — verify clicks pass through to underlying windows

---

## Summary

| Task | Files | Tests | Description |
|------|-------|-------|-------------|
| 1 | 2 new + 2 modified | 5 | MagicOverlayHandle + capability permissions — Tauri WebView window manager |
| 2 | 2 modified | 0 | 8 Tauri IPC commands + promote coaching_engine to AppState |
| 3 | 2 modified | 4 | Storage adapter coaching query methods |
| 4 | 2 modified | 0 | Wire LLM personalization + overlay into scheduler |
| 5 | 4 new + 1 modified | 0 | Overlay React app entry + Vite multi-page build |
| 6 | 3 new | 0 | TypeScript types + event/auto-dismiss hooks |
| 7 | 5 new | 0 | Overlay React components (popup, highlight, progress, heatmap) |
| 8 | 3 new + 2 modified | 0 | Coaching history page + hooks + API client |
| 9 | 1 new + 1 modified | 0 | Regime goal settings UI |
| 10 | 1 new + 2 modified | 0 | REST API endpoints for coaching data |
| 11 | 1 modified | 0 | i18n keys (en/ko) |
| 12 | 3 modified | 0 | Global shortcut plugin + capability + hotkey registration |
| 13 | 0 | 0 | Workspace verification + manual testing |
| **Total** | **17 new + 14 modified** | **~9** | |

### Dependency Order

```
Task 1 (MagicOverlayHandle)
  |
  +---> Task 2 (IPC commands) --- depends on Task 1
  |       |
  +---> Task 3 (Storage queries)
  |       |
  +-------+---> Task 4 (LLM + scheduler wiring) --- depends on Tasks 1, 2, 3
  |
  +---> Task 5 (Overlay entry + Vite config)
  |       |
  |       +---> Task 6 (TS types + hooks) --- depends on Task 5
  |               |
  |               +---> Task 7 (Overlay React components) --- depends on Tasks 2, 6
  |
  +---> Task 8 (Coaching history page) --- depends on Tasks 2, 3
  |
  +---> Task 9 (Goal settings UI) --- depends on Task 8
  |
  +---> Task 10 (REST endpoints) --- depends on Task 3
  |
  +---> Task 11 (i18n) --- independent, can run in parallel
  |
  +---> Task 12 (Hotkey) --- depends on Task 1
  |
  +----------- all ---------> Task 13 (Verification)
```

### Parallelizable Work

The following can execute concurrently:
- **Track A (Rust backend):** Tasks 1 -> 2 -> 3 -> 4 -> 12
- **Track B (Overlay frontend):** Tasks 5 -> 6 -> 7 (can start after Task 1)
- **Track C (Dashboard pages):** Tasks 8 -> 9 (can start after Task 3)
- **Track D (REST API):** Task 10 (can start after Task 3)
- **Track E (i18n):** Task 11 (anytime)

### Phase 2 does NOT include

- Wearable integration for break suggestions
- Calendar integration for auto quiet hours
- Weekly coaching digest reports
- Custom user-defined coaching profiles
- Peer comparison / anonymized benchmarks
- Full attention heatmap implementation (placeholder only)
