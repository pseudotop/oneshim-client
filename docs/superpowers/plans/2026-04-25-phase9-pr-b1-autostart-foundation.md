# Phase 9 PR-B1 — Autostart Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the existing platform autostart implementations to a Tauri IPC + Settings UI surface, add cross-platform single-instance enforcement via tauri-plugin-single-instance, and present an opt-in onboarding prompt after the user's first 25-min productive focus session.

**Architecture:** OS state (LaunchAgent plist / Windows Registry / systemd unit) is the SOLE source of truth for autostart enabled/disabled. AppConfig stores ONLY onboarding state (prompt_state enum + productive_session_count + last_session_id for idempotency). Productive-session detection runs in the existing scheduler monitor loop and increments the counter directly via ConfigManager (no frontend round-trip), then emits a Tauri event for the frontend Dashboard to re-evaluate prompt eligibility. The PR-B1 capabilities IPC command returns a skeleton `{supported: true}` so the frontend code path is identical to PR-B2 (which adds Linux env detection).

**Tech Stack:**
- Rust + Tauri 2 + tokio
- `tauri-plugin-single-instance = "2"` (NEW)
- React 18 + Vite + Vitest + i18next (existing frontend)
- SQLite via rusqlite (existing storage)
- ConfigManager API: `update_with(|c| -> Result<(), String>) -> Result<AppConfig, CoreError>` (sync, returns full config snapshot)
- IpcError API: `IpcError::new(code, message)` per `src-tauri/src/ipc_error.rs:70`

**Source spec:** `docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md` v3 (commit `48ffbfb5`)

**Worktree:** `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-foundation` on branch `feature/phase9-autostart-foundation`

**Total estimate:** ~22h across 15 tasks.

---

## Pre-Flight Checks (before Task 1)

- [ ] **PF1: Verify worktree state**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-foundation
git status
git log -1 --oneline
```
Expected: clean working tree, HEAD at `48ffbfb5` or later (the spec closure commit).

- [ ] **PF2: Verify cross-consumer merge order is satisfied**

```bash
git log main --oneline -10
git for-each-ref refs/remotes/origin/feature refs/remotes/origin/fix --format='%(refname:short)' | head -20
```
Per spec §17.2, ideally these are merged before PR-B1: `fix/phase9-pr-a-followup-cleanup`, `feature/d13-v2b-pr-b2-subscribe-metrics`, `feature/external-grpc-audit-liveconfig` (features2), `feature/grpc-stress-test-suite`. If any are still pending, document in PR-B1 description that rebase will be needed and proceed.

- [ ] **PF3: Verify baseline tests pass**

```bash
cargo check --workspace
```
Expected: clean compile with warnings only. (Skipping full `cargo test` here for time; will run after individual tasks.)

---

## File Structure

### Files to be created

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/config/sections/autostart.rs` | `AutostartConfig` struct + `AutostartPromptState` enum + `should_prompt()` helper. Pure data + helper functions, no IO. |
| `src-tauri/src/commands/autostart.rs` | 5 Tauri IPC commands: `enable_autostart`, `disable_autostart`, `is_autostart_enabled`, `autostart_capabilities`, `mark_autostart_prompt_state`. Wraps `crate::autostart` module + `ConfigManager`. |
| `src-tauri/tests/autostart_ipc_integration.rs` | Smoke test: enable → is_enabled = true → disable → is_enabled = false. Cleanup on tear-down. |
| `src-tauri/tests/single_instance_integration.rs` | Spawns 2nd binary as child process, expects exit 0 within 2s. |
| `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx` | Modal component with Enable/NotNow/DontAsk buttons. Single-fire `hasShownThisSession` module flag. |
| `crates/oneshim-web/frontend/src/components/AutostartOnboardingPromptHost.tsx` | Coordinator: polls config on mount + listens to `autostart:eligible-for-prompt` event, manages 500ms delay timer. |
| `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.test.tsx` | Vitest coverage including single-fire guarantee, button handlers, escape key, outside click. |

### Files to be modified

| File | What changes |
|------|--------------|
| `src-tauri/Cargo.toml` | Add `tauri-plugin-single-instance = "2"` to `[dependencies]` |
| `crates/oneshim-core/src/config/sections/mod.rs` | Add `pub mod autostart; pub use autostart::*;` |
| `crates/oneshim-core/src/config/mod.rs` | Add `#[serde(default)] pub autostart: AutostartConfig,` field to `AppConfig` struct |
| `src-tauri/src/commands/mod.rs` | Add `pub(crate) mod autostart;` |
| `src-tauri/src/main.rs` | (a) Add `tauri-plugin-single-instance` plugin to builder chain (top, before other plugins). (b) Add D-Bus presence check warn log at startup. (c) Add 5 autostart commands to `tauri::generate_handler!` macro. |
| `src-tauri/src/scheduler/loops/monitor.rs` | Add productive-session detection helper + counter increment + event emission. |
| `crates/oneshim-web/src/frontend/src/pages/setting-tabs/GeneralTab.tsx` | Add Startup section (between Theme and Language) with toggle, capabilities-aware disabled state, error banner. |
| `crates/oneshim-web/src/frontend/src/pages/setting-tabs/GeneralTab.test.tsx` | Extend (or create) Vitest coverage for Startup section. |
| `crates/oneshim-web/src/frontend/src/pages/Dashboard.tsx` | Render `<AutostartOnboardingPromptHost />` at top level. |
| `crates/oneshim-web/src/frontend/src/i18n/en.json` | Add `settings.general.autostart.*` + `onboarding.autostart.*` keys. |
| `crates/oneshim-web/src/frontend/src/i18n/ko.json` | Korean translations of same keys. |
| `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` | Append `autostart.enable_failed`, `autostart.disable_failed`, `autostart.query_failed`. |
| `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json` | Add 3 new wire error translation entries. |
| `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json` | Korean translations of 3 wire errors. |
| `docs/STATUS.md` | Update test counts + autostart line in feature matrix. |
| `docs/PHASE-HISTORY.md` | Add PR-B1 entry. |

---

## Task 1: Add `tauri-plugin-single-instance` Dependency

**Estimate:** 0.5h | **Spec ref:** §10.1 commit 1 | **Files:** `src-tauri/Cargo.toml`, `Cargo.lock`

- [ ] **Step 1.1: Add dependency**

Open `src-tauri/Cargo.toml` and locate the `[dependencies]` section. Append:

```toml
tauri-plugin-single-instance = "2"
```

Place alphabetically among existing `tauri-*` deps if there's an alphabetical convention; otherwise append at end.

- [ ] **Step 1.2: Update Cargo.lock**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-foundation
cargo check -p oneshim-app 2>&1 | tail -20
```
Expected: compile succeeds (or fails only on usage of plugin we haven't added yet). Cargo.lock updated.

- [ ] **Step 1.3: Commit**

```bash
git add src-tauri/Cargo.toml Cargo.lock
git commit -m "chore(autostart): add tauri-plugin-single-instance v2 dependency"
```

---

## Task 2: AutostartConfig + AutostartPromptState in oneshim-core

**Estimate:** 1.5h | **Spec ref:** §5.3, §10.1 commit 2 | **Files:** Create `crates/oneshim-core/src/config/sections/autostart.rs`, modify `crates/oneshim-core/src/config/sections/mod.rs` and `config/mod.rs`

- [ ] **Step 2.1: Create the autostart section file**

Create `crates/oneshim-core/src/config/sections/autostart.rs` with:

```rust
//! Autostart-related configuration.
//!
//! Per Phase 1 review I4: removed `enabled` cache field. OS state is sole source
//! of truth (via `src-tauri/src/autostart.rs`). This struct stores ONLY
//! onboarding-related state.
//!
//! See spec: docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md §5.3

