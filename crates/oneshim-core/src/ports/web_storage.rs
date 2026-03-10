use chrono::{DateTime, Utc};

use crate::error::CoreError;
use crate::models::activity::SessionStats;
use crate::models::storage_records::{
    DeletedRangeCounts, EventExportRecord, FocusInterruptionRecord, FocusWorkSessionRecord,
    FrameExportRecord, FrameRecord, FrameTagLinkRecord, HourlyMetricsRecord,
    LocalSuggestionRecord, MetricExportRecord, SearchEventRow, SearchFrameRow,
    StorageStatsSummaryRecord, TagRecord,
};
use crate::models::work_session::FocusMetrics;
use crate::ports::storage::{MetricsStorage, StorageService};

pub trait WebStorage: StorageService + MetricsStorage + Send + Sync {
    fn count_events_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError>;
    fn count_frames_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError>;
    fn get_frames(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<FrameRecord>, CoreError>;
    fn get_frame_file_path(&self, frame_id: i64) -> Result<Option<String>, CoreError>;

    fn get_storage_stats_summary(&self) -> Result<StorageStatsSummaryRecord, CoreError>;
    fn list_frame_file_paths_in_range(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, CoreError>;
    #[allow(clippy::too_many_arguments)]
    fn delete_data_in_range(
        &self,
        from: &str,
        to: &str,
        delete_events: bool,
        delete_frames: bool,
        delete_metrics: bool,
        delete_processes: bool,
        delete_idle: bool,
    ) -> Result<DeletedRangeCounts, CoreError>;
    fn delete_all_data(&self) -> Result<DeletedRangeCounts, CoreError>;

    fn count_search_frames(&self, count_sql: &str, pattern: Option<&str>)
        -> Result<u64, CoreError>;
    fn search_frames_with_sql(
        &self,
        select_sql: &str,
        pattern: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchFrameRow>, CoreError>;
    fn count_search_events(&self, pattern: &str) -> Result<u64, CoreError>;
    fn search_events(
        &self,
        pattern: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchEventRow>, CoreError>;

    fn get_all_tags(&self) -> Result<Vec<TagRecord>, CoreError>;
    fn get_tag(&self, tag_id: i64) -> Result<Option<TagRecord>, CoreError>;
    fn get_tag_ids_for_frames(
        &self,
        frame_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<i64>>, CoreError>;
    fn create_tag(&self, name: &str, color: &str) -> Result<TagRecord, CoreError>;
    fn update_tag(&self, tag_id: i64, name: &str, color: &str) -> Result<bool, CoreError>;
    fn delete_tag(&self, tag_id: i64) -> Result<bool, CoreError>;
    fn get_tags_for_frame(&self, frame_id: i64) -> Result<Vec<TagRecord>, CoreError>;
    fn add_tag_to_frame(&self, frame_id: i64, tag_id: i64) -> Result<(), CoreError>;
    fn remove_tag_from_frame(&self, frame_id: i64, tag_id: i64) -> Result<bool, CoreError>;

    fn get_app_durations_by_date(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, i64)>, CoreError>;
    fn get_daily_active_secs(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>, CoreError>;
    fn list_session_stats(&self, limit: usize) -> Result<Vec<SessionStats>, CoreError>;

    fn get_or_create_focus_metrics(&self, date: &str) -> Result<FocusMetrics, CoreError>;
    fn get_recent_focus_metrics(
        &self,
        days: usize,
    ) -> Result<Vec<(String, FocusMetrics)>, CoreError>;
    fn list_work_sessions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<FocusWorkSessionRecord>, CoreError>;
    fn list_interruptions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<FocusInterruptionRecord>, CoreError>;
    fn list_recent_local_suggestions(
        &self,
        cutoff: &str,
        limit: usize,
    ) -> Result<Vec<LocalSuggestionRecord>, CoreError>;
    fn mark_suggestion_shown(&self, suggestion_id: i64) -> Result<(), CoreError>;
    fn mark_suggestion_dismissed(&self, suggestion_id: i64) -> Result<(), CoreError>;
    fn mark_suggestion_acted(&self, suggestion_id: i64) -> Result<(), CoreError>;

    fn list_backup_tags(&self) -> Result<Vec<TagRecord>, CoreError>;
    fn list_backup_frame_tags(&self) -> Result<Vec<FrameTagLinkRecord>, CoreError>;
    fn list_event_exports(&self, from: &str, to: &str)
        -> Result<Vec<EventExportRecord>, CoreError>;
    fn list_metric_exports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<MetricExportRecord>, CoreError>;
    fn list_frame_exports(&self, from: &str, to: &str)
        -> Result<Vec<FrameExportRecord>, CoreError>;
    fn list_hourly_metrics_since(&self, from: &str) -> Result<Vec<HourlyMetricsRecord>, CoreError>;
    fn upsert_backup_tag(
        &self,
        id: i64,
        name: &str,
        color: &str,
        created_at: &str,
    ) -> Result<(), CoreError>;
    fn upsert_backup_frame_tag(
        &self,
        frame_id: i64,
        tag_id: i64,
        created_at: &str,
    ) -> Result<(), CoreError>;
    fn upsert_backup_event(
        &self,
        event_id: &str,
        event_type: &str,
        timestamp: &str,
        app_name: Option<&str>,
        window_title: Option<&str>,
    ) -> Result<(), CoreError>;
    #[allow(clippy::too_many_arguments)]
    fn upsert_backup_frame(
        &self,
        id: i64,
        timestamp: &str,
        trigger_type: &str,
        app_name: &str,
        window_title: &str,
        importance: f32,
        width: i32,
        height: i32,
        ocr_text: Option<&str>,
    ) -> Result<(), CoreError>;
}
