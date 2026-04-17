//! SqliteRegimeManagerStateStore — RegimeStoragePort over SQLite.

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::models::tiered_memory::Regime;
use oneshim_core::ports::regime_storage::RegimeStoragePort;
use rusqlite::{Connection, OptionalExtension};
use std::sync::Arc;

pub struct SqliteRegimeManagerStateStore {
    conn: Arc<parking_lot::Mutex<Connection>>,
}

impl SqliteRegimeManagerStateStore {
    pub fn new(conn: Arc<parking_lot::Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl RegimeStoragePort for SqliteRegimeManagerStateStore {
    async fn load_all(&self) -> Result<Vec<Regime>, CoreError> {
        let conn = self.conn.lock();
        let payload: Option<String> = conn
            .query_row(
                "SELECT payload FROM regime_manager_state WHERE id = 0",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| CoreError::Internal(e.to_string()))?;

        match payload {
            Some(json) => match serde_json::from_str::<Vec<Regime>>(&json) {
                Ok(regimes) => Ok(regimes),
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "regime_manager_state payload failed to parse; quarantining to payload_backup and starting fresh. Recover via manual inspection of the backup column."
                    );
                    let _ = conn.execute(
                        "UPDATE regime_manager_state
                            SET payload_backup = payload,
                                payload_backup_at = datetime('now'),
                                payload = '[]',
                                updated_at = datetime('now')
                          WHERE id = 0",
                        [],
                    );
                    Ok(Vec::new())
                }
            },
            None => Ok(Vec::new()),
        }
    }

    async fn save_all(&self, regimes: &[Regime]) -> Result<(), CoreError> {
        let json =
            serde_json::to_string(regimes).map_err(|e| CoreError::Internal(e.to_string()))?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO regime_manager_state
                (id, payload, payload_backup, payload_backup_at, updated_at)
             VALUES (
                0, ?1,
                (SELECT payload_backup FROM regime_manager_state WHERE id = 0),
                (SELECT payload_backup_at FROM regime_manager_state WHERE id = 0),
                datetime('now')
             )",
            rusqlite::params![json],
        )
        .map_err(|e| CoreError::Internal(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use oneshim_core::models::tiered_memory::{
        Regime, RegimeFeatures, RegimeStatus, TriggerParams,
    };
    use tempfile::TempDir;

    fn open_db() -> (TempDir, Arc<parking_lot::Mutex<Connection>>) {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("t.db")).unwrap();
        crate::migration::run_migrations(&conn).unwrap();
        (dir, Arc::new(parking_lot::Mutex::new(conn)))
    }

    fn sample_regime(id: &str) -> Regime {
        Regime {
            regime_id: id.into(),
            name: None,
            auto_label: format!("label-{id}"),
            centroid: RegimeFeatures::default(),
            optimal_params: TriggerParams::default(),
            sample_count: 0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            status: RegimeStatus::Active,
        }
    }

    /// T-C3c-1 — empty on first load.
    #[tokio::test]
    async fn empty_on_first_load() {
        let (_d, conn) = open_db();
        let store = SqliteRegimeManagerStateStore::new(conn);
        assert_eq!(store.load_all().await.unwrap().len(), 0);
    }

    /// T-C3c-2 — save then load roundtrip.
    #[tokio::test]
    async fn save_then_load_roundtrip() {
        let (_d, conn) = open_db();
        let store = SqliteRegimeManagerStateStore::new(conn);
        let regimes = vec![sample_regime("a"), sample_regime("b"), sample_regime("c")];
        store.save_all(&regimes).await.unwrap();
        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].regime_id, "a");
        assert_eq!(loaded[2].regime_id, "c");
    }

    /// T-C3c-3 — save replaces previous.
    #[tokio::test]
    async fn save_replaces_previous() {
        let (_d, conn) = open_db();
        let store = SqliteRegimeManagerStateStore::new(conn);
        store
            .save_all(&[sample_regime("a"), sample_regime("b"), sample_regime("c")])
            .await
            .unwrap();
        store.save_all(&[sample_regime("just_one")]).await.unwrap();
        let loaded = store.load_all().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].regime_id, "just_one");
    }

    /// T-C3c-4 — malformed payload quarantines, starts fresh.
    #[tokio::test]
    async fn malformed_payload_quarantines_and_starts_fresh() {
        let (_d, conn) = open_db();
        {
            let c = conn.lock();
            c.execute(
                "INSERT OR REPLACE INTO regime_manager_state (id, payload, updated_at) VALUES (0, '{not:valid json', datetime('now'))",
                [],
            )
            .unwrap();
        }
        let store = SqliteRegimeManagerStateStore::new(conn.clone());
        let result = store.load_all().await;
        assert!(result.is_ok(), "quarantine must not return Err");
        assert_eq!(result.unwrap().len(), 0, "fresh start expected");

        let c = conn.lock();
        let (backup, backup_at): (Option<String>, Option<String>) = c
            .query_row(
                "SELECT payload_backup, payload_backup_at FROM regime_manager_state WHERE id = 0",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(backup.unwrap(), "{not:valid json");
        assert!(backup_at.is_some(), "backup timestamp must be set");
    }
}
