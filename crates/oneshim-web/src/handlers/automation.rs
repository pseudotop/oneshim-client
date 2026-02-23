//! 자동화 API 핸들러.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use oneshim_automation::audit::AuditStatus;
use oneshim_automation::policy::AuditLevel;
use oneshim_automation::presets::builtin_presets;
use oneshim_core::config::{ExternalDataPolicy, PiiFilterLevel, SceneActionOverrideConfig};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::error::CoreError;
use oneshim_core::models::automation::AutomationAction;
use oneshim_core::models::intent::{
    AutomationIntent, ElementBounds, IntentCommand, IntentResult, WorkflowPreset,
};
use oneshim_core::models::ui_scene::{UiScene, UI_SCENE_SCHEMA_VERSION};
use std::path::{Path as FsPath, PathBuf};
use std::time::Instant;

use crate::{error::ApiError, AppState};

const AUTOMATION_AUDIT_SCHEMA_VERSION: &str = "automation.audit.v1";
const AUTOMATION_SCENE_ACTION_SCHEMA_VERSION: &str = "automation.scene_action.v1";

// ============================================================
// DTO
// ============================================================

/// 자동화 시스템 상태
#[derive(Debug, Serialize)]
pub struct AutomationStatusDto {
    pub enabled: bool,
    pub sandbox_enabled: bool,
    pub sandbox_profile: String,
    pub ocr_provider: String,
    pub llm_provider: String,
    pub external_data_policy: String,
    pub pending_audit_entries: usize,
}

/// 감사 로그 항목
#[derive(Debug, Serialize)]
pub struct AuditEntryDto {
    pub schema_version: String,
    pub entry_id: String,
    pub timestamp: String,
    pub session_id: String,
    pub command_id: String,
    pub action_type: String,
    pub status: String,
    pub details: Option<String>,
    pub elapsed_ms: Option<u64>,
}

/// 감사 로그 쿼리
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    pub limit: usize,
    pub status: Option<String>,
}

fn default_audit_limit() -> usize {
    50
}

fn require_config_manager(state: &AppState) -> Result<&ConfigManager, ApiError> {
    state
        .config_manager
        .as_ref()
        .ok_or_else(|| ApiError::Internal("설정 관리자 미설정".into()))
}

fn default_automation_status(pending: usize) -> AutomationStatusDto {
    AutomationStatusDto {
        enabled: false,
        sandbox_enabled: false,
        sandbox_profile: "Standard".to_string(),
        ocr_provider: "Local".to_string(),
        llm_provider: "Local".to_string(),
        external_data_policy: "PiiFilterStrict".to_string(),
        pending_audit_entries: pending,
    }
}

fn default_policies() -> PoliciesDto {
    PoliciesDto {
        automation_enabled: false,
        sandbox_profile: "Standard".to_string(),
        sandbox_enabled: false,
        allow_network: false,
        external_data_policy: "PiiFilterStrict".to_string(),
        scene_action_override_enabled: false,
        scene_action_override_active: false,
        scene_action_override_reason: None,
        scene_action_override_approved_by: None,
        scene_action_override_expires_at: None,
        scene_action_override_issue: None,
    }
}

fn parse_audit_status(status_filter: &str) -> Result<AuditStatus, ApiError> {
    match status_filter {
        "Started" => Ok(AuditStatus::Started),
        "Completed" => Ok(AuditStatus::Completed),
        "Failed" => Ok(AuditStatus::Failed),
        "Denied" => Ok(AuditStatus::Denied),
        "Timeout" => Ok(AuditStatus::Timeout),
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 상태 필터: {}",
            status_filter
        ))),
    }
}

/// 실행 통계
#[derive(Debug, Serialize)]
pub struct AutomationStatsDto {
    pub total_executions: usize,
    pub successful: usize,
    pub failed: usize,
    pub denied: usize,
    pub timeout: usize,
    pub avg_elapsed_ms: f64,
    pub success_rate: f64,
    pub blocked_rate: f64,
    pub p95_elapsed_ms: f64,
    pub timing_samples: usize,
}

/// 정책 정보
#[derive(Debug, Serialize)]
pub struct PoliciesDto {
    pub automation_enabled: bool,
    pub sandbox_profile: String,
    pub sandbox_enabled: bool,
    pub allow_network: bool,
    pub external_data_policy: String,
    pub scene_action_override_enabled: bool,
    pub scene_action_override_active: bool,
    pub scene_action_override_reason: Option<String>,
    pub scene_action_override_approved_by: Option<String>,
    pub scene_action_override_expires_at: Option<String>,
    pub scene_action_override_issue: Option<String>,
}

/// 프리셋 목록 응답
#[derive(Debug, Serialize)]
pub struct PresetListDto {
    pub presets: Vec<WorkflowPreset>,
}

/// 프리셋 실행 결과
#[derive(Debug, Serialize)]
pub struct PresetRunResult {
    pub preset_id: String,
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps_executed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_steps: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_elapsed_ms: Option<u64>,
}

