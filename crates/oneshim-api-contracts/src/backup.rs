use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct BackupQuery {
    #[serde(default = "default_true")]
    pub include_settings: bool,
    #[serde(default = "default_true")]
    pub include_tags: bool,
    #[serde(default)]
    pub include_events: bool,
    #[serde(default)]
    pub include_frames: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub version: String,
    pub created_at: String,
    pub app_version: String,
    pub includes: BackupIncludes,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupIncludes {
    pub settings: bool,
    pub tags: bool,
    pub events: bool,
    pub frames: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagBackup {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FrameTagBackup {
    pub frame_id: i64,
    pub tag_id: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingsBackup {
    pub capture_enabled: bool,
    pub capture_interval_secs: u64,
    pub idle_threshold_secs: u64,
    pub metrics_interval_secs: u64,
    pub web_port: u16,
    pub notification_enabled: bool,
    pub idle_notification_mins: u64,
    pub long_session_notification_mins: u64,
    pub high_usage_threshold_percent: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventBackup {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FrameBackup {
    pub id: i64,
    pub timestamp: String,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
    pub width: i32,
    pub height: i32,
    pub ocr_text: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupArchive {
    pub metadata: BackupMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<SettingsBackup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<TagBackup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_tags: Option<Vec<FrameTagBackup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<EventBackup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<FrameBackup>>,
}

#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub success: bool,
    pub restored: RestoredCounts,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RestoredCounts {
    pub settings: bool,
    pub tags: u64,
    pub frame_tags: u64,
    pub events: u64,
    pub frames: u64,
}
