# Phase 9 PR-B — Autostart IPC + Single-Instance + Linux Robustness Design Spec (v2)

**Date:** 2026-04-25
**Version:** v3 (rev-2 incorporates Phase 1 iter-2 review findings)
**Baseline:** main `5618558c` (post-PR #486 d13-task13 merge)
**Target release:** v0.4.40 (PR-B1) → v0.4.41 (PR-B2)
**Scope:** Phase 9 PR-B (split into PR-B1 foundation + PR-B2 Linux deep)
**Estimated effort:** ~22h (PR-B1) + ~15h (PR-B2) = **~37h total**
**Authoring source:** Brainstorming session 2026-04-25 (5 user-locked decisions U1-U5)
**Review history:**
- v1 (2026-04-25): initial spec from brainstorming
- v2 (2026-04-25): incorporates 5 Critical + 8 Important fixes from `/.claude/pr-b-review/phase1-iter1-findings.md`
- v3 (2026-04-25): incorporates 1 Critical + 3 Important + 3 Nice-to-have fixes from `/.claude/pr-b-review/phase1-iter2-findings.md`. Q7-Q9 resolved. Phase 1 deemed complete pending iter-3 verification.

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

### 1.2 Two-identifier design (clarified per v2 review)

ONESHIM uses two distinct identifiers by design:

| Identifier | Purpose | Source of truth |
|------------|---------|-----------------|
| `com.oneshim.client` | Main app bundle ID | `tauri.conf.json:4` `identifier` |
| `com.oneshim.agent` | Autostart service name (LaunchAgent plist filename, future systemd unit) | `autostart.rs:6` `APP_LABEL` constant |

This separation matches the standard pattern used by Slack (`com.slack.Slack` app + `com.slack.launcher` agent), Spotify, and other commercial apps. The split allows the autostart entry to have a distinct identity from the main app for OS-level service registration.

**Implications for PR-B**:
- `tauri-plugin-single-instance` derives D-Bus name (Linux) / lock identifier (Windows/macOS) from the **Tauri identifier** (`com.oneshim.client`). Verified against plugin docs in §12.1 Q1.
- LaunchAgent + systemd service files continue using `com.oneshim.agent`. Existing user installations are not broken.
- No identifier renaming in PR-B.

### 1.3 What is missing

| Missing piece | Impact |
|---------------|--------|
| Tauri IPC commands exposing autostart functions | Frontend cannot enable/disable autostart |
| Settings UI toggle | No user-discoverable surface |
| Onboarding prompt | Low feature discoverability |
| Single-instance enforcement | autostart + manual launch = duplicate processes |
| systemd Type=notify integration | Service marked READY before init complete |
| Snap/Flatpak/headless Linux detection | Toggle attempts on unsupported envs → broken errors |
| AppConfig persistence of onboarding state | Cannot remember "user dismissed prompt" |
| i18n strings | en/ko coverage missing |
| Cross-platform test matrix | macOS/Windows/Linux smoke tests + Linux-deep integration |

### 1.4 Why split into PR-B1 + PR-B2

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
6. **G6**: AppConfig records onboarding state (prompt_state + productive_session_count) for cross-session persistence and prompt logic
7. **G7**: All changes are additive — existing users upgrading don't see behavior changes until they interact with the new UI

### 2.2 Non-Goals (explicitly out of scope)

- **NG1**: Implementing autostart for platforms beyond macOS/Windows/Linux
- **NG2**: D-Bus method exposure for external tools to control ONESHIM
- **NG3**: CLI commands like `oneshim --quit` or `oneshim --status` that talk to running instance
- **NG4**: MPRIS / freedesktop notifications integration
- **NG5**: Snap/Flatpak best-effort autostart (we detect-and-refuse cleanly)
- **NG6**: Migrating existing users automatically (no auto-enable on upgrade)
- **NG7**: Custom IPC protocol for inter-instance communication beyond what `tauri-plugin-single-instance` provides
- **NG8**: macOS LaunchAgent KeepAlive=true behavior (current `false` retained — autostart ≠ auto-restart)
- **NG9**: Caching `enabled` state in AppConfig (v1 had this; v2 removed per review I4 — OS state is sole source of truth)

---

## 3. User-Locked Decisions (U1-U5)

These decisions were made interactively during brainstorming and are FIXED.

| ID | Decision | Rationale |
|----|----------|-----------|
| **U1** | Scope = B (full robustness) + basic IPC additions (single-instance + systemd notify) | User explicitly wanted basic IPC features included; declined CLI/D-Bus method exposure |
| **U2** | Single-instance via `tauri-plugin-single-instance` (Tauri ecosystem plugin) | Plugin uses D-Bus on Linux (matches "기본 IPC" intent), maintained by Tauri team, ~4-5h saved vs custom impl |
| **U3** | Default = Opt-in + onboarding prompt after first productive session | Privacy-friendly + discoverability; matches macOS/Windows app store guidelines (App Store Review Guideline 5.4.1 Login Items requires explicit opt-in; Microsoft Store Policy 10.2.4 prohibits silent background task enrollment) |
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
                                                     │  │ (single-fire)  │  │
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
              │ - Linux: D-Bus           │    │  │ autostart_capabilities │  │ (skeleton in B1)
              │ - Win: NamedPipe         │    │  │ mark_autostart_prompt  │  │
              │ - macOS: Unix socket     │    │  │   _state               │  │
              │ args+cwd callback        │    │  └───────────┬────────────┘  │
              │ → focus existing window  │    └──────────────┼───────────────┘
              └──────────────────────────┘                   ▼
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
                                              ┌─────────────────────────────────┐
                                              │  scheduler/loops/monitor.rs     │
                                              │   directly mutates              │
                                              │   AppConfig.autostart counter   │
                                              │   via ConfigManager.update_with │
                                              │   (no Tauri event round-trip)   │
                                              └─────────────┬───────────────────┘
                                                            ▼
                                              ┌─────────────────────────────────┐
                                              │ AppConfig.autostart             │
                                              │ AutostartConfig {               │
                                              │   prompt_state: enum            │
                                              │   productive_session_count: u32 │
                                              │   last_session_id: Option<Uuid> │ (idempotency)
                                              │ }                               │
                                              └─────────────────────────────────┘
```

### 4.2 PR-B1 / PR-B2 Boundary

| Layer | PR-B1 | PR-B2 |
|-------|-------|-------|
| `src-tauri/src/commands/autostart.rs` | 5 commands (NEW file). `autostart_capabilities` returns `{supported: true}` skeleton on all platforms | Real env detection in `autostart_capabilities` |
| `src-tauri/src/main.rs` | `tauri-plugin-single-instance` plugin | sd_notify init hook |
| `src-tauri/src/autostart.rs` | **untouched** | Linux mod adds `notify_ready()`, `detect_environment()` |
| `crates/oneshim-core/src/config/sections/autostart.rs` | NEW (3 fields, no `enabled` cache per I4) | unchanged |
| `src-tauri/src/scheduler/loops/monitor.rs` | Add productive-session detection + counter increment | unchanged |
| Frontend `GeneralTab.tsx` | Startup section + toggle + capabilities-aware disabled state | Tooltip text refinements per env |
| Frontend `AutostartOnboardingPrompt.tsx` | NEW component with single-fire coordinator | unchanged |
| Frontend i18n | autostart base keys | +capability tooltip keys |
| Tests | smoke tests on 3 platforms + Vitest + Wayland kept-hidden manual | Linux integration in CI (rootless systemd) |
| Docs | README/PHASE-HISTORY | Korean operations guide |
| Cargo.toml | `tauri-plugin-single-instance = "2"` | `sd-notify = "0.4"` (Linux only) |

PR-B2 depends on PR-B1.

---

## 5. PR-B1 Components — Cross-Platform Foundation

### 5.1 Tauri IPC Commands (revised per C1)

**File**: `src-tauri/src/commands/autostart.rs` (NEW)

Uses real `ConfigManager` API (sync `update_with` closure) and Tauri command parameter state injection.

```rust
//! Tauri IPC commands for autostart management.
//!
//! Source-of-truth: OS state is authoritative for `is_autostart_enabled`.
//! AppConfig.autostart stores ONLY onboarding state (prompt_state, counter).
//! Per Phase 1 review I4: removed AutostartConfig.enabled cache field.

use tauri::command;
use oneshim_core::config::{AutostartPromptState, AutostartConfig};
use crate::autostart;
use crate::ipc_error::IpcError;
use crate::runtime_state::ConfigRuntimeState;

/// Enable autostart at OS level.
///
/// On failure, returns Err. UI must re-fetch OS state to verify.
/// Does NOT write to AppConfig — OS state is sole source of truth.
#[command]
pub async fn enable_autostart() -> Result<(), IpcError> {
    autostart::enable_autostart()
        .map_err(|e| IpcError::new("autostart.enable_failed", format!("autostart enable failed: {e}")))
}

#[command]
pub async fn disable_autostart() -> Result<(), IpcError> {
    autostart::disable_autostart()
        .map_err(|e| IpcError::new("autostart.disable_failed", format!("autostart disable failed: {e}")))
}

/// Read autostart state from OS (source of truth).
#[command]
pub async fn is_autostart_enabled() -> Result<bool, IpcError> {
    autostart::is_autostart_enabled()
        .map_err(|e| IpcError::new("autostart.query_failed", format!("autostart query failed: {e}")))
}

/// PR-B1: skeleton — always returns supported=true for non-Linux,
/// supported=true for Linux without env detection. PR-B2: real detection.
///
/// Frontend code path is identical between B1 and B2 — UI gating logic
/// works in both cases (in B1 the gate is always pass).
#[command]
pub async fn autostart_capabilities() -> Result<AutostartCapabilities, IpcError> {
    Ok(autostart::detect_capabilities())  // PR-B1 returns {supported: true} unconditionally
                                          // PR-B2 implements real detection
}

/// Update onboarding prompt state.
///
/// Called by frontend after user answers the prompt (Enable/NotNow/DontAsk).
#[command]
pub async fn mark_autostart_prompt_state(
    new_state: AutostartPromptState,
    state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<(), IpcError> {
    state
        .config_manager()
        .update_with(|c| {
            c.autostart.prompt_state = new_state;
            Ok(())
        })
        .map(|_| ())
        .map_err(IpcError::from)
}
```

**Note: `increment_productive_session` IPC command REMOVED** (per C5 — counter increment moves to scheduler Rust-side, no frontend round-trip). Frontend instead listens for `autostart:eligible-for-prompt` event.

**Wiring**:
- `src-tauri/src/commands/mod.rs`: add `pub mod autostart;`
- `src-tauri/src/main.rs`: register all 5 in `.invoke_handler(tauri::generate_handler![...])` chain
- **No `ALLOWED_KEYS` change in `commands/settings.rs`** — autostart commands are dedicated, not routed through the generic `update_setting` JSON-patch path. Verified pattern via `commands/settings.rs:92-95`.
- **Wire code registration required** (per ADR-019 + `check-wire-error-i18n-coverage.sh` CI gate): add `autostart.enable_failed`, `autostart.disable_failed`, `autostart.query_failed` to `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` AND add corresponding entries to `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json` + `wire-errors.ko.json`. Failure to add will fail CI.

**Edge cases**:
- **OS enable fails**: error propagates, UI shows error banner, toggle reverts visually (UI re-fetches OS state via `is_autostart_enabled`)
- **No two-phase commit needed** (per C3): config no longer caches OS state, so no consistency to maintain across two writes
- **Concurrent enable + disable**: UI debounces toggle click for 500ms (existing pattern)
- **Reconciler**: see §11.4 for startup-time reconciliation (logs warn if any inconsistency observed; no auto-correct)

---

### 5.2 `tauri-plugin-single-instance` Integration (revised per C2, I7)

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

**D-Bus name**: derives from Tauri identifier `com.oneshim.client` (verified per §12.1 Q1). This is intentional and separate from autostart's `com.oneshim.agent` (see §1.2).

**Critical considerations**:

1. **Tray-only mode interaction**: ONESHIM hides main window to system tray. When 2nd instance fires:
   - `show()` → `unminimize()` → `set_focus()` order is mandatory
   - Reverse order can leave window unfocused on Linux/X11
   - `unminimize()` is no-op if window isn't minimized but harmless

2. **D-Bus absence on headless Linux** (per I7):
   - Plugin is added unconditionally — `Builder::plugin()` does NOT return Result, so no `match` wrap is possible
   - On headless Linux (no `DBUS_SESSION_BUS_ADDRESS`): plugin's IPC mechanism fails to find existing instance → 2nd instance launches as standalone (no focus-grab)
   - User experience: 2 windows open. App still launches.
   - **Mitigation**: at startup, log warn if `DBUS_SESSION_BUS_ADDRESS` env var absent: `"single-instance enforcement degraded — focus-grab may not work in headless session"`. Document as known limitation in §13.

3. **Wayland kept-hidden window** (per I1):
   - First-launch scenario: autostart fires, app starts in tray (window never shown). User clicks dock icon → 2nd instance fires → callback runs `show()` on never-mapped surface
   - Wayland compositors require xdg-toplevel mapping events that may not fire on previously-unmapped surface
   - **Mitigation**: explicit smoke test in §9.5 — verify behavior on GNOME Wayland, KDE Wayland, sway. If broken: add fallback (e.g., `window.create()` if mapping fails)

4. **Plugin ordering**: register BEFORE other plugins that may exit on init failure. Top of builder chain.

5. **Crash recovery**: Plugin handles stale lock files / orphaned D-Bus names automatically per its docs. Verify behavior with manual SIGKILL test in §9.5.

---

### 5.3 AppConfig — `AutostartConfig` (revised per I4)

**File**: `crates/oneshim-core/src/config/sections/autostart.rs` (NEW per ADR-003 sections pattern)

```rust
//! Autostart-related configuration.
//!
//! Per Phase 1 review I4: removed `enabled` cache field. OS state is sole source
//! of truth. This struct stores ONLY onboarding-related state.

use serde::{Deserialize, Serialize};

/// Per-user autostart configuration.
///
/// IMPORTANT: This struct does NOT store the autostart enabled/disabled state.
/// That state lives in OS-native locations (LaunchAgents plist, Registry,
/// systemd service file). Use `autostart::is_autostart_enabled()` to query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutostartConfig {
    /// State machine for one-time onboarding prompt.
    pub prompt_state: AutostartPromptState,

    /// Monotonic counter of completed productive sessions (≥25 min focus blocks).
    /// Incremented by scheduler in monitor.rs (NOT by frontend round-trip).
    pub productive_session_count: u32,

    /// Last observed productive session UUID — provides idempotency for
    /// counter increments. Scheduler increments only when current_session_id
    /// differs from last_session_id.
    pub last_session_id: Option<String>,
}

/// State machine for the onboarding prompt.
///
/// Transitions (per §5.5 ShowPromptCoordinator):
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
            prompt_state: AutostartPromptState::Pending,
            productive_session_count: 0,
            last_session_id: None,
        }
    }
}

