//! 데이터 삭제 관련 서비스 로직.

use crate::error::ApiError;
use crate::handlers::data::{DeleteRangeRequest, DeleteResult};
use crate::AppState;

/// 날짜 범위 데이터 삭제.
pub fn delete_data_range(
    state: &AppState,
    request: &DeleteRangeRequest,
) -> Result<DeleteResult, ApiError> {
    if request.from.is_empty() || request.to.is_empty() {
        return Err(ApiError::BadRequest(
            "시작 날짜와 종료 날짜가 필요합니다".to_string(),
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
    result.message = format!("{}개의 레코드가 삭제되었습니다", result.total());

    Ok(result)
}

/// 전체 데이터 삭제.
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
    result.message = format!("모든 데이터가 삭제되었습니다 ({}개 레코드)", result.total());

    Ok(result)
}
