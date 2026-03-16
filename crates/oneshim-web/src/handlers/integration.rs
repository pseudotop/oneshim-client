use axum::{extract::State, Json};
use oneshim_api_contracts::integration::{
    IntegrationOutboundRuntimeStatus, IntegrationSessionSummary, IntegrationStatus,
};
use oneshim_core::models::integration::IntegrationSessionState;
use tracing::warn;

use crate::AppState;

const INTEGRATION_STATUS_SCHEMA_VERSION: &str = "integration.status.v1";

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
        outbound_runtime.auth_source_configured = auth_token_env_var.is_some();
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

    Json(IntegrationStatus {
        schema_version: INTEGRATION_STATUS_SCHEMA_VERSION.to_string(),
        external_access_enabled,
        automation_controller_configured: state.automation_controller.is_some(),
        ai_runtime_status: state.ai_runtime_status.clone(),
        outbound_runtime,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_api_contracts::integration::IntegrationOutboundRuntimeStatus;
    use oneshim_core::error::CoreError;
    use oneshim_core::models::integration::{
        IntegrationAckCursor, IntegrationAuthScheme, IntegrationCapabilityScope,
        IntegrationSessionState, IntegrationSessionStatus, IntegrationTransportKind,
    };
    use oneshim_core::ports::integration::IntegrationSessionPort;
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
                preferred_transports: vec![IntegrationTransportKind::WebSocket],
                supported_auth_schemes: vec![IntegrationAuthScheme::BearerToken],
                current_session: None,
            }),
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
    }
}