/// 자연어 의도 실행 요청
#[derive(Debug, Deserialize)]
pub struct ExecuteIntentHintRequest {
    pub command_id: Option<String>,
    pub session_id: String,
    pub intent_hint: String,
}

/// 자연어 의도 실행 응답
#[derive(Debug, Serialize)]
pub struct ExecuteIntentHintResponse {
    pub command_id: String,
    pub session_id: String,
    pub planned_intent: AutomationIntent,
    pub result: IntentResult,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneActionType {
    Click,
    TypeText,
}

/// Scene 좌표 기반 실행 요청 (결정적 실행 경로)
#[derive(Debug, Deserialize)]
pub struct ExecuteSceneActionRequest {
    pub command_id: Option<String>,
    pub session_id: String,
    pub frame_id: Option<i64>,
    pub scene_id: Option<String>,
    pub element_id: String,
    pub action_type: SceneActionType,
    pub bbox_abs: ElementBounds,
    pub role: Option<String>,
    pub label: Option<String>,
    pub text: Option<String>,
    pub allow_sensitive_input: Option<bool>,
}

/// Scene 좌표 기반 실행 응답
#[derive(Debug, Serialize)]
pub struct ExecuteSceneActionResponse {
    pub schema_version: String,
    pub command_id: String,
    pub session_id: String,
    pub frame_id: Option<i64>,
    pub scene_id: Option<String>,
    pub element_id: String,
    pub applied_privacy_policy: String,
    pub scene_action_override_active: bool,
    pub scene_action_override_expires_at: Option<String>,
    pub executed_intents: Vec<AutomationIntent>,
    pub result: IntentResult,
}

/// 자동화 계약 버전 정보
#[derive(Debug, Serialize)]
pub struct AutomationContractsDto {
    pub audit_schema_version: String,
    pub scene_schema_version: String,
    pub scene_action_schema_version: String,
}

#[derive(Debug, Clone)]
struct SceneActionPolicyContext {
    policy: ExternalDataPolicy,
    pii_filter_level: PiiFilterLevel,
    override_enabled: bool,
    override_active: bool,
    override_reason: Option<String>,
    override_approved_by: Option<String>,
    override_expires_at: Option<DateTime<Utc>>,
    override_issue: Option<String>,
}

/// Scene 분석 쿼리
#[derive(Debug, Deserialize)]
pub struct SceneQuery {
    pub app_name: Option<String>,
    pub screen_id: Option<String>,
    pub frame_id: Option<i64>,
}

fn build_scene_action_intents(
    req: &ExecuteSceneActionRequest,
) -> Result<Vec<AutomationIntent>, ApiError> {
    if req.session_id.trim().is_empty() {
        return Err(ApiError::BadRequest("session_id는 필수입니다".to_string()));
    }
    if req.element_id.trim().is_empty() {
        return Err(ApiError::BadRequest("element_id는 필수입니다".to_string()));
    }
    if req.bbox_abs.width == 0 || req.bbox_abs.height == 0 {
        return Err(ApiError::BadRequest(
            "bbox_abs.width/height는 0보다 커야 합니다".to_string(),
        ));
    }

    let (center_x, center_y) = req.bbox_abs.center();

    match req.action_type {
        SceneActionType::Click => Ok(vec![AutomationIntent::Raw(AutomationAction::MouseClick {
            button: "left".to_string(),
            x: center_x,
            y: center_y,
        })]),
        SceneActionType::TypeText => {
            let text = req
                .text
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    ApiError::BadRequest("type_text 액션은 text가 필요합니다".to_string())
                })?;

            Ok(vec![
                AutomationIntent::Raw(AutomationAction::MouseClick {
                    button: "left".to_string(),
                    x: center_x,
                    y: center_y,
                }),
                AutomationIntent::Raw(AutomationAction::KeyType { text }),
            ])
        }
    }
}

fn evaluate_scene_action_override(
    cfg: &SceneActionOverrideConfig,
    now: DateTime<Utc>,
) -> (bool, Option<String>) {
    if !cfg.enabled {
        return (false, None);
    }

    let reason = cfg.reason.as_deref().map(str::trim).unwrap_or_default();
    if reason.is_empty() {
        return (
            false,
            Some("사유(reason)가 비어 있어 오버라이드가 무효입니다.".to_string()),
        );
    }

    let approved_by = cfg
        .approved_by
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    if approved_by.is_empty() {
        return (
            false,
            Some("승인자(approved_by)가 비어 있어 오버라이드가 무효입니다.".to_string()),
        );
    }

    let Some(expires_at) = cfg.expires_at else {
        return (
            false,
            Some("만료 시각(expires_at)이 없어 오버라이드가 무효입니다.".to_string()),
        );
    };

    if expires_at <= now {
        return (false, Some("오버라이드 TTL이 만료되었습니다.".to_string()));
    }

    (true, None)
}

