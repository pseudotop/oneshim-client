# Cross-Cutting Improvements Design Spec

**Date:** 2026-03-21
**Priority:** P1
**Effort:** 1.5 days
**Status:** Proposed
**Impact:** No new crates, no new ports, no schema migration

---

## 1. Vector Dimensionality Validation

**Gap:** `VectorStore::store()` and `store_quantized()` accept any vector length without dimension check. Model change (384→768) silently corrupts index.

**Location:** `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs`

**Fix:**

```rust
const EXPECTED_DIM: usize = 384;

fn store_quantized(&self, vector: &QuantizedVector) -> Result<(), CoreError> {
    if vector.int8.len() != EXPECTED_DIM {
        return Err(CoreError::InvalidInput(format!(
            "expected {EXPECTED_DIM}-dim vector, got {}",
            vector.int8.len()
        )));
    }
    // ... existing logic
}
```

---

## 2. Observability Framework

**Gap:** No performance metrics. Cannot measure improvement impact.

**Add `#[instrument(skip(self))]`** to key operations:

| Operation | File | Target Latency |
|---|---|---|
| Vector brute-force search | `vector_store_impl/trait_impl.rs` | <5ms |
| IVF search | `vector_index_impl/search.rs` | <10ms |
| FTS5 search | `fts_search_impl.rs` | <50ms |
| Dashboard digest generation | `timeline_service.rs` | <100ms |
| IVF index build | `vector_index_impl/build.rs` | <10s |
| Segment summarization | `segment_summarizer.rs` | <100ms |

---

## 3. GDPR `delete_all_data()` Verification

**Deep dive finding:** Agent found only 5-7 tables in `delete_all_data()` (`maintenance.rs:357-395`), but previous session claimed 35 tables. **Discrepancy needs verification.**

**Action:** Add automated test:

```rust
#[test]
fn delete_all_data_covers_all_tables() {
    let conn = Connection::open_in_memory().unwrap();
    run_migrations(&conn).unwrap();
    // Insert sample data into all tables
    delete_all_data(&conn).unwrap();
    // Verify all user-data tables are empty
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != 'schema_version'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    for table in &tables {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM [{table}]"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "Table {table} not cleared by delete_all_data()");
    }
}
```

---

## 4. Modified Files

| File | Change |
|---|---|
| `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs` | Dimensionality validation |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/search.rs` | `#[instrument]` |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/build.rs` | `#[instrument]` |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | `#[instrument]` |
| `crates/oneshim-web/src/services/timeline_service.rs` | `#[instrument]` |
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | GDPR fix (if gaps confirmed) |
| `src-tauri/tests/` or `crates/oneshim-storage/tests/` | GDPR verification test |

## 5. Effort

| Task | Estimate |
|---|---|
| Dimensionality validation | 0.25 day |
| Observability instrumentation (6+ functions) | 0.5 day |
| GDPR verification test + fix | 0.5 day |
| Integration tests | 0.25 day |
| **Total** | **1.5 days** |

## 6. Phased Rollout

| Phase | Scope |
|---|---|
| **A** | Dimensionality validation + GDPR verification test |
| **B** | Observability instrumentation |
