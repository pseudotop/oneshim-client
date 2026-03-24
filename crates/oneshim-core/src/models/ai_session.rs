//! AI conversation session models — JSONL protocol types, session metadata,
//! and context assembly data structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Session Metadata ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTransport {
    Subprocess,
    HttpApi,
    LocalLlm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Starting,
    Active,
    Idle,
    Recovering,
    Failed,
    Terminated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSessionInfo {
    pub session_id: String,
    pub provider_name: String,
    pub model: String,
    pub state: SessionState,
    pub transport: SessionTransport,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub turn_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub transport: SessionTransport,
    pub surface_id: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub tools_enabled: bool,
}

// ── JSONL Inbound Messages ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InboundMessage {
    Message(SessionMessage),
    Control { action: ControlAction },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<MessageContext>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    Cancel,
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_app: Option<String>,
}

// ── Attachments ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Attachment {
    Image {
        mime: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    File {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    Directory {
        path: String,
    },
    Skill {
        skill_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
    },
    AppReference {
        app_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_title: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub endpoint: String,
}

// ── JSONL Outbound Messages ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutboundMessage {
    Text {
        content: String,
        done: bool,
    },
    ToolUse {
        tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<serde_json::Value>,
        status: ToolUseStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },
    Result {
        content: String,
        done: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
    },
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
    Control {
        action: ControlAction,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolUseStatus {
    Started,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ── Context Assembly ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptContext {
    pub user_profile: UserProfileSummary,
    pub current_regime: String,
    pub recent_activity: ActivitySummary,
    pub suggestion_history: SuggestionPatterns,
    pub available_skills: Vec<SkillInfo>,
    pub system_info: SystemInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfileSummary {
    pub preferred_language: Option<String>,
    pub work_style: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivitySummary {
    pub top_apps: Vec<String>,
    pub active_minutes: u32,
    pub idle_minutes: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuggestionPatterns {
    pub total_received: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub active_app: Option<String>,
    pub timezone: String,
}

// ── Session Audit Entry ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAuditEntry {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub category: SessionAuditCategory,
    pub event_type: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionAuditCategory {
    Session,
    Message,
    ToolUse,
    Attachment,
    Error,
    Process,
    Usage,
    Context,
    PullApi,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_inbound_message() {
        let msg = InboundMessage::Message(SessionMessage {
            role: MessageRole::User,
            content: "hello".to_string(),
            attachments: vec![],
            tools: None,
            context: None,
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"message\""));
        assert!(json.contains("\"role\":\"user\""));
    }

    #[test]
    fn serializes_outbound_text() {
        let msg = OutboundMessage::Text {
            content: "hi".to_string(),
            done: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"done\":false"));
    }

    #[test]
    fn serializes_attachment_image() {
        let att = Attachment::Image {
            mime: "image/png".to_string(),
            path: Some("/tmp/test.png".to_string()),
            data: None,
        };
        let json = serde_json::to_string(&att).unwrap();
        assert!(json.contains("\"kind\":\"image\""));
    }

    #[test]
    fn deserializes_outbound_error() {
        let json = r#"{"type":"error","code":"rate_limit","message":"exceeded","retryable":true}"#;
        let msg: OutboundMessage = serde_json::from_str(json).unwrap();
        match msg {
            OutboundMessage::Error {
                code, retryable, ..
            } => {
                assert_eq!(code, "rate_limit");
                assert!(retryable);
            }
            _ => panic!("expected Error variant"),
        }
    }

    #[test]
    fn session_state_roundtrip() {
        let state = SessionState::Active;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"active\"");
        let parsed: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SessionState::Active);
    }
}