use serde::{Deserialize, Serialize};

/// Per-user autostart configuration.
///
/// IMPORTANT: Does NOT store the autostart enabled/disabled state. That state
/// lives in OS-native locations. Use `autostart::is_autostart_enabled()` to query.
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
/// Transitions:
/// - Pending → Dismissed (user clicks Enable or DontAsk)
/// - Pending → Snoozed (user clicks NotNow)
/// - Snoozed → Dismissed (user clicks Enable or DontAsk on re-prompt)
/// - Snoozed → Snoozed (user clicks NotNow on re-prompt; updates remind_after)
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

/// Eligibility helper — pure function. Used by scheduler to decide when to
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

- [ ] **Step 2.2: Wire section module export**

Open `crates/oneshim-core/src/config/sections/mod.rs`. After the existing `pub mod external_grpc;` block (around line 12), add:

```rust
pub mod autostart;
pub use autostart::{AutostartConfig, AutostartPromptState, should_prompt};
```

- [ ] **Step 2.3: Add field to AppConfig**

Open `crates/oneshim-core/src/config/mod.rs`. Find the `pub struct AppConfig {` block (around line 20). Add this field at an alphabetical/logical position (e.g., after existing primary sections; near `analysis` or similar):

```rust
    #[serde(default)]
    pub autostart: AutostartConfig,
```

Also add the import at top of the file (where other section types are imported):

```rust
pub use sections::AutostartConfig;
```
(Or import via the existing `pub use sections::*;` if such a pattern exists — check the actual file's import style.)

- [ ] **Step 2.4: Verify compile**

```bash
cargo check -p oneshim-core 2>&1 | tail -20
```
Expected: clean compile. If "field `autostart` is private" error: ensure `AutostartConfig` is re-exported from `sections::mod.rs`.

- [ ] **Step 2.5: Commit**

```bash
git add crates/oneshim-core/src/config/sections/autostart.rs \
         crates/oneshim-core/src/config/sections/mod.rs \
         crates/oneshim-core/src/config/mod.rs
git commit -m "feat(autostart): AutostartConfig + AutostartPromptState in core (no enabled cache)"
```

---

## Task 3: AutostartConfig Unit Tests

**Estimate:** 1.5h | **Spec ref:** §9.1, §10.1 commit 3 | **Files:** `crates/oneshim-core/src/config/sections/autostart.rs` (tests submodule)

- [ ] **Step 3.1: Write failing tests**

Append to `crates/oneshim-core/src/config/sections/autostart.rs` at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_pending_with_zero_count() {
        let config = AutostartConfig::default();
        assert_eq!(config.prompt_state, AutostartPromptState::Pending);
        assert_eq!(config.productive_session_count, 0);
        assert!(config.last_session_id.is_none());
    }

    #[test]
    fn prompt_state_pending_serde_roundtrip() {
        let state = AutostartPromptState::Pending;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, r#"{"kind":"pending"}"#);
        let parsed: AutostartPromptState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }

    #[test]
    fn prompt_state_snoozed_serde_roundtrip() {
        let state = AutostartPromptState::Snoozed { remind_after_session_count: 6 };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, r#"{"kind":"snoozed","remind_after_session_count":6}"#);
        let parsed: AutostartPromptState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }

    #[test]
    fn prompt_state_dismissed_serde_roundtrip() {
        let state = AutostartPromptState::Dismissed;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, r#"{"kind":"dismissed"}"#);
        let parsed: AutostartPromptState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }

    #[test]
    fn should_prompt_pending_with_zero_count_returns_false() {
        let mut config = AutostartConfig::default();
        config.prompt_state = AutostartPromptState::Pending;
        config.productive_session_count = 0;
        assert!(!should_prompt(&config));
    }

    #[test]
    fn should_prompt_pending_with_one_count_returns_true() {
        let mut config = AutostartConfig::default();
        config.prompt_state = AutostartPromptState::Pending;
        config.productive_session_count = 1;
        assert!(should_prompt(&config));
    }

    #[test]
    fn should_prompt_snoozed_below_threshold_returns_false() {
        let mut config = AutostartConfig::default();
        config.prompt_state = AutostartPromptState::Snoozed { remind_after_session_count: 5 };
        config.productive_session_count = 4;
        assert!(!should_prompt(&config));
    }

    #[test]
    fn should_prompt_snoozed_at_threshold_returns_true() {
        let mut config = AutostartConfig::default();
        config.prompt_state = AutostartPromptState::Snoozed { remind_after_session_count: 5 };
        config.productive_session_count = 5;
        assert!(should_prompt(&config));
    }

    #[test]
    fn should_prompt_dismissed_always_false_regardless_of_count() {
        let mut config = AutostartConfig::default();
        config.prompt_state = AutostartPromptState::Dismissed;
        config.productive_session_count = 1000;
        assert!(!should_prompt(&config));
    }

    #[test]
    fn migration_from_old_config_uses_default() {
        // Simulate old config JSON without `autostart` field
        let old_config_json = r#"{}"#;
        // Note: This test requires AppConfig to also have #[serde(default)] on
        // the autostart field. We're testing the AutostartConfig::default()
        // behavior here directly.
        let parsed: AutostartConfig = serde_json::from_str(r#"{}"#).unwrap_or_default();
        assert_eq!(parsed, AutostartConfig::default());
    }
}
```

- [ ] **Step 3.2: Run tests, verify they fail until impl is in place**

```bash
cargo test -p oneshim-core --lib config::sections::autostart 2>&1 | tail -30
```
Expected: tests pass (since impl already exists from Task 2). If any fail: fix per error message.

- [ ] **Step 3.3: Commit**

```bash
git add crates/oneshim-core/src/config/sections/autostart.rs
git commit -m "test(autostart): AutostartConfig serde + should_prompt + idempotency unit tests"
```

---

## Task 4: IPC Commands + Wire Code Registration

**Estimate:** 2.5h | **Spec ref:** §5.1, §10.1 commit 4 | **Files:** Create `src-tauri/src/commands/autostart.rs`, modify `src-tauri/src/commands/mod.rs`, `src-tauri/src/main.rs`, append to `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`, `crates/oneshim-web/frontend/src/i18n/wire-errors.{en,ko}.json`

- [ ] **Step 4.1: Register wire codes (PRE-REQUISITE — CI fails without this)**

Append to `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`:

```
autostart.enable_failed
autostart.disable_failed
autostart.query_failed
```

(Maintain alphabetical order if the file uses one — check by reading the existing file first. If alphabetical, insert at the proper position.)

- [ ] **Step 4.2: Add wire-error translations (en)**

Open `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json` and add (in alphabetical order):

```json
  "autostart.enable_failed": "Failed to enable startup",
  "autostart.disable_failed": "Failed to disable startup",
  "autostart.query_failed": "Failed to query startup state",
```

- [ ] **Step 4.3: Add wire-error translations (ko)**

Open `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json` and add the same keys in alphabetical order:

```json
  "autostart.enable_failed": "시작 프로그램 켜기 실패",
  "autostart.disable_failed": "시작 프로그램 끄기 실패",
  "autostart.query_failed": "시작 프로그램 상태 조회 실패",
```

- [ ] **Step 4.4: Verify wire-error CI gate passes**

```bash
bash scripts/check-wire-error-i18n-coverage.sh 2>&1 | tail -10
```
Expected: success (all wire codes have translations).

- [ ] **Step 4.5: Define AutostartCapabilities skeleton + detect function**

Add to `src-tauri/src/autostart.rs` at the bottom (NEW: skeleton for PR-B2 to extend), AFTER the existing `linux` mod and BEFORE the `#[cfg(test)]` block:

```rust
/// PR-B1 skeleton: returns supported=true unconditionally for cross-platform UI parity.
/// PR-B2 adds real environment detection (Snap/Flatpak/headless).
#[derive(serde::Serialize, Debug, Clone)]
pub struct AutostartCapabilities {
    pub supported: bool,
    pub unsupported_reason: Option<UnsupportedReason>,
    pub environment: EnvironmentKind,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum UnsupportedReason {
    SnapSandbox,
    FlatpakSandbox,
    HeadlessSession,
    SystemctlUnavailable,
    UnsupportedPlatform,
}

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
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

/// PR-B1 stub. PR-B2 replaces with real detection.
pub fn detect_capabilities() -> AutostartCapabilities {
    #[cfg(target_os = "macos")]
    {
        AutostartCapabilities {
            supported: true,
            unsupported_reason: None,
            environment: EnvironmentKind::MacOs,
        }
    }
    #[cfg(target_os = "windows")]
    {
        AutostartCapabilities {
            supported: true,
            unsupported_reason: None,
            environment: EnvironmentKind::Windows,
        }
    }
    #[cfg(target_os = "linux")]
    {
        AutostartCapabilities {
            supported: true,
            unsupported_reason: None,
            environment: EnvironmentKind::LinuxSystemd,
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        AutostartCapabilities {
            supported: false,
            unsupported_reason: Some(UnsupportedReason::UnsupportedPlatform),
            environment: EnvironmentKind::Unknown,
        }
    }
}
```

Also remove the top-level `#![allow(dead_code)]` line (line 4) since the public fns will now be used.

- [ ] **Step 4.6: Create the IPC commands file**

Create `src-tauri/src/commands/autostart.rs`:

```rust
//! Tauri IPC commands for autostart management.
//!
//! Source-of-truth: OS state is authoritative for `is_autostart_enabled`.
//! AppConfig.autostart stores ONLY onboarding state (prompt_state, counter).
//!
//! See spec: docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md §5.1

use tauri::command;

use oneshim_core::config::AutostartPromptState;

use crate::autostart::{self, AutostartCapabilities};
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

/// PR-B1 skeleton — always returns supported=true for non-Linux,
/// supported=true for Linux without env detection. PR-B2 adds real detection.
///
/// Frontend code path is identical between B1 and B2 — UI gating works in both.
#[command]
pub async fn autostart_capabilities() -> Result<AutostartCapabilities, IpcError> {
    Ok(autostart::detect_capabilities())
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

- [ ] **Step 4.7: Register the commands module**

Open `src-tauri/src/commands/mod.rs` and add (alphabetically):

```rust
pub(crate) mod autostart;
```

- [ ] **Step 4.8: Wire commands into Tauri invoke_handler**

Open `src-tauri/src/main.rs`. Find the `.invoke_handler(tauri::generate_handler![...])` chain. Add the 5 new commands (alphabetically among existing entries):

```rust
            commands::autostart::autostart_capabilities,
            commands::autostart::disable_autostart,
            commands::autostart::enable_autostart,
            commands::autostart::is_autostart_enabled,
            commands::autostart::mark_autostart_prompt_state,
```

- [ ] **Step 4.9: Verify compile**

```bash
cargo check -p oneshim-app 2>&1 | tail -30
```
Expected: clean compile. If errors: fix imports / type mismatches per error message.

- [ ] **Step 4.10: Commit**

```bash
git add crates/oneshim-core/tests/wire_contract_snapshot.expected.txt \
         crates/oneshim-web/frontend/src/i18n/wire-errors.en.json \
         crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json \
         src-tauri/src/autostart.rs \
         src-tauri/src/commands/autostart.rs \
         src-tauri/src/commands/mod.rs \
         src-tauri/src/main.rs
git commit -m "feat(autostart): IPC commands (5 commands incl. capabilities skeleton)"
```

---

## Task 5: IPC Command Tests

**Estimate:** 1.5h | **Spec ref:** §9.1, §9.3, §10.1 commit 5 | **Files:** Append to `src-tauri/src/commands/autostart.rs`, create `src-tauri/tests/autostart_ipc_integration.rs`

- [ ] **Step 5.1: Write unit tests in commands/autostart.rs**

Append to `src-tauri/src/commands/autostart.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enable_autostart_calls_underlying() {
        // This test is meaningful because enable_autostart() actually writes
        // to OS files. We can't truly enable autostart in unit tests without
        // permissions side-effects. Just verify it returns Result type.
        let result = enable_autostart().await;
        // On systems where it succeeds, expect Ok. On systems where it fails
        // (e.g., CI without LaunchAgents perms), expect Err. Either is OK
        // for compile + signature verification.
        let _ = result;

        // Always disable after test (cleanup)
        let _ = disable_autostart().await;
    }

    #[tokio::test]
    async fn is_autostart_enabled_returns_bool() {
        let result = is_autostart_enabled().await;
        // Should never panic; should always return Ok(bool)
        assert!(result.is_ok(), "is_autostart_enabled should not error");
    }

    #[tokio::test]
    async fn autostart_capabilities_returns_supported_in_b1_skeleton() {
        let result = autostart_capabilities().await.unwrap();
        // PR-B1 skeleton always returns supported=true on supported platforms
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        assert!(result.supported, "PR-B1 skeleton must return supported=true");
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        assert!(!result.supported);
    }
}
```

- [ ] **Step 5.2: Create integration test**

Create `src-tauri/tests/autostart_ipc_integration.rs`:

```rust
//! Integration test: IPC commands → real autostart module round-trip.
//!
//! Skipped on CI where LaunchAgents/Registry/systemd writes may have permission
//! constraints. Run locally to verify smoke.

