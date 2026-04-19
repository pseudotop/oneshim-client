use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    InsightPacket, IntegrationAckCursor, IntegrationEgressDisposition, IntegrationEnvelope,
    IntegrationInsightAuditRecord, IntegrationOutboundPayload,
};
use oneshim_core::ports::integration::{
    IntegrationAuditPort, IntegrationEgressDecision, IntegrationEgressPolicyPort,
    IntegrationEgressPort,
};

pub struct PolicyAwareIntegrationEgressCoordinator {
    inner: Arc<dyn IntegrationEgressPort>,
    policy: Arc<dyn IntegrationEgressPolicyPort>,
    audit: Arc<dyn IntegrationAuditPort>,
}

impl PolicyAwareIntegrationEgressCoordinator {
    pub fn new(
        inner: Arc<dyn IntegrationEgressPort>,
        policy: Arc<dyn IntegrationEgressPolicyPort>,
        audit: Arc<dyn IntegrationAuditPort>,
    ) -> Self {
        Self {
            inner,
            policy,
            audit,
        }
    }

    fn to_audit_record(
        envelope: &IntegrationEnvelope,
        packet: &InsightPacket,
        decision: &IntegrationEgressDecision,
    ) -> IntegrationInsightAuditRecord {
        IntegrationInsightAuditRecord {
            record_id: format!("{}:{}", envelope.envelope_id, packet.packet_id),
            envelope_id: envelope.envelope_id.clone(),
            packet_id: packet.packet_id.clone(),
            disposition: decision.disposition.clone(),
            reason: decision.reason.clone(),
            privacy_classification: packet.privacy_classification.clone(),
            capability_scope: envelope.capability_scope.clone(),
            occurred_at: Utc::now(),
        }
    }
}

#[async_trait]
impl IntegrationEgressPort for PolicyAwareIntegrationEgressCoordinator {
    async fn enqueue_message(
        &self,
        envelope: IntegrationEnvelope,
        payload: IntegrationOutboundPayload,
    ) -> Result<(), CoreError> {
        if let IntegrationOutboundPayload::Insight(packet) = &payload {
            let decision = self.policy.authorize_insight(&envelope, packet).await?;

            if decision.audit_required {
                self.audit
                    .record_insight_decision(Self::to_audit_record(&envelope, packet, &decision))
                    .await?;
            }

            match decision.disposition {
                IntegrationEgressDisposition::Allow => {
                    self.inner.enqueue_message(envelope, payload).await
                }
                IntegrationEgressDisposition::Deny => Err(CoreError::PolicyDeniedV2 {
                    code: oneshim_core::error_codes::PolicyCode::Denied,
                    message: decision
                        .reason
                        .unwrap_or_else(|| "integration egress denied".to_string()),
                }),
                IntegrationEgressDisposition::RequireUserApproval => {
                    Err(CoreError::ConsentRequiredV2 {
                        code: oneshim_core::error_codes::ConsentCode::Required,
                        message: decision.reason.unwrap_or_else(|| {
                            "integration egress requires user approval".to_string()
                        }),
                    })
                }
            }
        } else {
            self.inner.enqueue_message(envelope, payload).await
        }
    }