/// Eligibility helper — pure function, used by scheduler to decide when to
/// emit `autostart:eligible-for-prompt` Tauri event for frontend.
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

**Wiring**:
- `crates/oneshim-core/src/config/sections/mod.rs`: `pub mod autostart;` + `pub use autostart::*;`
- `crates/oneshim-core/src/config/mod.rs`: add field to `AppConfig`:

```rust
#[serde(default)]
pub autostart: AutostartConfig,
```

**Migration semantics** (existing users upgrading):
- Pre-PR-B1 config file has no `autostart` field
- `#[serde(default)]` applies `AutostartConfig::default()` automatically on first load
- Result: `prompt_state = Pending`, `productive_session_count = 0`, `last_session_id = None`
- Behavior: existing users see no autostart change, get prompted after their first ≥25 min focus session post-upgrade

**JSON serialization shape**:
```json
{
  "autostart": {
    "prompt_state": { "kind": "pending" },
    "productive_session_count": 0,
    "last_session_id": null
  }
}
```

**`AppConfig` `deny_unknown_fields` check** (per §12.1 Q3): verified NOT set on AppConfig (line 20 of `crates/oneshim-core/src/config/mod.rs` derives only `Debug, Clone, Serialize, Deserialize`). Downgrade is safe — extra `autostart` field is silently dropped if user downgrades.

---