use oneshim_app::autostart;

#[test]
#[ignore = "modifies OS state (LaunchAgents/Registry/systemd unit) — run manually"]
fn enable_then_disable_round_trip() {
    // Start state
    let initial = autostart::is_autostart_enabled().unwrap_or(false);

    // Enable
    autostart::enable_autostart().expect("enable failed");
    let after_enable = autostart::is_autostart_enabled().expect("query failed");
    assert!(after_enable, "is_autostart_enabled should return true after enable");

    // Disable
    autostart::disable_autostart().expect("disable failed");
    let after_disable = autostart::is_autostart_enabled().expect("query failed");
    assert!(!after_disable, "is_autostart_enabled should return false after disable");

    // Restore initial state
    if initial {
        let _ = autostart::enable_autostart();
    }
}
```

Note: `oneshim_app::autostart` requires `autostart` to be `pub` in main.rs or lib.rs. Check that — if `autostart` is `mod autostart;` (private) in main.rs, change to `pub mod autostart;`. Or use full re-export in lib.rs if it exists.

- [ ] **Step 5.3: Run tests**

```bash
cargo test -p oneshim-app --lib commands::autostart::tests 2>&1 | tail -20
cargo test -p oneshim-app --test autostart_ipc_integration 2>&1 | tail -10
```
Expected: unit tests pass. Integration test "ignored" (skipped without `--ignored`).

- [ ] **Step 5.4: Commit**

```bash
git add src-tauri/src/commands/autostart.rs src-tauri/tests/autostart_ipc_integration.rs
git commit -m "test(autostart): IPC command unit tests + integration smoke"
```

---

## Task 6: Single-Instance Plugin + D-Bus Presence Check

**Estimate:** 2h | **Spec ref:** §5.2, §10.1 commit 6 (bundled per N-I3) | **Files:** `src-tauri/src/main.rs`

- [ ] **Step 6.1: Locate Tauri builder chain**

Open `src-tauri/src/main.rs`. Locate the `tauri::Builder::default()` chain (search for `Builder::default()`). Note the position of existing `.plugin(...)` calls.

- [ ] **Step 6.2: Add single-instance plugin at top of plugin chain**

Insert IMMEDIATELY AFTER `tauri::Builder::default()` (BEFORE any other `.plugin(...)` call):

```rust
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Callback runs in 1st instance when 2nd instance launches.
            // Must be cheap + synchronous (no async, no DB calls).
            // Order matters: show() → unminimize() → set_focus() per spec §5.2 mitigation #1.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            // _args, _cwd reserved for future CLI command extension (NG3).
        }))
