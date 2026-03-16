use axum::{
    extract::{Path, State},
    Json,
};
use oneshim_api_contracts::integration::{
    IntegrationAckCursorSummary, IntegrationAuditLogResponse, IntegrationAuditRecordSummary,
    IntegrationDeviceAuthorizationCommandResult, IntegrationDeviceAuthorizationFlowRequest,
    IntegrationInboxActionResponse, IntegrationInboxDismissRequest, IntegrationInboxPromptSummary,
    IntegrationInboxRefreshResponse, IntegrationInboxResponse, IntegrationOutboundRuntimeStatus,
    IntegrationSessionSummary, IntegrationStatus,
};
use oneshim_core::models::integration::{
    default_integration_runtime_scopes, IntegrationInboxItemStatus, IntegrationInsightAuditRecord,
    IntegrationSessionState, ProactivePromptCategory, ProactivePromptPriority,
    StoredProactivePrompt,
};
use tracing::warn;

use crate::{error::ApiError, AppState};

const INTEGRATION_STATUS_SCHEMA_VERSION: &str = "integration.status.v1";
const INTEGRATION_AUDIT_SCHEMA_VERSION: &str = "integration.audit.v1";
const INTEGRATION_INBOX_SCHEMA_VERSION: &str = "integration.inbox.v1";
const INTEGRATION_INBOX_ACTION_SCHEMA_VERSION: &str = "integration.inbox-action.v1";

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

pub async fn get_status(State(state): State<AppState>) -> Json<IntegrationStatus> {
    let config = state
        .config_manager
        .as_ref()
        .map(|config_manager| config_manager.get());
    let external_access_enabled = config
        .as_ref()
        .map(|config| config.web.allow_external)
        .unwrap_or(false);
    let mut outbound_runtime = state
        .integration_runtime_status
        .clone()
        .unwrap_or_else(IntegrationOutboundRuntimeStatus::default);
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

    Json(IntegrationStatus {
        schema_version: INTEGRATION_STATUS_SCHEMA_VERSION.to_string(),
        external_access_enabled,
        automation_controller_configured: state.automation_controller.is_some(),
        ai_runtime_status: state.ai_runtime_status.clone(),
        outbound_runtime,
    })
}

pub async fn get_audit(State(state): State<AppState>) -> Json<IntegrationAuditLogResponse> {
    let records = if let Some(audit) = state.integration_audit.as_ref() {
        match audit.recent_insight_decisions(50).await {
            Ok(records) => records.into_iter().map(map_audit_record).collect(),
            Err(error) => {
                warn!(error = %error, "failed to read integration audit records");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    Json(IntegrationAuditLogResponse {
        schema_version: INTEGRATION_AUDIT_SCHEMA_VERSION.to_string(),
        records,
    })
}

pub async fn list_inbox(
    State(state): State<AppState>,
) -> Result<Json<IntegrationInboxResponse>, ApiError> {
    let inbox = state.integration_inbox.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration inbox runtime is not configured.".to_string())
    })?;

    let prompts = inbox
        .list_pending()
        .await?
        .into_iter()
        .map(map_prompt)
        .collect::<Vec<_>>();
    let pending_count = prompts.len();

    Ok(Json(IntegrationInboxResponse {
        schema_version: INTEGRATION_INBOX_SCHEMA_VERSION.to_string(),
        prompts,
        pending_count,
    }))
}

pub async fn refresh_inbox(
    State(state): State<AppState>,
) -> Result<Json<IntegrationInboxRefreshResponse>, ApiError> {
    let inbox = state.integration_inbox.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration inbox runtime is not configured.".to_string())
    })?;

    Ok(Json(IntegrationInboxRefreshResponse {
        schema_version: INTEGRATION_INBOX_SCHEMA_VERSION.to_string(),
        fetched_count: inbox.refresh().await?,
    }))
}

pub async fn acknowledge_inbox_prompt(
    State(state): State<AppState>,
    Path(prompt_id): Path<String>,
) -> Result<Json<IntegrationInboxActionResponse>, ApiError> {
    let inbox = state.integration_inbox.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration inbox runtime is not configured.".to_string())
    })?;

    inbox.acknowledge(&prompt_id).await?;
    Ok(Json(IntegrationInboxActionResponse {
        schema_version: INTEGRATION_INBOX_ACTION_SCHEMA_VERSION.to_string(),
        prompt_id,
        status: "acknowledged".to_string(),
    }))
}

