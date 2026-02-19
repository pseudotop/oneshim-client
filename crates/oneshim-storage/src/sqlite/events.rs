//! 이벤트 스토리지 (StorageService 포트 구현).
//!
//! 이벤트 저장, 조회, 전송 마킹, 보존 정책 적용.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::event::Event;
use oneshim_core::ports::storage::StorageService;
use tracing::{debug, info};

use super::SqliteStorage;

impl SqliteStorage {
    /// 이벤트에서 ID 추출
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

    /// 이벤트 타입 추출
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

    /// 이벤트 타임스탬프 추출
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

    /// 여러 이벤트를 한 트랜잭션으로 배치 저장 (성능 최적화)
    ///
    /// 단일 save_event() 호출을 여러 번 하는 것보다 훨씬 빠름.
    /// 모든 이벤트가 성공하거나 모두 롤백됨.
    ///
    /// # Arguments
    /// * `events` - 저장할 이벤트 슬라이스
    ///
    /// # Returns
    /// 저장된 이벤트 수 (INSERT OR IGNORE로 중복 제외됨)
    pub fn save_events_batch(&self, events: &[Event]) -> Result<usize, CoreError> {
        if events.is_empty() {
            return Ok(0);
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let tx = conn
            .transaction()
            .map_err(|e| CoreError::Internal(format!("트랜잭션 시작 실패: {e}")))?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT OR IGNORE INTO events (event_id, event_type, timestamp, data) VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

            for event in events {
                let event_id = Self::extract_event_id(event);
                let event_type = Self::extract_event_type(event);
                let timestamp = Self::extract_timestamp(event).to_rfc3339();
                let data = serde_json::to_string(event)?;

                stmt.execute(rusqlite::params![event_id, event_type, timestamp, data])
                    .map_err(|e| CoreError::Internal(format!("배치 저장 실패: {e}")))?;
            }
        }

        tx.commit()
            .map_err(|e| CoreError::Internal(format!("트랜잭션 커밋 실패: {e}")))?;

        debug!("이벤트 배치 저장: {}개", events.len());
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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO events (event_id, event_type, timestamp, data) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![event_id, event_type, timestamp, data],
        )
        .map_err(|e| CoreError::Internal(format!("이벤트 저장 실패: {e}")))?;

        debug!("이벤트 저장: {event_id}");
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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT data FROM events WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp DESC LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let events = stmt
            .query_map(rusqlite::params![from_str, to_str, limit as i64], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?
            .filter_map(|r| r.ok())
            .filter_map(|data| serde_json::from_str::<Event>(&data).ok())
            .collect();

        Ok(events)
    }

    async fn get_pending_events(&self, limit: usize) -> Result<Vec<Event>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT data FROM events WHERE is_sent = 0 ORDER BY timestamp ASC LIMIT ?1")
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let events = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?
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
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("전송 완료 마킹 실패: {e}")))?;

        debug!("{}개 이벤트 전송 완료 마킹", event_ids.len());
        Ok(())
    }

    async fn enforce_retention(&self) -> Result<usize, CoreError> {
        let cutoff = (Utc::now() - Duration::days(self.retention_days as i64)).to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let deleted = conn
            .execute(
                "DELETE FROM events WHERE timestamp < ?1 AND is_sent = 1",
                rusqlite::params![cutoff],
            )
            .map_err(|e| CoreError::Internal(format!("보존 정책 적용 실패: {e}")))?;

        if deleted > 0 {
            info!(
                "보존 정책: {deleted}개 이벤트 삭제 (>{} 일)",
                self.retention_days
            );
        }
        Ok(deleted)
    }
}
