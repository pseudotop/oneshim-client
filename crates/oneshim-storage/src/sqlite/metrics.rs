//! 메트릭 스토리지 (MetricsStorage 포트 구현).
//!
//! 시스템 메트릭, 프로세스 스냅샷, 유휴 기간, 세션 통계.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Timelike, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::activity::{
    IdlePeriod, ProcessSnapshot, ProcessSnapshotEntry, SessionStats,
};
use oneshim_core::models::system::{NetworkInfo, SystemMetrics};
use oneshim_core::ports::storage::MetricsStorage;
use tracing::{debug, info};

use super::SqliteStorage;

#[async_trait]
impl MetricsStorage for SqliteStorage {
    // --------------------------------------------------------
    // 시스템 메트릭
    // --------------------------------------------------------

    async fn save_metrics(&self, metrics: &SystemMetrics) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let (upload, download) = metrics
            .network
            .as_ref()
            .map(|n| (n.upload_speed as i64, n.download_speed as i64))
            .unwrap_or((0, 0));

        conn.execute(
            "INSERT INTO system_metrics (timestamp, cpu_usage, memory_used, memory_total, disk_used, disk_total, network_upload, network_download)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                metrics.timestamp.to_rfc3339(),
                metrics.cpu_usage,
                metrics.memory_used as i64,
                metrics.memory_total as i64,
                metrics.disk_used as i64,
                metrics.disk_total as i64,
                upload,
                download,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("시스템 메트릭 저장 실패: {e}")))?;

        debug!(
            "시스템 메트릭 저장: CPU {:.1}%, 메모리 {}MB",
            metrics.cpu_usage,
            metrics.memory_used / 1_048_576
        );
        Ok(())
    }

    async fn get_metrics(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<SystemMetrics>, CoreError> {
        let from_str = from.to_rfc3339();
        let to_str = to.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT timestamp, cpu_usage, memory_used, memory_total, disk_used, disk_total, network_upload, network_download
                 FROM system_metrics
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp DESC
                 LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let metrics = stmt
            .query_map(rusqlite::params![from_str, to_str, limit as i64], |row| {
                let ts_str: String = row.get(0)?;
                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                let upload: i64 = row.get(6)?;
                let download: i64 = row.get(7)?;

                Ok(SystemMetrics {
                    timestamp,
                    cpu_usage: row.get(1)?,
                    memory_used: row.get::<_, i64>(2)? as u64,
                    memory_total: row.get::<_, i64>(3)? as u64,
                    disk_used: row.get::<_, i64>(4)? as u64,
                    disk_total: row.get::<_, i64>(5)? as u64,
                    network: Some(NetworkInfo {
                        upload_speed: upload as u64,
                        download_speed: download as u64,
                        is_connected: upload > 0 || download > 0,
                    }),
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(metrics)
    }

    async fn aggregate_hourly_metrics(&self, hour: DateTime<Utc>) -> Result<(), CoreError> {
        // 해당 시간의 시작과 끝 계산
        let hour_start = hour
            .with_minute(0)
            .and_then(|dt| dt.with_second(0))
            .and_then(|dt| dt.with_nanosecond(0))
            .unwrap_or(hour);
        let hour_end = hour_start + Duration::hours(1);

        let hour_str = hour_start.format("%Y-%m-%dT%H:00:00Z").to_string();
        let from_str = hour_start.to_rfc3339();
        let to_str = hour_end.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        // 해당 시간의 메트릭 집계
        let result: Result<(f64, f64, i64, i64, i64), rusqlite::Error> = conn.query_row(
            "SELECT AVG(cpu_usage), MAX(cpu_usage), AVG(memory_used), MAX(memory_used), COUNT(*)
             FROM system_metrics
             WHERE timestamp >= ?1 AND timestamp < ?2",
            rusqlite::params![from_str, to_str],
            |row| {
                Ok((
                    row.get::<_, Option<f64>>(0)?.unwrap_or(0.0),
                    row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    row.get(4)?,
                ))
            },
        );

        match result {
            Ok((cpu_avg, cpu_max, memory_avg, memory_max, count)) if count > 0 => {
                conn.execute(
                    "INSERT OR REPLACE INTO system_metrics_hourly (hour, cpu_avg, cpu_max, memory_avg, memory_max, sample_count)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![hour_str, cpu_avg, cpu_max, memory_avg, memory_max, count],
                )
                .map_err(|e| CoreError::Internal(format!("시간별 집계 저장 실패: {e}")))?;
                debug!("시간별 메트릭 집계: {} ({count}개 샘플)", hour_str);
            }
            _ => {
                debug!("시간별 메트릭 집계: {} (데이터 없음)", hour_str);
            }
        }

        Ok(())
    }

    async fn cleanup_old_metrics(&self, before: DateTime<Utc>) -> Result<usize, CoreError> {
        let cutoff = before.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let deleted = conn
            .execute(
                "DELETE FROM system_metrics WHERE timestamp < ?1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| CoreError::Internal(format!("오래된 메트릭 삭제 실패: {e}")))?;

        if deleted > 0 {
            info!("오래된 메트릭 {deleted}개 삭제");
        }
        Ok(deleted)
    }

    // --------------------------------------------------------
    // 프로세스 스냅샷
    // --------------------------------------------------------

    async fn save_process_snapshot(&self, snapshot: &ProcessSnapshot) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let data = serde_json::to_string(&snapshot.processes)?;

        conn.execute(
            "INSERT INTO process_snapshots (timestamp, snapshot_data) VALUES (?1, ?2)",
            rusqlite::params![snapshot.timestamp.to_rfc3339(), data],
        )
        .map_err(|e| CoreError::Internal(format!("프로세스 스냅샷 저장 실패: {e}")))?;

        debug!(
            "프로세스 스냅샷 저장: {}개 프로세스",
            snapshot.processes.len()
        );
        Ok(())
    }

    async fn get_process_snapshots(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<ProcessSnapshot>, CoreError> {
        let from_str = from.to_rfc3339();
        let to_str = to.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT timestamp, snapshot_data FROM process_snapshots
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp DESC
                 LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let snapshots = stmt
            .query_map(rusqlite::params![from_str, to_str, limit as i64], |row| {
                let ts_str: String = row.get(0)?;
                let data: String = row.get(1)?;

                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let processes: Vec<ProcessSnapshotEntry> =
                    serde_json::from_str(&data).unwrap_or_default();

                Ok(ProcessSnapshot {
                    timestamp,
                    processes,
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(snapshots)
    }

    async fn cleanup_old_process_snapshots(
        &self,
        before: DateTime<Utc>,
    ) -> Result<usize, CoreError> {
        let cutoff = before.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let deleted = conn
            .execute(
                "DELETE FROM process_snapshots WHERE timestamp < ?1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| CoreError::Internal(format!("오래된 스냅샷 삭제 실패: {e}")))?;

        if deleted > 0 {
            info!("오래된 프로세스 스냅샷 {deleted}개 삭제");
        }
        Ok(deleted)
    }

    // --------------------------------------------------------
    // 유휴 기간
    // --------------------------------------------------------

    async fn start_idle_period(&self, start_time: DateTime<Utc>) -> Result<i64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT INTO idle_periods (start_time) VALUES (?1)",
            rusqlite::params![start_time.to_rfc3339()],
        )
        .map_err(|e| CoreError::Internal(format!("유휴 기간 시작 기록 실패: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!("유휴 기간 시작: id={}", id);
        Ok(id)
    }

    /// 유휴 기간 종료
    ///
    /// RETURNING clause로 SELECT+UPDATE를 1개 쿼리로 최적화 (N+1 제거)
    async fn end_idle_period(&self, id: i64, end_time: DateTime<Utc>) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let end_time_str = end_time.to_rfc3339();

        // RETURNING clause로 duration_secs를 한 번에 계산 + 반환
        let duration_secs: i64 = conn
            .query_row(
                "UPDATE idle_periods
                 SET end_time = ?1,
                     duration_secs = CAST((julianday(?1) - julianday(start_time)) * 86400 AS INTEGER)
                 WHERE id = ?2
                 RETURNING duration_secs",
                rusqlite::params![end_time_str, id],
                |row| row.get(0),
            )
            .map_err(|e| CoreError::Internal(format!("유휴 기간 종료 기록 실패: {e}")))?;

        debug!("유휴 기간 종료: id={}, 지속={}초", id, duration_secs);
        Ok(())
    }

    async fn get_ongoing_idle_period(&self) -> Result<Option<(i64, IdlePeriod)>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let result: Result<(i64, String), rusqlite::Error> = conn.query_row(
            "SELECT id, start_time FROM idle_periods WHERE end_time IS NULL ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        match result {
            Ok((id, start_str)) => {
                let start_time = DateTime::parse_from_rfc3339(&start_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(Some((
                    id,
                    IdlePeriod {
                        start_time,
                        end_time: None,
                        duration_secs: None,
                    },
                )))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Internal(format!(
                "진행 중 유휴 기간 조회 실패: {e}"
            ))),
        }
    }

    async fn get_idle_periods(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<IdlePeriod>, CoreError> {
        let from_str = from.to_rfc3339();
        let to_str = to.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT start_time, end_time, duration_secs FROM idle_periods
                 WHERE start_time >= ?1 AND start_time <= ?2
                 ORDER BY start_time DESC",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let periods = stmt
            .query_map(rusqlite::params![from_str, to_str], |row| {
                let start_str: String = row.get(0)?;
                let end_str: Option<String> = row.get(1)?;
                let duration: Option<i64> = row.get(2)?;

                let start_time = DateTime::parse_from_rfc3339(&start_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let end_time = end_str.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&Utc))
                        .ok()
                });

                Ok(IdlePeriod {
                    start_time,
                    end_time,
                    duration_secs: duration.map(|d| d as u64),
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(periods)
    }

    async fn cleanup_old_idle_periods(&self, before: DateTime<Utc>) -> Result<usize, CoreError> {
        let cutoff = before.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let deleted = conn
            .execute(
                "DELETE FROM idle_periods WHERE start_time < ?1 AND end_time IS NOT NULL",
                rusqlite::params![cutoff],
            )
            .map_err(|e| CoreError::Internal(format!("오래된 유휴 기간 삭제 실패: {e}")))?;

        if deleted > 0 {
            info!("오래된 유휴 기간 {deleted}개 삭제");
        }
        Ok(deleted)
    }

    // --------------------------------------------------------
    // 세션 통계
    // --------------------------------------------------------

    async fn upsert_session(&self, stats: &SessionStats) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT INTO session_stats (session_id, started_at, ended_at, total_events, total_frames, total_idle_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id) DO UPDATE SET
                ended_at = excluded.ended_at,
                total_events = excluded.total_events,
                total_frames = excluded.total_frames,
                total_idle_secs = excluded.total_idle_secs",
            rusqlite::params![
                stats.session_id,
                stats.started_at.to_rfc3339(),
                stats.ended_at.map(|dt| dt.to_rfc3339()),
                stats.total_events as i64,
                stats.total_frames as i64,
                stats.total_idle_secs as i64,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("세션 통계 저장 실패: {e}")))?;

        debug!("세션 통계 저장: {}", stats.session_id);
        Ok(())
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<SessionStats>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let result: Result<(String, Option<String>, i64, i64, i64), rusqlite::Error> = conn
            .query_row(
                "SELECT started_at, ended_at, total_events, total_frames, total_idle_secs
                 FROM session_stats WHERE session_id = ?1",
                rusqlite::params![session_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            );

        match result {
            Ok((started_str, ended_str, events, frames, idle)) => {
                let started_at = DateTime::parse_from_rfc3339(&started_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let ended_at = ended_str.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&Utc))
                        .ok()
                });

                Ok(Some(SessionStats {
                    session_id: session_id.to_string(),
                    started_at,
                    ended_at,
                    total_events: events as u64,
                    total_frames: frames as u64,
                    total_idle_secs: idle as u64,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Internal(format!("세션 조회 실패: {e}"))),
        }
    }

    async fn end_session(
        &self,
        session_id: &str,
        ended_at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE session_stats SET ended_at = ?1 WHERE session_id = ?2",
            rusqlite::params![ended_at.to_rfc3339(), session_id],
        )
        .map_err(|e| CoreError::Internal(format!("세션 종료 기록 실패: {e}")))?;

        debug!("세션 종료: {}", session_id);
        Ok(())
    }

    async fn increment_session_counters(
        &self,
        session_id: &str,
        events: u64,
        frames: u64,
        idle_secs: u64,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE session_stats SET
                total_events = total_events + ?1,
                total_frames = total_frames + ?2,
                total_idle_secs = total_idle_secs + ?3
             WHERE session_id = ?4",
            rusqlite::params![events as i64, frames as i64, idle_secs as i64, session_id],
        )
        .map_err(|e| CoreError::Internal(format!("세션 카운터 증가 실패: {e}")))?;

        Ok(())
    }
}