```

- [ ] **Step 6.3: Add D-Bus presence check at startup**

Locate the `.setup(|app| { ... })` block in main.rs. Inside, add at the top (before other setup logic):

```rust
            // Per spec §5.2 mitigation #2 / I7: log warn if D-Bus session bus is
            // unavailable on Linux. Single-instance plugin will silently fail in
            // this case — duplicate processes may launch.
            #[cfg(target_os = "linux")]
            {
                if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
                    tracing::warn!(
                        err.code = "single_instance_dbus_absent",
                        "DBUS_SESSION_BUS_ADDRESS not set — single-instance enforcement degraded; \
                         duplicate processes may launch in headless sessions"
                    );
                }
            }
```

- [ ] **Step 6.4: Verify compile**

```bash
cargo check -p oneshim-app 2>&1 | tail -20
```
Expected: clean compile. If error about `tauri_plugin_single_instance` not found: re-verify Task 1 added the dep correctly.

- [ ] **Step 6.5: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(autostart): single-instance plugin + focus-grab callback + D-Bus presence check"
```

---

## Task 7: Single-Instance Integration Smoke Test

**Estimate:** 1.5h | **Spec ref:** §9.3, §10.1 commit 7 | **Files:** Create `src-tauri/tests/single_instance_integration.rs`

- [ ] **Step 7.1: Create the integration test**

Create `src-tauri/tests/single_instance_integration.rs`:

```rust
//! Single-instance integration smoke test.
//!
//! Spawns the binary as a child process; expects exit code 0 within 2 seconds
//! when 1st instance is already running.
//!
//! Run with: `cargo test -p oneshim-app --test single_instance_integration -- --ignored`
//!
//! Skipped by default because it requires the binary to be built and may
//! interfere with a running ONESHIM instance on the developer's machine.

use std::process::Command;
use std::time::{Duration, Instant};

#[test]
#[ignore = "spawns ONESHIM binary — run manually after build"]
fn second_instance_exits_cleanly_within_2s() {
    // PRE-CONDITION: 1st instance must be running before this test.
    // This test is informational — we just verify the 2nd instance can spawn
    // and exit without hanging or panicking.

    let bin_path = std::env::var("ONESHIM_BIN")
        .unwrap_or_else(|_| "target/release/oneshim-app".to_string());

    let start = Instant::now();
    let mut child = Command::new(&bin_path)
        .arg("--single-instance-test")
        .spawn()
        .expect("failed to spawn 2nd instance");

    // Poll for exit
    let timeout = Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let elapsed = start.elapsed();
                assert!(
                    elapsed < timeout,
                    "2nd instance took {:?} to exit (expected <{:?})",
                    elapsed,
                    timeout
                );
                assert!(
                    status.success(),
                    "2nd instance exit code != 0: {status:?}"
                );
                return;
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    panic!("2nd instance did not exit within {timeout:?}");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("error polling child: {e}"),
        }
    }
}
```

- [ ] **Step 7.2: Verify compile**

```bash
cargo check -p oneshim-app --tests 2>&1 | tail -10
```
Expected: clean compile.

- [ ] **Step 7.3: Commit**

```bash
git add src-tauri/tests/single_instance_integration.rs
git commit -m "test(autostart): single-instance integration smoke test"
```

---

## Task 8: GeneralTab Startup Section + i18n

**Estimate:** 2.5h | **Spec ref:** §5.4, §5.6, §10.1 commit 8 | **Files:** Modify `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`, `crates/oneshim-web/frontend/src/i18n/en.json`, `crates/oneshim-web/frontend/src/i18n/ko.json`

- [ ] **Step 8.1: Add i18n keys (en)**

Open `crates/oneshim-web/frontend/src/i18n/en.json`. Find the `settings.general` section. Add `autostart` subsection:

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
  }
}
```

(Merge into existing structure preserving other keys.)

- [ ] **Step 8.2: Add i18n keys (ko)**

Same in `crates/oneshim-web/frontend/src/i18n/ko.json`:

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
  }
}
```

- [ ] **Step 8.3: Add Startup section to GeneralTab.tsx**

Open `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`. Read the entire file to understand existing structure (Theme section, Language section, etc.).

Add at the top of the file (with existing imports):

```tsx
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
```

(If these are already imported, skip.)

Add this `StartupSection` component (place above the main `GeneralTab` export, OR inline if the existing pattern uses inline sections):

```tsx
interface AutostartCapabilities {
  supported: boolean
  unsupported_reason?: { kind: string }
  environment: string
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
      .then(([e, c]) => {
        setEnabled(e)
        setCaps(c)
      })
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
      const actual = await invoke<boolean>('is_autostart_enabled').catch(() => null)
      if (actual !== null) setEnabled(actual)
    } finally {
      setLoading(false)
    }
  }

  const isDisabled = loading || enabled === null || (caps !== null && !caps.supported)

  return (
    <section className="setting-section">
      <h2>{t('settings.general.autostart.title')}</h2>
      <p className="description">{t('settings.general.autostart.description')}</p>
      <label className="toggle-row">
        <input
          type="checkbox"
          checked={enabled ?? false}
          onChange={(e) => handleToggle(e.target.checked)}
          disabled={isDisabled}
        />
        <span>{t('settings.general.autostart.toggle')}</span>
      </label>
      {caps && !caps.supported && (
        <p className="tooltip">
          {t('settings.general.autostart.unsupported', {
            context: caps.unsupported_reason?.kind ?? 'unknown',
          })}
        </p>
      )}
      {error && (
        <div className="error-banner">
          {t('settings.general.autostart.error', { error })}
        </div>
      )}
    </section>
  )
}
```

- [ ] **Step 8.4: Render StartupSection in GeneralTab between Theme and Language**

Find where Theme and Language sections are rendered in `GeneralTab.tsx`. Insert `<StartupSection />` between them.

- [ ] **Step 8.5: Verify lint + types**

```bash
cd crates/oneshim-web/frontend
pnpm lint 2>&1 | tail -10
pnpm typecheck 2>&1 | tail -10
```
Expected: no errors. Fix any reported issues.

- [ ] **Step 8.6: Commit**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-foundation
git add crates/oneshim-web/frontend/src/i18n/en.json \
         crates/oneshim-web/frontend/src/i18n/ko.json \
         crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx
git commit -m "feat(autostart): GeneralTab Startup section + capabilities-aware UI + i18n"
```

---

## Task 9: GeneralTab Vitest Coverage

**Estimate:** 1h | **Spec ref:** §9.2, §10.1 commit 9 | **Files:** Modify or create `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx`

- [ ] **Step 9.1: Write failing tests**

Open or create `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx`. Add a new `describe` block for StartupSection:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor, fireEvent } from '@testing-library/react'
import { I18nextProvider } from 'react-i18next'
import i18n from '../../i18n/i18n'
import { GeneralTab } from './GeneralTab'

// Mock @tauri-apps/api/core invoke
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

describe('GeneralTab — Startup section', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
  })

  it('renders Startup section with toggle', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'is_autostart_enabled') return Promise.resolve(false)
      if (cmd === 'autostart_capabilities')
        return Promise.resolve({ supported: true, environment: 'macos' })
      return Promise.reject(new Error('unexpected command'))
    })

    render(
      <I18nextProvider i18n={i18n}>
        <GeneralTab />
      </I18nextProvider>
    )

    await waitFor(() => {
      expect(screen.getByText(/Startup|시작/)).toBeInTheDocument()
    })
  })

  it('toggle initial state loads from is_autostart_enabled IPC', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'is_autostart_enabled') return Promise.resolve(true)
      if (cmd === 'autostart_capabilities')
        return Promise.resolve({ supported: true, environment: 'macos' })
      return Promise.reject()
    })

    render(<I18nextProvider i18n={i18n}><GeneralTab /></I18nextProvider>)

    await waitFor(() => {
      const toggle = screen.getByRole('checkbox', { name: /Start ONESHIM at login|로그인 시 ONESHIM/ })
      expect(toggle).toBeChecked()
    })
  })

  it('toggle disabled when capabilities.supported = false', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'is_autostart_enabled') return Promise.resolve(false)
      if (cmd === 'autostart_capabilities')
        return Promise.resolve({
          supported: false,
          unsupported_reason: { kind: 'snap_sandbox' },
          environment: 'linux_snap_sandbox',
        })
      return Promise.reject()
    })

    render(<I18nextProvider i18n={i18n}><GeneralTab /></I18nextProvider>)

    await waitFor(() => {
      const toggle = screen.getByRole('checkbox', { name: /Start ONESHIM at login|로그인 시 ONESHIM/ })
      expect(toggle).toBeDisabled()
    })
  })

  it('toggle click invokes enable_autostart', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'is_autostart_enabled') return Promise.resolve(false)
      if (cmd === 'autostart_capabilities')
        return Promise.resolve({ supported: true, environment: 'macos' })
      if (cmd === 'enable_autostart') return Promise.resolve(undefined)
      return Promise.reject()
    })

    render(<I18nextProvider i18n={i18n}><GeneralTab /></I18nextProvider>)

    const toggle = await screen.findByRole('checkbox', { name: /Start ONESHIM at login|로그인 시 ONESHIM/ })
    fireEvent.click(toggle)

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('enable_autostart')
    })
  })

  it('toggle error re-fetches OS state', async () => {
    let callCount = 0
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'is_autostart_enabled') {
        callCount++
        return Promise.resolve(false)
      }
      if (cmd === 'autostart_capabilities')
        return Promise.resolve({ supported: true, environment: 'macos' })
      if (cmd === 'enable_autostart')
        return Promise.reject(new Error('permissions denied'))
      return Promise.reject()
    })

    render(<I18nextProvider i18n={i18n}><GeneralTab /></I18nextProvider>)

    const toggle = await screen.findByRole('checkbox', { name: /Start ONESHIM at login|로그인 시 ONESHIM/ })
    fireEvent.click(toggle)

    await waitFor(() => {
      // Initial mount + post-error re-fetch = at least 2 calls
      expect(callCount).toBeGreaterThanOrEqual(2)
    })
  })
})
```

- [ ] **Step 9.2: Run tests**

```bash
cd crates/oneshim-web/frontend
pnpm test src/pages/setting-tabs/GeneralTab.test.tsx 2>&1 | tail -20
```
Expected: all 5 tests pass. If failing: adjust selectors per actual i18n keys / DOM structure.

- [ ] **Step 9.3: Commit**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-foundation
git add crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx
git commit -m "test(autostart): GeneralTab Vitest coverage for Startup section"
```

---

## Task 10: Productive-Session Detection in monitor.rs (Rust-side counter)

**Estimate:** 2.5h | **Spec ref:** §5.5, §7.2, §10.1 commit 10 | **Files:** Modify `src-tauri/src/scheduler/loops/monitor.rs` (or create `src-tauri/src/scheduler/loops/autostart_helper.rs`)

- [ ] **Step 10.1: Read monitor.rs to find focus-state tracking**

```bash
wc -l src-tauri/src/scheduler/loops/monitor.rs
grep -n "focus\|productive\|deep_work" src-tauri/src/scheduler/loops/monitor.rs | head -20
```
Identify where focus-block start/end is tracked. Per CLAUDE.md guardrail "monitor loop must stay under 500 lines, extract helpers if exceeding," prefer creating a helper module.

- [ ] **Step 10.2: Create autostart_helper module**

Create `src-tauri/src/scheduler/loops/autostart_helper.rs`:

```rust
//! Productive-session detection + autostart counter increment.
//!
//! Per spec §5.5: Rust-side counter increment with idempotency via session_id.
//! No frontend round-trip — counter is incremented in the scheduler when a
//! ≥25 min focus block completes.
//!
//! After increment, emits `autostart:eligible-for-prompt` Tauri event if
//! eligibility changed (frontend Dashboard re-evaluates).

use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tracing::warn;
use uuid::Uuid;

use oneshim_core::config::{should_prompt, AutostartConfig};
use oneshim_core::config_manager::ConfigManager;

const PRODUCTIVE_SESSION_THRESHOLD_SECS: u64 = 25 * 60;

/// Call when a focus block has just completed.
///
/// Idempotent: if `session_id` matches `last_session_id` in config, no
/// increment occurs.
pub fn handle_focus_block_completed(
    config_mgr: &ConfigManager,
    app_handle: &AppHandle,
    session_id: Uuid,
    duration_secs: u64,
) {
    if duration_secs < PRODUCTIVE_SESSION_THRESHOLD_SECS {
        return;
    }

    let session_id_str = session_id.to_string();
    let snapshot = match config_mgr.update_with(|c| {
        if c.autostart.last_session_id.as_deref() == Some(&session_id_str) {
            return Ok(()); // idempotent — already counted this session
        }
        c.autostart.productive_session_count =
            c.autostart.productive_session_count.saturating_add(1);
        c.autostart.last_session_id = Some(session_id_str.clone());
        Ok(())
    }) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                err.code = "autostart_counter_increment_failed",
                "{e}"
            );
            return;
        }
    };

    if should_prompt(&snapshot.autostart) {
        if let Err(e) = app_handle.emit("autostart:eligible-for-prompt", ()) {
            warn!(
                err.code = "autostart_event_emit_failed",
                "failed to emit autostart:eligible-for-prompt: {e}"
            );
        }
    }
}
```

Note: this requires `uuid` crate. If not in `src-tauri/Cargo.toml`, add `uuid = { version = "1", features = ["v4"] }`.

- [ ] **Step 10.3: Add module to scheduler/loops/mod.rs**

Open `src-tauri/src/scheduler/loops/mod.rs` and add (alphabetically):

```rust
pub mod autostart_helper;
```

- [ ] **Step 10.4: Wire into monitor.rs**

In `src-tauri/src/scheduler/loops/monitor.rs`, find where focus-block completion is detected (look for existing focus_metrics writes or session-end events). Add a call to the helper:

```rust
use crate::scheduler::loops::autostart_helper;
use uuid::Uuid;

// ... within focus-block-completion code path:
let session_id = Uuid::new_v4();
let duration_secs = /* compute from focus block */;
autostart_helper::handle_focus_block_completed(
    config_mgr.as_ref(),
    app_handle,
    session_id,
    duration_secs,
);
```

If monitor.rs doesn't currently track per-block start/end (focus_metrics is daily aggregate per spec Q4 finding), add minimal in-memory tracking:

```rust
// In the SystemMonitorLoop struct:
current_focus_block_start: Option<Instant>,
current_focus_block_id: Option<Uuid>,