fn read_scene_action_policy(state: &AppState) -> SceneActionPolicyContext {
    if let Some(config_manager) = state.config_manager.as_ref() {
        let config = config_manager.get();
        let override_cfg = &config.ai_provider.scene_action_override;
        let (override_active, override_issue) =
            evaluate_scene_action_override(override_cfg, Utc::now());

        SceneActionPolicyContext {
            policy: config.ai_provider.external_data_policy,
            pii_filter_level: config.privacy.pii_filter_level,
            override_enabled: override_cfg.enabled,
            override_active,
            override_reason: override_cfg.reason.clone(),
            override_approved_by: override_cfg.approved_by.clone(),
            override_expires_at: override_cfg.expires_at,
            override_issue,
        }
    } else {
        SceneActionPolicyContext {
            policy: ExternalDataPolicy::PiiFilterStrict,
            pii_filter_level: PiiFilterLevel::Standard,
            override_enabled: false,
            override_active: false,
            override_reason: None,
            override_approved_by: None,
            override_expires_at: None,
            override_issue: None,
        }
    }
}

fn enforce_scene_action_privacy(
    state: &AppState,
    req: &ExecuteSceneActionRequest,
) -> Result<SceneActionPolicyContext, ApiError> {
    let context = read_scene_action_policy(state);
    let allow_sensitive = req.allow_sensitive_input.unwrap_or(false);
    let override_active = context.override_active;
    let override_hint = context
        .override_issue
        .as_ref()
        .map(|issue| format!(" 현재 오버라이드 상태: {issue}"))
        .unwrap_or_default();

    match (context.policy, req.action_type) {
        (ExternalDataPolicy::PiiFilterStrict, SceneActionType::TypeText) => {
            if !allow_sensitive && !override_active {
                return Err(ApiError::BadRequest(format!(
                    "PiiFilterStrict 정책에서는 type_text 액션이 차단됩니다. allow_sensitive_input=true를 전달하거나 유효한 오버라이드를 설정하세요.{override_hint}"
                )));
            }
        }
        (ExternalDataPolicy::PiiFilterStandard, SceneActionType::TypeText) => {
            if !allow_sensitive && !override_active {
                return Err(ApiError::BadRequest(format!(
                    "PiiFilterStandard 정책에서는 type_text 액션에 allow_sensitive_input=true 또는 유효한 오버라이드가 필요합니다.{override_hint}"
                )));
            }
        }
        _ => {}
    }

    Ok(context)
}

fn infer_image_format(path: &FsPath) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
        .unwrap_or_else(|| "webp".to_string())
}

fn candidate_frame_paths(base: &FsPath, raw_relative: &FsPath) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let base_name = base
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let starts_with_frames = raw_relative
        .components()
        .next()
        .map(|c| c.as_os_str() == "frames")
        .unwrap_or(false);

    candidates.push(base.join(raw_relative));

    if base_name == "frames" && starts_with_frames {
        if let Some(parent) = base.parent() {
            candidates.push(parent.join(raw_relative));
        }
    } else if base_name != "frames" && !starts_with_frames {
        candidates.push(base.join("frames").join(raw_relative));
    }

    candidates
}

fn resolve_frame_image_path(state: &AppState, stored_path: &str) -> Result<PathBuf, ApiError> {
    let path = PathBuf::from(stored_path);
    if path.is_absolute() {
        return Ok(path);
    }

    let Some(base) = state.frames_dir.as_ref() else {
        return Err(ApiError::Internal(
            "프레임 경로 루트가 설정되지 않아 frame_id 조회를 처리할 수 없습니다".to_string(),
        ));
    };

    let candidates = candidate_frame_paths(base, &path);
    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    Ok(candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| base.join(path)))
}

// ============================================================
// 핸들러
// ============================================================

/// GET /api/automation/contracts — 자동화 계약 버전 조회
pub async fn get_contract_versions() -> Result<Json<AutomationContractsDto>, ApiError> {
    Ok(Json(AutomationContractsDto {
        audit_schema_version: AUTOMATION_AUDIT_SCHEMA_VERSION.to_string(),
        scene_schema_version: UI_SCENE_SCHEMA_VERSION.to_string(),
        scene_action_schema_version: AUTOMATION_SCENE_ACTION_SCHEMA_VERSION.to_string(),
    }))
}

/// GET /api/automation/status — 자동화 시스템 상태
pub async fn get_automation_status(
    State(state): State<AppState>,
) -> Result<Json<AutomationStatusDto>, ApiError> {
    let pending = if let Some(ref logger) = state.audit_logger {
        let guard = logger.read().await;
        guard.pending_count()
    } else {
        0
    };

    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        Ok(Json(AutomationStatusDto {
            enabled: config.automation.enabled,
            sandbox_enabled: config.automation.sandbox.enabled,
            sandbox_profile: format!("{:?}", config.automation.sandbox.profile),
            ocr_provider: format!("{:?}", config.ai_provider.ocr_provider),
            llm_provider: format!("{:?}", config.ai_provider.llm_provider),
            external_data_policy: format!("{:?}", config.ai_provider.external_data_policy),
            pending_audit_entries: pending,
        }))
    } else {
        Ok(Json(default_automation_status(pending)))
    }
}

