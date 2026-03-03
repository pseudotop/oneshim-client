use serde::Serialize;
use sysinfo::System;
use tauri::command;

use crate::setup::AppState;
use oneshim_web::update_control::UpdateAction;

/// 시스템 메트릭 응답
#[derive(Serialize)]
pub struct MetricsResponse {
    pub agent_cpu: f32,
    pub agent_memory_mb: f64,
    pub system_cpu: f32,
    pub system_memory_used_mb: f64,
    pub system_memory_total_mb: f64,
}

/// 시스템 메트릭 수집 — 기존 LocalMonitor 로직
#[command]
pub async fn get_metrics(
    _state: tauri::State<'_, AppState>,
) -> Result<MetricsResponse, String> {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let cpu = sys.global_cpu_usage();
    let mem_used = sys.used_memory() as f64 / 1_048_576.0;
    let mem_total = sys.total_memory() as f64 / 1_048_576.0;

    // Agent 프로세스 자체 메트릭
    let pid = sysinfo::get_current_pid().ok();
    let (agent_cpu, agent_mem) = if let Some(pid) = pid {
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        if let Some(proc) = sys.process(pid) {
            (proc.cpu_usage(), proc.memory() as f64 / 1_048_576.0)
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    };

    Ok(MetricsResponse {
        agent_cpu,
        agent_memory_mb: agent_mem,
        system_cpu: cpu,
        system_memory_used_mb: mem_used,
        system_memory_total_mb: mem_total,
    })
}

/// 설정 조회
#[command]
pub async fn get_settings(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let config = state.config_manager.get();
    serde_json::to_value(&config).map_err(|e| e.to_string())
}

/// 설정 업데이트 — 전체 AppConfig JSON을 받아서 저장
#[command]
pub async fn update_setting(
    config_json: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let new_config: oneshim_core::config::AppConfig =
        serde_json::from_str(&config_json).map_err(|e| e.to_string())?;
    state
        .config_manager
        .update(new_config)
        .map_err(|e| e.to_string())
}

/// 업데이트 상태 조회
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
pub async fn approve_update(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .update_action_tx
        .send(UpdateAction::Approve)
        .map_err(|e| e.to_string())
}

/// 업데이트 연기
#[command]
pub async fn defer_update(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .update_action_tx
        .send(UpdateAction::Defer)
        .map_err(|e| e.to_string())
}

/// 자동화 상태 조회 — 컨트롤러 구성 여부 반환
#[command]
pub async fn get_automation_status(
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    Ok(state.automation_controller.is_some())
}
