//! 프로세스 스냅샷 API 핸들러.

use axum::extract::{Query, State};
use axum::Json;
use oneshim_core::ports::storage::MetricsStorage;
use serde::Serialize;

use crate::error::ApiError;
use crate::AppState;

use super::TimeRangeQuery;

/// 프로세스 엔트리 응답 DTO
#[derive(Debug, Serialize)]
pub struct ProcessEntryResponse {
    /// 프로세스 ID
    pub pid: u32,
    /// 프로세스 이름
    pub name: String,
    /// CPU 사용률 (%)
    pub cpu_usage: f64,
    /// 메모리 사용량 (bytes)
    pub memory_bytes: u64,
}

/// 프로세스 스냅샷 응답 DTO
#[derive(Debug, Serialize)]
pub struct ProcessSnapshotResponse {
    /// 스냅샷 시각 (RFC3339)
    pub timestamp: String,
    /// 프로세스 목록
    pub processes: Vec<ProcessEntryResponse>,
}

/// 프로세스 스냅샷 조회
///
/// GET /api/processes?from=&to=&limit=
pub async fn get_processes(
    State(state): State<AppState>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<Vec<ProcessSnapshotResponse>>, ApiError> {
    let from = params.from_datetime();
    let to = params.to_datetime();
    let limit = params.limit_or_default();

    let snapshots = state.storage.get_process_snapshots(from, to, limit).await?;

    let response: Vec<ProcessSnapshotResponse> = snapshots
        .into_iter()
        .map(|s| ProcessSnapshotResponse {
            timestamp: s.timestamp.to_rfc3339(),
            processes: s
                .processes
                .into_iter()
                .map(|p| ProcessEntryResponse {
                    pid: p.pid,
                    name: p.name,
                    cpu_usage: p.cpu_usage as f64,
                    memory_bytes: p.memory_bytes,
                })
                .collect(),
        })
        .collect();

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_entry_serializes() {
        let entry = ProcessEntryResponse {
            pid: 1234,
            name: "code".to_string(),
            cpu_usage: 10.5,
            memory_bytes: 500_000_000,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("code"));
    }
}
