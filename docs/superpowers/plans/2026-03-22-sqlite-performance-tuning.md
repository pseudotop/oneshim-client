# SQLite Performance Tuning — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve SQLite performance and correctness by extracting shared PRAGMA configuration, adding missing PRAGMAs, caching FTS5 availability, adding WAL checkpoint/VACUUM/ANALYZE/FTS-merge to the scheduler, migrating hot-path queries to `prepare_cached()`, and adding a Korean trigram FTS table (V18 migration).

**Architecture:** Changes span two crates: `oneshim-storage` (PRAGMA setup, FTS caching, `prepare_cached()`, V18 migration) and `src-tauri` (scheduler maintenance loops). All new storage methods are sync functions called from `with_conn` closures or directly from the scheduler via `spawn_blocking`. No new ports or traits are introduced.

**Tech Stack:** Rust, rusqlite (`prepare_cached`, `execute_batch`, `Connection`), `std::sync::atomic::AtomicBool`, chrono, tokio (`spawn_blocking`)

**Spec:** `docs/superpowers/specs/2026-03-21-sqlite-performance-tuning-design.md`

---

## File Map

### New files

| File | Content |
|------|---------|
| (none) | All changes are modifications to existing files |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-storage/src/sqlite/mod.rs` | Extract `configure_connection()`, add `journal_size_limit` PRAGMA, call `PRAGMA optimize` after migrations, fix `open_in_memory()` PRAGMA parity |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | Add `FTS_AVAILABLE` `AtomicBool` cache; replace per-query `sqlite_master` lookup with fast-path check |
| `crates/oneshim-storage/src/sqlite/events.rs` | Migrate `save_event()`, `get_events()`, `get_pending_events()` to `prepare_cached()` |
| `crates/oneshim-storage/src/sqlite/frames.rs` | Migrate `save_frame_metadata_with_bounds()`, `get_frames()` to `prepare_cached()` |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | Migrate FTS insert/query to `prepare_cached()` |
| `crates/oneshim-storage/src/migration/mod.rs` | Bump `CURRENT_VERSION` to 18, add V18 dispatch |
| `crates/oneshim-storage/src/migration/v09_v17.rs` | Rename to `v09_v18.rs`; add `migrate_v18()` for Korean trigram FTS table |
| `src-tauri/src/scheduler/config.rs` | Add `WAL_CHECKPOINT_INTERVAL_SECS`, `VACUUM_FREELIST_THRESHOLD_PERCENT` constants |
| `src-tauri/src/scheduler/loops/system.rs` | Add WAL checkpoint (5-min PASSIVE), conditional VACUUM on idle, FTS5 merge, ANALYZE after bulk ops |
| `crates/oneshim-storage/src/sqlite/tests.rs` | Add PRAGMA parity regression test |
| `crates/oneshim-storage/src/migration/tests.rs` | Add V18 migration test |

---

## Task 1: Extract `configure_connection()` + `open_in_memory()` PRAGMA parity

**Why:** `open_in_memory()` skips all PRAGMAs, creating a test/production divergence. Extract a shared helper so both paths configure the connection identically (excluding I/O-only PRAGMAs for in-memory).

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs`
- Test: `crates/oneshim-storage/src/sqlite/tests.rs`

- [ ] **Step 1.1: Write test for PRAGMA parity**

Add a test that verifies `open_in_memory()` applies PRAGMAs. Append to the bottom of `crates/oneshim-storage/src/sqlite/tests.rs`:

```rust
#[test]
fn open_in_memory_applies_shared_pragmas() {
    let storage = SqliteStorage::open_in_memory(30).unwrap();
    let conn = storage.conn.lock().unwrap();

    // PRAGMAs that must be applied to both disk and in-memory databases
    let cache_size: i64 = conn
        .query_row("PRAGMA cache_size", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cache_size, 8000, "cache_size must be 8000 pages");

    let temp_store: i64 = conn
        .query_row("PRAGMA temp_store", [], |row| row.get(0))
        .unwrap();
    assert_eq!(temp_store, 2, "temp_store must be MEMORY (2)");

    let synchronous: i64 = conn
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();
    assert_eq!(synchronous, 1, "synchronous must be NORMAL (1)");
}
```

