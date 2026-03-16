use std::sync::Arc;

use oneshim_api_contracts::integration::{
    IntegrationAckCursorSummary, IntegrationAuditLogResponse, IntegrationAuditRecordSummary,
    IntegrationDeviceAuthorizationCommandResult, IntegrationInboxActionResponse,
    IntegrationInboxDismissRequest, IntegrationInboxPromptSummary, IntegrationInboxRefreshResponse,
    IntegrationInboxResponse, IntegrationOutboundRuntimeStatus, IntegrationSessionSummary,
    IntegrationStatus,
};
use oneshim_core::models::integration::{
    default_integration_runtime_scopes, IntegrationAuthProfileKind, IntegrationInboxItemStatus,
    IntegrationInsightAuditRecord, IntegrationSessionState, ProactivePromptCategory,
    ProactivePromptPriority, StoredProactivePrompt,
};
use tracing::warn;

use crate::{error::ApiError, AppState};

pub(crate) const INTEGRATION_STATUS_SCHEMA_VERSION: &str = "integration.status.v1";
pub(crate) const INTEGRATION_AUDIT_SCHEMA_VERSION: &str = "integration.audit.v1";
pub(crate) const INTEGRATION_INBOX_SCHEMA_VERSION: &str = "integration.inbox.v1";
pub(crate) const INTEGRATION_INBOX_ACTION_SCHEMA_VERSION: &str = "integration.inbox-action.v1";

#[derive(Debug, Clone, Default)]
struct IntegrationStatusConfigSnapshot {
    present: bool,
    external_access_enabled: bool,
    integration_enabled: bool,
    bootstrap_configured: bool,
    auth_profile_kind: IntegrationAuthProfileKind,
    auth_source_configured: bool,
    auth_token_env_var: Option<String>,
    resource_indicator_configured: bool,
}