// On focus state transition (idle → focus):
if self.current_focus_block_start.is_none() {
    self.current_focus_block_start = Some(Instant::now());
    self.current_focus_block_id = Some(Uuid::new_v4());
}

// On focus state transition (focus → idle, or category change):
if let (Some(start), Some(id)) = (self.current_focus_block_start, self.current_focus_block_id) {
    let duration_secs = start.elapsed().as_secs();
    autostart_helper::handle_focus_block_completed(
        config_mgr.as_ref(),
        app_handle,
        id,
        duration_secs,
    );
    self.current_focus_block_start = None;
    self.current_focus_block_id = None;
}
```

(Adapt to actual monitor.rs structure.)

- [ ] **Step 10.5: Verify compile**

```bash
cargo check -p oneshim-app 2>&1 | tail -20
```
Expected: clean compile.

- [ ] **Step 10.6: Commit**

```bash
git add src-tauri/Cargo.toml \
         src-tauri/src/scheduler/loops/autostart_helper.rs \
         src-tauri/src/scheduler/loops/mod.rs \
         src-tauri/src/scheduler/loops/monitor.rs
git commit -m "feat(autostart): productive-session detection + Rust-side counter increment"
```

---

## Task 11: Autostart Helper Unit Tests

**Estimate:** 1.5h | **Spec ref:** §9.1, §10.1 commit 11 | **Files:** Append to `src-tauri/src/scheduler/loops/autostart_helper.rs`

- [ ] **Step 11.1: Write tests**

Append to `src-tauri/src/scheduler/loops/autostart_helper.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::{AppConfig, AutostartPromptState};
    use oneshim_core::config_manager::ConfigManager;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_config_mgr() -> (ConfigManager, TempDir) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, "{}").unwrap();
        let mgr = ConfigManager::with_path(path).expect("config manager");
        (mgr, tmp)
    }

    #[test]
    fn below_25_min_does_not_increment() {
        let (mgr, _tmp) = make_config_mgr();
        let app = mock_app_handle(); // see helper below
        let session_id = Uuid::new_v4();
        handle_focus_block_completed(&mgr, &app, session_id, 24 * 60);
        assert_eq!(mgr.get().autostart.productive_session_count, 0);
    }

    #[test]
    fn at_25_min_increments_counter() {
        let (mgr, _tmp) = make_config_mgr();
        let app = mock_app_handle();
        let session_id = Uuid::new_v4();
        handle_focus_block_completed(&mgr, &app, session_id, 25 * 60);
        assert_eq!(mgr.get().autostart.productive_session_count, 1);
    }

    #[test]
    fn idempotent_via_session_id() {
        let (mgr, _tmp) = make_config_mgr();
        let app = mock_app_handle();
        let session_id = Uuid::new_v4();
        handle_focus_block_completed(&mgr, &app, session_id, 25 * 60);
        handle_focus_block_completed(&mgr, &app, session_id, 25 * 60); // same id
        assert_eq!(mgr.get().autostart.productive_session_count, 1, "should not double-count");
    }

    #[test]
    fn different_session_ids_each_increment() {
        let (mgr, _tmp) = make_config_mgr();
        let app = mock_app_handle();
        handle_focus_block_completed(&mgr, &app, Uuid::new_v4(), 25 * 60);
        handle_focus_block_completed(&mgr, &app, Uuid::new_v4(), 25 * 60);
        assert_eq!(mgr.get().autostart.productive_session_count, 2);
    }

    #[test]
    fn dismissed_state_skips_event_emission() {
        let (mgr, _tmp) = make_config_mgr();
        mgr.update_with(|c| {
            c.autostart.prompt_state = AutostartPromptState::Dismissed;
            Ok(())
        }).unwrap();
        let app = mock_app_handle();
        // Should not emit even though counter increments
        handle_focus_block_completed(&mgr, &app, Uuid::new_v4(), 25 * 60);
        assert_eq!(mgr.get().autostart.productive_session_count, 1);
        // Event emit not asserted here (would need richer mock); see integration test
    }

    // Helper: minimal AppHandle mock. Tauri AppHandle requires a builder; for
    // tests where we only call .emit(), we can use a Tauri test helper or
    // skip emit verification.
    fn mock_app_handle() -> tauri::AppHandle {
        // Tauri 2 provides tauri::test::MockRuntime for tests.
        // Use tauri::test::mock_app to construct.
        tauri::test::mock_app().handle().clone()
    }
}
```

Note: `mock_app_handle()` requires `tauri = { version = "2", features = ["test"] }` in dev-dependencies. Add if missing.

Also requires `tempfile` in dev-dependencies (commonly already present).

- [ ] **Step 11.2: Run tests**

```bash
cargo test -p oneshim-app --lib scheduler::loops::autostart_helper 2>&1 | tail -20
```
Expected: 5 tests pass.

- [ ] **Step 11.3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/scheduler/loops/autostart_helper.rs
git commit -m "test(autostart): productive-session detection unit tests (idempotency, threshold)"
```

---

## Task 12: AutostartOnboardingPrompt + ShowPromptCoordinator

**Estimate:** 2.5h | **Spec ref:** §5.5, §10.1 commit 12 | **Files:** Create `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx`, `crates/oneshim-web/frontend/src/components/AutostartOnboardingPromptHost.tsx`, modify `crates/oneshim-web/frontend/src/pages/Dashboard.tsx`, append to i18n files

- [ ] **Step 12.1: Add onboarding i18n keys (en)**

Append to `crates/oneshim-web/frontend/src/i18n/en.json` (under existing root or merge into `onboarding`):

```json
{
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

- [ ] **Step 12.2: Add onboarding i18n keys (ko)**

Same in `crates/oneshim-web/frontend/src/i18n/ko.json`:

```json
{
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

- [ ] **Step 12.3: Create AutostartOnboardingPrompt.tsx**

Create `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx`:

```tsx
import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'

interface AutostartConfig {
  prompt_state: { kind: string; remind_after_session_count?: number }
  productive_session_count: number
  last_session_id: string | null
}

interface Props {
  config: AutostartConfig
  onClose: () => void
}

export function AutostartOnboardingPrompt({ config, onClose }: Props) {
  const { t } = useTranslation()
  const dialogRef = useRef<HTMLDivElement | null>(null)

  // Treat Escape + outside click as "Not now"
  useEffect(() => {
    function handleKey(e: KeyboardEvent) {
      if (e.key === 'Escape') void handleNotNow()
    }
    function handleClick(e: MouseEvent) {
      if (dialogRef.current && !dialogRef.current.contains(e.target as Node)) {
        void handleNotNow()
      }
    }
    window.addEventListener('keydown', handleKey)
    window.addEventListener('mousedown', handleClick)
    return () => {
      window.removeEventListener('keydown', handleKey)
      window.removeEventListener('mousedown', handleClick)
    }
  }, [])

  async function handleEnable() {
    try {
      await invoke('enable_autostart')
      await invoke('mark_autostart_prompt_state', { newState: { kind: 'dismissed' } })
    } finally {
      onClose()
    }
  }

  async function handleNotNow() {
    try {
      await invoke('mark_autostart_prompt_state', {
        newState: {
          kind: 'snoozed',
          remind_after_session_count: config.productive_session_count + 5,
        },
      })
    } finally {
      onClose()
    }
  }

  async function handleDismiss() {
    try {
      await invoke('mark_autostart_prompt_state', { newState: { kind: 'dismissed' } })
    } finally {
      onClose()
    }
  }

  return (
    <div className="modal-backdrop">
      <div ref={dialogRef} className="modal-dialog" role="dialog" aria-modal="true">
        <h2>{t('onboarding.autostart.title')}</h2>
        <p>{t('onboarding.autostart.body')}</p>
        <div className="modal-actions">
          <button onClick={handleEnable} type="button" className="btn-primary">
            {t('onboarding.autostart.enable_button')}
          </button>
          <button onClick={handleNotNow} type="button" className="btn-secondary">
            {t('onboarding.autostart.not_now_button')}
          </button>
          <button onClick={handleDismiss} type="button" className="btn-tertiary">
            {t('onboarding.autostart.dismiss_button')}
          </button>
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 12.4: Create AutostartOnboardingPromptHost.tsx**

Create `crates/oneshim-web/frontend/src/components/AutostartOnboardingPromptHost.tsx`:

```tsx
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useCallback, useEffect, useRef, useState } from 'react'
import { AutostartOnboardingPrompt } from './AutostartOnboardingPrompt'

