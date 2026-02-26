use serde::Serialize;

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