impl IntegrationStatusConfigSnapshot {
    fn from_state(state: &AppState) -> Self {
        let Some(config) = state
            .config_manager
            .as_ref()
            .map(|config_manager| config_manager.get())
        else {
            return Self::default();
        };

        let auth_token_env_var = config
            .integration
            .auth_token_env_var
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let auth_source_configured = match config.integration.auth_profile_kind {
            IntegrationAuthProfileKind::EnvToken => auth_token_env_var.is_some(),
            IntegrationAuthProfileKind::OidcDeviceFlow => {
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

        Self {
            present: true,
            external_access_enabled: config.web.allow_external,
            integration_enabled: config.integration.enabled,
            bootstrap_configured: config
                .integration
                .bootstrap_url
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            auth_profile_kind: config.integration.auth_profile_kind.clone(),
            auth_source_configured,
            auth_token_env_var,
            resource_indicator_configured: config
                .integration
                .resource_indicator
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
        }
    }

    fn apply_to_runtime_status(&self, outbound_runtime: &mut IntegrationOutboundRuntimeStatus) {
        if !self.present {
            return;
        }
        outbound_runtime.enabled = self.integration_enabled;
        outbound_runtime.bootstrap_configured = self.bootstrap_configured;
        outbound_runtime.auth_profile_kind = self.auth_profile_kind.clone();
        outbound_runtime.auth_source_configured = self.auth_source_configured;
        outbound_runtime.auth_material_available = self
            .auth_token_env_var
            .as_deref()
            .and_then(|env_var| std::env::var(env_var).ok())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        outbound_runtime.resource_indicator_configured = self.resource_indicator_configured;
    }
}

#[derive(Clone)]
pub struct IntegrationWebContext {
    config: IntegrationStatusConfigSnapshot,
    automation_controller_configured: bool,
    ai_runtime_status: Option<crate::AiRuntimeStatus>,
    runtime_status_seed: IntegrationOutboundRuntimeStatus,
    auth: Option<Arc<dyn oneshim_core::ports::integration::IntegrationAuthPort>>,
    session: Option<Arc<dyn oneshim_core::ports::integration::IntegrationSessionPort>>,
    outbox: Option<Arc<dyn oneshim_core::ports::integration::IntegrationOutboxPort>>,
    inbox: Option<Arc<dyn oneshim_core::ports::integration::IntegrationInboxPort>>,
    inbox_store: Option<Arc<dyn oneshim_core::ports::integration::IntegrationInboxStorePort>>,
    audit: Option<Arc<dyn oneshim_core::ports::integration::IntegrationAuditPort>>,
}

impl IntegrationWebContext {
    pub fn from_state(state: &AppState) -> Self {
        Self {
            config: IntegrationStatusConfigSnapshot::from_state(state),
            automation_controller_configured: state.automation_controller.is_some(),
            ai_runtime_status: state.ai_runtime_status.clone(),
            runtime_status_seed: state.integration_runtime_status.clone().unwrap_or_default(),
            auth: state.integration_auth.clone(),
            session: state.integration_session.clone(),
            outbox: state.integration_outbox.clone(),
            inbox: state.integration_inbox.clone(),
            inbox_store: state.integration_inbox_store.clone(),
            audit: state.integration_audit.clone(),
        }
    }

    pub fn status_queries(&self) -> IntegrationStatusQueryService {
        IntegrationStatusQueryService { ctx: self.clone() }
    }

    pub fn audit_queries(&self) -> IntegrationAuditQueryService {
        IntegrationAuditQueryService { ctx: self.clone() }
    }

    pub fn inbox_commands(&self) -> IntegrationInboxCommandService {
        IntegrationInboxCommandService { ctx: self.clone() }
    }

    pub fn auth_commands(&self) -> IntegrationAuthCommandService {
        IntegrationAuthCommandService { ctx: self.clone() }
    }

    fn inbox(
        &self,
    ) -> Result<Arc<dyn oneshim_core::ports::integration::IntegrationInboxPort>, ApiError> {
        self.inbox.clone().ok_or_else(|| {
            ApiError::ServiceUnavailable("Integration inbox runtime is not configured.".to_string())
        })
    }

    fn auth(
        &self,
    ) -> Result<Arc<dyn oneshim_core::ports::integration::IntegrationAuthPort>, ApiError> {
        self.auth.clone().ok_or_else(|| {
            ApiError::ServiceUnavailable("Integration auth runtime is not configured.".to_string())
        })
    }
}

#[derive(Clone)]
pub struct IntegrationStatusQueryService {
    ctx: IntegrationWebContext,
}

impl IntegrationStatusQueryService {
    pub async fn build_status(&self) -> IntegrationStatus {
        let mut outbound_runtime = self.ctx.runtime_status_seed.clone();
        self.ctx
            .config
            .apply_to_runtime_status(&mut outbound_runtime);

        if let Some(auth_port) = self.ctx.auth.as_ref() {
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

        if let Some(session_port) = self.ctx.session.as_ref() {
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

        if let Some(outbox) = self.ctx.outbox.as_ref() {
            match outbox.pending_count().await {
                Ok(count) => outbound_runtime.outbox_pending_count = Some(count),
                Err(error) => warn!(error = %error, "failed to read integration outbox count"),
            }
            match outbox.last_ack_cursor().await {
                Ok(cursor) => {
                    outbound_runtime.outbox_ack_cursor = cursor.map(map_ack_cursor_summary)
                }
                Err(error) => warn!(error = %error, "failed to read integration outbox cursor"),
            }
        }

        if let Some(inbox_store) = self.ctx.inbox_store.as_ref() {
            match inbox_store.pending_count().await {
                Ok(count) => outbound_runtime.inbox_pending_count = Some(count),
                Err(error) => warn!(error = %error, "failed to read integration inbox count"),
            }
            match inbox_store.last_ack_cursor().await {
                Ok(cursor) => {
                    outbound_runtime.inbox_ack_cursor = cursor.map(map_ack_cursor_summary)
                }
                Err(error) => warn!(error = %error, "failed to read integration inbox cursor"),
            }
        }

        IntegrationStatus {
            schema_version: INTEGRATION_STATUS_SCHEMA_VERSION.to_string(),
            external_access_enabled: self.ctx.config.external_access_enabled,
            automation_controller_configured: self.ctx.automation_controller_configured,
            ai_runtime_status: self.ctx.ai_runtime_status.clone(),
            outbound_runtime,
        }
    }
}

#[derive(Clone)]
pub struct IntegrationAuditQueryService {
    ctx: IntegrationWebContext,
}

impl IntegrationAuditQueryService {
    pub async fn build_audit_log(&self, limit: usize) -> IntegrationAuditLogResponse {
        let records = if let Some(audit) = self.ctx.audit.as_ref() {
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
}

#[derive(Clone)]
pub struct IntegrationInboxCommandService {
    ctx: IntegrationWebContext,
}

impl IntegrationInboxCommandService {
    pub async fn list_inbox(&self) -> Result<IntegrationInboxResponse, ApiError> {
        let prompts = self
            .ctx
            .inbox()?
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

    pub async fn refresh_inbox(&self) -> Result<IntegrationInboxRefreshResponse, ApiError> {
        Ok(IntegrationInboxRefreshResponse {
            schema_version: INTEGRATION_INBOX_SCHEMA_VERSION.to_string(),
            fetched_count: self.ctx.inbox()?.refresh().await?,
        })
    }

    pub async fn acknowledge_inbox_prompt(
        &self,
        prompt_id: &str,
    ) -> Result<IntegrationInboxActionResponse, ApiError> {
        self.ctx.inbox()?.acknowledge(prompt_id).await?;
        Ok(IntegrationInboxActionResponse {
            schema_version: INTEGRATION_INBOX_ACTION_SCHEMA_VERSION.to_string(),
            prompt_id: prompt_id.to_string(),
            status: "acknowledged".to_string(),
        })
    }

    pub async fn dismiss_inbox_prompt(
        &self,
        prompt_id: &str,
        request: IntegrationInboxDismissRequest,
    ) -> Result<IntegrationInboxActionResponse, ApiError> {
        self.ctx.inbox()?.dismiss(prompt_id, request.reason).await?;
        Ok(IntegrationInboxActionResponse {
            schema_version: INTEGRATION_INBOX_ACTION_SCHEMA_VERSION.to_string(),
            prompt_id: prompt_id.to_string(),
            status: "dismissed".to_string(),
        })
    }
}

#[derive(Clone)]
pub struct IntegrationAuthCommandService {
    ctx: IntegrationWebContext,
}

impl IntegrationAuthCommandService {
    pub async fn get_auth_status(
        &self,
    ) -> Result<oneshim_core::models::integration::IntegrationAuthStatus, ApiError> {
        Ok(self.ctx.auth()?.current_auth_status().await?)
    }

    pub async fn start_device_authorization(
        &self,
    ) -> Result<IntegrationDeviceAuthorizationCommandResult, ApiError> {
        let auth = self.ctx.auth()?;
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
        &self,
        flow_id: &str,
    ) -> Result<IntegrationDeviceAuthorizationCommandResult, ApiError> {
        let auth_status = self.ctx.auth()?.poll_device_authorization(flow_id).await?;
        Ok(IntegrationDeviceAuthorizationCommandResult {
            flow: auth_status.pending_flow.clone(),
            auth_status,
        })
    }

    pub async fn cancel_device_authorization(
        &self,
        flow_id: &str,
    ) -> Result<IntegrationDeviceAuthorizationCommandResult, ApiError> {
        let auth = self.ctx.auth()?;
        auth.cancel_device_authorization(flow_id).await?;
        let auth_status = auth.current_auth_status().await?;
        Ok(IntegrationDeviceAuthorizationCommandResult {
            flow: auth_status.pending_flow.clone(),
            auth_status,
        })
    }
}

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
