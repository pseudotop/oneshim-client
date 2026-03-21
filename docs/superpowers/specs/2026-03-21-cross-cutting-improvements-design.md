# Cross-Cutting Improvements — Design Spec

> Created: 2026-03-21
> Status: Proposed
> Scope: All crates (observability, validation, GDPR compliance)
> Reference: ADR-001 (Rust Client Architecture Patterns)

## 1. Goal

Address cross-cutting concerns that span multiple crates: observability instrumentation, vector validation hardening, and GDPR compliance for data deletion operations.

## 2. Current State

### 2.1 Observability

- Tracing is console-only (`tracing_subscriber::fmt()` in main.rs:65-72)
- Only 7/500+ functions instrumented (1.4% coverage), all in gui_interaction crate
- Pattern to follow: `#[tracing::instrument(skip_all, fields(...))]`
- No structured logging output (JSON) for production
- No span correlation across async task boundaries

### 2.2 Vector Validation

- `QuantizedVector` struct: `data: Vec<i8>`, `scale: f32`, `offset: f32` — NO dimension field
- `cosine_similarity_int8()` returns 0.0 silently on dimension mismatch (line 86) — should return Result
- No validation at quantization or storage boundaries
- Dimension mismatches only caught at search time (if at all)

### 2.3 GDPR Data Deletion

- 12 DELETE operations use `let _ =` — silent failure risk for GDPR compliance
- No audit trail for deletion success/failure
- No transaction wrapping for multi-table deletions
- Missing tests for deletion edge cases

## 3. Architecture

### A. Observability Improvements

#### A.1 Tracing Coverage Plan

Priority instrumentation targets (by crate):

| Crate | Functions to Instrument | Priority |
|-------|------------------------|----------|
| `src-tauri/scheduler/loops` | All 13 spawn loops | P0 |
| `oneshim-network` | HTTP/gRPC/SSE client methods | P0 |
| `oneshim-storage` | All public query/insert methods | P1 |
| `oneshim-analysis` | Analyzer, pipeline, retriever | P1 |
| `oneshim-vision` | Capture, delta, processor | P2 |
| `oneshim-monitor` | System/process/activity tracking | P2 |

#### A.2 Instrumentation Pattern

```rust
#[tracing::instrument(skip_all, fields(
    user_id = %self.user_id,
    operation = "upload_batch",
    batch_size = payload.events.len(),
))]
async fn upload_batch(&self, payload: &BatchPayload) -> Result<(), CoreError> {
    // ...
}
```

- `skip_all` to avoid logging sensitive data
- `fields(...)` for structured context
- Return types with `Display` for automatic result logging

#### A.3 Production Logging

- Add `tracing-subscriber` JSON formatter behind feature flag
- Configure via `AppConfig::telemetry.log_format` (text/json)
- File rotation: `tracing-appender` with daily rotation, 7-day retention

#### Persistent Logging (round 3)

**Gap:** Tracing is console-only (`tracing_subscriber::fmt()` in main.rs:72-78).
No file output, no audit trail.

**Fix:** Add `tracing-appender` with rolling daily file:
```rust
let file_appender = tracing_appender::rolling::daily(log_dir, "oneshim.log");
let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
tracing_subscriber::fmt()
    .with_writer(non_blocking)
    .with_env_filter(...)
    .init();
```

### B. Vector Validation

#### B.1 Problem

`QuantizedVector` has no `dimensions` field. `cosine_similarity_int8()` silently returns 0.0 on dimension mismatch:

```rust
// Current (line 86 in embedding/lib.rs)
if a.len() != b.len() {
    return 0.0;  // Silent failure — should be Result
}
```

#### B.2 Fix: Return Result

```rust
pub fn cosine_similarity_int8(a: &[i8], b: &[i8]) -> Result<f32, EmbeddingError> {
    if a.len() != b.len() {
        return Err(EmbeddingError::DimensionMismatch {
            expected: a.len(),
            actual: b.len(),
        });
    }
    // ... compute similarity
}
```

#### B.3 Validation Points

Best validation points for dimension checking:

1. **`ScalarQuantizer::quantize()`** — validate input vector dimensions match model config
2. **`SqliteStorage::store_quantized()`** — validate before persisting to DB
3. **`VectorRetriever::search()`** — validate query dimensions match index dimensions

