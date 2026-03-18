use oneshim_api_contracts::integration::IntegrationDeviceAuthorizationCommandResult;
use oneshim_core::models::integration::default_integration_runtime_scopes;
use oneshim_core::ports::oauth::{OAuthConnectionStatus, OAuthFlowHandle, OAuthFlowStatus};
use serde::Serialize;
use std::sync::atomic::Ordering;
use sysinfo::System;
use tauri::command;

use crate::feature_capabilities::{
    build_feature_capability_snapshot,
    probe_provider_surface_endpoint as probe_provider_surface_endpoint_impl,
    FeatureCapabilitySnapshot, FeatureCapabilityState, ProviderEndpointProbeResult,
};
use crate::runtime_state::{
    AppState, IntegrationAuthState, OAuthCoordinatorState, OAuthState, SecretBackendCapabilities,
    SecretBackendState,
};
use oneshim_core::ports::web_storage::WebStorage;
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
    ("web", &["integration_auth_token"]),
    ("tls", &["enabled", "allow_self_signed"]),
    (
        "grpc",
        &[
            "grpc_endpoint",
            "tls_domain_name",
            "tls_ca_cert_path",
            "tls_client_cert_path",
            "tls_client_key_path",
        ],
    ),
];

const FORBIDDEN_ALLOWED_SUBPATHS: &[(&str, &[&str])] = &[("web", &["integration_auth_token"])];

/// WebView에서 수정 가능한 설정 키 화이트리스트.
/// update_setting + get_allowed_setting_keys에서 공유.
pub(crate) const ALLOWED_KEYS: &[&str] = &[
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
    "analysis",
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

    // Allowlist check — see module-level ALLOWED_KEYS

    for key in patch_obj.keys() {
        if !ALLOWED_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "modifying '{}' from the WebView is not permitted; allowed: {}",
                key,
                ALLOWED_KEYS.join(", "),
            ));
        }
    }

    reject_forbidden_allowed_subpaths(&patch)?;

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

