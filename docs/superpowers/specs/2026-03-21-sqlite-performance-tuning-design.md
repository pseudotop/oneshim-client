# SQLite Performance Tuning — Design Spec

> Created: 2026-03-21
> Revised: 2026-03-21 (post-review)
> Priority: P1
> Effort: 4 days
> Status: Proposed
> Scope: oneshim-storage (sqlite.rs, migration.rs), src-tauri (scheduler)
> Reference: ADR-003 (Directory Module Pattern for Large Source Files)

## 1. Current State (VERIFIED)

### 1.1 Existing PRAGMAs in `sqlite.rs` open()

| PRAGMA | Value | Status | Notes |
|--------|-------|--------|-------|
| `journal_mode` | WAL | VERIFIED CORRECT | Write-Ahead Logging enabled |
| `synchronous` | NORMAL | VERIFIED CORRECT | Safe with WAL mode |
| `cache_size` | 8000 | VERIFIED CORRECT | Positive value = 8000 pages = 32MB (page_size 4096) |
| `temp_store` | MEMORY | VERIFIED CORRECT | Temp tables in memory |
| `mmap_size` | 268435456 (256MB) | VERIFIED CORRECT | Already set for read-heavy workloads |
| `page_size` | 4096 | VERIFIED CORRECT | Already aligned with OS page size |

### 1.2 What is MISSING

| Item | Status | Notes |
|------|--------|-------|
| `journal_size_limit` | NOT SET | WAL file grows unbounded; should cap at 64MB |
| `PRAGMA optimize` | NOT CALLED | Should run after migrations in `open()` |
| `busy_timeout` | NOT SET | Not needed today (single Mutex connection). Becomes relevant only if a read-only connection is added in future |
| `prepare_cached()` | NOT USED | `conn.prepare()` called on every query; `prepare_cached()` would reuse statement handles |
| `open_in_memory()` PRAGMA parity | DIVERGED | Test helper `open_in_memory()` skips the entire PRAGMA block, creating a test/production divergence risk |

### 1.3 Existing Retention Policies (VERIFIED)

Edge Intelligence retention already exists in `enforce_all_retention()` (edge_intelligence/retention.rs):
- `work_sessions`: 90 days
- `interruptions`: 90 days
- `focus_metrics`: 365 days

These are NOT gaps and require no changes.

### 1.4 WAL Checkpoint

WAL checkpoint is only performed inside IVF index build (`build.rs:176,295`). No periodic checkpoint exists in the scheduler.

## 2. Proposed Changes

### A. New PRAGMAs

Add to the PRAGMA block in `open()`:

```sql
PRAGMA journal_size_limit = 67108864;  -- 64MB cap on WAL file size
```

Add after migration completion in `open()`:

```sql
PRAGMA optimize = 0x10002;  -- Analyze tables that need it, including schema changes
```

The `optimize` call ensures statistics are fresh after schema migrations. This may add 100-200ms to startup on large databases.

### B. FTS5 Existence Caching (AtomicBool)

Currently each FTS query checks whether the `search_fts` table exists. Replace with:

```rust
static FTS_AVAILABLE: AtomicBool = AtomicBool::new(false);

// Set once during open() after migration
FTS_AVAILABLE.store(table_exists("search_fts"), Ordering::Relaxed);

// Fast path in query methods
if !FTS_AVAILABLE.load(Ordering::Relaxed) {
    return Ok(vec![]);
}
```

Eliminates one SQLite round-trip per FTS query.

### C. FTS5 Merge/Optimize Scheduling

Add periodic FTS maintenance to the scheduler:

```sql
-- During idle periods (IdleState::Active -> Idle transition)
INSERT INTO search_fts(search_fts, rank) VALUES('merge', 200);  -- Merge b-tree segments

-- Daily (or after large batch inserts)
INSERT INTO search_fts(search_fts) VALUES('optimize');  -- Full optimization
```

### D. Korean FTS5 Trigram Table

Add a trigram-tokenized FTS table for Korean text search:

```sql
-- V18 migration
CREATE VIRTUAL TABLE IF NOT EXISTS search_trigram
    USING fts5(content, tokenize='trigram');
```

Requires verifying that the bundled SQLite (via `rusqlite` `bundled` feature) includes the trigram tokenizer. If not, the `bundled` build must be configured to include it.

### E. Conditional VACUUM (Idle-Time)

Run VACUUM only when beneficial and during idle periods:

```rust
// Triggered on IdleState::Active -> Idle transition
fn maybe_vacuum(conn: &Connection) -> Result<()> {
    let freelist: i64 = conn.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
    let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    // Only vacuum if >20% free pages
    if freelist > page_count / 5 {
        conn.execute_batch("VACUUM")?;
    }
    Ok(())
}
```

### F. WAL Checkpoint in Scheduler (5-10 min, PASSIVE)

Add periodic WAL checkpoint to the scheduler aggregate loop:

```rust
// Every 5-10 minutes (not every 10 seconds — checkpoints are not free)
conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE)")?;
```

PASSIVE mode is non-blocking: it checkpoints whatever pages are not currently in use by readers, and returns immediately. No risk of blocking writes.

### G. ANALYZE After Bulk Operations