Add `dimensions: usize` field to `QuantizedVector` for self-describing vectors:

```rust
pub struct QuantizedVector {
    pub data: Vec<i8>,
    pub scale: f32,
    pub offset: f32,
    pub dimensions: usize,  // New: self-describing
}
```

### C. GDPR Compliance Hardening

#### GDPR Transaction Safety (round 3, CRITICAL)

**Gap:** `delete_all_data()` (`maintenance.rs:357-421`) executes 34 DELETE statements
with **no BEGIN/COMMIT transaction**. If one fails midway:
- Previous DELETEs already committed (auto-commit mode)
- Function returns success via `let _ =` on 27 of 34 DELETEs
- **Result:** Partial data deletion reported as success

**Fix:** Wrap in explicit transaction:
```rust
conn.execute_batch("BEGIN IMMEDIATE;")?;
// ... all 34 DELETEs ...
conn.execute_batch("COMMIT;")?;
```

If any DELETE fails, ROLLBACK ensures atomicity. Replace `let _ =` with error collection.

#### C.1 Problem

12 DELETE operations use `let _ =` pattern — silent failure:

```rust
// Current pattern (multiple locations)
let _ = self.conn.execute("DELETE FROM events WHERE user_id = ?1", params![user_id]);
// ^ If this fails, GDPR deletion is incomplete but caller doesn't know
```

#### C.2 Fix: Track Success/Failure

Recommendation: Track success/failure per table, or wrap in transaction:

```rust
pub struct DeletionReport {
    pub tables: Vec<TableDeletionResult>,
    pub all_succeeded: bool,
}

pub struct TableDeletionResult {
    pub table_name: String,
    pub rows_deleted: usize,
    pub success: bool,
    pub error: Option<String>,
}

pub fn delete_user_data(&self, user_id: &str) -> Result<DeletionReport, StorageError> {
    let tx = self.conn.transaction()?;
    let mut report = DeletionReport { tables: vec![], all_succeeded: true };

    for table in ["events", "frames", "sessions", "focus_metrics", ...] {
        match tx.execute(&format!("DELETE FROM {table} WHERE user_id = ?1"), params![user_id]) {
            Ok(count) => report.tables.push(TableDeletionResult {
                table_name: table.to_string(),
                rows_deleted: count,
                success: true,
                error: None,
            }),
            Err(e) => {
                report.all_succeeded = false;
                report.tables.push(TableDeletionResult {
                    table_name: table.to_string(),
                    rows_deleted: 0,
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    if report.all_succeeded {
        tx.commit()?;
    } else {
        tx.rollback()?;
    }
    Ok(report)
}
```

#### C.3 Test Placement

Test file: `src-tauri/tests/gdpr_regression.rs`

Test cases:
- All tables cleaned for given user_id
- Partial failure → transaction rollback → no partial deletion
- DeletionReport correctly reports per-table results
- Consent revocation triggers complete data removal
- Vector embeddings deleted alongside structured data
- Frame files on disk deleted (not just DB records)

## 4. Testing Strategy

### Observability
- Verify `tracing::instrument` spans appear in test subscriber
- Test JSON log format output structure
- Verify no sensitive data in span fields

### Vector Validation
- Test dimension mismatch returns `Err` (not silent 0.0)
- Test valid dimensions compute correct similarity
- Test `QuantizedVector` with `dimensions` field round-trip through storage
- Property test: quantize → store → load → search always preserves dimensions

### GDPR
- Test placement: `src-tauri/tests/gdpr_regression.rs`
- Full deletion flow: create data → delete → verify empty
- Partial failure: mock table error → verify rollback
- Report accuracy: verify row counts and error messages

## 5. Risks

- Adding `dimensions` field to `QuantizedVector` requires storage migration (V18)
- Changing `cosine_similarity_int8()` return type is a breaking change — update all callers
- **CoreError variant for dimension validation:** Use `CoreError::Validation { field, message }` or `CoreError::InvalidArguments(String)`. No `InvalidInput` variant exists.
- Transaction-based deletion may be slower than individual DELETEs — benchmark
- Tracing instrumentation adds small overhead per function call (~100ns)

## 6. Execution Order

1. **Vector validation** — highest impact (silent bugs), smallest scope
2. **GDPR hardening** — compliance risk, moderate scope
3. **Observability** — largest scope, can be incremental
