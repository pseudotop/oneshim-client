
use axum::{extract::State, Json};
use chrono::Utc;
use serde::Serialize;

use crate::handlers::automation::AuditEntryDto;
use crate::handlers::settings::{AppSettings, StorageStats};
use crate::{services::settings_service, AppState};

const SUPPORT_DIAGNOSTICS_SCHEMA_VERSION: &str = "support.diagnostics.v1";
const SUPPORT_AUDIT_SCHEMA_VERSION: &str = "automation.audit.v1";

#[derive(Debug, Serialize)]
pub struct DiagnosticsHealthDto {
    pub storage_ok: bool,
    pub storage_error: Option<String>,
    pub frames_dir_configured: bool,
    pub frames_dir_path: Option<String>,
    pub frames_dir_exists: Option<bool>,
    pub config_manager_configured: bool,
    pub automation_controller_configured: bool,
    pub update_control_configured: bool,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsBundleDto {
    pub schema_version: String,
    pub generated_at: String,
    pub health: DiagnosticsHealthDto,
    pub settings_snapshot: AppSettings,
    pub storage_stats: Option<StorageStats>,
    pub recent_audit_entries: Vec<AuditEntryDto>,
    pub recent_policy_events: Vec<AuditEntryDto>,
}

fn to_audit_entry_dto(entry: oneshim_automation::audit::AuditEntry) -> AuditEntryDto {
    AuditEntryDto {
        schema_version: SUPPORT_AUDIT_SCHEMA_VERSION.to_string(),
        entry_id: entry.entry_id,
        timestamp: entry.timestamp.to_rfc3339(),
        session_id: entry.session_id,
        command_id: entry.command_id,
        action_type: entry.action_type,
        status: format!("{:?}", entry.status),
        details: entry.details,
        elapsed_ms: entry.execution_time_ms,
    }
}

pub async fn get_diagnostics(State(state): State<AppState>) -> Json<DiagnosticsBundleDto> {
    let settings_snapshot = settings_service::get_settings(&state);
    let (storage_stats, storage_error) = match settings_service::get_storage_stats(&state) {
        Ok(stats) => (Some(stats), None),
        Err(err) => (None, Some(err.to_string())),
    };

    let frames_dir_path = state
        .frames_dir
        .as_ref()
        .map(|path| path.display().to_string());
    let frames_dir_exists = state.frames_dir.as_ref().map(|path| path.exists());

    let health = DiagnosticsHealthDto {
        storage_ok: storage_error.is_none(),
        storage_error,
        frames_dir_configured: state.frames_dir.is_some(),
        frames_dir_path,
        frames_dir_exists,
        config_manager_configured: state.config_manager.is_some(),
        automation_controller_configured: state.automation_controller.is_some(),
        update_control_configured: state.update_control.is_some(),
    };

    let (recent_audit_entries, recent_policy_events) =
        if let Some(logger) = state.audit_logger.as_ref() {
            let guard = logger.read().await;
            let recent_entries = guard.recent_entries(400);
            let audit_entries = recent_entries
                .iter()
                .take(50)
                .cloned()
                .map(to_audit_entry_dto)
                .collect();
            let policy_entries = recent_entries
                .into_iter()
                .filter(|entry| entry.action_type.starts_with("policy."))
                .take(50)
                .map(to_audit_entry_dto)
                .collect();
            (audit_entries, policy_entries)
        } else {
            (Vec::new(), Vec::new())
        };

    Json(DiagnosticsBundleDto {
        schema_version: SUPPORT_DIAGNOSTICS_SCHEMA_VERSION.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        health,
        settings_snapshot,
        storage_stats,
        recent_audit_entries,
        recent_policy_events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_health_serializes() {
        let dto = DiagnosticsHealthDto {
            storage_ok: true,
            storage_error: None,
            frames_dir_configured: true,
            frames_dir_path: Some("/tmp/frames".to_string()),
            frames_dir_exists: Some(true),
            config_manager_configured: true,
            automation_controller_configured: true,
            update_control_configured: false,
        };

        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("frames_dir_configured"));
        assert!(json.contains("automation_controller_configured"));
    }
}