### 5.4 Settings UI — GeneralTab Startup Section

**File**: `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`

**Location**: New "Startup" section between "Theme" and "Language" sections.

**Component**:
```tsx
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

interface AutostartCapabilities {
  supported: boolean
  unsupported_reason?: { kind: string }
  environment: string  // discriminator string from Rust enum
}

function StartupSection() {
  const { t } = useTranslation()
  const [enabled, setEnabled] = useState<boolean | null>(null)
  const [caps, setCaps] = useState<AutostartCapabilities | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    Promise.all([
      invoke<boolean>('is_autostart_enabled'),
      invoke<AutostartCapabilities>('autostart_capabilities'),
    ])
      .then(([e, c]) => { setEnabled(e); setCaps(c) })
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
      // Re-fetch OS state to ensure UI matches reality (no two-phase commit needed)
      const actual = await invoke<boolean>('is_autostart_enabled').catch(() => null)
      if (actual !== null) setEnabled(actual)
    } finally {
      setLoading(false)
    }
  }

  const isDisabled = loading || enabled === null || (caps !== null && !caps.supported)

  return (
    <section>
      <h2>{t('settings.general.autostart.title')}</h2>
      <p className="description">{t('settings.general.autostart.description')}</p>
      <Toggle
        checked={enabled ?? false}
        onChange={handleToggle}
        disabled={isDisabled}
        label={t('settings.general.autostart.toggle')}
      />
      {caps && !caps.supported && (
        <Tooltip>
          {t('settings.general.autostart.unsupported', { context: caps.unsupported_reason?.kind ?? 'unknown' })}
        </Tooltip>
      )}
      {error && <ErrorBanner>{t('settings.general.autostart.error', { error })}</ErrorBanner>}
    </section>
  )
}
```

**i18n context pattern** (per I3): uses `t(key, { context: ... })` instead of template-literal key building. i18next resolves to `unsupported_snap_sandbox`, `unsupported_flatpak_sandbox`, etc. — keys explicitly enumerated in en.json/ko.json for parity lint coverage.

---

### 5.5 Onboarding Prompt — ShowPromptCoordinator (revised per C5, I2)

**Files**:
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx` (NEW)
- `crates/oneshim-web/frontend/src/pages/Dashboard.tsx`: render conditionally
- `src-tauri/src/scheduler/loops/monitor.rs` (or new helper): emit `autostart:eligible-for-prompt` event AFTER counter incremented in `ConfigManager`

**Productive session detection (Rust-side, per C5)**:
- Monitor loop tracks current focus block start time + cumulative focus duration
- When focus block reaches 25+ minutes: generate UUID for this session → call `ConfigManager::update_with` to:
  - Compare `current_session_id` vs `c.autostart.last_session_id`
  - If different: increment `productive_session_count`, set `last_session_id = current_session_id`
  - If same: no-op (idempotent)
- After config update: if `should_prompt(&config.autostart)` returns true AND last emit was different session → emit `app.emit("autostart:eligible-for-prompt", &payload)`

```rust
// Pseudo-code in monitor.rs
async fn handle_focus_block_completed(
    config_mgr: &ConfigManager,
    app_handle: &AppHandle,
    session_id: String,
    duration_secs: u64,
) {
    if duration_secs < 25 * 60 {
        return;
    }

    let snapshot = match config_mgr.update_with(|c| {
        // Idempotency check
        if c.autostart.last_session_id.as_deref() == Some(&session_id) {
            return Ok(()); // already counted
        }
        c.autostart.productive_session_count = c.autostart.productive_session_count.saturating_add(1);
        c.autostart.last_session_id = Some(session_id.clone());
        Ok(())
    }) {
        Ok(s) => s,
        Err(e) => {
            warn!(err.code = "autostart_counter_increment_failed", "{e}");
            return;
        }
    };

    if should_prompt(&snapshot.autostart) {
        let _ = app_handle.emit("autostart:eligible-for-prompt", ());
    }
}
```

**Race conditions handled**:
- **Scheduler restart mid-session**: in-memory session state lost → counter doesn't increment for that session. Acceptable: missed increments are harmless (worst case: prompt fires later)
- **Rapid event burst**: idempotency via `last_session_id` UUID comparison
- **Window closed**: counter increments regardless; event emit may be lost but next Dashboard mount re-evaluates eligibility from fresh config read

**Frontend ShowPromptCoordinator**:
```tsx
// Singleton state for "have we shown the prompt this session"
let hasShownThisSession = false

function AutostartOnboardingPromptHost() {
  const [shouldShow, setShouldShow] = useState(false)
  const [config, setConfig] = useState<AutostartConfig | null>(null)
  const timerRef = useRef<number | null>(null)

  // Re-evaluate eligibility from fresh config read
  const evaluate = useCallback(async () => {
    if (hasShownThisSession) return
    const cfg = await invoke<AutostartConfig>('get_app_config').then(c => c.autostart)
    setConfig(cfg)
    if (shouldShowPrompt(cfg)) {
      // 500ms delay only on first eligibility — single-fire
      if (timerRef.current === null) {
        timerRef.current = window.setTimeout(() => {
          if (!hasShownThisSession) {
            setShouldShow(true)
            hasShownThisSession = true
          }
        }, 500)
      }
    }
  }, [])

  useEffect(() => {
    evaluate()  // on mount
    const unlisten = listen('autostart:eligible-for-prompt', evaluate)
    return () => {
      unlisten.then(f => f())
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current)
        timerRef.current = null
      }
    }
  }, [evaluate])

  if (!shouldShow || !config) return null
  return <AutostartOnboardingPrompt config={config} onClose={() => setShouldShow(false)} />
}
```

**Single-fire guarantee**: `hasShownThisSession` module-level flag prevents re-show on re-mount (e.g., navigating away and back to Dashboard). Reset only on app restart.

**Dev-only quirk** (per Phase 1 iter-2 N-I2): Vite/React HMR resets module-level `let` bindings on hot reload. In `pnpm dev` mode with HMR enabled, the prompt may re-fire after code edits even after the user has acted on it. This does NOT affect production builds (no HMR). Optional hardening if it becomes annoying during dev: gate reset via `if (import.meta.env.DEV) hasShownThisSession = false`. Not worth implementing for PR-B1 — accept as known dev quirk.

**Modal UI** (unchanged from v1):
- Title: `t('onboarding.autostart.title')` — "Start ONESHIM automatically?"
- Body: `t('onboarding.autostart.body')`
- 3 buttons: Enable / Not now / Don't ask again

**Button handlers**:
```tsx
async function handleEnable() {
  await invoke('enable_autostart')
  await invoke('mark_autostart_prompt_state', { newState: { kind: 'dismissed' } })
  onClose()
}

async function handleNotNow() {
  await invoke('mark_autostart_prompt_state', {
    newState: { kind: 'snoozed', remind_after_session_count: config.productive_session_count + 5 }
  })
  onClose()
}

