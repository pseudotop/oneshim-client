//! 시스템 메트릭 API 핸들러.

use axum::extract::{Query, State};
use axum::Json;
use chrono::{Duration, Utc};
use oneshim_core::ports::storage::MetricsStorage;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

use super::TimeRangeQuery;

/// 메트릭 응답 DTO
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    /// 타임스탬프 (RFC3339)
    pub timestamp: String,
    /// CPU 사용률 (%)
    pub cpu_usage: f64,
    /// 메모리 사용량 (bytes)
    pub memory_used: u64,
    /// 전체 메모리 (bytes)
    pub memory_total: u64,
    /// 메모리 사용률 (%)
    pub memory_percent: f64,
    /// 디스크 사용량 (bytes)
    pub disk_used: u64,
    /// 전체 디스크 (bytes)
    pub disk_total: u64,
    /// 네트워크 업로드 속도 (bytes/s)
    pub network_upload: u64,
    /// 네트워크 다운로드 속도 (bytes/s)
    pub network_download: u64,
}

/// 시간별 메트릭 쿼리
#[derive(Debug, Deserialize)]
pub struct HourlyQuery {
    /// 조회할 시간 수 (기본: 24)
    pub hours: Option<usize>,
}

/// 시간별 집계 메트릭 응답
#[derive(Debug, Serialize)]
pub struct HourlyMetricsResponse {
    /// 시각 (시간 단위, RFC3339)
    pub hour: String,
    /// 평균 CPU 사용률
    pub cpu_avg: f64,
    /// 최대 CPU 사용률
    pub cpu_max: f64,
    /// 평균 메모리 사용량 (bytes)
    pub memory_avg: u64,
    /// 최대 메모리 사용량 (bytes)
    pub memory_max: u64,
    /// 샘플 수
    pub sample_count: u64,
}

/// 시스템 메트릭 조회
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

/// 시간별 집계 메트릭 조회
///
/// GET /api/metrics/hourly?hours=24
pub async fn get_hourly_metrics(
    State(state): State<AppState>,
    Query(params): Query<HourlyQuery>,
) -> Result<Json<Vec<HourlyMetricsResponse>>, ApiError> {
    let hours = params.hours.unwrap_or(24);
    let now = Utc::now();

    // 시간별 집계 테이블에서 조회 (직접 쿼리)
    // MetricsStorage trait에는 시간별 조회 메서드가 없으므로 raw SQL 사용
    let conn = state.storage.conn_ref();
    let conn = conn
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let from = (now - Duration::hours(hours as i64))
        .format("%Y-%m-%dT%H:00:00Z")
        .to_string();

    let mut stmt = conn
        .prepare(
            "SELECT hour, cpu_avg, cpu_max, memory_avg, memory_max, sample_count
             FROM system_metrics_hourly
             WHERE hour >= ?1
             ORDER BY hour ASC",
        )
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let rows = stmt
        .query_map([&from], |row| {
            Ok(HourlyMetricsResponse {
                hour: row.get(0)?,
                cpu_avg: row.get(1)?,
                cpu_max: row.get(2)?,
                memory_avg: row.get::<_, i64>(3)? as u64,
                memory_max: row.get::<_, i64>(4)? as u64,
                sample_count: row.get::<_, i64>(5)? as u64,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
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