/// GET /api/automation/audit — 감사 로그 조회
pub async fn get_audit_logs(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntryDto>>, ApiError> {
    let Some(ref logger) = state.audit_logger else {
        return Ok(Json(Vec::new()));
    };

    let guard = logger.read().await;

    let entries = if let Some(ref status_filter) = query.status {
        let status = parse_audit_status(status_filter)?;
        guard.entries_by_status(&status, query.limit)
    } else {
        guard.recent_entries(query.limit)
    };

    let dtos = entries
        .into_iter()
        .map(|e| AuditEntryDto {
            schema_version: AUTOMATION_AUDIT_SCHEMA_VERSION.to_string(),
            entry_id: e.entry_id,
            timestamp: e.timestamp.to_rfc3339(),
            session_id: e.session_id,
            command_id: e.command_id,
            action_type: e.action_type,
            status: format!("{:?}", e.status),
            details: e.details,
            elapsed_ms: e.execution_time_ms,
        })
        .collect();

    Ok(Json(dtos))
}

/// GET /api/automation/policies — 활성 정책 목록
pub async fn get_policies(State(state): State<AppState>) -> Result<Json<PoliciesDto>, ApiError> {
    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        let (override_active, override_issue) =
            evaluate_scene_action_override(&config.ai_provider.scene_action_override, Utc::now());
        Ok(Json(PoliciesDto {
            automation_enabled: config.automation.enabled,
            sandbox_profile: format!("{:?}", config.automation.sandbox.profile),
            sandbox_enabled: config.automation.sandbox.enabled,
            allow_network: config.automation.sandbox.allow_network,
            external_data_policy: format!("{:?}", config.ai_provider.external_data_policy),
            scene_action_override_enabled: config.ai_provider.scene_action_override.enabled,
            scene_action_override_active: override_active,
            scene_action_override_reason: config.ai_provider.scene_action_override.reason.clone(),
            scene_action_override_approved_by: config
                .ai_provider
                .scene_action_override
                .approved_by
                .clone(),
            scene_action_override_expires_at: config
                .ai_provider
                .scene_action_override
                .expires_at
                .map(|v| v.to_rfc3339()),
            scene_action_override_issue: override_issue,
        }))
    } else {
        Ok(Json(default_policies()))
    }
}

/// GET /api/automation/stats — 실행 통계
pub async fn get_automation_stats(
    State(state): State<AppState>,
) -> Result<Json<AutomationStatsDto>, ApiError> {
    let Some(ref logger) = state.audit_logger else {
        return Ok(Json(AutomationStatsDto {
            total_executions: 0,
            successful: 0,
            failed: 0,
            denied: 0,
            timeout: 0,
            avg_elapsed_ms: 0.0,
            success_rate: 0.0,
            blocked_rate: 0.0,
            p95_elapsed_ms: 0.0,
            timing_samples: 0,
        }));
    };

    let guard = logger.read().await;
    let (total, success, failed, denied, timeout) = guard.stats();

    // 평균 실행 시간 계산
    let all_entries = guard.recent_entries(1000);
    let elapsed_values: Vec<u64> = all_entries
        .iter()
        .filter_map(|e| e.execution_time_ms)
        .collect();
    let avg_elapsed = if elapsed_values.is_empty() {
        0.0
    } else {
        elapsed_values.iter().sum::<u64>() as f64 / elapsed_values.len() as f64
    };
    let p95_elapsed_ms = if elapsed_values.is_empty() {
        0.0
    } else {
        let mut sorted = elapsed_values.clone();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64) * 0.95).ceil() as usize;
        sorted[idx.saturating_sub(1).min(sorted.len() - 1)] as f64
    };
    let total_f64 = total as f64;
    let success_rate = if total > 0 {
        success as f64 / total_f64
    } else {
        0.0
    };
    let blocked_rate = if total > 0 {
        denied as f64 / total_f64
    } else {
        0.0
    };

    Ok(Json(AutomationStatsDto {
        total_executions: total,
        successful: success,
        failed,
        denied,
        timeout,
        avg_elapsed_ms: avg_elapsed,
        success_rate,
        blocked_rate,
        p95_elapsed_ms,
        timing_samples: elapsed_values.len(),
    }))
}

/// GET /api/automation/presets — 전체 프리셋 목록 (내장 + 사용자)
pub async fn list_presets(State(state): State<AppState>) -> Result<Json<PresetListDto>, ApiError> {
    let mut presets = builtin_presets();

    // 사용자 정의 프리셋 추가
    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        presets.extend(config.automation.custom_presets.clone());
    }

    Ok(Json(PresetListDto { presets }))
}

