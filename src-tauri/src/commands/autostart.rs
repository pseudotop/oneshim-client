//! Tauri IPC commands for autostart management.
//!
//! Source-of-truth: OS state is authoritative for `is_autostart_enabled`.
//! AppConfig.autostart stores ONLY onboarding state (prompt_state, counter).

use tauri::command;

use oneshim_core::config::AutostartPromptState;
use oneshim_core::error_codes::AutostartCode;

use crate::autostart::{self, AutostartCapabilities};
use crate::ipc_error::IpcError;
use crate::runtime_state::ConfigRuntimeState;

#[command]
pub async fn enable_autostart() -> Result<(), IpcError> {
    autostart::enable_autostart().map_err(|e| {
        IpcError::new(
            AutostartCode::EnableFailed.as_str(),
            format!("autostart enable failed: {e}"),
        )
    })
}

#[command]
pub async fn disable_autostart() -> Result<(), IpcError> {
    autostart::disable_autostart().map_err(|e| {
        IpcError::new(
            AutostartCode::DisableFailed.as_str(),
            format!("autostart disable failed: {e}"),
        )
    })
}

#[command]
pub async fn is_autostart_enabled() -> Result<bool, IpcError> {
    autostart::is_autostart_enabled().map_err(|e| {
        IpcError::new(
            AutostartCode::QueryFailed.as_str(),
            format!("autostart query failed: {e}"),
        )
    })
}

#[command]
pub async fn autostart_capabilities() -> Result<AutostartCapabilities, IpcError> {
    Ok(autostart::detect_capabilities())
}

#[command]
pub async fn mark_autostart_prompt_state(
    new_state: AutostartPromptState,
    state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<(), IpcError> {
    state
        .config_manager()
        .update_with(|c| {
            c.autostart.prompt_state = new_state;
            Ok(())
        })
        .map(|_| ())
        .map_err(IpcError::from)
}

/// Get autostart-only config (smaller payload than full AppConfig).
#[command]
pub async fn get_autostart_config(
    state: tauri::State<'_, ConfigRuntimeState>,
) -> Result<oneshim_core::config::AutostartConfig, IpcError> {
    Ok(state.config_manager().get().autostart)
}
