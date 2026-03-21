use oneshim_core::error::CoreError;
use std::sync::atomic::Ordering;
use tracing::{debug, info};

use super::{
    DeletedRangeCounts, EventExportRecord, FrameExportRecord, FrameTagLinkRecord,
    MetricExportRecord, SearchEventRow, SearchFrameRow, SqliteStorage, StorageStatsSummaryRecord,
    FTS_AVAILABLE,
};

impl SqliteStorage {
    pub fn list_backup_tags(&self) -> Result<Vec<super::TagRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT id, name, color, created_at FROM tags ORDER BY id")
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(super::TagRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_backup_frame_tags(&self) -> Result<Vec<FrameTagLinkRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT frame_id, tag_id, created_at FROM frame_tags ORDER BY frame_id, tag_id",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(FrameTagLinkRecord {
                    frame_id: row.get(0)?,
                    tag_id: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    pub fn upsert_backup_tag(
        &self,
        id: i64,
        name: &str,
        color: &str,
        created_at: &str,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO tags (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, name, color, created_at],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to save tag: {e}")))?;

        Ok(())
    }

    pub fn upsert_backup_frame_tag(
        &self,
        frame_id: i64,
        tag_id: i64,
        created_at: &str,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO frame_tags (frame_id, tag_id, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![frame_id, tag_id, created_at],
        )
        .map_err(|e| CoreError::Internal(format!("frame-Failed to save tag: {e}")))?;

        Ok(())
    }

    pub fn upsert_backup_event(
        &self,
        event_id: &str,
        event_type: &str,
        timestamp: &str,
        app_name: Option<&str>,
        window_title: Option<&str>,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let data = serde_json::json!({
            "app_name": app_name,
            "window_title": window_title,
        })
        .to_string();

        conn.execute(
            "INSERT OR IGNORE INTO events (event_id, event_type, timestamp, data)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![event_id, event_type, timestamp, data],
        )
        .map_err(|e| CoreError::Internal(format!("event save failure: {e}")))?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_backup_frame(
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
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM frames WHERE id = ?1)",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !exists {
            conn.execute(
                "INSERT INTO frames (
                    id, timestamp, trigger_type, app_name, window_title,
                    importance, resolution_w, resolution_h, has_image, ocr_text, file_path
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, NULL)",
                rusqlite::params![
                    id,
                    timestamp,
                    trigger_type,
                    app_name,
                    window_title,
                    importance,
                    width,
                    height,
                    ocr_text,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("frame save failure: {e}")))?;
        }

