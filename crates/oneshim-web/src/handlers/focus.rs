//!

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{Duration, Utc};
use oneshim_api_contracts::focus::{
    FocusMetricsDto, FocusMetricsResponse, InterruptionDto, LocalSuggestionDto,
    SuggestionFeedbackRequest, WorkSessionDto,
};
use tracing::debug;

use crate::error::ApiError;
use crate::handlers::TimeRangeQuery;
use crate::AppState;

pub async fn get_focus_metrics(
    State(state): State<AppState>,
) -> Result<Json<FocusMetricsResponse>, ApiError> {
    debug!("GET /api/focus/metrics");

    let storage = &state.storage;
    let today = Utc::now().format("%Y-%m-%d").to_string();

    let today_metrics = storage
        .get_or_create_focus_metrics(&today)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let history_raw = storage
        .get_recent_focus_metrics(7)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let today_dto = FocusMetricsDto {
        date: today.clone(),
        total_active_secs: today_metrics.total_active_secs,
        deep_work_secs: today_metrics.deep_work_secs,
        communication_secs: today_metrics.communication_secs,
        context_switches: today_metrics.context_switches,
        interruption_count: today_metrics.interruption_count,
        avg_focus_duration_secs: today_metrics.avg_focus_duration_secs,
        max_focus_duration_secs: today_metrics.max_focus_duration_secs,
        focus_score: today_metrics.focus_score,
    };

    let history: Vec<FocusMetricsDto> = history_raw
        .into_iter()
        .filter(|(date, _)| date != &today) // exclude today's aggregate
        .map(|(date, m)| FocusMetricsDto {
            date,
            total_active_secs: m.total_active_secs,
            deep_work_secs: m.deep_work_secs,
            communication_secs: m.communication_secs,
            context_switches: m.context_switches,
            interruption_count: m.interruption_count,
            avg_focus_duration_secs: m.avg_focus_duration_secs,
            max_focus_duration_secs: m.max_focus_duration_secs,
            focus_score: m.focus_score,
        })
        .collect();

    Ok(Json(FocusMetricsResponse {
        today: today_dto,
        history,
    }))
}

pub async fn get_work_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<TimeRangeQuery>,
) -> Result<Json<Vec<WorkSessionDto>>, ApiError> {
    debug!("GET /api/focus/sessions");

    let storage = &state.storage;
    let from = query.from_datetime();
    let to = query.to_datetime();
    let limit = query.limit_or_default();

    let sessions = storage
        .list_work_sessions(&from.to_rfc3339(), &to.to_rfc3339(), limit)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|row| WorkSessionDto {
            id: row.id,
            started_at: row.started_at,
            ended_at: row.ended_at,
            primary_app: row.primary_app,
            category: row.category,
            state: row.state,
            interruption_count: row.interruption_count,
            deep_work_secs: row.deep_work_secs,
            duration_secs: row.duration_secs,
        })
        .collect();

    Ok(Json(sessions))
}

pub async fn get_interruptions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<TimeRangeQuery>,
) -> Result<Json<Vec<InterruptionDto>>, ApiError> {
    debug!("GET /api/focus/interruptions");

    let storage = &state.storage;
    let from = query.from_datetime();
    let to = query.to_datetime();
    let limit = query.limit_or_default();

    let interruptions = storage
        .list_interruptions(&from.to_rfc3339(), &to.to_rfc3339(), limit)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|row| InterruptionDto {
            id: row.id,
            interrupted_at: row.interrupted_at,
            from_app: row.from_app,
            from_category: row.from_category,
            to_app: row.to_app,
            to_category: row.to_category,
            resumed_at: row.resumed_at,
            resumed_to_app: row.resumed_to_app,
            duration_secs: row.duration_secs,
        })
        .collect();

    Ok(Json(interruptions))
}

pub async fn get_suggestions(
    State(state): State<AppState>,
) -> Result<Json<Vec<LocalSuggestionDto>>, ApiError> {
    debug!("GET /api/focus/suggestions");

    let storage = &state.storage;

    let cutoff = (Utc::now() - Duration::hours(24)).to_rfc3339();

    let suggestions = storage
        .list_recent_local_suggestions(&cutoff, 50)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|row| LocalSuggestionDto {
            id: row.id,
            suggestion_type: row.suggestion_type,
            payload: row.payload,
            created_at: row.created_at,
            shown_at: row.shown_at,
            dismissed_at: row.dismissed_at,
            acted_at: row.acted_at,
        })
        .collect();

    Ok(Json(suggestions))
}

pub async fn submit_suggestion_feedback(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<SuggestionFeedbackRequest>,
) -> Result<StatusCode, ApiError> {
    debug!(
        "POST /api/focus/suggestions/{}/feedback action={}",
        id, request.action
    );

    let storage = &state.storage;

    match request.action.as_str() {
        "shown" => storage
            .mark_suggestion_shown(id)
            .map_err(|e| ApiError::Internal(e.to_string()))?,
        "dismissed" => storage
            .mark_suggestion_dismissed(id)
            .map_err(|e| ApiError::Internal(e.to_string()))?,
        "acted" => storage
            .mark_suggestion_acted(id)
            .map_err(|e| ApiError::Internal(e.to_string()))?,
        _ => {
            return Err(ApiError::BadRequest(format!(
                "유효하지 않은 액션: {}",
                request.action
            )))
        }
    }

    Ok(StatusCode::NO_CONTENT)
}
