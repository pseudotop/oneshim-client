# Phase 9 PR-B — Autostart IPC + Single-Instance + Linux Robustness Design Spec

**Date:** 2026-04-25
**Baseline:** main `5618558c` (post-PR #486 d13-task13 merge)
**Target release:** v0.4.40 (PR-B1) → v0.4.41 (PR-B2)
**Scope:** Phase 9 PR-B (split into PR-B1 foundation + PR-B2 Linux deep)
**Estimated effort:** ~22h (PR-B1) + ~15h (PR-B2) = **~37h total**
**Authoring source:** Brainstorming session 2026-04-25 (5 user-locked decisions U1-U5)

---

## 1. Background & Current State

### 1.1 What already exists (`src-tauri/src/autostart.rs`)

The autostart module is **already fully implemented** for all 3 platforms:

- **macOS** (`macos` mod, lines 77-164): LaunchAgent plist at `~/Library/LaunchAgents/com.oneshim.agent.plist`, uses `launchctl load/unload`. Plist generation, enable/disable/is_enabled, tests.
- **Windows** (`windows` mod, lines 166-295): HKCU `Software\Microsoft\Windows\CurrentVersion\Run` Registry entry via `windows-sys`. Safe wrapping of unsafe blocks, tests.
- **Linux** (`linux` mod, lines 297-458): systemd user service at `~/.config/systemd/user/oneshim.service` (primary) with XDG autostart `~/.config/autostart/oneshim.desktop` fallback when `systemctl` unavailable. Service file generation with `Restart=on-failure`, tests.

The module top-level comment explicitly says (`autostart.rs:4`):

> `#![allow(dead_code)] // Module ready for IPC command wiring; all public fns used once autostart UI is enabled`

This signals that the **wiring layer (IPC commands + UI + lifecycle integration) is the missing piece**, not the platform-specific implementations.

### 1.2 What is missing

| Missing piece | Impact |
|---------------|--------|
| Tauri IPC commands exposing autostart functions | Frontend cannot enable/disable autostart |
| Settings UI toggle | No user-discoverable surface |
| Onboarding prompt | Low feature discoverability |
| Single-instance enforcement | autostart + manual launch = duplicate processes |
| systemd Type=notify integration | Service marked READY before init complete |
| Snap/Flatpak/headless Linux detection | Toggle attempts on unsupported envs → broken errors |
| AppConfig persistence of user intent | OS state alone can drift from user expectation |
| i18n strings | en/ko coverage missing |
| Cross-platform test matrix | macOS/Windows/Linux smoke tests + Linux-deep integration |

### 1.3 Why split into PR-B1 + PR-B2

The work has two natural axes:
- **Cross-platform UX surface** (IPC, UI, single-instance, onboarding, config) — uniform across macOS/Windows/Linux
- **Linux-specific deep robustness** (Type=notify, env detection, capability gating) — Linux-only, requires more validation

Splitting yields:
- **Independent value**: PR-B1 alone gives macOS+Windows users complete autostart UX; Linux users get the existing systemd path (already working) without env gating
- **Risk isolation**: PR-B2 changes systemd unit semantics (`Type=simple` → `Type=notify`) which carries lifecycle risk; PR-B1 doesn't touch that
- **Reviewability**: Avoids single 35-40h PR (features2-style), each PR is bounded and reviewable

---

## 2. Goals & Non-Goals

### 2.1 Goals

1. **G1**: Users can enable/disable autostart from Settings UI on all 3 platforms
2. **G2**: First-time users discover autostart via opt-in prompt after first productive session (25 min focus block)
3. **G3**: Duplicate launches (autostart fires + manual launch) result in focus on existing window, not 2nd process
4. **G4**: Linux systemd service signals readiness only after app is fully initialized (PR-B2)
5. **G5**: Linux Snap/Flatpak/headless environments show clear UI feedback ("not supported here") instead of broken errors (PR-B2)
6. **G6**: AppConfig records user intent (enabled/dismissed) for cross-session persistence and prompt logic
7. **G7**: All changes are additive — existing users upgrading don't see behavior changes until they interact with the new UI

### 2.2 Non-Goals (explicitly out of scope)

- **NG1**: Implementing autostart for platforms beyond macOS/Windows/Linux
- **NG2**: D-Bus method exposure for external tools to control ONESHIM (only `tauri-plugin-single-instance` D-Bus name registration is in scope)
- **NG3**: CLI commands like `oneshim --quit` or `oneshim --status` that talk to running instance via IPC (deferred to future PR)
- **NG4**: MPRIS / freedesktop notifications integration
- **NG5**: Snap/Flatpak best-effort autostart (we detect-and-refuse cleanly; sandbox-aware autostart deferred)
- **NG6**: Migrating existing users automatically (no auto-enable on upgrade — requires explicit user consent via prompt or toggle)
- **NG7**: Custom IPC protocol for inter-instance communication beyond what `tauri-plugin-single-instance` provides
- **NG8**: macOS LaunchAgent KeepAlive=true behavior (current `false` retained — autostart ≠ auto-restart)

---

## 3. User-Locked Decisions (U1-U5)

These decisions were made interactively during brainstorming and are FIXED. Implementation must honor them.

| ID | Decision | Rationale |
|----|----------|-----------|
| **U1** | Scope = B (full robustness) + basic IPC additions (single-instance + systemd notify) | User explicitly wanted basic IPC features included; declined CLI/D-Bus method exposure |
| **U2** | Single-instance via `tauri-plugin-single-instance` (Tauri ecosystem plugin) | Plugin uses D-Bus on Linux (matches "기본 IPC" intent), maintained by Tauri team, ~4-5h saved vs custom impl |
| **U3** | Default = Opt-in + onboarding prompt after first productive session | Privacy-friendly + discoverability; matches macOS/Windows app store guidelines |
| **U4** | Linux env matrix = Detect + clean refusal (Snap/Flatpak/headless) | ROI on Detect-and-attempt is low; we don't publish Snap/Flatpak ourselves |
| **U5** | Delivery = 2-PR split (PR-B1 foundation + PR-B2 Linux deep) | Independent value, balanced review load, learns from features2 (65 commits stalling risk) |

---

## 4. Architecture Overview

### 4.1 Component Layout

```
                                                     ┌──────────────────────┐
                                                     │  Frontend (React)    │
                                                     │  ┌────────────────┐  │
                                                     │  │ GeneralTab     │  │
                                                     │  │ Startup toggle │  │
                                                     │  └───────┬────────┘  │
                                                     │  ┌───────▼────────┐  │
                                                     │  │ Onboarding     │  │
                                                     │  │ Prompt Modal   │  │
                                                     │  └────────────────┘  │
                                                     └──────────┬───────────┘
                                                                │ invoke()
                                                                ▼
                                              ┌──────────────────────────────┐
                                              │  src-tauri Tauri IPC         │
                                              │  commands/autostart.rs       │
                                              │  ┌────────────────────────┐  │
              ┌──────────────────────────┐    │  │ enable_autostart       │  │
              │ tauri-plugin-single-     │    │  │ disable_autostart      │  │
              │ instance                 │◀───┤  │ is_autostart_enabled   │  │
              │ - Linux: D-Bus           │    │  │ autostart_capabilities │  │ ◀ B2
              │ - Win: NamedPipe         │    │  │ mark_autostart_prompt  │  │
              │ - macOS: Unix socket     │    │  │   _state               │  │
              │ args+cwd callback        │    │  │ increment_productive   │  │
              │ → focus existing window  │    │  │   _session             │  │
              └──────────────────────────┘    │  └───────────┬────────────┘  │
                                              └──────────────┼───────────────┘
                                                             ▼
                                              ┌────────────────────────────────┐
                                              │  src-tauri/src/autostart.rs    │
                                              │  ┌──────────────────────────┐  │
                                              │  │ macos mod    (existing)  │  │
                                              │  │ windows mod  (existing)  │  │
                                              │  │ linux mod    (existing)  │  │
                                              │  │   + sd_notify   ◀ B2     │  │
                                              │  │   + env_detect  ◀ B2     │  │
                                              │  └──────────────────────────┘  │
                                              └─────────────┬──────────────────┘
                                                            ▼
                                              ┌────────────────────────────────┐
                                              │ AppConfig.autostart            │
                                              │ AutostartConfig {              │
                                              │   enabled: bool                │
                                              │   prompt_state: enum           │
                                              │   productive_session_count: u32│
                                              │ }                              │
                                              └────────────────────────────────┘
```

### 4.2 PR-B1 / PR-B2 Boundary

| Layer | PR-B1 | PR-B2 |
|-------|-------|-------|
| `src-tauri/src/commands/autostart.rs` | 5 commands (NEW file) | +1 command (`autostart_capabilities`) |
| `src-tauri/src/main.rs` | `tauri-plugin-single-instance` plugin | sd_notify init hook |
| `src-tauri/src/autostart.rs` | **untouched** | Linux mod adds `notify_ready()`, `detect_environment()` |
| `crates/oneshim-core/src/config/sections/autostart.rs` | NEW (3 fields) | unchanged |
| Frontend `GeneralTab.tsx` | Startup section + toggle | +disabled state with tooltip |
| Frontend `AutostartOnboardingPrompt.tsx` | NEW component | unchanged |
| Frontend i18n | autostart base keys | +capability tooltip keys |
| Tests | smoke tests on 3 platforms | Linux integration in CI container |
| Docs | README/PHASE-HISTORY | Korean operations guide |
| Cargo.toml | `tauri-plugin-single-instance = "2"` | `sd-notify = "0.4"` (Linux only) |

PR-B2 depends on PR-B1 (extends `commands/autostart.rs`, depends on `AutostartConfig` schema).

---

## 5. PR-B1 Components — Cross-Platform Foundation

### 5.1 Tauri IPC Commands

**File**: `src-tauri/src/commands/autostart.rs` (NEW; flat file, will be promoted to directory module per ADR-003 if it exceeds 500 lines)

```rust
//! Tauri IPC commands for autostart management.
//!
//! Source-of-truth strategy: OS state authoritative for `is_autostart_enabled`;
//! AppConfig.autostart caches user intent for UI display + onboarding logic.

use std::sync::Arc;
use tauri::AppHandle;
use oneshim_core::config::AutostartPromptState;

use crate::autostart;
use crate::runtime_state::ConfigManagerHandle;

/// Enable autostart at OS level + persist intent to AppConfig.
///
/// Two-phase commit: OS enable first, then config update. If config update fails
/// after OS enable succeeded, OS state is reverted to keep them in sync.
#[tauri::command]
pub async fn enable_autostart(app: AppHandle) -> Result<(), String> {
    autostart::enable_autostart()?;
    let config = app.state::<Arc<ConfigManagerHandle>>();
    if let Err(e) = config.update(|c| c.autostart.enabled = true).await {
        // Revert OS state to maintain consistency
        let _ = autostart::disable_autostart();
        return Err(format!("Config update failed (OS state reverted): {e}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn disable_autostart(app: AppHandle) -> Result<(), String> {
    autostart::disable_autostart()?;
    let config = app.state::<Arc<ConfigManagerHandle>>();
    if let Err(e) = config.update(|c| c.autostart.enabled = false).await {
        // Best-effort re-enable on revert; log but don't fail
        let _ = autostart::enable_autostart();
        return Err(format!("Config update failed (OS state reverted): {e}"));
    }
    Ok(())
}

/// Read autostart state from OS (source of truth).
#[tauri::command]
pub async fn is_autostart_enabled() -> Result<bool, String> {
    autostart::is_autostart_enabled()
}

/// Update onboarding prompt state.
///
/// Called by frontend after user answers the prompt (Enable/NotNow/DontAsk).
#[tauri::command]
pub async fn mark_autostart_prompt_state(
    app: AppHandle,
    new_state: AutostartPromptState,
) -> Result<(), String> {
    let config = app.state::<Arc<ConfigManagerHandle>>();
    config
        .update(|c| c.autostart.prompt_state = new_state)
        .await
        .map_err(|e| format!("Config update failed: {e}"))
}

/// Increment productive session counter.
///
/// Called by scheduler when a focus session ≥25 min completes.
/// Drives onboarding prompt eligibility.
#[tauri::command]
pub async fn increment_productive_session(app: AppHandle) -> Result<u32, String> {
    let config = app.state::<Arc<ConfigManagerHandle>>();
    let new_count = config
        .update_returning(|c| {
            c.autostart.productive_session_count = c.autostart.productive_session_count.saturating_add(1);
            c.autostart.productive_session_count
        })
        .await
        .map_err(|e| format!("Config update failed: {e}"))?;
    Ok(new_count)
}
```

**Wiring**:
- `src-tauri/src/commands/mod.rs`: add `pub mod autostart;`
- `src-tauri/src/commands/settings.rs`: add 5 command names to allowlist (existing pattern from PR-A)
- `src-tauri/src/main.rs`: register all 5 in `.invoke_handler(tauri::generate_handler![...])`

**Edge cases**:
- **OS enable succeeds, config update fails (disk full / IO error)**: revert OS to maintain bidirectional consistency. User sees clear error.
- **`autostart::enable_autostart` returns Err on Linux without systemctl AND XDG path also fails**: surface error; UI must not toggle to "on" state.
- **Concurrent enable + disable from rapid double-click**: ConfigManagerHandle.update is async-serialized internally (existing pattern). UI debounces toggle click for 500ms.

---

### 5.2 `tauri-plugin-single-instance` Integration

**Files**:
- `src-tauri/Cargo.toml`: add `tauri-plugin-single-instance = "2"`
- `src-tauri/src/main.rs`: register plugin in builder chain

```rust
// main.rs (excerpt — exact location: top of builder chain, before other plugins)
.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
    // Callback runs in 1st instance when 2nd instance launches.
    // Must be cheap + synchronous (no async, no DB calls).
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    // _args, _cwd reserved for future CLI command extension (NG3)
}))
```

**Critical considerations**:

1. **D-Bus name resolution**: Plugin derives D-Bus name from `tauri.conf.json` `identifier`. Verify current identifier is `com.oneshim.agent` (matches `autostart.rs::APP_LABEL`). If mismatched, fix `tauri.conf.json` or rely on plugin's auto-derivation.

2. **Tray-only mode interaction**: ONESHIM hides main window to system tray. When 2nd instance fires:
   - `show()` → `unminimize()` → `set_focus()` order is **mandatory**
   - Reverse order can leave window unfocused on Linux/X11
   - `unminimize()` is no-op if window isn't minimized but harmless

3. **Headless Linux fail-mode**: D-Bus session bus may be absent (e.g., SSH session, headless server). Plugin init errors in this case.
   - **PR-B1 stance**: fail-open. Wrap plugin init in `match` and log warning. App still launches without single-instance protection. This is acceptable because headless servers don't typically have a "double-launch from tray icon" scenario.
   - **PR-B2 follow-up**: capability check at startup logs which IPC features are degraded.

4. **Plugin ordering**: must be registered BEFORE other plugins that may exit early on init failure (e.g., webview registration). Place at top of builder chain.

5. **Crash recovery**: Plugin handles stale lock files / orphaned D-Bus names automatically per its docs. Verify behavior with manual SIGKILL test (see §10).

---

### 5.3 AppConfig — `AutostartConfig`

**File**: `crates/oneshim-core/src/config/sections/autostart.rs` (NEW per ADR-003 sections pattern)

```rust
//! Autostart-related configuration.
//!
//! See ADR-003 for the directory module pattern this file follows.

use serde::{Deserialize, Serialize};

/// Per-user autostart configuration.
///
/// Note: `enabled` is a CACHE of user intent. The OS state (LaunchAgent file
/// existence, Registry entry, systemd unit file) is the source of truth for
/// "is autostart actually active right now?". `enabled` exists for UI
/// instant-response (avoids round-trip to OS on every render) and for
/// onboarding logic (when did user last opt in?).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutostartConfig {
    /// User's last expressed intent (true if they enabled, false if disabled or never enabled).
    pub enabled: bool,

    /// State machine for one-time onboarding prompt.
    pub prompt_state: AutostartPromptState,

    /// Monotonic counter of completed productive sessions (≥25 min focus blocks).
    /// Drives prompt eligibility per `prompt_state` transitions.
    pub productive_session_count: u32,
}

/// State machine for the onboarding prompt.
///
/// Transitions:
/// - Pending → Dismissed   (user clicks Enable or DontAsk)
/// - Pending → Snoozed     (user clicks NotNow)
/// - Snoozed → Dismissed   (user clicks Enable or DontAsk on re-prompt)
/// - Snoozed → Snoozed     (user clicks NotNow on re-prompt; updates remind_after)
/// - Dismissed → Dismissed (terminal — no further prompts)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutostartPromptState {
    /// Never prompted. Show prompt when productive_session_count >= 1.
    #[default]
    Pending,

    /// "Not now" — re-prompt when productive_session_count >= remind_after_session_count.
    Snoozed { remind_after_session_count: u32 },

    /// "Don't ask again" or already enabled — never prompt.
    Dismissed,
}

impl Default for AutostartConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prompt_state: AutostartPromptState::Pending,
            productive_session_count: 0,
        }
    }
}
```

**Wiring**:
- `crates/oneshim-core/src/config/sections/mod.rs`: `pub mod autostart;` + `pub use autostart::*;`
- `crates/oneshim-core/src/config/mod.rs`: add field to `AppConfig`:

```rust
#[serde(default)]
pub autostart: AutostartConfig,
```

**Migration semantics**:
- Existing users (config file pre-PR-B1): `#[serde(default)]` applies `AutostartConfig::default()` automatically on first load
- Result: `enabled = false`, `prompt_state = Pending`, `productive_session_count = 0`
- Behavior: existing users see no autostart change, get prompted after their first ≥25 min session post-upgrade

**JSON serialization shape** (for existing config compat tests):
```json
{
  "autostart": {
    "enabled": false,
    "prompt_state": { "kind": "pending" },
    "productive_session_count": 0
  }
}
```
Note: `tag = "kind"` with `rename_all = "snake_case"` chosen for JSON ergonomics (vs untagged enum that breaks `Snoozed` field deserialization).

**Eligibility logic** (consumed by frontend Dashboard):
```rust
pub fn should_prompt(config: &AutostartConfig) -> bool {
    match &config.prompt_state {
        AutostartPromptState::Dismissed => false,
        AutostartPromptState::Pending => config.productive_session_count >= 1,
        AutostartPromptState::Snoozed { remind_after_session_count } => {
            config.productive_session_count >= *remind_after_session_count
        }
    }
}
```

This helper lives in `crates/oneshim-core/src/config/sections/autostart.rs` and is unit-tested.

---

### 5.4 Settings UI — GeneralTab Startup Section

**File**: `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`

**Location in tab**: New "Startup" section between "Theme" and "Language" sections (high-visibility but not at top — matches Settings convention of personalization-first ordering).

**Component shape**:
```tsx
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

function StartupSection() {
  const { t } = useTranslation()
  const [enabled, setEnabled] = useState<boolean | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Initial fetch from OS state (source of truth)
  useEffect(() => {
    invoke<boolean>('is_autostart_enabled')
      .then(setEnabled)
      .catch((e) => setError(String(e)))
  }, [])

  const handleToggle = async (next: boolean) => {
    if (loading) return
    setLoading(true)
    setError(null)
    try {
      await invoke(next ? 'enable_autostart' : 'disable_autostart')
      setEnabled(next)
    } catch (e) {
      setError(String(e))
      // Re-fetch OS state to ensure UI matches reality
      const actual = await invoke<boolean>('is_autostart_enabled').catch(() => null)
      if (actual !== null) setEnabled(actual)
    } finally {
      setLoading(false)
    }
  }

  return (
    <section>
      <h2>{t('settings.general.autostart.title')}</h2>
      <p className="description">{t('settings.general.autostart.description')}</p>
      <Toggle
        checked={enabled ?? false}
        onChange={handleToggle}
        disabled={loading || enabled === null}
        label={t('settings.general.autostart.toggle')}
      />
      {error && <ErrorBanner>{t('settings.general.autostart.error', { error })}</ErrorBanner>}
    </section>
  )
}
```

**State management notes**:
- `enabled === null` means "still loading initial state" → toggle disabled
- On error, re-fetch OS state to re-sync UI (defensive against partial-success)
- No drift indicator UI — drift handled by always trusting OS state on render
- Per `feedback_color_consistency`: toggle uses semantic primary token, not named color

---

### 5.5 Onboarding Prompt Component

**Files**:
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx` (NEW)
- `crates/oneshim-web/frontend/src/pages/Dashboard.tsx`: render conditionally
- `src-tauri/src/scheduler/loops/focus.rs` or similar: emit `productive-session-completed` Tauri event when ≥25 min focus block ends → frontend listener calls `increment_productive_session` IPC

**Productive session definition**:
- **Source**: existing `focus_metrics` table records focus blocks. A "productive session" = single focus block of ≥25 minutes (1 pomodoro equivalent)
- **Boundary**: session ends when (a) user becomes idle ≥5 min OR (b) app changes to non-productive category
- **Counter**: `productive_session_count` incremented exactly once per qualifying session — by scheduler emitting an event, frontend invoking IPC. Server-side counter chosen (not derived from query) to keep prompt logic deterministic and avoid retroactive counting
- **Edge case**: if session crosses prompt threshold at 25:00 and user hits 25:01, counter increments once. Subsequent ms-level events are debounced server-side.

**Trigger eligibility** (Dashboard render):
```tsx
const shouldShowPrompt = (
  promptState.kind === 'pending' && productiveCount >= 1
) || (
  promptState.kind === 'snoozed' && productiveCount >= promptState.remind_after_session_count
)
```
Polled from AppConfig on Dashboard mount + on `productive-session-completed` event.

**UI** (modal):
- Title: `t('onboarding.autostart.title')` ("Start ONESHIM automatically?")
- Body: `t('onboarding.autostart.body')`
- 3 buttons: Enable / Not now / Don't ask again

**Button handlers**:
```tsx
async function handleEnable() {
  await invoke('enable_autostart')
  await invoke('mark_autostart_prompt_state', { newState: { kind: 'dismissed' } })
  closeModal()
}

async function handleNotNow() {
  await invoke('mark_autostart_prompt_state', {
    newState: { kind: 'snoozed', remind_after_session_count: productiveCount + 5 }
  })
  closeModal()
}

async function handleDismiss() {
  await invoke('mark_autostart_prompt_state', { newState: { kind: 'dismissed' } })
  closeModal()
}
```

**UX details**:
- Modal appears 500ms after Dashboard mount (avoids feeling jarring on fresh load)
- Clicking outside = same as "Not now" (snoozed)
- Pressing Escape = same as "Not now"
- Snooze interval: +5 sessions (~5 productive work blocks ≈ 1 work day for active users)
- Single instance: once dismissed in this session, no re-show even if state somehow becomes eligible again
- Focus trap: standard modal a11y (per existing modal components in codebase)

---

### 5.6 i18n Strings

**Files**:
- `crates/oneshim-web/frontend/src/i18n/en.json`
- `crates/oneshim-web/frontend/src/i18n/ko.json`

**Keys** (added at appropriate nested locations):
```json
{
  "settings": {
    "general": {
      "autostart": {
        "title": "Startup",
        "description": "Automatically start ONESHIM when you sign in to your computer.",
        "toggle": "Start ONESHIM at login",
        "error": "Failed to update autostart: {{error}}"
      }
    }
  },
  "onboarding": {
    "autostart": {
      "title": "Start ONESHIM automatically?",
      "body": "ONESHIM works best when running in the background. Want it to launch when you sign in to your computer?",
      "enable_button": "Enable",
      "not_now_button": "Not now",
      "dismiss_button": "Don't ask again"
    }
  }
}
```

**Korean translations**:
```json
{
  "settings": {
    "general": {
      "autostart": {
        "title": "시작 프로그램",
        "description": "컴퓨터에 로그인할 때 ONESHIM을 자동으로 시작합니다.",
        "toggle": "로그인 시 ONESHIM 시작",
        "error": "자동 시작 설정 실패: {{error}}"
      }
    }
  },
  "onboarding": {
    "autostart": {
      "title": "ONESHIM을 자동으로 시작할까요?",
      "body": "ONESHIM은 백그라운드에서 실행될 때 가장 잘 작동합니다. 컴퓨터에 로그인할 때 자동으로 시작하시겠습니까?",
      "enable_button": "켜기",
      "not_now_button": "나중에",
      "dismiss_button": "다시 묻지 않기"
    }
  }
}
```

i18n key parity: enforced by existing CI lint that diffs en.json vs ko.json key sets.

---

## 6. PR-B2 Components — Linux Deep Robustness

### 6.1 systemd Type=notify Integration

**Goal**: Service marked READY only after app initialization completes, not at process spawn.

**Files**:
- `src-tauri/Cargo.toml`: add Linux-only dep `sd-notify = { version = "0.4", optional = true }` + feature flag `systemd-notify = ["dep:sd-notify"]`
- `src-tauri/src/autostart.rs`: change `linux::generate_service_file` `Type=simple` → `Type=notify` and add `NotifyAccess=main`
- `src-tauri/src/main.rs`: after init complete, call `sd_notify_ready()` helper
- `src-tauri/src/lifecycle/sd_notify.rs` (NEW): wrapper module

```rust
// src-tauri/src/lifecycle/sd_notify.rs
//! systemd Type=notify integration.
//!
//! No-op on non-Linux or when `sd-notify` feature is disabled.

#[cfg(all(target_os = "linux", feature = "systemd-notify"))]
pub fn notify_ready() {
    if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]) {
        tracing::debug!("sd_notify READY skipped (not run under systemd): {e}");
    }
}

