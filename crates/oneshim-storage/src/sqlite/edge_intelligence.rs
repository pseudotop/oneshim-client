use chrono::{DateTime, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::work_session::{
    AppCategory, FocusMetrics, Interruption, LocalSuggestion, SessionState, WorkSession,
};
use tracing::debug;

use super::{
    FocusInterruptionRecord, FocusWorkSessionRecord, LocalSuggestionRecord, SqliteStorage,
};

impl SqliteStorage {
    // --------------------------------------------------------
    // --------------------------------------------------------

    pub fn start_work_session(
        &self,
        primary_app: &str,
        category: AppCategory,
    ) -> Result<WorkSession, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let now = Utc::now();
        let category_str = format!("{:?}", category);

        conn.execute(
            "INSERT INTO work_sessions (started_at, primary_app, category, state)
             VALUES (?1, ?2, ?3, 'active')",
            rusqlite::params![now.to_rfc3339(), primary_app, category_str],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to start work session: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!(
            "작업 session started: id={}, app={}, category={:?}",
            id, primary_app, category
        );

        Ok(WorkSession {
            id,
            started_at: now,
            ended_at: None,
            primary_app: primary_app.to_string(),
            category,
            state: SessionState::Active,
            interruption_count: 0,
            deep_work_secs: 0,
            duration_secs: 0,
        })
    }

    pub fn get_active_work_session(&self) -> Result<Option<WorkSession>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let result = conn.query_row(
            "SELECT id, started_at, primary_app, category, interruption_count, deep_work_secs, duration_secs
             FROM work_sessions WHERE state = 'active' ORDER BY id DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u64>(5)?,
                    row.get::<_, u64>(6)?,
                ))
            },
        );

        match result {
            Ok((
                id,
                started_str,
                primary_app,
                category_str,
                interruption_count,
                deep_work_secs,
                duration_secs,
            )) => {
                let started_at = DateTime::parse_from_rfc3339(&started_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let category = Self::parse_app_category(&category_str);

                Ok(Some(WorkSession {
                    id,
                    started_at,
                    ended_at: None,
                    primary_app,
                    category,
                    state: SessionState::Active,
                    interruption_count,
                    deep_work_secs,
                    duration_secs,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Internal(format!(
                "Failed to query work session: {e}"
            ))),
        }
    }

    pub fn end_work_session(&self, session_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let now = Utc::now();
        let now_str = now.to_rfc3339();

        let duration_secs: i64 = conn
            .query_row(
                "UPDATE work_sessions
                 SET ended_at = ?1,
                     state = 'completed',
                     duration_secs = CAST((julianday(?1) - julianday(started_at)) * 86400 AS INTEGER)
                 WHERE id = ?2
                 RETURNING duration_secs",
                rusqlite::params![now_str, session_id],
                |row| row.get(0),
            )
            .map_err(|e| CoreError::Internal(format!("Failed to end work session: {e}")))?;

        debug!(
            "작업 session ended: id={}, duration={}초",
            session_id, duration_secs
        );
        Ok(())
    }

    pub fn increment_work_session_interruption(&self, session_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE work_sessions SET interruption_count = interruption_count + 1 WHERE id = ?1",
            rusqlite::params![session_id],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to increment interruption count: {e}")))?;

        Ok(())
    }

    pub fn add_deep_work_secs(&self, session_id: i64, secs: u64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE work_sessions SET deep_work_secs = deep_work_secs + ?1 WHERE id = ?2",
            rusqlite::params![secs as i64, session_id],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to increment deep_work_secs: {e}")))?;

        Ok(())
    }

    // --------------------------------------------------------
    // --------------------------------------------------------

    pub fn get_app_durations_by_date(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, i64)>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT primary_app, SUM(duration_secs) as total_secs
                 FROM work_sessions
                 WHERE state = 'completed'
                   AND started_at >= ?1 AND started_at < ?2
                 GROUP BY primary_app
                 ORDER BY total_secs DESC",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare SQL: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| CoreError::Internal(format!("Query failed: {e}")))?;

        let result: Vec<_> = rows.flatten().collect();

        Ok(result)
    }

    pub fn get_daily_active_secs(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, i64)>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT DATE(started_at) as day, SUM(duration_secs) as total_secs
                 FROM work_sessions
                 WHERE state = 'completed'
                   AND started_at >= ?1 AND started_at < ?2
                 GROUP BY day
                 ORDER BY day",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare SQL: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| CoreError::Internal(format!("Query failed: {e}")))?;

        let result: Vec<_> = rows.flatten().collect();

        Ok(result)
    }

    // --------------------------------------------------------
    // --------------------------------------------------------

    pub fn record_interruption(&self, interruption: &Interruption) -> Result<i64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "INSERT INTO interruptions (interrupted_at, from_app, from_category, to_app, to_category, snapshot_frame_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                interruption.interrupted_at.to_rfc3339(),
                interruption.from_app,
                format!("{:?}", interruption.from_category),
                interruption.to_app,
                format!("{:?}", interruption.to_category),
                interruption.snapshot_frame_id,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to record interruption: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!(
            "인터럽션 record: {} → {}",
            interruption.from_app, interruption.to_app
        );
        Ok(id)
    }

    pub fn record_interruption_resume(
        &self,
        interruption_id: i64,
        resumed_to_app: &str,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE interruptions SET resumed_at = ?1, resumed_to_app = ?2 WHERE id = ?3",
            rusqlite::params![Utc::now().to_rfc3339(), resumed_to_app, interruption_id],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to record interruption resume: {e}")))?;

        Ok(())
    }

    pub fn get_pending_interruption(&self) -> Result<Option<Interruption>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let result = conn.query_row(
            "SELECT id, interrupted_at, from_app, from_category, to_app, to_category, snapshot_frame_id
             FROM interruptions WHERE resumed_at IS NULL ORDER BY id DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            },
        );

        match result {
            Ok((
                id,
                interrupted_at_str,
                from_app,
                from_category_str,
                to_app,
                to_category_str,
                snapshot_frame_id,
            )) => {
                let interrupted_at = DateTime::parse_from_rfc3339(&interrupted_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(Some(Interruption {
                    id,
                    interrupted_at,
                    from_app,
                    from_category: Self::parse_app_category(&from_category_str),
                    to_app,
                    to_category: Self::parse_app_category(&to_category_str),
                    snapshot_frame_id,
                    resumed_at: None,
                    resumed_to_app: None,
                    duration_secs: None,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Internal(format!(
                "Failed to query interruptions: {e}"
            ))),
        }
    }

    // --------------------------------------------------------
    // --------------------------------------------------------

    pub fn get_or_create_today_focus_metrics(&self) -> Result<FocusMetrics, CoreError> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        self.get_or_create_focus_metrics(&today)
    }

    pub fn get_or_create_focus_metrics(&self, date: &str) -> Result<FocusMetrics, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let result = conn.query_row(
            "SELECT total_active_secs, deep_work_secs, communication_secs, context_switches,
                    interruption_count, avg_focus_duration_secs, max_focus_duration_secs, focus_score
             FROM focus_metrics WHERE date = ?1",
            rusqlite::params![date],
            |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u64>(5)?,
                    row.get::<_, u64>(6)?,
                    row.get::<_, f32>(7)?,
                ))
            },
        );

        let (period_start, period_end) = Self::date_to_period_range(date);

        match result {
            Ok((
                total_active_secs,
                deep_work_secs,
                communication_secs,
                context_switches,
                interruption_count,
                avg_focus_duration_secs,
                max_focus_duration_secs,
                focus_score,
            )) => Ok(FocusMetrics {
                period_start,
                period_end,
                total_active_secs,
                deep_work_secs,
                communication_secs,
                context_switches,
                interruption_count,
                avg_focus_duration_secs,
                max_focus_duration_secs,
                focus_score,
            }),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                conn.execute(
                    "INSERT INTO focus_metrics (date) VALUES (?1)",
                    rusqlite::params![date],
                )
                .map_err(|e| CoreError::Internal(format!("Failed to create focus metric: {e}")))?;

                Ok(FocusMetrics::new(period_start, period_end))
            }
            Err(e) => Err(CoreError::Internal(format!(
                "Failed to query focus metric: {e}"
            ))),
        }
    }

    pub fn update_focus_metrics(
        &self,
        date: &str,
        metrics: &FocusMetrics,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE focus_metrics SET
                total_active_secs = ?1,
                deep_work_secs = ?2,
                communication_secs = ?3,
                context_switches = ?4,
                interruption_count = ?5,
                avg_focus_duration_secs = ?6,
                max_focus_duration_secs = ?7,
                focus_score = ?8,
                updated_at = datetime('now')
             WHERE date = ?9",
            rusqlite::params![
                metrics.total_active_secs as i64,
                metrics.deep_work_secs as i64,
                metrics.communication_secs as i64,
                metrics.context_switches as i64,
                metrics.interruption_count as i64,
                metrics.avg_focus_duration_secs as i64,
                metrics.max_focus_duration_secs as i64,
                metrics.focus_score,
                date,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to update focus metric: {e}")))?;

        debug!(
            "집중도 메트릭 update: date={}, score={:.2}",
            date, metrics.focus_score
        );
        Ok(())
    }

    pub fn increment_focus_metrics(
        &self,
        date: &str,
        total_active_secs: u64,
        deep_work_secs: u64,
        communication_secs: u64,
        context_switches: u32,
        interruption_count: u32,
    ) -> Result<(), CoreError> {
        let _ = self.get_or_create_focus_metrics(date)?;

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE focus_metrics SET
                total_active_secs = total_active_secs + ?1,
                deep_work_secs = deep_work_secs + ?2,
                communication_secs = communication_secs + ?3,
                context_switches = context_switches + ?4,
                interruption_count = interruption_count + ?5,
                updated_at = datetime('now')
             WHERE date = ?6",
            rusqlite::params![
                total_active_secs as i64,
                deep_work_secs as i64,
                communication_secs as i64,
                context_switches as i64,
                interruption_count as i64,
                date,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to increment focus metric: {e}")))?;

        Ok(())
    }

    pub fn get_recent_focus_metrics(
        &self,
        days: usize,
    ) -> Result<Vec<(String, FocusMetrics)>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT date, total_active_secs, deep_work_secs, communication_secs, context_switches,
                        interruption_count, avg_focus_duration_secs, max_focus_duration_secs, focus_score
                 FROM focus_metrics ORDER BY date DESC LIMIT ?1",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![days as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, u64>(6)?,
                    row.get::<_, u64>(7)?,
                    row.get::<_, f32>(8)?,
                ))
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            let (
                date,
                total_active_secs,
                deep_work_secs,
                communication_secs,
                context_switches,
                interruption_count,
                avg_focus_duration_secs,
                max_focus_duration_secs,
                focus_score,
            ) = row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?;

            let (period_start, period_end) = Self::date_to_period_range(&date);

            results.push((
                date,
                FocusMetrics {
                    period_start,
                    period_end,
                    total_active_secs,
                    deep_work_secs,
                    communication_secs,
                    context_switches,
                    interruption_count,
                    avg_focus_duration_secs,
                    max_focus_duration_secs,
                    focus_score,
                },
            ));
        }

        Ok(results)
    }

    pub fn list_work_sessions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<FocusWorkSessionRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, started_at, ended_at, primary_app, category, state,
                        interruption_count, deep_work_secs, duration_secs
                 FROM work_sessions
                 WHERE started_at >= ?1 AND started_at <= ?2
                 ORDER BY started_at DESC
                 LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to, limit as i64], |row| {
                Ok(FocusWorkSessionRecord {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    ended_at: row.get(2)?,
                    primary_app: row.get(3)?,
                    category: row.get(4)?,
                    state: row.get(5)?,
                    interruption_count: row.get(6)?,
                    deep_work_secs: row.get(7)?,
                    duration_secs: row.get(8)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_interruptions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<FocusInterruptionRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, interrupted_at, from_app, from_category, to_app, to_category,
                        resumed_at, resumed_to_app,
                        CASE WHEN resumed_at IS NOT NULL
                             THEN CAST((julianday(resumed_at) - julianday(interrupted_at)) * 86400 AS INTEGER)
                             ELSE NULL END as duration_secs
                 FROM interruptions
                 WHERE interrupted_at >= ?1 AND interrupted_at <= ?2
                 ORDER BY interrupted_at DESC
                 LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to, limit as i64], |row| {
                Ok(FocusInterruptionRecord {
                    id: row.get(0)?,
                    interrupted_at: row.get(1)?,
                    from_app: row.get(2)?,
                    from_category: row.get(3)?,
                    to_app: row.get(4)?,
                    to_category: row.get(5)?,
                    resumed_at: row.get(6)?,
                    resumed_to_app: row.get(7)?,
                    duration_secs: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_recent_local_suggestions(
        &self,
        cutoff: &str,
        limit: usize,
    ) -> Result<Vec<LocalSuggestionRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, suggestion_type, payload, created_at, shown_at, dismissed_at, acted_at
                 FROM local_suggestions
                 WHERE created_at >= ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![cutoff, limit as i64], |row| {
                let payload_str: String = row.get(2)?;
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_str).unwrap_or(serde_json::json!({}));

                Ok(LocalSuggestionRecord {
                    id: row.get(0)?,
                    suggestion_type: row.get(1)?,
                    payload,
                    created_at: row.get(3)?,
                    shown_at: row.get(4)?,
                    dismissed_at: row.get(5)?,
                    acted_at: row.get(6)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    // --------------------------------------------------------
    // --------------------------------------------------------

    pub fn save_local_suggestion(&self, suggestion: &LocalSuggestion) -> Result<i64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let (suggestion_type, payload) = Self::serialize_suggestion(suggestion);

        conn.execute(
            "INSERT INTO local_suggestions (suggestion_type, payload) VALUES (?1, ?2)",
            rusqlite::params![suggestion_type, payload],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to save local suggestion: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!("suggestion save: id={}, type={}", id, suggestion_type);
        Ok(id)
    }

    pub fn mark_suggestion_shown(&self, suggestion_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE local_suggestions SET shown_at = datetime('now') WHERE id = ?1",
            rusqlite::params![suggestion_id],
        )
        .map_err(|e| CoreError::Internal(format!("suggestion display record failure: {e}")))?;

        Ok(())
    }

    pub fn mark_suggestion_dismissed(&self, suggestion_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE local_suggestions SET dismissed_at = datetime('now') WHERE id = ?1",
            rusqlite::params![suggestion_id],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to record suggestion dismissal: {e}")))?;

        Ok(())
    }

    pub fn mark_suggestion_acted(&self, suggestion_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE local_suggestions SET acted_at = datetime('now') WHERE id = ?1",
            rusqlite::params![suggestion_id],
        )
        .map_err(|e| CoreError::Internal(format!("suggestion execution record failure: {e}")))?;

        Ok(())
    }

    // --------------------------------------------------------
    // --------------------------------------------------------

    pub(super) fn date_to_period_range(date: &str) -> (DateTime<Utc>, DateTime<Utc>) {
        use chrono::NaiveDate;

        if let Ok(naive_date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            let start = naive_date
                .and_hms_opt(0, 0, 0)
                .map(|dt| dt.and_utc())
                .unwrap_or_else(Utc::now);
            let end = naive_date
                .and_hms_opt(23, 59, 59)
                .map(|dt| dt.and_utc())
                .unwrap_or_else(Utc::now);
            (start, end)
        } else {
            let now = Utc::now();
            let start = now
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|dt| dt.and_utc())
                .unwrap_or(now);
            let end = now
                .date_naive()
                .and_hms_opt(23, 59, 59)
                .map(|dt| dt.and_utc())
                .unwrap_or(now);
            (start, end)
        }
    }

    pub(crate) fn parse_app_category(s: &str) -> AppCategory {
        match s {
            "Communication" => AppCategory::Communication,
            "Development" => AppCategory::Development,
            "Documentation" => AppCategory::Documentation,
            "Browser" => AppCategory::Browser,
            "Design" => AppCategory::Design,
            "Media" => AppCategory::Media,
            "System" => AppCategory::System,
            _ => AppCategory::Other,
        }
    }

    fn serialize_suggestion(suggestion: &LocalSuggestion) -> (String, String) {
        match suggestion {
            LocalSuggestion::NeedFocusTime {
                communication_ratio,
                suggested_focus_mins,
            } => (
                "NeedFocusTime".to_string(),
                serde_json::json!({
                    "communication_ratio": communication_ratio,
                    "suggested_focus_mins": suggested_focus_mins,
                })
                .to_string(),
            ),
            LocalSuggestion::TakeBreak {
                continuous_work_mins,
            } => (
                "TakeBreak".to_string(),
                serde_json::json!({
                    "continuous_work_mins": continuous_work_mins,
                })
                .to_string(),
            ),
            LocalSuggestion::RestoreContext {
                interrupted_app,
                interrupted_at,
                snapshot_frame_id,
            } => (
                "RestoreContext".to_string(),
                serde_json::json!({
                    "interrupted_app": interrupted_app,
                    "interrupted_at": interrupted_at.to_rfc3339(),
                    "snapshot_frame_id": snapshot_frame_id,
                })
                .to_string(),
            ),
            LocalSuggestion::PatternDetected {
                pattern_description,
                confidence,
            } => (
                "PatternDetected".to_string(),
                serde_json::json!({
                    "pattern_description": pattern_description,
                    "confidence": confidence,
                })
                .to_string(),
            ),
            LocalSuggestion::ExcessiveCommunication {
                today_communication_mins,
                avg_communication_mins,
            } => (
                "ExcessiveCommunication".to_string(),
                serde_json::json!({
                    "today_communication_mins": today_communication_mins,
                    "avg_communication_mins": avg_communication_mins,
                })
                .to_string(),
            ),
        }
    }
}
