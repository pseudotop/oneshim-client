//! Coaching API contracts.

use oneshim_core::models::coaching::{CoachingEventRow, GoalProgressView};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Query parameters for GET /api/coaching/history.
#[derive(Debug, Deserialize)]
pub struct CoachingHistoryQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Response DTO for a single coaching event.
#[derive(Debug, Serialize)]
pub struct CoachingEventResponse {
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

impl From<CoachingEventRow> for CoachingEventResponse {
    fn from(row: CoachingEventRow) -> Self {
        Self {
            event_id: row.event_id,
            trigger_type: row.trigger_type,
            profile_name: row.profile_name,
            regime_id: row.regime_id,
            message_template: row.message_template,
            personalized_message: row.personalized_message,
            shown_at: row.shown_at,
            dismissed_at: row.dismissed_at,
            dismiss_action: row.dismiss_action,
            feedback_type: row.feedback_type,
            feedback_score: row.feedback_score,
        }
    }
}

/// Response DTO for goal progress.
#[derive(Debug, Serialize)]
pub struct GoalProgressResponse {
    pub regime_label: String,
    pub current_minutes: u32,
    pub target_minutes: u32,
    pub percentage: u16,
    pub display_color: String,
}

impl From<GoalProgressView> for GoalProgressResponse {
    fn from(gp: GoalProgressView) -> Self {
        Self {
            regime_label: gp.regime_label,
            current_minutes: gp.current_minutes,
            target_minutes: gp.target_minutes,
            percentage: gp.percentage,
            display_color: gp.display_color,
        }
    }
}

/// Request body for PUT /api/coaching/goals.
#[derive(Debug, Deserialize)]
pub struct UpdateGoalsRequest {
    pub goals: HashMap<String, u32>,
}
