use std::sync::Arc;

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    IntegrationCapabilityScope, IntegrationSessionState, IntegrationSessionStatus,
};
use oneshim_core::ports::integration::IntegrationSessionPort;
use tokio::sync::RwLock;

use super::transport::{IntegrationTransportClient, IntegrationTransportConnectRequest};

pub struct IntegrationSessionCoordinator {
    device_id: String,
    transport: Arc<dyn IntegrationTransportClient>,
    state: Arc<RwLock<Option<IntegrationSessionState>>>,
}

impl IntegrationSessionCoordinator {
    pub fn new(
        device_id: impl Into<String>,
        transport: Arc<dyn IntegrationTransportClient>,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            transport,
            state: Arc::new(RwLock::new(None)),
        }
    }

    fn scopes_satisfied(
        state: &IntegrationSessionState,
        requested_scopes: &[IntegrationCapabilityScope],
    ) -> bool {
        requested_scopes
            .iter()
            .all(|scope| state.granted_scopes.contains(scope))
    }

    async fn set_failed_state(&self, requested_scopes: Vec<IntegrationCapabilityScope>) {
        let mut guard = self.state.write().await;
        *guard = Some(IntegrationSessionState {
            session_id: String::new(),
            device_id: self.device_id.clone(),
            status: IntegrationSessionStatus::Failed,
            connected_at: None,
            last_heartbeat_at: None,
            requested_scopes,
            granted_scopes: Vec::new(),
            ack_cursor: None,
        });
    }
}

#[async_trait]
impl IntegrationSessionPort for IntegrationSessionCoordinator {
    async fn connect(
        &self,
        requested_scopes: Vec<IntegrationCapabilityScope>,
    ) -> Result<IntegrationSessionState, CoreError> {
        if let Some(existing) = self.current_session().await? {
            if matches!(
                existing.status,
                IntegrationSessionStatus::Connected | IntegrationSessionStatus::Degraded
            ) && Self::scopes_satisfied(&existing, &requested_scopes)
            {
                return Ok(existing);
            }
        }

        {
            let mut guard = self.state.write().await;
            *guard = Some(IntegrationSessionState {
                session_id: String::new(),
                device_id: self.device_id.clone(),
                status: IntegrationSessionStatus::Connecting,
                connected_at: None,
                last_heartbeat_at: None,
                requested_scopes: requested_scopes.clone(),
                granted_scopes: Vec::new(),
                ack_cursor: None,
            });
        }

        let response = match self
            .transport
            .connect(IntegrationTransportConnectRequest {
                device_id: self.device_id.clone(),
                requested_scopes: requested_scopes.clone(),
            })
            .await
        {
            Ok(response) => response,
            Err(err) => {
                self.set_failed_state(requested_scopes).await;
                return Err(err);
            }
        };

        let state = IntegrationSessionState {
            session_id: response.session_id,
            device_id: self.device_id.clone(),
            status: IntegrationSessionStatus::Connected,
            connected_at: Some(response.connected_at),
            last_heartbeat_at: Some(response.connected_at),
            requested_scopes,
            granted_scopes: response.granted_scopes,
            ack_cursor: None,
        };

        let mut guard = self.state.write().await;
        *guard = Some(state.clone());
        Ok(state)
    }

    async fn current_session(&self) -> Result<Option<IntegrationSessionState>, CoreError> {
        Ok(self.state.read().await.clone())
    }

    async fn heartbeat(&self, session_id: &str) -> Result<IntegrationSessionState, CoreError> {
        let heartbeat_at = match self.transport.heartbeat(session_id).await {
            Ok(value) => value,
            Err(err) => {
                let mut guard = self.state.write().await;
                if let Some(state) = guard.as_mut() {
                    if state.session_id == session_id {
                        state.status = IntegrationSessionStatus::Degraded;
                    }
                }
                return Err(err);
            }
        };

        let mut guard = self.state.write().await;
        let state = guard.as_mut().ok_or_else(|| CoreError::NotFound {
            resource_type: "integration_session".to_string(),
            id: session_id.to_string(),
        })?;

        if state.session_id != session_id {
            return Err(CoreError::NotFound {
                resource_type: "integration_session".to_string(),
                id: session_id.to_string(),
            });
        }

        state.status = IntegrationSessionStatus::Connected;
        state.last_heartbeat_at = Some(heartbeat_at);
        Ok(state.clone())
    }

