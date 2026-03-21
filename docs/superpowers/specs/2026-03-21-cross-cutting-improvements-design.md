# Cross-Cutting Improvements — Design Spec

> Created: 2026-03-21
> Revised: 2026-03-21 (post-review)
> Priority: P1
> Effort: 4.5 days
> Status: Proposed
> Scope: All crates (GDPR compliance, vector validation, observability)
> Reference: ADR-001 (Rust Client Architecture Patterns)

## 1. Goal

Address cross-cutting concerns that span multiple crates: GDPR compliance for data deletion operations (including frame files), vector validation hardening, and observability instrumentation.

## 2. Current State

### 2.1 GDPR Data Deletion

- 12 DELETE operations use `let _ =` — silent failure risk for GDPR compliance
- No transaction wrapping for multi-table deletions
- No audit trail for deletion success/failure
- **CRITICAL GAP**: Frame files on disk are not deleted during GDPR data purge — only DB records
- No FTS5 shadow table deletion verification
- Missing tests for deletion edge cases

### 2.2 Vector Validation

- `QuantizedVector` struct: `data: Vec<i8>`, `scale: f32`, `offset: f32` — NO dimension field
- `cosine_similarity_int8()` returns 0.0 silently on dimension mismatch (line 86) — should return Result
- No validation at quantization or storage boundaries
- Dimension mismatches only caught at search time (if at all)
- 8 caller sites depend on the current `f32` return type

### 2.3 Observability

- Tracing is console-only (`tracing_subscriber::fmt()` in main.rs:65-72)
- Only 7/500+ functions instrumented (1.4% coverage), all in gui_interaction crate
- Pattern to follow: `#[tracing::instrument(skip_all, fields(...))]`
- No structured logging output (JSON) for production
- No span correlation across async task boundaries
- `tracing-appender` daily rotation does NOT auto-delete old log files

## 3. Architecture

### A. GDPR Compliance Hardening

#### A.1 Transaction Model

Use `rusqlite::Connection::transaction()` for all-or-nothing GDPR deletion. This is idiomatic Rust — the transaction auto-rolls-back on drop if not explicitly committed.

**Do NOT use raw `execute_batch("BEGIN IMMEDIATE")`** — if a panic occurs between BEGIN and COMMIT, the Mutex is poisoned and the transaction is left open, potentially corrupting the database.

```rust
pub fn delete_user_data(&self, user_id: &str) -> Result<(), CoreError> {
    let conn = self.conn.lock();
    let tx = conn.transaction()?;

    // Delete from all tables in transaction
    let tables = [
        "events", "frames", "work_sessions", "interruptions",
        "focus_metrics", "local_suggestions", "activity_segments",
        "embedding_vectors", "regimes", "gui_interactions",
        "coaching_sessions", "coaching_events",
    ];

    for table in &tables {
        tx.execute(
            &format!("DELETE FROM {} WHERE user_id = ?1", table),
            params![user_id],
        )?;
    }

    // FTS5 shadow table deletion
    // DELETE FROM search_fts works within a transaction — SQLite handles
    // shadow table cleanup automatically.
    tx.execute(
        "DELETE FROM search_fts WHERE rowid IN (SELECT rowid FROM events WHERE user_id = ?1)",
        params![user_id],
    )?;

    tx.commit()?;  // All-or-nothing: commit only if all DELETEs succeeded
    Ok(())         // On error, tx drops and auto-rollbacks
}
```

Return type is `Result<(), CoreError>` — success means all deleted, error means full rollback. This is simpler and safer than the previous `DeletionReport` approach.

**Note**: This changes the port trait signature. `DeletedRangeCounts` return type needs updating — this is a breaking change that must be propagated to callers.

#### A.2 Frame File Deletion (CRITICAL GAP)

After the DB transaction commits successfully, delete frame files from disk:

```rust
pub fn delete_user_data_complete(&self, user_id: &str) -> Result<(), CoreError> {
    // Step 1: DB transaction (all-or-nothing)
    self.delete_user_data(user_id)?;

    // Step 2: Frame file deletion (best-effort, AFTER DB commit)
    match self.frame_storage.delete_all_files_for_user(user_id) {
        Ok(count) => info!("Deleted {} frame files for user {}", count, user_id),
        Err(e) => {
            // Log warning but do NOT fail — DB records are already gone,
            // which is the GDPR-critical part. Orphaned files are less
            // sensitive than DB records and will be cleaned by retention.
            warn!("Partial frame file deletion for user {}: {}", user_id, e);
        }
    }

    Ok(())
}
```

New method needed in `FrameStorage`:

```rust
impl FrameStorage {
    /// Delete all frame image files for a given user.
    /// Returns the count of files deleted.
    pub fn delete_all_files_for_user(&self, user_id: &str) -> Result<usize, CoreError> {
        // List frame files matching user_id pattern in frame directory
        // Delete each file, counting successes
        // Return count
    }
}
```

Order matters: DB first, then files. If file deletion partially fails, the DB is already clean (GDPR satisfied for structured data), and orphaned files will be caught by the existing retention policy.

#### A.3 FTS5 Deletion Verification

Verify that `DELETE FROM search_fts WHERE ...` works correctly within a `rusqlite::Connection::transaction()`. FTS5 uses shadow tables internally, and SQLite handles their cleanup automatically on DELETE. However, this should be explicitly tested.

Test case: Insert rows into `search_fts`, delete within transaction, commit, verify FTS search returns no results.

#### A.4 Test Placement

Test file: `src-tauri/tests/gdpr_regression.rs`

Test cases:
- All tables cleaned for given user_id (transaction success)
- Simulated table error -> transaction auto-rollback -> no partial deletion
- Frame files deleted after successful DB transaction
- Partial frame file deletion failure -> warning logged, no error returned
- FTS5 deletion within transaction -> search returns empty
- Consent revocation triggers complete data removal
- Vector embeddings deleted alongside structured data

### B. Vector Validation

#### B.1 Problem

`cosine_similarity_int8()` silently returns 0.0 on dimension mismatch:

```rust
// Current (line 86 in embedding/lib.rs)
if a.len() != b.len() {
    return 0.0;  // Silent failure — should be Result
}
```

#### B.2 Fix: Return Result with CoreError

Use `CoreError::Validation` or `CoreError::InvalidArguments` (NOT `EmbeddingError` which does not exist as a type):

```rust
pub fn cosine_similarity_int8(a: &[i8], b: &[i8]) -> Result<f32, CoreError> {
    if a.len() != b.len() {
        return Err(CoreError::Validation(format!(
            "Dimension mismatch: expected {}, got {}",
            a.len(), b.len()
        )));
    }
    // ... compute similarity
    Ok(similarity)
}
```

#### B.3 Hot-Path Optimization

Validate dimensions ONCE before any loop, then use an unchecked inner function:

```rust
pub fn cosine_similarity_int8(a: &[i8], b: &[i8]) -> Result<f32, CoreError> {
    if a.len() != b.len() {
        return Err(CoreError::Validation(format!(
            "Dimension mismatch: expected {}, got {}", a.len(), b.len()
        )));
    }
    Ok(cosine_similarity_int8_unchecked(a, b))
}

/// Pre-validated: caller guarantees a.len() == b.len()
fn cosine_similarity_int8_unchecked(a: &[i8], b: &[i8]) -> f32 {
    // ... compute similarity without bounds check
}
```

#### B.4 Validation Points (Boundary Only)

Validate at the boundary — NOT on every comparison:

1. **`ScalarQuantizer::quantize()`** — validate input vector dimensions match model config
2. **`SqliteStorage::store_quantized()`** — validate before persisting to DB

These two points catch all invalid data before it enters the system. No need for a `dimensions` field on `QuantizedVector` — it would be redundant with `data.len()` and would be a serde-breaking change.

#### B.5 Caller Sites Requiring Migration

The following 8 call sites must be updated to handle the new `Result` return type:

1. `EmbeddingService::similarity()` in `oneshim-embedding/src/lib.rs`
2. `VectorRetriever::score_and_rank()` in `oneshim-analysis/src/vector_retriever.rs`
3. `AdaptiveSearchCoordinator::brute_force_search()` in `oneshim-analysis/src/adaptive_search.rs`
4. `AdaptiveSearchCoordinator::ivf_search()` in `oneshim-analysis/src/adaptive_search.rs`
5. `EmbeddingPipeline::find_similar()` in `oneshim-analysis/src/embedding_pipeline.rs`
6. `SqliteStorage` vector query methods in `oneshim-storage/src/sqlite.rs`
7. `CoachingEngine` context similarity in `oneshim-analysis/src/coaching_engine/`
8. Test mocks in `#[cfg(test)]` modules

Most callers can propagate the `?` operator. Test mocks may need `.unwrap()` or `.expect()`.

#### B.6 NOT Adding `dimensions` Field to QuantizedVector

Adding `dimensions: usize` to `QuantizedVector` is NOT recommended:
- Redundant with `data.len()` (dimensions == data length for INT8)
- Would require storage migration (V18) for a field that adds no information
- Serde-breaking change for serialized data
- Instead: validate at `quantize()` and `store_quantized()` boundary only

### C. Observability Improvements

#### C.1 Tracing Coverage Plan

Priority instrumentation targets (by crate):

| Crate | Functions to Instrument | Priority |
|-------|------------------------|----------|
| `src-tauri/scheduler/loops` | All 13 spawn loops | P0 |
| `oneshim-network` | HTTP/gRPC/SSE client methods | P0 |
| `oneshim-storage` | All public query/insert methods | P1 |
| `oneshim-analysis` | Analyzer, pipeline, retriever | P1 |
| `oneshim-vision` | Capture, delta, processor | P2 |
| `oneshim-monitor` | System/process/activity tracking | P2 |

#### C.2 Instrumentation Pattern

```rust
#[tracing::instrument(skip_all, fields(
    operation = "upload_batch",
    batch_size = payload.events.len(),
))]
async fn upload_batch(&self, payload: &BatchPayload) -> Result<(), CoreError> {
    // ...
}
```

- `skip_all` to avoid logging sensitive data (PII risk)
- `fields(...)` for structured context — whitelist only non-sensitive fields
- **PII Warning**: Never include `user_id`, `window_title`, `app_name`, or file paths in span fields. Use `skip_all` and explicitly whitelist safe fields only.

#### C.3 Production Logging

- Add `tracing-subscriber` JSON formatter behind feature flag
- Configure via `AppConfig::telemetry.log_format` (text/json)
- File rotation: `tracing-appender` with daily rotation

#### C.4 Log File Retention (CRITICAL)

`tracing-appender` daily rotation does NOT auto-delete old log files. Add scheduled cleanup:

```rust
fn cleanup_old_logs(log_dir: &Path, max_age_days: u32) -> Result<usize, std::io::Error> {
    let cutoff = SystemTime::now() - Duration::from_secs(max_age_days as u64 * 86400);
    let mut deleted = 0;
    for entry in std::fs::read_dir(log_dir)? {
        let entry = entry?;
        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                if modified < cutoff {
                    std::fs::remove_file(entry.path())?;
                    deleted += 1;
                }
            }
        }
    }
    Ok(deleted)
}
```

Run on scheduler startup and daily thereafter. Default retention: 7 days.

#### C.5 Guard Lifetime

The `tracing-appender` `WorkerGuard` must be stored in the application lifetime scope. If dropped, log writes are silently lost.

```rust
// In Tauri setup or static scope
let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
// Store _guard in Tauri managed state or a static — do NOT let it drop
```

## 4. Testing Strategy

