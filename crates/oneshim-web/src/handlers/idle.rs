//! 유휴 기간 API 핸들러.

use axum::extract::{Query, State};
use axum::Json;
use oneshim_core::ports::storage::MetricsStorage;
use serde::Serialize;

use crate::error::ApiError;
use crate::AppState;

use super::TimeRangeQuery;

/// 유휴 기간 응답 DTO
#[derive(Debug, Serialize)]
pub struct IdlePeriodResponse {
    /// 시작 시각 (RFC3339)
    pub start_time: String,
    /// 종료 시각 (RFC3339, null이면 진행 중)
    pub end_time: Option<String>,
    /// 지속 시간 (초, null이면 진행 중)
    pub duration_secs: Option<u64>,
}

/// 유휴 기간 조회
///
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
