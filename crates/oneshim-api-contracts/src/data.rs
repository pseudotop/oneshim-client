use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct DeleteRangeRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub data_types: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub success: bool,
    pub events_deleted: u64,
    pub frames_deleted: u64,
    pub metrics_deleted: u64,
    pub process_snapshots_deleted: u64,
    pub idle_periods_deleted: u64,
    pub message: String,
}

impl DeleteResult {
    pub fn empty() -> Self {
        Self {
            success: true,
            events_deleted: 0,
            frames_deleted: 0,
            metrics_deleted: 0,
            process_snapshots_deleted: 0,
            idle_periods_deleted: 0,
            message: String::new(),
        }
    }

    pub fn total(&self) -> u64 {
        self.events_deleted
            + self.frames_deleted
            + self.metrics_deleted
            + self.process_snapshots_deleted
            + self.idle_periods_deleted
    }
}
