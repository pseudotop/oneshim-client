use tauri::command;

use crate::feature_capabilities::{
    build_feature_capability_snapshot,
    probe_provider_surface_endpoint as probe_provider_surface_endpoint_impl,
    FeatureCapabilitySnapshot, FeatureCapabilityState, ProviderEndpointProbeResult,
};
use crate::runtime_state::{AppState, SecretBackendCapabilities, SecretBackendState};

/// 자동화 상태 조회 — 사용자 설정 기반 반환
#[command]
pub async fn get_automation_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.config_manager.get().automation.enabled)
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
