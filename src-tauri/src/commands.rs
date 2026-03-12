use serde::Serialize;
use std::sync::atomic::Ordering;
use sysinfo::System;
use tauri::command;

use crate::setup::AppState;
use oneshim_web::update_control::UpdateAction;

/// Recursively merge `patch` into `base`.
/// Objects are merged key-by-key; all other values are replaced.
fn deep_merge(base: &mut serde_json::Value, patch: serde_json::Value) {
    match (base.as_object_mut(), patch) {
        (Some(base_obj), serde_json::Value::Object(patch_obj)) => {
            for (k, v) in patch_obj {
                deep_merge(base_obj.entry(k).or_insert(serde_json::Value::Null), v);
            }
        }
        (_, patch) => *base = patch,
    }
}

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
pub async fn get_metrics(_state: tauri::State<'_, AppState>) -> Result<MetricsResponse, String> {
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

/// WebView에 노출되는 민감 필드를 마스킹하는 키 목록
const REDACTED_PATHS: &[(&str, &[&str])] = &[
    ("server", &["base_url", "api_key"]),
    ("ai_provider", &["ocr_api.api_key", "llm_api.api_key"]),
    (
        "tls",
        &["ca_cert_path", "client_cert_path", "client_key_path"],
    ),
    ("grpc", &["server_url"]),
];

/// 설정 조회 — 민감 필드 마스킹 후 반환
#[command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let config = state.config_manager.get();
    let mut v = serde_json::to_value(&config).map_err(|e| e.to_string())?;
    redact_sensitive_fields(&mut v);
    Ok(v)
}

fn redact_sensitive_fields(config: &mut serde_json::Value) {
    let redacted = serde_json::Value::String("[REDACTED]".to_string());
    for &(section, fields) in REDACTED_PATHS {
        if let Some(sec) = config.get_mut(section) {
            for &field in fields {
                // "ocr_api.api_key" 같은 중첩 경로 처리
                let parts: Vec<&str> = field.split('.').collect();
                let mut target = &mut *sec;
                let mut found = true;
                for (i, part) in parts.iter().enumerate() {
                    if i == parts.len() - 1 {
                        if let Some(obj) = target.as_object_mut() {
                            if obj.contains_key(*part) {
                                obj.insert((*part).to_string(), redacted.clone());
                            }
                        }
                    } else if let Some(next) = target.get_mut(*part) {
                        target = next;
                    } else {
                        found = false;
                        break;
                    }
                }
                let _ = found; // suppress unused warning
            }
        }
    }
}

/// WebView에서 수정 가능한 설정 필드 — 화이트리스트 모델
///
/// 허용: monitoring, capture, notification, web, schedule, telemetry, privacy, update, language, theme
/// 그 외 모든 키 거부 (sandbox, ai_provider, file_access, server 등)
#[command]
pub async fn update_setting(
    config_json: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let patch: serde_json::Value = serde_json::from_str(&config_json).map_err(|e| e.to_string())?;

    let patch_obj = patch.as_object().ok_or("expected JSON object")?;

    // Allowlist: only these top-level keys may be modified from the WebView
    const ALLOWED_KEYS: &[&str] = &[
        "monitoring",
        "capture",
        "notification",
        "web",
        "schedule",
        "telemetry",
        "privacy",
        "update",
        "language",
        "theme",
    ];

    for key in patch_obj.keys() {
        if !ALLOWED_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "modifying '{}' from the WebView is not permitted; allowed: {}",
                key,
                ALLOWED_KEYS.join(", "),
            ));
        }
    }

    // Deep-merge allowed keys into current config.
    // This preserves existing sub-keys that the patch does not mention,
    // preventing silent resets to struct defaults (e.g. privacy.pii_filter_level).
    let current = state.config_manager.get();
    let mut current_val = serde_json::to_value(&current).map_err(|e| e.to_string())?;

    if let (Some(base), Some(patch)) = (current_val.as_object_mut(), patch.as_object()) {
        for (k, v) in patch {
            deep_merge(
                base.entry(k.clone()).or_insert(serde_json::Value::Null),
                v.clone(),
            );
        }
    }

    let new_config: oneshim_core::config::AppConfig =
        serde_json::from_value(current_val).map_err(|e| e.to_string())?;
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

/// 웹 서버 포트 조회 — 프론트엔드 API base URL 결정용
#[command]
pub async fn get_web_port(state: tauri::State<'_, AppState>) -> Result<u16, String> {
    Ok(state.web_port.load(Ordering::Relaxed))
}