```
cargo test -p oneshim-storage open_in_memory_applies_shared_pragmas
```

Verify the test **fails** (because `open_in_memory()` currently skips PRAGMAs).

- [ ] **Step 1.2: Extract `configure_connection()` helper**

In `crates/oneshim-storage/src/sqlite/mod.rs`, add a private function above `impl SqliteStorage`:

```rust
/// Apply shared PRAGMA settings to a connection.
///
/// Called by both `open()` (disk) and `open_in_memory()` (test).
/// PRAGMAs that only make sense for on-disk databases (`journal_mode`,
/// `mmap_size`, `journal_size_limit`, `page_size`) are gated by `is_disk`.
fn configure_connection(conn: &Connection, is_disk: bool) -> Result<(), CoreError> {
    // Shared PRAGMAs (apply to both disk and in-memory)
    conn.execute_batch(
        "
        PRAGMA synchronous=NORMAL;
        PRAGMA cache_size=8000;
        PRAGMA temp_store=MEMORY;
        ",
    )
    .map_err(|e| CoreError::Internal(format!("Failed to apply shared PRAGMA settings: {e}")))?;

    // Disk-only PRAGMAs
    if is_disk {
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA mmap_size=268435456;
            PRAGMA page_size=4096;
            PRAGMA journal_size_limit=67108864;
            ",
        )
        .map_err(|e| CoreError::Internal(format!("Failed to apply disk PRAGMA settings: {e}")))?;
    }

    Ok(())
}
```

- [ ] **Step 1.3: Refactor `open()` to use `configure_connection()`**

Replace the existing `conn.execute_batch(...)` block inside `open()` with:

```rust
configure_connection(&conn, true)?;
```

Remove the old inline PRAGMA block entirely.

- [ ] **Step 1.4: Refactor `open_in_memory()` to use `configure_connection()`**

Add `configure_connection(&conn, false)?;` to `open_in_memory()` right after the `Connection::open_in_memory()` call, before `migration::run_migrations(&conn)`:

```rust
pub fn open_in_memory(retention_days: u32) -> Result<Self, CoreError> {
    let conn = Connection::open_in_memory().map_err(|e| {
        CoreError::Internal(format!("Failed to create in-memory SQLite database: {e}"))
    })?;

    configure_connection(&conn, false)?;

    migration::run_migrations(&conn)
        .map_err(|e| CoreError::Internal(format!("migration failure: {e}")))?;

    Ok(Self {
        conn: Arc::new(Mutex::new(conn)),
        retention_days,
    })
}
```

- [ ] **Step 1.5: Verify test passes**

```
cargo test -p oneshim-storage open_in_memory_applies_shared_pragmas
```

```
cargo test -p oneshim-storage
```

Verify all existing tests still pass, confirming no regressions.

---

## Task 2: Add `journal_size_limit` + `PRAGMA optimize` after migrations

**Why:** WAL file grows unbounded without `journal_size_limit`. `PRAGMA optimize` ensures query planner statistics are fresh after schema migrations.

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs`
- Test: `crates/oneshim-storage/src/sqlite/tests.rs`

- [ ] **Step 2.1: Write test for `journal_size_limit`**

Append to `crates/oneshim-storage/src/sqlite/tests.rs`:

```rust
#[test]
fn open_disk_sets_journal_size_limit() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let storage = SqliteStorage::open(&db_path, 30).unwrap();
    let conn = storage.conn.lock().unwrap();

    let limit: i64 = conn
        .query_row("PRAGMA journal_size_limit", [], |row| row.get(0))
        .unwrap();
    assert_eq!(limit, 67108864, "journal_size_limit must be 64MB");
}
```

```
cargo test -p oneshim-storage open_disk_sets_journal_size_limit
```

Verify the test passes (the PRAGMA was already added in Step 1.2).

- [ ] **Step 2.2: Add `PRAGMA optimize` after migration in `open()`**

In `crates/oneshim-storage/src/sqlite/mod.rs`, inside `open()`, add after `migration::run_migrations(&conn)`:

```rust
// Refresh query planner statistics after schema changes.
// 0x10002 = analyze tables that need it + include schema changes.
conn.execute_batch("PRAGMA optimize = 0x10002;")
    .map_err(|e| CoreError::Internal(format!("Failed to run PRAGMA optimize: {e}")))?;
