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
        AutostartPromptState::Snoozed {
            remind_after_session_count,
        } => config.productive_session_count >= *remind_after_session_count,
    }
}
