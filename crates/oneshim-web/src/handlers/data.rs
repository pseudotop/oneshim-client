use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, services::data_service, AppState};

#[derive(Debug, Deserialize)]
pub struct DeleteRangeRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub data_types: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub success: bool,
    pub events_deleted: u64,
    pub frames_deleted: u64,
    pub metrics_deleted: u64,
    pub process_snapshots_deleted: u64,
    pub idle_periods_deleted: u64,
    pub message: String,
}

impl DeleteResult {
    pub(crate) fn empty() -> Self {
        Self {
            success: true,
            events_deleted: 0,
            frames_deleted: 0,
            metrics_deleted: 0,
            process_snapshots_deleted: 0,
            idle_periods_deleted: 0,
            message: String::new(),
        }
    }

    pub(crate) fn total(&self) -> u64 {
        self.events_deleted
            + self.frames_deleted
            + self.metrics_deleted
            + self.process_snapshots_deleted
            + self.idle_periods_deleted
    }
}

pub async fn delete_data_range(
    State(state): State<AppState>,
    Json(request): Json<DeleteRangeRequest>,
) -> Result<Json<DeleteResult>, ApiError> {
    Ok(Json(data_service::delete_data_range(&state, &request)?))
}

pub async fn delete_all_data(
    State(state): State<AppState>,
) -> Result<Json<DeleteResult>, ApiError> {
    Ok(Json(data_service::delete_all_data(&state)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_result_total() {
        let result = DeleteResult {
            success: true,
            events_deleted: 10,
            frames_deleted: 5,
            metrics_deleted: 100,
            process_snapshots_deleted: 20,
            idle_periods_deleted: 3,
            message: String::new(),
        };

        assert_eq!(result.total(), 138);
    }

    #[test]
    fn delete_result_empty() {
        let result = DeleteResult::empty();
        assert!(result.success);
        assert_eq!(result.total(), 0);
    }

    #[test]
    fn delete_range_request_deserializes() {
        let json =
            r#"{"from": "2024-01-01", "to": "2024-01-31", "data_types": ["events", "frames"]}"#;
        let request: DeleteRangeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.from, "2024-01-01");
        assert_eq!(request.to, "2024-01-31");
        assert_eq!(request.data_types.len(), 2);
    }

    #[test]
    fn delete_range_request_default_data_types() {
        let json = r#"{"from": "2024-01-01", "to": "2024-01-31"}"#;
        let request: DeleteRangeRequest = serde_json::from_str(json).unwrap();
        assert!(request.data_types.is_empty());
    }
}
