use crate::error::ApiError;
use crate::AppState;
use oneshim_api_contracts::data::{DeleteRangeRequest, DeleteResult};

pub fn delete_data_range(
    state: &AppState,
    request: &DeleteRangeRequest,
) -> Result<DeleteResult, ApiError> {
    if request.from.is_empty() || request.to.is_empty() {
        return Err(ApiError::BadRequest(
            "started 날짜와 ended 날짜가 필요합니다".to_string(),
        ));
    }

    let mut result = DeleteResult::empty();
    let delete_all = request.data_types.is_empty();
    let data_types = &request.data_types;

    if delete_all || data_types.iter().any(|t| t == "frames") {
        if let Some(ref frames_dir) = state.frames_dir {
            let paths = state
                .storage
                .list_frame_file_paths_in_range(&request.from, &request.to)
                .map_err(|e| ApiError::Internal(e.to_string()))?;

            for path in paths {
                let full_path = frames_dir.join(&path);
                let _ = std::fs::remove_file(full_path);
            }
        }
    }

    let deleted = state
        .storage
        .delete_data_in_range(
            &request.from,
            &request.to,
            delete_all || data_types.iter().any(|t| t == "events"),
            delete_all || data_types.iter().any(|t| t == "frames"),
            delete_all || data_types.iter().any(|t| t == "metrics"),
            delete_all || data_types.iter().any(|t| t == "processes"),
            delete_all || data_types.iter().any(|t| t == "idle"),
        )
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    result.events_deleted = deleted.events_deleted;
    result.frames_deleted = deleted.frames_deleted;
    result.metrics_deleted = deleted.metrics_deleted;
    result.process_snapshots_deleted = deleted.process_snapshots_deleted;
    result.idle_periods_deleted = deleted.idle_periods_deleted;
    result.message = format!("{} records were deleted", result.total());

    Ok(result)
}

pub fn delete_all_data(state: &AppState) -> Result<DeleteResult, ApiError> {
    let mut result = DeleteResult::empty();

    if let Some(ref frames_dir) = state.frames_dir {
        if frames_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(frames_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }

    let deleted = state
        .storage
        .delete_all_data()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    result.events_deleted = deleted.events_deleted;
    result.frames_deleted = deleted.frames_deleted;
    result.metrics_deleted = deleted.metrics_deleted;
    result.process_snapshots_deleted = deleted.process_snapshots_deleted;
    result.idle_periods_deleted = deleted.idle_periods_deleted;
    result.message = format!("All data was deleted ({} records)", result.total());

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    fn test_state() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        }
    }

    #[test]
    fn delete_data_range_requires_from_and_to() {
        let state = test_state();
        let request = DeleteRangeRequest {
            from: String::new(),
            to: String::new(),
            data_types: vec![],
        };

        let result = delete_data_range(&state, &request);
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }
}
