use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub labels: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetrics {
    pub session_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub suggestions_received: u32,
    pub suggestions_accepted: u32,
    pub suggestions_rejected: u32,
    pub batches_sent: u32,
    pub batches_failed: u32,
    pub avg_sse_latency_ms: Option<f64>,
}
