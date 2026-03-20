use axum::extract::{Query, State};
use axum::Json;
use oneshim_core::models::coaching::CoachingEventRow;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CoachingHistoryQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

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

#[derive(Debug, Serialize)]
pub struct GoalProgressResponse {
    pub regime_label: String,
    pub current_minutes: u32,
    pub target_minutes: u32,
    pub percentage: u16,
    pub display_color: String,
}

impl From<oneshim_core::models::coaching::GoalProgressView> for GoalProgressResponse {
    fn from(gp: oneshim_core::models::coaching::GoalProgressView) -> Self {
        Self {
            regime_label: gp.regime_label,
            current_minutes: gp.current_minutes,
            target_minutes: gp.target_minutes,
            percentage: gp.percentage,
            display_color: gp.display_color,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateGoalsRequest {
    pub goals: std::collections::HashMap<String, u32>,
}

/// GET /api/coaching/history
pub async fn get_coaching_history(
    State(state): State<AppState>,
    Query(params): Query<CoachingHistoryQuery>,
) -> Result<Json<Vec<CoachingEventResponse>>, ApiError> {
    let events = state
        .storage
        .query_coaching_events(params.limit.unwrap_or(50), params.offset.unwrap_or(0))
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(
        events
            .into_iter()
            .map(CoachingEventResponse::from)
            .collect(),
    ))
}

/// GET /api/coaching/goals
pub async fn get_goals(
    State(state): State<AppState>,
) -> Result<Json<Vec<GoalProgressResponse>>, ApiError> {
    if let Some(ref engine) = state.coaching_engine {
        let progress = engine.all_goal_progress_blocking();
        Ok(Json(
            progress
                .into_iter()
                .map(GoalProgressResponse::from)
                .collect(),
        ))
    } else {
        Ok(Json(vec![]))
    }
}

/// PUT /api/coaching/goals
pub async fn update_goals(
    State(state): State<AppState>,
    Json(body): Json<UpdateGoalsRequest>,
) -> Result<Json<()>, ApiError> {
    if let Some(ref engine) = state.coaching_engine {
        engine.update_regime_goals_blocking(&body.goals);
    }
    Ok(Json(()))
}
