use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{command, AppHandle, Emitter, Manager, State};

use crate::runtime_state::AppState;

#[derive(Serialize)]
pub struct CaptureStatusResponse {
    pub paused: bool,
    pub indicator_visible: bool,
}

#[derive(Serialize)]
pub struct ConnectionStatusResponse {
    pub server: bool,
    pub llm: bool,
    pub cli: bool,
}

#[command]
pub async fn get_capture_status(
    state: State<'_, AppState>,
) -> Result<CaptureStatusResponse, String> {
    Ok(CaptureStatusResponse {
        paused: state.capture_paused.load(Ordering::Relaxed),
        indicator_visible: state.indicator_visible.load(Ordering::Relaxed),
    })
}

#[command]
pub async fn toggle_capture_pause(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CaptureStatusResponse, String> {
    let was_paused = state.capture_paused.fetch_xor(true, Ordering::Relaxed);
    let new_paused = !was_paused;
    let indicator_visible = state.indicator_visible.load(Ordering::Relaxed);

    let payload =
        serde_json::json!({ "paused": new_paused, "indicator_visible": indicator_visible });
    let _ = app.emit_to("magic-overlay", "overlay:capture-state-changed", &payload);
    let _ = app.emit_to("tracking-panel", "overlay:capture-state-changed", &payload);
    let _ = crate::tray::sync_tray_state(&app, new_paused, indicator_visible);

    Ok(CaptureStatusResponse {
        paused: new_paused,
        indicator_visible,
    })
}

#[command]
pub async fn set_indicator_visible(
    app: AppHandle,
    state: State<'_, AppState>,
    visible: bool,
) -> Result<(), String> {
    state.indicator_visible.store(visible, Ordering::Relaxed);
    let paused = state.capture_paused.load(Ordering::Relaxed);

    let payload = serde_json::json!({ "paused": paused, "indicator_visible": visible });
    let _ = app.emit_to("magic-overlay", "overlay:capture-state-changed", &payload);
    let _ = app.emit_to("tracking-panel", "overlay:capture-state-changed", &payload);

    if let Some(panel) = app.get_webview_window("tracking-panel") {
        if visible {
            let _ = panel.show();
        } else {
            let _ = panel.hide();
        }
    }
    let _ = crate::tray::sync_tray_state(&app, paused, visible);

    Ok(())
}

#[command]
pub async fn get_connection_status(
    state: State<'_, AppState>,
) -> Result<ConnectionStatusResponse, String> {
    Ok(ConnectionStatusResponse {
        server: state.connection.server_connected.load(Ordering::Relaxed),
        llm: state.connection.llm_connected.load(Ordering::Relaxed),
        cli: state.connection.cli_connected.load(Ordering::Relaxed),
    })
}

#[command]
pub async fn show_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        Ok(())
    } else {
        Err("main window not found".to_string())
    }
}

#[command]
pub async fn open_devtools(app: AppHandle, label: Option<String>) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        let target = label.as_deref().unwrap_or("main");
        if let Some(window) = app.get_webview_window(target) {
            window.open_devtools();
        }
    }
    Ok(())
}

#[command]
pub async fn save_panel_position(state: State<'_, AppState>, x: f64, y: f64) -> Result<(), String> {
    let pos = format!("{x},{y}");
    state.storage.set_meta("tracking_panel_position", &pos);
    Ok(())
}

#[command]
pub async fn get_panel_position(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.storage.get_meta("tracking_panel_position"))
}
