use axum::extract::{Query, State};
use axum::Json;
use oneshim_api_contracts::coaching::{
    CoachingEventResponse, CoachingHistoryQuery, GoalProgressResponse, UpdateGoalsRequest,
};

use crate::error::ApiError;
use crate::AppState;

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
