use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;

use oneshim_api_contracts::automation::{
    AuditEntryDto, AuditQuery, AutomationContractsDto, AutomationStatsDto, AutomationStatusDto,
    ExecuteIntentHintRequest, ExecuteIntentHintResponse, ExecuteSceneActionRequest,
    ExecuteSceneActionResponse, PoliciesDto, PolicyEventQuery, PresetListDto, PresetRunResult,
};
use oneshim_automation::policy::AuditLevel;
use oneshim_core::models::intent::builtin_presets;
use oneshim_core::error::CoreError;
use oneshim_core::models::intent::{IntentCommand, IntentResult, WorkflowPreset};
use oneshim_core::models::ui_scene::UI_SCENE_SCHEMA_VERSION;
use std::time::Instant;

use crate::{error::ApiError, AppState};

use super::helpers::{
    build_scene_action_intents, default_automation_status, default_policies,
    enforce_scene_action_privacy, evaluate_scene_action_override, parse_audit_status,
    read_scene_intelligence_config, require_config_manager, resolve_ai_runtime_status,
    AUTOMATION_AUDIT_SCHEMA_VERSION, AUTOMATION_SCENE_ACTION_SCHEMA_VERSION,
};

pub async fn get_contract_versions() -> Result<Json<AutomationContractsDto>, ApiError> {
    Ok(Json(AutomationContractsDto {
        audit_schema_version: AUTOMATION_AUDIT_SCHEMA_VERSION.to_string(),
        scene_schema_version: UI_SCENE_SCHEMA_VERSION.to_string(),
        scene_action_schema_version: AUTOMATION_SCENE_ACTION_SCHEMA_VERSION.to_string(),
    }))
}

pub async fn get_automation_status(
    State(state): State<AppState>,
) -> Result<Json<AutomationStatusDto>, ApiError> {
    let pending = if let Some(ref logger) = state.audit_logger {
        logger.pending_count().await
    } else {
        0
    };

    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        let runtime_status = resolve_ai_runtime_status(
            &state,
            config.ai_provider.access_mode,
            config.ai_provider.ocr_provider,
            config.ai_provider.llm_provider,
        );
        Ok(Json(AutomationStatusDto {
            enabled: config.automation.enabled,
            sandbox_enabled: config.automation.sandbox.enabled,
            sandbox_profile: format!("{:?}", config.automation.sandbox.profile),
            ocr_provider: format!("{:?}", config.ai_provider.ocr_provider),
            llm_provider: format!("{:?}", config.ai_provider.llm_provider),
            ocr_source: runtime_status.ocr_source,
            llm_source: runtime_status.llm_source,
            ocr_fallback_reason: runtime_status.ocr_fallback_reason,
            llm_fallback_reason: runtime_status.llm_fallback_reason,
            external_data_policy: format!("{:?}", config.ai_provider.external_data_policy),
            pending_audit_entries: pending,
        }))
    } else {
        Ok(Json(default_automation_status(pending)))
    }
}

pub async fn get_audit_logs(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntryDto>>, ApiError> {
    let Some(ref logger) = state.audit_logger else {
        return Ok(Json(Vec::new()));
    };

    let entries = if let Some(ref status_filter) = query.status {
        let status = parse_audit_status(status_filter)?;
        logger.entries_by_status(&status, query.limit).await
    } else {
        logger.recent_entries(query.limit).await
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

pub async fn get_policy_events(
    State(state): State<AppState>,
    Query(query): Query<PolicyEventQuery>,
) -> Result<Json<Vec<AuditEntryDto>>, ApiError> {
    let Some(ref logger) = state.audit_logger else {
        return Ok(Json(Vec::new()));
    };

    let limit = query.limit.clamp(1, 500);
    let read_limit = limit.saturating_mul(8);
    let entries = logger
        .recent_entries(read_limit)
        .await
        .into_iter()
        .filter(|entry| entry.action_type.starts_with("policy."))
        .take(limit)
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

    Ok(Json(entries))
}

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

    let stats = logger.stats().await;
    let (total, success, failed, denied, timeout) =
        (stats.total, stats.completed, stats.failed, stats.denied, stats.timeout);

    let all_entries = logger.recent_entries(1000).await;
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

pub async fn list_presets(State(state): State<AppState>) -> Result<Json<PresetListDto>, ApiError> {
    let mut presets = builtin_presets();

    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        presets.extend(config.automation.custom_presets.clone());
    }

    Ok(Json(PresetListDto { presets }))
}

pub async fn create_preset(
    State(state): State<AppState>,
    Json(preset): Json<WorkflowPreset>,
) -> Result<Json<WorkflowPreset>, ApiError> {
    if preset.id.is_empty() || preset.name.is_empty() {
        return Err(ApiError::BadRequest(
            "Preset ID and name are required".into(),
        ));
    }
    if preset.steps.is_empty() {
        return Err(ApiError::BadRequest("At least one step is required".into()));
    }

    let config_manager = require_config_manager(&state)?;

    config_manager
        .update_with(|config| {
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
        .map_err(|e| ApiError::Internal(format!("Failed to save preset: {e}")))?;

    Ok(Json(preset))
}

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
        .map_err(|e| ApiError::Internal(format!("Failed to update preset: {e}")))?;

    if !found {
        return Err(ApiError::NotFound(format!("Preset '{}' not found", id)));
    }

    Ok(Json(preset))
}

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
        .map_err(|e| ApiError::Internal(format!("Failed to delete preset: {e}")))?;

    if !found {
        return Err(ApiError::NotFound(format!("Preset '{}' not found", id)));
    }

    Ok(Json(serde_json::json!({ "deleted": id })))
}

pub async fn run_preset(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PresetRunResult>, ApiError> {
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
        return Err(ApiError::NotFound(format!("Preset '{}' not found", id)));
    };

    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        if !config.automation.enabled {
            return Err(ApiError::BadRequest(
                "자동화가 비active화 state입니다".to_string(),
            ));
        }
    }

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
            Err(e) => Err(ApiError::Internal(format!("execution failure: {}", e))),
        }
    } else {
        tracing::info!(
            preset_id = %preset.id,
            steps = preset.steps.len(),
            "workflow preset execution requested (controller not configured, logging only)"
        );

        Ok(Json(PresetRunResult {
            preset_id: id,
            success: true,
            message: format!(
                "preset '{}' execution requested ({} steps, logging only)",
                preset.name,
                preset.steps.len()
            ),
            steps_executed: None,
            total_steps: Some(preset.steps.len()),
            total_elapsed_ms: None,
        }))
    }
}

