use axum::{extract::State, Json};
use oneshim_api_contracts::integration::IntegrationStatus;

use crate::AppState;

const INTEGRATION_STATUS_SCHEMA_VERSION: &str = "integration.status.v1";

pub async fn get_status(State(state): State<AppState>) -> Json<IntegrationStatus> {
    let external_access_enabled = state
        .config_manager
        .as_ref()
        .map(|config_manager| config_manager.get().web.allow_external)
        .unwrap_or(false);

    Json(IntegrationStatus {
        schema_version: INTEGRATION_STATUS_SCHEMA_VERSION.to_string(),
        external_access_enabled,
        automation_controller_configured: state.automation_controller.is_some(),
        ai_runtime_status: state.ai_runtime_status.clone(),
    })
}
