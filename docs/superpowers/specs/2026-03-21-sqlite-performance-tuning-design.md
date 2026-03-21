# SQLite Performance Tuning Design Spec

**Date:** 2026-03-21
**Priority:** P1
**Effort:** 4 days
**Status:** Proposed
**Impact:** No new crates, no new ports, no schema migration

---

## 1. Current State

**PRAGMA configuration** (`crates/oneshim-storage/src/sqlite/mod.rs:26-71`):

```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA cache_size=8000;       -- 32MB
PRAGMA temp_store=MEMORY;
PRAGMA mmap_size=268435456;   -- 256MB
PRAGMA page_size=4096;
```

**Bundled SQLite:** 3.51.1 (via `libsqlite3-sys 0.36.0`).

**Connection model:** Single `Arc<Mutex<Connection>>` — all reads/writes serialized.

**Code-level findings (deep dive 2026-03-21):**

| Finding | Location | Detail |
|---------|----------|--------|
| FTS existence check per-query | `fts_search_impl.rs:20-27, 117-134, 179-185` | 3x `sqlite_master` queries per enriched sync |
| No FTS sync in scheduler | `scheduler/loops/system.rs:112-443` | FTS merge/optimize never called |
| No ANALYZE after IVF build | `vector_index_impl/build.rs:176,295` | `wal_checkpoint(TRUNCATE)` present, ANALYZE missing |
| mmap unconditional 256MB | `mod.rs:69` | No external drive detection |
| GDPR `delete_all_data()` gap | `maintenance.rs:357-395` | Only 5-7 of 33 tables deleted — needs verification |
| DB path platform-specific | `bootstrap_runtime.rs:116-121` | macOS `~/Library/Application Support/oneshim/data/` |

---

## 2. Proposed Changes

### A. Add Missing PRAGMAs (at connection open)

```sql
PRAGMA journal_size_limit=67108864;  -- 64MB WAL size safety cap
PRAGMA optimize=0x10002;             -- ANALYZE all tables at open (3.46.0+)
```

**`busy_timeout`:** Irrelevant for single-Mutex design. Add only when read-only connection introduced (§2.F).

### B. Scheduled VACUUM (idle-time)

`auto_vacuum=INCREMENTAL` cannot be enabled on existing DB without full VACUUM conversion.

```rust
let freelist: u64 = conn.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
let total: u64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
if total > 0 && freelist * 100 / total > 15 {
    conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
}
```

Trigger: freelist > 15% AND user idle > 30 min (or startup if last VACUUM > 7 days).

### C. FTS5 Optimization

**C1. Cache table existence:** Replace 3x per-operation `sqlite_master` queries with `AtomicBool` checked once at open.

```rust
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
    retention_days: u32,
    fts_available: AtomicBool,
    gui_table_available: AtomicBool,
}
```

**C2. FTS5 merge/optimize scheduling:**

```sql
-- Hourly: gentle incremental merge
INSERT INTO search_fts(search_fts, rank) VALUES('merge', 500);
-- Daily (idle): full defrag
INSERT INTO search_fts(search_fts) VALUES('optimize');
```

**C3. Korean trigram FTS:** Add second FTS5 table with `trigram` tokenizer for Korean content. Detect language via Hangul range (`\uAC00-\uD7A3`).

### D. Periodic `PRAGMA optimize`

```sql
-- Hourly
PRAGMA optimize;
```

### E. `ANALYZE` After Bulk Operations

Add after IVF index builds and bulk retention enforcement.

### F. Read-Only Connection (future evolution)

Separate `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_WAL` connection for dashboard/search queries. Add `busy_timeout=5000` on both connections.

### G. mmap Safety on External Drives

Detect `/Volumes/` prefix on macOS → reduce `mmap_size` to 0 or 64MB. SIGBUS risk on drive ejection.

---

## 3. Modified Files

| File | Change |
|---|---|
| `crates/oneshim-storage/src/sqlite/mod.rs` | PRAGMAs, FTS cache flags, mmap detection |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | Remove per-query existence checks |
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | `conditional_vacuum()`, `fts_merge()`, `fts_optimize()` |
| `src-tauri/src/scheduler/loops/system.rs` | FTS merge (hourly), VACUUM check (idle), optimize (hourly) |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/build.rs` | Add `ANALYZE` after IVF build |

## 4. Effort

| Task | Estimate |
|---|---|
| PRAGMAs (journal_size_limit, optimize, mmap safety) | 0.5 day |
| FTS existence caching | 0.5 day |
| Conditional VACUUM + WAL checkpoint | 0.5 day |
| FTS5 merge/optimize scheduling | 0.5 day |
| FTS5 trigram Korean table | 1 day |
| ANALYZE integration | 0.25 day |
| Tests | 0.75 day |
| **Total** | **4 days** |

## 5. Phased Rollout

| Phase | Scope |
|---|---|
| **A** | PRAGMAs + FTS existence caching + mmap safety |
| **B** | Conditional VACUUM + FTS merge/optimize scheduling |
| **C** | ANALYZE after bulk ops + Korean trigram FTS table |
| **D** | Read-only connection (future, when query latency measured) |

## 6. Research Sources

- [SQLite PRAGMA docs](https://sqlite.org/pragma.html), [FTS5](https://www.sqlite.org/fts5.html), [WAL](https://sqlite.org/wal.html), [mmap safety](https://sqlite.org/mmap.html)
