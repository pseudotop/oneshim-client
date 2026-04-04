use serde::Serialize;
use tauri::command;

use crate::runtime_state::AutomationRuntimeState;

#[derive(Serialize)]
pub struct AutomationAvailabilityDto {
    pub available: bool,
}

#[derive(Serialize)]
pub struct PresetDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
}

#[derive(Serialize, Default)]
pub struct PresetRunResultDto {
    pub success: bool,
    pub message: String,
    pub steps_executed: usize,
    pub total_steps: usize,
}

/// Check if automation controller is available (distinct from system::get_automation_status).
#[command]
pub async fn check_automation_available(
    state: tauri::State<'_, AutomationRuntimeState>,
) -> Result<AutomationAvailabilityDto, String> {
    Ok(AutomationAvailabilityDto {
        available: state.controller().is_some(),
    })
}

/// List available workflow presets.
#[command]
pub async fn list_automation_presets(
    _state: tauri::State<'_, AutomationRuntimeState>,
) -> Result<Vec<PresetDto>, String> {
    // Presets are loaded from config at startup. For now return empty
    // until the controller exposes a preset listing method.
    // The web dashboard uses REST API which has full preset CRUD.
    Ok(vec![])
}

/// Run a workflow preset by ID.
#[command]
pub async fn run_automation_preset(
    state: tauri::State<'_, AutomationRuntimeState>,
    preset_id: String,
) -> Result<PresetRunResultDto, String> {
    let _controller = state.controller().ok_or("Automation not available")?;

    // Preset lookup requires access to config or storage.
    // For now, return an error indicating the preset wasn't found.
    // Full implementation will query presets from the web API layer.
    Err(format!(
        "Preset '{}' not found via IPC. Use the web dashboard for preset management.",
        preset_id
    ))
}

/// Execute an automation action from a natural language hint.
#[command]
pub async fn execute_automation_hint(
    state: tauri::State<'_, AutomationRuntimeState>,
    hint: String,
) -> Result<String, String> {
    let controller = state.controller().ok_or("Automation not available")?;

    let command_id = uuid::Uuid::new_v4().to_string();
    let session_id = uuid::Uuid::new_v4().to_string();

    let result = controller
        .execute_intent_hint(&command_id, &session_id, &hint)
        .await
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Analyze the current screen for automation targets.
#[command]
pub async fn analyze_automation_scene(
    state: tauri::State<'_, AutomationRuntimeState>,
) -> Result<String, String> {
    let controller = state.controller().ok_or("Automation not available")?;

    let scene = controller
        .analyze_scene(None, None)
        .await
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&scene).map_err(|e| e.to_string())
}