/// POST /api/automation/presets — 사용자 프리셋 생성
pub async fn create_preset(
    State(state): State<AppState>,
    Json(preset): Json<WorkflowPreset>,
) -> Result<Json<WorkflowPreset>, ApiError> {
    if preset.id.is_empty() || preset.name.is_empty() {
        return Err(ApiError::BadRequest("프리셋 ID와 이름은 필수입니다".into()));
    }
    if preset.steps.is_empty() {
        return Err(ApiError::BadRequest("최소 1개 단계가 필요합니다".into()));
    }

    let config_manager = require_config_manager(&state)?;

    config_manager
        .update_with(|config| {
            // 중복 ID 확인
            if config
                .automation
                .custom_presets
                .iter()
                .any(|p| p.id == preset.id)
            {
                return;
            }
            let mut new_preset = preset.clone();
            new_preset.builtin = false;
            config.automation.custom_presets.push(new_preset);
        })
        .map_err(|e| ApiError::Internal(format!("프리셋 저장 실패: {e}")))?;

    Ok(Json(preset))
}

/// PUT /api/automation/presets/:id — 사용자 프리셋 수정
pub async fn update_preset(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(preset): Json<WorkflowPreset>,
) -> Result<Json<WorkflowPreset>, ApiError> {
    let config_manager = require_config_manager(&state)?;

    let mut found = false;
    config_manager
        .update_with(|config| {
            if let Some(existing) = config
                .automation
                .custom_presets
                .iter_mut()
                .find(|p| p.id == id)
            {
                existing.name = preset.name.clone();
                existing.description = preset.description.clone();
                existing.category = preset.category;
                existing.steps = preset.steps.clone();
                existing.platform = preset.platform.clone();
                found = true;
            }
        })
        .map_err(|e| ApiError::Internal(format!("프리셋 수정 실패: {e}")))?;

    if !found {
        return Err(ApiError::NotFound(format!("프리셋 '{}' 미발견", id)));
    }

    Ok(Json(preset))
}

/// DELETE /api/automation/presets/:id — 사용자 프리셋 삭제
pub async fn delete_preset(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let config_manager = require_config_manager(&state)?;

    let mut found = false;
    config_manager
        .update_with(|config| {
            let before_len = config.automation.custom_presets.len();
            config.automation.custom_presets.retain(|p| p.id != id);
            found = config.automation.custom_presets.len() < before_len;
        })
        .map_err(|e| ApiError::Internal(format!("프리셋 삭제 실패: {e}")))?;

    if !found {
        return Err(ApiError::NotFound(format!("프리셋 '{}' 미발견", id)));
    }

    Ok(Json(serde_json::json!({ "deleted": id })))
}

/// POST /api/automation/presets/:id/run — 프리셋 실행
pub async fn run_preset(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PresetRunResult>, ApiError> {
    // 프리셋 찾기 (내장 + 사용자)
    let all_presets = builtin_presets();
    let mut preset = all_presets.iter().find(|p| p.id == id).cloned();

    if preset.is_none() {
        if let Some(ref config_manager) = state.config_manager {
            let config = config_manager.get();
            preset = config
                .automation
                .custom_presets
                .iter()
                .find(|p| p.id == id)
                .cloned();
        }
    }

    let Some(preset) = preset else {
        return Err(ApiError::NotFound(format!("프리셋 '{}' 미발견", id)));
    };

    // 자동화 활성화 확인
    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        if !config.automation.enabled {
            return Err(ApiError::BadRequest(
                "자동화가 비활성화 상태입니다".to_string(),
            ));
        }
    }

    // 자동화 컨트롤러가 설정된 경우 실제 실행
    if let Some(ref controller) = state.automation_controller {
        match controller.run_workflow(&preset).await {
            Ok(result) => {
                if !result.success {
                    return Err(ApiError::BadRequest(result.message));
                }

                Ok(Json(PresetRunResult {
                    preset_id: result.preset_id,
                    success: true,
                    message: result.message,
                    steps_executed: Some(result.steps_executed),
                    total_steps: Some(result.total_steps),
                    total_elapsed_ms: Some(result.total_elapsed_ms),
                }))
            }
            Err(e) => Err(ApiError::Internal(format!("실행 실패: {}", e))),
        }
    } else {
        // 폴백: 컨트롤러 미설정 → 로깅 전용
        tracing::info!(
            preset_id = %preset.id,
            steps = preset.steps.len(),
            "워크플로우 프리셋 실행 요청 (컨트롤러 미설정, 로깅 전용)"
        );

        Ok(Json(PresetRunResult {
            preset_id: id,
            success: true,
            message: format!(
                "프리셋 '{}' 실행 요청됨 ({}단계, 로깅 전용)",
                preset.name,
                preset.steps.len()
            ),
            steps_executed: None,
            total_steps: Some(preset.steps.len()),
            total_elapsed_ms: None,
        }))
    }
}