### GDPR
- Test placement: `src-tauri/tests/gdpr_regression.rs`
- Full deletion flow: create data -> delete -> verify all tables empty
- Transaction rollback: simulate error in middle table -> verify NO tables partially deleted
- Frame file deletion: create files -> delete_user_data_complete -> verify files gone
- Partial file failure: mock file system error -> verify warning logged, no error returned
- FTS5 deletion in transaction: verify shadow tables cleaned
- Port trait change: verify all callers compile with new return type

### Vector Validation
- Test dimension mismatch returns `Err(CoreError::Validation)` (not silent 0.0)
- Test valid dimensions compute correct similarity
- Test boundary validation at `quantize()` catches bad input
- Test boundary validation at `store_quantized()` catches bad input
- Test hot-path `cosine_similarity_int8_unchecked` matches checked version

### Observability
- Verify `tracing::instrument` spans appear in test subscriber
- Test JSON log format output structure
- Verify no sensitive data (PII) in span fields
- Test log file cleanup deletes files older than 7 days
- Test WorkerGuard lifetime (logs still written after long runtime)

## 5. Effort (4.5 days)

| Task | Days |
|------|------|
| A. GDPR transaction model + port trait change | 1.0 |
| A. Frame file deletion (FrameStorage method + integration) | 0.75 |
| A. FTS5 deletion verification + GDPR regression tests | 0.5 |
| B. Vector validation (cosine_similarity return type + 8 callers) | 1.0 |
| B. Boundary validation at quantize() + store_quantized() | 0.25 |
| C. Observability instrumentation (P0 + P1 crates) | 0.5 |
| C. Production logging + log file retention cleanup | 0.25 |
| C. Guard lifetime + PII audit of span fields | 0.25 |
| **Total** | **4.5** |

## 6. Phased Rollout

### Phase A (Day 1-2.5): GDPR Transaction Fix + Frame File Deletion

1. Refactor `delete_user_data()` to use `Connection::transaction()` (all-or-nothing)
2. Update port trait return type (`DeletedRangeCounts` -> `Result<(), CoreError>`)
3. Propagate port trait change to all callers
4. Implement `FrameStorage::delete_all_files_for_user()`
5. Wire `delete_user_data_complete()` (DB first, then files)
6. Write GDPR regression tests (transaction rollback, frame file deletion, FTS5)

### Phase B (Day 2.5-3.5): Vector Dimensionality Validation

1. Change `cosine_similarity_int8()` return type to `Result<f32, CoreError>`
2. Add `cosine_similarity_int8_unchecked()` for hot paths
3. Add boundary validation at `quantize()` and `store_quantized()`
4. Migrate 8 caller sites to handle `Result`
5. Write validation unit tests

### Phase C (Day 3.5-4.5): Observability Instrumentation + Persistent Logging

1. Add `#[tracing::instrument(skip_all, fields(...))]` to P0 functions
2. Add JSON log formatter behind feature flag
3. Add `tracing-appender` file rotation + 7-day cleanup
4. Store `WorkerGuard` in Tauri managed state
5. Audit all span fields for PII leakage

## 7. Risks

| Risk | Mitigation |
|------|------------|
| Port trait change (`DeletedRangeCounts`) is breaking | Phase A includes caller propagation; grep for all usages |
| FTS5 DELETE within transaction may have edge cases | Explicit test in GDPR regression suite |
| Frame file deletion partially fails | Best-effort after DB commit; retention policy catches orphans |
| Changing `cosine_similarity_int8()` return type breaks 8 callers | Mechanical migration; most callers just add `?` |
| PII in tracing span fields | `skip_all` default + explicit whitelist of safe fields only |
| `tracing-appender` guard dropped prematurely | Store in Tauri managed state or `Box::leak` for static lifetime |
| Transaction-based deletion may be slower than individual DELETEs | Negligible for GDPR (infrequent operation, correctness > speed) |
| Tracing instrumentation adds small overhead per function call (~100ns) | Acceptable; only instrument P0/P1 functions initially |

## 8. Execution Order

1. **GDPR hardening** — compliance risk, transaction correctness is critical
2. **Vector validation** — silent bugs, well-scoped change
3. **Observability** — largest scope, can be incremental
