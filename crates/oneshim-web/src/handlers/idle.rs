use axum::extract::{Query, State};
use axum::Json;
use oneshim_api_contracts::idle::IdlePeriodResponse;

use crate::error::ApiError;
use crate::AppState;

use super::TimeRangeQuery;

/// GET /api/idle?from=&to=
pub async fn get_idle_periods(
    State(state): State<AppState>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<Vec<IdlePeriodResponse>>, ApiError> {
    let from = params.from_datetime();
    let to = params.to_datetime();

    let periods = state.storage.get_idle_periods(from, to).await?;

    let response: Vec<IdlePeriodResponse> = periods
        .into_iter()
        .map(|p| IdlePeriodResponse {
            start_time: p.start_time.to_rfc3339(),
            end_time: p.end_time.map(|dt| dt.to_rfc3339()),
            duration_secs: p.duration_secs,
        })
        .collect();

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_period_serializes() {
        let period = IdlePeriodResponse {
            start_time: "2024-01-01T12:00:00Z".to_string(),
            end_time: Some("2024-01-01T12:05:00Z".to_string()),
            duration_secs: Some(300),
        };
        let json = serde_json::to_string(&period).unwrap();
        assert!(json.contains("300"));
    }
}