/// POST /api/automation/execute-hint — 자연어 의도 실행
pub async fn execute_intent_hint(
    State(state): State<AppState>,
    Json(req): Json<ExecuteIntentHintRequest>,
) -> Result<Json<ExecuteIntentHintResponse>, ApiError> {
    if req.session_id.trim().is_empty() {
        return Err(ApiError::BadRequest("session_id는 필수입니다".to_string()));
    }
    if req.intent_hint.trim().is_empty() {
        return Err(ApiError::BadRequest("intent_hint는 필수입니다".to_string()));
    }

    let Some(ref controller) = state.automation_controller else {
        return Err(ApiError::BadRequest(
            "자동화 컨트롤러가 활성화되지 않았습니다".to_string(),
        ));
    };

    let command_id = req
        .command_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "intent-hint-{}",
                chrono::Utc::now().timestamp_millis().abs()
            )
        });

    match controller
        .execute_intent_hint(&command_id, &req.session_id, &req.intent_hint)
        .await
    {
        Ok(planned) => Ok(Json(ExecuteIntentHintResponse {
            command_id,
            session_id: req.session_id,
            planned_intent: planned.planned_intent,
            result: planned.result,
        })),
        Err(
            CoreError::PolicyDenied(msg)
            | CoreError::InvalidArguments(msg)
            | CoreError::ElementNotFound(msg),
        ) => Err(ApiError::BadRequest(msg)),
        Err(CoreError::Internal(msg))
            if msg.contains("IntentPlanner") || msg.contains("IntentExecutor") =>
        {
            Err(ApiError::BadRequest(msg))
        }
        Err(e) => Err(ApiError::Internal(format!("자연어 의도 실행 실패: {e}"))),
    }
}

/// POST /api/automation/execute-scene-action — Scene 좌표 기반 결정적 액션 실행
pub async fn execute_scene_action(
    State(state): State<AppState>,
    Json(req): Json<ExecuteSceneActionRequest>,
) -> Result<Json<ExecuteSceneActionResponse>, ApiError> {
    let Some(ref controller) = state.automation_controller else {
        return Err(ApiError::BadRequest(
            "자동화 컨트롤러가 활성화되지 않았습니다".to_string(),
        ));
    };

    let intents = build_scene_action_intents(&req)?;
    let policy_context = enforce_scene_action_privacy(&state, &req)?;
    let command_id = req
        .command_id
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            format!(
                "scene-action-{}",
                chrono::Utc::now().timestamp_millis().abs()
            )
        });
    let started_at = Instant::now();

    if let Some(logger) = state.audit_logger.as_ref() {
        let mut guard = logger.write().await;
        guard.log_start_if(
            AuditLevel::Detailed,
            &command_id,
            &req.session_id,
            &format!(
                "scene_action frame_id={:?} scene_id={:?} element_id={} action_type={:?} policy={:?} pii_level={:?} override_enabled={} override_active={} override_reason={:?} override_approved_by={:?} override_expires_at={:?} override_issue={:?}",
                req.frame_id,
                req.scene_id,
                req.element_id,
                req.action_type,
                policy_context.policy,
                policy_context.pii_filter_level,
                policy_context.override_enabled,
                policy_context.override_active,
                policy_context.override_reason.as_deref(),
                policy_context.override_approved_by.as_deref(),
                policy_context
                    .override_expires_at
                    .as_ref()
                    .map(|value| value.to_rfc3339()),
                policy_context.override_issue.as_deref()
            ),
        );
    }

    let mut last_result: Option<IntentResult> = None;
    for (index, intent) in intents.iter().enumerate() {
        let staged_command_id = if index == 0 {
            command_id.clone()
        } else {
            format!("{command_id}:stage-{index}")
        };

        let intent_command = IntentCommand {
            command_id: staged_command_id,
            session_id: req.session_id.clone(),
            intent: intent.clone(),
            config: None,
            timeout_ms: None,
            policy_token: "scene-action".to_string(),
        };

        match controller.execute_intent(&intent_command).await {
            Ok(result) => {
                let failed = !result.success;
                last_result = Some(result);
                if failed {
                    break;
                }
            }
            Err(
                CoreError::PolicyDenied(msg)
                | CoreError::InvalidArguments(msg)
                | CoreError::ElementNotFound(msg),
            ) => return Err(ApiError::BadRequest(msg)),
            Err(CoreError::Internal(msg))
                if msg.contains("IntentExecutor") || msg.contains("IntentPlanner") =>
            {
                return Err(ApiError::BadRequest(msg));
            }
            Err(e) => return Err(ApiError::Internal(format!("scene 액션 실행 실패: {e}"))),
        }
    }

    let result = last_result.unwrap_or(IntentResult {
        success: false,
        element: None,
        verification: None,
        retry_count: 0,
        elapsed_ms: 0,
        error: Some("실행 가능한 액션이 없습니다".to_string()),
    });
    let elapsed_ms = started_at.elapsed().as_millis() as u64;

    if let Some(logger) = state.audit_logger.as_ref() {
        let mut guard = logger.write().await;
        guard.log_complete_with_time(
            AuditLevel::Detailed,
            &command_id,
            &req.session_id,
            &format!(
                "scene_action_result success={} frame_id={:?} scene_id={:?} element_id={} policy={:?} override_active={} error={:?}",
                result.success,
                req.frame_id,
                req.scene_id,
                req.element_id,
                policy_context.policy,
                policy_context.override_active,
                result.error
            ),
            elapsed_ms,
        );
    }

    Ok(Json(ExecuteSceneActionResponse {
        schema_version: AUTOMATION_SCENE_ACTION_SCHEMA_VERSION.to_string(),
        command_id,
        session_id: req.session_id,
        frame_id: req.frame_id,
        scene_id: req.scene_id,
        element_id: req.element_id,
        applied_privacy_policy: format!("{:?}", policy_context.policy),
        scene_action_override_active: policy_context.override_active,
        scene_action_override_expires_at: policy_context
            .override_expires_at
            .map(|value| value.to_rfc3339()),
        executed_intents: intents,
        result,
    }))
}