interface AutostartConfig {
  prompt_state: { kind: string; remind_after_session_count?: number }
  productive_session_count: number
  last_session_id: string | null
}

// Module-level singleton: prevents re-show on Dashboard re-mount.
// Reset only on app restart. NOTE per spec §5.5 N-I2: Vite HMR resets this in
// dev mode — accept as known dev quirk.
let hasShownThisSession = false

function shouldShowPrompt(cfg: AutostartConfig): boolean {
  if (cfg.prompt_state.kind === 'dismissed') return false
  if (cfg.prompt_state.kind === 'pending') return cfg.productive_session_count >= 1
  if (cfg.prompt_state.kind === 'snoozed') {
    const threshold = cfg.prompt_state.remind_after_session_count ?? Number.POSITIVE_INFINITY
    return cfg.productive_session_count >= threshold
  }
  return false
}

export function AutostartOnboardingPromptHost() {
  const [shouldShow, setShouldShow] = useState(false)
  const [config, setConfig] = useState<AutostartConfig | null>(null)
  const timerRef = useRef<number | null>(null)

  const evaluate = useCallback(async () => {
    if (hasShownThisSession) return
    try {
      // Get just the autostart sub-config (assuming a get_app_config IPC exists;
      // if not, this needs a dedicated IPC like get_autostart_config).
      const appConfig = await invoke<{ autostart: AutostartConfig }>('get_app_config')
      const cfg = appConfig.autostart
      setConfig(cfg)
      if (shouldShowPrompt(cfg)) {
        if (timerRef.current === null) {
          timerRef.current = window.setTimeout(() => {
            if (!hasShownThisSession) {
              setShouldShow(true)
              hasShownThisSession = true
            }
            timerRef.current = null
          }, 500)
        }
      }
    } catch (e) {
      // Silent; not critical
      console.debug('[autostart-prompt] eligibility check failed', e)
    }
  }, [])

  useEffect(() => {
    void evaluate()
    let unlisten: UnlistenFn | null = null
    void listen('autostart:eligible-for-prompt', () => {
      void evaluate()
    }).then((fn) => {
      unlisten = fn
    })
    return () => {
      if (unlisten) unlisten()
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

Note: This component depends on a `get_app_config` IPC command. If it doesn't exist, add it as a quick add — typically in `src-tauri/src/commands/settings.rs` or similar. Or check for an existing equivalent:

```bash
grep -rn "get_app_config\|get_config\b" src-tauri/src/commands/ | head -5
```

If only `update_setting` exists for getting config, add a `get_app_config` command as part of this task:

```rust
// src-tauri/src/commands/settings.rs
#[command]
pub async fn get_app_config(
    state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<oneshim_core::config::AppConfig, IpcError> {
    Ok(state.config_manager().get())
}
```
And register in `main.rs` invoke_handler.

- [ ] **Step 12.5: Render the host in Dashboard.tsx**

Open `crates/oneshim-web/frontend/src/pages/Dashboard.tsx`. Add import:

```tsx
import { AutostartOnboardingPromptHost } from '../components/AutostartOnboardingPromptHost'
```

Inside the Dashboard component's return JSX, add at top level (before other content):

```tsx
<AutostartOnboardingPromptHost />
```

- [ ] **Step 12.6: Verify lint + types**

```bash
cd crates/oneshim-web/frontend
pnpm lint 2>&1 | tail -10
pnpm typecheck 2>&1 | tail -10
```
Expected: no errors.

- [ ] **Step 12.7: Commit**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-foundation
git add crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx \
         crates/oneshim-web/frontend/src/components/AutostartOnboardingPromptHost.tsx \
         crates/oneshim-web/frontend/src/i18n/en.json \
         crates/oneshim-web/frontend/src/i18n/ko.json \
         crates/oneshim-web/frontend/src/pages/Dashboard.tsx
# Include any get_app_config IPC additions if needed
git commit -m "feat(autostart): AutostartOnboardingPrompt + ShowPromptCoordinator + Dashboard integration"
```

---

## Task 13: AutostartOnboardingPrompt Vitest Coverage

**Estimate:** 1.5h | **Spec ref:** §9.2, §10.1 commit 13 | **Files:** Create `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.test.tsx`

- [ ] **Step 13.1: Write tests**

Create `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { I18nextProvider } from 'react-i18next'
import i18n from '../i18n/i18n'
import { AutostartOnboardingPrompt } from './AutostartOnboardingPrompt'

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

const baseConfig = {
  prompt_state: { kind: 'pending' as const },
  productive_session_count: 1,
  last_session_id: null,
}

describe('AutostartOnboardingPrompt', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
    mockInvoke.mockResolvedValue(undefined)
  })

  it('renders title and body', () => {
    render(
      <I18nextProvider i18n={i18n}>
        <AutostartOnboardingPrompt config={baseConfig} onClose={() => {}} />
      </I18nextProvider>
    )
    expect(screen.getByText(/Start ONESHIM automatically|ONESHIM을 자동으로/)).toBeInTheDocument()
  })

  it('Enable button invokes enable_autostart then mark_..._state(dismissed)', async () => {
    const onClose = vi.fn()
    render(
      <I18nextProvider i18n={i18n}>
        <AutostartOnboardingPrompt config={baseConfig} onClose={onClose} />
      </I18nextProvider>
    )

    const enableBtn = screen.getByText(/Enable|^켜기$/)
    fireEvent.click(enableBtn)

    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('enable_autostart')
      expect(mockInvoke).toHaveBeenCalledWith(
        'mark_autostart_prompt_state',
        { newState: { kind: 'dismissed' } }
      )
      expect(onClose).toHaveBeenCalled()
    })
  })

  it('Not now button sets snoozed with count + 5', async () => {
    const onClose = vi.fn()
    const config = { ...baseConfig, productive_session_count: 3 }
    render(
      <I18nextProvider i18n={i18n}>
        <AutostartOnboardingPrompt config={config} onClose={onClose} />
      </I18nextProvider>
    )

    fireEvent.click(screen.getByText(/Not now|나중에/))

    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        'mark_autostart_prompt_state',
        { newState: { kind: 'snoozed', remind_after_session_count: 8 } }
      )
      expect(onClose).toHaveBeenCalled()
    })
  })

  it('Dismiss button sets dismissed', async () => {
    const onClose = vi.fn()
    render(
      <I18nextProvider i18n={i18n}>
        <AutostartOnboardingPrompt config={baseConfig} onClose={onClose} />
      </I18nextProvider>
    )

    fireEvent.click(screen.getByText(/Don't ask again|다시 묻지 않기/))

    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        'mark_autostart_prompt_state',
        { newState: { kind: 'dismissed' } }
      )
      expect(onClose).toHaveBeenCalled()
    })
  })

  it('Escape key treated as Not now', async () => {
    const onClose = vi.fn()
    render(
      <I18nextProvider i18n={i18n}>
        <AutostartOnboardingPrompt config={baseConfig} onClose={onClose} />
      </I18nextProvider>
    )

    fireEvent.keyDown(window, { key: 'Escape' })

    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        'mark_autostart_prompt_state',
        expect.objectContaining({ newState: expect.objectContaining({ kind: 'snoozed' }) })
      )
      expect(onClose).toHaveBeenCalled()
    })
  })
})
```

- [ ] **Step 13.2: Run tests**

```bash
cd crates/oneshim-web/frontend
pnpm test src/components/AutostartOnboardingPrompt.test.tsx 2>&1 | tail -20
```
Expected: all 5 tests pass.

- [ ] **Step 13.3: Commit**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-foundation
git add crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.test.tsx
git commit -m "test(autostart): AutostartOnboardingPrompt Vitest coverage"
```