    async fn disconnect(&self, session_id: &str) -> Result<(), CoreError> {
        self.transport.disconnect(session_id).await?;
        let mut guard = self.state.write().await;
        if let Some(state) = guard.as_mut() {
            if state.session_id == session_id {
                state.status = IntegrationSessionStatus::Disconnected;
            }
        }
        *guard = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use oneshim_core::error::CoreError;
    use oneshim_core::models::integration::{IntegrationCapabilityScope, IntegrationSessionStatus};
    use tokio::sync::Mutex;

    use super::*;
    use crate::integration::transport::IntegrationTransportConnectResponse;

    struct MockTransport {
        calls: Arc<Mutex<Vec<String>>>,
        fail_connect: bool,
    }

    #[async_trait]
    impl IntegrationTransportClient for MockTransport {
        async fn connect(
            &self,
            request: IntegrationTransportConnectRequest,
        ) -> Result<IntegrationTransportConnectResponse, CoreError> {
            self.calls
                .lock()
                .await
                .push(format!("connect:{}", request.device_id));
            if self.fail_connect {
                return Err(CoreError::ServiceUnavailable(
                    "transport unavailable".to_string(),
                ));
            }
            Ok(IntegrationTransportConnectResponse {
                session_id: "integration-session-1".to_string(),
                connected_at: Utc::now(),
                granted_scopes: request.requested_scopes,
            })
        }

        async fn heartbeat(&self, session_id: &str) -> Result<DateTime<Utc>, CoreError> {
            self.calls
                .lock()
                .await
                .push(format!("heartbeat:{session_id}"));
            Ok(Utc::now())
        }

        async fn disconnect(&self, session_id: &str) -> Result<(), CoreError> {
            self.calls
                .lock()
                .await
                .push(format!("disconnect:{session_id}"));
            Ok(())
        }
    }

    #[tokio::test]
    async fn connect_creates_connected_session() {
        let coordinator = IntegrationSessionCoordinator::new(
            "device-1",
            Arc::new(MockTransport {
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_connect: false,
            }),
        );

        let session = coordinator
            .connect(vec![IntegrationCapabilityScope::InsightWrite])
            .await
            .unwrap();

        assert_eq!(session.session_id, "integration-session-1");
        assert_eq!(session.status, IntegrationSessionStatus::Connected);
        assert_eq!(session.granted_scopes.len(), 1);
    }

    #[tokio::test]
    async fn connect_reuses_existing_session_when_scopes_are_satisfied() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let coordinator = IntegrationSessionCoordinator::new(
            "device-1",
            Arc::new(MockTransport {
                calls: calls.clone(),
                fail_connect: false,
            }),
        );

        coordinator
            .connect(vec![
                IntegrationCapabilityScope::InsightWrite,
                IntegrationCapabilityScope::PromptRead,
            ])
            .await
            .unwrap();
        coordinator
            .connect(vec![IntegrationCapabilityScope::InsightWrite])
            .await
            .unwrap();

        let recorded = calls.lock().await;
        assert_eq!(
            recorded
                .iter()
                .filter(|entry| entry.starts_with("connect:"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn connect_failure_moves_state_to_failed() {
        let coordinator = IntegrationSessionCoordinator::new(
            "device-1",
            Arc::new(MockTransport {
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_connect: true,
            }),
        );

        let err = coordinator
            .connect(vec![IntegrationCapabilityScope::SessionManage])
            .await
            .expect_err("connect should fail");
        assert!(matches!(err, CoreError::ServiceUnavailable(_)));

        let state = coordinator.current_session().await.unwrap().unwrap();
        assert_eq!(state.status, IntegrationSessionStatus::Failed);
    }

    #[tokio::test]
    async fn heartbeat_updates_session_state() {
        let coordinator = IntegrationSessionCoordinator::new(
            "device-1",
            Arc::new(MockTransport {
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_connect: false,
            }),
        );

        let session = coordinator
            .connect(vec![IntegrationCapabilityScope::SessionManage])
            .await
            .unwrap();

        let updated = coordinator.heartbeat(&session.session_id).await.unwrap();
        assert_eq!(updated.status, IntegrationSessionStatus::Connected);
        assert!(updated.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn disconnect_clears_current_session() {
        let coordinator = IntegrationSessionCoordinator::new(
            "device-1",
            Arc::new(MockTransport {
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_connect: false,
            }),
        );

        let session = coordinator
            .connect(vec![IntegrationCapabilityScope::SessionManage])
            .await
            .unwrap();

        coordinator.disconnect(&session.session_id).await.unwrap();
        assert!(coordinator.current_session().await.unwrap().is_none());
    }
}