/// GET /api/automation/scene — 현재 화면의 구조화된 UI Scene 조회
pub async fn get_automation_scene(
    State(state): State<AppState>,
    Query(query): Query<SceneQuery>,
) -> Result<Json<UiScene>, ApiError> {
    let Some(ref controller) = state.automation_controller else {
        return Err(ApiError::BadRequest(
            "자동화 컨트롤러가 활성화되지 않았습니다".to_string(),
        ));
    };

    let analyze_result = if let Some(frame_id) = query.frame_id {
        let stored_path = state
            .storage
            .get_frame_file_path(frame_id)
            .map_err(|e| ApiError::Internal(format!("프레임 경로 조회 실패: {e}")))?
            .ok_or_else(|| ApiError::NotFound(format!("프레임 {frame_id}에 이미지가 없습니다")))?;

        let image_path = resolve_frame_image_path(&state, &stored_path)?;
        let image_data = std::fs::read(&image_path)
            .map_err(|e| ApiError::Internal(format!("프레임 이미지 읽기 실패: {e}")))?;

        controller
            .analyze_scene_from_image(
                image_data,
                infer_image_format(&image_path),
                query.app_name.as_deref(),
                query.screen_id.as_deref(),
            )
            .await
    } else {
        controller
            .analyze_scene(query.app_name.as_deref(), query.screen_id.as_deref())
            .await
    };

    match analyze_result {
        Ok(scene) => Ok(Json(scene)),
        Err(
            CoreError::PolicyDenied(msg)
            | CoreError::InvalidArguments(msg)
            | CoreError::ElementNotFound(msg),
        ) => Err(ApiError::BadRequest(msg)),
        Err(CoreError::Internal(msg))
            if msg.contains("Scene 분석기")
                || msg.contains("scene 분석을 지원하지")
                || msg.contains("이미지 직접 scene 분석") =>
        {
            Err(ApiError::BadRequest(msg))
        }
        Err(e) => Err(ApiError::Internal(format!("scene 분석 실패: {e}"))),
    }
}

