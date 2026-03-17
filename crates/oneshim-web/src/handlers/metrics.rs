use axum::extract::{Query, State};
use axum::Json;
#[cfg(test)]
use oneshim_api_contracts::metrics::MetricsResponse;
use oneshim_api_contracts::metrics::{HourlyMetricsResponse, HourlyQuery};

use crate::error::ApiError;
use crate::services::metrics_service::MetricsQueryService;
use crate::services::web_contexts::StorageWebContext;

use super::TimeRangeQuery;

/// GET /api/metrics?from=&to=&limit=
pub async fn get_metrics(
    State(context): State<StorageWebContext>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<Vec<oneshim_api_contracts::metrics::MetricsResponse>>, ApiError> {
    Ok(Json(
        MetricsQueryService::new(context)
            .get_metrics(&params)
            .await?,
    ))
}

/// GET /api/metrics/hourly?hours=24
pub async fn get_hourly_metrics(
    State(context): State<StorageWebContext>,
    Query(params): Query<HourlyQuery>,
) -> Result<Json<Vec<HourlyMetricsResponse>>, ApiError> {
    Ok(Json(
        MetricsQueryService::new(context).get_hourly_metrics(&params)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_response_memory_percent() {
        let response = MetricsResponse {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            cpu_usage: 50.0,
            memory_used: 8_000_000_000,
            memory_total: 16_000_000_000,
            memory_percent: 50.0,
            disk_used: 0,
            disk_total: 0,
            network_upload: 0,
            network_download: 0,
        };
        assert_eq!(response.memory_percent, 50.0);
    }
}
