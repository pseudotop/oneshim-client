use tauri::command;

use oneshim_api_contracts::integration::IntegrationDeviceAuthorizationCommandResult;
use oneshim_core::models::integration::default_integration_runtime_scopes;
use oneshim_core::ports::oauth::{OAuthConnectionStatus, OAuthFlowHandle, OAuthFlowStatus};

use crate::runtime_state::{IntegrationAuthState, OAuthCoordinatorState, OAuthState};

fn require_integration_auth(
    state: &IntegrationAuthState,
) -> Result<std::sync::Arc<dyn oneshim_core::ports::integration::IntegrationAuthPort>, String> {
    state
        .0
        .clone()
        .ok_or_else(|| "Integration auth is not configured for this runtime".to_string())
}

#[command]
pub async fn integration_auth_status(
    integration_auth: tauri::State<'_, IntegrationAuthState>,
) -> Result<oneshim_core::models::integration::IntegrationAuthStatus, String> {
    let port = require_integration_auth(&integration_auth)?;
    port.current_auth_status()
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())
}

#[command]
pub async fn integration_start_device_authorization(
    integration_auth: tauri::State<'_, IntegrationAuthState>,
) -> Result<IntegrationDeviceAuthorizationCommandResult, String> {
    let port = require_integration_auth(&integration_auth)?;
    let flow = port
        .start_device_authorization(&default_integration_runtime_scopes(), None)
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())?;
    let auth_status = port
        .current_auth_status()
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())?;
    Ok(IntegrationDeviceAuthorizationCommandResult {
        auth_status,
        flow: Some(flow),
    })
}

#[command]
pub async fn integration_poll_device_authorization(
    flow_id: String,
    integration_auth: tauri::State<'_, IntegrationAuthState>,
) -> Result<IntegrationDeviceAuthorizationCommandResult, String> {
    let port = require_integration_auth(&integration_auth)?;
    let auth_status = port
        .poll_device_authorization(&flow_id)
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())?;
    Ok(IntegrationDeviceAuthorizationCommandResult {
        flow: auth_status.pending_flow.clone(),
        auth_status,
    })
}

#[command]
pub async fn integration_cancel_device_authorization(
    flow_id: String,
    integration_auth: tauri::State<'_, IntegrationAuthState>,
) -> Result<(), String> {
    let port = require_integration_auth(&integration_auth)?;
    port.cancel_device_authorization(&flow_id)
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())
}

#[command]
pub async fn integration_reset_auth_state(
    integration_auth: tauri::State<'_, IntegrationAuthState>,
) -> Result<IntegrationDeviceAuthorizationCommandResult, String> {
    let port = require_integration_auth(&integration_auth)?;
    port.reset_auth_state()
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())?;
    let auth_status = port
        .current_auth_status()
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())?;
    Ok(IntegrationDeviceAuthorizationCommandResult {
        flow: auth_status.pending_flow.clone(),
        auth_status,
    })
}

// ── OAuth IPC commands ──────────────────────────────────────

fn require_oauth(
    state: &OAuthState,
) -> Result<std::sync::Arc<dyn oneshim_core::ports::oauth::OAuthPort>, String> {
    state.0.clone().ok_or_else(|| {
        "OAuth is not available (OS keychain unavailable or feature disabled)".into()
    })
}

/// OAuth 인증 플로우 시작 — auth_url을 프론트엔드에 반환
#[command]
pub async fn oauth_start_flow(
    provider_id: String,
    oauth: tauri::State<'_, OAuthState>,
) -> Result<OAuthFlowHandle, String> {
    let port = require_oauth(&oauth)?;
    port.start_flow(&provider_id)
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())
}

/// OAuth 플로우 상태 조회 — 프론트엔드 폴링용
///
/// When the flow completes successfully, the coordinator's backoff state
/// is reset so background refresh resumes immediately.
#[command]
pub async fn oauth_flow_status(
    flow_id: String,
    oauth: tauri::State<'_, OAuthState>,
    coordinator: tauri::State<'_, OAuthCoordinatorState>,
) -> Result<OAuthFlowStatus, String> {
    let port = require_oauth(&oauth)?;
    let status = port
        .flow_status(&flow_id)
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())?;

    // Reset coordinator backoff after successful re-authentication so the
    // background refresh loop resumes normal operation immediately.
    #[cfg(feature = "server")]
    if matches!(status, OAuthFlowStatus::Completed) {
        if let Some(ref coord) = coordinator.0 {
            coord.reset().await;
        }
    }
    let _ = &coordinator; // suppress unused-variable warning when server feature is off

    Ok(status)
}

/// OAuth 플로우 취소
#[command]
pub async fn oauth_cancel_flow(
    flow_id: String,
    oauth: tauri::State<'_, OAuthState>,
) -> Result<(), String> {
    let port = require_oauth(&oauth)?;
    port.cancel_flow(&flow_id)
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())
}

/// OAuth 연결 해제 — stored credentials 삭제
#[command]
pub async fn oauth_revoke(
    provider_id: String,
    oauth: tauri::State<'_, OAuthState>,
) -> Result<(), String> {
    let port = require_oauth(&oauth)?;
    port.revoke(&provider_id)
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())
}

/// OAuth 연결 상태 조회
#[command]
pub async fn oauth_connection_status(
    provider_id: String,
    oauth: tauri::State<'_, OAuthState>,
) -> Result<OAuthConnectionStatus, String> {
    let port = require_oauth(&oauth)?;
    port.connection_status(&provider_id)
        .await
        .map_err(|e: oneshim_core::error::CoreError| e.to_string())
}