// ============================================================
// 테스트
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_status_dto_serializes() {
        let dto = AutomationStatusDto {
            enabled: true,
            sandbox_enabled: true,
            sandbox_profile: "Standard".to_string(),
            ocr_provider: "Local".to_string(),
            llm_provider: "Remote".to_string(),
            external_data_policy: "PiiFilterStrict".to_string(),
            pending_audit_entries: 5,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("sandbox_profile"));
        assert!(json.contains("pending_audit_entries"));
    }

    #[test]
    fn audit_entry_dto_serializes() {
        let dto = AuditEntryDto {
            schema_version: AUTOMATION_AUDIT_SCHEMA_VERSION.to_string(),
            entry_id: "e-001".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            session_id: "sess-001".to_string(),
            command_id: "cmd-001".to_string(),
            action_type: "MouseClick".to_string(),
            status: "Completed".to_string(),
            details: Some("OK".to_string()),
            elapsed_ms: Some(150),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("cmd-001"));
        assert!(json.contains("150"));
    }

    #[test]
    fn automation_stats_dto_serializes() {
        let dto = AutomationStatsDto {
            total_executions: 100,
            successful: 80,
            failed: 10,
            denied: 5,
            timeout: 5,
            avg_elapsed_ms: 250.5,
            success_rate: 0.8,
            blocked_rate: 0.05,
            p95_elapsed_ms: 420.0,
            timing_samples: 92,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("total_executions"));
        assert!(json.contains("avg_elapsed_ms"));
        assert!(json.contains("success_rate"));
        assert!(json.contains("p95_elapsed_ms"));
    }

    #[test]
    fn policies_dto_serializes() {
        let dto = PoliciesDto {
            automation_enabled: true,
            sandbox_profile: "Strict".to_string(),
            sandbox_enabled: true,
            allow_network: false,
            external_data_policy: "PiiFilterStrict".to_string(),
            scene_action_override_enabled: true,
            scene_action_override_active: true,
            scene_action_override_reason: Some("calibration".to_string()),
            scene_action_override_approved_by: Some("security-reviewer".to_string()),
            scene_action_override_expires_at: Some("2026-02-24T03:00:00Z".to_string()),
            scene_action_override_issue: None,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("Strict"));
        assert!(json.contains("scene_action_override_active"));
    }

    #[test]
    fn preset_run_result_serializes() {
        let dto = PresetRunResult {
            preset_id: "save-file".to_string(),
            success: true,
            message: "실행됨".to_string(),
            steps_executed: Some(2),
            total_steps: Some(3),
            total_elapsed_ms: Some(150),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("save-file"));
        assert!(json.contains("steps_executed"));
    }

    #[test]
    fn preset_run_result_omits_none_fields() {
        let dto = PresetRunResult {
            preset_id: "test".to_string(),
            success: false,
            message: "실패".to_string(),
            steps_executed: None,
            total_steps: None,
            total_elapsed_ms: None,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(!json.contains("steps_executed"));
        assert!(!json.contains("total_steps"));
        assert!(!json.contains("total_elapsed_ms"));
    }

    #[test]
    fn audit_query_defaults() {
        let json = "{}";
        let query: AuditQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.limit, 50);
        assert!(query.status.is_none());
    }

    #[test]
    fn execute_intent_hint_request_deserializes_optional_command_id() {
        let payload = r#"{
            "session_id": "sess-1",
            "intent_hint": "저장 버튼 클릭"
        }"#;
        let request: ExecuteIntentHintRequest = serde_json::from_str(payload).unwrap();
        assert!(request.command_id.is_none());
        assert_eq!(request.session_id, "sess-1");
        assert_eq!(request.intent_hint, "저장 버튼 클릭");
    }

    #[test]
    fn execute_intent_hint_response_serializes() {
        let response = ExecuteIntentHintResponse {
            command_id: "hint-1".to_string(),
            session_id: "sess-1".to_string(),
            planned_intent: oneshim_core::models::intent::AutomationIntent::ExecuteHotkey {
                keys: vec!["Ctrl".to_string(), "S".to_string()],
            },
            result: oneshim_core::models::intent::IntentResult {
                success: true,
                element: None,
                verification: None,
                retry_count: 0,
                elapsed_ms: 10,
                error: None,
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("planned_intent"));
        assert!(json.contains("command_id"));
    }

    #[test]
    fn scene_query_deserializes_frame_id() {
        let json = r#"{"app_name":"Code","screen_id":"main","frame_id":42}"#;
        let query: SceneQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.app_name.as_deref(), Some("Code"));
        assert_eq!(query.screen_id.as_deref(), Some("main"));
        assert_eq!(query.frame_id, Some(42));
    }

    #[test]
    fn infer_image_format_falls_back_to_webp() {
        let path = std::path::Path::new("frames/2026-02-24/capture");
        assert_eq!(infer_image_format(path), "webp");
    }

    #[test]
    fn build_scene_action_intents_click_returns_raw_click() {
        let req = ExecuteSceneActionRequest {
            command_id: None,
            session_id: "sess-1".to_string(),
            frame_id: Some(1),
            scene_id: Some("scene-1".to_string()),
            element_id: "el-1".to_string(),
            action_type: SceneActionType::Click,
            bbox_abs: ElementBounds {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            },
            role: Some("button".to_string()),
            label: Some("Save".to_string()),
            text: None,
            allow_sensitive_input: None,
        };

        let intents = build_scene_action_intents(&req).unwrap();
        assert_eq!(intents.len(), 1);
        assert!(matches!(intents[0], AutomationIntent::Raw(_)));
    }

    #[test]
    fn build_scene_action_intents_type_text_requires_text() {
        let req = ExecuteSceneActionRequest {
            command_id: None,
            session_id: "sess-1".to_string(),
            frame_id: Some(1),
            scene_id: Some("scene-1".to_string()),
            element_id: "el-2".to_string(),
            action_type: SceneActionType::TypeText,
            bbox_abs: ElementBounds {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            },
            role: Some("input".to_string()),
            label: Some("Search".to_string()),
            text: None,
            allow_sensitive_input: None,
        };

        let err = build_scene_action_intents(&req).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn evaluate_scene_action_override_reports_missing_reason() {
        let cfg = SceneActionOverrideConfig {
            enabled: true,
            reason: None,
            approved_by: Some("reviewer".to_string()),
            expires_at: Some(Utc::now() + chrono::Duration::minutes(10)),
        };
        let (active, issue) = evaluate_scene_action_override(&cfg, Utc::now());
        assert!(!active);
        assert!(issue.unwrap_or_default().contains("사유"));
    }

    #[test]
    fn evaluate_scene_action_override_reports_expired_ttl() {
        let cfg = SceneActionOverrideConfig {
            enabled: true,
            reason: Some("incident".to_string()),
            approved_by: Some("reviewer".to_string()),
            expires_at: Some(Utc::now() - chrono::Duration::minutes(1)),
        };
        let (active, issue) = evaluate_scene_action_override(&cfg, Utc::now());
        assert!(!active);
        assert!(issue.unwrap_or_default().contains("만료"));
    }

    #[test]
    fn evaluate_scene_action_override_active_when_valid() {
        let cfg = SceneActionOverrideConfig {
            enabled: true,
            reason: Some("high-fidelity validation".to_string()),
            approved_by: Some("reviewer".to_string()),
            expires_at: Some(Utc::now() + chrono::Duration::minutes(20)),
        };
        let (active, issue) = evaluate_scene_action_override(&cfg, Utc::now());
        assert!(active);
        assert!(issue.is_none());
    }
}
