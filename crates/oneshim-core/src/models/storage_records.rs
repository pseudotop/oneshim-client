/// Row types returned by the web storage port.
///
/// These structs model rows retrieved from SQLite queries. They live in
/// `oneshim-core` so that the `WebStorage` port trait (also in core) can
/// reference them without pulling in the `oneshim-storage` adapter crate.

#[derive(Debug, Clone)]
pub struct FrameRecord {
    pub id: i64,
    pub timestamp: String,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
    pub resolution_w: u32,
    pub resolution_h: u32,
    pub file_path: Option<String>,
    pub ocr_text: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TagRecord {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct FocusWorkSessionRecord {
    pub id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub primary_app: String,
    pub category: String,
    pub state: String,
    pub interruption_count: u32,
    pub deep_work_secs: u64,
    pub duration_secs: u64,
}

#[derive(Debug, Clone)]
pub struct FocusInterruptionRecord {
    pub id: i64,
    pub interrupted_at: String,
    pub from_app: String,
    pub from_category: String,
    pub to_app: String,
    pub to_category: String,
    pub resumed_at: Option<String>,
    pub resumed_to_app: Option<String>,
    pub duration_secs: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct LocalSuggestionRecord {
    pub id: i64,
    pub suggestion_type: String,
    pub payload: serde_json::Value,
    pub created_at: String,
    pub shown_at: Option<String>,
    pub dismissed_at: Option<String>,
    pub acted_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HourlyMetricsRecord {
    pub hour: String,
    pub cpu_avg: f64,
    pub cpu_max: f64,
    pub memory_avg: u64,
    pub memory_max: u64,
    pub sample_count: u64,
}

#[derive(Debug, Clone)]
pub struct StorageStatsSummaryRecord {
    pub frame_count: u64,
    pub event_count: u64,
    pub metric_count: u64,
    pub oldest_data_date: Option<String>,
    pub newest_data_date: Option<String>,
    pub page_count: u64,
    pub page_size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DeletedRangeCounts {
    pub events_deleted: u64,
    pub frames_deleted: u64,
    pub metrics_deleted: u64,
    pub process_snapshots_deleted: u64,
    pub idle_periods_deleted: u64,
}

#[derive(Debug, Clone)]
pub struct EventExportRecord {
    pub event_id: String,
    pub event_type: String,
    pub timestamp: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MetricExportRecord {
    pub timestamp: String,
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub network_upload: u64,
    pub network_download: u64,
}

#[derive(Debug, Clone)]
pub struct FrameExportRecord {
    pub id: i64,
    pub timestamp: String,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
    pub resolution_w: u32,
    pub resolution_h: u32,
    pub ocr_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchFrameRow {
    pub id: i64,
    pub timestamp: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub matched_text: Option<String>,
    pub importance: Option<f32>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchEventRow {
    pub event_id: String,
    pub timestamp: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub data: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FrameTagLinkRecord {
    pub frame_id: i64,
    pub tag_id: i64,
    pub created_at: String,
}
