//! Integration domain ports for outbound egress, inbound inbox delivery, and
//! session/auth orchestration.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::models::integration::{
    InsightPacket, IntegrationAckCursor, IntegrationAuthContext, IntegrationAuthStatus,
    IntegrationCapabilityScope, IntegrationDeviceAuthorizationFlow, IntegrationEgressDisposition,
    IntegrationEnvelope, IntegrationInboxItemStatus, IntegrationInsightAuditRecord,
    IntegrationInsightCandidate, IntegrationOutboundPayload, IntegrationPromptReceipt,
    IntegrationSessionState, QueuedIntegrationEgressMessage, StoredProactivePrompt,
};
use crate::models::storage_records::LocalSuggestionRecord;

#[async_trait]
pub trait IntegrationSessionPort: Send + Sync {
    /// Connect or resume an outbound integration session for the requested scopes.
    async fn connect(
        &self,
        requested_scopes: Vec<IntegrationCapabilityScope>,
    ) -> Result<IntegrationSessionState, CoreError>;

    /// Return the current session state, if one exists.
    async fn current_session(&self) -> Result<Option<IntegrationSessionState>, CoreError>;

    /// Send a liveness heartbeat and return the refreshed session state.
    async fn heartbeat(&self, session_id: &str) -> Result<IntegrationSessionState, CoreError>;

    /// Persist the latest acknowledged cursor for the active session.
    async fn store_ack_cursor(
        &self,
        session_id: &str,
        cursor: IntegrationAckCursor,
    ) -> Result<IntegrationSessionState, CoreError>;

