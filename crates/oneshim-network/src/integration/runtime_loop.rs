use std::sync::Arc;
use std::time::Duration;

use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    IntegrationCapabilityScope, IntegrationSessionState, IntegrationSessionStatus,
};
use oneshim_core::ports::integration::{
    IntegrationEgressPort, IntegrationInboxPort, IntegrationSessionPort,
};
use tokio::sync::watch;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct IntegrationRuntimeLoopProfile {
    pub requested_scopes: Vec<IntegrationCapabilityScope>,
    pub connect_retry_interval: Duration,
    pub heartbeat_interval: Duration,
    pub egress_interval: Duration,
    pub inbox_refresh_interval: Duration,
}

impl Default for IntegrationRuntimeLoopProfile {
    fn default() -> Self {
        Self {
            requested_scopes: vec![
                IntegrationCapabilityScope::InsightWrite,
                IntegrationCapabilityScope::PromptRead,
                IntegrationCapabilityScope::PromptAck,
                IntegrationCapabilityScope::SessionManage,
            ],
            connect_retry_interval: Duration::from_secs(15),
            heartbeat_interval: Duration::from_secs(30),
            egress_interval: Duration::from_secs(15),
            inbox_refresh_interval: Duration::from_secs(15),
        }
    }
}

#[derive(Clone)]
pub struct IntegrationRuntimeLoop {
    session: Arc<dyn IntegrationSessionPort>,
    egress: Arc<dyn IntegrationEgressPort>,
    inbox: Arc<dyn IntegrationInboxPort>,
    profile: IntegrationRuntimeLoopProfile,
}

impl IntegrationRuntimeLoop {
    pub fn new(
        session: Arc<dyn IntegrationSessionPort>,
        egress: Arc<dyn IntegrationEgressPort>,
        inbox: Arc<dyn IntegrationInboxPort>,
        profile: IntegrationRuntimeLoopProfile,
    ) -> Self {
        Self {
            session,
            egress,
            inbox,
            profile,
        }
    }

    fn session_satisfies_scopes(
        session: &IntegrationSessionState,
        requested_scopes: &[IntegrationCapabilityScope],
    ) -> bool {
        matches!(
            session.status,
            IntegrationSessionStatus::Connected | IntegrationSessionStatus::Degraded
        ) && !session.session_id.is_empty()
            && requested_scopes
                .iter()
                .all(|scope| session.granted_scopes.contains(scope))
    }

    async fn ensure_session_ready(&self) -> Result<(), CoreError> {
        if let Some(current) = self.session.current_session().await? {
            if Self::session_satisfies_scopes(&current, &self.profile.requested_scopes) {
                return Ok(());
            }
        }

        self.session
            .connect(self.profile.requested_scopes.clone())
            .await
            .map(|_| ())
    }

    async fn run_connect_cycle(&self) -> Result<(), CoreError> {
        self.ensure_session_ready().await
    }

    async fn run_egress_cycle(&self) -> Result<usize, CoreError> {
        self.ensure_session_ready().await?;
        self.egress.flush().await
    }

    async fn run_inbox_cycle(&self) -> Result<usize, CoreError> {
        self.ensure_session_ready().await?;
        self.inbox.refresh().await
    }

    async fn run_heartbeat_cycle(&self) -> Result<(), CoreError> {
        let Some(current) = self.session.current_session().await? else {
            return Ok(());
        };

        if matches!(
            current.status,
            IntegrationSessionStatus::Connected | IntegrationSessionStatus::Degraded
        ) && !current.session_id.is_empty()
        {
            self.session.heartbeat(&current.session_id).await?;
        }

        Ok(())
    }

