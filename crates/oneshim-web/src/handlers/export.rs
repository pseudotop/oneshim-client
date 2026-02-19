//! 데이터 내보내기 API 핸들러.

use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, AppState};

/// 내보내기 쿼리 파라미터
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// 시작 시간 (RFC3339)
    pub from: Option<String>,
    /// 종료 시간 (RFC3339)
    pub to: Option<String>,
    /// 내보내기 형식 (csv, json)
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "json".to_string()
}

/// 메트릭 내보내기 레코드
#[derive(Debug, Serialize)]
pub struct MetricExportRecord {
    pub timestamp: String,
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub memory_percent: f32,
    pub disk_used: u64,
    pub disk_total: u64,
    pub network_upload: u64,
    pub network_download: u64,
}

/// 이벤트 내보내기 레코드
#[derive(Debug, Serialize)]
pub struct EventExportRecord {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

/// 프레임 내보내기 레코드 (메타데이터만)
#[derive(Debug, Serialize)]
pub struct FrameExportRecord {
    pub id: i64,
    pub timestamp: String,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
    pub resolution: String,
    pub ocr_text: Option<String>,
}

/// GET /api/export/metrics - 메트릭 내보내기
pub async fn export_metrics(
    State(state): State<AppState>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let from = params
        .from
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(7));
    let to = params
        .to
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let mut stmt = conn
        .prepare(
            "SELECT timestamp, cpu_usage, memory_used, memory_total, disk_used, disk_total,
                    network_upload, network_download
             FROM system_metrics
             WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp ASC",
        )
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let records: Vec<MetricExportRecord> = stmt
        .query_map([from.to_rfc3339(), to.to_rfc3339()], |row| {
            let memory_used: u64 = row.get(2)?;
            let memory_total: u64 = row.get(3)?;
            let memory_percent = if memory_total > 0 {
                (memory_used as f32 / memory_total as f32) * 100.0
            } else {
                0.0
            };
            Ok(MetricExportRecord {
                timestamp: row.get(0)?,
                cpu_usage: row.get(1)?,
                memory_used,
                memory_total,
                memory_percent,
                disk_used: row.get(4)?,
                disk_total: row.get(5)?,
                network_upload: row.get(6)?,
                network_download: row.get(7)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    export_response(&records, &params.format, "metrics")
}

/// GET /api/export/events - 이벤트 내보내기
pub async fn export_events(
    State(state): State<AppState>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let from = params
        .from
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(7));
    let to = params
        .to
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let mut stmt = conn
        .prepare(
            "SELECT event_id, event_type, timestamp, app_name, window_title
             FROM events
             WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp ASC",
        )
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let records: Vec<EventExportRecord> = stmt
        .query_map([from.to_rfc3339(), to.to_rfc3339()], |row| {
            Ok(EventExportRecord {
                event_id: row.get(0)?,
                event_type: row.get(1)?,
                timestamp: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    export_response(&records, &params.format, "events")
}

/// GET /api/export/frames - 프레임 메타데이터 내보내기
pub async fn export_frames(
    State(state): State<AppState>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let from = params
        .from
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(7));
    let to = params
        .to
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, trigger_type, app_name, window_title, importance,
                    width, height, ocr_text
             FROM frames
             WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp ASC",
        )
        .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

    let records: Vec<FrameExportRecord> = stmt
        .query_map([from.to_rfc3339(), to.to_rfc3339()], |row| {
            let width: i32 = row.get(6)?;
            let height: i32 = row.get(7)?;
            Ok(FrameExportRecord {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                trigger_type: row.get(2)?,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
                importance: row.get(5)?,
                resolution: format!("{}x{}", width, height),
                ocr_text: row.get(8)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("쿼리 실행 실패: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

    export_response(&records, &params.format, "frames")
}

/// 내보내기 응답 생성 (JSON 또는 CSV)
fn export_response<T: Serialize>(
    records: &[T],
    format: &str,
    filename_prefix: &str,
) -> Result<Response, ApiError> {
    let now = Utc::now().format("%Y%m%d_%H%M%S");

    match format.to_lowercase().as_str() {
        "csv" => {
            let csv = records_to_csv(records)?;
            let filename = format!("{filename_prefix}_{now}.csv");
            Ok((
                [
                    (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                csv,
            )
                .into_response())
        }
        _ => {
            // JSON (기본값)
            let json = serde_json::to_string_pretty(records)
                .map_err(|e| ApiError::Internal(format!("JSON 직렬화 실패: {e}")))?;
            let filename = format!("{filename_prefix}_{now}.json");
            Ok((
                [
                    (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                json,
            )
                .into_response())
        }
    }
}

/// 레코드를 CSV 문자열로 변환
fn records_to_csv<T: Serialize>(records: &[T]) -> Result<String, ApiError> {
    if records.is_empty() {
        return Ok(String::new());
    }

    // JSON 값을 사용하여 CSV 생성
    let json_values: Vec<serde_json::Value> = records
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::Internal(format!("JSON 변환 실패: {e}")))?;

    // 헤더 추출
    let headers: Vec<String> = json_values
        .first()
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    let mut csv = headers.join(",") + "\n";

    // 데이터 행 추가
    for value in &json_values {
        if let Some(obj) = value.as_object() {
            let row: Vec<String> = headers
                .iter()
                .map(|h| {
                    obj.get(h)
                        .map(|v| {
                            match v {
                                serde_json::Value::String(s) => {
                                    // CSV 이스케이프 (쌍따옴표, 쉼표, 줄바꿈 포함 시)
                                    if s.contains(',') || s.contains('"') || s.contains('\n') {
                                        format!("\"{}\"", s.replace('"', "\"\""))
                                    } else {
                                        s.clone()
                                    }
                                }
                                serde_json::Value::Null => String::new(),
                                other => other.to_string(),
                            }
                        })
                        .unwrap_or_default()
                })
                .collect();
            csv.push_str(&row.join(","));
            csv.push('\n');
        }
    }

    Ok(csv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escapes_special_chars() {
        let records = vec![EventExportRecord {
            event_id: "1".to_string(),
            event_type: "context".to_string(),
            timestamp: "2024-01-30T10:00:00Z".to_string(),
            app_name: Some("VS Code".to_string()),
            window_title: Some("file.rs, modified".to_string()), // 쉼표 포함
        }];
        let csv = records_to_csv(&records).unwrap();
        assert!(csv.contains("\"file.rs, modified\"")); // 쌍따옴표로 감싸짐
    }

    #[test]
    fn empty_records_returns_empty_csv() {
        let records: Vec<MetricExportRecord> = vec![];
        let csv = records_to_csv(&records).unwrap();
        assert!(csv.is_empty());
    }

    #[test]
    fn default_format_is_json() {
        assert_eq!(default_format(), "json");
    }
}
