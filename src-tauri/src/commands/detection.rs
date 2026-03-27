use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::State;
use tracing::{info, warn};

use oneshim_core::ports::element_finder::ElementFinder;

use crate::magic_overlay::MagicOverlayHandle;
use crate::runtime_state::AppState;

#[derive(Debug, Serialize)]
pub struct ToggleDetectionResponse {
    pub active: bool,
}

#[tauri::command]
pub async fn toggle_detection_overlay(
    active: bool,
    state: State<'_, AppState>,
) -> Result<ToggleDetectionResponse, String> {
    state.detection_active.store(active, Ordering::Relaxed);

    if active {
        info!("detection overlay activated — running scene analysis");
        spawn_detection_analysis_from_state(&state).await;
    } else {
        info!("detection overlay deactivated");
        if let Some(ref overlay) = state.magic_overlay {
            overlay.clear_detection_scene().await;
        }
    }

    Ok(ToggleDetectionResponse { active })
}

#[tauri::command]
pub async fn refresh_detection_overlay(state: State<'_, AppState>) -> Result<(), String> {
    if !state.detection_active.load(Ordering::Relaxed) {
        return Err("detection overlay is not active".to_string());
    }
    info!("detection overlay manual refresh");
    spawn_detection_analysis_from_state(&state).await;
    Ok(())
}

/// Helper for global shortcut handler (setup.rs) and IPC commands.
/// Clones only the Arc fields needed, not the entire AppState.
pub async fn spawn_detection_analysis_from_state(state: &AppState) {
    let finder: Arc<dyn ElementFinder> = match state.automation_controller.as_ref() {
        Some(controller) => match controller.scene_finder() {
            Some(finder) => finder.clone(),
            None => {
                warn!("scene_finder not configured — cannot run detection");
                return;
            }
        },
        None => {
            warn!("automation_controller not configured — cannot run detection");
            return;
        }
    };

    let overlay: MagicOverlayHandle = match state.magic_overlay.as_ref() {
        Some(overlay) => overlay.clone(),
        None => {
            warn!("magic_overlay not available — cannot show detection");
            return;
        }
    };

    tokio::spawn(async move {
        match finder.analyze_scene(None, None).await {
            Ok(scene) => {
                overlay.emit_detection_scene(&scene).await;
            }
            Err(e) => {
                warn!("detection scene analysis failed: {e}");
            }
        }
    });
}
