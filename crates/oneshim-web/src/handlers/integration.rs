use axum::{extract::State, Json};
use oneshim_api_contracts::integration::{
    IntegrationAckCursorSummary, IntegrationAuditLogResponse, IntegrationAuditRecordSummary,
    IntegrationOutboundRuntimeStatus, IntegrationSessionSummary, IntegrationStatus,
};
use oneshim_core::models::integration::{IntegrationInsightAuditRecord, IntegrationSessionState};
use tracing::warn;

use crate::AppState;

const INTEGRATION_STATUS_SCHEMA_VERSION: &str = "integration.status.v1";
const INTEGRATION_AUDIT_SCHEMA_VERSION: &str = "integration.audit.v1";

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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_api_contracts::integration::IntegrationOutboundRuntimeStatus;
    use oneshim_core::error::CoreError;
    use oneshim_core::models::integration::{
        InsightPacket, IntegrationAckCursor, IntegrationAuthScheme, IntegrationCapabilityScope,
        IntegrationEgressDisposition, IntegrationEnvelope, IntegrationInboxItemStatus,
        IntegrationInsightAuditRecord, IntegrationPrivacyClassification, IntegrationSessionState,
        IntegrationSessionStatus, IntegrationTransportKind, StoredProactivePrompt,
    };
    use oneshim_core::ports::integration::{
        IntegrationAuditPort, IntegrationInboxStorePort, IntegrationOutboxPort,
        IntegrationSessionPort,
    };
    use oneshim_storage::sqlite::SqliteStorage;
    use std::sync::Arc;
    use tokio::sync::broadcast;

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

    #[async_trait]
    impl IntegrationOutboxPort for TestOutbox {
        async fn enqueue_insight(
            &self,
            _envelope: IntegrationEnvelope,
            _packet: InsightPacket,
        ) -> Result<String, CoreError> {
            Ok("queue-1".to_string())
        }

        async fn list_pending(
            &self,
            _limit: usize,
        ) -> Result<Vec<oneshim_core::models::integration::QueuedInsightPacket>, CoreError>
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

        async fn pending_count(&self) -> Result<usize, CoreError> {
            Ok(self.pending_count)
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
            integration_auth: None,
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
}
