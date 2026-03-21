# Architecture Improvements Design Spec (v2)

**Date:** 2026-03-21 (revised 2026-03-21 with deep research)
**Status:** Proposed
**MCP Server:** Deferred (security concerns + Skills trend)
**Audio/STT:** Deferred to separate spec (customer demand-driven, see `deferred/audio-stt-research.md`)

> **Revision note:** v2 incorporates findings from 4 parallel research agents covering
> HNSW library landscape, SQLite tuning best practices, and codebase architecture audit.
> Audio/STT removed from scope — research preserved in deferred spec for future use.

---

## Table of Contents

1. [SQLite Performance Tuning (P1)](#1-sqlite-performance-tuning-p1)
2. [USearch HNSW Vector Index (P1)](#2-usearch-hnsw-vector-index-p1)
3. [Tauri IPC Optimization (P3)](#3-tauri-ipc-optimization-p3)
4. [Cross-Cutting Improvements](#4-cross-cutting-improvements)

---

## 1. SQLite Performance Tuning (P1)

> **Priority change:** P2 → P1. Smallest effort, immediate benefit, improves foundation
> for all other improvements.

### 1.1 Current State

**PRAGMA configuration** (from `crates/oneshim-storage/src/sqlite/mod.rs`):

```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA cache_size=8000;       -- 8000 pages = ~32MB
PRAGMA temp_store=MEMORY;
PRAGMA mmap_size=268435456;   -- 256MB memory-mapped I/O
PRAGMA page_size=4096;
```

**Bundled SQLite version:** 3.51.1 (via `libsqlite3-sys 0.36.0`). All modern PRAGMAs
including `PRAGMA optimize` with `0x10002` bitmask are available.

**Connection model:** Single `Arc<Mutex<Connection>>` — all reads and writes serialized.

### 1.2 Proposed Changes

#### A. Add Missing PRAGMAs (at connection open)

```sql
PRAGMA journal_size_limit=67108864;  -- 64MB WAL size safety cap
PRAGMA optimize=0x10002;             -- ANALYZE all tables at open (3.46.0+)
```

**`busy_timeout` clarification:** Under the current single-Mutex design, `busy_timeout`
is **irrelevant** — the Rust Mutex serializes access before SQLite sees contention.
Add `PRAGMA busy_timeout=5000;` **only when** a read-only connection is introduced
(see §1.2.F below).

#### B. Scheduled VACUUM (idle-time, not incremental)

> **v1 correction:** `auto_vacuum=INCREMENTAL` **cannot be enabled on an existing
> database** without a full `VACUUM` to convert the page format. The overhead of
> conversion is unjustified.

**Strategy:** Conditional manual `VACUUM` during idle periods.

```rust
// In scheduler idle detection callback or daily maintenance
let freelist: u64 = conn.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
let total: u64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
if total > 0 && freelist * 100 / total > 15 {
    conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
}
```

Trigger conditions: freelist > 15% of page_count AND user has been idle > 30 minutes
(or at app startup if last VACUUM was > 7 days ago).

#### C. FTS5 Optimization

**C1. Cache table existence check:**

The `search_fts` table is checked via `sqlite_master` on **every FTS operation** — thousands
of queries per day. Cache this as an `AtomicBool` set once during initialization.

```rust
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
    retention_days: u32,
    fts_available: AtomicBool,       // Checked once at open()
    gui_table_available: AtomicBool, // Checked once at open()
}
```

**C2. FTS5 `merge` (hourly) + `optimize` (daily):**

```sql
-- Hourly: gentle incremental merge
INSERT INTO search_fts(search_fts, rank) VALUES('merge', 500);

-- Daily (idle window): full segment defragmentation
INSERT INTO search_fts(search_fts) VALUES('optimize');

-- Periodic WAL reclaim
PRAGMA wal_checkpoint(TRUNCATE);
```

**C3. Korean language support — `trigram` tokenizer:**

Current `porter unicode61` handles English stemming but does nothing for Korean morphology.
The `trigram` tokenizer handles Hangul syllable blocks well (each syllable = 1 char,
trigrams catch 3-syllable sequences). Options:

| Approach | Pros | Cons |
|----------|------|------|
| Replace with `trigram` | One table, Korean+English substring matching | Loses English stemming |
| **Add second FTS5 table** | Best of both — `search_fts` for English, `search_fts_ko` for Korean | Doubles FTS storage |
| Keep `porter unicode61` | No change | Korean search remains poor |

**Recommendation:** Add a second FTS5 table with `trigram` tokenizer for Korean content.
Detect language via simple Hangul Unicode range check (`\uAC00-\uD7A3`).

#### D. Periodic `PRAGMA optimize`

```sql
-- Hourly: let SQLite update statistics for recently-queried tables
PRAGMA optimize;
```

SQLite 3.46.0+ automatically applies `analysis_limit` to prevent long ANALYZE runs.
No need to set `analysis_limit` manually.

#### E. `ANALYZE` After Bulk Operations

Run `ANALYZE` after:
- IVF index builds (already does `wal_checkpoint(TRUNCATE)`, add `ANALYZE`)
- Bulk retention enforcement (large DELETEs invalidate planner statistics)

#### F. Read-Only Connection (architecture evolution)

For vector search (10-50ms) and FTS queries that block the scheduler's 3-second write
cycle, a dedicated read-only connection provides parallel read/write under WAL mode.

```rust
// Open with read-only flags
let read_conn = Connection::open_with_flags(
    &db_path,
    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_WAL,
)?;
read_conn.execute_batch("PRAGMA busy_timeout=5000;")?;
```

Route all SELECT-only operations (dashboard, search, stats) through the read connection.
The write connection retains the existing Mutex pattern.

> **Note:** This is a future evolution, not part of the initial SQLite tuning phase.

#### G. mmap Safety on External Drives

> **CRITICAL:** The working directory is on `/Volumes/ext-PCIe4-1TB/` (external PCIe drive).
> If the drive is ejected while the app is running with mmap active, the process crashes
> with SIGBUS.

**Mitigation:** Detect external/removable media at startup. If the database resides on a
removable volume, reduce `mmap_size` to 0 (disable) or 64MB. On internal drives, retain
256MB.

```rust
#[cfg(target_os = "macos")]
fn is_external_volume(path: &Path) -> bool {
    path.starts_with("/Volumes/")
}
```

### 1.3 Architecture Impact

**No new crates. No new ports. No schema migration.**

| File | Change |
|---|---|
| `crates/oneshim-storage/src/sqlite/mod.rs` | Add PRAGMAs, FTS cache flags, mmap detection |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | Remove per-query existence checks, use cached `AtomicBool` |
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | Add `conditional_vacuum()`, `fts_merge()`, `fts_optimize()` |
| `src-tauri/src/scheduler/loops.rs` | Call FTS merge (hourly), VACUUM check (idle), PRAGMA optimize (hourly) |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/build.rs` | Add `ANALYZE` after IVF build |

### 1.4 Effort Estimate

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

### 1.5 Phased Rollout

| Phase | Scope |
|---|---|
| **A** | PRAGMAs + FTS existence caching + mmap safety (quick wins) |
| **B** | Conditional VACUUM + FTS merge/optimize scheduling |
| **C** | ANALYZE after bulk ops + Korean trigram FTS table |
| **D** | Read-only connection (future, when query latency measured) |

---

## 2. USearch HNSW Vector Index (P1)

### 2.1 Current State

The codebase implements a 3-tier adaptive vector search system managed by
`AdaptiveSearchCoordinator` in `crates/oneshim-analysis/src/adaptive_search.rs`:

| Strategy | Vector Count Range | Implementation |
|---|---|---|
| `BruteForceInt8` | < 10,000 | Full table scan with INT8 cosine similarity |
| `IvfInt8` | 10,000 -- 100,000 | IVF k-means++ clustering with nprobe partition scan |
| `IvfBinaryRerank` | >= 100,000 | IVF + 2-bit Hamming filter + INT8 re-rank |

**Port traits:** `VectorStore` (109 LOC) + `VectorIndex` (135 LOC, IVF-centric).
**Storage:** `SqliteVectorStore` + `SqliteVectorIndex` (both directory modules).
**IVF infrastructure:** V16 migration tables (`ivf_centroids`, `ivf_assignments`,
`vector_binary_codes`, `vector_index_meta`).

### 2.2 Library Selection: `usearch` (confirmed via deep research)

| Criterion | usearch | hnswlib-rs (fallback) |
|---|---|---|
| Type | C++ core + Rust FFI | Pure Rust |
| SIMD | Full: AVX2/AVX-512/NEON/SVE (via SimSIMD) | LLVM auto-vectorization only |
| INT8 support | Native i8, f16, bf16 | f32, f16, per-vector int8 |
| Maintenance | Monthly releases (3,950+ commits) | Active (Jan 2026) |
| Apple Silicon | Auto-detected NEON (3-8x speedup) | No explicit NEON |
| Concurrency | Concurrent add + search (thread ID hint) | Lock-free reads + mutation |
| Binary size | +2-5MB (C++ static lib) | +200-500KB (pure Rust) |
| License | Apache-2.0 | Apache-2.0 / MIT |

**Primary:** `usearch v2.24.0`. **Fallback:** `hnswlib-rs` if C++ build dependency
is unacceptable (CI simplicity or pure-Rust policy).

**Libraries NOT recommended:** `instant-distance` (dormant ~2y), `hora` (abandoned ~4y),
`sqlite-vec` (brute-force only, no ANN, 17x slower than hand-rolled INT8).

### 2.3 Known Issues

> **⚠️ `Send + Sync`:** USearch's Rust `Index` is not natively `Send + Sync`.
> Workaround required. Tracked in [usearch#482](https://github.com/unum-cloud/usearch/issues/482).
> Solution: wrap in `unsafe impl Send/Sync` with documented safety invariants, or use
> `Mutex<Index>` for exclusive access.

> **⚠️ Thread crash:** If thread count exceeds `hardware_concurrency()`, USearch may
> crash ([usearch#389](https://github.com/unum-cloud/usearch/issues/389)). Mitigate by
> capping parallelism to `num_cpus::get() - 1` during index build.

### 2.4 Proposed Change

**Add HNSW as a fourth strategy complementing IVF:**

| Strategy | Vector Count Range | Index Type |
|---|---|---|
| `BruteForceInt8` | < 5,000 | None (scan all) |
| `HnswInt8` | 5,000 -- 50,000 | In-memory HNSW graph (usearch) |
| `IvfInt8` | 50,000 -- 100,000 | SQLite-persisted IVF |
| `IvfBinaryRerank` | >= 100,000 | IVF + Hamming + re-rank |

**Corrected memory analysis (384-dim, INT8 vectors):**

| Vector Count | HNSW Graph | INT8 Vectors (in USearch) | Total Overhead | IVF Memory |
|---|---|---|---|---|
| 5,000 | ~0.7MB | ~1.8MB | **~2.5MB** | ~8MB |
| 10,000 | ~1.3MB | ~3.7MB | **~5MB** | ~9MB |
| 25,000 | ~3.4MB | ~9.2MB | **~13MB** | ~12MB |
| 50,000 | ~6.7MB | ~18.4MB | **~25MB** | ~16MB |

> **v1 correction:** Original spec estimated ~105MB for 50K vectors. Using INT8 vectors
> (384 bytes/vector) instead of f32 (1,536 bytes/vector) reduces overhead to ~25MB.

### 2.5 Architecture Impact

**Port trait decision:** Add HNSW-specific methods to existing `VectorIndex` trait rather
than creating a new trait. This keeps the hexagonal boundary clean while allowing the
coordinator to dispatch to HNSW or IVF as an implementation detail.

| File | Change |
|---|---|
| `crates/oneshim-analysis/src/adaptive_search.rs` | Add `HnswInt8` variant, `HnswIndex` wrapper field |
| `crates/oneshim-analysis/Cargo.toml` | Add `usearch = { version = "2", optional = true }` |
| `crates/oneshim-core/src/config/sections.rs` | Add `hnsw_enabled`, `hnsw_max_vectors` to `SearchConfig` |
| `crates/oneshim-core/src/ports/vector_store.rs` | **Add dimensionality validation** (see §5.1) |

**New files:**

| File | Purpose |
|---|---|
| `crates/oneshim-analysis/src/hnsw_index.rs` | `HnswIndex` wrapper — build, search, serialize/load, Send+Sync shim |

**No new crates. No schema migration.** Feature-gated behind `hnsw` cargo feature (default: off).

### 2.6 Migration/Compatibility

- When `hnsw` feature disabled: existing 3-strategy ladder unchanged.
- HNSW graph is in-memory only, rebuilt on startup from SQLite `embedding_vectors` table.
- Optional: persist serialized graph to disk for faster restart (Phase D).
- `SearchConfig.forced_strategy` gains `"hnsw"` value.
- Existing IVF tables remain untouched.

### 2.7 Effort Estimate

| Task | Estimate |
|---|---|
| `HnswIndex` wrapper (build/search/serialize + Send+Sync shim) | 2 days |
| `AdaptiveSearchCoordinator` strategy update | 1 day |
| Config + feature flag + dimensionality validation | 0.5 day |
| Tests (unit + integration) | 1 day |
| Benchmarks (brute-force vs HNSW vs IVF) | 0.5 day |
| **Total** | **5 days** |

### 2.8 Phased Rollout

| Phase | Scope |
|---|---|
| **A** | `HnswIndex` wrapper with build/search/serialize. Unit tests. Feature-gated. |
| **B** | Integration into `AdaptiveSearchCoordinator`. Strategy selection logic. |
| **C** | Benchmarks. Threshold tuning. Documentation. |
| **D** | Optional: persist serialized HNSW graph to disk for faster restart. |

---

## 3. Tauri IPC Optimization (P3)

> **Priority change:** P1 → P3. Deep analysis reveals payloads are 10-50KB (small),
> serialization is already efficient via serde_json, and the optimization ROI is low
> compared to SQLite tuning and HNSW.

### 3.1 Current State

All IPC payloads are in the 5-50KB range. Frame thumbnails are served via REST (not IPC).
Coaching overlay uses Tauri events (tiny payloads). The largest payload
(`get_dashboard_day`) is 10-50KB depending on segment count.

### 3.2 Remaining Value

**Tier 1 (conditional fetch):** Still valid but low priority. Hash-based `if_changed`
pattern avoids redundant re-serialization when dashboard is polled frequently.

**Tier 2 (pagination):** `list_overrides` returns unbounded results in 7-day window.
Add `limit`/`offset` parameters.

**Potential N+1 query:** `record_to_segment_summary()` in `dashboard.rs` is called
per-segment. Investigate whether it performs additional database queries.

### 3.3 Effort Estimate

| Task | Estimate |
|---|---|
| Tier 1: Conditional fetch (dashboard + settings) | 1.5 days |
| Tier 2: Pagination for overrides | 0.5 day |
| N+1 investigation + fix | 0.5 day |
| **Total** | **2.5 days** |

### 3.4 Phased Rollout

| Phase | Scope |
|---|---|
| **A** | Investigate N+1 query in `record_to_segment_summary()` |
| **B** | Pagination for `list_overrides` |
| **C** | Conditional fetch (if profiling shows benefit) |

---

## 4. Cross-Cutting Improvements

These gaps were discovered during the architecture audit and apply across all improvements.

### 4.1 Vector Dimensionality Validation

**Gap:** `VectorStore::store()` accepts any vector length without validating dimensions.
If the embedding model changes (e.g., 384-dim → 768-dim), stale vectors with wrong
dimensions silently corrupt the index.

**Fix:** Add dimension check in `store_quantized()`:

```rust
const EXPECTED_DIM: usize = 384;

fn store_quantized(&self, vector: &QuantizedVector) -> Result<(), CoreError> {
    if vector.int8.len() != EXPECTED_DIM {
        return Err(CoreError::InvalidInput(format!(
            "expected {EXPECTED_DIM}-dim vector, got {}",
            vector.int8.len()
        )));
    }
    // ... existing store logic
}
```

### 4.2 Observability Framework

**Gap:** No performance metrics collection. Cannot measure improvement impact.

**Recommendation:** Add `tracing::instrument` spans around key operations:

| Operation | Target Latency | Current Instrumentation |
|---|---|---|
| Vector brute-force search | <5ms | None |
| IVF search | <10ms | `tracing::debug!` only |
| FTS5 search | <50ms | None |
| HNSW search (future) | <1ms | N/A |
| Dashboard digest generation | <100ms | None |
| IVF index build | <10s | `tracing::info!` |

Add `#[instrument(skip(self))]` to async search/build methods before implementing
improvements so we can measure before/after.

### 4.3 GDPR `delete_all_data()` Verification

**Gap flagged by audit:** The audit found potential missing tables in `delete_all_data()`.
Previous session added 35 tables — needs verification against current V17 schema to
confirm completeness. All 4 new V17 tables (`coaching_events`, `regime_goals`,
`coaching_effectiveness`) were explicitly added during the 2026-03-19/20/21 session.

**Action:** Run a verification query at test time:

```sql
SELECT name FROM sqlite_master WHERE type='table'
  AND name NOT IN (/* list of tables covered by delete_all_data */)
  AND name NOT LIKE 'sqlite_%'
  AND name NOT LIKE 'search_fts%';
```

---

## Summary

| # | Improvement | Priority | Effort | New Crate | New Port | Schema |
|---|---|---|---|---|---|---|
| 1 | SQLite Performance Tuning | **P1** | 4 days | No | No | No |
| 2 | USearch HNSW Vector Index | **P1** | 5 days | No | No | No |
| 3 | Tauri IPC Optimization | **P3** | 2.5 days | No | No | No |
| 4 | Cross-cutting (validation, observability) | **P1** | 1.5 days | No | No | No |
| | **Total** | | **13 days** | | | |

### Deferred

| Item | Reason | Reference |
|---|---|---|
| Audio/STT (`oneshim-stt`) | 고객 요구사항 기반 착수, 프라이버시 리스크 높음, +350MB 의존성 | `deferred/audio-stt-research.md` |
| MCP Server | 보안 우려 + Skills 트렌드 | — |

### Dependency Graph

```
Cross-cutting (§4, independent, start immediately)

SQLite Tuning (§1, independent, start immediately)
    │
    ▼
USearch HNSW (§2, benefits from tuned SQLite)

Tauri IPC (§3, independent, low priority)
```

### Recommended Execution Order

1. **Cross-cutting** (§4) — Observability + dimensionality validation (1.5 days)
2. **SQLite Tuning** (§1) — Quick wins, foundation improvement (4 days)
3. **USearch HNSW** (§2) — Search quality on tuned foundation (5 days)
4. **Tauri IPC** (§3) — Only if profiling justifies (2.5 days)

### Research Sources

- USearch: [GitHub](https://github.com/unum-cloud/USearch), issues [#482](https://github.com/unum-cloud/usearch/issues/482), [#389](https://github.com/unum-cloud/usearch/issues/389)
- hnswlib-rs: [GitHub](https://github.com/jean-pierreBoth/hnswlib-rs)
- sqlite-vec: [GitHub](https://github.com/asg017/sqlite-vec), ANN [tracking #25](https://github.com/asg017/sqlite-vec/issues/25)
- SQLite: [PRAGMA docs](https://sqlite.org/pragma.html), [FTS5](https://www.sqlite.org/fts5.html), [WAL](https://sqlite.org/wal.html), [mmap safety](https://sqlite.org/mmap.html)