pub async fn dismiss_inbox_prompt(
    State(state): State<AppState>,
    Path(prompt_id): Path<String>,
    Json(request): Json<IntegrationInboxDismissRequest>,
) -> Result<Json<IntegrationInboxActionResponse>, ApiError> {
    let inbox = state.integration_inbox.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration inbox runtime is not configured.".to_string())
    })?;

    inbox.dismiss(&prompt_id, request.reason).await?;
    Ok(Json(IntegrationInboxActionResponse {
        schema_version: INTEGRATION_INBOX_ACTION_SCHEMA_VERSION.to_string(),
        prompt_id,
        status: "dismissed".to_string(),
    }))
}

pub async fn get_auth_status(
    State(state): State<AppState>,
) -> Result<Json<oneshim_core::models::integration::IntegrationAuthStatus>, ApiError> {
    let auth = state.integration_auth.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration auth runtime is not configured.".to_string())
    })?;
    Ok(Json(auth.current_auth_status().await?))
}

pub async fn start_device_authorization(
    State(state): State<AppState>,
) -> Result<Json<IntegrationDeviceAuthorizationCommandResult>, ApiError> {
    let auth = state.integration_auth.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration auth runtime is not configured.".to_string())
    })?;
    let flow = auth
        .start_device_authorization(&default_integration_runtime_scopes(), None)
        .await?;
    let auth_status = auth.current_auth_status().await?;
    Ok(Json(IntegrationDeviceAuthorizationCommandResult {
        auth_status,
        flow: Some(flow),
    }))
}

pub async fn poll_device_authorization(
    State(state): State<AppState>,
    Json(request): Json<IntegrationDeviceAuthorizationFlowRequest>,
) -> Result<Json<IntegrationDeviceAuthorizationCommandResult>, ApiError> {
    let auth = state.integration_auth.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration auth runtime is not configured.".to_string())
    })?;
    let auth_status = auth.poll_device_authorization(&request.flow_id).await?;
    Ok(Json(IntegrationDeviceAuthorizationCommandResult {
        flow: auth_status.pending_flow.clone(),
        auth_status,
    }))
}

