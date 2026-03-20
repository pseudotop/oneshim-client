use async_trait::async_trait;
use std::collections::HashMap;

use crate::models::coaching::GoalProgressView;

/// Port for accessing coaching engine capabilities from adapter crates.
///
/// Implemented by `CoachingEngine` in `oneshim-analysis`.
/// Used by `oneshim-web` REST handlers (blocking variants) and Tauri IPC
/// commands (async variants).
///
/// # Why synchronous `_blocking` methods?
///
/// The Axum web handlers in `oneshim-web` run inside a `tokio::spawn_blocking`
/// context where `.await` is not available. The `_blocking` suffix signals that
/// these methods bridge the async `RwLock` internals via
/// `tokio::task::block_in_place` + `Handle::block_on`. This is a deliberate
/// deviation from the ADR-001 section 2 async-trait convention: the coaching engine's
/// internal state uses `tokio::sync::RwLock`, so a fully sync trait is not
/// possible, but the blocking wrappers let Axum handlers call through the port
/// without requiring an async context.
///
/// # Async methods
///
/// The async methods (`snooze_profile`, `record_feedback`, `all_goal_progress`,
/// `update_regime_goals`) are used by Tauri IPC commands which already run in
/// an async context and can `.await` directly.
#[async_trait]
pub trait CoachingPort: Send + Sync {
    /// Return goal progress for all configured regimes (blocking).
    fn all_goal_progress_blocking(&self) -> Vec<GoalProgressView>;

    /// Update the goal tracker's regime targets at runtime (blocking).
    fn update_regime_goals_blocking(&self, goals: &HashMap<String, u32>);

    /// Snooze a coaching profile for the given duration in seconds.
    ///
    /// While snoozed, `evaluate()` skips triggers for this profile.
    /// Called from the Tauri `dismiss_coaching_message` IPC command.
    async fn snooze_profile(&self, profile: &str, duration_secs: u64);

    /// Record explicit feedback (thumbs-up/down) for a coaching message.
    ///
    /// Called from the Tauri `submit_coaching_feedback` IPC command.
    async fn record_feedback(&self, message_id: &str, positive: bool);

    /// Return goal progress for all configured regimes (async).
    async fn all_goal_progress(&self) -> Vec<GoalProgressView>;

    /// Update the goal tracker's regime targets at runtime (async).
    async fn update_regime_goals(&self, goals: &HashMap<String, u32>);
}
