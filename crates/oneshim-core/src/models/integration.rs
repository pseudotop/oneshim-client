use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Shared metadata envelope for outbound and inbound integration messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationEnvelope {
    pub envelope_id: String,
    pub schema_version: String,
    pub message_type: IntegrationMessageType,
    pub timestamp: DateTime<Utc>,
    pub nonce: String,
    pub origin: IntegrationOrigin,
    pub capability_scope: IntegrationCapabilityScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationMessageType {
    InsightPacket,
    ProactivePrompt,
    SessionState,
    Bootstrap,
    Ack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationOrigin {
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationCapabilityScope {
    InsightWrite,
    PromptRead,
    PromptAck,
    DevicePresenceWrite,
    SessionManage,
    PolicyRead,
}

/// Privacy-filtered outbound packet sent to an integration backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightPacket {
    pub packet_id: String,
    pub summary: String,
    #[serde(default)]
    pub derived_tags: Vec<String>,
    pub source_window: InsightSourceWindow,
    pub privacy_classification: IntegrationPrivacyClassification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_reference_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedInsightPacket {
    pub queue_id: String,
    pub envelope: IntegrationEnvelope,
    pub packet: InsightPacket,
    pub queued_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightSourceWindow {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationPrivacyClassification {
    DeviceLocal,
    DerivedSummary,
    UserApprovedAttachment,
}

/// Inbound prompt/task packet delivered to the desktop client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactivePrompt {
    pub prompt_id: String,
    pub category: ProactivePromptCategory,
    pub title: String,
    pub body: String,
    pub priority: ProactivePromptPriority,
    #[serde(default)]
    pub actions: Vec<ProactivePromptAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub provenance: PromptProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProactivePrompt {
    pub prompt: ProactivePrompt,
    pub received_at: DateTime<Utc>,
    pub status: IntegrationInboxItemStatus,
    pub status_updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismiss_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationInboxItemStatus {
    Pending,
    Acknowledged,
    Dismissed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProactivePromptCategory {
    Insight,
    Task,
    Reminder,
    Escalation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProactivePromptPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactivePromptAction {
    pub action_id: String,
    pub label: String,
    pub action_type: ProactivePromptActionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProactivePromptActionType {
    OpenLink,
    OpenSettings,
    RunAutomation,
    Dismiss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptProvenance {
    pub source_system: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationSessionState {
    pub session_id: String,
    pub device_id: String,
    pub status: IntegrationSessionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub requested_scopes: Vec<IntegrationCapabilityScope>,
    #[serde(default)]
    pub granted_scopes: Vec<IntegrationCapabilityScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ack_cursors: Vec<IntegrationAckCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationSessionStatus {
    Disconnected,
    Connecting,
    Connected,
    Degraded,
    Reconnecting,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationAckCursor {
    pub stream_id: String,
    pub cursor: String,
    pub acknowledged_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_envelope_roundtrip() {
        let envelope = IntegrationEnvelope {
            envelope_id: "env-001".to_string(),
            schema_version: "integration.envelope.v1".to_string(),
            message_type: IntegrationMessageType::InsightPacket,
            timestamp: Utc::now(),
            nonce: "nonce-001".to_string(),
            origin: IntegrationOrigin {
                device_id: "device-001".to_string(),
                workspace_id: Some("workspace-001".to_string()),
                session_id: Some("session-001".to_string()),
                source: "desktop-client".to_string(),
            },
            capability_scope: IntegrationCapabilityScope::InsightWrite,
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: IntegrationEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.envelope_id, "env-001");
        assert_eq!(parsed.message_type, IntegrationMessageType::InsightPacket);
        assert_eq!(
            parsed.capability_scope,
            IntegrationCapabilityScope::InsightWrite
        );
    }

    #[test]
    fn proactive_prompt_roundtrip() {
        let prompt = ProactivePrompt {
            prompt_id: "prompt-001".to_string(),
            category: ProactivePromptCategory::Task,
            title: "Review inbox".to_string(),
            body: "A teammate requested a review.".to_string(),
            priority: ProactivePromptPriority::High,
            actions: vec![ProactivePromptAction {
                action_id: "open-settings".to_string(),
                label: "Open Settings".to_string(),
                action_type: ProactivePromptActionType::OpenSettings,
                payload: None,
            }],
            expires_at: None,
            provenance: PromptProvenance {
                source_system: "team-server".to_string(),
                source_actor: Some("scheduler".to_string()),
                correlation_id: Some("corr-001".to_string()),
            },
        };

        let json = serde_json::to_string(&prompt).unwrap();
        let parsed: ProactivePrompt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.prompt_id, "prompt-001");
        assert_eq!(parsed.priority, ProactivePromptPriority::High);
        assert_eq!(parsed.actions.len(), 1);
    }

    #[test]
    fn stored_prompt_roundtrip() {
        let stored = StoredProactivePrompt {
            prompt: ProactivePrompt {
                prompt_id: "prompt-001".to_string(),
                category: ProactivePromptCategory::Insight,
                title: "Review focus".to_string(),
                body: "A server-side insight is ready.".to_string(),
                priority: ProactivePromptPriority::Medium,
                actions: Vec::new(),
                expires_at: None,
                provenance: PromptProvenance {
                    source_system: "team-server".to_string(),
                    source_actor: None,
                    correlation_id: Some("corr-002".to_string()),
                },
            },
            received_at: Utc::now(),
            status: IntegrationInboxItemStatus::Pending,
            status_updated_at: Utc::now(),
            dismiss_reason: None,
        };

        let json = serde_json::to_string(&stored).unwrap();
        let parsed: StoredProactivePrompt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.prompt.prompt_id, "prompt-001");
        assert_eq!(parsed.status, IntegrationInboxItemStatus::Pending);
    }

    #[test]
    fn session_state_roundtrip() {
        let state = IntegrationSessionState {
            session_id: "session-001".to_string(),
            device_id: "device-001".to_string(),
            status: IntegrationSessionStatus::Connected,
            connected_at: Some(Utc::now()),
            last_heartbeat_at: Some(Utc::now()),
            requested_scopes: vec![IntegrationCapabilityScope::SessionManage],
            granted_scopes: vec![IntegrationCapabilityScope::SessionManage],
            ack_cursors: vec![IntegrationAckCursor {
                stream_id: "inbox".to_string(),
                cursor: "42".to_string(),
                acknowledged_at: Utc::now(),
            }],
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: IntegrationSessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, IntegrationSessionStatus::Connected);
        assert_eq!(parsed.granted_scopes.len(), 1);
        assert_eq!(parsed.ack_cursors[0].cursor, "42");
    }

    #[test]
    fn queued_insight_packet_roundtrip() {
        let queued = QueuedInsightPacket {
            queue_id: "queue-001".to_string(),
            envelope: IntegrationEnvelope {
                envelope_id: "env-001".to_string(),
                schema_version: "integration.envelope.v1".to_string(),
                message_type: IntegrationMessageType::InsightPacket,
                timestamp: Utc::now(),
                nonce: "nonce-001".to_string(),
                origin: IntegrationOrigin {
                    device_id: "device-001".to_string(),
                    workspace_id: None,
                    session_id: None,
                    source: "desktop-client".to_string(),
                },
                capability_scope: IntegrationCapabilityScope::InsightWrite,
            },
            packet: InsightPacket {
                packet_id: "packet-001".to_string(),
                summary: "summary".to_string(),
                derived_tags: vec!["focus".to_string()],
                source_window: InsightSourceWindow {
                    started_at: Utc::now(),
                    ended_at: Utc::now(),
                },
                privacy_classification: IntegrationPrivacyClassification::DerivedSummary,
                audit_reference_id: Some("audit-001".to_string()),
            },
            queued_at: Utc::now(),
        };

        let json = serde_json::to_string(&queued).unwrap();
        let parsed: QueuedInsightPacket = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.queue_id, "queue-001");
        assert_eq!(parsed.packet.packet_id, "packet-001");
    }
}
