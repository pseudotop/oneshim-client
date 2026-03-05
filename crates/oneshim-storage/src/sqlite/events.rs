use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::event::Event;
use oneshim_core::ports::storage::StorageService;
use tracing::{debug, info};

use super::SqliteStorage;

impl SqliteStorage {
    pub fn count_events_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE timestamp >= ?1 AND timestamp <= ?2",
                rusqlite::params![from, to],
                |row| row.get(0),
            )
            .map_err(|e| CoreError::Internal(format!("Failed to count events: {e}")))?;

        Ok(count as u64)
    }

    pub(super) fn extract_event_id(event: &Event) -> String {
        match event {
            Event::User(user_event) => user_event.event_id.to_string(),
            Event::System(system_event) => system_event.event_id.to_string(),
            Event::Context(context_event) => {
                format!(
                    "ctx_{}_{}_{}",
                    context_event.timestamp.timestamp_millis(),
                    context_event.app_name,
                    context_event
                        .window_title
                        .chars()
                        .take(20)
                        .collect::<String>()
                )
            }
            Event::Input(input_event) => {
                format!(
                    "input_{}_{}",
                    input_event.timestamp.timestamp_millis(),
                    input_event.app_name
                )
            }
            Event::Process(process_event) => {
                format!("proc_{}", process_event.timestamp.timestamp_millis())
            }
            Event::Window(window_event) => {
                format!(
                    "win_{}_{:?}",
                    window_event.timestamp.timestamp_millis(),
                    window_event.event_type
                )
            }
        }
    }

    pub(super) fn extract_event_type(event: &Event) -> String {
        match event {
            Event::User(user_event) => format!("{:?}", user_event.event_type),
            Event::System(system_event) => format!("{:?}", system_event.event_type),
            Event::Context(_) => "context_change".to_string(),
            Event::Input(_) => "input_activity".to_string(),
            Event::Process(_) => "process_snapshot".to_string(),
            Event::Window(w) => format!("window_{:?}", w.event_type),
        }
    }

    pub(super) fn extract_timestamp(event: &Event) -> DateTime<Utc> {
        match event {
            Event::User(user_event) => user_event.timestamp,
            Event::System(system_event) => system_event.timestamp,
            Event::Context(context_event) => context_event.timestamp,
            Event::Input(input_event) => input_event.timestamp,
            Event::Process(process_event) => process_event.timestamp,
            Event::Window(window_event) => window_event.timestamp,
        }
    }

    /// # Arguments
    ///
    /// # Returns
    pub fn save_events_batch(&self, events: &[Event]) -> Result<usize, CoreError> {
        if events.is_empty() {
            return Ok(0);
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let tx = conn
            .transaction()
            .map_err(|e| CoreError::Internal(format!("Failed to start transaction: {e}")))?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT OR IGNORE INTO events (event_id, event_type, timestamp, data) VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

            for event in events {
                let event_id = Self::extract_event_id(event);
                let event_type = Self::extract_event_type(event);
                let timestamp = Self::extract_timestamp(event).to_rfc3339();
                let data = serde_json::to_string(event)?;

                stmt.execute(rusqlite::params![event_id, event_type, timestamp, data])
                    .map_err(|e| CoreError::Internal(format!("batch save failure: {e}")))?;
            }
        }

        tx.commit()
            .map_err(|e| CoreError::Internal(format!("Failed to commit transaction: {e}")))?;

        debug!("event batch save: {}items", events.len());
        Ok(events.len())
    }
}

#[async_trait]
impl StorageService for SqliteStorage {
    async fn save_event(&self, event: &Event) -> Result<(), CoreError> {
        let event_id = Self::extract_event_id(event);
        let event_type = Self::extract_event_type(event);
        let timestamp = Self::extract_timestamp(event).to_rfc3339();
        let data = serde_json::to_string(event)?;

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO events (event_id, event_type, timestamp, data) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![event_id, event_type, timestamp, data],
        )
        .map_err(|e| CoreError::Internal(format!("event save failure: {e}")))?;

        debug!("event save: {event_id}");
        Ok(())
    }

    async fn get_events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<Event>, CoreError> {
        let from_str = from.to_rfc3339();
        let to_str = to.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT data FROM events WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp DESC LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let events = stmt
            .query_map(rusqlite::params![from_str, to_str, limit as i64], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str::<Event>(&data).ok())
            .collect();

        Ok(events)
    }

    async fn get_pending_events(&self, limit: usize) -> Result<Vec<Event>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT data FROM events WHERE is_sent = 0 ORDER BY timestamp ASC LIMIT ?1")
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let events = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str::<Event>(&data).ok())
            .collect();

        Ok(events)
    }

    async fn mark_as_sent(&self, event_ids: &[String]) -> Result<(), CoreError> {
        if event_ids.is_empty() {
            return Ok(());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let placeholders: Vec<String> = event_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let sql = format!(
            "UPDATE events SET is_sent = 1 WHERE event_id IN ({})",
            placeholders.join(", ")
        );

        let params: Vec<&dyn rusqlite::types::ToSql> = event_ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();

        conn.execute(&sql, params.as_slice())
            .map_err(|e| CoreError::Internal(format!("Failed to mark as sent: {e}")))?;

        debug!("{}items event sent completed", event_ids.len());
        Ok(())
    }

    async fn enforce_retention(&self) -> Result<usize, CoreError> {
        let cutoff = (Utc::now() - Duration::days(self.retention_days as i64)).to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let deleted = conn
            .execute(
                "DELETE FROM events WHERE timestamp < ?1 AND is_sent = 1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| CoreError::Internal(format!("Failed to apply retention policy: {e}")))?;

        if deleted > 0 {
            info!(
                "retention policy: deleted {deleted} events (>{} days)",
                self.retention_days
            );
        }
        Ok(deleted)
    }
}