```

- [ ] **Step 2.3: Add `PRAGMA optimize` after migration in `open_in_memory()`**

Add the same `PRAGMA optimize` call after `migration::run_migrations(&conn)` in `open_in_memory()`:

```rust
conn.execute_batch("PRAGMA optimize = 0x10002;")
    .map_err(|e| CoreError::Internal(format!("Failed to run PRAGMA optimize: {e}")))?;
```

- [ ] **Step 2.4: Verify all tests pass**

```
cargo test -p oneshim-storage
```

---

## Task 3: FTS5 existence caching (`AtomicBool`)

**Why:** Each FTS query currently runs `SELECT COUNT(*) > 0 FROM sqlite_master WHERE name='search_fts'`. An `AtomicBool` eliminates one SQLite round-trip per FTS operation.

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/fts_search_impl.rs`
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs`
- Test: `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` (existing `#[cfg(test)]` module)

- [ ] **Step 3.1: Write test for FTS cache behavior**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/oneshim-storage/src/sqlite/fts_search_impl.rs`:

```rust
#[tokio::test]
async fn fts_available_flag_is_set_after_open() {
    // open_in_memory runs migrations which create search_fts, so the flag should be true
    let _storage = SqliteStorage::open_in_memory(30).unwrap();
    assert!(
        super::FTS_AVAILABLE.load(std::sync::atomic::Ordering::Relaxed),
        "FTS_AVAILABLE must be true after successful migration"
    );
}
```

```
cargo test -p oneshim-storage fts_available_flag_is_set_after_open
```

Verify the test **fails** (flag does not exist yet).

- [ ] **Step 3.2: Add `FTS_AVAILABLE` static `AtomicBool`**

At the top of `crates/oneshim-storage/src/sqlite/fts_search_impl.rs`, after the imports, add:

```rust
use std::sync::atomic::{AtomicBool, Ordering};

/// Cached flag: whether the `search_fts` FTS5 table exists.
/// Set once during `SqliteStorage::open()` / `open_in_memory()` after migrations.
/// Avoids a `sqlite_master` lookup on every FTS query.
pub(super) static FTS_AVAILABLE: AtomicBool = AtomicBool::new(false);
```

- [ ] **Step 3.3: Initialize the flag in `open()` and `open_in_memory()`**

In `crates/oneshim-storage/src/sqlite/mod.rs`, add an import at the top:

```rust
use fts_search_impl::FTS_AVAILABLE;
```

Then, in both `open()` and `open_in_memory()`, after the `PRAGMA optimize` call (and before the final `Ok(Self { ... })`), add:

```rust
// Cache FTS5 table availability to avoid per-query sqlite_master lookups.
let fts_exists: bool = conn
    .query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='search_fts'",
        [],
        |row| row.get(0),
    )
    .unwrap_or(false);
FTS_AVAILABLE.store(fts_exists, std::sync::atomic::Ordering::Relaxed);
```

- [ ] **Step 3.4: Replace per-query existence checks with fast-path**

In `crates/oneshim-storage/src/sqlite/fts_search_impl.rs`, modify the `search_fts()` method. Replace the `table_exists` query block:

```rust
// OLD: per-query sqlite_master lookup
let table_exists: bool = conn
    .query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='search_fts'",
        [],
        |row| row.get(0),
    )
    .unwrap_or(false);