async function handleDismiss() {
  await invoke('mark_autostart_prompt_state', { newState: { kind: 'dismissed' } })
  onClose()
}
```

**UX**: Escape key + outside click = "Not now" (snoozed +5).

---

### 5.6 i18n Strings (revised per I3)

**Files**:
- `crates/oneshim-web/frontend/src/i18n/en.json`
- `crates/oneshim-web/frontend/src/i18n/ko.json`

**Keys**:
```json
{
  "settings": {
    "general": {
      "autostart": {
        "title": "Startup",
        "description": "Automatically start ONESHIM when you sign in to your computer.",
        "toggle": "Start ONESHIM at login",
        "error": "Failed to update autostart: {{error}}",
        "unsupported_snap_sandbox": "Use Snap's built-in autostart settings",
        "unsupported_flatpak_sandbox": "Use Flatpak's built-in autostart settings",
        "unsupported_headless_session": "Autostart requires a desktop session",
        "unsupported_systemctl_unavailable": "systemctl not available — using XDG autostart fallback",
        "unsupported_unsupported_platform": "Autostart not supported on this platform",
        "unsupported_unknown": "Autostart unavailable in this environment"
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
        "error": "자동 시작 설정 실패: {{error}}",
        "unsupported_snap_sandbox": "Snap의 내장 자동 시작 설정을 사용하세요",
        "unsupported_flatpak_sandbox": "Flatpak의 내장 자동 시작 설정을 사용하세요",
        "unsupported_headless_session": "자동 시작은 데스크톱 세션이 필요합니다",
        "unsupported_systemctl_unavailable": "systemctl 사용 불가 — XDG 자동 시작 fallback 사용 중",
        "unsupported_unsupported_platform": "이 플랫폼에서는 자동 시작을 지원하지 않습니다",
        "unsupported_unknown": "이 환경에서는 자동 시작을 사용할 수 없습니다"
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

i18n key parity: enforced by existing CI lint that diffs en.json vs ko.json key sets (verify per §12.1 Q3-supplement).

---

## 6. PR-B2 Components — Linux Deep Robustness

### 6.1 systemd Type=notify Integration (revised per C4)

**Goal**: Service marked READY only after app initialization completes.

**Files**:
- `src-tauri/Cargo.toml`: add Linux-only dep `sd-notify = { version = "0.4", optional = true }` + feature flag `systemd-notify = ["dep:sd-notify"]`
- `src-tauri/src/autostart.rs`: change `linux::generate_service_file` `Type=simple` → `Type=notify` and add `NotifyAccess=main`, `TimeoutStartSec=30`
- `src-tauri/src/main.rs`: after init complete, call `sd_notify_ready()` helper
- `src-tauri/src/lifecycle/sd_notify.rs` (NEW): wrapper module

**Service file template change**:
```ini
[Unit]
Description=ONESHIM Desktop Agent
After=graphical-session.target

[Service]
Type=notify              # changed from simple in PR-B2
NotifyAccess=main        # NEW: only main process can send notify
ExecStart={binary_path}
Restart=on-failure
RestartSec=5
TimeoutStartSec=30       # NEW: bounded startup window
Environment=DISPLAY=:0

[Install]
WantedBy=default.target
```

**Migration policy** (revised per C4):

The dangerous v1 approach (overwrite + immediate `daemon-reload`) is **rejected** because:
1. Overwriting + reloading while service is currently running causes systemd to expect READY notification on already-running unit → `TimeoutStartSec=30` fail → restart loop
2. Blind overwrite destroys user customizations (e.g., custom `Environment=`)

**Revised migration** (deferred + hash-checked):
1. **At startup, scan**: read existing `~/.config/systemd/user/oneshim.service` if exists
2. **Hash check**: compute SHA-256 of current file
3. **Match against known prior-version hashes** (we maintain a list in `src-tauri/src/lifecycle/migration_hashes.rs`)
4. **Decision**:
   - File matches known PR-B1-era template (Type=simple): safe to overwrite. Write new file, log info, **DO NOT** call `daemon-reload`. Document in user notification: "Restart ONESHIM next session for systemd integration to take effect."
   - File doesn't match any known hash: user customized. Log warn: `"Skipping autostart unit migration — file appears customized. Manual update required (see docs/guides/autostart.ko.md)"`. Do not overwrite.
   - File doesn't exist (autostart was never enabled): no migration needed
5. **Next-session activation**: on next user login, systemd loads the new file. Service starts under new Type=notify protocol normally.

**Init hook placement** (`src-tauri/src/main.rs`):
- Call `sd_notify::notify_ready()` AFTER:
  - Tauri builder.setup() finishes
  - All scheduler loops spawned
  - SQLite migrations applied
  - WebView main window shown (or hidden-to-tray confirmed)

```rust
// src-tauri/src/lifecycle/sd_notify.rs
//! systemd Type=notify integration. No-op on non-Linux.

#[cfg(all(target_os = "linux", feature = "systemd-notify"))]
pub fn notify_ready() {
    if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]) {
        tracing::debug!(err.code = "sd_notify_skipped", "sd_notify READY skipped: {e}");
    }
}

#[cfg(not(all(target_os = "linux", feature = "systemd-notify")))]
pub fn notify_ready() {}

#[cfg(all(target_os = "linux", feature = "systemd-notify"))]
pub fn notify_stopping() {
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
}

#[cfg(not(all(target_os = "linux", feature = "systemd-notify")))]
pub fn notify_stopping() {}
```

**Failure modes**:
- Process not run under systemd: `sd_notify::notify` returns Err → logged at debug, app continues
- `NOTIFY_SOCKET` env var missing: same as above
- systemd timeout (>30s init): systemd kills process → user sees "Service failed to start" in logs. Mitigation: ensure init path is fast (<5s typical). If init is slow, increase TimeoutStartSec in template.

---

### 6.2 Environment Detection — `autostart_capabilities` IPC

**File**: `src-tauri/src/commands/autostart.rs` (extend PR-B1 file's skeleton)

```rust
#[derive(serde::Serialize)]
pub struct AutostartCapabilities {
    pub supported: bool,
    pub unsupported_reason: Option<UnsupportedReason>,
    pub environment: EnvironmentKind,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum UnsupportedReason {
    SnapSandbox,
    FlatpakSandbox,
    HeadlessSession,
    SystemctlUnavailable,
    UnsupportedPlatform,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentKind {
    MacOs,
    Windows,
    LinuxSystemd,
    LinuxXdg,
    LinuxSnapSandbox,
    LinuxFlatpakSandbox,
    LinuxHeadless,
    Unknown,
}
```

**Detection logic** (Linux mod additions in PR-B2, expanding on `autostart::detect_capabilities()` which exists as skeleton in PR-B1):

```rust
#[cfg(target_os = "linux")]
pub fn detect_capabilities() -> AutostartCapabilities {
    // Sandbox detection (highest priority)
    if std::env::var("SNAP").is_ok() {
        return AutostartCapabilities {
            supported: false,
            unsupported_reason: Some(UnsupportedReason::SnapSandbox),
            environment: EnvironmentKind::LinuxSnapSandbox,
        };
    }
    if std::env::var("FLATPAK_ID").is_ok() {
        return AutostartCapabilities {
            supported: false,
            unsupported_reason: Some(UnsupportedReason::FlatpakSandbox),
            environment: EnvironmentKind::LinuxFlatpakSandbox,
        };
    }

    let has_display = std::env::var("DISPLAY").is_ok()
        || std::env::var("WAYLAND_DISPLAY").is_ok();
    if !has_display {
        return AutostartCapabilities {
            supported: false,
            unsupported_reason: Some(UnsupportedReason::HeadlessSession),
            environment: EnvironmentKind::LinuxHeadless,
        };
    }

    if has_systemctl() {
        AutostartCapabilities {
            supported: true,
            unsupported_reason: None,
            environment: EnvironmentKind::LinuxSystemd,
        }
    } else {
        AutostartCapabilities {
            supported: true,  // XDG fallback supported
            unsupported_reason: None,
            environment: EnvironmentKind::LinuxXdg,
        }
    }
}

#[cfg(target_os = "macos")]
pub fn detect_capabilities() -> AutostartCapabilities {
    AutostartCapabilities {
        supported: true,
        unsupported_reason: None,
        environment: EnvironmentKind::MacOs,
    }
}

#[cfg(target_os = "windows")]
pub fn detect_capabilities() -> AutostartCapabilities { /* ... Windows ... */ }
```

**PR-B1 skeleton** (placeholder in PR-B1, real impl in PR-B2):
```rust
// PR-B1 — returns supported=true unconditionally on Linux (no env detection)
#[cfg(target_os = "linux")]
pub fn detect_capabilities() -> AutostartCapabilities {
    AutostartCapabilities {
        supported: true,
        unsupported_reason: None,
        environment: EnvironmentKind::LinuxSystemd,  // assume systemd path
    }
}
```

**Onboarding prompt gating**: also check `caps.supported` — don't show prompt if can't honor it.

---

### 6.3 Linux Integration Tests (revised per I5)

**CI infrastructure** (revised — no `--privileged`):

Two-job split:
1. **Unit tests** (always-on, all PRs): service file generation, hash check, env detection logic with mocked env vars. No systemd needed. Run on `ubuntu-latest`.
2. **Live systemd integration** (manual trigger, PR-B2 only): use rootless systemd via `systemd-run --user --scope` or dedicated branch-protected workflow.

```yaml
# .github/workflows/ci.yml — unit tests (always-on)
linux-autostart-unit:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: cargo test -p oneshim-app --features systemd-notify --test linux_autostart_unit

# .github/workflows/linux-systemd-integration.yml — manual trigger
on:
  workflow_dispatch:
linux-systemd-integration:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Run rootless systemd
      run: |
        # Use systemd-run --user inside the runner's existing user session
        sudo apt-get install -y systemd dbus-user-session
        systemctl --user start dbus
        cargo test -p oneshim-app --features systemd-notify --test linux_autostart_systemd_live
```

**Tests covered** (`src-tauri/tests/linux_autostart_unit.rs`):
- T1: `generate_service_file` produces `Type=notify`
- T2: `generate_service_file` includes `NotifyAccess=main`, `TimeoutStartSec=30`
- T3: `detect_capabilities` returns `LinuxSnapSandbox` when SNAP env var set
- T4: `detect_capabilities` returns `LinuxFlatpakSandbox` when FLATPAK_ID set
- T5: `detect_capabilities` returns `LinuxHeadless` when DISPLAY/WAYLAND_DISPLAY absent
- T6: `migration_hash_check` matches known PR-B1-era template
- T7: `migration_hash_check` skips when file doesn't match (user customization)

**Tests covered** (`src-tauri/tests/linux_autostart_systemd_live.rs`, manual):
- T8: Real `systemctl --user enable oneshim.service` + verify file written
- T9: `sd_notify::notify_ready()` returns Ok when run under `systemd-run --user`
- T10: Service file migration end-to-end (write Type=simple, app start, verify defer + next-launch behavior)

---

### 6.4 Korean Operations Documentation

**File**: `docs/guides/autostart.ko.md` (NEW)

Structure:
- 개요: autostart 기능 소개
- 플랫폼별 동작: macOS/Windows/Linux
- Linux 환경별 지원: systemd, XDG, Snap/Flatpak, headless
- 마이그레이션: PR-B2 service file 변경 사항 + 사용자 customization 가이드 (per C4)
- 트러블슈팅:
  - "토글이 회색이에요" → 환경 매트릭스 설명
  - "활성화했는데 시작 안 돼요" → 로그 위치 + systemctl status 확인 방법
  - "Snap/Flatpak에서 안 돼요" → OS-native autostart 설정 안내
  - "service 파일이 customize되어 마이그레이션 스킵됨" → 수동 마이그레이션 단계
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
    → Returns Ok(())
  → setEnabled(true), setLoading(false)
  → Toggle visually reflects ON state
  
(NO config write — OS state is sole source of truth per I4)
```

### 7.2 Productive session counter increment (revised per C5)

```
User completes 25-min focus block
  → scheduler/loops/monitor.rs detects threshold
  → generates session_id UUID (fresh per session)
  → calls config_mgr.update_with(|c| {
      if c.autostart.last_session_id == Some(session_id) { return Ok(()); }  // idempotent
      c.autostart.productive_session_count += 1;
      c.autostart.last_session_id = Some(session_id);
      Ok(())
    })
  → if should_prompt(snapshot.autostart): app.emit("autostart:eligible-for-prompt", ())

Frontend (Dashboard mounted with AutostartOnboardingPromptHost):
  → listener fires for autostart:eligible-for-prompt
  → re-evaluates eligibility from fresh config
  → if eligible AND !hasShownThisSession: setTimeout 500ms → show modal

User clicks "Not now"
  → invoke('mark_autostart_prompt_state', { newState: { kind: 'snoozed', remind_after_session_count: count + 5 } })
  → Modal closes
  → hasShownThisSession = true (single-fire — no re-show until app restart)
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

(Headless Linux fallback per I7: 2nd process launches as standalone, log warn)
```

### 7.4 Linux systemd Type=notify (PR-B2)

```
systemd starts ONESHIM via user service unit (autostart enabled)
  → ExecStart={binary} runs
  → Process inherits NOTIFY_SOCKET env var from systemd
  → systemd waits for READY notification (timeout: 30s per unit file)

ONESHIM init:
  → migration check: read existing service file, hash check
    - matches old template → write new template, log info, NO daemon-reload
    - matches new template → no-op
    - doesn't match → log warn, skip (user customization)
  → Tauri builder.setup() runs
  → All scheduler loops spawn (16 loops)
  → SQLite migrations applied
  → Main window shown (or hidden-to-tray confirmed)
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
| OS enable fails (permissions, disk full) | `autostart::enable_autostart()` returns Err | Error banner. Toggle reverts (UI re-fetches OS) | `enable_autostart` IPC propagates Err |
| systemctl missing on Linux | (PR-B1) `has_systemctl()` returns false → XDG fallback. (PR-B2) capabilities returns `LinuxXdg`, toggle still works | XDG `.desktop` file written instead | `autostart::linux::enable` |
| D-Bus unavailable for single-instance | Plugin's IPC silently fails. (No init Result.) | 2nd instance launches as standalone. Log warn at startup if `DBUS_SESSION_BUS_ADDRESS` absent | main.rs startup check |
| sd_notify NOTIFY_SOCKET missing | `sd_notify::notify` returns Err | Logged at debug, app continues | lifecycle::sd_notify |
| User on Snap/Flatpak attempts toggle | (PR-B2) `autostart_capabilities` returns unsupported | Toggle disabled with tooltip | UI gating |
| User on headless Linux attempts toggle | (PR-B2) capabilities returns LinuxHeadless | Toggle disabled with tooltip | UI gating |
| Existing user's service file is `Type=simple` after PR-B2 deploys | Startup migration check | If hash matches: write new file, log info, defer reload to next session. If hash mismatches: log warn, skip | PR-B2 init migration |
| 2nd instance fires while 1st is hidden in tray (Wayland kept-hidden) | Plugin callback runs | window.show() may not surface on Wayland — fallback: log + manual fallback | Manual smoke test verifies |
| Productive session counter overflow (u32 max) | `saturating_add` | Counter caps at u32::MAX, prompt logic still works | scheduler/loops/monitor.rs |
| Concurrent enable/disable via rapid double-click | UI loading state guards | First click locks toggle | `if (loading) return` |
| Counter increment write fails (config disk full) | `update_with` returns Err | Logged at warn (`autostart_counter_increment_failed`), no user impact | scheduler emit |
| Migration to Type=notify breaks running service (despite hash check) | systemd marks unit failed after timeout | Service restart loop. User sees "Service failed to start" in logs. Recovery: re-toggle from Settings UI | PR-B2 docs/guides/autostart.ko.md user runbook |

### 8.2 Logging conventions (wire codes for Loki/Grafana grouping)

- `autostart_enable_failed`
- `autostart_disable_failed`
- `autostart_counter_increment_failed`
- `autostart_capability_check_failed`
- `single_instance_dbus_absent`
- `sd_notify_skipped`
- `autostart_service_migrated`
- `autostart_service_migration_skipped` (user customization)

Example:
```rust
warn!(
    err.code = "autostart_enable_failed",
    platform = std::env::consts::OS,
    "autostart enable failed: {e}"
);
```

---

## 9. Testing Strategy

### 9.1 Unit tests

**`crates/oneshim-core/src/config/sections/autostart.rs`** (new):
- `default_config_is_pending`
- `prompt_state_serde_roundtrip` (each variant)
- `should_prompt_pending_with_zero_count` → false
- `should_prompt_pending_with_one_count` → true
- `should_prompt_snoozed_below_threshold` → false
- `should_prompt_snoozed_at_threshold` → true
- `should_prompt_dismissed_always_false`
- `migration_from_old_config_uses_default` (no `autostart` field → default)
- `idempotency_via_session_id` — same session_id called twice doesn't double-increment

**`src-tauri/src/commands/autostart.rs`** (new):
- `enable_autostart_propagates_os_error`
- `disable_autostart_propagates_os_error`
- `is_autostart_enabled_returns_os_state`
- `mark_prompt_state_persists` (Pending/Snoozed/Dismissed each)
- `autostart_capabilities_returns_skeleton_in_b1` (PR-B1)

**`src-tauri/src/scheduler/loops/monitor.rs`** (extended):
- `productive_session_increments_counter`
- `productive_session_idempotent_via_session_id`
- `productive_session_below_25_min_no_increment`
- `productive_session_emits_event_when_eligible`
- `productive_session_no_event_when_dismissed`

**`src-tauri/src/autostart.rs`** (existing tests preserved + extended):
- (PR-B2) `detect_capabilities_macos`
- (PR-B2) `detect_capabilities_windows`
- (PR-B2) `detect_capabilities_linux_systemd`
- (PR-B2) `detect_capabilities_snap`
- (PR-B2) `detect_capabilities_flatpak`
- (PR-B2) `detect_capabilities_headless`
- (PR-B2) `service_file_uses_type_notify`
- (PR-B2) `service_file_includes_notify_access_main`
- (PR-B2) `service_file_includes_timeout_start_sec`
- (PR-B2) `migration_hash_matches_known_template`
- (PR-B2) `migration_skips_on_unknown_hash`

**`src-tauri/src/lifecycle/sd_notify.rs`** (new, PR-B2):
- `notify_ready_no_op_on_non_linux`
- `notify_ready_returns_ok_with_socket`
- `notify_ready_logs_when_socket_missing`

### 9.2 Frontend tests (Vitest)

**`crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx`**:
- `startup_section_renders`
- `toggle_initial_state_loads_from_ipc`
- `toggle_loads_capabilities`
- `toggle_disabled_when_capabilities_unsupported`
- `toggle_click_invokes_enable_or_disable`
- `toggle_error_re_fetches_os_state`
- `toggle_disabled_during_loading`
- `unsupported_tooltip_uses_i18n_context`

**`crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.test.tsx`** (new):
- `prompt_visible_when_eligible`
- `prompt_hidden_when_dismissed`
- `prompt_hidden_when_snoozed_below_threshold`
- `single_fire_per_session` — once shown, never re-shown until restart
- `enable_button_invokes_correct_ipcs`
- `not_now_button_sets_snoozed_with_count_plus_5`
- `dismiss_button_sets_dismissed`
- `escape_key_treated_as_not_now`
- `outside_click_treated_as_not_now`
- `event_listener_re_evaluates_eligibility`

### 9.3 Integration tests (PR-B1)

**`src-tauri/tests/autostart_ipc_integration.rs`** (new):
- IPC → real `autostart::*` round-trip on host platform
- Smoke test: enable → is_enabled = true → disable → is_enabled = false
- Cleanup: always disable on tear-down

**`src-tauri/tests/single_instance_integration.rs`** (new):
- Spawn 2nd binary as child process → expect exit code 0 within 2s
- Skip on Linux without D-Bus session bus

### 9.4 Integration tests (PR-B2)

**`src-tauri/tests/linux_autostart_unit.rs`** (new, `#[cfg(target_os = "linux")]`):
- T1-T7 as listed in §6.3 (no real systemd needed)

**`src-tauri/tests/linux_autostart_systemd_live.rs`** (manual trigger only):
- T8-T10 with rootless systemd-run

### 9.5 Manual smoke test matrix

**PR-B1 sign-off**:
- macOS (latest): toggle, autostart at login, single-instance focus-grab
- Windows 11: same
- Linux Ubuntu 24.04 (X11 GNOME): same
- Linux Wayland (Fedora 40 GNOME): single-instance focus-grab on kept-hidden window — **per I1, explicit case**
- Linux Wayland (sway): same

**PR-B2 sign-off** (additional):
- Linux Snap: toggle disabled with tooltip
- Linux Flatpak: toggle disabled with tooltip
- Linux headless SSH: toggle disabled with tooltip
- Linux service file migration: install PR-B1 → enable autostart → upgrade to PR-B2 → verify file written, daemon-reload deferred, next login starts cleanly

**Recording location** (per §12.1 Q6): each PR description includes a "Manual Smoke Test Results" section as a checklist with one row per platform. Persisted in PR body (markdown table). Optional: copy to `.claude/manual-smoke-tests/<release>.md` for historical record.

### 9.6 Pass criteria

- All unit tests GREEN
- All Vitest tests GREEN
- All integration tests GREEN (host platform only for non-Linux integration)
- `cargo check/test/clippy/fmt --workspace` GREEN (per ADR-001)
- Frontend `pnpm lint` GREEN
- Manual smoke test matrix per phase (above)
- No new clippy warnings
- i18n key parity en.json ↔ ko.json

---

## 10. Delivery Plan

### 10.1 PR-B1 commit structure (~23h, +1h vs v1 for capabilities skeleton per I8)

| # | Commit | Estimate | Files |
|---|--------|----------|-------|
| 1 | `chore(autostart): add tauri-plugin-single-instance dep` | 0.5h | `src-tauri/Cargo.toml`, `Cargo.lock` |
| 2 | `feat(autostart): AutostartConfig + AutostartPromptState in core (no enabled cache)` | 1.5h | `crates/oneshim-core/src/config/sections/autostart.rs`, `mod.rs`, `crates/oneshim-core/src/config/mod.rs` |
| 3 | `test(autostart): AutostartConfig serde + should_prompt + idempotency unit tests` | 1.5h | `autostart.rs` (tests submodule) |
| 4 | `feat(autostart): IPC commands (5 commands incl. capabilities skeleton)` | 2.5h | `src-tauri/src/commands/autostart.rs`, `commands/mod.rs`, `main.rs` invoke_handler |
| 5 | `test(autostart): IPC command unit tests` | 1.5h | `commands/autostart.rs` tests + `tests/autostart_ipc_integration.rs` |
| 6 | `feat(autostart): single-instance plugin in main builder + focus-grab callback + D-Bus presence check (per I3+I7)` | 2h | `src-tauri/src/main.rs` |
| 7 | `test(autostart): single-instance integration smoke test` | 1.5h | `src-tauri/tests/single_instance_integration.rs` |
| 8 | `feat(autostart): GeneralTab Startup section + toggle wiring + capabilities-aware UI` | 2.5h | `GeneralTab.tsx`, i18n |
| 9 | `test(autostart): GeneralTab Vitest coverage` | 1h | `GeneralTab.test.tsx` |
| 10 | `feat(autostart): productive-session detection + Rust-side counter increment in monitor.rs` | 2.5h | `src-tauri/src/scheduler/loops/monitor.rs` (or new helper) |
| 11 | `test(autostart): productive-session detection unit tests (idempotency, threshold)` | 1.5h | corresponding tests |
| 12 | `feat(autostart): AutostartOnboardingPrompt + ShowPromptCoordinator + Dashboard integration` | 2.5h | `AutostartOnboardingPrompt.tsx`, `Dashboard.tsx`, i18n |
| 13 | `test(autostart): AutostartOnboardingPrompt Vitest coverage incl. single-fire` | 1.5h | corresponding `.test.tsx` |
| 14 | `docs(autostart): STATUS.md + PHASE-HISTORY entry for PR-B1` | 0.5h | `docs/STATUS.md`, `docs/PHASE-HISTORY.md` |
| 15 | `chore(autostart): manual smoke test matrix per platform + checklist (PR body)` | 1h | manual session, no commit |

**Total**: ~22h (per Phase 1 iter-3: bundled D-Bus check into commit 6 to remove redundant standalone commit per N-I3).

**Commit dependency graph** (per Phase 1 iter-2 N-N1):
- Hard ordering: `2 → 3` (type before tests), `2 → 4` (type before commands), `4 → 5` (commands before tests), `2 → 10` (type before scheduler), `4 → 12` (mark_prompt_state IPC before frontend), `10 → 12` (event emitter before listener)
- Wire codes (`autostart.enable_failed/disable_failed/query_failed`) added to `wire_contract_snapshot.expected.txt` + `wire-errors.{en,ko}.json` as part of commit 4 (alongside IPC commands; otherwise CI fails)

### 10.2 PR-B2 commit structure (~15h, unchanged from v1)

| # | Commit | Estimate |
|---|--------|----------|
| 1 | `chore(autostart): add sd-notify dep with feature flag` | 0.5h |
| 2 | `feat(autostart): lifecycle::sd_notify wrapper module` | 1.5h |
| 3 | `test(autostart): sd_notify unit coverage` | 1h |
| 4 | `feat(autostart): change Linux service file template to Type=notify + NotifyAccess + TimeoutStartSec` | 1.5h |
| 5 | `feat(autostart): hash-based service file migration (defer reload, skip user customization)` | 2h |
| 6 | `test(autostart): migration hash check + defer behavior` | 1.5h |
| 7 | `feat(autostart): wire sd_notify::notify_ready/stopping in main lifecycle` | 1h |
| 8 | `feat(autostart): real detect_capabilities replacing PR-B1 skeleton` | 2h |
| 9 | `test(autostart): detect_capabilities unit coverage per env` | 1.5h |
| 10 | `feat(autostart): linux_autostart_unit.rs CI integration (no --privileged)` | 1.5h |
| 11 | `feat(autostart): manual-trigger linux_autostart_systemd_live.rs workflow` | 1h |
| 12 | `docs(autostart): docs/guides/autostart.ko.md operations + migration guide` | 1h |
| 13 | `docs(autostart): STATUS.md + PHASE-HISTORY for PR-B2` | 0.5h |

**Total**: ~15h.

### 10.3 Branch naming

- PR-B1: `feature/phase9-autostart-foundation` (already created)
- PR-B2: `feature/phase9-autostart-linux-deep` (off main after B1 merges)

### 10.4 Release plan

- PR-B1 merges → release `0.4.40-rc.1`
- PR-B2 merges → release `0.4.41-rc.1`

---

## 11. Migration & Backward Compatibility

### 11.1 Existing user upgrade path

| Pre-PR-B1 state | Post-PR-B1 behavior |
|-----------------|---------------------|
| autostart never enabled, no `autostart` field in config | `AutostartConfig::default()` applied. Settings shows OFF (per OS). Onboarding prompt fires after first 25-min session. |
| autostart manually enabled via OS (user added LaunchAgent themselves) | `is_autostart_enabled` returns true. Settings toggle reflects ON. Config has no `enabled` cache (per I4). |

### 11.2 Service file migration (PR-B2, revised per C4)

- Detection: on startup, read existing service file (if exists), compute SHA-256
- Hash matrix (maintained in `src-tauri/src/lifecycle/migration_hashes.rs`):
  - `KNOWN_PRIOR_HASHES`: list of hashes of all prior templates (currently 1: PR-B1-era Type=simple)
- Decision tree:
  1. File doesn't exist → no-op (autostart wasn't enabled)
  2. File hash matches `KNOWN_PRIOR_HASHES`: safe to overwrite. Write new file, log info `autostart_service_migrated`, **DO NOT** call `daemon-reload`. systemd loads new file on next user login session.
  3. File hash matches NEW PR-B2 template: already migrated, no-op
  4. File hash doesn't match anything: user customized. Log warn `autostart_service_migration_skipped`. Document in `docs/guides/autostart.ko.md` how user can manually update.

### 11.3 Downgrade safety

- Pre-PR-B1 → install PR-B1: forward-compatible (default config)
- PR-B1 → downgrade to pre-PR-B1: `autostart` field unknown but `AppConfig` doesn't have `deny_unknown_fields` (verified §12.1 Q3). Field silently dropped. Safe.
- PR-B2 → downgrade to PR-B1: service file has `Type=notify` but PR-B1 doesn't call `sd_notify_ready()`. systemd marks unit failed after `TimeoutStartSec=30`. **Recovery**: PR-B2 release notes recommend disable+re-enable on downgrade, or document manual edit of service file back to `Type=simple`.

### 11.4 Reconciler — startup-time consistency check (NEW per C3)

On app startup, reconcile observed OS state vs prior config snapshot:

```rust
// src-tauri/src/lifecycle/reconciler.rs (NEW, called from main.rs setup hook)
async fn reconcile_autostart_state(config_mgr: &ConfigManager) {
    let os_state = match autostart::is_autostart_enabled() {
        Ok(s) => s,
        Err(e) => {
            warn!(err.code = "autostart_reconcile_query_failed", "{e}");
            return;
        }
    };

    // No config-side autostart enabled cache to compare against (per I4).
    // Reconciler instead checks: does this user have prompt_state == Dismissed
    // AND OS state is false? If so, log info — user previously dismissed but
    // never explicitly disabled at OS level. Could indicate manual revert.
    let cfg = config_mgr.get();
    if matches!(cfg.autostart.prompt_state, AutostartPromptState::Dismissed) && !os_state {
        info!(
            err.code = "autostart_reconcile_state",
            "User previously dismissed prompt; OS autostart is currently disabled"
        );
    }
}
```

The reconciler is informational only — no auto-correct. Users can re-toggle from Settings UI to fix any drift.

---

## 12. Open Questions / Future Work

### 12.1 Resolved questions (Q1-Q6)

- **Q1 (Tauri identifier match)**: ✅ Identifier is `com.oneshim.client` (`tauri.conf.json:4`). Plugin uses this as D-Bus name. APP_LABEL `com.oneshim.agent` is intentionally separate (LaunchAgent service name). See §1.2.
- **Q2 (ConfigManager API)**: ✅ Real API is `update_with<F: FnOnce(&mut AppConfig) -> Result<(), String>> -> Result<AppConfig, CoreError>` (sync, returns whole config snapshot). Spec §5.1 rewritten to use this.
- **Q3 (`deny_unknown_fields`)**: ✅ NOT set on `AppConfig` (verified `crates/oneshim-core/src/config/mod.rs:20`). Downgrade-safe. **Supplement**: i18n key parity CI lint name TBD — verify exists or add to NICE-TO-HAVE.
- **Q4 (productive-session event)**: ✅ Does NOT exist in current scheduler. PR-B1 adds Rust-side detection in `monitor.rs` (per §5.5 + commit 11). focus_metrics is daily aggregate; per-session detection requires new in-memory state in scheduler.
- **Q5 (sd-notify 0.4 acceptable)**: ✅ Not currently in workspace. Adding as optional Linux-only dep with feature flag `systemd-notify`. Workspace policy: confirmed acceptable per recent additions (e.g., chrono-tz in PR-A).
- **Q6 (smoke matrix recording)**: ✅ Per-PR description body as checklist table. Optional historical archive to `.claude/manual-smoke-tests/<release>.md`.

### 12.2 Resolved in iter-2 (Q7-Q9)

- **Q7**: ✅ Resolved. Plugin source verified at `tauri-apps/plugins-workspace v2/plugins/single-instance/src/platform_impl/linux.rs:31-49` — D-Bus well-known name = `<identifier>.SingleInstance` = **`com.oneshim.client.SingleInstance`**. Object path = `/com/oneshim/client/SingleInstance`. Interface = `org.SingleInstance.DBus`. No identifier transformation; suffix only. (`.SingleInstance_<semver>` if `semver` feature enabled — we do NOT enable it.)
- **Q8**: ✅ Resolved. Existing `scripts/check-wire-error-i18n-coverage.sh` ONLY checks wire-error keys (`wire-errors.{en,ko}.json` against the snapshot registry). NO general i18n key parity lint exists. Recommendation: keep manual review for `settings.general.autostart.*` and `onboarding.autostart.*` — adding a general parity script is out of PR-B1 scope. If parity drift becomes an issue post-launch, add as separate PR.
- **Q9**: ✅ Resolved. `autostart::linux::is_enabled()` already checks BOTH `service_path()` AND `desktop_path()` (verified `autostart.rs:449-457`). The reconciler (§11.4) calls `autostart::is_autostart_enabled()` which routes to `is_enabled()` for Linux — automatically gets the union. No spec change needed.

### 12.3 Future work explicitly deferred

- **F1**: CLI commands (`oneshim --quit`, `oneshim --status`) using `tauri-plugin-single-instance` args+cwd callback (NG3)
- **F2**: D-Bus method exposure for external automation tools (NG2)
- **F3**: Snap/Flatpak best-effort autostart (NG5 — only if we publish those packages ourselves)
- **F4**: macOS LaunchAgent KeepAlive=true
- **F5**: Windows scheduled task with LogonType=Interactive as alternative to Run key
- **F6**: `update_with_returning<F, T>` API in ConfigManager for value-returning closures (per I6)

---

## 13. Risk Register (re-rated per N4)

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `tauri-plugin-single-instance` D-Bus name doesn't match Tauri identifier exactly | Low | Medium | Verify Q7 in iter-2 |
| systemd Type=notify with 30s timeout too tight for slow init | Medium | Medium | Bench init time, increase if needed |
| Type=simple → Type=notify migration corrupts running service | Low | **Medium** ⬆️ | Hash check + defer reload (per C4) |
| User customization clobbered by migration | Low | **Medium** ⬆️ | Hash check skips unknown files (per C4) |
| Onboarding prompt feels intrusive | Medium | Low | UX studies post-launch |
| Productive session counter has scheduler-restart double-count | Low | Low | Idempotency via session_id UUID |
| Frontend i18n key drift between en.json and ko.json | Low | Low | CI lint (Q8 verifies exists) |
| Plugin init failure on headless Linux | Low | Medium | Log warn at startup, accept duplicate-process behavior (per I7) |
| Wayland kept-hidden window unmappable on focus | Medium | Medium | **Accepted as known limitation in PR-B1** (per Phase 1 iter-2 N-I4). Manual smoke test in §9.5 surfaces the case. If broken: documented in PR-B2 `docs/guides/autostart.ko.md` user runbook. Concrete fallback (e.g., `window.create()` if `is_visible()` returns false post-`set_focus()`) is a follow-up PR if smoke test reveals breakage. |
| Two-phase commit revert fails (no longer applicable) | N/A | N/A | Removed two-phase commit (per C3) |
| `cargo build` size increase from `tauri-plugin-single-instance` | Low | Low | Plugin is small |
| Cross-consumer merge conflicts with features2/grpc-stress branches | Medium | Medium | Coordinate merge order (see §17) |

---

## 14. Appendix

### 14.1 File inventory

**PR-B1 new files**:
- `src-tauri/src/commands/autostart.rs`
- `crates/oneshim-core/src/config/sections/autostart.rs`
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx`
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.test.tsx`
- `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx` (or extension if exists)
- `src-tauri/tests/autostart_ipc_integration.rs`
- `src-tauri/tests/single_instance_integration.rs`
- `src-tauri/src/lifecycle/reconciler.rs` (per §11.4)

**PR-B1 modified files**:
- `src-tauri/Cargo.toml`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/main.rs`
- `src-tauri/src/scheduler/loops/monitor.rs` (productive-session detection)
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
- `src-tauri/src/lifecycle/migration_hashes.rs`
- `src-tauri/tests/linux_autostart_unit.rs`
- `src-tauri/tests/linux_autostart_systemd_live.rs` (manual)
- `docs/guides/autostart.ko.md`
- `.github/workflows/linux-systemd-integration.yml` (manual trigger)

**PR-B2 modified files**:
- `src-tauri/Cargo.toml` (sd-notify dep)
- `src-tauri/src/commands/autostart.rs` (real capabilities)
- `src-tauri/src/autostart.rs` (Linux mod: notify, env detect, service file template)
- `src-tauri/src/main.rs` (sd_notify hooks, lifecycle wiring)
- `crates/oneshim-web/frontend/src/i18n/en.json`, `ko.json` (capability tooltips)
- `.github/workflows/ci.yml` (linux-autostart-unit job)
- `docs/STATUS.md`, `docs/PHASE-HISTORY.md`

### 14.2 New dependencies

| Crate | Version | PR | Purpose |
|-------|---------|----|---------|
| `tauri-plugin-single-instance` | `2` | PR-B1 | Cross-platform single-instance enforcement |
| `sd-notify` | `0.4` (Linux only, optional via feature flag) | PR-B2 | systemd Type=notify protocol |

### 14.3 ADRs referenced

- **ADR-001** (Rust Client Architecture Patterns)
- **ADR-003** (Directory Module Pattern)
- **ADR-004** (Tauri v2 Migration)
- **ADR-019** (Error Code Infrastructure)

### 14.4 Alignment with project memory

- `feedback_3loop_quality_gate`: spec → plan → impl, each with deep review (current Phase 1)
- `feedback_industry_convention_check`: U3 cites App Store 5.4.1 + Microsoft Store 10.2.4
- `feedback_subagent_driven_catches_stale_plans`: Phase 1 review caught 5 Critical issues
- `feedback_cross_consumer_audit`: §17 enumerates conflicts
- `feedback_release_process`: release.sh / promote-stable.sh
- `feedback_test_forwarding_and_source`: scheduler emission has unit test on producer side
- `feedback_pipelined_reviews_pattern`: Phase 1 used 3 parallel subagent reviewers

---

## 15. Spec Self-Review (v2)

### 15.1 Placeholders / TODOs scan
- ✅ No "TBD" in spec body
- ⚠ Q7-Q9 (§12.2) intentionally listed for iter-2 verification

### 15.2 Internal consistency
- ✅ Removal of `enabled` field consistently applied across §5.3, §5.1, §11.1, §11.4
- ✅ Counter increment moved to Rust-side consistently in §5.5, §7.2, §10.1 commit 11
- ✅ Migration hash check consistently applied in §6.1, §11.2, commit 5

### 15.3 Scope check
- ✅ 2 PRs sequential, both within client-rust
- Reconciler (§11.4) is small addition, included in scope

### 15.4 Ambiguity check
- ✅ "First productive session" defined as ≥25 min focus block detected by scheduler
- ✅ "Single-fire" defined as `hasShownThisSession` module flag, reset on app restart
- ✅ "Hash check" defined as SHA-256 against `KNOWN_PRIOR_HASHES` list

---

## 16. Implementation Status

- **Spec v1**: 2026-04-25 (commit `fd8f64cf`)
- **Spec v2**: 2026-04-25 (this document — incorporates 5 Critical + 8 Important fixes)
- **Worktree**: `/Volumes/.../client-rust/.claude/worktrees/phase9-autostart-foundation` on `feature/phase9-autostart-foundation`
- **Phase 1 review iter-1**: complete (`/.claude/pr-b-review/phase1-iter1-findings.md`)
- **Phase 1 review iter-2**: TBD — verify v2 fixes are correct, address Q7-Q9
- **Phase 2 (writing-plans)**: TBD — only after Phase 1 zero-issue gate
- **Phase 3 (subagent-driven impl)**: TBD

---

## 17. Cross-Consumer Dependencies (NEW per Phase 1 audit)

### 17.1 Active branches modifying overlapping files

| Branch | Files in conflict with PR-B1 | Conflict severity | Mitigation |
|--------|------------------------------|-------------------|------------|
| `feature/external-grpc-audit-liveconfig` (features2) | `crates/oneshim-core/src/config/mod.rs` (AppConfig), `GeneralTab.tsx`, `commands/settings.rs` | **CRITICAL** | features2 merges first per memory; PR-B1 rebases onto features2-merged main |
| `feature/grpc-stress-test-suite` | `Cargo.toml`, AppConfig, `commands/mod.rs`, `commands/settings.rs` | Important | Whichever merges second rebases; conflict resolution mechanical (additive) |
| `fix/phase9-pr-a-followup-cleanup` | `tracking_schedule` related, AppConfig | **CRITICAL** | PR-A itself (`feature/phase9-tracking-schedule` #487) is **already merged** to main per memory `project_next_tasks`. Only `fix/phase9-pr-a-followup-cleanup` remains as a separate cleanup branch. Merge before PR-B1 to avoid tracking_schedule conflicts. |
| `feature/d13-v2b-pr-b2-subscribe-metrics` | `crates/oneshim-core/src/config/mod.rs` (REMOVES `external_grpc`) | **CRITICAL SEMANTIC** | Coordinate with v2b owner; likely merges before PR-B1 |

### 17.2 Recommended merge order

(PR-A `feature/phase9-tracking-schedule` #487 already in main HEAD `5618558c`; not in this list)

1. `fix/phase9-pr-a-followup-cleanup` (cleanup of merged PR-A)
2. `feature/d13-v2b-pr-b2-subscribe-metrics` (semantic field removal)
3. `feature/external-grpc-audit-liveconfig` (features2)
4. `feature/grpc-stress-test-suite`
5. **`feature/phase9-autostart-foundation` (PR-B1)**
6. `feature/phase9-autostart-linux-deep` (PR-B2 — off main after PR-B1)

### 17.3 Pre-PR-B1 actions

Before opening PR-B1:
- Verify queue position: confirm 1-4 above are merged
- Rebase `feature/phase9-autostart-foundation` onto current main
- Re-run cargo check/test/clippy/fmt to catch any drift
- Update §17 in this spec with actual merge outcomes

---

**End of spec v2.**
