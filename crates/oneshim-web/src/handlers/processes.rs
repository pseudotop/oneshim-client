use axum::extract::{Query, State};
use axum::Json;
use oneshim_api_contracts::processes::{ProcessEntryResponse, ProcessSnapshotResponse};

use crate::error::ApiError;
use crate::AppState;

use super::TimeRangeQuery;

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