        Ok(())
    }

    pub fn get_storage_stats_summary(&self) -> Result<StorageStatsSummaryRecord, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let frame_count: u64 = conn
            .query_row("SELECT COUNT(*) FROM frames", [], |row| row.get(0))
            .unwrap_or(0);
        let event_count: u64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap_or(0);
        let metric_count: u64 = conn
            .query_row("SELECT COUNT(*) FROM system_metrics", [], |row| row.get(0))
            .unwrap_or(0);

        let oldest_data_date: Option<String> = conn
            .query_row(
                "SELECT MIN(timestamp) FROM (
                    SELECT timestamp FROM events
                    UNION ALL
                    SELECT timestamp FROM frames
                    UNION ALL
                    SELECT timestamp FROM system_metrics
                )",
                [],
                |row| row.get(0),
            )
            .ok();

        let newest_data_date: Option<String> = conn
            .query_row(
                "SELECT MAX(timestamp) FROM (
                    SELECT timestamp FROM events
                    UNION ALL
                    SELECT timestamp FROM frames
                    UNION ALL
                    SELECT timestamp FROM system_metrics
                )",
                [],
                |row| row.get(0),
            )
            .ok();

        let page_count: u64 = conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap_or(0);
        let page_size: u64 = conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap_or(4096);

        Ok(StorageStatsSummaryRecord {
            frame_count,
            event_count,
            metric_count,
            oldest_data_date,
            newest_data_date,
            page_count,
            page_size,
        })
    }

    pub fn list_frame_file_paths_in_range(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT file_path FROM frames WHERE timestamp >= ?1 AND timestamp <= ?2")
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                row.get::<_, Option<String>>(0)
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut paths = Vec::new();
        for row in rows {
            if let Some(path) = row
                .map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?
                .filter(|p| !p.is_empty())
            {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn delete_data_in_range(
        &self,
        from: &str,
        to: &str,
        delete_events: bool,
        delete_frames: bool,
        delete_metrics: bool,
        delete_processes: bool,
        delete_idle: bool,
    ) -> Result<DeletedRangeCounts, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut counts = DeletedRangeCounts::default();

        if delete_events {
            counts.events_deleted = conn
                .execute(
                    "DELETE FROM events WHERE timestamp >= ?1 AND timestamp <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| CoreError::Internal(format!("event delete failure: {e}")))?
                as u64;
        }

        if delete_frames {
            counts.frames_deleted = conn
                .execute(
                    "DELETE FROM frames WHERE timestamp >= ?1 AND timestamp <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| CoreError::Internal(format!("frame delete failure: {e}")))?
                as u64;
        }

        if delete_metrics {
            counts.metrics_deleted = conn
                .execute(
                    "DELETE FROM system_metrics WHERE timestamp >= ?1 AND timestamp <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| CoreError::Internal(format!("Failed to delete metrics: {e}")))?
                as u64;

            let _ = conn.execute(
                "DELETE FROM system_metrics_hourly WHERE hour >= ?1 AND hour <= ?2",
                rusqlite::params![from, to],
            );
        }

        if delete_processes {
            counts.process_snapshots_deleted = conn
                .execute(
                    "DELETE FROM process_snapshots WHERE timestamp >= ?1 AND timestamp <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| {
                    CoreError::Internal(format!("Failed to delete process snapshots: {e}"))
                })? as u64;
        }

        if delete_idle {
            counts.idle_periods_deleted = conn
                .execute(
                    "DELETE FROM idle_periods WHERE start_time >= ?1 AND start_time <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| CoreError::Internal(format!("idle record delete failure: {e}")))?
                as u64;
        }

        Ok(counts)
    }

    /// Atomically delete all user data from every known table inside a single
    /// SQLite transaction. On any failure the transaction auto-rolls-back so
    /// the database is never left in a partially-deleted state (GDPR compliance).
    pub fn delete_all_data(&self) -> Result<(), CoreError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        // All tables created by V1-V17 migrations (excluding schema_version).
        // Order: child/referencing tables before parent tables to avoid FK issues
        // if foreign keys are ever enabled.
        const ALL_TABLES: &[&str] = &[
            // V1-V7
            "events",
            "frames",
            "system_metrics",
            "system_metrics_hourly",
            "process_snapshots",
            "idle_periods",
            "session_stats",
            "work_sessions",
            "interruptions",
            "focus_metrics",
            "suggestions",
            "local_suggestions",
            "frame_tags",
            "tags",
            // V8-V10
            "activity_segments",
            "calibration_log",
            "daily_digests",
            "weekly_digests",
            "embedding_vectors",
            "regime_overrides",
            "regimes",
            "trigger_params_snapshots",
            // V11: FTS5 virtual table
            "search_fts",
            // V12-V14
            "vector_binary_codes",
            "vector_index_meta",
            "ivf_centroids",
            "ivf_assignments",
            "gui_interactions",
            "device_identity",
            "sync_peers",
            // V15-V16
            "lan_peer_pins",
            // V17: coaching
            "coaching_events",
            "regime_goals",
            "coaching_effectiveness",
        ];

        let tx = conn
            .transaction()
            .map_err(|e| CoreError::Internal(format!("Failed to begin transaction: {e}")))?;

        for table in ALL_TABLES {
            tx.execute(&format!("DELETE FROM {table}"), [])
                .map_err(|e| {
                    CoreError::Internal(format!("GDPR delete failed on table '{table}': {e}"))
                })?;
        }

        tx.commit()
            .map_err(|e| CoreError::Internal(format!("Failed to commit GDPR deletion: {e}")))?;

        Ok(())
    }

    pub fn list_event_exports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<EventExportRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT event_id, event_type, timestamp, app_name, window_title
                 FROM events
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok(EventExportRecord {
                    event_id: row.get(0)?,
                    event_type: row.get(1)?,
                    timestamp: row.get(2)?,
                    app_name: row.get(3)?,
                    window_title: row.get(4)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_metric_exports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<MetricExportRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT timestamp, cpu_usage, memory_used, memory_total, disk_used, disk_total,
                        network_upload, network_download
                 FROM system_metrics
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok(MetricExportRecord {
                    timestamp: row.get(0)?,
                    cpu_usage: row.get(1)?,
                    memory_used: row.get(2)?,
                    memory_total: row.get(3)?,
                    disk_used: row.get(4)?,
                    disk_total: row.get(5)?,
                    network_upload: row.get(6)?,
                    network_download: row.get(7)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_frame_exports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<FrameExportRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, trigger_type, app_name, window_title, importance,
                        resolution_w, resolution_h, ocr_text
                 FROM frames
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok(FrameExportRecord {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    trigger_type: row.get(2)?,
                    app_name: row.get(3)?,
                    window_title: row.get(4)?,
                    importance: row.get(5)?,
                    resolution_w: row.get(6)?,
                    resolution_h: row.get(7)?,
                    ocr_text: row.get(8)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    pub fn count_search_frames(
        &self,
        count_sql: &str,
        pattern: Option<&str>,
    ) -> Result<u64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let count: i64 = match pattern {
            Some(p) => conn
                .query_row(count_sql, rusqlite::params![p], |row| row.get(0))
                .map_err(|e| {
                    CoreError::Internal(format!("Failed to count frame search results: {e}"))
                })?,
            None => conn
                .query_row(count_sql, [], |row| row.get(0))
                .map_err(|e| {
                    CoreError::Internal(format!("Failed to count frame search results: {e}"))
                })?,
        };

        Ok(count as u64)
    }

    pub fn search_frames_with_sql(
        &self,
        select_sql: &str,
        pattern: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchFrameRow>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(select_sql)
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        if let Some(p) = pattern {
            let rows = stmt
                .query_map(
                    rusqlite::params![p, limit.to_string(), offset.to_string()],
                    |row| {
                        Ok(SearchFrameRow {
                            id: row.get(0)?,
                            timestamp: row.get(1)?,
                            app_name: row.get(2)?,
                            window_title: row.get(3)?,
                            matched_text: row.get(4)?,
                            importance: row.get(5)?,
                            file_path: row.get(6)?,
                        })
                    },
                )
                .map_err(|e| CoreError::Internal(format!("Failed to query frames: {e}")))?;

            let mut records = Vec::new();
            for row in rows {
                records.push(
                    row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?,
                );
            }
            Ok(records)
        } else {
            let rows = stmt
                .query_map(
                    rusqlite::params![limit.to_string(), offset.to_string()],
                    |row| {
                        Ok(SearchFrameRow {
                            id: row.get(0)?,
                            timestamp: row.get(1)?,
                            app_name: row.get(2)?,
                            window_title: row.get(3)?,
                            matched_text: row.get(4)?,
                            importance: row.get(5)?,
                            file_path: row.get(6)?,
                        })
                    },
                )
                .map_err(|e| CoreError::Internal(format!("Failed to query frames: {e}")))?;

            let mut records = Vec::new();
            for row in rows {
                records.push(
                    row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?,
                );
            }
            Ok(records)
        }
    }

    pub fn count_search_events(&self, pattern: &str) -> Result<u64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events
                 WHERE app_name LIKE ?1 OR window_title LIKE ?1 OR data LIKE ?1",
                rusqlite::params![pattern],
                |row| row.get(0),
            )
            .map_err(|e| {
                CoreError::Internal(format!("Failed to count event search results: {e}"))
            })?;

        Ok(count as u64)
    }

    pub fn search_events(
        &self,
        pattern: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SearchEventRow>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT event_id, timestamp, app_name, window_title, data
                 FROM events
                 WHERE app_name LIKE ?1 OR window_title LIKE ?1 OR data LIKE ?1
                 ORDER BY timestamp DESC
                 LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(
                rusqlite::params![pattern, limit.to_string(), offset.to_string()],
                |row| {
                    Ok(SearchEventRow {
                        event_id: row.get(0)?,
                        timestamp: row.get(1)?,
                        app_name: row.get(2)?,
                        window_title: row.get(3)?,
                        data: row.get(4)?,
                    })
                },
            )
            .map_err(|e| CoreError::Internal(format!("Failed to query events: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    // --- SQLite maintenance methods ---

    /// Execute a PASSIVE WAL checkpoint. Non-blocking — does not wait for
    /// concurrent readers or writers to finish.
    pub fn wal_checkpoint_passive(&self) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE)")
            .map_err(|e| CoreError::Internal(format!("WAL checkpoint PASSIVE failed: {e}")))?;

        debug!("WAL checkpoint PASSIVE completed");
        Ok(())
    }

    /// Run VACUUM if freelist_count / page_count exceeds `threshold_percent`.
    /// Returns `true` when VACUUM was actually executed.
    pub fn maybe_vacuum(&self, threshold_percent: u64) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let freelist_count: u64 = conn
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))
            .unwrap_or(0);
        let page_count: u64 = conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap_or(1); // avoid div-by-zero

        if page_count == 0 {
            return Ok(false);
        }

        let free_pct = (freelist_count * 100) / page_count;
        if free_pct > threshold_percent {
            info!(
                "Running VACUUM: freelist={freelist_count} pages={page_count} ({free_pct}% free)"
            );
            conn.execute_batch("VACUUM")
                .map_err(|e| CoreError::Internal(format!("VACUUM failed: {e}")))?;
            Ok(true)
        } else {
            debug!(
                "VACUUM skipped: freelist={freelist_count} pages={page_count} ({free_pct}% free)"
            );
            Ok(false)
        }
    }

    /// Incrementally merge up to `pages` FTS5 b-tree pages.
    /// No-op if the search_fts table does not exist.
    pub fn fts_merge(&self, pages: u32) -> Result<(), CoreError> {
        if !FTS_AVAILABLE.load(Ordering::Relaxed) {
            return Ok(());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "INSERT INTO search_fts(search_fts, rank) VALUES('merge', ?1)",
            rusqlite::params![pages as i64],
        )
        .map_err(|e| CoreError::Internal(format!("FTS5 merge failed: {e}")))?;

        debug!("FTS5 incremental merge completed ({pages} pages)");
        Ok(())
    }

    /// Full FTS5 optimize — merges all b-tree segments into one. Expensive
    /// but dramatically speeds up subsequent FTS queries.
    /// No-op if the search_fts table does not exist.
    pub fn fts_optimize(&self) -> Result<(), CoreError> {
        if !FTS_AVAILABLE.load(Ordering::Relaxed) {
            return Ok(());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute("INSERT INTO search_fts(search_fts) VALUES('optimize')", [])
            .map_err(|e| CoreError::Internal(format!("FTS5 optimize failed: {e}")))?;

        info!("FTS5 full optimize completed");
        Ok(())
    }

    /// Run ANALYZE to refresh query planner statistics for all tables.
    pub fn run_analyze(&self) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute_batch("ANALYZE")
            .map_err(|e| CoreError::Internal(format!("ANALYZE failed: {e}")))?;

        debug!("ANALYZE completed");
        Ok(())
    }

    /// Run ANALYZE using an already-held connection guard. Use this inside
    /// methods that have already locked `self.conn` to avoid deadlocking.
    pub(super) fn run_analyze_with_conn(conn: &rusqlite::Connection) -> Result<(), CoreError> {
        conn.execute_batch("ANALYZE")
            .map_err(|e| CoreError::Internal(format!("ANALYZE failed: {e}")))?;
        debug!("ANALYZE (inline) completed");
        Ok(())
    }
}
