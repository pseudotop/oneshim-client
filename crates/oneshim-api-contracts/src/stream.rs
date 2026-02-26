use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AiRuntimeStatus {
    pub ocr_source: String,
    pub llm_source: String,
    pub ocr_fallback_reason: Option<String>,
    pub llm_fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum RealtimeEvent {
    #[serde(rename = "metrics")]
    Metrics(MetricsUpdate),
    #[serde(rename = "frame")]
    Frame(FrameUpdate),
    #[serde(rename = "idle")]
    Idle(IdleUpdate),
    #[serde(rename = "ai_runtime_status")]
    AiRuntimeStatus(AiRuntimeStatus),
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsUpdate {
    pub timestamp: String,
    pub cpu_usage: f32,
    pub memory_percent: f32,
    pub memory_used: u64,
    pub memory_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameUpdate {
    pub id: i64,
    pub timestamp: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdleUpdate {
    pub is_idle: bool,
    pub idle_secs: u64,
}