if !table_exists {
    warn!("search_fts table not available; returning empty results");
    return Ok(vec![]);
}
```

With:

```rust
if !FTS_AVAILABLE.load(Ordering::Relaxed) {
    return Ok(vec![]);
}
```

Apply the same replacement in `upsert_fts()` — replace the `table_exists` block with:

```rust
if !FTS_AVAILABLE.load(Ordering::Relaxed) {
    warn!("search_fts table not available; skipping FTS upsert");
    return Ok(());
}
```

- [ ] **Step 3.5: Verify all FTS tests pass**

```
cargo test -p oneshim-storage fts
```

```
cargo test -p oneshim-storage
```

---

## Task 4: WAL checkpoint in scheduler (5-min PASSIVE)

**Why:** WAL checkpoint is only called during IVF index build. A periodic PASSIVE checkpoint prevents unbounded WAL growth during normal operation.

**Files:**
- Modify: `src-tauri/src/scheduler/config.rs`
- Modify: `src-tauri/src/scheduler/loops/system.rs`

- [ ] **Step 4.1: Add constant for WAL checkpoint interval**

In `src-tauri/src/scheduler/config.rs`, add after the `COACHING_INTERVAL_SECS` constant:

```rust
/// WAL checkpoint interval (seconds). PASSIVE mode is non-blocking.
pub(super) const WAL_CHECKPOINT_INTERVAL_SECS: i64 = 300; // 5 minutes
```

- [ ] **Step 4.2: Add `wal_checkpoint_passive()` method to `SqliteStorage`**

In `crates/oneshim-storage/src/sqlite/mod.rs`, add a public method inside `impl SqliteStorage` (after `read_only_query`):

```rust
/// Run a PASSIVE WAL checkpoint. Non-blocking: checkpoints only pages
/// not currently in use by readers. Safe to call from a background loop.
pub fn wal_checkpoint_passive(&self) -> Result<(), CoreError> {
    let conn = self
        .conn
        .lock()
        .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
    conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE)")
        .map_err(|e| CoreError::Internal(format!("WAL checkpoint failed: {e}")))?;
    Ok(())
}
```

- [ ] **Step 4.3: Write test for WAL checkpoint**

In `crates/oneshim-storage/src/sqlite/tests.rs`, add:

```rust
#[test]
fn wal_checkpoint_passive_runs_without_error() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let storage = SqliteStorage::open(&db_path, 30).unwrap();
    // Should not error on a disk-backed DB with WAL mode
    storage.wal_checkpoint_passive().unwrap();
}
```

```
cargo test -p oneshim-storage wal_checkpoint_passive
```

- [ ] **Step 4.4: Wire WAL checkpoint into `spawn_aggregation_loop`**

In `src-tauri/src/scheduler/loops/system.rs`, inside `spawn_aggregation_loop`, add a `last_wal_checkpoint` tracker and periodic checkpoint. After the `let mut last_index_maintenance` line, add:

```rust
let mut last_wal_checkpoint: Option<chrono::DateTime<Utc>> = None;
```

Then, inside the tick body (at the end, before the `debug!("completed")` line), add:

```rust
// --- WAL checkpoint (every 5 minutes, PASSIVE) ---
{
    let should_checkpoint = last_wal_checkpoint
        .map(|last| (now - last).num_seconds() >= super::super::config::WAL_CHECKPOINT_INTERVAL_SECS)
        .unwrap_or(true);

    if should_checkpoint {
        last_wal_checkpoint = Some(now);
        if let Err(e) = sqlite6.wal_checkpoint_passive() {
            warn!("WAL checkpoint failure: {e}");
        }
    }
}
```

- [ ] **Step 4.5: Verify build**

```
cargo check --workspace
```

---

## Task 5: Conditional VACUUM on idle transition

**Why:** VACUUM reclaims free pages but is expensive. Only run it when the database has >20% free pages and the user is idle.

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs`
- Modify: `src-tauri/src/scheduler/config.rs`
- Modify: `src-tauri/src/scheduler/loops/system.rs`
- Test: `crates/oneshim-storage/src/sqlite/tests.rs`

- [ ] **Step 5.1: Add constant for VACUUM threshold**

In `src-tauri/src/scheduler/config.rs`, add after `WAL_CHECKPOINT_INTERVAL_SECS`:

```rust
/// Minimum freelist percentage before triggering VACUUM (20%).
pub(super) const VACUUM_FREELIST_THRESHOLD_PERCENT: i64 = 20;
```

- [ ] **Step 5.2: Add `maybe_vacuum()` method to `SqliteStorage`**

In `crates/oneshim-storage/src/sqlite/mod.rs`, add inside `impl SqliteStorage`:

```rust
/// Conditionally run VACUUM if free pages exceed `threshold_percent` of total pages.
/// Returns `true` if VACUUM was executed, `false` if skipped.
pub fn maybe_vacuum(&self, threshold_percent: i64) -> Result<bool, CoreError> {
    let conn = self
        .conn
        .lock()
        .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;

    let freelist: i64 = conn
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .map_err(|e| CoreError::Internal(format!("freelist_count failed: {e}")))?;
    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .map_err(|e| CoreError::Internal(format!("page_count failed: {e}")))?;

    if page_count > 0 && freelist * 100 / page_count > threshold_percent {
        conn.execute_batch("VACUUM")
            .map_err(|e| CoreError::Internal(format!("VACUUM failed: {e}")))?;
        Ok(true)
    } else {
        Ok(false)
    }
}
```

- [ ] **Step 5.3: Write test for `maybe_vacuum()`**

In `crates/oneshim-storage/src/sqlite/tests.rs`, add:

```rust
#[test]
fn maybe_vacuum_on_fresh_db_skips() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let storage = SqliteStorage::open(&db_path, 30).unwrap();
    // Fresh DB has no free pages, so VACUUM should be skipped
    let vacuumed = storage.maybe_vacuum(20).unwrap();
    assert!(!vacuumed, "fresh DB should not need VACUUM");
}
```

```
cargo test -p oneshim-storage maybe_vacuum_on_fresh_db_skips
```

- [ ] **Step 5.4: Wire conditional VACUUM into aggregation loop**

In `src-tauri/src/scheduler/loops/system.rs`, inside the aggregation loop tick body, add after the WAL checkpoint block:

```rust
// --- Conditional VACUUM (run only when idle and >20% free pages) ---
// Note: In a full integration this would be triggered on idle transition.
// For now, run alongside the WAL checkpoint interval as a lightweight check.
{
    if should_checkpoint {
        match sqlite6.maybe_vacuum(super::super::config::VACUUM_FREELIST_THRESHOLD_PERCENT) {
            Ok(true) => info!("Conditional VACUUM completed"),
            Ok(false) => {} // Skipped, freelist below threshold
            Err(e) => warn!("Conditional VACUUM failure: {e}"),
        }
    }
}
```

- [ ] **Step 5.5: Verify build**

```
cargo check --workspace
```

---

## Task 6: FTS5 merge/optimize scheduling

**Why:** FTS5 b-tree segments accumulate over time. Periodic merge keeps query performance stable.

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs`
- Modify: `src-tauri/src/scheduler/loops/system.rs`
- Test: `crates/oneshim-storage/src/sqlite/tests.rs`

- [ ] **Step 6.1: Add `fts_merge()` and `fts_optimize()` methods to `SqliteStorage`**

In `crates/oneshim-storage/src/sqlite/mod.rs`, add inside `impl SqliteStorage`:

```rust
/// Merge FTS5 b-tree segments. Lightweight, safe to call periodically.
/// No-op if the `search_fts` table does not exist.
pub fn fts_merge(&self, merge_pages: u32) -> Result<(), CoreError> {
    if !fts_search_impl::FTS_AVAILABLE.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
    let conn = self
        .conn
        .lock()
        .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
    conn.execute(
        "INSERT INTO search_fts(search_fts, rank) VALUES('merge', ?1)",
        rusqlite::params![merge_pages as i64],
    )
    .map_err(|e| CoreError::Internal(format!("FTS5 merge failed: {e}")))?;
    Ok(())
}

/// Fully optimize FTS5 index. More expensive; run daily or after bulk inserts.
/// No-op if the `search_fts` table does not exist.
pub fn fts_optimize(&self) -> Result<(), CoreError> {
    if !fts_search_impl::FTS_AVAILABLE.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
    let conn = self
        .conn
        .lock()
        .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
    conn.execute_batch("INSERT INTO search_fts(search_fts) VALUES('optimize')")
        .map_err(|e| CoreError::Internal(format!("FTS5 optimize failed: {e}")))?;
    Ok(())
}
```

- [ ] **Step 6.2: Write tests for FTS merge/optimize**

In `crates/oneshim-storage/src/sqlite/tests.rs`, add:

```rust
#[test]
fn fts_merge_runs_without_error() {
    let storage = SqliteStorage::open_in_memory(30).unwrap();
    storage.fts_merge(200).unwrap();
}

