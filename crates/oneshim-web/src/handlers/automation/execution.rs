use axum::{
    extract::{Path, Query, State},
    Json,
};

use oneshim_api_contracts::automation::{
    AuditQuery, ExecuteIntentHintRequest, ExecuteSceneActionRequest, PolicyEventQuery,
};

use crate::error::ApiError;
use crate::services::automation_service::{AutomationCommandService, AutomationQueryService};
use crate::services::web_contexts::AutomationWebContext;

pub async fn get_contract_versions(
) -> Result<Json<oneshim_api_contracts::automation::AutomationContractsDto>, ApiError> {
    Ok(Json(AutomationQueryService::contract_versions()))
}

pub async fn get_automation_status(
    State(context): State<AutomationWebContext>,
) -> Result<Json<oneshim_api_contracts::automation::AutomationStatusDto>, ApiError> {
    Ok(Json(
        AutomationQueryService::new(context)
            .automation_status()
            .await?,
    ))
}

pub async fn get_audit_logs(
    State(context): State<AutomationWebContext>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<oneshim_api_contracts::automation::AuditEntryDto>>, ApiError> {
    Ok(Json(
        AutomationQueryService::new(context)
            .audit_logs(query)
            .await?,
    ))
}

pub async fn get_policy_events(
    State(context): State<AutomationWebContext>,
    Query(query): Query<PolicyEventQuery>,
) -> Result<Json<Vec<oneshim_api_contracts::automation::AuditEntryDto>>, ApiError> {
    Ok(Json(
        AutomationQueryService::new(context)
            .policy_events(query)
            .await?,
    ))
}

pub async fn get_policies(
    State(context): State<AutomationWebContext>,
) -> Result<Json<oneshim_api_contracts::automation::PoliciesDto>, ApiError> {
    Ok(Json(AutomationQueryService::new(context).policies()))
}

pub async fn get_automation_stats(
    State(context): State<AutomationWebContext>,
) -> Result<Json<oneshim_api_contracts::automation::AutomationStatsDto>, ApiError> {
    Ok(Json(
        AutomationQueryService::new(context)
            .automation_stats()
            .await,
    ))
}

pub async fn list_presets(
    State(context): State<AutomationWebContext>,
) -> Result<Json<oneshim_api_contracts::automation::PresetListDto>, ApiError> {
    Ok(Json(AutomationQueryService::new(context).list_presets()))
}

pub async fn create_preset(
    State(context): State<AutomationWebContext>,
    Json(preset): Json<oneshim_core::models::intent::WorkflowPreset>,
) -> Result<Json<oneshim_core::models::intent::WorkflowPreset>, ApiError> {
    Ok(Json(
        AutomationCommandService::new(context).create_preset(preset)?,
    ))
}

pub async fn update_preset(
    State(context): State<AutomationWebContext>,
    Path(id): Path<String>,
    Json(preset): Json<oneshim_core::models::intent::WorkflowPreset>,
) -> Result<Json<oneshim_core::models::intent::WorkflowPreset>, ApiError> {
    Ok(Json(
        AutomationCommandService::new(context).update_preset(id, preset)?,
    ))
}

pub async fn delete_preset(
    State(context): State<AutomationWebContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        AutomationCommandService::new(context).delete_preset(id)?,
    ))
}

pub async fn run_preset(
    State(context): State<AutomationWebContext>,
    Path(id): Path<String>,
) -> Result<Json<oneshim_api_contracts::automation::PresetRunResult>, ApiError> {
    Ok(Json(
        AutomationCommandService::new(context)
            .run_preset(id)
            .await?,
    ))
}

pub async fn execute_intent_hint(
    State(context): State<AutomationWebContext>,
    Json(req): Json<ExecuteIntentHintRequest>,
) -> Result<Json<oneshim_api_contracts::automation::ExecuteIntentHintResponse>, ApiError> {
    Ok(Json(
        AutomationCommandService::new(context)
            .execute_intent_hint(req)
            .await?,
    ))
}

pub async fn execute_scene_action(
    State(context): State<AutomationWebContext>,
    Json(req): Json<ExecuteSceneActionRequest>,
) -> Result<Json<oneshim_api_contracts::automation::ExecuteSceneActionResponse>, ApiError> {
    Ok(Json(
        AutomationCommandService::new(context)
            .execute_scene_action(req)
            .await?,
    ))
}
