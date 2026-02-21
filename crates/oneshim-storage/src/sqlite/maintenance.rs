use oneshim_core::error::CoreError;

use super::{
    DeletedRangeCounts, EventExportRecord, FrameExportRecord, FrameTagLinkRecord,
    MetricExportRecord, SearchEventRow, SearchFrameRow, SqliteStorage, StorageStatsSummaryRecord,
};

impl SqliteStorage {
    pub fn list_backup_tags(&self) -> Result<Vec<super::TagRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT id, name, color, created_at FROM tags ORDER BY id")
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(super::TagRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_backup_frame_tags(&self) -> Result<Vec<FrameTagLinkRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT frame_id, tag_id, created_at FROM frame_tags ORDER BY frame_id, tag_id",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(FrameTagLinkRecord {
                    frame_id: row.get(0)?,
                    tag_id: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO tags (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, name, color, created_at],
        )
        .map_err(|e| CoreError::Internal(format!("태그 저장 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO frame_tags (frame_id, tag_id, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![frame_id, tag_id, created_at],
        )
        .map_err(|e| CoreError::Internal(format!("프레임-태그 저장 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

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
        .map_err(|e| CoreError::Internal(format!("이벤트 저장 실패: {e}")))?;

        Ok(())
    }

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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("프레임 저장 실패: {e}")))?;
        }

        Ok(())
    }

    pub fn get_storage_stats_summary(&self) -> Result<StorageStatsSummaryRecord, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT file_path FROM frames WHERE timestamp >= ?1 AND timestamp <= ?2")
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                row.get::<_, Option<String>>(0)
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut paths = Vec::new();
        for row in rows {
            if let Some(path) = row
                .map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?
                .filter(|p| !p.is_empty())
            {
                paths.push(path);
            }
        }
        Ok(paths)
    }

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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut counts = DeletedRangeCounts::default();

        if delete_events {
            counts.events_deleted = conn
                .execute(
                    "DELETE FROM events WHERE timestamp >= ?1 AND timestamp <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| CoreError::Internal(format!("이벤트 삭제 실패: {e}")))?
                as u64;
        }

        if delete_frames {
            counts.frames_deleted = conn
                .execute(
                    "DELETE FROM frames WHERE timestamp >= ?1 AND timestamp <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| CoreError::Internal(format!("프레임 삭제 실패: {e}")))?
                as u64;
        }

        if delete_metrics {
            counts.metrics_deleted = conn
                .execute(
                    "DELETE FROM system_metrics WHERE timestamp >= ?1 AND timestamp <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| CoreError::Internal(format!("메트릭 삭제 실패: {e}")))?
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
                .map_err(|e| CoreError::Internal(format!("프로세스 스냅샷 삭제 실패: {e}")))?
                as u64;
        }

        if delete_idle {
            counts.idle_periods_deleted = conn
                .execute(
                    "DELETE FROM idle_periods WHERE start_time >= ?1 AND start_time <= ?2",
                    rusqlite::params![from, to],
                )
                .map_err(|e| CoreError::Internal(format!("유휴 기록 삭제 실패: {e}")))?
                as u64;
        }

        Ok(counts)
    }

    pub fn delete_all_data(&self) -> Result<DeletedRangeCounts, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let events_deleted = conn
            .execute("DELETE FROM events", [])
            .map_err(|e| CoreError::Internal(format!("이벤트 삭제 실패: {e}")))?
            as u64;
        let frames_deleted = conn
            .execute("DELETE FROM frames", [])
            .map_err(|e| CoreError::Internal(format!("프레임 삭제 실패: {e}")))?
            as u64;
        let metrics_deleted = conn
            .execute("DELETE FROM system_metrics", [])
            .map_err(|e| CoreError::Internal(format!("메트릭 삭제 실패: {e}")))?
            as u64;
        let _ = conn.execute("DELETE FROM system_metrics_hourly", []);

        let process_snapshots_deleted = conn
            .execute("DELETE FROM process_snapshots", [])
            .map_err(|e| CoreError::Internal(format!("프로세스 스냅샷 삭제 실패: {e}")))?
            as u64;
        let idle_periods_deleted = conn
            .execute("DELETE FROM idle_periods", [])
            .map_err(|e| CoreError::Internal(format!("유휴 기록 삭제 실패: {e}")))?
            as u64;

        let _ = conn.execute("DELETE FROM session_stats", []);

        Ok(DeletedRangeCounts {
            events_deleted,
            frames_deleted,
            metrics_deleted,
            process_snapshots_deleted,
            idle_periods_deleted,
        })
    }

    pub fn list_event_exports(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<EventExportRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT event_id, event_type, timestamp, app_name, window_title
                 FROM events
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT timestamp, cpu_usage, memory_used, memory_total, disk_used, disk_total,
                        network_upload, network_download
                 FROM system_metrics
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, trigger_type, app_name, window_title, importance,
                        resolution_w, resolution_h, ocr_text
                 FROM frames
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp ASC",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let count: i64 = match pattern {
            Some(p) => conn
                .query_row(count_sql, rusqlite::params![p], |row| row.get(0))
                .map_err(|e| CoreError::Internal(format!("프레임 검색 개수 조회 실패: {e}")))?,
            None => conn
                .query_row(count_sql, [], |row| row.get(0))
                .map_err(|e| CoreError::Internal(format!("프레임 검색 개수 조회 실패: {e}")))?,
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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(select_sql)
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

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
                .map_err(|e| CoreError::Internal(format!("프레임 검색 실패: {e}")))?;

            let mut records = Vec::new();
            for row in rows {
                records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
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
                .map_err(|e| CoreError::Internal(format!("프레임 검색 실패: {e}")))?;

            let mut records = Vec::new();
            for row in rows {
                records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
            }
            Ok(records)
        }
    }

    pub fn count_search_events(&self, pattern: &str) -> Result<u64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events
                 WHERE app_name LIKE ?1 OR window_title LIKE ?1 OR data LIKE ?1",
                rusqlite::params![pattern],
                |row| row.get(0),
            )
            .map_err(|e| CoreError::Internal(format!("이벤트 검색 개수 조회 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT event_id, timestamp, app_name, window_title, data
                 FROM events
                 WHERE app_name LIKE ?1 OR window_title LIKE ?1 OR data LIKE ?1
                 ORDER BY timestamp DESC
                 LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("이벤트 검색 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
        }
        Ok(records)
    }
}