#[test]
fn fts_optimize_runs_without_error() {
    let storage = SqliteStorage::open_in_memory(30).unwrap();
    storage.fts_optimize().unwrap();
}
```

```
cargo test -p oneshim-storage fts_merge_runs
cargo test -p oneshim-storage fts_optimize_runs
```

- [ ] **Step 6.3: Wire FTS merge into aggregation loop**

In `src-tauri/src/scheduler/loops/system.rs`, inside the aggregation loop tick body, add after the VACUUM block:

```rust
// --- FTS5 merge (every 5 minutes alongside WAL checkpoint) ---
{
    if should_checkpoint {
        if let Err(e) = sqlite6.fts_merge(200) {
            warn!("FTS5 merge failure: {e}");
        }
    }
}
```

- [ ] **Step 6.4: Wire FTS optimize into daily digest block**

In `src-tauri/src/scheduler/loops/system.rs`, inside the daily digest block (after `info!("Daily digest generated for {}", date_str);`), add:

```rust
// FTS5 full optimize — daily, after digest generation
if let Err(e) = sqlite6.fts_optimize() {
    warn!("FTS5 optimize failure: {e}");
}
```

- [ ] **Step 6.5: Verify build**

```
cargo check --workspace
```

---

## Task 7: ANALYZE after bulk operations

**Why:** `ANALYZE` updates query planner statistics. Important after batch inserts to keep the query planner informed.

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs`
- Modify: `crates/oneshim-storage/src/sqlite/events.rs`
- Test: `crates/oneshim-storage/src/sqlite/tests.rs`

- [ ] **Step 7.1: Add `run_analyze()` method to `SqliteStorage`**

In `crates/oneshim-storage/src/sqlite/mod.rs`, add inside `impl SqliteStorage`:

```rust
/// Run `ANALYZE` to update query planner statistics.
/// Safe to call after bulk insert operations.
pub fn run_analyze(&self) -> Result<(), CoreError> {
    let conn = self
        .conn
        .lock()
        .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
    conn.execute_batch("ANALYZE")
        .map_err(|e| CoreError::Internal(format!("ANALYZE failed: {e}")))?;
    Ok(())
}
```

- [ ] **Step 7.2: Write test for `run_analyze()`**

In `crates/oneshim-storage/src/sqlite/tests.rs`, add:

```rust
#[test]
fn run_analyze_completes_without_error() {
    let storage = SqliteStorage::open_in_memory(30).unwrap();
    storage.run_analyze().unwrap();
}
```

```
cargo test -p oneshim-storage run_analyze_completes
```

- [ ] **Step 7.3: Call `run_analyze()` after large batch inserts in `save_events_batch()`**

In `crates/oneshim-storage/src/sqlite/events.rs`, in `save_events_batch()`, after the `tx.commit()` call and before the `debug!("event batch save: ...")` log, add:

```rust
// Update query planner statistics after bulk inserts (>100 rows)
if events.len() >= 100 {
    if let Ok(inner_conn) = self.conn.lock() {
        let _ = inner_conn.execute_batch("ANALYZE");
    }
}
```

- [ ] **Step 7.4: Verify all tests pass**

```
cargo test -p oneshim-storage
```

---

## Task 8: `prepare_cached()` migration for hot paths