    pub async fn run(&self, mut shutdown_rx: watch::Receiver<bool>) {
        let mut connect_interval = tokio::time::interval(self.profile.connect_retry_interval);
        let mut heartbeat_interval = tokio::time::interval(self.profile.heartbeat_interval);
        let mut egress_interval = tokio::time::interval(self.profile.egress_interval);
        let mut inbox_interval = tokio::time::interval(self.profile.inbox_refresh_interval);

        connect_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        egress_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        inbox_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = connect_interval.tick() => {
                    if let Err(error) = self.run_connect_cycle().await {
                        warn!(error = %error, "integration runtime connect cycle failed");
                    }
                }
                _ = heartbeat_interval.tick() => {
                    if let Err(error) = self.run_heartbeat_cycle().await {
                        warn!(error = %error, "integration runtime heartbeat cycle failed");
                    }
                }
                _ = egress_interval.tick() => {
                    if let Err(error) = self.run_egress_cycle().await {
                        warn!(error = %error, "integration runtime egress cycle failed");
                    }
                }
                _ = inbox_interval.tick() => {
                    if let Err(error) = self.run_inbox_cycle().await {
                        warn!(error = %error, "integration runtime inbox cycle failed");
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use tokio::sync::Mutex;

    use super::*;
    use oneshim_core::models::integration::{
        IntegrationAckCursor, IntegrationAuthScheme, IntegrationSessionStatus,
        IntegrationTransportKind,
    };

    #[derive(Default)]
    struct MockSessionPort {
        current: Arc<Mutex<Option<IntegrationSessionState>>>,
        connect_calls: Arc<Mutex<usize>>,
        heartbeat_calls: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl IntegrationSessionPort for MockSessionPort {
        async fn connect(
            &self,
            requested_scopes: Vec<IntegrationCapabilityScope>,
        ) -> Result<IntegrationSessionState, CoreError> {
            *self.connect_calls.lock().await += 1;
            let session = IntegrationSessionState {
                session_id: "session-runtime".to_string(),
                device_id: "device-1".to_string(),
                status: IntegrationSessionStatus::Connected,
                transport_kind: IntegrationTransportKind::WebSocket,
                auth_scheme: IntegrationAuthScheme::BearerToken,
                connected_at: Some(Utc::now()),
                last_heartbeat_at: None,
                requested_scopes: requested_scopes.clone(),
                granted_scopes: requested_scopes,
                ack_cursors: Vec::new(),
            };
            *self.current.lock().await = Some(session.clone());
            Ok(session)
        }

        async fn current_session(&self) -> Result<Option<IntegrationSessionState>, CoreError> {
            Ok(self.current.lock().await.clone())
        }

        async fn heartbeat(&self, _session_id: &str) -> Result<IntegrationSessionState, CoreError> {
            *self.heartbeat_calls.lock().await += 1;
            let session = self.current.lock().await.clone().ok_or_else(|| {
                CoreError::ServiceUnavailable("integration session missing".to_string())
            })?;
            Ok(session)
        }

        async fn store_ack_cursor(
            &self,
            _session_id: &str,
            _cursor: IntegrationAckCursor,
        ) -> Result<IntegrationSessionState, CoreError> {
            self.current_session().await?.ok_or_else(|| {
                CoreError::ServiceUnavailable("integration session missing".to_string())
            })
        }

        async fn disconnect(&self, _session_id: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockEgressPort {
        flush_calls: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl IntegrationEgressPort for MockEgressPort {
        async fn enqueue_message(
            &self,
            _envelope: oneshim_core::models::integration::IntegrationEnvelope,
            _payload: oneshim_core::models::integration::IntegrationOutboundPayload,
        ) -> Result<(), CoreError> {
            Ok(())
        }

        async fn flush(&self) -> Result<usize, CoreError> {
            *self.flush_calls.lock().await += 1;
            Ok(0)
        }

        async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct MockInboxPort {
        refresh_calls: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl IntegrationInboxPort for MockInboxPort {
        async fn refresh(&self) -> Result<usize, CoreError> {
            *self.refresh_calls.lock().await += 1;
            Ok(0)
        }

        async fn list_pending(
            &self,
        ) -> Result<Vec<oneshim_core::models::integration::StoredProactivePrompt>, CoreError>
        {
            Ok(Vec::new())
        }

        async fn acknowledge(&self, _prompt_id: &str) -> Result<(), CoreError> {
            Ok(())
        }

        async fn dismiss(
            &self,
            _prompt_id: &str,
            _reason: Option<String>,
        ) -> Result<(), CoreError> {
            Ok(())
        }

        async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn egress_cycle_connects_before_flushing() {
        let session = Arc::new(MockSessionPort::default());
        let egress = Arc::new(MockEgressPort::default());
        let inbox = Arc::new(MockInboxPort::default());
        let runtime = IntegrationRuntimeLoop::new(
            session.clone(),
            egress.clone(),
            inbox,
            IntegrationRuntimeLoopProfile::default(),
        );

        runtime.run_egress_cycle().await.unwrap();

        assert_eq!(*session.connect_calls.lock().await, 1);
        assert_eq!(*egress.flush_calls.lock().await, 1);
    }

    #[tokio::test]
    async fn heartbeat_cycle_uses_existing_session() {
        let session = Arc::new(MockSessionPort::default());
        session
            .current
            .lock()
            .await
            .replace(IntegrationSessionState {
                session_id: "session-runtime".to_string(),
                device_id: "device-1".to_string(),
                status: IntegrationSessionStatus::Connected,
                transport_kind: IntegrationTransportKind::WebSocket,
                auth_scheme: IntegrationAuthScheme::BearerToken,
                connected_at: Some(Utc::now()),
                last_heartbeat_at: None,
                requested_scopes: vec![IntegrationCapabilityScope::SessionManage],
                granted_scopes: vec![IntegrationCapabilityScope::SessionManage],
                ack_cursors: Vec::new(),
            });
        let runtime = IntegrationRuntimeLoop::new(
            session.clone(),
            Arc::new(MockEgressPort::default()),
            Arc::new(MockInboxPort::default()),
            IntegrationRuntimeLoopProfile::default(),
        );

        runtime.run_heartbeat_cycle().await.unwrap();

        assert_eq!(*session.connect_calls.lock().await, 0);
        assert_eq!(*session.heartbeat_calls.lock().await, 1);
    }
}
