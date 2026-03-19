mod calibration_store_impl;
pub(crate) mod edge_intelligence;
mod events;
mod focus_storage_impl;
mod frames;
mod fts_search_impl;
mod integration_query_impl;
mod maintenance;
mod metrics;
mod override_store_impl;
mod tags;
pub mod vector_index_impl;
pub mod vector_store_impl;
mod web_storage_impl;

use oneshim_core::error::CoreError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::migration;

pub struct SqliteStorage {
    pub(super) conn: Arc<Mutex<Connection>>,
    pub(super) retention_days: u32,
}

impl SqliteStorage {
    pub fn open(path: &Path, retention_days: u32) -> Result<Self, CoreError> {
        let conn = Connection::open(path)
            .map_err(|e| CoreError::Internal(format!("Failed to open SQLite database: {e}")))?;

        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA cache_size=8000;
            PRAGMA temp_store=MEMORY;
            PRAGMA mmap_size=268435456;
            PRAGMA page_size=4096;
            ",
        )
        .map_err(|e| CoreError::Internal(format!("Failed to apply PRAGMA settings: {e}")))?;

        migration::run_migrations(&conn)
            .map_err(|e| CoreError::Internal(format!("migration failure: {e}")))?;

        info!("SQLite save initialize: {}", path.display());

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            retention_days,
        })
    }

    pub fn open_in_memory(retention_days: u32) -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory().map_err(|e| {
            CoreError::Internal(format!("Failed to create in-memory SQLite database: {e}"))
        })?;

        migration::run_migrations(&conn)
            .map_err(|e| CoreError::Internal(format!("migration failure: {e}")))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            retention_days,
        })
    }

    /// Expose the underlying connection Arc for shared-connection adapters
    /// (e.g., `SqliteVectorStore`).
    pub fn connection_arc(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// 동기 SQLite 읽기/단순 쓰기 연산을 spawn_blocking으로 격리한다.
    /// 클로저는 커넥션의 공유 참조를 받는다.
    pub(super) async fn with_conn<F, T>(&self, f: F) -> Result<T, CoreError>
    where
        F: FnOnce(&Connection) -> Result<T, CoreError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
            f(&guard)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// 동기 SQLite 트랜잭션 연산을 spawn_blocking으로 격리한다.
    /// 클로저는 커넥션의 배타적(가변) 참조를 받는다.
    #[allow(dead_code)]
    pub(super) async fn with_conn_mut<F, T>(&self, f: F) -> Result<T, CoreError>
    where
        F: FnOnce(&mut Connection) -> Result<T, CoreError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
            f(&mut guard)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// Ensure a device identity row exists in the `device_identity` table.
    ///
    /// On first call (empty table), generates a UUID v4 device_id and inserts
    /// it with the given device_name. On subsequent calls, returns the existing
    /// identity. The table enforces `id = 1` (singleton row).
    ///
    /// Returns `(device_id, device_name)`.
    pub fn ensure_device_identity(&self, device_name: &str) -> Result<(String, String), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;

        // Try to read existing identity first.
        let existing: Option<(String, String)> = conn
            .query_row(
                "SELECT device_id, device_name FROM device_identity WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some(identity) = existing {
            return Ok(identity);
        }

        // First launch -- generate a new UUID v4 device_id.
        let device_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO device_identity (id, device_id, device_name) VALUES (1, ?1, ?2)",
            rusqlite::params![device_id, device_name],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to insert device identity: {e}")))?;

        info!(
            device_id = %device_id,
            device_name = %device_name,
            "device identity generated (first launch)"
        );

        Ok((device_id, device_name.to_string()))
    }

    /// Reset the device identity by deleting the existing row and generating
    /// a new one. This allows users to disassociate from their sync history.
    ///
    /// Returns the new `(device_id, device_name)`.
    pub fn reset_device_identity(&self, device_name: &str) -> Result<(String, String), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;

        conn.execute("DELETE FROM device_identity WHERE id = 1", [])
            .map_err(|e| CoreError::Internal(format!("Failed to delete device identity: {e}")))?;

        drop(conn); // Release lock before calling ensure_device_identity

        self.ensure_device_identity(device_name)
    }
}

