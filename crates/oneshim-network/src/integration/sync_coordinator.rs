use std::sync::Arc;

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    InsightPacket, IntegrationAckCursor, IntegrationEnvelope, IntegrationSessionStatus,
    QueuedInsightPacket,
};
use oneshim_core::ports::integration::{
    InsightSyncPort, IntegrationOutboxPort, IntegrationSessionPort,
};

use super::transport::IntegrationSyncTransportClient;

pub struct InsightSyncCoordinator {
    session_port: Arc<dyn IntegrationSessionPort>,
    outbox: Arc<dyn IntegrationOutboxPort>,
    transport: Arc<dyn IntegrationSyncTransportClient>,
    max_batch_size: usize,
}

impl InsightSyncCoordinator {
    pub fn new(
        session_port: Arc<dyn IntegrationSessionPort>,
        outbox: Arc<dyn IntegrationOutboxPort>,
        transport: Arc<dyn IntegrationSyncTransportClient>,
        max_batch_size: usize,
    ) -> Self {
        Self {
            session_port,
            outbox,
            transport,
            max_batch_size: max_batch_size.max(1),
        }
    }

    fn queue_ids(items: &[QueuedInsightPacket]) -> Vec<String> {
        items.iter().map(|item| item.queue_id.clone()).collect()
    }
}

#[async_trait]
impl InsightSyncPort for InsightSyncCoordinator {
    async fn enqueue(
        &self,
        envelope: IntegrationEnvelope,
        packet: InsightPacket,
    ) -> Result<(), CoreError> {
        self.outbox.enqueue_insight(envelope, packet).await?;
        Ok(())
    }

    async fn flush(&self) -> Result<usize, CoreError> {
        let session = self.session_port.current_session().await?.ok_or_else(|| {
            CoreError::ServiceUnavailable("integration session is not connected".to_string())
        })?;

        if !matches!(
            session.status,
            IntegrationSessionStatus::Connected | IntegrationSessionStatus::Degraded
        ) || session.session_id.is_empty()
        {
            return Err(CoreError::ServiceUnavailable(
                "integration session is not ready for sync".to_string(),
            ));
        }

        let items = self.outbox.list_pending(self.max_batch_size).await?;
        if items.is_empty() {
            return Ok(0);
        }

        let response = self
            .transport
            .send_insights(&session.session_id, items.clone())
            .await?;

        self.outbox.delete(&Self::queue_ids(&items)).await?;
        if let Some(cursor) = response.ack_cursor {
            self.outbox.store_ack_cursor(cursor).await?;
        }

        Ok(response.accepted_count)
    }

    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
        self.outbox.last_ack_cursor().await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use oneshim_core::models::integration::{
        InsightSourceWindow, IntegrationCapabilityScope, IntegrationOrigin,
        IntegrationPrivacyClassification, IntegrationSessionState, IntegrationSessionStatus,
        QueuedInsightPacket,
    };
    use oneshim_core::ports::integration::{IntegrationOutboxPort, IntegrationSessionPort};
    use tokio::sync::Mutex;

    use super::*;
    use crate::integration::transport::IntegrationSyncTransportResponse;

    struct MockSessionPort {
        state: Option<IntegrationSessionState>,
    }

    #[async_trait]
    impl IntegrationSessionPort for MockSessionPort {
        async fn connect(
            &self,
            _requested_scopes: Vec<IntegrationCapabilityScope>,
        ) -> Result<IntegrationSessionState, CoreError> {
            self.state
                .clone()
                .ok_or_else(|| CoreError::ServiceUnavailable("no session".to_string()))
        }

        async fn current_session(&self) -> Result<Option<IntegrationSessionState>, CoreError> {
            Ok(self.state.clone())
        }

        async fn heartbeat(&self, _session_id: &str) -> Result<IntegrationSessionState, CoreError> {
            self.state
                .clone()
                .ok_or_else(|| CoreError::ServiceUnavailable("no session".to_string()))
        }

        async fn disconnect(&self, _session_id: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    struct MockOutbox {
        items: Arc<Mutex<VecDeque<QueuedInsightPacket>>>,
        last_cursor: Arc<Mutex<Option<IntegrationAckCursor>>>,
    }

    #[async_trait]
    impl IntegrationOutboxPort for MockOutbox {
        async fn enqueue_insight(
            &self,
            envelope: IntegrationEnvelope,
            packet: InsightPacket,
        ) -> Result<String, CoreError> {
            let queue_id = format!("queue-{}", packet.packet_id);
            self.items.lock().await.push_back(QueuedInsightPacket {
                queue_id: queue_id.clone(),
                envelope,
                packet,
                queued_at: Utc::now(),
            });
            Ok(queue_id)
        }

        async fn list_pending(&self, limit: usize) -> Result<Vec<QueuedInsightPacket>, CoreError> {
            Ok(self
                .items
                .lock()
                .await
                .iter()
                .take(limit)
                .cloned()
                .collect())
        }

        async fn delete(&self, queue_ids: &[String]) -> Result<(), CoreError> {
            let mut guard = self.items.lock().await;
            guard.retain(|item| !queue_ids.contains(&item.queue_id));
            Ok(())
        }

        async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
            Ok(self.last_cursor.lock().await.clone())
        }

        async fn store_ack_cursor(&self, cursor: IntegrationAckCursor) -> Result<(), CoreError> {
            *self.last_cursor.lock().await = Some(cursor);
            Ok(())
        }
    }