fn reject_forbidden_allowed_subpaths(patch: &serde_json::Value) -> Result<(), String> {
    for &(section, fields) in FORBIDDEN_ALLOWED_SUBPATHS {
        let Some(section_value) = patch.get(section) else {
            continue;
        };

        for &field in fields {
            let mut target = section_value;
            let mut found = true;
            for part in field.split('.') {
                if let Some(next) = target.get(part) {
                    target = next;
                } else {
                    found = false;
                    break;
                }
            }

            if found {
                return Err(format!(
                    "modifying '{}.{}' from the WebView is not permitted",
                    section, field
                ));
            }
        }
    }

    Ok(())
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

/// 허용된 설정 키 목록 반환 — 프론트엔드 allowlist 검증 및 drift detection용
#[command]
pub async fn get_allowed_setting_keys() -> Vec<String> {
    ALLOWED_KEYS.iter().map(|s| s.to_string()).collect()
}

/// 웹 서버 포트 조회 — 프론트엔드 API base URL 결정용
#[command]
pub async fn get_web_port(state: tauri::State<'_, AppState>) -> Result<u16, String> {
    Ok(state.web_port.load(Ordering::Relaxed))
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

// ── Semantic search IPC commands ──────────────────────────────

/// Semantic search over embedded vectors.
/// Full semantic search requires the embedding pipeline to be configured.
/// Use the web API at /api/semantic-search for the full implementation.
#[command]
pub async fn semantic_search(
    _state: tauri::State<'_, AppState>,
    _query: String,
    _limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    // The Tauri semantic search command delegates to the web API endpoint.
    // The web API has access to the embedding provider and vector store via AppState.
    Err(
        "Semantic search requires embedding pipeline — use the web API at /api/semantic-search"
            .to_string(),
    )
}

/// Get weekly digest for the given week offset (0 = current, -1 = last week).
#[command]
pub async fn get_weekly_digest(
    state: tauri::State<'_, AppState>,
    week_offset: Option<i32>,
) -> Result<serde_json::Value, String> {
    let offset = week_offset.unwrap_or(0);
    let limit = if offset == 0 {
        1
    } else {
        (offset.unsigned_abs() as usize) + 1
    };

    let digests = state
        .storage
        .list_weekly_digests(limit)
        .map_err(|e| e.to_string())?;

    let target_idx = offset.unsigned_abs() as usize;
    if let Some(digest) = digests.into_iter().nth(target_idx) {
        serde_json::to_value(&digest).map_err(|e| e.to_string())
    } else {
        Ok(serde_json::json!(null))
    }
}

// ── Analysis config IPC commands ───────────────────────────────

/// 분석 설정 조회
///
/// AnalysisConfig contains no sensitive fields (no API keys, credentials).
/// If sensitive fields are added in the future, apply redact_sensitive_fields().
#[command]
pub async fn get_analysis_config(
    state: tauri::State<'_, AppState>,
) -> Result<oneshim_core::config::AnalysisConfig, String> {
    let config = state.config_manager.get();
    Ok(config.analysis.clone())
}

/// Validate an AnalysisConfig, returning Err(String) on constraint violation.
pub(crate) fn validate_analysis_config(
    config: &oneshim_core::config::AnalysisConfig,
) -> Result<(), String> {
    if config.min_confidence < 0.0 || config.min_confidence > 1.0 {
        return Err("min_confidence must be between 0.0 and 1.0".to_string());
    }
    if config.max_suggestions == 0 {
        return Err("max_suggestions must be at least 1".to_string());
    }
    if config.throttle_secs == 0 {
        return Err("throttle_secs must be at least 1".to_string());
    }
    if config.interval_secs < 10 {
        return Err("interval_secs must be at least 10".to_string());
    }
    if config.full_interval_secs < config.interval_secs {
        return Err("full_interval_secs must be >= interval_secs".to_string());
    }
    Ok(())
}

/// 분석 설정 부분 업데이트 (patch merge)
///
/// Uses `update_with` to hold the write lock for the entire read-modify-write
/// cycle, preventing TOCTOU races between concurrent callers.
#[command]
pub async fn update_analysis_config(
    state: tauri::State<'_, AppState>,
    patch: serde_json::Value,
) -> Result<oneshim_core::config::AnalysisConfig, String> {
    let updated = state
        .config_manager
        .update_with(|config| {
            // Deep-merge patch into current analysis section
            let mut analysis_json =
                serde_json::to_value(&config.analysis).map_err(|e| e.to_string())?;
            deep_merge(&mut analysis_json, patch.clone());

            // Deserialize back and validate
            let new_analysis: oneshim_core::config::AnalysisConfig =
                serde_json::from_value(analysis_json)
                    .map_err(|e| format!("Invalid config: {e}"))?;
            validate_analysis_config(&new_analysis)?;

            config.analysis = new_analysis;
            Ok(())
        })
        .map_err(|e| e.to_string())?;

    Ok(updated.analysis)
}

/// 분석 파이프라인 상태 응답
#[derive(Serialize)]
pub struct AnalysisStatusResponse {
    pub enabled: bool,
    pub provider_configured: bool,
    pub provider_name: Option<String>,
    pub throttle_secs: u64,
    pub interval_secs: u64,
    pub full_interval_secs: u64,
    pub min_confidence: f64,
    pub max_suggestions: usize,
}

/// 분석 파이프라인 상태 조회 (enabled, provider 설정 여부 등)
#[command]
pub async fn get_analysis_status(
    state: tauri::State<'_, AppState>,
) -> Result<AnalysisStatusResponse, String> {
    let config = state.config_manager.get();
    let provider_name = config
        .ai_provider
        .llm_api
        .as_ref()
        .map(|api| format!("{:?}", api.provider_type));
    Ok(AnalysisStatusResponse {
        enabled: config.analysis.enabled,
        provider_configured: config.ai_provider.llm_api.is_some(),
        provider_name,
        throttle_secs: config.analysis.throttle_secs,
        interval_secs: config.analysis.interval_secs,
        full_interval_secs: config.analysis.full_interval_secs,
        min_confidence: config.analysis.min_confidence,
        max_suggestions: config.analysis.max_suggestions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── deep_merge ────────────────────────────────────────────

    #[test]
    fn deep_merge_replaces_flat_value() {
        let mut base = json!({"a": 1});
        deep_merge(&mut base, json!({"a": 2}));
        assert_eq!(base, json!({"a": 2}));
    }

    #[test]
    fn deep_merge_adds_new_key() {
        let mut base = json!({"a": 1});
        deep_merge(&mut base, json!({"b": 2}));
        assert_eq!(base, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn deep_merge_recurses_into_objects() {
        let mut base = json!({"a": {"x": 1, "y": 2}});
        deep_merge(&mut base, json!({"a": {"y": 99, "z": 3}}));
        assert_eq!(base, json!({"a": {"x": 1, "y": 99, "z": 3}}));
    }

    #[test]
    fn deep_merge_replaces_non_object_with_object() {
        let mut base = json!({"a": "string"});
        deep_merge(&mut base, json!({"a": {"nested": true}}));
        assert_eq!(base, json!({"a": {"nested": true}}));
    }

    #[test]
    fn deep_merge_replaces_object_with_non_object() {
        let mut base = json!({"a": {"nested": true}});
        deep_merge(&mut base, json!({"a": "flat"}));
        assert_eq!(base, json!({"a": "flat"}));
    }

    // ── redact_sensitive_fields ───────────────────────────────

    #[test]
    fn redact_masks_server_keys() {
        let mut config = json!({
            "server": {"base_url": "http://real.com", "api_key": "secret123", "timeout": 30}
        });
        redact_sensitive_fields(&mut config);
        assert_eq!(config["server"]["base_url"], "[REDACTED]");
        assert_eq!(config["server"]["api_key"], "[REDACTED]");
        assert_eq!(config["server"]["timeout"], 30);
    }

    #[test]
    fn redact_masks_nested_ai_provider_keys() {
        let mut config = json!({
            "ai_provider": {
                "ocr_api": {"api_key": "ocr-secret", "model": "gpt4"},
                "llm_api": {"api_key": "llm-secret", "model": "claude"}
            }
        });
        redact_sensitive_fields(&mut config);
        assert_eq!(config["ai_provider"]["ocr_api"]["api_key"], "[REDACTED]");
        assert_eq!(config["ai_provider"]["ocr_api"]["model"], "gpt4");
        assert_eq!(config["ai_provider"]["llm_api"]["api_key"], "[REDACTED]");
    }

    #[test]
    fn redact_masks_tls_paths() {
        let mut config = json!({
            "grpc": {
                "grpc_endpoint": "https://grpc.example.com:50051",
                "tls_domain_name": "grpc.example.com",
                "tls_ca_cert_path": "/etc/ssl/ca.pem",
                "tls_client_cert_path": "/etc/ssl/client.pem",
                "tls_client_key_path": "/etc/ssl/client.key",
                "use_tls": true
            }
        });
        redact_sensitive_fields(&mut config);
        assert_eq!(config["grpc"]["grpc_endpoint"], "[REDACTED]");
        assert_eq!(config["grpc"]["tls_domain_name"], "[REDACTED]");
        assert_eq!(config["grpc"]["tls_ca_cert_path"], "[REDACTED]");
        assert_eq!(config["grpc"]["tls_client_cert_path"], "[REDACTED]");
        assert_eq!(config["grpc"]["tls_client_key_path"], "[REDACTED]");
        assert_eq!(config["grpc"]["use_tls"], true);
    }

    #[test]
    fn redact_masks_web_integration_auth_token() {
        let mut config = json!({
            "web": {
                "port": 10090,
                "allow_external": true,
                "integration_auth_token": "secret-token"
            }
        });
        redact_sensitive_fields(&mut config);
        assert_eq!(config["web"]["integration_auth_token"], "[REDACTED]");
        assert_eq!(config["web"]["port"], 10090);
    }

    #[test]
    fn redact_ignores_missing_sections() {
        let mut config = json!({"monitoring": {"interval": 10}});
        // Should not panic when sections like "server", "tls" are absent
        redact_sensitive_fields(&mut config);
        assert_eq!(config["monitoring"]["interval"], 10);
    }

    // ── ALLOWED_KEYS contract ─────────────────────────────────

    #[test]
    fn allowed_keys_matches_expected_set() {
        let expected: Vec<&str> = vec![
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
            "analysis",
        ];
        assert_eq!(ALLOWED_KEYS, expected.as_slice());
    }

    #[test]
    fn allowed_keys_excludes_sensitive_sections() {
        let forbidden = [
            "server",
            "ai_provider",
            "tls",
            "grpc",
            "sandbox",
            "file_access",
        ];
        for key in &forbidden {
            assert!(
                !ALLOWED_KEYS.contains(key),
                "ALLOWED_KEYS must not contain sensitive key '{key}'"
            );
        }
    }

    // ── REDACTED_PATHS contract ───────────────────────────────

    #[test]
    fn redacted_paths_covers_all_sensitive_sections() {
        let sections: Vec<&str> = REDACTED_PATHS.iter().map(|(s, _)| *s).collect();
        assert!(sections.contains(&"server"));
        assert!(sections.contains(&"ai_provider"));
        assert!(sections.contains(&"web"));
        assert!(sections.contains(&"grpc"));
    }

    #[test]
    fn reject_forbidden_allowed_subpaths_rejects_web_integration_token() {
        let patch = json!({
            "web": {
                "integration_auth_token": "secret-token"
            }
        });
        let err = reject_forbidden_allowed_subpaths(&patch).expect_err("forbidden subpath");
        assert!(err.contains("web.integration_auth_token"));
    }

    // ── validate_analysis_config ──────────────────────────────

    fn default_analysis() -> oneshim_core::config::AnalysisConfig {
        oneshim_core::config::AnalysisConfig::default()
    }

    #[test]
    fn validate_analysis_rejects_min_confidence_above_one() {
        let mut cfg = default_analysis();
        cfg.min_confidence = 1.1;
        let err = validate_analysis_config(&cfg).unwrap_err();
        assert!(err.contains("min_confidence"), "got: {err}");
    }

    #[test]
    fn validate_analysis_rejects_min_confidence_below_zero() {
        let mut cfg = default_analysis();
        cfg.min_confidence = -0.1;
        let err = validate_analysis_config(&cfg).unwrap_err();
        assert!(err.contains("min_confidence"), "got: {err}");
    }

    #[test]
    fn validate_analysis_rejects_zero_max_suggestions() {
        let mut cfg = default_analysis();
        cfg.max_suggestions = 0;
        let err = validate_analysis_config(&cfg).unwrap_err();
        assert!(err.contains("max_suggestions"), "got: {err}");
    }

    #[test]
    fn validate_analysis_rejects_interval_below_ten() {
        let mut cfg = default_analysis();
        cfg.interval_secs = 9;
        let err = validate_analysis_config(&cfg).unwrap_err();
        assert!(err.contains("interval_secs"), "got: {err}");
    }

    #[test]
    fn validate_analysis_rejects_full_interval_below_interval() {
        let mut cfg = default_analysis();
        cfg.interval_secs = 60;
        cfg.full_interval_secs = 30;
        let err = validate_analysis_config(&cfg).unwrap_err();
        assert!(err.contains("full_interval_secs"), "got: {err}");
    }

    #[test]
    fn validate_analysis_rejects_zero_throttle() {
        let mut cfg = default_analysis();
        cfg.throttle_secs = 0;
        let err = validate_analysis_config(&cfg).unwrap_err();
        assert!(err.contains("throttle_secs"), "got: {err}");
    }

    #[test]
    fn validate_analysis_accepts_valid_defaults() {
        let cfg = default_analysis();
        assert!(validate_analysis_config(&cfg).is_ok());
    }
}
