use axum::extract::{Query, State};
use axum::Json;
#[cfg(test)]
use oneshim_api_contracts::stats::{AppUsageEntry, HeatmapCell};
use oneshim_api_contracts::stats::{
    AppUsageResponse, DailySummaryResponse, DateQuery, HeatmapQuery, HeatmapResponse,
};

use crate::error::ApiError;
use crate::services::stats_service::StatsQueryService;
use crate::services::web_contexts::StorageWebContext;

/// GET /api/stats/summary?date=YYYY-MM-DD
pub async fn get_summary(
    State(context): State<StorageWebContext>,
    Query(params): Query<DateQuery>,
) -> Result<Json<DailySummaryResponse>, ApiError> {
    Ok(Json(
        StatsQueryService::new(context).get_summary(&params).await?,
    ))
}

/// GET /api/stats/apps?date=YYYY-MM-DD
pub async fn get_app_usage(
    State(context): State<StorageWebContext>,
    Query(params): Query<DateQuery>,
) -> Result<Json<AppUsageResponse>, ApiError> {
    Ok(Json(
        StatsQueryService::new(context)
            .get_app_usage(&params)
            .await?,
    ))
}

/// GET /api/stats/heatmap?days=7
pub async fn get_heatmap(
    State(context): State<StorageWebContext>,
    Query(params): Query<HeatmapQuery>,
) -> Result<Json<HeatmapResponse>, ApiError> {
    Ok(Json(
        StatsQueryService::new(context)
            .get_heatmap(params.days)
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_response_serializes() {
        let response = DailySummaryResponse {
            date: "2024-01-30".to_string(),
            total_active_secs: 28800,
            total_idle_secs: 3600,
            top_apps: vec![AppUsageEntry {
                name: "VS Code".to_string(),
                duration_secs: 14400,
                event_count: 2880,
                frame_count: 100,
            }],
            cpu_avg: 35.2,
            memory_avg_percent: 68.5,
            frames_captured: 1234,
            events_logged: 567,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("VS Code"));
    }

    #[test]
    fn heatmap_response_serializes() {
        let response = HeatmapResponse {
            from_date: "2024-01-23".to_string(),
            to_date: "2024-01-30".to_string(),
            cells: vec![
                HeatmapCell {
                    day: 0,
                    hour: 9,
                    value: 42,
                },
                HeatmapCell {
                    day: 0,
                    hour: 10,
                    value: 58,
                },
            ],
            max_value: 58,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"day\":0"));
        assert!(json.contains("\"hour\":9"));
        assert!(json.contains("\"value\":42"));
        assert!(json.contains("\"max_value\":58"));
    }
}
