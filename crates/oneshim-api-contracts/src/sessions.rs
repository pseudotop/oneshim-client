use serde::{Deserialize, Serialize};

/// Path parameter for AI conversation session endpoints.
#[derive(Debug, Deserialize)]
pub struct AiSessionPath {
    pub id: String,
}

/// Request body for sending a message to an AI conversation session.
#[derive(Debug, Deserialize)]
pub struct AiSendMessageRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub total_events: u64,
    pub total_frames: u64,
    pub total_idle_secs: u64,
    pub active_duration_secs: Option<u64>,
}