#[cfg(not(all(target_os = "linux", feature = "systemd-notify")))]
pub fn notify_ready() {
    // No-op on non-Linux or when systemd-notify feature disabled
}

#[cfg(all(target_os = "linux", feature = "systemd-notify"))]
pub fn notify_stopping() {
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
}

#[cfg(not(all(target_os = "linux", feature = "systemd-notify")))]
pub fn notify_stopping() {}
```

**Service file change**:
```ini
[Unit]
Description=ONESHIM Desktop Agent
After=graphical-session.target

[Service]
Type=notify              # changed from simple
NotifyAccess=main        # NEW: only main process can send notify
ExecStart={binary_path}
Restart=on-failure
RestartSec=5
TimeoutStartSec=30       # NEW: bounded startup window
Environment=DISPLAY=:0

[Install]
WantedBy=default.target
```

**Init hook placement** (`src-tauri/src/main.rs`):
- Call `sd_notify::notify_ready()` AFTER:
  - Tauri builder finishes setup
  - All scheduler loops spawned
  - SQLite migrations applied
  - WebView main window shown (or hidden-to-tray confirmed)

**Failure modes**:
- Process not run under systemd (e.g., user double-clicks binary) → `sd_notify::notify` returns Err → logged at debug, app continues normally
- `NOTIFY_SOCKET` env var missing → same as above
- systemd timeout (>30s init) → systemd kills process → user sees "Service failed to start" in logs. Mitigation: ensure init path is fast (<5s typical)

**Backward compatibility for existing service files**:
- Users who upgraded from older ONESHIM with `Type=simple` service file already installed:
  - On first launch post-upgrade, if config.autostart.enabled is true and OS state still has old `Type=simple` file → automatic regeneration via `autostart::enable_autostart()` no-op (file content already matches new template after PR-B2)
  - **Action needed**: PR-B2 includes a one-time migration check at startup: if Linux + `service_path().exists()` + reading file shows `Type=simple` → call `linux::enable()` to overwrite + `systemctl --user daemon-reload`. Logged as info.

---

### 6.2 Environment Detection — `autostart_capabilities` IPC

**File**: `src-tauri/src/commands/autostart.rs` (extend PR-B1 file)

```rust
#[derive(serde::Serialize)]
pub struct AutostartCapabilities {
    /// Whether autostart toggle should be enabled in UI
    pub supported: bool,

