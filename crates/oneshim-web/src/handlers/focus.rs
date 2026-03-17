use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use oneshim_api_contracts::focus::{
    FocusMetricsResponse, InterruptionDto, LocalSuggestionDto, SuggestionFeedbackRequest,
    WorkSessionDto,
};
use tracing::debug;

use crate::error::ApiError;
use crate::handlers::TimeRangeQuery;
use crate::services::focus_service::{FocusCommandService, FocusQueryService};
use crate::services::web_contexts::StorageWebContext;

pub async fn get_focus_metrics(
    State(context): State<StorageWebContext>,
) -> Result<Json<FocusMetricsResponse>, ApiError> {
    debug!("GET /api/focus/metrics");
    Ok(Json(FocusQueryService::new(context).get_focus_metrics()?))
}

pub async fn get_work_sessions(
    State(context): State<StorageWebContext>,
    axum::extract::Query(query): axum::extract::Query<TimeRangeQuery>,
) -> Result<Json<Vec<WorkSessionDto>>, ApiError> {
    debug!("GET /api/focus/sessions");
    Ok(Json(
        FocusQueryService::new(context).get_work_sessions(&query)?,
    ))
}

pub async fn get_interruptions(
    State(context): State<StorageWebContext>,
    axum::extract::Query(query): axum::extract::Query<TimeRangeQuery>,
) -> Result<Json<Vec<InterruptionDto>>, ApiError> {
    debug!("GET /api/focus/interruptions");
    Ok(Json(
        FocusQueryService::new(context).get_interruptions(&query)?,
    ))
}

pub async fn get_suggestions(
    State(context): State<StorageWebContext>,
) -> Result<Json<Vec<LocalSuggestionDto>>, ApiError> {
    debug!("GET /api/focus/suggestions");
    Ok(Json(FocusQueryService::new(context).get_suggestions()?))
}

pub async fn submit_suggestion_feedback(
    State(context): State<StorageWebContext>,
    Path(id): Path<i64>,
    Json(request): Json<SuggestionFeedbackRequest>,
) -> Result<StatusCode, ApiError> {
    debug!(
        "POST /api/focus/suggestions/{}/feedback action={}",
        id, request.action
    );
    FocusCommandService::new(context).submit_suggestion_feedback(id, &request)?;
    Ok(StatusCode::NO_CONTENT)
}