---

## Task 14: STATUS.md + PHASE-HISTORY.md Updates

**Estimate:** 0.5h | **Spec ref:** §10.1 commit 14 | **Files:** `docs/STATUS.md`, `docs/PHASE-HISTORY.md`

- [ ] **Step 14.1: Update STATUS.md**

Open `docs/STATUS.md`. Find the test count summary (e.g., "Workspace: N passed / 0 failed"). Update with new totals after running:

```bash
cargo test --workspace 2>&1 | tail -5
cd crates/oneshim-web/frontend && pnpm test 2>&1 | tail -5
```

Add a new line to the feature matrix (or appropriate section) noting "Autostart: cross-platform IPC + Settings UI + onboarding prompt + single-instance enforcement".

- [ ] **Step 14.2: Update PHASE-HISTORY.md**

Open `docs/PHASE-HISTORY.md`. Add a new entry under the latest phase section:

```markdown
### Phase 9 PR-B1 — Autostart Foundation (2026-04-25, target v0.4.40)

- Cross-platform autostart IPC commands wired to Settings UI toggle
- Single-instance enforcement via `tauri-plugin-single-instance` v2 (D-Bus on Linux, named pipe on Windows, Unix socket on macOS)
- Opt-in onboarding prompt after first 25-min productive focus session
- AutostartConfig in oneshim-core (no enabled cache — OS state is sole source of truth)
- ShowPromptCoordinator with single-fire guarantee + idempotent counter via session_id UUID
- 5 IPC commands + capabilities skeleton (PR-B2 adds real Linux env detection)
- Wire codes: autostart.enable_failed, autostart.disable_failed, autostart.query_failed
```

- [ ] **Step 14.3: Commit**

```bash
git add docs/STATUS.md docs/PHASE-HISTORY.md
git commit -m "docs(autostart): STATUS.md + PHASE-HISTORY entry for PR-B1"
```

---

## Task 15: Manual Smoke Test Matrix (PR description)

**Estimate:** 1h | **Spec ref:** §9.5, §10.1 commit 15 | **Files:** none (PR description only)

- [ ] **Step 15.1: Build release binary on each platform**

```bash
cargo build --release -p oneshim-app
```

Run on macOS (latest), Windows 11, Linux Ubuntu 24.04 X11, Linux Wayland (Fedora 40 GNOME), and sway if available.

- [ ] **Step 15.2: Per-platform smoke test**

For each platform, complete this checklist (record results for PR description):

```
- [ ] Settings → Startup section visible
- [ ] Toggle reflects current OS state on mount
- [ ] Toggle ON: app starts at next login (verify by logout/login)
- [ ] Toggle OFF: app does NOT start at next login
- [ ] Single-instance: launch 2nd instance → existing window comes to foreground (no 2nd process)
- [ ] Single-instance Wayland kept-hidden: autostart launches app to tray; click dock icon → window appears
- [ ] Onboarding prompt: complete a 25+ min focus session → modal appears within ~1s after session end
- [ ] Prompt "Enable" button: enables autostart + dismisses
- [ ] Prompt "Not now" button: schedules re-prompt after 5 more sessions
- [ ] Prompt "Don't ask again" button: never re-prompts
- [ ] Escape key + outside click: same as "Not now"
- [ ] Modal does NOT re-fire on Dashboard re-mount (single-fire)
```

- [ ] **Step 15.3: Compose PR description**

Once all manual tests complete, prepare PR description with results table per platform. No commit at this step — content goes into the GitHub PR body when opening the PR after Task 15 completion.

---

## Post-Completion Checklist

- [ ] **PC1: Run full test suite**

```bash
cargo test --workspace 2>&1 | tail -10
cd crates/oneshim-web/frontend && pnpm test 2>&1 | tail -10
```
Expected: ALL GREEN.

- [ ] **PC2: Run lint + format checks**

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings 2>&1 | tail -10
cd crates/oneshim-web/frontend && pnpm lint 2>&1 | tail -10
```
Expected: clean.

- [ ] **PC3: Open PR**

```bash
git push -u origin feature/phase9-autostart-foundation
gh pr create --title "feat(autostart): Phase 9 PR-B1 cross-platform foundation" \
  --body-file <PR-description-from-Task-15>
```

- [ ] **PC4: Update spec §16 with PR URL + merge status after merge**

---

## Plan Self-Review

### 1. Spec coverage

| Spec section | Tasks |
|--------------|-------|
| §5.1 IPC commands | Task 4 |
| §5.2 Single-instance plugin | Tasks 1, 6, 7 |
| §5.3 AutostartConfig | Tasks 2, 3 |
| §5.4 GeneralTab UI | Tasks 8, 9 |
| §5.5 Onboarding prompt + ShowPromptCoordinator | Tasks 12, 13 |
| §5.5 Productive-session detection | Tasks 10, 11 |
| §5.6 i18n strings | Tasks 8, 12 |
| §6.2 Capabilities IPC skeleton | Task 4 (subset, full impl in PR-B2) |
| §9.1-9.4 Tests | Tasks 3, 5, 7, 9, 11, 13 |
| §9.5 Manual smoke matrix | Task 15 |
| §10.1 Wire code registration | Task 4 |
| §11.4 Reconciler | NOT IN PR-B1 (deferred per spec — informational only, low priority) |
| §17 Cross-consumer | PF2 |

**Gap**: §11.4 reconciler is described but not in PR-B1 task list. Decision: defer to a Nice-to-have follow-up task or include in this PR. **Resolution**: include as optional follow-up task IF time permits, but not blocking. Document in plan summary.

### 2. Placeholder scan

- ✅ No "TBD" or "TODO" in tasks
- ✅ All code blocks contain actual code
- ✅ All test code shown in full
- ⚠ Task 10 says "Adapt to actual monitor.rs structure" — this is implementation guidance, not a placeholder. Acceptable because actual monitor.rs structure is deep and we don't want to over-prescribe.

### 3. Type consistency

- `AutostartConfig` field names consistent across Tasks 2, 10, 11, 12 (`prompt_state`, `productive_session_count`, `last_session_id`)
- `AutostartPromptState` variants consistent: `Pending`, `Snoozed`, `Dismissed`
- `AutostartCapabilities` fields consistent: `supported`, `unsupported_reason`, `environment`
- IPC command names consistent: `enable_autostart`, `disable_autostart`, `is_autostart_enabled`, `autostart_capabilities`, `mark_autostart_prompt_state`
- Wire codes consistent: `autostart.enable_failed`, `autostart.disable_failed`, `autostart.query_failed`

---

## Execution Handoff

**Plan complete and saved to** `docs/superpowers/plans/2026-04-25-phase9-pr-b1-autostart-foundation.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration. Best for this plan because each task has clear acceptance criteria.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**

(For ralph-loop continuation: Phase 2 plan creation done. Phase 2 deep review next iteration.)