    /// Reason if not supported (for UI tooltip)
    pub unsupported_reason: Option<UnsupportedReason>,

    /// Detected environment classification
    pub environment: EnvironmentKind,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum UnsupportedReason {
    SnapSandbox,                    // SNAP env var set
    FlatpakSandbox,                 // FLATPAK_ID env var set
    HeadlessSession,                // No DISPLAY/WAYLAND_DISPLAY
    SystemctlUnavailable,           // PATH lookup failed
    UnsupportedPlatform,            // Future-proof for non-mac/win/linux
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentKind {
    MacOs,
    Windows,
    LinuxSystemd,
    LinuxXdg,                       // systemctl missing → XDG fallback supported
    LinuxSnapSandbox,
    LinuxFlatpakSandbox,
    LinuxHeadless,
    Unknown,
}

#[tauri::command]
pub async fn autostart_capabilities() -> Result<AutostartCapabilities, String> {
    Ok(autostart::detect_capabilities())
}
```

**Detection logic** (`src-tauri/src/autostart.rs::linux` mod, PR-B2 additions):

```rust
#[cfg(target_os = "linux")]
pub fn detect_environment() -> (EnvironmentKind, Option<UnsupportedReason>) {
    // Sandbox detection (highest priority — overrides everything else)
    if std::env::var("SNAP").is_ok() {
        return (EnvironmentKind::LinuxSnapSandbox, Some(UnsupportedReason::SnapSandbox));
    }
    if std::env::var("FLATPAK_ID").is_ok() {
        return (EnvironmentKind::LinuxFlatpakSandbox, Some(UnsupportedReason::FlatpakSandbox));
    }

    // Headless detection
    let has_display = std::env::var("DISPLAY").is_ok()
        || std::env::var("WAYLAND_DISPLAY").is_ok();
    if !has_display {
        return (EnvironmentKind::LinuxHeadless, Some(UnsupportedReason::HeadlessSession));
    }

    // systemctl availability
    if has_systemctl() {
        (EnvironmentKind::LinuxSystemd, None)
    } else {
        (EnvironmentKind::LinuxXdg, None)  // XDG fallback supported
    }
}
```

For non-Linux:
```rust
#[cfg(target_os = "macos")]
pub fn detect_environment() -> (EnvironmentKind, Option<UnsupportedReason>) {
    (EnvironmentKind::MacOs, None)
}

#[cfg(target_os = "windows")]
pub fn detect_environment() -> (EnvironmentKind, Option<UnsupportedReason>) {
    (EnvironmentKind::Windows, None)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn detect_environment() -> (EnvironmentKind, Option<UnsupportedReason>) {
    (EnvironmentKind::Unknown, Some(UnsupportedReason::UnsupportedPlatform))
}
```

**UI gating** (PR-B2 update to `GeneralTab.tsx`):
```tsx
const [caps, setCaps] = useState<AutostartCapabilities | null>(null)
useEffect(() => {
  invoke<AutostartCapabilities>('autostart_capabilities').then(setCaps)
}, [])

<Toggle
  checked={enabled ?? false}
  onChange={handleToggle}
  disabled={loading || enabled === null || !caps?.supported}
  label={t('settings.general.autostart.toggle')}
/>
{caps && !caps.supported && (
  <Tooltip>{t(`settings.general.autostart.unsupported.${caps.unsupported_reason?.kind}`)}</Tooltip>
)}
```

**Onboarding prompt gating**: also check `caps.supported` — don't show prompt if can't honor it.

---

### 6.3 Linux Integration Tests

**CI infrastructure**: GitHub Actions matrix add `ubuntu-latest` job that runs in a systemd-enabled container.

```yaml
# .github/workflows/ci.yml — new job
linux-systemd-integration:
  runs-on: ubuntu-latest
  container:
    image: ubuntu:24.04
    options: --privileged --tmpfs /run --tmpfs /run/lock -v /sys/fs/cgroup:/sys/fs/cgroup:rw
  steps:
    - uses: actions/checkout@v4
    - run: apt-get update && apt-get install -y systemd dbus-user-session libsystemd-dev
    - run: # bootstrap systemd in container
    - run: cargo test -p oneshim-app --features systemd-notify --test linux_autostart_integration
```

**Tests covered** (`src-tauri/tests/linux_autostart_integration.rs`):
- T1: `enable_autostart` writes service file with `Type=notify`
- T2: `disable_autostart` removes service file + reloads daemon
- T3: `is_autostart_enabled` returns true after enable
- T4: `detect_environment` returns `LinuxSystemd` in clean container
- T5: `detect_environment` returns `LinuxSnapSandbox` when SNAP env var set
- T6: `detect_environment` returns `LinuxFlatpakSandbox` when FLATPAK_ID set
- T7: `detect_environment` returns `LinuxHeadless` when DISPLAY/WAYLAND_DISPLAY absent
- T8: Existing `Type=simple` service file is migrated to `Type=notify` on first run
- T9: `sd_notify::notify_ready()` returns Ok when `NOTIFY_SOCKET` is set, Err (logged) when absent

**Skipped on non-Linux**: `#[cfg(target_os = "linux")]` gate on entire integration test file.

---

### 6.4 Korean Operations Documentation

**File**: `docs/guides/autostart.ko.md` (NEW; English companion deferred unless reviewer requests)

Structure:
- 개요: autostart 기능 소개
- 플랫폼별 동작: macOS/Windows/Linux
- Linux 환경별 지원: systemd, XDG, Snap/Flatpak, headless
- 트러블슈팅:
  - "토글이 회색이에요" → 환경 매트릭스 설명
  - "활성화했는데 시작 안 돼요" → 로그 위치 + systemctl status 확인 방법
  - "Snap/Flatpak에서 안 돼요" → OS-native autostart 설정 안내
- 단일 인스턴스 동작: 설명 + 알려진 한계 (headless에서 미동작)

---

## 7. Data Flow

### 7.1 Enable autostart from Settings UI (happy path)

```
User clicks toggle ON in GeneralTab
  → Toggle.onChange(true) → handleToggle(true)
  → setLoading(true)
  → invoke('enable_autostart')
    → Tauri IPC → enable_autostart command
    → autostart::enable_autostart() (writes plist/registry/service file)
    → ConfigManagerHandle.update(|c| c.autostart.enabled = true)
      → AppConfig persisted to disk via JSON serialize
    → Returns Ok(())
  → setEnabled(true), setLoading(false)
  → Toggle visually reflects ON state
```

### 7.2 Onboarding prompt full lifecycle

```
First launch after upgrade
  → AppConfig loads with autostart = AutostartConfig::default()
  → Dashboard mounts, no prompt (productive_session_count = 0)

User completes 25-min focus block
  → scheduler emits productive-session-completed Tauri event
  → frontend listener: invoke('increment_productive_session')
  → IPC: config.autostart.productive_session_count += 1 (= 1)
  → returns 1

Dashboard re-evaluates eligibility
  → should_prompt() = (Pending && count >= 1) → true
  → 500ms delay → AutostartOnboardingPrompt modal appears

User clicks "Not now"
  → invoke('mark_autostart_prompt_state', { newState: { kind: 'snoozed', remind_after_session_count: 6 } })
  → Modal closes

User completes 5 more focus blocks (count = 2, 3, 4, 5, 6)
  → On count=6 event, Dashboard re-evaluates
  → should_prompt() = (Snoozed{6} && count >= 6) → true
  → Modal re-appears

User clicks "Don't ask again"
  → mark_autostart_prompt_state({ kind: 'dismissed' })
  → Modal closes, never appears again
```

### 7.3 Single-instance via tauri-plugin-single-instance

```
User has ONESHIM running (autostart triggered at login)

User double-clicks ONESHIM icon in dock/taskbar
  → 2nd process starts
  → tauri_plugin_single_instance::init detects existing instance
    via D-Bus (Linux) / NamedPipe (Windows) / UnixSocket (macOS)
  → 2nd process sends args+cwd to 1st process
  → 1st process callback fires:
    → window.show()
    → window.unminimize()
    → window.set_focus()
  → 2nd process exits cleanly with code 0
  → User sees: existing window comes to foreground
```

### 7.4 Linux systemd Type=notify (PR-B2)

```
systemd starts ONESHIM via user service unit (autostart enabled)
  → ExecStart={binary} runs
  → Process inherits NOTIFY_SOCKET env var from systemd
  → systemd waits for READY notification (timeout: 30s per unit file)

ONESHIM init:
  → Tauri builder.setup() runs
  → All scheduler loops spawn (16 loops)
  → SQLite migrations applied
  → Main window shown (or hidden-to-tray)
  → main.rs calls lifecycle::sd_notify::notify_ready()
    → sd_notify::notify(false, &[NotifyState::Ready])
    → systemd marks unit "active (running)"

systemd shutdown signals (SIGTERM):
  → Tauri shutdown handler fires
  → lifecycle::sd_notify::notify_stopping()
  → Cleanup runs (close DB, save state)
  → Process exits cleanly
```

---

## 8. Error Handling

### 8.1 Failure modes & mitigations

| Failure mode | Detection | User-facing behavior | Code path |
|--------------|-----------|----------------------|-----------|
| OS enable fails (e.g., LaunchAgents dir not writable) | `autostart::enable_autostart()` returns Err | Error banner in Settings: "Failed to enable: {{error}}". Toggle reverts to OFF. | `enable_autostart` IPC propagates Err |
| Config save fails after OS enable | ConfigManagerHandle.update Err | Two-phase commit reverts OS state, surfaces combined error message | `enable_autostart` IPC reverts then errors |
| systemctl missing on Linux | `has_systemctl()` returns false | (PR-B1) Falls back to XDG `.desktop` file silently. (PR-B2) `autostart_capabilities` returns `LinuxXdg` env, toggle still works | `autostart::linux::enable` |
| D-Bus unavailable for single-instance | Plugin init returns Err | Plugin disabled, app launches without single-instance protection. Warning logged. | main.rs match block on plugin init |
| sd_notify NOTIFY_SOCKET missing | `sd_notify::notify` returns Err | Logged at debug, app continues. Only matters when run under systemd. | lifecycle::sd_notify |
| User on Snap/Flatpak attempts toggle | (PR-B2) `autostart_capabilities` returns unsupported | Toggle disabled with tooltip "Use Snap/Flatpak's built-in autostart settings" | UI gating + IPC validation |
| User on headless Linux attempts toggle | (PR-B2) capabilities returns LinuxHeadless | Toggle disabled with tooltip "Autostart requires a desktop session" | UI gating |
| Existing user's service file is `Type=simple` after PR-B2 deploys | Startup migration check | Service file regenerated to `Type=notify`, daemon-reload, info logged | PR-B2 init migration |
| 2nd instance fires while 1st is hidden in tray | Plugin callback runs | Window unhidden + focused via show()→unminimize()→set_focus() | Plugin callback |
| User's config.autostart.enabled=true but OS state is false (drift) | `is_autostart_enabled` returns false on UI mount | UI shows OFF (OS truth), no automatic re-enable. User can re-toggle to fix. | UI defensive re-fetch |
| Productive session counter overflow (u32 max) | `saturating_add` | Counter caps at u32::MAX, prompt logic still works (Snoozed comparison) | `increment_productive_session` IPC |
| Concurrent enable/disable via rapid double-click | UI loading state guards | First click locks toggle, second click ignored until first completes | `if (loading) return` in handleToggle |
| sd-notify feature disabled (build without `--features systemd-notify`) | `cfg` gate | `notify_ready()` is no-op stub, no error | Build-time |

### 8.2 Logging conventions

Per CLAUDE.md `feedback_observability` and ADR-019 follow-up:

```rust
// In autostart enable/disable
tracing::warn!(
    err.code = "autostart_enable_failed",
    platform = std::env::consts::OS,
    "autostart enable failed: {e}"
);
```

Wire codes for Loki/Grafana grouping:
- `autostart_enable_failed`
- `autostart_disable_failed`
- `autostart_config_persist_failed`
- `autostart_os_state_revert`
- `single_instance_plugin_init_failed`
- `sd_notify_failed`
- `autostart_capability_check_failed`

---

## 9. Testing Strategy

### 9.1 Unit tests (per crate)

**`crates/oneshim-core/src/config/sections/autostart.rs`** (new):
- `default_config_is_pending`: `AutostartConfig::default()` matches expected JSON shape
- `prompt_state_serde_roundtrip`: each variant (Pending/Snoozed/Dismissed) serde-roundtrips correctly with `tag = "kind"` discriminator
- `should_prompt_pending_with_zero_count`: returns false
- `should_prompt_pending_with_one_count`: returns true
- `should_prompt_snoozed_below_threshold`: returns false
- `should_prompt_snoozed_at_threshold`: returns true
- `should_prompt_dismissed_always_false`: regardless of count
- `migration_from_old_config_uses_default`: deserialize old config without `autostart` field → AutostartConfig::default applied via `#[serde(default)]`

**`src-tauri/src/commands/autostart.rs`** (new — using mock ConfigManagerHandle):
- `enable_autostart_two_phase_commit_success`: OS enable + config update both succeed
- `enable_autostart_reverts_on_config_failure`: OS enable succeeds, config fails → OS reverted, error returned
- `disable_autostart_reverts_on_config_failure`: same pattern in reverse
- `is_autostart_enabled_uses_os_truth`: returns OS state, not config cache
- `mark_prompt_state_persists`: state correctly written to config
- `increment_productive_session_returns_new_count`: counter increments and returns new value
- `increment_productive_session_saturates_at_max`: u32::MAX cap

**`src-tauri/src/autostart.rs`** (existing tests preserved + extended):
- existing tests on plist/service/desktop file generation: unchanged
- (PR-B2) `detect_environment_macos`: returns `MacOs` on macOS
- (PR-B2) `detect_environment_windows`: returns `Windows` on Windows
- (PR-B2) `detect_environment_linux_systemd`: returns `LinuxSystemd` (assuming default test env)
- (PR-B2) `detect_environment_snap`: with `SNAP` env set, returns `LinuxSnapSandbox`
- (PR-B2) `detect_environment_flatpak`: with `FLATPAK_ID` env set
- (PR-B2) `detect_environment_headless`: with no `DISPLAY`/`WAYLAND_DISPLAY`
- (PR-B2) `service_file_uses_type_notify`: generated service file contains `Type=notify`
- (PR-B2) `service_file_includes_notify_access_main`: contains `NotifyAccess=main`
- (PR-B2) `service_file_includes_timeout_start_sec`: contains `TimeoutStartSec=30`

**`src-tauri/src/lifecycle/sd_notify.rs`** (new, PR-B2):
- `notify_ready_no_op_on_non_linux`: compiles + runs to completion on macOS/Windows
- `notify_ready_returns_ok_with_socket`: with `NOTIFY_SOCKET` set, succeeds
- `notify_ready_logs_when_socket_missing`: without NOTIFY_SOCKET, logged at debug

### 9.2 Frontend tests (Vitest)

**`crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx`**:
- `startup_section_renders`: section visible
- `toggle_initial_state_loads_from_ipc`: invokes `is_autostart_enabled` on mount
- `toggle_click_invokes_enable_or_disable`: correct IPC called per direction
- `toggle_error_re_fetches_os_state`: error path triggers re-fetch
- `toggle_disabled_during_loading`: prevents double-click

**`crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.test.tsx`** (new):
- `prompt_visible_when_eligible`: pending + count >= 1 shows modal
- `prompt_hidden_when_dismissed`: dismissed never shows
- `prompt_hidden_when_snoozed_below_threshold`: snoozed{N} + count<N hides
- `enable_button_invokes_correct_ipcs`: `enable_autostart` then `mark_..._state(dismissed)`
- `not_now_button_sets_snoozed`: marks state with current_count + 5
- `dismiss_button_sets_dismissed`: marks state as dismissed
- `escape_key_treated_as_not_now`: a11y compliance
- `outside_click_treated_as_not_now`: a11y compliance

### 9.3 Integration tests (PR-B1)

**`src-tauri/tests/autostart_ipc_integration.rs`** (new):
- IPC → real `autostart::*` → real ConfigManager (temp dir) round-trip on host platform
- Smoke test: `enable_autostart` → `is_autostart_enabled` returns true → `disable_autostart` → returns false
- Cleanup: test always disables on tear-down to avoid polluting host

**`src-tauri/tests/single_instance_integration.rs`** (new):
- Spawn 2nd binary as child process → expect exit code 0 within 2s
- Verify 1st instance receives callback (via Tauri event mock)
- Skip in CI on Linux without D-Bus session bus (set `OS_TEST_SKIP_SINGLE_INSTANCE=1`)

### 9.4 Integration tests (PR-B2)

**`src-tauri/tests/linux_autostart_integration.rs`** (new, `#[cfg(target_os = "linux")]`):
- T1-T9 as listed in §6.3
- Run in CI under systemd-enabled container

### 9.5 Manual smoke test matrix (per PR)

**PR-B1 sign-off requires manual verification on**:
- macOS (latest): toggle, autostart at login, single-instance focus-grab
- Windows 11: same
- Linux (Ubuntu 24.04 with systemd): same

**PR-B2 sign-off requires additional**:
- Linux Snap (toggle disabled with tooltip)
- Linux Flatpak (toggle disabled with tooltip)
- Linux headless SSH session (toggle disabled with tooltip)
- Linux Wayland session (Fedora 40 GNOME): autostart works
- Linux X11 session (Ubuntu 22.04 GNOME): autostart works

### 9.6 Pass criteria

- All unit tests GREEN
- All Vitest tests GREEN
- All integration tests GREEN (host platform only for non-Linux integration)
- `cargo check/test/clippy/fmt --workspace` GREEN (per ADR-001)
- Frontend `pnpm lint` GREEN (Biome + useExhaustiveDependencies)
- Manual smoke test matrix per phase (above)
- No new clippy warnings
- i18n key parity en.json ↔ ko.json (CI lint)

---

## 10. Delivery Plan

### 10.1 PR-B1 commit structure (~22h)

Following PR-A pattern (`feature/phase9-tracking-schedule`, atomic per-concern commits):

| # | Commit | Estimate | Files |
|---|--------|----------|-------|
| 1 | `chore(autostart): add tauri-plugin-single-instance dep` | 0.5h | `src-tauri/Cargo.toml`, `Cargo.lock` |
| 2 | `feat(autostart): AutostartConfig + AutostartPromptState in core` | 2h | `crates/oneshim-core/src/config/sections/autostart.rs`, `mod.rs`, `crates/oneshim-core/src/config/mod.rs` |
| 3 | `test(autostart): AutostartConfig serde + should_prompt unit tests` | 1.5h | `crates/oneshim-core/src/config/sections/autostart.rs` (tests submodule) |
| 4 | `feat(autostart): IPC commands (enable/disable/is_enabled/mark_prompt/increment)` | 3h | `src-tauri/src/commands/autostart.rs`, `commands/mod.rs`, `commands/settings.rs`, `main.rs` invoke_handler |
| 5 | `test(autostart): IPC command unit tests + two-phase commit coverage` | 2h | `src-tauri/src/commands/autostart.rs` tests submodule + `tests/autostart_ipc_integration.rs` |
| 6 | `feat(autostart): single-instance plugin in main builder + focus-grab callback` | 1.5h | `src-tauri/src/main.rs` |
| 7 | `test(autostart): single-instance integration smoke test` | 1.5h | `src-tauri/tests/single_instance_integration.rs` |
| 8 | `feat(autostart): GeneralTab Startup section + toggle wiring` | 2h | `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`, `i18n/en.json`, `i18n/ko.json` |
| 9 | `test(autostart): GeneralTab Vitest coverage` | 1h | `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx` |
| 10 | `feat(autostart): productive session event emission from scheduler` | 1.5h | `src-tauri/src/scheduler/loops/focus.rs` (or analysis loop), `commands/autostart.rs` (event listener) |
| 11 | `feat(autostart): AutostartOnboardingPrompt component + Dashboard integration` | 2.5h | `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx`, `pages/Dashboard.tsx`, `i18n/*.json` |
| 12 | `test(autostart): AutostartOnboardingPrompt Vitest coverage` | 1.5h | corresponding `.test.tsx` |
| 13 | `docs(autostart): STATUS.md + PHASE-HISTORY entry for PR-B1` | 0.5h | `docs/STATUS.md`, `docs/PHASE-HISTORY.md`, `crates/oneshim-web/src/handlers` if new endpoint added |
| 14 | `chore(autostart): manual smoke test matrix per platform + checklist` | 1h | manual session, no commit unless issues found |

Total: ~22h. Bundle related test commits per `feedback_lefthook_clippy_cost`.

### 10.2 PR-B2 commit structure (~15h)

After PR-B1 merges to main:

| # | Commit | Estimate |
|---|--------|----------|
| 1 | `chore(autostart): add sd-notify dep with feature flag` | 0.5h |
| 2 | `feat(autostart): lifecycle::sd_notify wrapper module` | 1.5h |
| 3 | `test(autostart): sd_notify unit coverage (Linux + non-Linux)` | 1h |
| 4 | `feat(autostart): change Linux service file to Type=notify + NotifyAccess=main + TimeoutStartSec` | 1.5h |
| 5 | `feat(autostart): one-time migration of Type=simple service files at startup` | 1.5h |
| 6 | `feat(autostart): wire sd_notify::notify_ready/stopping in main lifecycle` | 1h |
| 7 | `feat(autostart): detect_environment + EnvironmentKind/UnsupportedReason types` | 2h |
| 8 | `test(autostart): detect_environment unit coverage per env (Snap/Flatpak/headless/X11/Wayland)` | 1.5h |
| 9 | `feat(autostart): autostart_capabilities IPC + UI gating in GeneralTab` | 2h |
| 10 | `feat(autostart): onboarding prompt respects capabilities (no prompt if unsupported)` | 0.5h |
| 11 | `feat(autostart): i18n keys for capability tooltips (en + ko)` | 0.5h |
| 12 | `test(autostart): linux_autostart_integration.rs (T1-T9) + CI workflow` | 3h |
| 13 | `docs(autostart): docs/guides/autostart.ko.md operations guide` | 1h |
| 14 | `docs(autostart): STATUS.md + PHASE-HISTORY for PR-B2` | 0.5h |

Total: ~15h.

### 10.3 Branch naming

- PR-B1: `feature/phase9-autostart-foundation` (already created)
- PR-B2: `feature/phase9-autostart-linux-deep` (off main after B1 merges)

### 10.4 Release plan

- PR-B1 merges → release `0.4.40-rc.1` (alongside features2 + Phase 9 PR-A already on main)
- PR-B2 merges → release `0.4.41-rc.1`
- Stable promotion per `release.sh` + `promote-stable.sh` workflow (per memory `feedback_release_process`)

---

## 11. Migration & Backward Compatibility

### 11.1 Existing user upgrade path

| Pre-PR-B1 state | Post-PR-B1 behavior |
|-----------------|---------------------|
| autostart never enabled, config has no `autostart` field | `AutostartConfig::default()` applied. Settings shows OFF. Onboarding prompt fires after first 25-min session. |
| autostart manually enabled via OS (e.g., user added LaunchAgent themselves) | `is_autostart_enabled` returns true. Settings toggle reflects ON. `config.autostart.enabled` may be false (drift) — UI trusts OS. |

### 11.2 Service file migration (PR-B2)

- Detection: on startup, read existing service file (if exists), check if contains `Type=simple`
- Action: regenerate file with new template (`Type=notify` + `NotifyAccess=main` + `TimeoutStartSec=30`), run `systemctl --user daemon-reload`
- Idempotent: if file already matches new template, no-op
- Logged: `tracing::info!(err.code = "autostart_service_migrated", "Migrated systemd unit file from Type=simple to Type=notify")`

### 11.3 Downgrade safety

- If user installs PR-B1 then downgrades to pre-PR-B1: config file has unknown `autostart` field. Existing serde will tolerate (extra field ignored unless `deny_unknown_fields` is set on AppConfig). **Verification**: check `AppConfig` derive macros for `deny_unknown_fields` — if present, downgrade requires manual config edit. Document this in PR description.
- If user installs PR-B2 then downgrades to PR-B1: service file has `Type=notify` but PR-B1 doesn't call `sd_notify::notify_ready()`. Behavior: systemd will mark unit as failed after `TimeoutStartSec=30`. Mitigation: PR-B2 release notes recommend disable+re-enable on downgrade, or pre-B2 migration script.

### 11.4 No breaking API changes

All IPC commands are additive. No existing commands modified. `AppConfig` field added is additive (existing serializations unchanged).

---

## 12. Open Questions / Future Work

### 12.1 Open questions for ralph-loop spec review to resolve

- **Q1**: Does Tauri 2's `tauri.conf.json` `identifier` field auto-derive D-Bus name as `<identifier>` directly, or with a transformation? Verify against plugin docs to ensure it matches `com.oneshim.agent`.
- **Q2**: Is the existing `ConfigManagerHandle` API (`update`, `update_returning`) capable of returning a value from the closure? If not, refactor needed (small, in PR-B1 commit 2).
- **Q3**: Does the existing `AppConfig` derive `#[serde(deny_unknown_fields)]`? Affects downgrade story (§11.3).
- **Q4**: Does the existing scheduler emit a "focus session ended ≥25 min" event we can hook into, or do we need to add one? Affects PR-B1 commit 10 estimate.
- **Q5**: Is `sd-notify` 0.4 the latest stable / acceptable to add? Check workspace Cargo policy on adding deps.
- **Q6**: Where does the manual smoke test matrix get recorded? PR description? Separate doc? Per-PR checklist file?

### 12.2 Future work explicitly deferred

- **F1**: CLI commands (`oneshim --quit`, `oneshim --status`) using `tauri-plugin-single-instance` args+cwd callback (NG3 — future PR)
- **F2**: D-Bus method exposure for external automation tools (NG2 — future PR if requested)
- **F3**: Snap/Flatpak best-effort autostart (NG5 — only if we publish those packages ourselves)
- **F4**: macOS LaunchAgent KeepAlive=true (auto-restart on crash) — requires UX consideration: do we want crash-loop behavior for a productivity app?
- **F5**: Windows scheduled task with `LogonType=Interactive` as alternative to Run key (more robust, supports delays). Current Run key sufficient for v1.

---

## 13. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `tauri-plugin-single-instance` Linux D-Bus implementation incompatible with our hide-to-tray pattern | Low | Medium | Manual smoke test on macOS/Windows/Linux required; if fails, fall back to custom Unix socket impl (B-approach in brainstorm) |
| systemd Type=notify with 30s timeout too tight for slow init (large DB migration) | Medium | Medium | Bench init time on dev machines + slowest target hardware. Increase TimeoutStartSec if needed. |
| User's existing `Type=simple` service file migration fails partial (file overwritten but daemon-reload fails) | Low | Low | Idempotent migration: re-runs on next startup. Logged as warn. |
| Onboarding prompt feels intrusive after first 25-min session | Medium | Low | UX studies post-launch. Snooze interval (5 sessions) provides escape valve. |
| Productive session event emission has bugs (counter increments wrong) | Medium | Medium | Unit tests + manual verification during PR-B1 manual smoke. |
| Frontend i18n key drift between en.json and ko.json | Low | Low | Existing CI lint catches this |
| Plugin init failure on headless Linux blocks app launch entirely | Low | High | PR-B1: wrap plugin init in match, log warn, continue without single-instance protection (fail-open). |
| Existing `tauri.conf.json` identifier doesn't match `com.oneshim.agent` | Medium | Low | PR-B1 commit 1 includes verification + fix if mismatched |
| Two-phase commit revert in `enable_autostart` itself fails (plist removed but config update keeps trying) | Low | Low | Best-effort revert with warn log; user can manually toggle to recover |
| `cargo build` size increase from `tauri-plugin-single-instance` is significant | Low | Low | Plugin is small; verify with `cargo bloat` post-add. Acceptable tradeoff for ~5h dev savings. |

---

## 14. Appendix

### 14.1 File inventory

**PR-B1 new files**:
- `src-tauri/src/commands/autostart.rs`
- `crates/oneshim-core/src/config/sections/autostart.rs`
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx`
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.test.tsx`
- `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx` (if not already exists)
- `src-tauri/tests/autostart_ipc_integration.rs`
- `src-tauri/tests/single_instance_integration.rs`

**PR-B1 modified files**:
- `src-tauri/Cargo.toml`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/commands/settings.rs` (allowlist)
- `src-tauri/src/main.rs` (plugin + invoke_handler)
- `src-tauri/src/scheduler/loops/focus.rs` (or wherever session-end is emitted)
- `crates/oneshim-core/src/config/mod.rs`
- `crates/oneshim-core/src/config/sections/mod.rs`
- `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`
- `crates/oneshim-web/frontend/src/pages/Dashboard.tsx`
- `crates/oneshim-web/frontend/src/i18n/en.json`
- `crates/oneshim-web/frontend/src/i18n/ko.json`
- `docs/STATUS.md`
- `docs/PHASE-HISTORY.md`

**PR-B2 new files**:
- `src-tauri/src/lifecycle/sd_notify.rs`
- `src-tauri/tests/linux_autostart_integration.rs`
- `docs/guides/autostart.ko.md`
- `.github/workflows/ci.yml` (job addition)

**PR-B2 modified files**:
- `src-tauri/Cargo.toml` (sd-notify dep)
- `src-tauri/src/commands/autostart.rs` (capabilities command)
- `src-tauri/src/autostart.rs` (Linux mod: sd_notify integration, detect_environment, service file template)
- `src-tauri/src/main.rs` (sd_notify ready/stopping hooks, lifecycle module wiring)
- `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx` (capabilities-aware UI)
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx` (capabilities check)
- `crates/oneshim-web/frontend/src/i18n/en.json`, `ko.json` (capability tooltip keys)
- `docs/STATUS.md`, `docs/PHASE-HISTORY.md`

### 14.2 New dependencies

| Crate | Version | PR | Purpose |
|-------|---------|----|---------|
| `tauri-plugin-single-instance` | `2` | PR-B1 | Cross-platform single-instance enforcement with focus-grab |
| `sd-notify` | `0.4` (Linux only, optional via feature flag) | PR-B2 | systemd Type=notify protocol implementation |

### 14.3 ADRs referenced

- **ADR-001** (Rust Client Architecture Patterns): Async trait pattern, DI, error strategy
- **ADR-003** (Directory Module Pattern): `commands/autostart.rs` may be promoted to directory if grows beyond 500 LoC
- **ADR-004** (Tauri v2 Migration): Builder chain pattern for plugin registration
- **ADR-019** (Error Code Infrastructure): Wire codes for autostart failure modes (§8.2)

### 14.4 Alignment with project memory

- `feedback_3loop_quality_gate`: spec → plan → impl, each with deep review per ralph-loop
- `feedback_color_consistency`: toggle uses semantic primary token
- `feedback_release_process`: release.sh / promote-stable.sh
- `feedback_route_refactor_e2e_completeness`: PR-B1 frontend changes include corresponding test updates in same PR
- `feedback_industry_convention_check`: opt-in with onboarding prompt matches macOS/Windows app store guidelines (cited)
- `feedback_subagent_driven_catches_stale_plans`: implementation will use subagent-driven-development with 2-stage review
- `feedback_test_forwarding_and_source`: scheduler emission of productive-session event has unit test on producer side, separate from frontend integration

---

## 15. Spec Self-Review (Inline Issues Found & Fixed)

Per the brainstorming skill's spec review step:

### 15.1 Placeholders / TODOs scan
- ✅ No "TBD" or "TODO" in spec body
- ⚠ Open Questions §12.1 (Q1-Q6) intentionally listed — to be resolved during ralph-loop spec deep review

### 15.2 Internal consistency
- ✅ PR-B1/B2 boundary in §4.2 matches detail in §5/§6
- ✅ AutostartPromptState transitions in §5.3 match button handlers in §5.5
- ✅ Migration story in §11 consistent with `#[serde(default)]` in §5.3

### 15.3 Scope check
- ✅ Single implementation plan: 2 PRs are sequential, both within 14 crates this client owns
- ⚠ Some Open Questions (§12.1) require minor refactoring of existing code (e.g., `update_returning` in ConfigManagerHandle) — acceptable scope inclusion per "improvements you'd make as a good developer working in this code"

### 15.4 Ambiguity check
- ✅ "First productive session" defined as ≥25 min focus block
- ✅ "Snooze interval" defined as +5 sessions
- ✅ "Source of truth" for autostart state defined as OS state, not config
- ⚠ "Tauri identifier matches APP_LABEL" assumed but not verified — added to Q1

---

## 16. Implementation Status

- **Spec authored**: 2026-04-25 (this document)
- **Worktree created**: `/Volumes/.../client-rust/.claude/worktrees/phase9-autostart-foundation` on `feature/phase9-autostart-foundation`
- **Spec deep review**: TBD via `/ralph-loop:ralph-loop` (per user workflow)
- **Plan deep review**: TBD via `/ralph-loop:ralph-loop` after spec approved
- **Implementation deep review**: TBD via `/ralph-loop:ralph-loop` after plan approved
- **Target merge**: PR-B1 first, PR-B2 after

---

**End of spec.**
