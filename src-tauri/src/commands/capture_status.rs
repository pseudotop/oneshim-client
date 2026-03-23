use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{command, AppHandle, Emitter, Manager, State};

use crate::runtime_state::AppState;

#[derive(Serialize)]
pub struct CaptureStatusResponse {
    pub paused: bool,
    pub indicator_visible: bool,
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

    Ok(())
}
