mod calibration_store_impl;
mod coaching_storage;
mod device_identity;
pub(crate) mod edge_intelligence;
mod events;
mod focus_storage_impl;
mod frames;
mod fts_search_impl;
mod integration_query_impl;
mod lan_pin_store;
mod maintenance;
mod metrics;
mod override_store_impl;
mod tags;
pub mod vector_index_impl;
pub mod vector_store_impl;
mod web_storage_impl;

#[cfg(test)]
pub(crate) mod test_utils;
#[cfg(test)]
mod tests;

use oneshim_core::error::CoreError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::migration;

/// Local SQLite storage with a single-connection, Mutex-guarded design.
///
/// # Connection design
///
/// This store uses a single `Connection` behind a `Mutex` rather than a
/// connection pool. The rationale:
///
/// 1. **WAL mode** (`PRAGMA journal_mode=WAL`) allows concurrent readers
///    from the OS level, but rusqlite's `Connection` is not `Sync`, so we
///    still need a Mutex for Rust's thread-safety requirements.
/// 2. All blocking SQLite operations are offloaded to `spawn_blocking`,
///    which prevents the Mutex from starving the async runtime.
/// 3. A full read/write pool (e.g. r2d2 + separate read-only connections)
///    adds complexity without measurable benefit for our workload profile:
///    the scheduler ticks at 1-10 Hz and queries complete in <1ms.
///
/// If profiling reveals lock contention, the next step would be opening a
/// second read-only connection (`SQLITE_OPEN_READ_ONLY`) and routing
/// SELECT-only queries through it. The [`read_only_query`](Self::read_only_query)
/// helper already enforces the "acquire lock, clone data out, release lock"
/// pattern to minimise the critical section.
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

    /// Execute a read-only query with a short-lived lock scope.
    ///
    /// The closure `f` receives a `&Connection` and must clone/copy the
    /// data it needs into a fully-owned `T`. The Mutex is released as soon
    /// as `f` returns, before the `spawn_blocking` future completes, so
    /// writers are not blocked while the caller processes the result.
    ///
    /// This is the recommended pattern for pure SELECT queries that return
    /// small to medium result sets (e.g., config lookups, aggregate stats).
    /// For large result sets, consider streaming via `with_conn` with
    /// incremental fetching.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count: i64 = storage.read_only_query(|conn| {
    ///     conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
    ///         .map_err(|e| CoreError::Internal(e.to_string()))
    /// }).await?;
    /// ```
    pub async fn read_only_query<F, T>(&self, f: F) -> Result<T, CoreError>
    where
        F: FnOnce(&Connection) -> Result<T, CoreError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            // Acquire lock, execute query, release lock -- all within the
            // blocking thread. The result `T` is owned so the lock is not
            // held while the async runtime schedules the continuation.
            let guard = conn
                .lock()
                .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
            f(&guard)
            // guard drops here, releasing the Mutex
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
