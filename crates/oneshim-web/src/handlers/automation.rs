//! 자동화 API 핸들러.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use oneshim_automation::audit::AuditStatus;
use oneshim_automation::presets::builtin_presets;
use oneshim_core::models::intent::WorkflowPreset;

use crate::{error::ApiError, AppState};

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

/// 실행 통계
#[derive(Debug, Serialize)]
pub struct AutomationStatsDto {
    pub total_executions: usize,
    pub successful: usize,
    pub failed: usize,
    pub denied: usize,
    pub timeout: usize,
    pub avg_elapsed_ms: f64,
}

/// 정책 정보
#[derive(Debug, Serialize)]
pub struct PoliciesDto {
    pub automation_enabled: bool,
    pub sandbox_profile: String,
    pub sandbox_enabled: bool,
    pub allow_network: bool,
    pub external_data_policy: String,
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

// ============================================================
// 핸들러
// ============================================================

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
        Ok(Json(AutomationStatusDto {
            enabled: false,
            sandbox_enabled: false,
            sandbox_profile: "Standard".to_string(),
            ocr_provider: "Local".to_string(),
            llm_provider: "Local".to_string(),
            external_data_policy: "PiiFilterStrict".to_string(),
            pending_audit_entries: pending,
        }))
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
        let status = match status_filter.as_str() {
            "Started" => AuditStatus::Started,
            "Completed" => AuditStatus::Completed,
            "Failed" => AuditStatus::Failed,
            "Denied" => AuditStatus::Denied,
            "Timeout" => AuditStatus::Timeout,
            _ => {
                return Err(ApiError::BadRequest(format!(
                    "유효하지 않은 상태 필터: {}",
                    status_filter
                )))
            }
        };
        guard.entries_by_status(&status, query.limit)
    } else {
        guard.recent_entries(query.limit)
    };

    let dtos = entries
        .into_iter()
        .map(|e| AuditEntryDto {
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
        Ok(Json(PoliciesDto {
            automation_enabled: config.automation.enabled,
            sandbox_profile: format!("{:?}", config.automation.sandbox.profile),
            sandbox_enabled: config.automation.sandbox.enabled,
            allow_network: config.automation.sandbox.allow_network,
            external_data_policy: format!("{:?}", config.ai_provider.external_data_policy),
        }))
    } else {
        Ok(Json(PoliciesDto {
            automation_enabled: false,
            sandbox_profile: "Standard".to_string(),
            sandbox_enabled: false,
            allow_network: false,
            external_data_policy: "PiiFilterStrict".to_string(),
        }))
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

    Ok(Json(AutomationStatsDto {
        total_executions: total,
        successful: success,
        failed,
        denied,
        timeout,
        avg_elapsed_ms: avg_elapsed,
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

    let Some(ref config_manager) = state.config_manager else {
        return Err(ApiError::Internal("설정 관리자 미설정".into()));
    };

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
    let Some(ref config_manager) = state.config_manager else {
        return Err(ApiError::Internal("설정 관리자 미설정".into()));
    };

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
    let Some(ref config_manager) = state.config_manager else {
        return Err(ApiError::Internal("설정 관리자 미설정".into()));
    };

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
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("total_executions"));
        assert!(json.contains("avg_elapsed_ms"));
    }

    #[test]
    fn policies_dto_serializes() {
        let dto = PoliciesDto {
            automation_enabled: true,
            sandbox_profile: "Strict".to_string(),
            sandbox_enabled: true,
            allow_network: false,
            external_data_policy: "PiiFilterStrict".to_string(),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("Strict"));
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
}
