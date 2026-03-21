use tauri::command;

use crate::feature_capabilities::{
    build_feature_capability_snapshot,
    probe_provider_surface_endpoint as probe_provider_surface_endpoint_impl,
    FeatureCapabilitySnapshot, FeatureCapabilityState, ProviderEndpointProbeResult,
};
use crate::runtime_state::{AppState, SecretBackendCapabilities, SecretBackendState};
use oneshim_web::update_control::UpdateAction;

/// 업데이트 상태 조회
#[deprecated(
    since = "0.42.0",
    note = "Use REST endpoint /api/update-status instead"
)]
#[command]
pub async fn get_update_status(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    if let Some(ref control) = state.update_control {
        let status = control.state.read().await;
        serde_json::to_value(&*status).map_err(|e| e.to_string())
    } else {
        Ok(serde_json::json!({"phase": "Disabled", "message": "Updates disabled"}))
    }
}

/// 업데이트 승인
#[command]
pub async fn approve_update(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .update_action_tx
        .send(UpdateAction::Approve)
        .map_err(|e| e.to_string())
}

/// 업데이트 연기
#[command]
pub async fn defer_update(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .update_action_tx
        .send(UpdateAction::Defer)
        .map_err(|e| e.to_string())
}

/// 자동화 상태 조회 — 컨트롤러 구성 여부 반환
#[command]
pub async fn get_automation_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.automation_controller.is_some())
}

/// Secret backend capability snapshot for desktop runtime surfaces.
#[command]
pub async fn get_secret_backend_capabilities(
    state: tauri::State<'_, SecretBackendState>,
) -> Result<SecretBackendCapabilities, String> {
    Ok(state.0.clone())
}

/// Generic feature capability + maturity snapshot for desktop runtime surfaces.
#[command]
pub async fn get_feature_capabilities(
    state: tauri::State<'_, FeatureCapabilityState>,
) -> Result<FeatureCapabilitySnapshot, String> {
    let secret_backend = state.0.clone();
    Ok(build_feature_capability_snapshot(&secret_backend).await)
}

/// Probe the currently configured provider endpoint for a direct/self-hosted surface.
#[command]
pub async fn probe_provider_surface_endpoint(
    surface_id: String,
    endpoint_kind: String,
    endpoint: String,
) -> Result<ProviderEndpointProbeResult, String> {
    Ok(probe_provider_surface_endpoint_impl(&surface_id, &endpoint_kind, &endpoint).await)
}