    /// Disconnect an established integration session.
    async fn disconnect(&self, session_id: &str) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationSessionStorePort: Send + Sync {
    /// Load the last persisted integration session state, if one exists.
    async fn load(&self) -> Result<Option<IntegrationSessionState>, CoreError>;

    /// Persist the latest integration session state snapshot.
    async fn store(&self, state: IntegrationSessionState) -> Result<(), CoreError>;

    /// Clear any persisted integration session state.
    async fn clear(&self) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationAuthPort: Send + Sync {
    /// Resolve outbound session auth material for the requested scopes and resource.
    async fn resolve_session_auth(
        &self,
        requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationAuthContext, CoreError>;

    /// Return the current runtime status of the integration auth profile.
    async fn current_auth_status(&self) -> Result<IntegrationAuthStatus, CoreError>;

    /// Start a device authorization flow if the auth profile supports it.
    async fn start_device_authorization(
        &self,
        requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationDeviceAuthorizationFlow, CoreError>;

    /// Poll a pending device authorization flow.
    async fn poll_device_authorization(
        &self,
        flow_id: &str,
    ) -> Result<IntegrationAuthStatus, CoreError>;

    /// Cancel a pending device authorization flow.
    async fn cancel_device_authorization(&self, flow_id: &str) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationEgressPort: Send + Sync {
    /// Queue a typed outbound integration message for delivery.
    async fn enqueue_message(
        &self,
        envelope: IntegrationEnvelope,
        payload: IntegrationOutboundPayload,
    ) -> Result<(), CoreError>;

    /// Queue a privacy-filtered outbound insight packet.
    async fn enqueue_insight(
        &self,
        envelope: IntegrationEnvelope,
        packet: InsightPacket,
    ) -> Result<(), CoreError> {
        self.enqueue_message(envelope, IntegrationOutboundPayload::Insight(packet))
            .await
    }

    /// Flush queued outbound messages to the remote integration backend.
    async fn flush(&self) -> Result<usize, CoreError>;

    /// Read the latest acknowledged cursor returned by the remote side.
    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError>;
}

#[async_trait]
pub trait IntegrationInsightProducerPort: Send + Sync {
    /// Collect locally derived insight candidates and enqueue them for outbound delivery.
    async fn produce_pending(&self) -> Result<usize, CoreError>;
}

#[async_trait]
pub trait IntegrationInsightSourcePort: Send + Sync {
    /// Stable namespace used for durable checkpoint storage.
    fn checkpoint_namespace(&self) -> &'static str;

    /// Return locally derived outbound insight candidates after the checkpoint cursor.
    ///
    /// Implementations must return candidates in stable ascending cursor order so
    /// the producer can safely persist progress after each successful enqueue.
    async fn list_candidates_after(
        &self,
        after_cursor: Option<String>,
        limit: usize,
    ) -> Result<Vec<IntegrationInsightCandidate>, CoreError>;
}

#[async_trait]
pub trait LocalSuggestionQueryPort: Send + Sync {
    /// List locally derived focus suggestions in ascending id order after the given id.
    async fn list_local_suggestions_after(
        &self,
        after_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<LocalSuggestionRecord>, CoreError>;
}

#[async_trait]
pub trait IntegrationOutboxPort: Send + Sync {
    /// Persist an outbound integration message before transport delivery.
    async fn enqueue_message(
        &self,
        envelope: IntegrationEnvelope,
        payload: IntegrationOutboundPayload,
    ) -> Result<String, CoreError>;

    /// List pending outbound messages in delivery order.
    async fn list_pending(
        &self,
        limit: usize,
    ) -> Result<Vec<QueuedIntegrationEgressMessage>, CoreError>;

    /// Count currently pending outbound packets.
    async fn pending_count(&self) -> Result<usize, CoreError>;

    /// Remove acknowledged or successfully delivered queue items.
    async fn delete(&self, queue_ids: &[String]) -> Result<(), CoreError>;

    /// Read the latest acknowledged cursor returned by the remote side.
    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError>;

    /// Persist the latest acknowledged cursor from the remote side.
    async fn store_ack_cursor(&self, cursor: IntegrationAckCursor) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationPromptReceiptStorePort: Send + Sync {
    /// Persist a local inbox lifecycle transition and its corresponding outbound
    /// prompt receipt message atomically.
    async fn record_prompt_receipt(
        &self,
        prompt_id: &str,
        envelope: IntegrationEnvelope,
        receipt: IntegrationPromptReceipt,
    ) -> Result<String, CoreError>;
}

#[async_trait]
pub trait IntegrationCheckpointStorePort: Send + Sync {
    /// Load a producer-specific checkpoint cursor.
    async fn load_checkpoint(&self, namespace: &str) -> Result<Option<String>, CoreError>;

    /// Persist a producer-specific checkpoint cursor.
    async fn store_checkpoint(&self, namespace: &str, cursor: String) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationInboxPort: Send + Sync {
    /// Pull new inbound prompts/tasks from the integration backend.
    async fn refresh(&self) -> Result<usize, CoreError>;

    /// List pending inbound proactive prompts/tasks.
    async fn list_pending(&self) -> Result<Vec<StoredProactivePrompt>, CoreError>;

    /// Acknowledge receipt of a prompt/task.
    async fn acknowledge(&self, prompt_id: &str) -> Result<(), CoreError>;

    /// Dismiss a prompt/task with an optional local reason.
    async fn dismiss(&self, prompt_id: &str, reason: Option<String>) -> Result<(), CoreError>;

    /// Read the latest acknowledged inbox cursor returned by the remote side.
    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError>;
}

#[async_trait]
pub trait IntegrationInboxStorePort: Send + Sync {
    /// Upsert inbound prompts/tasks received from the remote side.
    ///
    /// Implementations must treat `prompt_id` as a durable identity and avoid
    /// resetting local lifecycle state (for example, acknowledged or dismissed)
    /// when the same prompt is redelivered by the remote side.
    async fn upsert_prompts(&self, prompts: Vec<StoredProactivePrompt>) -> Result<(), CoreError>;

    /// List locally pending prompts/tasks.
    async fn list_pending(&self) -> Result<Vec<StoredProactivePrompt>, CoreError>;

    /// List locally pending prompts/tasks that have not yet been presented.
    async fn list_unpresented(&self, limit: usize)
        -> Result<Vec<StoredProactivePrompt>, CoreError>;

    /// Count currently pending prompts/tasks.
    async fn pending_count(&self) -> Result<usize, CoreError>;

    /// Mark a prompt/task as presented to the local user experience.
    async fn mark_presented(
        &self,
        prompt_id: &str,
        presented_at: DateTime<Utc>,
    ) -> Result<(), CoreError>;

    /// Update the lifecycle state of a stored prompt/task.
    async fn update_status(
        &self,
        prompt_id: &str,
        status: IntegrationInboxItemStatus,
        reason: Option<String>,
    ) -> Result<(), CoreError>;

    /// Remove or mark expired prompts whose expiration time has passed.
    async fn expire_stale(&self) -> Result<usize, CoreError>;

    /// Read the latest acknowledged cursor returned by the remote side.
    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError>;

    /// Persist the latest acknowledged cursor from the remote side.
    async fn store_ack_cursor(&self, cursor: IntegrationAckCursor) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationPromptPresenterPort: Send + Sync {
    /// Present a proactive prompt to the local desktop user experience.
    async fn present_prompt(&self, prompt: &StoredProactivePrompt) -> Result<(), CoreError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationEgressDecision {
    pub disposition: IntegrationEgressDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub audit_required: bool,
}

impl IntegrationEgressDecision {
    pub fn allow() -> Self {
        Self {
            disposition: IntegrationEgressDisposition::Allow,
            reason: None,
            audit_required: true,
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            disposition: IntegrationEgressDisposition::Deny,
            reason: Some(reason.into()),
            audit_required: true,
        }
    }

    pub fn require_user_approval(reason: impl Into<String>) -> Self {
        Self {
            disposition: IntegrationEgressDisposition::RequireUserApproval,
            reason: Some(reason.into()),
            audit_required: true,
        }
    }
}

#[async_trait]
pub trait IntegrationEgressPolicyPort: Send + Sync {
    /// Evaluate whether an outbound insight packet may leave the device.
    async fn authorize_insight(
        &self,
        envelope: &IntegrationEnvelope,
        packet: &InsightPacket,
    ) -> Result<IntegrationEgressDecision, CoreError>;
}

#[async_trait]
pub trait IntegrationAuditPort: Send + Sync {
    /// Persist an auditable record for an outbound integration insight decision.
    async fn record_insight_decision(
        &self,
        record: IntegrationInsightAuditRecord,
    ) -> Result<(), CoreError>;

    /// Read recent auditable insight decisions in reverse chronological order.
    async fn recent_insight_decisions(
        &self,
        limit: usize,
    ) -> Result<Vec<IntegrationInsightAuditRecord>, CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egress_decision_builders_cover_all_dispositions() {
        let allow = IntegrationEgressDecision::allow();
        assert_eq!(allow.disposition, IntegrationEgressDisposition::Allow);
        assert!(allow.audit_required);

        let deny = IntegrationEgressDecision::deny("policy denied");
        assert_eq!(deny.disposition, IntegrationEgressDisposition::Deny);
        assert_eq!(deny.reason.as_deref(), Some("policy denied"));

        let approval = IntegrationEgressDecision::require_user_approval("needs consent");
        assert_eq!(
            approval.disposition,
            IntegrationEgressDisposition::RequireUserApproval
        );
        assert_eq!(approval.reason.as_deref(), Some("needs consent"));
    }
}
