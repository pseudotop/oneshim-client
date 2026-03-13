pub(crate) mod edge_intelligence;
mod events;
mod frames;
mod maintenance;
mod metrics;
mod tags;
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
    use oneshim_core::models::work_session::{
        AppCategory, FocusMetrics, Interruption, LocalSuggestion,
    };
    use oneshim_core::ports::storage::{MetricsStorage, StorageService};
    use uuid::Uuid;

    fn make_user_event() -> Event {
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
}
