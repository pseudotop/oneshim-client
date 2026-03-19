use axum::extract::{Query, State};
use axum::Json;
use chrono::Utc;
use tracing::debug;

use oneshim_core::models::daily_digest::DailyDigest;

use crate::error::ApiError;
use oneshim_api_contracts::dashboard::DashboardDayQuery;

use crate::handlers::dashboard::get_dashboard_day;
use crate::AppState;

/// GET /api/digests/daily?date=YYYY-MM-DD — returns a daily digest.
///
/// Uses the same cache-or-generate logic as the dashboard endpoint.
pub async fn get_daily_digest(
    state: State<AppState>,
    Query(params): Query<DashboardDayQuery>,
) -> Result<Json<DailyDigest>, ApiError> {
    debug!("GET /api/digests/daily date={:?}", params.date);
    get_dashboard_day(state, Query(params)).await
}

/// GET /api/digests/daily/today — shortcut for today's daily digest.
pub async fn get_daily_digest_today(state: State<AppState>) -> Result<Json<DailyDigest>, ApiError> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    debug!("GET /api/digests/daily/today ({})", today);
    get_dashboard_day(state, Query(DashboardDayQuery { date: Some(today) })).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_digest_query_defaults() {
        let json = r#"{}"#;
        let query: DashboardDayQuery = serde_json::from_str(json).unwrap();
        assert!(query.date.is_none());
    }

    #[test]
    fn daily_digest_query_with_date() {
        let json = r#"{"date": "2026-03-18"}"#;
        let query: DashboardDayQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.date.as_deref(), Some("2026-03-18"));
    }
}