After batch inserts (e.g., batch upload, IVF rebuild), run:

```sql
ANALYZE;
```

This updates query planner statistics. Already a pattern in the IVF build path but not after regular batch inserts.

### H. mmap Safety on External Drives

The current `mmap_size=256MB` is correct for internal drives. On external/removable drives (e.g., the project's own PCIe4 SSD), mmap is safe. However, if the database is on a network drive or USB stick that can be ejected:

- Detection: Check if `data_dir` is on a removable volume (platform-specific)
- Mitigation: Set `mmap_size=0` for removable media

This is low priority and can be deferred.

### I. `prepare_cached()` Migration

Replace `conn.prepare()` with `conn.prepare_cached()` in hot-path queries. `prepare_cached()` reuses compiled statement handles, avoiding repeated SQL parsing.

Target locations (highest frequency):
- `store_event()` / `store_events_batch()`
- `store_frame_metadata()`
- `get_events_in_range()`
- `get_frames_in_range()`
- FTS insert/query paths

### J. `open_in_memory()` PRAGMA Parity

The test helper `open_in_memory()` creates an in-memory database but skips all PRAGMA configuration. This means tests run under different conditions than production.

Fix: Extract PRAGMA setup into a shared `configure_connection(conn: &Connection)` function called by both `open()` and `open_in_memory()`. Exclude only PRAGMAs that are meaningless for in-memory databases (`journal_mode`, `mmap_size`, `journal_size_limit`).

## 3. NOT Changing (Already Correct)

| Item | Reason |
|------|--------|
| `cache_size=8000` (32MB) | Adequate for desktop workload. Positive value means pages, not KB. 8000 * 4096 = 32MB. |
| `mmap_size=256MB` | Already set in PRAGMA block. No action needed. |
| `page_size=4096` | Already set. Changing page_size on an existing database is a no-op (requires VACUUM to take effect, and benefits are marginal). |
| `busy_timeout` | Irrelevant with single-Mutex connection model. Only becomes relevant if/when a separate read-only connection is added. |
| Edge Intelligence retention | Already implemented: work_sessions 90d, interruptions 90d, focus_metrics 365d. |
| Connection pooling | Single Mutex is sufficient at the current write rate (~7 writes/sec). Pooling adds complexity without measurable benefit. |

## 4. Modified Files

| File | Change |
|------|--------|
| `crates/oneshim-storage/src/sqlite.rs` | Add `journal_size_limit` PRAGMA, `PRAGMA optimize` after migrations, extract `configure_connection()`, add FTS5 `AtomicBool` cache, migrate hot paths to `prepare_cached()` |
| `crates/oneshim-storage/src/migration.rs` | Add V18 migration for Korean trigram FTS table |
| `src-tauri/src/scheduler/loops.rs` | Add WAL checkpoint (5-10 min interval, PASSIVE), ANALYZE after bulk ops, conditional VACUUM on idle transition |
| `src-tauri/tests/` | PRAGMA parity regression test, FTS merge/optimize test |

## 5. Effort (4 days)

| Task | Days |
|------|------|
| A. New PRAGMAs + J. PRAGMA parity refactor | 0.5 |
| B. FTS5 existence caching | 0.25 |
| C. FTS5 merge/optimize scheduling | 0.25 |
| D. Korean trigram FTS table + verify bundled SQLite | 1.0 |
| E. Conditional VACUUM | 0.25 |
| F. WAL checkpoint in scheduler | 0.25 |
| G. ANALYZE after bulk ops | 0.25 |
| I. `prepare_cached()` migration | 0.75 |
| Testing + benchmarking | 0.5 |
| **Total** | **4.0** |

Note: Reviewer estimated 2-3 days once duplicate/already-implemented items were removed. Korean trigram FTS verification and `prepare_cached()` migration across multiple call sites add the remaining day.

## 6. Phased Rollout

**Phase 1 (Day 1-2)**: PRAGMAs + PRAGMA parity + WAL checkpoint + conditional VACUUM
- Low risk, high confidence
- Immediately measurable via WAL file size and free page count

**Phase 2 (Day 2-3)**: FTS5 caching + merge/optimize + ANALYZE
- Moderate complexity, requires scheduler integration
- FTS existence caching is zero-risk

**Phase 3 (Day 3-4)**: Korean trigram FTS + `prepare_cached()` migration
- Korean trigram requires bundled SQLite verification
- `prepare_cached()` is mechanical but touches many call sites

## 7. Risks

| Risk | Mitigation |
|------|------------|
| `PRAGMA optimize` after migration could slow startup by 100-200ms | Acceptable trade-off; only runs once at startup |
| Korean trigram FTS requires verifying bundled SQLite includes trigram tokenizer | Check `rusqlite` bundled feature flags; if missing, file upstream issue or enable via build config |
| `open_in_memory()` PRAGMA parity may require refactor of test setup | Extract shared `configure_connection()` function; exclude I/O-only PRAGMAs for in-memory |
| `prepare_cached()` LRU eviction under high statement diversity | Default LRU size (64) is sufficient for our ~30 distinct queries |
| Aggressive WAL checkpointing wastes I/O if WAL is small | 5-10 min interval with PASSIVE mode is conservative enough |
