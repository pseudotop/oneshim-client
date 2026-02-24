
use axum::extract::{Query, State};
use axum::Json;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

use super::TimeRangeQuery;

#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub timestamp: String,
    pub cpu_usage: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub memory_percent: f64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub network_upload: u64,
    pub network_download: u64,
}

#[derive(Debug, Deserialize)]
pub struct HourlyQuery {
    pub hours: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct HourlyMetricsResponse {
    pub hour: String,
    pub cpu_avg: f64,
    pub cpu_max: f64,
    pub memory_avg: u64,
    pub memory_max: u64,
    pub sample_count: u64,
}

///
/// GET /api/metrics?from=&to=&limit=
pub async fn get_metrics(
    State(state): State<AppState>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<Vec<MetricsResponse>>, ApiError> {
    let from = params.from_datetime();
    let to = params.to_datetime();
    let limit = params.limit_or_default();

    let metrics = state.storage.get_metrics(from, to, limit).await?;

    let response: Vec<MetricsResponse> = metrics
        .into_iter()
        .map(|m| {
            let memory_percent = if m.memory_total > 0 {
                (m.memory_used as f64 / m.memory_total as f64) * 100.0
            } else {
                0.0
            };

            MetricsResponse {
                timestamp: m.timestamp.to_rfc3339(),
                cpu_usage: m.cpu_usage as f64,
                memory_used: m.memory_used,
                memory_total: m.memory_total,
                memory_percent,
                disk_used: m.disk_used,
                disk_total: m.disk_total,
                network_upload: m.network.as_ref().map(|n| n.upload_speed).unwrap_or(0),
                network_download: m.network.as_ref().map(|n| n.download_speed).unwrap_or(0),
            }
        })
        .collect();

    Ok(Json(response))
}

///
/// GET /api/metrics/hourly?hours=24
pub async fn get_hourly_metrics(
    State(state): State<AppState>,
    Query(params): Query<HourlyQuery>,
) -> Result<Json<Vec<HourlyMetricsResponse>>, ApiError> {
    let hours = params.hours.unwrap_or(24);
    let now = Utc::now();

    let from = (now - Duration::hours(hours as i64))
        .format("%Y-%m-%dT%H:00:00Z")
        .to_string();

    let rows = state
        .storage
        .list_hourly_metrics_since(&from)
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .into_iter()
        .map(|row| HourlyMetricsResponse {
            hour: row.hour,
            cpu_avg: row.cpu_avg,
            cpu_max: row.cpu_max,
            memory_avg: row.memory_avg,
            memory_max: row.memory_max,
            sample_count: row.sample_count,
        })
        .collect();

    Ok(Json(rows))
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
