use oneshim_api_contracts::data::{DeleteRangeRequest, DeleteResult};

use crate::error::ApiError;
use crate::services::web_contexts::StorageWebContext;

#[derive(Clone)]
pub struct DataCommandService {
    ctx: StorageWebContext,
}

impl DataCommandService {
    pub fn new(ctx: StorageWebContext) -> Self {
        Self { ctx }
    }

    pub fn delete_data_range(
        &self,
        request: &DeleteRangeRequest,
    ) -> Result<DeleteResult, ApiError> {
        if request.from.is_empty() || request.to.is_empty() {
            return Err(ApiError::BadRequest(
                "Both `from` and `to` dates are required.".to_string(),
            ));
        }

        let mut result = DeleteResult::empty();
        let delete_all = request.data_types.is_empty();
        let data_types = &request.data_types;

        if delete_all || data_types.iter().any(|item| item == "frames") {
            if let Some(ref frames_dir) = self.ctx.frames_dir {
                let paths = self
                    .ctx
                    .storage
                    .list_frame_file_paths_in_range(&request.from, &request.to)
                    .map_err(|error| ApiError::Internal(error.to_string()))?;

                for path in paths {
                    let full_path = frames_dir.join(&path);
                    let _ = std::fs::remove_file(full_path);
                }
            }
        }

        let deleted = self
            .ctx
            .storage
            .delete_data_in_range(
                &request.from,
                &request.to,
                delete_all || data_types.iter().any(|item| item == "events"),
                delete_all || data_types.iter().any(|item| item == "frames"),
                delete_all || data_types.iter().any(|item| item == "metrics"),
                delete_all || data_types.iter().any(|item| item == "processes"),
                delete_all || data_types.iter().any(|item| item == "idle"),
            )
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        result.events_deleted = deleted.events_deleted;
        result.frames_deleted = deleted.frames_deleted;
        result.metrics_deleted = deleted.metrics_deleted;
        result.process_snapshots_deleted = deleted.process_snapshots_deleted;
        result.idle_periods_deleted = deleted.idle_periods_deleted;
        result.message = format!("{} records were deleted", result.total());

        Ok(result)
    }

    pub fn delete_all_data(&self) -> Result<DeleteResult, ApiError> {
        // Phase 1: Atomic DB deletion (transaction — all-or-nothing)
        self.ctx
            .storage
            .delete_all_data()
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        // Phase 2: Best-effort frame file deletion (after DB commit)
        if let Some(ref frames_dir) = self.ctx.frames_dir {
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

        let mut result = DeleteResult::empty();
        result.message = "All data was deleted".to_string();

        Ok(result)
    }
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
            default_secret_backend_kind: oneshim_core::config::CredentialBackendKind::Unavailable,
            secret_store: None,
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            integration_runtime_status: None,
            integration_auth: None,
            integration_session: None,
            integration_outbox: None,
            integration_inbox: None,
            integration_inbox_store: None,
            integration_audit: None,
            integration_runtime_telemetry: None,
            update_control: None,
            vector_store: None,
            embedding_provider: None,
            text_search: None,
            override_store: None,
            recluster_requested: None,
            coaching_engine: None,
            session_manager: None,
            pomodoro: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    #[test]
    fn delete_data_range_requires_from_and_to() {
        let service = DataCommandService::new(StorageWebContext::from_state(&test_state()));
        let request = DeleteRangeRequest {
            from: String::new(),
            to: String::new(),
            data_types: vec![],
        };

        let result = service.delete_data_range(&request);
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }
}
