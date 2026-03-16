//! Integration domain ports for outbound sync, inbound inbox delivery, and
//! session/auth orchestration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::models::integration::{
    InsightPacket, IntegrationAckCursor, IntegrationAuthContext, IntegrationAuthStatus,
    IntegrationCapabilityScope, IntegrationDeviceAuthorizationFlow, IntegrationEgressDisposition,
    IntegrationEnvelope, IntegrationInboxItemStatus, IntegrationInsightAuditRecord,
    IntegrationSessionState, QueuedInsightPacket, StoredProactivePrompt,
};

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
pub trait InsightSyncPort: Send + Sync {
    /// Queue a privacy-filtered outbound insight packet.
    async fn enqueue(
        &self,
        envelope: IntegrationEnvelope,
        packet: InsightPacket,
    ) -> Result<(), CoreError>;

    /// Flush queued packets to the remote integration backend.
    async fn flush(&self) -> Result<usize, CoreError>;

    /// Read the latest acknowledged cursor returned by the remote side.
    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError>;
}

#[async_trait]
pub trait IntegrationOutboxPort: Send + Sync {
    /// Persist an outbound insight packet before transport delivery.
    async fn enqueue_insight(
        &self,
        envelope: IntegrationEnvelope,
        packet: InsightPacket,
    ) -> Result<String, CoreError>;

    /// List pending outbound packets in delivery order.
    async fn list_pending(&self, limit: usize) -> Result<Vec<QueuedInsightPacket>, CoreError>;

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

    /// Count currently pending prompts/tasks.
    async fn pending_count(&self) -> Result<usize, CoreError>;

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
