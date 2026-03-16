use std::sync::Arc;

use oneshim_api_contracts::integration::{
    IntegrationAckCursorSummary, IntegrationAuditLogResponse, IntegrationAuditRecordSummary,
    IntegrationDeviceAuthorizationCommandResult, IntegrationInboxActionResponse,
    IntegrationInboxDismissRequest, IntegrationInboxPromptSummary, IntegrationInboxRefreshResponse,
    IntegrationInboxResponse, IntegrationSessionSummary, IntegrationStatus,
};
use oneshim_core::models::integration::{
    default_integration_runtime_scopes, IntegrationInboxItemStatus, IntegrationInsightAuditRecord,
    IntegrationSessionState, ProactivePromptCategory, ProactivePromptPriority,
    StoredProactivePrompt,
};
use tracing::warn;

use crate::{error::ApiError, AppState};

pub(crate) const INTEGRATION_STATUS_SCHEMA_VERSION: &str = "integration.status.v1";
pub(crate) const INTEGRATION_AUDIT_SCHEMA_VERSION: &str = "integration.audit.v1";
pub(crate) const INTEGRATION_INBOX_SCHEMA_VERSION: &str = "integration.inbox.v1";
pub(crate) const INTEGRATION_INBOX_ACTION_SCHEMA_VERSION: &str = "integration.inbox-action.v1";

fn map_session_summary(state: IntegrationSessionState) -> IntegrationSessionSummary {
    IntegrationSessionSummary {
        status: state.status,
        transport_kind: state.transport_kind,
        auth_scheme: state.auth_scheme,
        connected_at: state.connected_at,
        last_heartbeat_at: state.last_heartbeat_at,
        requested_scopes: state
            .requested_scopes
            .iter()
            .map(|scope| scope.as_str().to_string())
            .collect(),
        granted_scopes: state
            .granted_scopes
            .iter()
            .map(|scope| scope.as_str().to_string())
            .collect(),
    }
}

fn map_ack_cursor_summary(
    cursor: oneshim_core::models::integration::IntegrationAckCursor,
) -> IntegrationAckCursorSummary {
    IntegrationAckCursorSummary {
        stream_id: cursor.stream_id,
        cursor: cursor.cursor,
        acknowledged_at: cursor.acknowledged_at,
    }
}

fn map_audit_record(record: IntegrationInsightAuditRecord) -> IntegrationAuditRecordSummary {
    IntegrationAuditRecordSummary {
        record_id: record.record_id,
        envelope_id: record.envelope_id,
        packet_id: record.packet_id,
        disposition: match record.disposition {
            oneshim_core::models::integration::IntegrationEgressDisposition::Allow => "allow",
            oneshim_core::models::integration::IntegrationEgressDisposition::Deny => "deny",
            oneshim_core::models::integration::IntegrationEgressDisposition::RequireUserApproval => {
                "require_user_approval"
            }
        }
        .to_string(),
        reason: record.reason,
        privacy_classification: match record.privacy_classification {
            oneshim_core::models::integration::IntegrationPrivacyClassification::DeviceLocal => {
                "device_local"
            }
            oneshim_core::models::integration::IntegrationPrivacyClassification::DerivedSummary => {
                "derived_summary"
            }
            oneshim_core::models::integration::IntegrationPrivacyClassification::UserApprovedAttachment => {
                "user_approved_attachment"
            }
        }
        .to_string(),
        capability_scope: record.capability_scope.as_str().to_string(),
        occurred_at: record.occurred_at,
    }
}

fn prompt_category_label(category: &ProactivePromptCategory) -> &'static str {
    match category {
        ProactivePromptCategory::Insight => "insight",
        ProactivePromptCategory::Task => "task",
        ProactivePromptCategory::Reminder => "reminder",
        ProactivePromptCategory::Escalation => "escalation",
    }
}

fn prompt_priority_label(priority: &ProactivePromptPriority) -> &'static str {
    match priority {
        ProactivePromptPriority::Low => "low",
        ProactivePromptPriority::Medium => "medium",
        ProactivePromptPriority::High => "high",
        ProactivePromptPriority::Critical => "critical",
    }
}

fn inbox_status_label(status: &IntegrationInboxItemStatus) -> &'static str {
    match status {
        IntegrationInboxItemStatus::Pending => "pending",
        IntegrationInboxItemStatus::Acknowledged => "acknowledged",
        IntegrationInboxItemStatus::Dismissed => "dismissed",
        IntegrationInboxItemStatus::Expired => "expired",
    }
}