    async fn flush(&self) -> Result<usize, CoreError> {
        self.inner.flush().await
    }

    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
        self.inner.last_ack_cursor().await
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
        InsightSourceWindow, IntegrationCapabilityScope, IntegrationMessageType, IntegrationOrigin,
        IntegrationPrivacyClassification,
    };

    struct MockEgress {
        enqueued: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl IntegrationEgressPort for MockEgress {
        async fn enqueue_message(
            &self,
            envelope: IntegrationEnvelope,
            _payload: IntegrationOutboundPayload,
        ) -> Result<(), CoreError> {
            self.enqueued.lock().await.push(envelope.envelope_id);
            Ok(())
        }

        async fn flush(&self) -> Result<usize, CoreError> {
            Ok(0)
        }

        async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
            Ok(None)
        }
    }

    struct MockPolicy {
        decision: IntegrationEgressDecision,
    }

    #[async_trait]
    impl IntegrationEgressPolicyPort for MockPolicy {
        async fn authorize_insight(
            &self,
            _envelope: &IntegrationEnvelope,
            _packet: &InsightPacket,
        ) -> Result<IntegrationEgressDecision, CoreError> {
            Ok(self.decision.clone())
        }
    }

    struct MockAudit {
        records: Arc<Mutex<Vec<IntegrationInsightAuditRecord>>>,
    }

    #[async_trait]
    impl IntegrationAuditPort for MockAudit {
        async fn record_insight_decision(
            &self,
            record: IntegrationInsightAuditRecord,
        ) -> Result<(), CoreError> {
            self.records.lock().await.push(record);
            Ok(())
        }

        async fn recent_insight_decisions(
            &self,
            limit: usize,
        ) -> Result<Vec<IntegrationInsightAuditRecord>, CoreError> {
            Ok(self
                .records
                .lock()
                .await
                .iter()
                .rev()
                .take(limit)
                .cloned()
                .collect())
        }
    }

    fn sample_envelope() -> IntegrationEnvelope {
        IntegrationEnvelope {
            envelope_id: "env-1".to_string(),
            schema_version: "integration.envelope.v1".to_string(),
            message_type: IntegrationMessageType::InsightPacket,
            timestamp: Utc::now(),
            nonce: "nonce-1".to_string(),
            origin: IntegrationOrigin {
                device_id: "device-1".to_string(),
                workspace_id: None,
                session_id: Some("session-1".to_string()),
                source: "desktop-client".to_string(),
            },
            capability_scope: IntegrationCapabilityScope::InsightWrite,
        }
    }

    fn sample_packet() -> InsightPacket {
        InsightPacket {
            packet_id: "packet-1".to_string(),
            summary: "summary".to_string(),
            derived_tags: vec!["focus".to_string()],
            source_window: InsightSourceWindow {
                started_at: Utc::now(),
                ended_at: Utc::now(),
            },
            privacy_classification: IntegrationPrivacyClassification::DerivedSummary,
            audit_reference_id: Some("audit-ref-1".to_string()),
        }
    }

    #[tokio::test]
    async fn enqueue_allows_and_audits_when_policy_allows() {
        let enqueued = Arc::new(Mutex::new(Vec::new()));
        let records = Arc::new(Mutex::new(Vec::new()));
        let coordinator = PolicyAwareIntegrationEgressCoordinator::new(
            Arc::new(MockEgress {
                enqueued: enqueued.clone(),
            }),
            Arc::new(MockPolicy {
                decision: IntegrationEgressDecision::allow(),
            }),
            Arc::new(MockAudit {
                records: records.clone(),
            }),
        );

        coordinator
            .enqueue_insight(sample_envelope(), sample_packet())
            .await
            .unwrap();

        assert_eq!(enqueued.lock().await.len(), 1);
        let records = records.lock().await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].disposition, IntegrationEgressDisposition::Allow);
    }

    #[tokio::test]
    async fn enqueue_denied_by_policy_does_not_queue() {
        let enqueued = Arc::new(Mutex::new(Vec::new()));
        let records = Arc::new(Mutex::new(Vec::new()));
        let coordinator = PolicyAwareIntegrationEgressCoordinator::new(
            Arc::new(MockEgress {
                enqueued: enqueued.clone(),
            }),
            Arc::new(MockPolicy {
                decision: IntegrationEgressDecision::deny("policy blocked"),
            }),
            Arc::new(MockAudit {
                records: records.clone(),
            }),
        );

        let err = coordinator
            .enqueue_insight(sample_envelope(), sample_packet())
            .await
            .expect_err("enqueue should fail");

        assert!(matches!(err, CoreError::PolicyDeniedV2 { .. }));
        assert!(enqueued.lock().await.is_empty());
        assert_eq!(records.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn enqueue_requires_user_approval_without_queueing() {
        let enqueued = Arc::new(Mutex::new(Vec::new()));
        let records = Arc::new(Mutex::new(Vec::new()));
        let coordinator = PolicyAwareIntegrationEgressCoordinator::new(
            Arc::new(MockEgress {
                enqueued: enqueued.clone(),
            }),
            Arc::new(MockPolicy {
                decision: IntegrationEgressDecision::require_user_approval(
                    "requires explicit consent",
                ),
            }),
            Arc::new(MockAudit {
                records: records.clone(),
            }),
        );

        let err = coordinator
            .enqueue_insight(sample_envelope(), sample_packet())
            .await
            .expect_err("enqueue should fail");

        assert!(matches!(err, CoreError::ConsentRequiredV2 { .. }));
        assert!(enqueued.lock().await.is_empty());
        assert_eq!(records.lock().await.len(), 1);
    }
}
