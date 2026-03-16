//! Integration domain ports for outbound sync, inbound inbox delivery, and
//! session/auth orchestration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::models::integration::{
    InsightPacket, IntegrationAckCursor, IntegrationCapabilityScope, IntegrationEnvelope,
    IntegrationSessionState, ProactivePrompt, QueuedInsightPacket,
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

    /// Remove acknowledged or successfully delivered queue items.
    async fn delete(&self, queue_ids: &[String]) -> Result<(), CoreError>;

    /// Read the latest acknowledged cursor returned by the remote side.
    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError>;

    /// Persist the latest acknowledged cursor from the remote side.
    async fn store_ack_cursor(&self, cursor: IntegrationAckCursor) -> Result<(), CoreError>;
}

#[async_trait]
pub trait IntegrationInboxPort: Send + Sync {
    /// List pending inbound proactive prompts/tasks.
    async fn list_pending(&self) -> Result<Vec<ProactivePrompt>, CoreError>;

    /// Acknowledge receipt of a prompt/task.
    async fn acknowledge(&self, prompt_id: &str) -> Result<(), CoreError>;

    /// Dismiss a prompt/task with an optional local reason.
    async fn dismiss(&self, prompt_id: &str, reason: Option<String>) -> Result<(), CoreError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationEgressDisposition {
    Allow,
    Deny,
    RequireUserApproval,
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
