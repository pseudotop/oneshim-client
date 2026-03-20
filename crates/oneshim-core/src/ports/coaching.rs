use std::collections::HashMap;

use crate::models::coaching::GoalProgressView;

/// Port for accessing coaching engine capabilities from adapter crates.
///
/// Implemented by `CoachingEngine` in `oneshim-analysis`.
/// Used by `oneshim-web` REST handlers for goal progress queries.
pub trait CoachingPort: Send + Sync {
    /// Return goal progress for all configured regimes.
    fn all_goal_progress_blocking(&self) -> Vec<GoalProgressView>;

    /// Update the goal tracker's regime targets at runtime.
    fn update_regime_goals_blocking(&self, goals: &HashMap<String, u32>);
}