**Why:** `conn.prepare()` compiles SQL on every call. `conn.prepare_cached()` reuses compiled statement handles from an LRU cache, avoiding repeated SQL parsing on high-frequency queries.

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/events.rs`
- Modify: `crates/oneshim-storage/src/sqlite/frames.rs`
- Modify: `crates/oneshim-storage/src/sqlite/fts_search_impl.rs`

- [ ] **Step 8.1: Migrate `save_event()` to `prepare_cached()`**

In `crates/oneshim-storage/src/sqlite/events.rs`, in the `save_event()` method (inside the `StorageService` impl), replace `conn.execute(` with `prepare_cached` + `execute`:

Replace:
```rust
conn.execute(
    "INSERT OR IGNORE INTO events (event_id, event_type, timestamp, data) VALUES (?1, ?2, ?3, ?4)",
    rusqlite::params![event_id, event_type, timestamp, data],
)
```

With:
```rust
conn.prepare_cached(
    "INSERT OR IGNORE INTO events (event_id, event_type, timestamp, data) VALUES (?1, ?2, ?3, ?4)",
)
.map_err(|e| CoreError::Internal(format!("event prepare failed: {e}")))?
.execute(rusqlite::params![event_id, event_type, timestamp, data])
```

- [ ] **Step 8.2: Migrate `get_events()` to `prepare_cached()`**

In `crates/oneshim-storage/src/sqlite/events.rs`, in `get_events()`, replace `conn.prepare(` with `conn.prepare_cached(`.

- [ ] **Step 8.3: Migrate `get_pending_events()` to `prepare_cached()`**

In `crates/oneshim-storage/src/sqlite/events.rs`, in `get_pending_events()`, replace `conn.prepare(` with `conn.prepare_cached(`.

- [ ] **Step 8.4: Migrate `save_frame_metadata_with_bounds()` to `prepare_cached()`**

In `crates/oneshim-storage/src/sqlite/frames.rs`, in `save_frame_metadata_with_bounds()`, replace `conn.execute(` with `prepare_cached` + `execute`:

Replace:
```rust
conn.execute(
    "INSERT INTO frames (timestamp, trigger_type, app_name, window_title, importance, resolution_w, resolution_h, has_image, file_path, ocr_text, window_x, window_y, window_width, window_height)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    rusqlite::params![...],
)
```

With:
```rust
conn.prepare_cached(
    "INSERT INTO frames (timestamp, trigger_type, app_name, window_title, importance, resolution_w, resolution_h, has_image, file_path, ocr_text, window_x, window_y, window_width, window_height)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
)
.map_err(|e| CoreError::Internal(format!("frame prepare failed: {e}")))?
.execute(rusqlite::params![...])
```

- [ ] **Step 8.5: Migrate `get_frames()` to `prepare_cached()`**

In `crates/oneshim-storage/src/sqlite/frames.rs`, in `get_frames()`, replace `conn.prepare(` with `conn.prepare_cached(`.

- [ ] **Step 8.6: Migrate FTS search query to `prepare_cached()`**

In `crates/oneshim-storage/src/sqlite/fts_search_impl.rs`, in `search_fts()`, replace `conn.prepare(` with `conn.prepare_cached(`:

Replace:
```rust
let mut stmt = conn
    .prepare(
        "SELECT segment_id, content_type, searchable_text, rank ..."
    )
```

With:
```rust
let mut stmt = conn
    .prepare_cached(
        "SELECT segment_id, content_type, searchable_text, rank ..."
    )
```

- [ ] **Step 8.7: Verify all existing tests pass**

```
cargo test -p oneshim-storage
```

All tests should pass unchanged because `prepare_cached()` is API-compatible with `prepare()`.

---

## Task 9: Korean FTS5 trigram table (V18 migration)

**Why:** The existing FTS5 `porter unicode61` tokenizer does not support Korean text. A trigram-tokenized table enables Korean substring search.

**Files:**
- Modify: `crates/oneshim-storage/src/migration/mod.rs`
- Rename: `crates/oneshim-storage/src/migration/v09_v17.rs` -> `crates/oneshim-storage/src/migration/v09_v18.rs`
- Test: `crates/oneshim-storage/src/migration/tests.rs`

- [ ] **Step 9.1: Write test for V18 migration**

In `crates/oneshim-storage/src/migration/tests.rs`, add:

```rust
#[test]
fn migration_v18_creates_trigram_table() {
    let conn = Connection::open_in_memory().unwrap();
    super::run_migrations(&conn).unwrap();

    let version = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get::<_, u32>(0),
        )
        .unwrap();
    assert!(version >= 18, "schema version must be at least 18");

    // Verify the trigram table exists by attempting a query
    let result = conn.execute_batch(
        "INSERT INTO search_trigram (content) VALUES ('test content');",
    );
    assert!(result.is_ok(), "search_trigram table must exist after V18 migration");
}
```

```
cargo test -p oneshim-storage migration_v18
```

Verify the test **fails** (V18 does not exist yet).

- [ ] **Step 9.2: Rename migration file**

Rename `crates/oneshim-storage/src/migration/v09_v17.rs` to `crates/oneshim-storage/src/migration/v09_v18.rs`.

Update the module declaration in `crates/oneshim-storage/src/migration/mod.rs`:

Replace:
```rust
mod v09_v17;
```

With:
```rust
mod v09_v18;
```

And update all `v09_v17::` references in `run_migrations()` to `v09_v18::`.

- [ ] **Step 9.3: Update the module docstring**

In `crates/oneshim-storage/src/migration/v09_v18.rs`, update the top-level docstring to include V18:

```rust
//! Migrations V9–V18: tiered memory, vectors, sync, IVF, coaching, Korean FTS.
//!
//! V9:  calibration_log, trigger_params_snapshots, regimes, activity_segments
//! V10: embedding_vectors, weekly_digests
//! V11: FTS5 search_fts, daily_digests
//! V12: regime_overrides (recalibration)
//! V13: gui_interactions
//! V14: INT8 quantization columns + cross-device sync metadata (HLC, tombstones)
//! V15: lan_peer_pins (Sync 3b TOFU)
//! V16: IVF index + 2-bit binary codes
//! V17: coaching engine tables
//! V18: Korean FTS5 trigram table
```

- [ ] **Step 9.4: Add `migrate_v18()` function**

Append to `crates/oneshim-storage/src/migration/v09_v18.rs`:

```rust
pub(super) fn migrate_v18(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V18 execution: Korean FTS5 trigram table");

    // Trigram tokenizer enables substring matching for Korean (and CJK) text.
    // The bundled SQLite in rusqlite includes the trigram tokenizer by default.
    let trigram_result = conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS search_trigram
            USING fts5(content, tokenize='trigram');
        ",
    );
    if let Err(e) = trigram_result {
        tracing::warn!("FTS5 trigram table creation skipped (tokenizer not available): {e}");
    }

    conn.execute_batch(
        "
        -- version record
        INSERT INTO schema_version (version) VALUES (18);
        ",
    )?;

    info!("migration V18 completed");
    Ok(())
}
```

- [ ] **Step 9.5: Bump `CURRENT_VERSION` and add V18 dispatch**

In `crates/oneshim-storage/src/migration/mod.rs`:

Change:
```rust
pub(crate) const CURRENT_VERSION: u32 = 17;
```

To:
```rust
pub(crate) const CURRENT_VERSION: u32 = 18;
```

Add after the `if current < 17` block:

```rust
if current < 18 {
    v09_v18::migrate_v18(conn)?;
}
```

- [ ] **Step 9.6: Verify V18 migration test passes**

```
cargo test -p oneshim-storage migration_v18
```

- [ ] **Step 9.7: Verify all tests pass**

```
cargo test -p oneshim-storage
```

```
cargo check --workspace
```

---

## Verification Checklist

After all tasks are complete, run the full verification:

```bash
# Full workspace build
cargo check --workspace

# All storage tests
cargo test -p oneshim-storage

# Clippy lint
cargo clippy -p oneshim-storage

# Format check
cargo fmt --check

# Workspace-wide test
cargo test --workspace
```

Expected outcomes:
- `journal_size_limit` = 64MB on disk databases
- `PRAGMA optimize` runs after migrations in both `open()` and `open_in_memory()`
- `open_in_memory()` applies `synchronous`, `cache_size`, `temp_store` PRAGMAs
- FTS queries skip `sqlite_master` lookup via `AtomicBool` cache
- WAL checkpoint runs every 5 minutes (PASSIVE, non-blocking)
- VACUUM only runs when freelist exceeds 20% of pages
- FTS merge runs every 5 minutes; FTS optimize runs daily
- `ANALYZE` runs after batch inserts of 100+ events
- Hot-path queries use `prepare_cached()` for statement reuse
- V18 migration creates `search_trigram` table for Korean text search