    struct MockSyncTransport {
        accepted_count: usize,
        cursor: Option<IntegrationAckCursor>,
    }

    #[async_trait]
    impl IntegrationSyncTransportClient for MockSyncTransport {
        async fn send_insights(
            &self,
            _session_id: &str,
            items: Vec<QueuedInsightPacket>,
        ) -> Result<IntegrationSyncTransportResponse, CoreError> {
            Ok(IntegrationSyncTransportResponse {
                accepted_count: self.accepted_count.min(items.len()),
                ack_cursor: self.cursor.clone(),
            })
        }
    }

    fn queued_item(id: &str) -> QueuedInsightPacket {
        QueuedInsightPacket {
            queue_id: format!("queue-{id}"),
            envelope: IntegrationEnvelope {
                envelope_id: format!("env-{id}"),
                schema_version: "integration.envelope.v1".to_string(),
                message_type:
                    oneshim_core::models::integration::IntegrationMessageType::InsightPacket,
                timestamp: Utc::now(),
                nonce: format!("nonce-{id}"),
                origin: IntegrationOrigin {
                    device_id: "device-1".to_string(),
                    workspace_id: None,
                    session_id: Some("session-1".to_string()),
                    source: "desktop-client".to_string(),
                },
                capability_scope: IntegrationCapabilityScope::InsightWrite,
            },
            packet: InsightPacket {
                packet_id: id.to_string(),
                summary: format!("summary-{id}"),
                derived_tags: vec!["focus".to_string()],
                source_window: InsightSourceWindow {
                    started_at: Utc::now(),
                    ended_at: Utc::now(),
                },
                privacy_classification: IntegrationPrivacyClassification::DerivedSummary,
                audit_reference_id: None,
            },
            queued_at: Utc::now(),
        }
    }

    fn connected_session() -> IntegrationSessionState {
        IntegrationSessionState {
            session_id: "session-1".to_string(),
            device_id: "device-1".to_string(),
            status: IntegrationSessionStatus::Connected,
            connected_at: Some(Utc::now()),
            last_heartbeat_at: Some(Utc::now()),
            requested_scopes: vec![IntegrationCapabilityScope::InsightWrite],
            granted_scopes: vec![IntegrationCapabilityScope::InsightWrite],
            ack_cursor: None,
        }
    }

    #[tokio::test]
    async fn enqueue_persists_to_outbox() {
        let outbox = Arc::new(MockOutbox {
            items: Arc::new(Mutex::new(VecDeque::new())),
            last_cursor: Arc::new(Mutex::new(None)),
        });
        let coordinator = InsightSyncCoordinator::new(
            Arc::new(MockSessionPort {
                state: Some(connected_session()),
            }),
            outbox.clone(),
            Arc::new(MockSyncTransport {
                accepted_count: 1,
                cursor: None,
            }),
            10,
        );

        coordinator
            .enqueue(queued_item("1").envelope, queued_item("1").packet)
            .await
            .unwrap();

        assert_eq!(outbox.list_pending(10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn flush_sends_and_clears_pending_items() {
        let outbox = Arc::new(MockOutbox {
            items: Arc::new(Mutex::new(VecDeque::from(vec![
                queued_item("1"),
                queued_item("2"),
            ]))),
            last_cursor: Arc::new(Mutex::new(None)),
        });
        let coordinator = InsightSyncCoordinator::new(
            Arc::new(MockSessionPort {
                state: Some(connected_session()),
            }),
            outbox.clone(),
            Arc::new(MockSyncTransport {
                accepted_count: 2,
                cursor: Some(IntegrationAckCursor {
                    stream_id: "insights".to_string(),
                    cursor: "cursor-2".to_string(),
                    acknowledged_at: Utc::now(),
                }),
            }),
            10,
        );

        let flushed = coordinator.flush().await.unwrap();
        assert_eq!(flushed, 2);
        assert!(outbox.list_pending(10).await.unwrap().is_empty());
        assert_eq!(
            coordinator.last_ack_cursor().await.unwrap().unwrap().cursor,
            "cursor-2"
        );
    }

    #[tokio::test]
    async fn flush_requires_connected_session() {
        let coordinator = InsightSyncCoordinator::new(
            Arc::new(MockSessionPort { state: None }),
            Arc::new(MockOutbox {
                items: Arc::new(Mutex::new(VecDeque::from(vec![queued_item("1")]))),
                last_cursor: Arc::new(Mutex::new(None)),
            }),
            Arc::new(MockSyncTransport {
                accepted_count: 1,
                cursor: None,
            }),
            10,
        );

        let err = coordinator.flush().await.expect_err("flush should fail");
        assert!(matches!(err, CoreError::ServiceUnavailable(_)));
    }
}