pub async fn execute_intent_hint(
    State(state): State<AppState>,
    Json(req): Json<ExecuteIntentHintRequest>,
) -> Result<Json<ExecuteIntentHintResponse>, ApiError> {
    if req.session_id.trim().is_empty() {
        return Err(ApiError::BadRequest("session_id is required".to_string()));
    }
    if req.intent_hint.trim().is_empty() {
        return Err(ApiError::BadRequest("intent_hint is required".to_string()));
    }

    let Some(ref controller) = state.automation_controller else {
        return Err(ApiError::BadRequest(
            "자동화 컨트롤러가 active화되지 않았습니다".to_string(),
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
        Err(e) => Err(ApiError::Internal(format!(
            "Natural language intent execution failed: {e}"
        ))),
    }
}

pub async fn execute_scene_action(
    State(state): State<AppState>,
    Json(req): Json<ExecuteSceneActionRequest>,
) -> Result<Json<ExecuteSceneActionResponse>, ApiError> {
    let Some(ref controller) = state.automation_controller else {
        return Err(ApiError::BadRequest(
            "자동화 컨트롤러가 active화되지 않았습니다".to_string(),
        ));
    };

    let scene_cfg = read_scene_intelligence_config(&state);
    if !scene_cfg.enabled {
        return Err(ApiError::BadRequest(
            "Scene intelligence가 비active화되어 있습니다.".to_string(),
        ));
    }
    if !scene_cfg.allow_action_execution {
        return Err(ApiError::BadRequest(
            "Scene action execution이 설정에서 비active화되어 있습니다.".to_string(),
        ));
    }

    let intents = build_scene_action_intents(&req)?;
    let policy_context = match enforce_scene_action_privacy(&state, &req) {
        Ok(context) => context,
        Err(err) => {
            if let Some(logger) = state.audit_logger.as_ref() {
                logger.log_event(
                    "policy.scene_action.blocked",
                    &req.session_id,
                    &format!(
                        "action_type={:?} element_id={} allow_sensitive_input={} error={}",
                        req.action_type,
                        req.element_id,
                        req.allow_sensitive_input.unwrap_or(false),
                        err
                    ),
                ).await;
            }
            return Err(err);
        }
    };
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
        if policy_context.override_active {
            logger.log_event(
                "policy.scene_action_override.applied",
                &req.session_id,
                &format!(
                    "action_type={:?} element_id={} approved_by={:?} expires_at={:?}",
                    req.action_type,
                    req.element_id,
                    policy_context.override_approved_by.as_deref(),
                    policy_context
                        .override_expires_at
                        .as_ref()
                        .map(|value| value.to_rfc3339()),
                ),
            ).await;
        } else if policy_context.override_enabled || policy_context.override_issue.is_some() {
            logger.log_event(
                "policy.scene_action_override.issue",
                &req.session_id,
                &format!(
                    "action_type={:?} element_id={} issue={:?} enabled={} expires_at={:?}",
                    req.action_type,
                    req.element_id,
                    policy_context.override_issue.as_deref(),
                    policy_context.override_enabled,
                    policy_context
                        .override_expires_at
                        .as_ref()
                        .map(|value| value.to_rfc3339()),
                ),
            ).await;
        }
        if req.allow_sensitive_input.unwrap_or(false) {
            logger.log_event(
                "policy.scene_action.allow_sensitive_input",
                &req.session_id,
                &format!(
                    "action_type={:?} element_id={} policy={:?} override_active={}",
                    req.action_type,
                    req.element_id,
                    policy_context.policy,
                    policy_context.override_active,
                ),
            ).await;
        }
        logger.log_start_if(
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
        ).await;
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
            Err(e) => {
                return Err(ApiError::Internal(format!(
                    "Scene action execution failed: {e}"
                )))
            }
        }
    }

    let result = last_result.unwrap_or(IntentResult {
        success: false,
        element: None,
        verification: None,
        retry_count: 0,
        elapsed_ms: 0,
        error: Some("execution 가능한 액션이 없습니다".to_string()),
    });
    let elapsed_ms = started_at.elapsed().as_millis() as u64;

    if let Some(logger) = state.audit_logger.as_ref() {
        logger.log_complete_with_time(
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
        ).await;
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