fn map_prompt(prompt: StoredProactivePrompt) -> IntegrationInboxPromptSummary {
    IntegrationInboxPromptSummary {
        prompt_id: prompt.prompt.prompt_id,
        category: prompt_category_label(&prompt.prompt.category).to_string(),
        priority: prompt_priority_label(&prompt.prompt.priority).to_string(),
        title: prompt.prompt.title,
        body: prompt.prompt.body,
        status: inbox_status_label(&prompt.status).to_string(),
        received_at: prompt.received_at,
        status_updated_at: prompt.status_updated_at,
        presented_at: prompt.presented_at,
        expires_at: prompt.prompt.expires_at,
        source_system: prompt.prompt.provenance.source_system,
        source_actor: prompt.prompt.provenance.source_actor,
        correlation_id: prompt.prompt.provenance.correlation_id,
        dismiss_reason: prompt.dismiss_reason,
    }
}

fn integration_inbox(
    state: &AppState,
) -> Result<Arc<dyn oneshim_core::ports::integration::IntegrationInboxPort>, ApiError> {
    state.integration_inbox.clone().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration inbox runtime is not configured.".to_string())
    })
}

fn integration_auth(
    state: &AppState,
) -> Result<Arc<dyn oneshim_core::ports::integration::IntegrationAuthPort>, ApiError> {
    state.integration_auth.clone().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration auth runtime is not configured.".to_string())
    })
}

pub async fn build_status(state: &AppState) -> IntegrationStatus {
    let config = state
        .config_manager
        .as_ref()
        .map(|config_manager| config_manager.get());
    let external_access_enabled = config
        .as_ref()
        .map(|config| config.web.allow_external)
        .unwrap_or(false);
    let mut outbound_runtime = state.integration_runtime_status.clone().unwrap_or_default();

    if let Some(config) = config.as_ref() {
        let auth_token_env_var = config
            .integration
            .auth_token_env_var
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        outbound_runtime.enabled = config.integration.enabled;
        outbound_runtime.bootstrap_configured = config
            .integration
            .bootstrap_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        outbound_runtime.auth_profile_kind = config.integration.auth_profile_kind.clone();
        outbound_runtime.auth_source_configured = match config.integration.auth_profile_kind {
            oneshim_core::models::integration::IntegrationAuthProfileKind::EnvToken => {
                auth_token_env_var.is_some()
            }
            oneshim_core::models::integration::IntegrationAuthProfileKind::OidcDeviceFlow => {
                config
                    .integration
                    .oidc_device_flow
                    .client_id
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|value| !value.is_empty())
                    && config
                        .integration
                        .oidc_device_flow
                        .device_authorization_url
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|value| !value.is_empty())
                    && config
                        .integration
                        .oidc_device_flow
                        .token_url
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|value| !value.is_empty())
            }
        };
        outbound_runtime.auth_material_available = auth_token_env_var
            .and_then(|env_var| std::env::var(env_var).ok())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        outbound_runtime.resource_indicator_configured = config
            .integration
            .resource_indicator
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
    }

    if let Some(auth_port) = state.integration_auth.as_ref() {
        match auth_port.current_auth_status().await {
            Ok(auth_status) => {
                outbound_runtime.auth_material_available = auth_status.authenticated;
                outbound_runtime.auth_status = Some(auth_status);
            }
            Err(error) => {
                warn!(error = %error, "failed to read integration auth status");
            }
        }
    }

    if let Some(session_port) = state.integration_session.as_ref() {
        match session_port.current_session().await {
            Ok(Some(current_session)) => {
                outbound_runtime.current_session = Some(map_session_summary(current_session));
            }
            Ok(None) => {}
            Err(error) => {
                warn!(error = %error, "failed to read integration session state");
            }
        }
    }

    if let Some(outbox) = state.integration_outbox.as_ref() {
        match outbox.pending_count().await {
            Ok(count) => outbound_runtime.outbox_pending_count = Some(count),
            Err(error) => warn!(error = %error, "failed to read integration outbox count"),
        }
        match outbox.last_ack_cursor().await {
            Ok(cursor) => outbound_runtime.outbox_ack_cursor = cursor.map(map_ack_cursor_summary),
            Err(error) => warn!(error = %error, "failed to read integration outbox cursor"),
        }
    }

    if let Some(inbox_store) = state.integration_inbox_store.as_ref() {
        match inbox_store.pending_count().await {
            Ok(count) => outbound_runtime.inbox_pending_count = Some(count),
            Err(error) => warn!(error = %error, "failed to read integration inbox count"),
        }
        match inbox_store.last_ack_cursor().await {
            Ok(cursor) => outbound_runtime.inbox_ack_cursor = cursor.map(map_ack_cursor_summary),
            Err(error) => warn!(error = %error, "failed to read integration inbox cursor"),
        }
    }

    IntegrationStatus {
        schema_version: INTEGRATION_STATUS_SCHEMA_VERSION.to_string(),
        external_access_enabled,
        automation_controller_configured: state.automation_controller.is_some(),
        ai_runtime_status: state.ai_runtime_status.clone(),
        outbound_runtime,
    }
}

