use chrono::{DateTime, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::SuggestionSource;
#[allow(deprecated)]
use oneshim_core::models::work_session::{
    AppCategory, FocusMetrics, Interruption, LocalSuggestion, SessionState, WorkSession,
};
use tracing::debug;

use super::{
    FocusInterruptionRecord, FocusWorkSessionRecord, LocalSuggestionRecord, SqliteStorage,
};

/// Serialize an enum to its SQL string representation using serde.
/// Produces consistent casing (e.g. "FocusReminder") instead of Debug
/// format which may differ between enum variants.
pub(crate) fn enum_to_sql_str<T: serde::Serialize>(val: &T) -> String {
    serde_json::to_string(val)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

/// Map a `local_suggestions` row to a `LocalSuggestionRecord`.
/// Shared by `list_recent_local_suggestions`, `list_local_suggestions_after_id`,
/// and `integration_query_impl`.
pub(crate) fn map_local_suggestion_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<LocalSuggestionRecord> {
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
}

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
        // TODO: migrate to enum_to_sql_str when AppCategory/SessionState use serde derives consistently
        let category_str = format!("{:?}", category);

        conn.execute(
            "INSERT INTO work_sessions (started_at, primary_app, category, state)
             VALUES (?1, ?2, ?3, 'active')",
            rusqlite::params![now.to_rfc3339(), primary_app, category_str],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to start work session: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!(
            "work session started: id={}, app={}, category={:?}",
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
            "work session ended: id={}, duration={}s",
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
                format!("{:?}", interruption.from_category), // TODO: migrate to enum_to_sql_str
                interruption.to_app,
                format!("{:?}", interruption.to_category), // TODO: migrate to enum_to_sql_str
                interruption.snapshot_frame_id,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to record interruption: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!(
            "interruption recorded: {} -> {}",
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
            "focus metrics updated: date={}, score={:.2}",
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
            .query_map(
                rusqlite::params![cutoff, limit as i64],
                map_local_suggestion_row,
            )
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_local_suggestions_after_id(
        &self,
        after_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<LocalSuggestionRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let sql = if after_id.is_some() {
            "SELECT id, suggestion_type, payload, created_at, shown_at, dismissed_at, acted_at
             FROM local_suggestions
             WHERE id > ?1
             ORDER BY id ASC
             LIMIT ?2"
        } else {
            "SELECT id, suggestion_type, payload, created_at, shown_at, dismissed_at, acted_at
             FROM local_suggestions
             ORDER BY id ASC
             LIMIT ?1"
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let rows = if let Some(after_id) = after_id {
            stmt.query_map(
                rusqlite::params![after_id, limit as i64],
                map_local_suggestion_row,
            )
        } else {
            stmt.query_map(rusqlite::params![limit as i64], map_local_suggestion_row)
        }
        .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    // --------------------------------------------------------
    // Unified suggestion persistence (sync version for FocusStorage trait)
    // --------------------------------------------------------

    /// Synchronously save a unified `Suggestion` to the V8 `suggestions` table.
    /// Returns the `suggestion_id` (UUID string).
    pub fn save_rule_suggestion_sync(
        &self,
        suggestion: &oneshim_core::models::suggestion::Suggestion,
    ) -> Result<String, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "INSERT OR REPLACE INTO suggestions \
             (suggestion_id, suggestion_type, source, content, priority, \
              confidence_score, relevance_score, is_actionable, reasoning, \
              created_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                suggestion.suggestion_id,
                enum_to_sql_str(&suggestion.suggestion_type),
                enum_to_sql_str(&suggestion.source),
                suggestion.content,
                enum_to_sql_str(&suggestion.priority),
                suggestion.confidence_score,
                suggestion.relevance_score,
                suggestion.is_actionable as i32,
                suggestion.reasoning,
                suggestion.created_at.to_rfc3339(),
                suggestion.expires_at.map(|t| t.to_rfc3339()),
            ],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to save suggestion: {e}")))?;

        debug!(id = %suggestion.suggestion_id, "rule-based suggestion persisted to SQLite");
        Ok(suggestion.suggestion_id.clone())
    }

    /// Mark a unified suggestion as shown by its string suggestion_id.
    pub fn mark_unified_suggestion_shown(&self, suggestion_id: &str) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "UPDATE suggestions SET shown_at = datetime('now') WHERE suggestion_id = ?1",
            rusqlite::params![suggestion_id],
        )
        .map_err(|e| CoreError::Internal(format!("suggestion shown record failure: {e}")))?;

        Ok(())
    }

    // --------------------------------------------------------
    // Legacy local_suggestions persistence (deprecated — kept for migration)
    // --------------------------------------------------------

    #[allow(deprecated)]
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
    // Unified V8 suggestions queries
    // --------------------------------------------------------

    /// List non-dismissed suggestions from the unified `suggestions` table,
    /// newest first, up to `limit` rows.
    pub fn list_suggestions(
        &self,
        limit: usize,
    ) -> Result<Vec<oneshim_core::models::storage_records::SuggestionRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, suggestion_id, suggestion_type, source, content, priority, \
                 confidence_score, relevance_score, is_actionable, reasoning, \
                 shown_at, dismissed_at, acted_at, created_at, expires_at \
                 FROM suggestions \
                 WHERE dismissed_at IS NULL \
                 ORDER BY created_at DESC \
                 LIMIT ?1",
            )
            .map_err(|e| CoreError::Internal(format!("prepare failure: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok(oneshim_core::models::storage_records::SuggestionRecord {
                    id: row.get(0)?,
                    suggestion_id: row.get(1)?,
                    suggestion_type: row.get(2)?,
                    source: row.get(3)?,
                    content: row.get(4)?,
                    priority: row.get(5)?,
                    confidence_score: row.get(6)?,
                    relevance_score: row.get(7)?,
                    is_actionable: row.get::<_, i32>(8)? != 0,
                    reasoning: row.get(9)?,
                    shown_at: row.get(10)?,
                    dismissed_at: row.get(11)?,
                    acted_at: row.get(12)?,
                    created_at: row.get(13)?,
                    expires_at: row.get(14)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("query failure: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?);
        }
        Ok(records)
    }

    /// Dismiss a unified suggestion by its string `suggestion_id`.
    /// Returns `true` if a row was updated, `false` otherwise.
    pub fn dismiss_unified_suggestion(&self, suggestion_id: &str) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let changed = conn
            .execute(
                "UPDATE suggestions SET dismissed_at = datetime('now') WHERE suggestion_id = ?1 AND dismissed_at IS NULL",
                rusqlite::params![suggestion_id],
            )
            .map_err(|e| CoreError::Internal(format!("dismiss failure: {e}")))?;

        Ok(changed > 0)
    }

    /// List closed segments whose time range falls within [from, to].
    /// Returns deserialized `SegmentSummary` structs from the `activity_segments` table.
    pub fn list_segments_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<oneshim_core::models::tiered_memory::SegmentSummary>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        // Check table existence (may not have run V9 migration yet)
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='activity_segments'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !table_exists {
            return Ok(Vec::new());
        }

        let from_str = from.to_rfc3339();
        let to_str = to.to_rfc3339();

        let mut stmt = conn
            .prepare(
                "SELECT id, start_time, end_time, duration_secs, regime_id, trigger_reason, \
                 event_count, app_breakdown, category_breakdown, context_switch_count, \
                 dominant_category, avg_importance, patterns_json, content_activities_json, \
                 container_json, llm_summary \
                 FROM activity_segments \
                 WHERE start_time >= ?1 AND end_time <= ?2 \
                 ORDER BY start_time",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare segments query: {e}")))?;

        let segments: Vec<oneshim_core::models::tiered_memory::SegmentSummary> = stmt
            .query_map(rusqlite::params![from_str, to_str], |row| {
                let id: String = row.get(0)?;
                let start_str: String = row.get(1)?;
                let end_str: String = row.get(2)?;
                let dur: i64 = row.get(3)?;
                let regime: Option<String> = row.get(4)?;
                let reason_str: String = row.get(5)?;
                let events: i64 = row.get(6)?;
                let app_json: String = row.get(7)?;
                let cat_json: String = row.get(8)?;
                let switches: i64 = row.get(9)?;
                let dominant: String = row.get(10)?;
                let importance: f64 = row.get(11)?;
                let patterns_json: String = row.get(12)?;
                let content_json: String = row.get(13)?;
                let container_json: Option<String> = row.get(14)?;
                let llm_summary: Option<String> = row.get(15)?;
                Ok((
                    id,
                    start_str,
                    end_str,
                    dur,
                    regime,
                    reason_str,
                    events,
                    app_json,
                    cat_json,
                    switches,
                    dominant,
                    importance,
                    patterns_json,
                    content_json,
                    container_json,
                    llm_summary,
                ))
            })
            .map_err(|e| CoreError::Internal(format!("Failed to query segments: {e}")))?
            .filter_map(|r| r.ok())
            .filter_map(
                |(
                    id,
                    start_str,
                    end_str,
                    dur,
                    regime,
                    reason_str,
                    events,
                    app_json,
                    cat_json,
                    switches,
                    dominant,
                    importance,
                    patterns_json,
                    content_json,
                    container_json,
                    llm_summary,
                )| {
                    let start_time = chrono::DateTime::parse_from_rfc3339(&start_str)
                        .ok()?
                        .with_timezone(&chrono::Utc);
                    let end_time = chrono::DateTime::parse_from_rfc3339(&end_str)
                        .ok()?
                        .with_timezone(&chrono::Utc);
                    let trigger_reason: oneshim_core::models::tiered_memory::TriggerReason =
                        serde_json::from_str(&format!("\"{reason_str}\"")).unwrap_or(
                            oneshim_core::models::tiered_memory::TriggerReason::ScoreHigh,
                        );
                    let app_breakdown = serde_json::from_str(&app_json).unwrap_or_default();
                    let category_breakdown = serde_json::from_str(&cat_json).unwrap_or_default();
                    let patterns_detected =
                        serde_json::from_str(&patterns_json).unwrap_or_default();
                    let content_activities =
                        serde_json::from_str(&content_json).unwrap_or_default();
                    let container = container_json.and_then(|j| serde_json::from_str(&j).ok());

                    Some(oneshim_core::models::tiered_memory::SegmentSummary {
                        segment_id: id,
                        start_time,
                        end_time,
                        duration_secs: dur as u64,
                        regime_id: regime,
                        trigger_reason,
                        event_count: events as u32,
                        app_breakdown,
                        category_breakdown,
                        context_switch_count: switches as u32,
                        dominant_category: dominant,
                        avg_importance: importance as f32,
                        patterns_detected,
                        content_activities,
                        container,
                        llm_summary,
                    })
                },
            )
            .collect();

        Ok(segments)
    }

    /// Check whether LLM_SERVER suggestions exist within the given lookback
    /// window. Used by the analysis loop to suppress local analysis when the
    /// server is actively sending suggestions.
    pub fn has_recent_server_suggestions(&self, lookback_secs: u64) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let sql = "SELECT COUNT(*) FROM suggestions \
             WHERE source = ?1 \
             AND created_at > datetime('now', ?2)";
        let count: i64 = conn
            .query_row(
                sql,
                rusqlite::params![
                    SuggestionSource::LLM_SERVER_STR,
                    format!("-{lookback_secs} seconds")
                ],
                |row| row.get(0),
            )
            .map_err(|e| CoreError::Internal(format!("query failure: {e}")))?;

        Ok(count > 0)
    }

    /// Delete activity segments older than `max_days`. Returns the number of deleted rows.
    pub fn enforce_segment_retention(&self, max_days: u32) -> Result<usize, CoreError> {
        let cutoff = (Utc::now() - chrono::Duration::days(max_days as i64)).to_rfc3339();
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='activity_segments'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !table_exists {
            return Ok(0);
        }
        let deleted = conn
            .execute(
                "DELETE FROM activity_segments WHERE start_time < ?1 AND start_time IS NOT NULL",
                rusqlite::params![cutoff],
            )
            .map_err(|e| CoreError::Internal(format!("segment retention failure: {e}")))?;
        tracing::debug!(
            "Enforced segment retention: deleted {deleted} rows older than {max_days} days"
        );
        Ok(deleted)
    }

    /// Delete weekly digests older than `max_weeks`. Returns the number of deleted rows.
    pub fn enforce_digest_retention(&self, max_weeks: u32) -> Result<usize, CoreError> {
        let cutoff = (Utc::now() - chrono::Duration::days(max_weeks as i64 * 7)).to_rfc3339();
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='weekly_digests'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !table_exists {
            return Ok(0);
        }
        let deleted = conn
            .execute(
                "DELETE FROM weekly_digests WHERE week_start < ?1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| CoreError::Internal(format!("digest retention failure: {e}")))?;
        tracing::debug!(
            "Enforced digest retention: deleted {deleted} rows older than {max_weeks} weeks"
        );
        Ok(deleted)
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

    #[allow(deprecated)]
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
