use chrono::{DateTime, Utc};

use crate::error::CoreError;
use crate::models::activity::SessionStats;
use crate::models::daily_digest::DailyDigest;
use crate::models::storage_records::{
    DeletedRangeCounts, EventExportRecord, FocusInterruptionRecord, FocusWorkSessionRecord,
    FrameExportRecord, FrameRecord, FrameTagLinkRecord, GuiInteractionRecord, HourlyMetricsRecord,
    LocalSuggestionRecord, MetricExportRecord, NewGuiInteraction, SearchEventRow, SearchFrameRow,
    SegmentDetailRecord, SegmentSummaryRecord, StorageStatsSummaryRecord, SuggestionRecord,
    TagRecord,
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

    /// List unified suggestions from the V8 `suggestions` table, newest first.
    /// Only returns non-dismissed suggestions.
    fn list_suggestions(&self, limit: usize) -> Result<Vec<SuggestionRecord>, CoreError>;

    /// Dismiss a unified suggestion by its string `suggestion_id`.
    fn dismiss_unified_suggestion(&self, suggestion_id: &str) -> Result<bool, CoreError>;

    /// Check whether server-sourced (LLM_SERVER) suggestions exist within the
    /// given lookback window (in seconds). Used for server coexistence gating.
    fn has_recent_server_suggestions(&self, lookback_secs: u64) -> Result<bool, CoreError>;

    /// List recent weekly digests, newest first.
    fn list_weekly_digests(
        &self,
        limit: usize,
    ) -> Result<Vec<crate::models::weekly_digest::WeeklyDigest>, CoreError>;

    /// Get the digest for the current week (if exists).
    fn get_current_week_digest(
        &self,
    ) -> Result<Option<crate::models::weekly_digest::WeeklyDigest>, CoreError>;

    /// Save a weekly digest. Upserts by week_start.
    fn save_weekly_digest(
        &self,
        digest: &crate::models::weekly_digest::WeeklyDigest,
    ) -> Result<(), CoreError>;

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

    /// Retrieve segment details for the given segment IDs.
    /// Used to enrich vector search results with segment metadata.
    /// Default implementation returns an empty map (for stores without segment support).
    fn get_segment_details(
        &self,
        _segment_ids: &[String],
    ) -> Result<std::collections::HashMap<String, SegmentDetailRecord>, CoreError> {
        Ok(std::collections::HashMap::new())
    }

    /// Save a daily digest. Upserts by date.
    fn save_daily_digest(&self, _digest: &DailyDigest) -> Result<(), CoreError> {
        Ok(()) // No-op default — storage adapters override
    }

    /// Get the daily digest for a specific date (YYYY-MM-DD).
    fn get_daily_digest(&self, _date: &str) -> Result<Option<DailyDigest>, CoreError> {
        Ok(None)
    }

    /// List recent daily digests, newest first.
    fn list_daily_digests(&self, _limit: usize) -> Result<Vec<DailyDigest>, CoreError> {
        Ok(vec![])
    }

    /// Get activity segment summaries for a given date (YYYY-MM-DD).
    /// Used as input for daily digest generation.
    fn get_segments_for_date(&self, _date: &str) -> Result<Vec<SegmentSummaryRecord>, CoreError> {
        Ok(vec![])
    }

    /// Save a GUI interaction event to the gui_interactions table (V13).
    ///
    /// **Privacy contract**: Callers MUST apply PII filtering to `element_text`
    /// before calling this method. The storage adapter applies a basic email/phone
    /// scrub as defense-in-depth, but upstream filtering via `sanitize_title_with_level()`
    /// is the primary safeguard.
    fn save_gui_interaction(&self, _input: &NewGuiInteraction<'_>) -> Result<(), CoreError> {
        Ok(()) // No-op default — storage adapters override
    }

    /// List GUI interaction events for a given segment.
    fn list_gui_interactions_for_segment(
        &self,
        _segment_id: &str,
    ) -> Result<Vec<GuiInteractionRecord>, CoreError> {
        Ok(vec![])
    }

    /// Query coaching events, newest first, with pagination.
    /// Default returns empty — storage adapters that support coaching tables override.
    fn query_coaching_events(
        &self,
        _limit: u32,
        _offset: u32,
    ) -> Result<Vec<crate::models::coaching::CoachingEventRow>, CoreError> {
        Ok(vec![])
    }
}