// Record types are canonical in oneshim-core; re-exported here for backward compatibility.
pub use oneshim_core::models::storage_records::{
    DeletedRangeCounts, EventExportRecord, FocusInterruptionRecord, FocusWorkSessionRecord,
    FrameExportRecord, FrameRecord, FrameTagLinkRecord, HourlyMetricsRecord, LocalSuggestionRecord,
    MetricExportRecord, SearchEventRow, SearchFrameRow, StorageStatsSummaryRecord, TagRecord,
};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use oneshim_core::models::activity::{ProcessSnapshot, ProcessSnapshotEntry, SessionStats};
    use oneshim_core::models::event::{ContextEvent, Event, UserEvent, UserEventType};
    use oneshim_core::models::system::{NetworkInfo, SystemMetrics};
    #[allow(deprecated)]
    use oneshim_core::models::work_session::{
        AppCategory, FocusMetrics, Interruption, LocalSuggestion,
    };
    use oneshim_core::ports::storage::{MetricsStorage, StorageService};
    use uuid::Uuid;

    pub(crate) fn make_user_event() -> Event {
        Event::User(UserEvent {
            event_id: Uuid::new_v4(),
            event_type: UserEventType::WindowChange,
            timestamp: Utc::now(),
            app_name: "Code".to_string(),
            window_title: "test.rs".to_string(),
        })
    }

    fn make_context_event() -> Event {
        Event::Context(ContextEvent {
            app_name: "Firefox".to_string(),
            window_title: "ONESHIM".to_string(),
            prev_app_name: Some("Code".to_string()),
            timestamp: Utc::now(),
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn save_and_get_events() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage.save_event(&make_user_event()).await.unwrap();
        storage.save_event(&make_context_event()).await.unwrap();

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let events = storage.get_events(from, to, 100).await.unwrap();
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn pending_events() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage.save_event(&make_user_event()).await.unwrap();
        storage.save_event(&make_user_event()).await.unwrap();

        let pending = storage.get_pending_events(100).await.unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn enforce_retention() {
        let storage = SqliteStorage::open_in_memory(0).unwrap(); // 0-day retention triggers immediate cleanup
        storage.save_event(&make_user_event()).await.unwrap();

        {
            let conn = storage.conn.lock().unwrap();
            conn.execute("UPDATE events SET is_sent = 1", []).unwrap();
        } // MutexGuard await
        let deleted = storage.enforce_retention().await.unwrap();
        assert!(deleted >= 1);
    }

    #[tokio::test]
    async fn empty_storage() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let events = storage.get_events(from, to, 100).await.unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn get_events_invalid_time_range() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage.save_event(&make_user_event()).await.unwrap();

        let from = Utc::now() + Duration::hours(1);
        let to = Utc::now() - Duration::hours(1);
        let events = storage.get_events(from, to, 100).await.unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn mark_nonexistent_ids_no_error() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let ids = vec!["nonexistent1".to_string(), "nonexistent2".to_string()];
        let result = storage.mark_as_sent(&ids).await;
        assert!(result.is_ok());

        let result = storage.mark_as_sent(&[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn large_batch_insert() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        for _ in 0..100 {
            storage.save_event(&make_user_event()).await.unwrap();
        }

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let events = storage.get_events(from, to, 200).await.unwrap();
        assert_eq!(events.len(), 100);
    }

    #[tokio::test]
    async fn retention_does_not_delete_unsent() {
        let storage = SqliteStorage::open_in_memory(0).unwrap();

        storage.save_event(&make_user_event()).await.unwrap();

        let deleted = storage.enforce_retention().await.unwrap();

        assert_eq!(deleted, 0);

        let pending = storage.get_pending_events(100).await.unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn mark_as_sent_affects_pending() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let event = make_user_event();
        let event_id = match &event {
            Event::User(e) => e.event_id.to_string(),
            _ => panic!("unexpected event type"),
        };
        storage.save_event(&event).await.unwrap();

        let pending = storage.get_pending_events(100).await.unwrap();
        assert_eq!(pending.len(), 1);

        storage.mark_as_sent(&[event_id]).await.unwrap();

        let pending = storage.get_pending_events(100).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn concurrent_save_and_get() {
        let storage = std::sync::Arc::new(SqliteStorage::open_in_memory(30).unwrap());

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let s = storage.clone();
                tokio::spawn(async move {
                    for _ in 0..10 {
                        s.save_event(&make_user_event()).await.unwrap();
                    }
                })
            })
            .collect();

        for h in handles {
            h.await.unwrap();
        }

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let events = storage.get_events(from, to, 200).await.unwrap();
        assert_eq!(events.len(), 100);
    }

    fn make_system_metrics() -> SystemMetrics {
        SystemMetrics {
            timestamp: Utc::now(),
            cpu_usage: 45.5,
            memory_used: 8 * 1024 * 1024 * 1024,   // 8GB
            memory_total: 16 * 1024 * 1024 * 1024, // 16GB
            disk_used: 100 * 1024 * 1024 * 1024,
            disk_total: 500 * 1024 * 1024 * 1024,
            network: Some(NetworkInfo {
                upload_speed: 1000,
                download_speed: 5000,
                is_connected: true,
            }),
        }
    }

    fn make_process_snapshot() -> ProcessSnapshot {
        ProcessSnapshot {
            timestamp: Utc::now(),
            processes: vec![
                ProcessSnapshotEntry {
                    pid: 1234,
                    name: "firefox".to_string(),
                    cpu_usage: 10.5,
                    memory_bytes: 512 * 1024 * 1024, // 512MB
                },
                ProcessSnapshotEntry {
                    pid: 5678,
                    name: "code".to_string(),
                    cpu_usage: 5.2,
                    memory_bytes: 256 * 1024 * 1024, // 256MB
                },
            ],
        }
    }

    #[tokio::test]
    async fn save_and_get_metrics() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage.save_metrics(&make_system_metrics()).await.unwrap();

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let metrics = storage.get_metrics(from, to, 100).await.unwrap();
        assert_eq!(metrics.len(), 1);
        assert!(metrics[0].cpu_usage > 40.0);
    }

    #[tokio::test]
    async fn cleanup_old_metrics() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage.save_metrics(&make_system_metrics()).await.unwrap();

        let future = Utc::now() + Duration::days(1);
        let deleted = storage.cleanup_old_metrics(future).await.unwrap();
        assert_eq!(deleted, 1);
    }

    #[tokio::test]
    async fn save_and_get_process_snapshot() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage
            .save_process_snapshot(&make_process_snapshot())
            .await
            .unwrap();

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let snapshots = storage.get_process_snapshots(from, to, 100).await.unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].processes.len(), 2);
    }

    #[tokio::test]
    async fn idle_period_lifecycle() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let start = Utc::now();
        let id = storage.start_idle_period(start).await.unwrap();
        assert!(id > 0);

        let ongoing = storage.get_ongoing_idle_period().await.unwrap();
        assert!(ongoing.is_some());
        let (ongoing_id, _) = ongoing.unwrap();
        assert_eq!(ongoing_id, id);

        let end = start + Duration::minutes(5);
        storage.end_idle_period(id, end).await.unwrap();

        let ongoing = storage.get_ongoing_idle_period().await.unwrap();
        assert!(ongoing.is_none());

        let from = Utc::now() - Duration::hours(1);
        let to = Utc::now() + Duration::hours(1);
        let periods = storage.get_idle_periods(from, to).await.unwrap();
        assert_eq!(periods.len(), 1);
        assert!(periods[0].duration_secs.is_some());
    }

    #[tokio::test]
    async fn session_stats_lifecycle() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let session_id = "test-session-123";
        let stats = SessionStats {
            session_id: session_id.to_string(),
            started_at: Utc::now(),
            ended_at: None,
            total_events: 10,
            total_frames: 5,
            total_idle_secs: 60,
        };

        storage.upsert_session(&stats).await.unwrap();

        let loaded = storage.get_session(session_id).await.unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.total_events, 10);

        storage
            .increment_session_counters(session_id, 5, 2, 30)
            .await
            .unwrap();

        let loaded = storage.get_session(session_id).await.unwrap().unwrap();
        assert_eq!(loaded.total_events, 15);
        assert_eq!(loaded.total_frames, 7);
        assert_eq!(loaded.total_idle_secs, 90);

        storage.end_session(session_id, Utc::now()).await.unwrap();

        let loaded = storage.get_session(session_id).await.unwrap().unwrap();
        assert!(loaded.ended_at.is_some());
    }

    #[tokio::test]
    async fn session_not_found() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let result = storage.get_session("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn create_and_get_tags() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let tag = storage.create_tag("work", "#3b82f6").unwrap();
        assert_eq!(tag.name, "work");
        assert_eq!(tag.color, "#3b82f6");

        let tags = storage.get_all_tags().unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn delete_tag() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let tag = storage.create_tag("temp", "#ef4444").unwrap();
        let deleted = storage.delete_tag(tag.id).unwrap();
        assert!(deleted);

        let tags = storage.get_all_tags().unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn get_tag_by_id() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let tag = storage.create_tag("important", "#f59e0b").unwrap();
        let found = storage.get_tag(tag.id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "important");

        let not_found = storage.get_tag(99999).unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn update_tag() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let tag = storage.create_tag("old", "#000000").unwrap();
        let updated = storage.update_tag(tag.id, "new", "#ffffff").unwrap();
        assert!(updated);

        let found = storage.get_tag(tag.id).unwrap().unwrap();
        assert_eq!(found.name, "new");
        assert_eq!(found.color, "#ffffff");
    }

    #[test]
    fn frame_tag_operations() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        {
            let conn = storage.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO frames (timestamp, trigger_type, app_name, window_title, importance, resolution_w, resolution_h, has_image)
                 VALUES ('2024-01-01T00:00:00Z', 'manual', 'test', 'test', 0.5, 1920, 1080, 0)",
                [],
            )
            .unwrap();
        }

        let tag1 = storage.create_tag("tag1", "#ff0000").unwrap();
        let tag2 = storage.create_tag("tag2", "#00ff00").unwrap();

        storage.add_tag_to_frame(1, tag1.id).unwrap();
        storage.add_tag_to_frame(1, tag2.id).unwrap();

        let tags = storage.get_tags_for_frame(1).unwrap();
        assert_eq!(tags.len(), 2);

        let frames = storage.get_frames_by_tag(tag1.id, 100).unwrap();
        assert_eq!(frames.len(), 1);

        let removed = storage.remove_tag_from_frame(1, tag1.id).unwrap();
        assert!(removed);

        let tags = storage.get_tags_for_frame(1).unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn duplicate_tag_name_fails() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage.create_tag("unique", "#000000").unwrap();
        let result = storage.create_tag("unique", "#ffffff");
        assert!(result.is_err());
    }

    #[test]
    fn add_tag_to_frame_idempotent() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        {
            let conn = storage.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO frames (timestamp, trigger_type, app_name, window_title, importance, resolution_w, resolution_h, has_image)
                 VALUES ('2024-01-01T00:00:00Z', 'manual', 'test', 'test', 0.5, 1920, 1080, 0)",
                [],
            )
            .unwrap();
        }

        let tag = storage.create_tag("tag", "#000000").unwrap();

        storage.add_tag_to_frame(1, tag.id).unwrap();
        storage.add_tag_to_frame(1, tag.id).unwrap();

        let tags = storage.get_tags_for_frame(1).unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn work_session_lifecycle() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let session = storage
            .start_work_session("Code", AppCategory::Development)
            .unwrap();
        assert!(session.id > 0);
        assert_eq!(session.category, AppCategory::Development);

        let active = storage.get_active_work_session().unwrap();
        assert!(active.is_some());

        storage.end_work_session(session.id).unwrap();

        let active = storage.get_active_work_session().unwrap();
        assert!(active.is_none());
    }

    #[test]
    fn interruption_tracking() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let session = storage
            .start_work_session("Code", AppCategory::Development)
            .unwrap();
        let _ = session; // session ID
        let interruption = Interruption::new(
            0,
            "Code".to_string(),
            "Slack".to_string(),
            None, // snapshot_frame_id
        );

        let int_id = storage.record_interruption(&interruption).unwrap();
        assert!(int_id > 0);

        let pending = storage.get_pending_interruption().unwrap();
        assert!(pending.is_some());

        storage.record_interruption_resume(int_id, "Code").unwrap();

        let pending = storage.get_pending_interruption().unwrap();
        assert!(pending.is_none());
    }

    #[test]
    fn focus_metrics_lifecycle() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let metrics = storage.get_or_create_today_focus_metrics().unwrap();
        assert_eq!(metrics.deep_work_secs, 0);

        // increment_focus_metrics(date, total_active_secs, deep_work_secs, communication_secs, context_switches, interruption_count)
        let today = Utc::now().format("%Y-%m-%d").to_string();
        storage
            .increment_focus_metrics(&today, 300, 200, 100, 5, 2)
            .unwrap();

        let updated = storage.get_or_create_today_focus_metrics().unwrap();
        assert_eq!(updated.total_active_secs, 300);
        assert_eq!(updated.deep_work_secs, 200);
        assert_eq!(updated.communication_secs, 100);
        assert_eq!(updated.context_switches, 5);
        assert_eq!(updated.interruption_count, 2);

        let full_metrics = FocusMetrics::new(updated.period_start, updated.period_end);
        storage.update_focus_metrics(&today, &full_metrics).unwrap();
    }

    #[test]
    #[allow(deprecated)]
    fn local_suggestion_persistence() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let suggestion = LocalSuggestion::NeedFocusTime {
            communication_ratio: 0.6,
            suggested_focus_mins: 25,
        };

        let id = storage.save_local_suggestion(&suggestion).unwrap();
        assert!(id > 0);

        storage.mark_suggestion_shown(id).unwrap();
        storage.mark_suggestion_dismissed(id).unwrap();

        let suggestion2 = LocalSuggestion::TakeBreak {
            continuous_work_mins: 90,
        };
        let id2 = storage.save_local_suggestion(&suggestion2).unwrap();
        storage.mark_suggestion_acted(id2).unwrap();
    }

    #[test]
    fn daily_digest_save_and_get_roundtrip() {
        use oneshim_core::models::daily_digest::{
            DailyDigest, DailyInsight, DailyStatistics, DigestHighlight, HighlightType,
        };
        use oneshim_core::ports::web_storage::WebStorage;

        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let today = Utc::now().date_naive();

        let digest = DailyDigest {
            date: today,
            insight: Some(DailyInsight {
                narrative: "Great focus day!".to_string(),
                highlights: vec![DigestHighlight {
                    highlight_type: HighlightType::Achievement,
                    text: "2h deep work".to_string(),
                    segment_id: Some("seg-001".to_string()),
                }],
            }),
            timeline: vec![],
            statistics: DailyStatistics {
                deep_work_hours: 4.2,
                ..DailyStatistics::default()
            },
            generated_at: Utc::now(),
        };

        storage.save_daily_digest(&digest).unwrap();

        let loaded = storage.get_daily_digest(&today.to_string()).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.date, today);
        assert!(loaded.insight.is_some());
        let insight = loaded.insight.unwrap();
        assert_eq!(insight.narrative, "Great focus day!");
        assert_eq!(insight.highlights.len(), 1);
        assert!((loaded.statistics.deep_work_hours - 4.2).abs() < f32::EPSILON);
    }

    #[test]
    fn daily_digest_list_ordering() {
        use chrono::Days;
        use oneshim_core::models::daily_digest::{DailyDigest, DailyStatistics};
        use oneshim_core::ports::web_storage::WebStorage;

        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let today = Utc::now().date_naive();

        for offset in 0..3 {
            let date = today - Days::new(offset);
            let digest = DailyDigest {
                date,
                insight: None,
                timeline: vec![],
                statistics: DailyStatistics::default(),
                generated_at: Utc::now(),
            };
            storage.save_daily_digest(&digest).unwrap();
        }

        let digests = storage.list_daily_digests(10).unwrap();
        assert_eq!(digests.len(), 3);
        // Newest first
        assert_eq!(digests[0].date, today);
        assert_eq!(digests[1].date, today - Days::new(1));
    }

    #[test]
    fn daily_digest_get_nonexistent_returns_none() {
        use oneshim_core::ports::web_storage::WebStorage;

        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let result = storage.get_daily_digest("2020-01-01").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn segments_for_date_query() {
        use oneshim_core::ports::web_storage::WebStorage;

        let storage = SqliteStorage::open_in_memory(30).unwrap();

        // Insert a test segment
        {
            let conn = storage.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO activity_segments (id, start_time, end_time, duration_secs, trigger_reason, dominant_category, event_count, avg_importance)
                 VALUES ('seg-001', '2026-03-19T09:00:00Z', '2026-03-19T10:00:00Z', 3600, 'SCORE_HIGH', 'Development', 50, 0.8)",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO activity_segments (id, start_time, end_time, duration_secs, trigger_reason, dominant_category, event_count, avg_importance)
                 VALUES ('seg-002', '2026-03-20T09:00:00Z', '2026-03-20T10:00:00Z', 3600, 'SCORE_HIGH', 'Communication', 30, 0.5)",
                [],
            ).unwrap();
        }

        let segments = storage.get_segments_for_date("2026-03-19").unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].segment_id, "seg-001");
        assert_eq!(segments[0].dominant_category, "Development");
        assert_eq!(segments[0].duration_secs, 3600);

        // Different date returns different segment
        let segments2 = storage.get_segments_for_date("2026-03-20").unwrap();
        assert_eq!(segments2.len(), 1);
        assert_eq!(segments2[0].segment_id, "seg-002");

        // Non-existent date returns empty
        let empty = storage.get_segments_for_date("2020-01-01").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn app_category_parsing() {
        assert_eq!(
            SqliteStorage::parse_app_category("Communication"),
            AppCategory::Communication
        );
        assert_eq!(
            SqliteStorage::parse_app_category("Development"),
            AppCategory::Development
        );
        assert_eq!(
            SqliteStorage::parse_app_category("Unknown"),
            AppCategory::Other
        );
    }

    #[test]
    fn ensure_device_identity_generates_uuid_on_first_call() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let (device_id, device_name) = storage.ensure_device_identity("Test Machine").unwrap();

        assert!(!device_id.is_empty());
        // Validate UUID v4 format (8-4-4-4-12 hex chars)
        assert_eq!(device_id.len(), 36);
        assert_eq!(device_name, "Test Machine");
    }

    #[test]
    fn ensure_device_identity_returns_same_id_on_second_call() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let (id1, _) = storage.ensure_device_identity("Machine A").unwrap();
        let (id2, name2) = storage.ensure_device_identity("Machine B").unwrap();

        // Second call must return the FIRST identity, not generate a new one.
        assert_eq!(id1, id2);
        assert_eq!(name2, "Machine A"); // Original name preserved
    }

    #[test]
    fn ensure_device_identity_persists_across_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let id1 = {
            let storage = SqliteStorage::open(&db_path, 30).unwrap();
            let (id, _) = storage.ensure_device_identity("Laptop").unwrap();
            id
        };

        // Reopen the database
        let id2 = {
            let storage = SqliteStorage::open(&db_path, 30).unwrap();
            let (id, name) = storage.ensure_device_identity("Different Name").unwrap();
            assert_eq!(name, "Laptop"); // Original name preserved
            id
        };

        assert_eq!(id1, id2);
    }

    #[test]
    fn reset_device_identity_generates_new_uuid() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let (id1, name1) = storage.ensure_device_identity("Original").unwrap();
        assert_eq!(name1, "Original");

        let (id2, name2) = storage.reset_device_identity("Reset Device").unwrap();
        assert_eq!(name2, "Reset Device");

        // After reset, device_id must be different
        assert_ne!(id1, id2, "reset must generate a new device_id");
        assert_eq!(id2.len(), 36, "new id must be valid UUID format");
    }

    #[test]
    fn reset_device_identity_allows_re_ensure() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        let (id1, _) = storage.ensure_device_identity("First").unwrap();
        let (id2, _) = storage.reset_device_identity("Second").unwrap();
        assert_ne!(id1, id2);

        // After reset, ensure_device_identity returns the new identity
        let (id3, name3) = storage.ensure_device_identity("Third").unwrap();
        assert_eq!(
            id2, id3,
            "ensure after reset must return the reset identity"
        );
        assert_eq!(name3, "Second"); // Name from reset is preserved
    }
}