pub async fn build_audit_log(state: &AppState, limit: usize) -> IntegrationAuditLogResponse {
    let records = if let Some(audit) = state.integration_audit.as_ref() {
        match audit.recent_insight_decisions(limit).await {
            Ok(records) => records.into_iter().map(map_audit_record).collect(),
            Err(error) => {
                warn!(error = %error, "failed to read integration audit records");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    IntegrationAuditLogResponse {
        schema_version: INTEGRATION_AUDIT_SCHEMA_VERSION.to_string(),
        records,
    }
}

pub async fn list_inbox(state: &AppState) -> Result<IntegrationInboxResponse, ApiError> {
    let prompts = integration_inbox(state)?
        .list_pending()
        .await?
        .into_iter()
        .map(map_prompt)
        .collect::<Vec<_>>();
    let pending_count = prompts.len();

    Ok(IntegrationInboxResponse {
        schema_version: INTEGRATION_INBOX_SCHEMA_VERSION.to_string(),
        prompts,
        pending_count,
    })
}

pub async fn refresh_inbox(state: &AppState) -> Result<IntegrationInboxRefreshResponse, ApiError> {
    Ok(IntegrationInboxRefreshResponse {
        schema_version: INTEGRATION_INBOX_SCHEMA_VERSION.to_string(),
        fetched_count: integration_inbox(state)?.refresh().await?,
    })
}

pub async fn acknowledge_inbox_prompt(
    state: &AppState,
    prompt_id: &str,
) -> Result<IntegrationInboxActionResponse, ApiError> {
    integration_inbox(state)?.acknowledge(prompt_id).await?;
    Ok(IntegrationInboxActionResponse {
        schema_version: INTEGRATION_INBOX_ACTION_SCHEMA_VERSION.to_string(),
        prompt_id: prompt_id.to_string(),
        status: "acknowledged".to_string(),
    })
}

pub async fn dismiss_inbox_prompt(
    state: &AppState,
    prompt_id: &str,
    request: IntegrationInboxDismissRequest,
) -> Result<IntegrationInboxActionResponse, ApiError> {
    integration_inbox(state)?
        .dismiss(prompt_id, request.reason)
        .await?;
    Ok(IntegrationInboxActionResponse {
        schema_version: INTEGRATION_INBOX_ACTION_SCHEMA_VERSION.to_string(),
        prompt_id: prompt_id.to_string(),
        status: "dismissed".to_string(),
    })
}

pub async fn get_auth_status(
    state: &AppState,
) -> Result<oneshim_core::models::integration::IntegrationAuthStatus, ApiError> {
    Ok(integration_auth(state)?.current_auth_status().await?)
}

pub async fn start_device_authorization(
    state: &AppState,
) -> Result<IntegrationDeviceAuthorizationCommandResult, ApiError> {
    let auth = integration_auth(state)?;
    let flow = auth
        .start_device_authorization(&default_integration_runtime_scopes(), None)
        .await?;
    let auth_status = auth.current_auth_status().await?;
    Ok(IntegrationDeviceAuthorizationCommandResult {
        auth_status,
        flow: Some(flow),
    })
}

pub async fn poll_device_authorization(
    state: &AppState,
    flow_id: &str,
) -> Result<IntegrationDeviceAuthorizationCommandResult, ApiError> {
    let auth_status = integration_auth(state)?
        .poll_device_authorization(flow_id)
        .await?;
    Ok(IntegrationDeviceAuthorizationCommandResult {
        flow: auth_status.pending_flow.clone(),
        auth_status,
    })
}

pub async fn cancel_device_authorization(
    state: &AppState,
    flow_id: &str,
) -> Result<IntegrationDeviceAuthorizationCommandResult, ApiError> {
    let auth = integration_auth(state)?;
    auth.cancel_device_authorization(flow_id).await?;
    let auth_status = auth.current_auth_status().await?;
    Ok(IntegrationDeviceAuthorizationCommandResult {
        flow: auth_status.pending_flow.clone(),
        auth_status,
    })
}