pub async fn cancel_device_authorization(
    State(state): State<AppState>,
    Json(request): Json<IntegrationDeviceAuthorizationFlowRequest>,
) -> Result<Json<IntegrationDeviceAuthorizationCommandResult>, ApiError> {
    let auth = state.integration_auth.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("Integration auth runtime is not configured.".to_string())
    })?;
    auth.cancel_device_authorization(&request.flow_id).await?;
    let auth_status = auth.current_auth_status().await?;
    Ok(Json(IntegrationDeviceAuthorizationCommandResult {
        flow: auth_status.pending_flow.clone(),
        auth_status,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_api_contracts::integration::{
        IntegrationDeviceAuthorizationFlowRequest, IntegrationOutboundRuntimeStatus,
    };
    use oneshim_core::error::CoreError;
    use oneshim_core::models::integration::{
        IntegrationAckCursor, IntegrationAuthContext, IntegrationAuthProfileKind,
        IntegrationAuthScheme, IntegrationAuthStatus, IntegrationAuthStatusKind,
        IntegrationCapabilityScope, IntegrationDeviceAuthorizationFlow,
        IntegrationEgressDisposition, IntegrationEnvelope, IntegrationInboxItemStatus,
        IntegrationInsightAuditRecord, IntegrationPrivacyClassification, IntegrationSessionState,
        IntegrationSessionStatus, IntegrationTransportKind, ProactivePrompt,
        ProactivePromptCategory, ProactivePromptPriority, PromptProvenance, StoredProactivePrompt,
    };
    use oneshim_core::ports::integration::{
        IntegrationAuditPort, IntegrationAuthPort, IntegrationInboxPort, IntegrationInboxStorePort,
        IntegrationOutboxPort, IntegrationSessionPort,
    };
    use oneshim_storage::sqlite::SqliteStorage;
    use std::sync::Arc;
    use tokio::sync::{broadcast, Mutex};

    struct TestSessionPort(Option<IntegrationSessionState>);

    #[async_trait]
    impl IntegrationSessionPort for TestSessionPort {
        async fn connect(
            &self,
            _requested_scopes: Vec<IntegrationCapabilityScope>,
        ) -> Result<IntegrationSessionState, CoreError> {
            self.current_session()
                .await?
                .ok_or_else(|| CoreError::Auth("no session".to_string()))
        }

        async fn current_session(&self) -> Result<Option<IntegrationSessionState>, CoreError> {
            Ok(self.0.clone())
        }

        async fn heartbeat(&self, _session_id: &str) -> Result<IntegrationSessionState, CoreError> {
            self.connect(Vec::new()).await
        }

        async fn store_ack_cursor(
            &self,
            _session_id: &str,
            _cursor: IntegrationAckCursor,
        ) -> Result<IntegrationSessionState, CoreError> {
            self.connect(Vec::new()).await
        }

        async fn disconnect(&self, _session_id: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    struct TestOutbox {
        pending_count: usize,
        last_ack_cursor: Option<IntegrationAckCursor>,
    }

    struct TestAuthPort {
        status: Arc<Mutex<IntegrationAuthStatus>>,
    }

    #[async_trait]
    impl IntegrationAuthPort for TestAuthPort {
        async fn resolve_session_auth(
            &self,
            _requested_scopes: &[IntegrationCapabilityScope],
            _resource_indicator: Option<&str>,
        ) -> Result<IntegrationAuthContext, CoreError> {
            Ok(IntegrationAuthContext {
                access_token: "integration-token".to_string(),
                scheme: IntegrationAuthScheme::BearerToken,
                expires_at: None,
                resource_indicator: Some("https://integration.example.com".to_string()),
            })
        }

        async fn current_auth_status(&self) -> Result<IntegrationAuthStatus, CoreError> {
            Ok(self.status.lock().await.clone())
        }

        async fn start_device_authorization(
            &self,
            requested_scopes: &[IntegrationCapabilityScope],
            resource_indicator: Option<&str>,
        ) -> Result<IntegrationDeviceAuthorizationFlow, CoreError> {
            let flow = IntegrationDeviceAuthorizationFlow {
                flow_id: "flow-1".to_string(),
                user_code: "ABCD-EFGH".to_string(),
                verification_uri: "https://verify.example.com".to_string(),
                verification_uri_complete: Some(
                    "https://verify.example.com?user_code=ABCD-EFGH".to_string(),
                ),
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
                interval_secs: 5,
                requested_scopes: requested_scopes.to_vec(),
                resource_indicator: resource_indicator.map(str::to_string),
            };
            *self.status.lock().await = IntegrationAuthStatus {
                profile_kind: IntegrationAuthProfileKind::OidcDeviceFlow,
                status: IntegrationAuthStatusKind::AwaitingUserAuthorization,
                interactive: true,
                authenticated: false,
                expires_at: None,
                resource_indicator: resource_indicator.map(str::to_string),
                pending_flow: Some(flow.clone()),
                message: Some("authorize the device".to_string()),
            };
            Ok(flow)
        }

        async fn poll_device_authorization(
            &self,
            flow_id: &str,
        ) -> Result<IntegrationAuthStatus, CoreError> {
            let mut status = self.status.lock().await;
            if status
                .pending_flow
                .as_ref()
                .map(|flow| flow.flow_id.as_str())
                != Some(flow_id)
            {
                return Err(CoreError::NotFound {
                    resource_type: "integration_device_flow".to_string(),
                    id: flow_id.to_string(),
                });
            }
            status.status = IntegrationAuthStatusKind::Ready;
            status.authenticated = true;
            status.pending_flow = None;
            status.message = None;
            Ok(status.clone())
        }

        async fn cancel_device_authorization(&self, flow_id: &str) -> Result<(), CoreError> {
            let mut status = self.status.lock().await;
            if status
                .pending_flow
                .as_ref()
                .map(|flow| flow.flow_id.as_str())
                != Some(flow_id)
            {
                return Err(CoreError::NotFound {
                    resource_type: "integration_device_flow".to_string(),
                    id: flow_id.to_string(),
                });
            }
            status.status = IntegrationAuthStatusKind::Unauthenticated;
            status.pending_flow = None;
            status.message = Some("device authorization cancelled".to_string());
            Ok(())
        }
    }

    #[async_trait]
    impl IntegrationOutboxPort for TestOutbox {
        async fn enqueue_message(
            &self,
            _envelope: IntegrationEnvelope,
            _payload: oneshim_core::models::integration::IntegrationOutboundPayload,
        ) -> Result<String, CoreError> {
            Ok("queue-1".to_string())
        }

        async fn list_pending(
            &self,
            _limit: usize,
        ) -> Result<Vec<oneshim_core::models::integration::QueuedIntegrationEgressMessage>, CoreError>
        {
            Ok(Vec::new())
        }

        async fn pending_count(&self) -> Result<usize, CoreError> {
            Ok(self.pending_count)
        }

        async fn delete(&self, _queue_ids: &[String]) -> Result<(), CoreError> {
            Ok(())
        }

        async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
            Ok(self.last_ack_cursor.clone())
        }

        async fn store_ack_cursor(&self, _cursor: IntegrationAckCursor) -> Result<(), CoreError> {
            Ok(())
        }
    }

    struct TestInboxStore {
        pending_count: usize,
        last_ack_cursor: Option<IntegrationAckCursor>,
    }

    #[async_trait]
    impl IntegrationInboxStorePort for TestInboxStore {
        async fn upsert_prompts(
            &self,
            _prompts: Vec<StoredProactivePrompt>,
        ) -> Result<(), CoreError> {
            Ok(())
        }

        async fn list_pending(&self) -> Result<Vec<StoredProactivePrompt>, CoreError> {
            Ok(Vec::new())
        }

        async fn list_unpresented(
            &self,
            _limit: usize,
        ) -> Result<Vec<StoredProactivePrompt>, CoreError> {
            Ok(Vec::new())
        }

        async fn pending_count(&self) -> Result<usize, CoreError> {
            Ok(self.pending_count)
        }

        async fn mark_presented(
            &self,
            _prompt_id: &str,
            _presented_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), CoreError> {
            Ok(())
        }

        async fn update_status(
            &self,
            _prompt_id: &str,
            _status: IntegrationInboxItemStatus,
            _reason: Option<String>,
        ) -> Result<(), CoreError> {
            Ok(())
        }

        async fn expire_stale(&self) -> Result<usize, CoreError> {
            Ok(0)
        }

        async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
            Ok(self.last_ack_cursor.clone())
        }

        async fn store_ack_cursor(&self, _cursor: IntegrationAckCursor) -> Result<(), CoreError> {
            Ok(())
        }
    }

    struct TestInboxPort {
        prompts: Arc<Mutex<Vec<StoredProactivePrompt>>>,
    }

    #[async_trait]
    impl IntegrationInboxPort for TestInboxPort {
        async fn refresh(&self) -> Result<usize, CoreError> {
            Ok(self.prompts.lock().await.len())
        }

        async fn list_pending(&self) -> Result<Vec<StoredProactivePrompt>, CoreError> {
            Ok(self.prompts.lock().await.clone())
        }

        async fn acknowledge(&self, prompt_id: &str) -> Result<(), CoreError> {
            let mut prompts = self.prompts.lock().await;
            let prompt = prompts
                .iter_mut()
                .find(|prompt| prompt.prompt.prompt_id == prompt_id)
                .ok_or_else(|| CoreError::NotFound {
                    resource_type: "integration_prompt".to_string(),
                    id: prompt_id.to_string(),
                })?;
            prompt.status = IntegrationInboxItemStatus::Acknowledged;
            Ok(())
        }

        async fn dismiss(&self, prompt_id: &str, reason: Option<String>) -> Result<(), CoreError> {
            let mut prompts = self.prompts.lock().await;
            let prompt = prompts
                .iter_mut()
                .find(|prompt| prompt.prompt.prompt_id == prompt_id)
                .ok_or_else(|| CoreError::NotFound {
                    resource_type: "integration_prompt".to_string(),
                    id: prompt_id.to_string(),
                })?;
            prompt.status = IntegrationInboxItemStatus::Dismissed;
            prompt.dismiss_reason = reason;
            Ok(())
        }

        async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
            Ok(None)
        }
    }

    struct TestAuditPort(Vec<IntegrationInsightAuditRecord>);

    #[async_trait]
    impl IntegrationAuditPort for TestAuditPort {
        async fn record_insight_decision(
            &self,
            _record: IntegrationInsightAuditRecord,
        ) -> Result<(), CoreError> {
            Ok(())
        }

        async fn recent_insight_decisions(
            &self,
            _limit: usize,
        ) -> Result<Vec<IntegrationInsightAuditRecord>, CoreError> {
            Ok(self.0.clone())
        }
    }

    fn test_state() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let (event_tx, _) = broadcast::channel(8);
        let inbox_prompts = Arc::new(Mutex::new(vec![StoredProactivePrompt {
            prompt: ProactivePrompt {
                prompt_id: "prompt-1".to_string(),
                category: ProactivePromptCategory::Reminder,
                title: "Review insight".to_string(),
                body: "A prompt arrived from integration.".to_string(),
                priority: ProactivePromptPriority::Medium,
                actions: Vec::new(),
                expires_at: None,
                provenance: PromptProvenance {
                    source_system: "integration-server".to_string(),
                    source_actor: Some("scheduler".to_string()),
                    correlation_id: Some("corr-1".to_string()),
                },
            },
            received_at: chrono::Utc::now(),
            status: IntegrationInboxItemStatus::Pending,
            status_updated_at: chrono::Utc::now(),
            presented_at: None,
            dismiss_reason: None,
        }]));
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            default_secret_backend_kind: oneshim_core::config::CredentialBackendKind::Unavailable,
            secret_store: None,
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            integration_runtime_status: Some(IntegrationOutboundRuntimeStatus {
                enabled: true,
                bootstrap_configured: true,
                auth_source_configured: true,
                auth_material_available: false,
                runtime_configured: true,
                resource_indicator_configured: true,
                auth_profile_kind:
                    oneshim_core::models::integration::IntegrationAuthProfileKind::EnvToken,
                preferred_transports: vec![IntegrationTransportKind::WebSocket],
                supported_auth_schemes: vec![IntegrationAuthScheme::BearerToken],
                outbox_pending_count: None,
                inbox_pending_count: None,
                outbox_ack_cursor: None,
                inbox_ack_cursor: None,
                auth_status: None,
                current_session: None,
            }),
            integration_auth: Some(Arc::new(TestAuthPort {
                status: Arc::new(Mutex::new(IntegrationAuthStatus {
                    profile_kind: IntegrationAuthProfileKind::OidcDeviceFlow,
                    status: IntegrationAuthStatusKind::Unauthenticated,
                    interactive: true,
                    authenticated: false,
                    expires_at: None,
                    resource_indicator: Some("https://integration.example.com".to_string()),
                    pending_flow: None,
                    message: Some("authorize the device".to_string()),
                })),
            }) as Arc<dyn IntegrationAuthPort>),
            integration_session: Some(Arc::new(TestSessionPort(Some(IntegrationSessionState {
                session_id: "session-1".to_string(),
                device_id: "device-1".to_string(),
                status: IntegrationSessionStatus::Connected,
                transport_kind: IntegrationTransportKind::WebSocket,
                auth_scheme: IntegrationAuthScheme::BearerToken,
                connected_at: None,
                last_heartbeat_at: None,
                requested_scopes: vec![IntegrationCapabilityScope::InsightWrite],
                granted_scopes: vec![IntegrationCapabilityScope::InsightWrite],
                ack_cursors: Vec::new(),
            }))) as Arc<dyn IntegrationSessionPort>),
            integration_outbox: Some(Arc::new(TestOutbox {
                pending_count: 3,
                last_ack_cursor: Some(IntegrationAckCursor {
                    stream_id: "insights".to_string(),
                    cursor: "cursor-outbox".to_string(),
                    acknowledged_at: chrono::Utc::now(),
                }),
            }) as Arc<dyn IntegrationOutboxPort>),
            integration_inbox: Some(Arc::new(TestInboxPort {
                prompts: inbox_prompts,
            }) as Arc<dyn IntegrationInboxPort>),
            integration_inbox_store: Some(Arc::new(TestInboxStore {
                pending_count: 2,
                last_ack_cursor: Some(IntegrationAckCursor {
                    stream_id: "prompts".to_string(),
                    cursor: "cursor-inbox".to_string(),
                    acknowledged_at: chrono::Utc::now(),
                }),
            }) as Arc<dyn IntegrationInboxStorePort>),
            integration_audit: Some(Arc::new(TestAuditPort(vec![IntegrationInsightAuditRecord {
                record_id: "audit-1".to_string(),
                envelope_id: "env-1".to_string(),
                packet_id: "packet-1".to_string(),
                disposition: IntegrationEgressDisposition::Allow,
                reason: None,
                privacy_classification: IntegrationPrivacyClassification::DerivedSummary,
                capability_scope: IntegrationCapabilityScope::InsightWrite,
                occurred_at: chrono::Utc::now(),
            }])) as Arc<dyn IntegrationAuditPort>),
            update_control: None,
        }
    }

    #[tokio::test]
    async fn get_status_merges_runtime_snapshot_and_current_session() {
        let response = get_status(State(test_state())).await.0;

        assert!(response.outbound_runtime.enabled);
        assert!(response.outbound_runtime.runtime_configured);
        assert_eq!(
            response
                .outbound_runtime
                .current_session
                .as_ref()
                .map(|session| session.status.clone()),
            Some(IntegrationSessionStatus::Connected)
        );
        assert_eq!(
            response
                .outbound_runtime
                .current_session
                .as_ref()
                .map(|session| session.granted_scopes.clone()),
            Some(vec!["insight:write".to_string()])
        );
        assert_eq!(response.outbound_runtime.outbox_pending_count, Some(3));
        assert_eq!(response.outbound_runtime.inbox_pending_count, Some(2));
        assert_eq!(
            response
                .outbound_runtime
                .outbox_ack_cursor
                .as_ref()
                .map(|cursor| cursor.stream_id.as_str()),
            Some("insights")
        );
        assert_eq!(
            response
                .outbound_runtime
                .inbox_ack_cursor
                .as_ref()
                .map(|cursor| cursor.stream_id.as_str()),
            Some("prompts")
        );
    }

    #[tokio::test]
    async fn get_audit_returns_recent_integration_records() {
        let response = get_audit(State(test_state())).await.0;

        assert_eq!(response.schema_version, INTEGRATION_AUDIT_SCHEMA_VERSION);
        assert_eq!(response.records.len(), 1);
        assert_eq!(response.records[0].record_id, "audit-1");
        assert_eq!(response.records[0].disposition, "allow");
        assert_eq!(
            response.records[0].privacy_classification,
            "derived_summary"
        );
    }

    #[tokio::test]
    async fn list_inbox_returns_pending_prompts() {
        let response = list_inbox(State(test_state())).await.unwrap().0;

        assert_eq!(response.schema_version, INTEGRATION_INBOX_SCHEMA_VERSION);
        assert_eq!(response.pending_count, 1);
        assert_eq!(response.prompts.len(), 1);
        assert_eq!(response.prompts[0].prompt_id, "prompt-1");
        assert_eq!(response.prompts[0].status, "pending");
    }

    #[tokio::test]
    async fn acknowledge_and_dismiss_inbox_prompt_return_action_status() {
        let ack_response =
            acknowledge_inbox_prompt(State(test_state()), Path("prompt-1".to_string()))
                .await
                .unwrap()
                .0;
        assert_eq!(
            ack_response.schema_version,
            INTEGRATION_INBOX_ACTION_SCHEMA_VERSION
        );
        assert_eq!(ack_response.status, "acknowledged");

        let dismiss_response = dismiss_inbox_prompt(
            State(test_state()),
            Path("prompt-1".to_string()),
            Json(IntegrationInboxDismissRequest {
                reason: Some("handled locally".to_string()),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(dismiss_response.status, "dismissed");
    }

    #[tokio::test]
    async fn auth_handlers_roundtrip_device_authorization_flow() {
        let state = test_state();

        let auth_status = get_auth_status(State(state.clone())).await.unwrap().0;
        assert_eq!(
            auth_status.status,
            IntegrationAuthStatusKind::Unauthenticated
        );

        let start_response = start_device_authorization(State(state.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(
            start_response.auth_status.status,
            IntegrationAuthStatusKind::AwaitingUserAuthorization
        );
        assert!(start_response
            .flow
            .as_ref()
            .is_some_and(|flow| flow.requested_scopes.len() >= 4));

        let poll_response = poll_device_authorization(
            State(state),
            Json(IntegrationDeviceAuthorizationFlowRequest {
                flow_id: "flow-1".to_string(),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(
            poll_response.auth_status.status,
            IntegrationAuthStatusKind::Ready
        );
    }
}
