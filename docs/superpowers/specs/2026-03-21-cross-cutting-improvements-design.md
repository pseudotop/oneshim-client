# Cross-Cutting Improvements Design Spec

**Date:** 2026-03-21
**Priority:** P1
**Effort:** 1.5 days
**Status:** Proposed
**Impact:** No new crates, no new ports, no schema migration

---

## 1. Vector Dimensionality Validation

**Gap:** `VectorStore::store()` and `store_quantized()` accept any vector length without
dimension check. `ScalarQuantizer::quantize()` (`quantization.rs:25-65`) rejects only
empty/NaN/Inf — not dimension mismatches.

**Code-level detail (deep dive):**
- `quantize()` accepts any `&[f32]` length
- `store_quantized()` in `trait_impl.rs:242-290` passes vectors through without validation
- Model dimensions hardcoded in `oneshim-embedding/src/lib.rs:155-195`:
  AllMiniLML6V2Q = 384, BGEBaseENV15Q = 768
- **Risk:** Switching models silently corrupts similarity search

**Fix:** Add dimension check in `store_quantized()` at the storage adapter level:

```rust
const EXPECTED_DIM: usize = 384;

fn store_quantized(&self, vector_f32: Vec<f32>, vector_int8: &QuantizedVector, ...) -> Result<(), CoreError> {
    if vector_int8.int8.len() != EXPECTED_DIM {
        return Err(CoreError::InvalidInput(format!(
            "expected {EXPECTED_DIM}-dim vector, got {}", vector_int8.int8.len()
        )));
    }
    // ... existing logic
}
```

---

## 2. Observability Framework

**Gap confirmed (deep dive):** Zero `#[instrument]` attributes in entire `oneshim-analysis` crate.

**Specific gaps found:**
- `adaptive_search.rs` — `search()`, `determine_strategy()`, `refresh_count()`: only `debug!` macro
- `hybrid_search_service.rs` — `hybrid_search()`, `vector_search()`, `keyword_search()`: no timing, silent `unwrap_or_default()` on errors
- `embedding_pipeline.rs` — `process_content_activities()`, `process_llm_summary()`: no timing
- `vector_retriever.rs` — `search()`: no timing
- All storage operations: no `#[instrument]`

**Add `#[instrument(skip(self))]`** to:

| Operation | File | Target |
|---|---|---|
| `search()` | `adaptive_search.rs:120` | <5ms (brute), <1ms (HNSW) |
| `hybrid_search()` | `hybrid_search_service.rs:74` | <50ms |
| `search_quantized()` | `vector_store_impl/trait_impl.rs:292` | <5ms |
| `search_ivf()` | `vector_index_impl/search.rs` | <10ms |
| `search_fts()` | `fts_search_impl.rs:10` | <50ms |
| `build_ivf_index()` | `vector_index_impl/build.rs` | <10s |
| `process_content_activities()` | `embedding_pipeline.rs:67` | <100ms |
| `build_timeline_response()` | `timeline_service.rs:23` | <100ms |

**Also fix:** Silent error swallowing in `hybrid_search_service.rs:120-125`:
```rust
// Current: silently returns empty on error
let vector_results = vector_results.unwrap_or_default();
// Should: log the error before defaulting
let vector_results = vector_results.unwrap_or_else(|e| { warn!("vector search failed: {e}"); vec![] });
```

---

## 3. GDPR `delete_all_data()` — VERIFIED COMPLETE

**Verification result:** Manual code review confirms `maintenance.rs:357-421` deletes
from **34 tables** covering V1-V17. Both research agents incorrectly reported only 7
tables (they read only the first portion of the function).

**Coverage:**
- V1-V7: events, frames, system_metrics, system_metrics_hourly, process_snapshots, idle_periods, session_stats, work_sessions, interruptions, focus_metrics, suggestions, local_suggestions, tags, frame_tags (14)
- V8-V11: activity_segments, calibration_log, daily_digests, weekly_digests, embedding_vectors, regime_overrides, regimes, trigger_params_snapshots, search_fts (9)
- V12-V14: vector_binary_codes, vector_index_meta, ivf_centroids, ivf_assignments, gui_interactions, device_identity, sync_peers (7)
- V15-V16: lan_peer_pins (1)
- V17: coaching_events, regime_goals, coaching_effectiveness (3)

**Only `schema_version` excluded** (correct — schema metadata, not user data).

**Action:** Add automated regression test to prevent future gaps:

```rust
#[test]
fn delete_all_data_covers_all_tables() {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    delete_all_data(&conn).unwrap();
    let uncovered: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != 'schema_version'")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|name| {
            let count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM [{name}]"), [], |r| r.get(0)).unwrap_or(0);
            count > 0
        })
        .collect();
    assert!(uncovered.is_empty(), "Tables not cleared: {uncovered:?}");
}
```

---

## 4. Unbounded Collection Risks (deep dive finding)

**Additional gap discovered:**

| Location | Issue |
|---|---|
| `edge_intelligence.rs:533,607,655,700,757` | `get_work_sessions()` etc. return full result sets without LIMIT |
| `integration_state_store.rs:55,60` | `outbox`, `audit_records` Vecs grow unbounded |
| `frame_storage.rs:198,300` | Directory listing loads all file paths into Vec |
| `maintenance.rs:428-432` | Export queries have no pagination |

**Recommendation:** Add LIMIT to all unbounded queries. Low effort, high safety.

---

## 5. Modified Files

| File | Change |
|---|---|
| `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs` | Dimensionality validation |
| `crates/oneshim-analysis/src/adaptive_search.rs` | `#[instrument]` on `search()` |
| `crates/oneshim-analysis/src/hybrid_search_service.rs` | `#[instrument]` + error logging fix |
| `crates/oneshim-analysis/src/embedding_pipeline.rs` | `#[instrument]` |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/search.rs` | `#[instrument]` |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/build.rs` | `#[instrument]` |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | `#[instrument]` |
| `crates/oneshim-web/src/services/timeline_service.rs` | `#[instrument]` |
| `crates/oneshim-storage/tests/` | GDPR regression test |

## 6. Effort

| Task | Estimate |
|---|---|
| Dimensionality validation | 0.25 day |
| Observability instrumentation (8+ functions) | 0.5 day |
| Error logging fix (hybrid search) | 0.25 day |
| GDPR regression test | 0.25 day |
| Integration tests | 0.25 day |
| **Total** | **1.5 days** |

## 7. Phased Rollout

| Phase | Scope |
|---|---|
| **A** | Dimensionality validation + GDPR regression test |
| **B** | Observability instrumentation + error logging fix |
