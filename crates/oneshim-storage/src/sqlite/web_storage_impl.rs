use chrono::{DateTime, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::activity::SessionStats;
use oneshim_core::models::storage_records::{
    DeletedRangeCounts, EventExportRecord, FocusInterruptionRecord, FocusWorkSessionRecord,
    FrameExportRecord, FrameRecord, FrameTagLinkRecord, HourlyMetricsRecord, LocalSuggestionRecord,
    MetricExportRecord, SearchEventRow, SearchFrameRow, SegmentDetailRecord,
    StorageStatsSummaryRecord, SuggestionRecord, TagRecord,
};
use oneshim_core::models::work_session::FocusMetrics;
use oneshim_core::ports::web_storage::WebStorage;

use super::SqliteStorage;

impl WebStorage for SqliteStorage {
    fn count_events_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError> {
        SqliteStorage::count_events_in_range(self, from, to)
    }

    fn count_frames_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError> {
        SqliteStorage::count_frames_in_range(self, from, to)
    }

    fn get_frames(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<FrameRecord>, CoreError> {
        SqliteStorage::get_frames(self, from, to, limit)
    }

    fn get_frame_file_path(&self, frame_id: i64) -> Result<Option<String>, CoreError> {
        SqliteStorage::get_frame_file_path(self, frame_id)
    }

    fn get_storage_stats_summary(&self) -> Result<StorageStatsSummaryRecord, CoreError> {
        SqliteStorage::get_storage_stats_summary(self)
    }

    fn list_frame_file_paths_in_range(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, CoreError> {
        SqliteStorage::list_frame_file_paths_in_range(self, from, to)
    }

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
    ) -> Result<DeletedRangeCounts, CoreError> {
        SqliteStorage::delete_data_in_range(
            self,
            from,
            to,
            delete_events,
            delete_frames,
            delete_metrics,
            delete_processes,
            delete_idle,
        )
    }

    fn delete_all_data(&self) -> Result<DeletedRangeCounts, CoreError> {
        SqliteStorage::delete_all_data(self)
    }

    fn count_search_frames(
        &self,
        count_sql: &str,
        pattern: Option<&str>,
    ) -> Result<u64, CoreError> {
        SqliteStorage::count_search_frames(self, count_sql, pattern)
    }

    fn search_frames_with_sql(
        &self,
        select_sql: &str,
        pattern: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchFrameRow>, CoreError> {
        SqliteStorage::search_frames_with_sql(self, select_sql, pattern, limit, offset)
    }

    fn count_search_events(&self, pattern: &str) -> Result<u64, CoreError> {
        SqliteStorage::count_search_events(self, pattern)
    }

    fn search_events(
        &self,
        pattern: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchEventRow>, CoreError> {
        SqliteStorage::search_events(self, pattern, limit, offset)
    }

    fn get_all_tags(&self) -> Result<Vec<TagRecord>, CoreError> {
        SqliteStorage::get_all_tags(self)
    }

    fn get_tag(&self, tag_id: i64) -> Result<Option<TagRecord>, CoreError> {
        SqliteStorage::get_tag(self, tag_id)
    }

    fn get_tag_ids_for_frames(
        &self,
        frame_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<i64>>, CoreError> {
        SqliteStorage::get_tag_ids_for_frames(self, frame_ids)
    }

    fn create_tag(&self, name: &str, color: &str) -> Result<TagRecord, CoreError> {
        SqliteStorage::create_tag(self, name, color)
    }

    fn update_tag(&self, tag_id: i64, name: &str, color: &str) -> Result<bool, CoreError> {
        SqliteStorage::update_tag(self, tag_id, name, color)
    }

    fn delete_tag(&self, tag_id: i64) -> Result<bool, CoreError> {
        SqliteStorage::delete_tag(self, tag_id)
    }

    fn get_tags_for_frame(&self, frame_id: i64) -> Result<Vec<TagRecord>, CoreError> {
        SqliteStorage::get_tags_for_frame(self, frame_id)
    }

    fn add_tag_to_frame(&self, frame_id: i64, tag_id: i64) -> Result<(), CoreError> {
        SqliteStorage::add_tag_to_frame(self, frame_id, tag_id)
    }

    fn remove_tag_from_frame(&self, frame_id: i64, tag_id: i64) -> Result<bool, CoreError> {
        SqliteStorage::remove_tag_from_frame(self, frame_id, tag_id)
    }

    fn get_app_durations_by_date(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, i64)>, CoreError> {
        SqliteStorage::get_app_durations_by_date(self, from, to)
    }

    fn get_daily_active_secs(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>, CoreError> {
        SqliteStorage::get_daily_active_secs(self, from, to)
    }

    fn list_session_stats(&self, limit: usize) -> Result<Vec<SessionStats>, CoreError> {
        SqliteStorage::list_session_stats(self, limit)
    }

    fn get_or_create_focus_metrics(&self, date: &str) -> Result<FocusMetrics, CoreError> {
        SqliteStorage::get_or_create_focus_metrics(self, date)
    }

    fn get_recent_focus_metrics(
        &self,
        days: usize,
    ) -> Result<Vec<(String, FocusMetrics)>, CoreError> {
        SqliteStorage::get_recent_focus_metrics(self, days)
    }

    fn list_work_sessions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<FocusWorkSessionRecord>, CoreError> {
        SqliteStorage::list_work_sessions(self, from, to, limit)
    }

    fn list_interruptions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<FocusInterruptionRecord>, CoreError> {
        SqliteStorage::list_interruptions(self, from, to, limit)
    }

    fn list_recent_local_suggestions(
        &self,
        cutoff: &str,
        limit: usize,
    ) -> Result<Vec<LocalSuggestionRecord>, CoreError> {
        SqliteStorage::list_recent_local_suggestions(self, cutoff, limit)
    }

    fn mark_suggestion_shown(&self, suggestion_id: i64) -> Result<(), CoreError> {
        SqliteStorage::mark_suggestion_shown(self, suggestion_id)
    }

    fn mark_suggestion_dismissed(&self, suggestion_id: i64) -> Result<(), CoreError> {
        SqliteStorage::mark_suggestion_dismissed(self, suggestion_id)
    }

    fn mark_suggestion_acted(&self, suggestion_id: i64) -> Result<(), CoreError> {
        SqliteStorage::mark_suggestion_acted(self, suggestion_id)
    }

    fn list_suggestions(&self, limit: usize) -> Result<Vec<SuggestionRecord>, CoreError> {
        SqliteStorage::list_suggestions(self, limit)
    }

    fn dismiss_unified_suggestion(&self, suggestion_id: &str) -> Result<bool, CoreError> {
        SqliteStorage::dismiss_unified_suggestion(self, suggestion_id)
    }

    fn has_recent_server_suggestions(&self, lookback_secs: u64) -> Result<bool, CoreError> {
        SqliteStorage::has_recent_server_suggestions(self, lookback_secs)
    }

    fn list_weekly_digests(
        &self,
        limit: usize,
    ) -> Result<Vec<oneshim_core::models::weekly_digest::WeeklyDigest>, CoreError> {
        let guard = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
        let mut stmt = guard
            .prepare(
                "SELECT stats_json, comparison_json, llm_narrative FROM weekly_digests ORDER BY week_start DESC LIMIT ?1",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare weekly_digests query: {e}")))?;
        let digests: Vec<oneshim_core::models::weekly_digest::WeeklyDigest> = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                let stats_json: String = row.get(0)?;
                let comparison_json: Option<String> = row.get(1)?;
                let llm_narrative: Option<String> = row.get(2)?;
                Ok((stats_json, comparison_json, llm_narrative))
            })
            .map_err(|e| CoreError::Internal(format!("Failed to query weekly_digests: {e}")))?
            .filter_map(|r| r.ok())
            .filter_map(|(stats_json, comparison_json, llm_narrative)| {
                let mut digest: oneshim_core::models::weekly_digest::WeeklyDigest =
                    serde_json::from_str(&stats_json).ok()?;
                if let Some(ref cj) = comparison_json {
                    digest.comparison = serde_json::from_str(cj).ok();
                }
                digest.llm_narrative = llm_narrative;
                Some(digest)
            })
            .collect();
        Ok(digests)
    }

    fn get_current_week_digest(
        &self,
    ) -> Result<Option<oneshim_core::models::weekly_digest::WeeklyDigest>, CoreError> {
        let digests = self.list_weekly_digests(1)?;
        // The most recent digest is the current week if it overlaps with now
        Ok(digests.into_iter().next())
    }

    fn save_weekly_digest(
        &self,
        digest: &oneshim_core::models::weekly_digest::WeeklyDigest,
    ) -> Result<(), CoreError> {
        let guard = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
        let stats_json = serde_json::to_string(digest)
            .map_err(|e| CoreError::Internal(format!("Failed to serialize digest: {e}")))?;
        let comparison_json = digest
            .comparison
            .as_ref()
            .map(|c| serde_json::to_string(c).unwrap_or_default());
        let week_start = digest.week_start.to_rfc3339();
        let week_end = digest.week_end.to_rfc3339();

        guard
            .execute(
                "INSERT OR REPLACE INTO weekly_digests (week_start, week_end, stats_json, comparison_json, llm_narrative)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![week_start, week_end, stats_json, comparison_json, digest.llm_narrative],
            )
            .map_err(|e| CoreError::Internal(format!("Failed to save weekly digest: {e}")))?;
        Ok(())
    }

    fn list_backup_tags(&self) -> Result<Vec<TagRecord>, CoreError> {
        SqliteStorage::list_backup_tags(self)
    }

    fn list_backup_frame_tags(&self) -> Result<Vec<FrameTagLinkRecord>, CoreError> {
        SqliteStorage::list_backup_frame_tags(self)
    }

    fn list_event_exports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<EventExportRecord>, CoreError> {
        SqliteStorage::list_event_exports(self, from, to)
    }

    fn list_metric_exports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<MetricExportRecord>, CoreError> {
        SqliteStorage::list_metric_exports(self, from, to)
    }

    fn list_frame_exports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<FrameExportRecord>, CoreError> {
        SqliteStorage::list_frame_exports(self, from, to)
    }

    fn list_hourly_metrics_since(&self, from: &str) -> Result<Vec<HourlyMetricsRecord>, CoreError> {
        SqliteStorage::list_hourly_metrics_since(self, from)
    }

    fn upsert_backup_tag(
        &self,
        id: i64,
        name: &str,
        color: &str,
        created_at: &str,
    ) -> Result<(), CoreError> {
        SqliteStorage::upsert_backup_tag(self, id, name, color, created_at)
    }

    fn upsert_backup_frame_tag(
        &self,
        frame_id: i64,
        tag_id: i64,
        created_at: &str,
    ) -> Result<(), CoreError> {
        SqliteStorage::upsert_backup_frame_tag(self, frame_id, tag_id, created_at)
    }

    fn upsert_backup_event(
        &self,
        event_id: &str,
        event_type: &str,
        timestamp: &str,
        app_name: Option<&str>,
        window_title: Option<&str>,
    ) -> Result<(), CoreError> {
        SqliteStorage::upsert_backup_event(
            self,
            event_id,
            event_type,
            timestamp,
            app_name,
            window_title,
        )
    }

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
    ) -> Result<(), CoreError> {
        SqliteStorage::upsert_backup_frame(
            self,
            id,
            timestamp,
            trigger_type,
            app_name,
            window_title,
            importance,
            width,
            height,
            ocr_text,
        )
    }

    fn get_segment_details(
        &self,
        segment_ids: &[String],
    ) -> Result<std::collections::HashMap<String, SegmentDetailRecord>, CoreError> {
        if segment_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;

        // Check if the activity_segments table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='activity_segments'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !table_exists {
            return Ok(std::collections::HashMap::new());
        }

        let mut map = std::collections::HashMap::new();
        for id in segment_ids {
            let result = conn.query_row(
                "SELECT id, start_time, end_time, duration_secs, llm_summary, dominant_category, regime_id
                 FROM activity_segments WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(SegmentDetailRecord {
                        segment_id: row.get(0)?,
                        start_time: row.get(1)?,
                        end_time: row.get(2)?,
                        duration_secs: row.get::<_, i64>(3)? as u64,
                        llm_summary: row.get(4)?,
                        dominant_category: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        regime_label: row.get(6)?,
                    })
                },
            );
            if let Ok(record) = result {
                map.insert(id.clone(), record);
            }
        }
        Ok(map)
    }
}
