# Cross-Cutting Improvements — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden GDPR compliance (transactional deletion + frame file cleanup), validate vector similarity dimensions, and add persistent observability (tracing instrumentation + file-based logging with rotation).

**Architecture:** Cross-cutting — touches `oneshim-core` (error types, quantization), `oneshim-storage` (SQLite transaction model, frame file deletion), `oneshim-web` (caller propagation), `src-tauri` (tracing setup, scheduler instrumentation, GDPR regression tests).

**Tech Stack:** Rust, rusqlite (`Connection::transaction()`), `tracing` / `tracing-subscriber` / `tracing-appender`, tokio.

**Spec:** `docs/superpowers/specs/2026-03-21-cross-cutting-improvements-design.md`

**Prerequisites:** `cargo check --workspace` and `cargo test --workspace` pass.

---

## File Map

| File | Action | Description |
|------|--------|-------------|
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | Modify | Wrap `delete_all_data` in `Connection::transaction()`, change return type to `Result<(), CoreError>` |
| `crates/oneshim-core/src/models/storage_records.rs` | Modify | Keep `DeletedRangeCounts` (used by `delete_data_in_range`); no removal needed |
| `crates/oneshim-core/src/ports/web_storage.rs` | Modify | Change `delete_all_data` return type to `Result<(), CoreError>` |
| `crates/oneshim-storage/src/sqlite/web_storage_impl.rs` | Modify | Update `delete_all_data` impl to match new signature |
| `crates/oneshim-web/src/services/data_web_service.rs` | Modify | Update `delete_all_data` caller to handle `Result<(), CoreError>` |
| `crates/oneshim-storage/src/frame_storage.rs` | Modify | Add `delete_all_files` method |
| `crates/oneshim-core/src/quantization.rs` | Modify | `cosine_similarity_int8` returns `Result<f32, CoreError>`, add `_unchecked` variant |
| `crates/oneshim-core/src/ivf_index.rs` | Modify | Propagate `Result` from `cosine_similarity_int8` at 2 call sites |
| `crates/oneshim-storage/src/sqlite/vector_store_impl/helpers.rs` | Modify | Propagate `Result` at 1 call site |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/search.rs` | Modify | Propagate `Result` at 2 call sites |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/mod.rs` | Modify | Propagate `Result` at 1 call site |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/metadata.rs` | Modify | Propagate `Result` at 1 call site |
| `Cargo.toml` (workspace root) | Modify | Add `tracing-appender` workspace dependency |
| `src-tauri/Cargo.toml` | Modify | Add `tracing-appender` dependency |
| `src-tauri/src/main.rs` | Modify | Replace console-only tracing with layered subscriber (console + file) |
| `src-tauri/src/scheduler/loops/system.rs` | Modify | Add `#[tracing::instrument]` to 3 loop functions |
| `src-tauri/src/scheduler/loops/network.rs` | Modify | Add `#[tracing::instrument]` to 2 loop functions |
| `src-tauri/src/scheduler/loops/monitor.rs` | Modify | Add `#[tracing::instrument]` to 1 loop function |
| `src-tauri/src/scheduler/loops/events.rs` | Modify | Add `#[tracing::instrument]` to 2 loop functions |
| `src-tauri/src/scheduler/loops/sync.rs` | Modify | Add `#[tracing::instrument]` to 3 loop functions |
| `src-tauri/src/scheduler/loops/intelligence.rs` | Modify | Add `#[tracing::instrument]` to 3 loop functions |
| `src-tauri/src/scheduler/gui_pipeline.rs` | Modify | Add `#[tracing::instrument]` if public entry fn exists |
| `src-tauri/tests/gdpr_regression.rs` | Create | GDPR regression test suite |

---

## Task 1: GDPR Transaction Model for `delete_all_data`

Refactor `delete_all_data` to use `Connection::transaction()` so all table deletions are atomic. Change return type from `Result<DeletedRangeCounts, CoreError>` to `Result<(), CoreError>`.

### Files

- **Modify:** `crates/oneshim-storage/src/sqlite/maintenance.rs`
- **Modify:** `crates/oneshim-core/src/ports/web_storage.rs`
- **Modify:** `crates/oneshim-storage/src/sqlite/web_storage_impl.rs`
- **Modify:** `crates/oneshim-web/src/services/data_web_service.rs`
- **Test:** `cargo test -p oneshim-storage && cargo test -p oneshim-web`

### Steps

- [ ] **1a.** In `crates/oneshim-core/src/ports/web_storage.rs`, change the `delete_all_data` return type on line 44 from `Result<DeletedRangeCounts, CoreError>` to `Result<(), CoreError>`:
  ```rust
  fn delete_all_data(&self) -> Result<(), CoreError>;
  ```
  Verify: `cargo check -p oneshim-core` fails (expected — callers not yet updated).

- [ ] **1b.** In `crates/oneshim-storage/src/sqlite/maintenance.rs`, rewrite `delete_all_data` (lines 357-430) to use `Connection::transaction()`. Replace all `let _ = conn.execute(...)` with `tx.execute(...)?.` propagation within the transaction:
  ```rust
  pub fn delete_all_data(&self) -> Result<(), CoreError> {
      let conn = self
          .conn
          .lock()
          .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

      let tx = conn.transaction().map_err(|e| {
          CoreError::Internal(format!("Failed to begin transaction: {e}"))
      })?;

      // All tables: V1-V17
      let tables = [
          "events",
          "frames",
          "system_metrics",
          "system_metrics_hourly",
          "process_snapshots",
          "idle_periods",
          "session_stats",
          "work_sessions",
          "interruptions",
          "focus_metrics",
          "suggestions",
          "local_suggestions",
          "tags",
          "frame_tags",
          "activity_segments",
          "calibration_log",
          "daily_digests",
          "weekly_digests",
          "embedding_vectors",
          "regime_overrides",
          "regimes",
          "trigger_params_snapshots",
          "search_fts",
          "vector_binary_codes",
          "vector_index_meta",
          "ivf_centroids",
          "ivf_assignments",
          "gui_interactions",
          "device_identity",
          "sync_peers",
          "lan_peer_pins",
          "coaching_events",
          "regime_goals",
          "coaching_effectiveness",
      ];

      for table in &tables {
          tx.execute(&format!("DELETE FROM {table}"), [])
              .map_err(|e| {
                  CoreError::Internal(format!("Failed to delete from {table}: {e}"))
              })?;
      }

      tx.commit().map_err(|e| {
          CoreError::Internal(format!("Failed to commit deletion transaction: {e}"))
      })?;

      Ok(())
  }
  ```
  Verify: `cargo check -p oneshim-storage` compiles.

- [ ] **1c.** In `crates/oneshim-storage/src/sqlite/web_storage_impl.rs`, update the `delete_all_data` impl on line 118 to match the new return type:
  ```rust
  fn delete_all_data(&self) -> Result<(), CoreError> {
      SqliteStorage::delete_all_data(self)
  }
  ```
  Verify: `cargo check -p oneshim-storage` compiles.

- [ ] **1d.** In `crates/oneshim-web/src/services/data_web_service.rs`, update `delete_all_data` (lines 69-99). The method no longer receives `DeletedRangeCounts`, so simplify the response:
  ```rust
  pub fn delete_all_data(&self) -> Result<DeleteResult, ApiError> {
      if let Some(ref frames_dir) = self.ctx.frames_dir {
          if frames_dir.exists() {
              if let Ok(entries) = std::fs::read_dir(frames_dir) {
                  for entry in entries.flatten() {
                      let path = entry.path();
                      if path.is_file() {
                          let _ = std::fs::remove_file(&path);
                      }
                  }
              }
          }
      }

      self.ctx
          .storage
          .delete_all_data()
          .map_err(|error| ApiError::Internal(error.to_string()))?;

      Ok(DeleteResult {
          success: true,
          events_deleted: 0,
          frames_deleted: 0,
          metrics_deleted: 0,
          process_snapshots_deleted: 0,
          idle_periods_deleted: 0,
          message: "All data was deleted".to_string(),
      })
  }
  ```
  Verify: `cargo check -p oneshim-web` compiles.

- [ ] **1e.** Run full check and tests:
  ```bash
  cargo check --workspace && cargo test -p oneshim-storage && cargo test -p oneshim-web
  ```

---

## Task 2: Frame File Deletion

Add `delete_all_files` to `FrameFileStorage` that recursively deletes all frame files from the `frames/` directory.

### Files

- **Modify:** `crates/oneshim-storage/src/frame_storage.rs`
- **Test:** `cargo test -p oneshim-storage`

### Steps

- [ ] **2a.** Write a test first. At the bottom of `crates/oneshim-storage/src/frame_storage.rs` inside `#[cfg(test)] mod tests`, add:
  ```rust
  #[tokio::test]
  async fn delete_all_files_removes_everything() {
      let (storage, temp) = create_test_storage().await;

      // Save several frames across different timestamps
      let t1 = Utc::now();
      let t2 = Utc::now() - chrono::Duration::days(1);
      storage.save_frame(t1, b"frame-a").await.unwrap();
      storage.save_frame(t1, b"frame-b").await.unwrap();
      storage.save_frame(t2, b"frame-c").await.unwrap();

      // Verify files exist
      let size_before = storage.total_size_mb().await.unwrap();
      // At least some files were written
      assert!(temp.path().join("frames").exists());

      // Delete all
      let deleted = storage.delete_all_files().await.unwrap();
      assert!(deleted >= 3, "expected >= 3 files deleted, got {deleted}");

      // frames/ dir should still exist but be empty (no date subdirs)
      let dirs = list_date_dirs(&storage.frames_dir()).await.unwrap();
      assert!(dirs.is_empty(), "expected no date dirs remaining");
  }

  #[tokio::test]
  async fn delete_all_files_empty_dir_returns_zero() {
      let (storage, _temp) = create_test_storage().await;
      let deleted = storage.delete_all_files().await.unwrap();
      assert_eq!(deleted, 0);
  }
  ```
  Verify: `cargo test -p oneshim-storage -- delete_all_files` fails (method does not exist yet).

- [ ] **2b.** Implement `delete_all_files` in `FrameFileStorage` (in `crates/oneshim-storage/src/frame_storage.rs`), add it after the `enforce_storage_limit` method (before `frames_dir`):
  ```rust
  /// Delete all frame files from the frames directory.
  /// Returns the count of files deleted. Used for GDPR data purge.
  pub async fn delete_all_files(&self) -> Result<usize, CoreError> {
      let frames_dir = self.base_dir.join("frames");

      if !frames_dir.exists() {
          return Ok(0);
      }

      let mut dirs = list_date_dirs(&frames_dir).await?;
      if dirs.is_empty() {
          return Ok(0);
      }

      let mut deleted_count = 0;
      dirs.sort();

      for chunk in dirs.chunks(PARALLEL_DELETE_LIMIT) {
          let mut handles = Vec::with_capacity(chunk.len());

          for dir_name in chunk {
              let dir_path = frames_dir.join(dir_name);
              handles.push(tokio::spawn(async move {
                  let count = count_files_in_dir(&dir_path).await;
                  match fs::remove_dir_all(&dir_path).await {
                      Ok(()) => Some(count),
                      Err(e) => {
                          warn!("frame folder delete failure during GDPR purge: {e}");
                          None
                      }
                  }
              }));
          }

          for handle in handles {
              if let Ok(Some(count)) = handle.await {
                  deleted_count += count;
              }
          }
      }

      if deleted_count > 0 {
          info!("GDPR purge: deleted {deleted_count} frame files");
      }

      Ok(deleted_count)
  }
  ```
  Verify: `cargo test -p oneshim-storage -- delete_all_files` passes.

---

## Task 3: FTS5 Deletion Verification within Transaction

Add a test proving `DELETE FROM search_fts` works correctly inside a `Connection::transaction()` and that post-commit FTS queries return no results.

### Files

- **Create:** `src-tauri/tests/gdpr_regression.rs`
- **Test:** `cargo test -p oneshim-app -- gdpr`

### Steps

- [ ] **3a.** Create `src-tauri/tests/gdpr_regression.rs` with the FTS5 and transaction tests:
  ```rust
  //! GDPR regression tests — transactional deletion, FTS5 cleanup, frame file deletion.

  use oneshim_core::ports::storage::StorageService;
  use oneshim_core::ports::web_storage::WebStorage;
  use oneshim_storage::sqlite::SqliteStorage;

  /// Helper: create in-memory storage with V1-V17 schema.
  fn make_storage() -> SqliteStorage {
      SqliteStorage::open_in_memory(30).expect("in-memory sqlite")
  }

  #[tokio::test]
  async fn delete_all_data_transaction_clears_all_tables() {
      let storage = make_storage();

      // Insert test data into events
      use chrono::Utc;
      use oneshim_core::models::event::{ContextEvent, Event};

      let event = Event::Context(ContextEvent {
          app_name: "TestApp".to_string(),
          window_title: "TestWindow".to_string(),
          prev_app_name: None,
          timestamp: Utc::now(),
          ..Default::default()
      });
      storage.save_event(&event).await.unwrap();

      // Verify data exists
      let from = Utc::now() - chrono::Duration::hours(1);
      let to = Utc::now() + chrono::Duration::hours(1);
      let events_before = storage.get_events(from, to, 100).await.unwrap();
      assert!(!events_before.is_empty(), "should have events before deletion");

      // Delete all data (transactional)
      storage.delete_all_data().unwrap();

      // Verify everything is gone
      let events_after = storage.get_events(from, to, 100).await.unwrap();
      assert!(events_after.is_empty(), "events should be empty after delete_all_data");
  }

  #[tokio::test]
  async fn delete_all_data_returns_ok_on_empty_db() {
      let storage = make_storage();
      // Should succeed even with no data
      let result = storage.delete_all_data();
      assert!(result.is_ok());
  }

  #[tokio::test]
  async fn delete_all_data_fts5_search_returns_empty_after_deletion() {
      let storage = make_storage();

      // Insert an event that populates the FTS5 index
      use chrono::Utc;
      use oneshim_core::models::event::{ContextEvent, Event};

      let event = Event::Context(ContextEvent {
          app_name: "VSCode".to_string(),
          window_title: "important_file.rs".to_string(),
          prev_app_name: None,
          timestamp: Utc::now(),
          ..Default::default()
      });
      storage.save_event(&event).await.unwrap();

      // Verify search finds something before deletion
      let count_before = storage.count_search_events("%VSCode%").unwrap();
      assert!(count_before > 0, "FTS should find events before deletion");

      // Delete all data
      storage.delete_all_data().unwrap();

      // Verify FTS search returns empty
      let count_after = storage.count_search_events("%VSCode%").unwrap();
      assert_eq!(count_after, 0, "FTS search should return 0 after delete_all_data");
  }

  #[tokio::test]
  async fn delete_all_data_vector_embeddings_cleared() {
      use oneshim_core::models::embedding::{ContentType, EmbeddingMetadata, SearchFilters};
      use oneshim_core::ports::vector_store::VectorStore;
      use oneshim_core::quantization::ScalarQuantizer;

      let storage = make_storage();

      // Store a quantized vector
      let f32_vec: Vec<f32> = (0..64).map(|i| i as f32 / 64.0).collect();
      let qv = ScalarQuantizer::quantize(&f32_vec).unwrap();
      let metadata = EmbeddingMetadata {
          segment_id: "seg-001".to_string(),
          content_type: ContentType::Activity,
          content_label: Some("test label".to_string()),
          original_text: "test text".to_string(),
          model_id: "test-model".to_string(),
          timestamp: chrono::Utc::now(),
      };
      storage
          .store_quantized(f32_vec.clone(), &qv, metadata, false)
          .await
          .unwrap();

      // Verify vector exists
      let results = storage
          .search_quantized(&qv, 10, 24.0, &SearchFilters::default())
          .await
          .unwrap();
      assert!(!results.is_empty(), "should find vectors before deletion");

      // Delete all data
      storage.delete_all_data().unwrap();

      // Verify vectors are gone
      let results_after = storage
          .search_quantized(&qv, 10, 24.0, &SearchFilters::default())
          .await
          .unwrap();
      assert!(
          results_after.is_empty(),
          "vectors should be empty after delete_all_data"
      );
  }
  ```
  Verify: `cargo test -p oneshim-app -- gdpr` passes all 4 tests.

- [ ] **3b.** Run the full workspace test to confirm no regressions:
  ```bash
  cargo test --workspace
  ```

---

## Task 4: Vector Validation — `cosine_similarity_int8` Returns `Result`

Change `cosine_similarity_int8` to return `Result<f32, CoreError>` on dimension mismatch. Add an unchecked variant for hot paths.

### Files

- **Modify:** `crates/oneshim-core/src/quantization.rs`
- **Test:** `cargo test -p oneshim-core`

### Steps

- [ ] **4a.** In `crates/oneshim-core/src/quantization.rs`, add a test for the new error case. Append to `#[cfg(test)] mod tests`:
  ```rust
  #[test]
  fn cosine_similarity_dimension_mismatch_returns_error() {
      let a = ScalarQuantizer::quantize(&[1.0, 2.0, 3.0]).unwrap();
      let b = ScalarQuantizer::quantize(&[1.0, 2.0]).unwrap();
      let result = ScalarQuantizer::cosine_similarity_int8(&a, &b);
      assert!(result.is_err(), "dimension mismatch should return Err");
      let err_msg = format!("{}", result.unwrap_err());
      assert!(err_msg.contains("Dimension mismatch"), "error should mention dimension mismatch");
  }

  #[test]
  fn cosine_similarity_empty_vectors_returns_error() {
      let a = QuantizedVector { data: vec![], scale: 1.0, offset: 0.0 };
      let b = QuantizedVector { data: vec![], scale: 1.0, offset: 0.0 };
      let result = ScalarQuantizer::cosine_similarity_int8(&a, &b);
      assert!(result.is_err(), "empty vectors should return Err");
  }

  #[test]
  fn cosine_similarity_unchecked_matches_checked() {
      let v = vec![0.1, 0.5, 0.9, -0.3, 0.7];
      let qv = ScalarQuantizer::quantize(&v).unwrap();
      let checked = ScalarQuantizer::cosine_similarity_int8(&qv, &qv).unwrap();
      let unchecked = ScalarQuantizer::cosine_similarity_int8_unchecked(&qv, &qv);
      assert!((checked - unchecked).abs() < f32::EPSILON);
  }
  ```
  Verify: `cargo test -p oneshim-core -- cosine_similarity_dimension` fails (method still returns `f32`).

- [ ] **4b.** Rewrite `cosine_similarity_int8` (lines 85-111) to return `Result<f32, CoreError>` and add the `_unchecked` variant:
  ```rust
  /// Compute approximate cosine similarity between two quantized vectors
  /// using INT8 dot product (avoids full dequantization).
  ///
  /// Returns `Err(CoreError::InvalidArguments)` on dimension mismatch or empty vectors.
  pub fn cosine_similarity_int8(
      a: &QuantizedVector,
      b: &QuantizedVector,
  ) -> Result<f32, CoreError> {
      if a.data.is_empty() || b.data.is_empty() {
          return Err(CoreError::InvalidArguments(
              "cannot compute cosine similarity on empty vectors".to_string(),
          ));
      }
      if a.data.len() != b.data.len() {
          return Err(CoreError::InvalidArguments(format!(
              "Dimension mismatch: expected {}, got {}",
              a.data.len(),
              b.data.len()
          )));
      }
      Ok(Self::cosine_similarity_int8_unchecked(a, b))
  }

  /// Pre-validated cosine similarity: caller guarantees `a.data.len() == b.data.len()`
  /// and both are non-empty. For hot paths after one-time validation.
  pub fn cosine_similarity_int8_unchecked(a: &QuantizedVector, b: &QuantizedVector) -> f32 {
      // i32 accumulator: max possible value for 384 dims of i8 is
      // 384 * 127 * 127 = 6,193,152, well within i32 max (2,147,483,647).
      // Using i32 enables LLVM auto-vectorization with SIMD (SDOT on ARM, SSSE3 on x86).
      let mut dot: i32 = 0;
      let mut norm_a: i32 = 0;
      let mut norm_b: i32 = 0;

      for (va, vb) in a.data.iter().zip(b.data.iter()) {
          let a_val = *va as i32;
          let b_val = *vb as i32;
          dot += a_val * b_val;
          norm_a += a_val * a_val;
          norm_b += b_val * b_val;
      }

      let denom = ((norm_a as f64).sqrt() * (norm_b as f64).sqrt()) as f32;
      if denom < f32::EPSILON {
          0.0
      } else {
          dot as f32 / denom
      }
  }
  ```

- [ ] **4c.** Update the existing tests in `quantization.rs` that call `cosine_similarity_int8` to unwrap the Result:
  - `cosine_similarity_identical` (line 166): change `let sim = ScalarQuantizer::cosine_similarity_int8(&qv, &qv);` to `let sim = ScalarQuantizer::cosine_similarity_int8(&qv, &qv).unwrap();`
  - `cosine_similarity_different` (line 184): change `let sim = ScalarQuantizer::cosine_similarity_int8(&qa, &qb);` to `let sim = ScalarQuantizer::cosine_similarity_int8(&qa, &qb).unwrap();`

  Verify: `cargo test -p oneshim-core` passes all quantization tests.

---

## Task 5: Migrate 7 Caller Sites to Handle `Result`

Propagate the `Result<f32, CoreError>` return type from `cosine_similarity_int8` to all call sites.

### Files

- **Modify:** `crates/oneshim-core/src/ivf_index.rs` (2 sites)
- **Modify:** `crates/oneshim-storage/src/sqlite/vector_store_impl/helpers.rs` (1 site)
- **Modify:** `crates/oneshim-storage/src/sqlite/vector_index_impl/search.rs` (2 sites)
- **Modify:** `crates/oneshim-storage/src/sqlite/vector_index_impl/mod.rs` (1 site)
- **Modify:** `crates/oneshim-storage/src/sqlite/vector_index_impl/metadata.rs` (1 site)
- **Test:** `cargo check --workspace && cargo test --workspace`

### Steps

- [ ] **5a.** In `crates/oneshim-core/src/ivf_index.rs`, update `nearest_centroids` (line 290). The closure in `.map()` needs to propagate the error. Since `nearest_centroids` returns `Vec<usize>` (not `Result`), use `unwrap_or(0.0)` for centroids (dimensions are validated at index-build time, so mismatch here indicates a bug):
  ```rust
  // Line 290: inside nearest_centroids
  let sim = ScalarQuantizer::cosine_similarity_int8(&c.vector, query)
      .unwrap_or(0.0);
  ```

- [ ] **5b.** In `crates/oneshim-core/src/ivf_index.rs`, update `assign` (line 308). Same pattern:
  ```rust
  // Line 308: inside assign
  let sim = ScalarQuantizer::cosine_similarity_int8(&c.vector, vector)
      .unwrap_or(0.0);
  ```
  Verify: `cargo check -p oneshim-core` compiles.

- [ ] **5c.** In `crates/oneshim-storage/src/sqlite/vector_store_impl/helpers.rs` (line 171), the `score_and_rank` function maps rows. Use `unwrap_or(0.0)` since rows come from DB (dimensions were validated at storage time):
  ```rust
  // Line 171-174: inside score_and_rank
  let similarity = oneshim_core::quantization::ScalarQuantizer::cosine_similarity_int8(
      query_vector,
      &row_qv,
  )
  .unwrap_or(0.0);
  ```
  Verify: `cargo check -p oneshim-storage` compiles.

- [ ] **5d.** In `crates/oneshim-storage/src/sqlite/vector_index_impl/search.rs`, update the two call sites (lines 33 and 129). These are centroid similarity comparisons within a function returning `Result`:
  ```rust
  // Line 33: inside search_ivf_impl centroid probe
  let sim = ScalarQuantizer::cosine_similarity_int8(&c.vector, query_vector)
      .unwrap_or(0.0);
  ```
  ```rust
  // Line 129: inside search_ivf_binary_impl centroid probe
  let sim = ScalarQuantizer::cosine_similarity_int8(&c.vector, query_vector)
      .unwrap_or(0.0);
  ```
  Verify: `cargo check -p oneshim-storage` compiles.

- [ ] **5e.** In `crates/oneshim-storage/src/sqlite/vector_index_impl/mod.rs` (line 160), the brute-force search loop. Use `unwrap_or(0.0)`:
  ```rust
  // Line 160: inside brute force search
  let similarity = ScalarQuantizer::cosine_similarity_int8(query, &row_qv)
      .unwrap_or(0.0);
  ```
  Verify: `cargo check -p oneshim-storage` compiles.

- [ ] **5f.** In `crates/oneshim-storage/src/sqlite/vector_index_impl/metadata.rs` (line 25), the centroid assignment. Use `unwrap_or(0.0)`:
  ```rust
  // Line 25: inside assign_vector_to_cluster
  let sim = ScalarQuantizer::cosine_similarity_int8(&c.vector, vector)
      .unwrap_or(0.0);
  ```
  Verify: `cargo check -p oneshim-storage` compiles.

- [ ] **5g.** Run full workspace check and tests:
  ```bash
  cargo check --workspace && cargo test --workspace
  ```

---

## Task 6: Boundary Validation at `quantize()` and `store_quantized()`

Add dimension validation at the two entry boundaries to catch invalid vectors before they enter the system.

### Files

- **Modify:** `crates/oneshim-core/src/quantization.rs`
- **Modify:** `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs`
- **Test:** `cargo test -p oneshim-core && cargo test -p oneshim-storage`

### Steps

- [ ] **6a.** Add a test for extreme dimension vectors in `crates/oneshim-core/src/quantization.rs`:
  ```rust
  #[test]
  fn quantize_single_element_vector() {
      let v = vec![0.5];
      let qv = ScalarQuantizer::quantize(&v).unwrap();
      assert_eq!(qv.data.len(), 1);
  }

  #[test]
  fn quantize_large_dimension_vector() {
      let v: Vec<f32> = (0..1024).map(|i| (i as f32).sin()).collect();
      let qv = ScalarQuantizer::quantize(&v).unwrap();
      assert_eq!(qv.data.len(), 1024);
      // Round-trip fidelity
      let reconstructed = ScalarQuantizer::dequantize(&qv);
      assert_eq!(reconstructed.len(), 1024);
  }
  ```
  Verify: `cargo test -p oneshim-core -- quantize_single_element quantize_large_dimension` passes.

- [ ] **6b.** In `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs`, add dimension consistency check at the start of `store_quantized` (line 242). Validate that `vector_int8.data.len()` matches `vector_f32.len()` when `skip_float32` is false:
  ```rust
  async fn store_quantized(
      &self,
      vector_f32: Vec<f32>,
      vector_int8: &QuantizedVector,
      metadata: EmbeddingMetadata,
      skip_float32: bool,
  ) -> Result<(), CoreError> {
      // Boundary validation: INT8 dimension must match f32 dimension
      if !skip_float32 && vector_f32.len() != vector_int8.data.len() {
          return Err(CoreError::InvalidArguments(format!(
              "Vector dimension mismatch: f32 has {}, INT8 has {}",
              vector_f32.len(),
              vector_int8.data.len()
          )));
      }
      if vector_int8.data.is_empty() {
          return Err(CoreError::InvalidArguments(
              "cannot store empty quantized vector".to_string(),
          ));
      }
      // ... rest of existing implementation unchanged
  ```
  Verify: `cargo test -p oneshim-storage` passes.

---

## Task 7: Observability — `#[instrument]` on P0 Scheduler Loop Functions

Add `#[tracing::instrument(skip_all)]` to all 14 scheduler spawn loop functions. These are the P0 instrumentation targets.

### Files

- **Modify:** `src-tauri/src/scheduler/loops/system.rs` (3 functions)
- **Modify:** `src-tauri/src/scheduler/loops/network.rs` (2 functions)
- **Modify:** `src-tauri/src/scheduler/loops/monitor.rs` (1 function)
- **Modify:** `src-tauri/src/scheduler/loops/events.rs` (2 functions)
- **Modify:** `src-tauri/src/scheduler/loops/sync.rs` (3 functions)
- **Modify:** `src-tauri/src/scheduler/loops/intelligence.rs` (3 functions)
- **Test:** `cargo check -p oneshim-app`

### Steps

- [ ] **7a.** In `src-tauri/src/scheduler/loops/system.rs`, add `#[tracing::instrument(skip_all)]` above each of the 3 `pub(in crate::scheduler) fn` functions: `spawn_metrics_loop`, `spawn_process_loop`, `spawn_aggregation_loop`.
  Verify: `cargo check -p oneshim-app` compiles.

- [ ] **7b.** In `src-tauri/src/scheduler/loops/network.rs`, add `#[tracing::instrument(skip_all)]` above `spawn_sync_loop` and `spawn_heartbeat_loop`.
  Verify: `cargo check -p oneshim-app` compiles.

- [ ] **7c.** In `src-tauri/src/scheduler/loops/monitor.rs`, add `#[tracing::instrument(skip_all)]` above `spawn_monitor_loop`.
  Verify: `cargo check -p oneshim-app` compiles.

- [ ] **7d.** In `src-tauri/src/scheduler/loops/events.rs`, add `#[tracing::instrument(skip_all)]` above `spawn_event_snapshot_loop` and `spawn_notification_loop`.
  Verify: `cargo check -p oneshim-app` compiles.

- [ ] **7e.** In `src-tauri/src/scheduler/loops/sync.rs`, add `#[tracing::instrument(skip_all)]` above `spawn_oauth_refresh_loop`, `spawn_cross_device_sync_loop`, and `run_scheduler_loops`.
  Verify: `cargo check -p oneshim-app` compiles.

- [ ] **7f.** In `src-tauri/src/scheduler/loops/intelligence.rs`, add `#[tracing::instrument(skip_all)]` above `spawn_analysis_loop`, `spawn_focus_loop`, and `spawn_coaching_loop`.
  Verify: `cargo check -p oneshim-app` compiles.

---

## Task 8: Persistent Logging — `tracing-appender` with File Rotation and Cleanup

Add daily file rotation via `tracing-appender`, a log file cleanup function, and `WorkerGuard` lifetime management in the Tauri app.

### Files

- **Modify:** `Cargo.toml` (workspace root)
- **Modify:** `src-tauri/Cargo.toml`
- **Modify:** `src-tauri/src/main.rs`
- **Test:** `cargo check -p oneshim-app`

### Steps

- [ ] **8a.** In `Cargo.toml` (workspace root), add `tracing-appender` to the `[workspace.dependencies]` section near the existing `tracing-subscriber` entry (around line 87):
  ```toml
  tracing-appender = "0.2"
  ```
  Verify: no syntax error — `cargo check -p oneshim-core` compiles.

- [ ] **8b.** In `src-tauri/Cargo.toml`, add `tracing-appender` dependency after the `tracing-subscriber` line (around line 40):
  ```toml
  tracing-appender = { workspace = true }
  ```
  Verify: `cargo check -p oneshim-app` compiles.

- [ ] **8c.** In `src-tauri/src/main.rs`, replace the console-only tracing setup (lines 82-88) with a layered subscriber that writes to both console and a daily-rotated log file. Also add a `cleanup_old_logs` helper and store the `WorkerGuard`:
  ```rust
  use tracing_subscriber::layer::SubscriberExt;
  use tracing_subscriber::util::SubscriberInitExt;

  // ... inside main(), replace lines 82-88 with:

  let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
      EnvFilter::new("oneshim=info,oneshim_app=info,oneshim_core=info,oneshim_monitor=info,oneshim_vision=info,oneshim_storage=info,oneshim_network=info,oneshim_suggestion=info")
  });

  // Console layer (always active)
  let console_layer = tracing_subscriber::fmt::layer()
      .with_writer(std::io::stderr);

  // File layer (daily rotation, stored in platform log directory)
  let log_dir = oneshim_core::config_manager::ConfigManager::data_dir()
      .map(|d| d.join("logs"))
      .unwrap_or_else(|_| std::path::PathBuf::from("logs"));
  let file_appender = tracing_appender::rolling::daily(&log_dir, "oneshim.log");
  let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
  let file_layer = tracing_subscriber::fmt::layer()
      .with_writer(non_blocking)
      .with_ansi(false);

  tracing_subscriber::registry()
      .with(env_filter)
      .with(console_layer)
      .with(file_layer)
      .init();

  // Clean up old log files (>7 days) at startup
  cleanup_old_logs(&log_dir, 7);

  // CRITICAL: _guard must live for the entire app lifetime.
  // Box::leak ensures it is never dropped. This is intentional —
  // the guard flushes pending log writes on program exit.
  Box::leak(Box::new(_guard));
  ```

- [ ] **8d.** Add the `cleanup_old_logs` function at the bottom of `src-tauri/src/main.rs`, before the `mod` declarations or after `main()`:
  ```rust
  /// Delete log files older than `max_age_days`.
  /// Runs at startup to prevent unbounded log accumulation since
  /// `tracing-appender` daily rotation does NOT auto-delete old files.
  fn cleanup_old_logs(log_dir: &std::path::Path, max_age_days: u32) {
      use std::time::{Duration, SystemTime};

      let Ok(entries) = std::fs::read_dir(log_dir) else {
          return;
      };
      let cutoff = SystemTime::now() - Duration::from_secs(max_age_days as u64 * 86400);
      let mut deleted = 0u32;

      for entry in entries.flatten() {
          let Ok(metadata) = entry.metadata() else {
              continue;
          };
          let Ok(modified) = metadata.modified() else {
              continue;
          };
          if modified < cutoff {
              if std::fs::remove_file(entry.path()).is_ok() {
                  deleted += 1;
              }
          }
      }

      if deleted > 0 {
          // This runs after tracing init, so tracing macros work.
          tracing::info!("Cleaned up {deleted} old log files from {}", log_dir.display());
      }
  }
  ```
  Verify: `cargo check -p oneshim-app` compiles.

- [ ] **8e.** Verify the `use` imports at the top of `main.rs` include the new items. The existing `use tracing_subscriber::EnvFilter;` stays. Add (if not already present):
  ```rust
  use tracing_subscriber::layer::SubscriberExt;
  use tracing_subscriber::util::SubscriberInitExt;
  ```
  Verify: `cargo check -p oneshim-app` compiles.

- [ ] **8f.** Run full workspace check:
  ```bash
  cargo check --workspace
  ```

---

## Task 9: GDPR Regression Tests (Remaining Cases)

Add the remaining GDPR regression tests to the file created in Task 3.

### Files

- **Modify:** `src-tauri/tests/gdpr_regression.rs`
- **Test:** `cargo test -p oneshim-app -- gdpr`

### Steps

- [ ] **9a.** Add consent revocation test to `src-tauri/tests/gdpr_regression.rs`:
  ```rust
  #[tokio::test]
  async fn consent_revocation_triggers_full_deletion() {
      use oneshim_core::consent::{ConsentManager, ConsentPermissions};

      let storage = make_storage();

      // Insert data
      use chrono::Utc;
      use oneshim_core::models::event::{ContextEvent, Event};

      let event = Event::Context(ContextEvent {
          app_name: "SecretApp".to_string(),
          window_title: "Confidential".to_string(),
          prev_app_name: None,
          timestamp: Utc::now(),
          ..Default::default()
      });
      storage.save_event(&event).await.unwrap();

      // Simulate consent revocation by calling delete_all_data
      // (In production, consent revocation triggers this path)
      storage.delete_all_data().unwrap();

      // Verify complete removal
      let from = Utc::now() - chrono::Duration::hours(1);
      let to = Utc::now() + chrono::Duration::hours(1);
      let events = storage.get_events(from, to, 100).await.unwrap();
      assert!(events.is_empty());
  }
  ```
  Verify: `cargo test -p oneshim-app -- consent_revocation` passes.

- [ ] **9b.** Add multiple-deletion idempotency test:
  ```rust
  #[tokio::test]
  async fn delete_all_data_idempotent() {
      let storage = make_storage();

      // Insert data
      use chrono::Utc;
      use oneshim_core::models::event::{ContextEvent, Event};

      let event = Event::Context(ContextEvent {
          app_name: "App".to_string(),
          window_title: "Win".to_string(),
          prev_app_name: None,
          timestamp: Utc::now(),
          ..Default::default()
      });
      storage.save_event(&event).await.unwrap();

      // Delete twice — second call should not error
      storage.delete_all_data().unwrap();
      storage.delete_all_data().unwrap();
  }
  ```
  Verify: `cargo test -p oneshim-app -- delete_all_data_idempotent` passes.

- [ ] **9c.** Run all GDPR tests:
  ```bash
  cargo test -p oneshim-app -- gdpr
  ```

---

## Task 10: Final Verification

Full workspace build and test pass.

### Steps

- [ ] **10a.** Run clippy:
  ```bash
  cargo clippy --workspace
  ```

- [ ] **10b.** Run fmt check:
  ```bash
  cargo fmt --check
  ```

- [ ] **10c.** Run all tests:
  ```bash
  cargo test --workspace
  ```

- [ ] **10d.** Commit with message:
  ```
  feat: cross-cutting improvements — GDPR transactional deletion, vector validation, persistent logging
  ```

---

## Summary

| Task | What | Effort |
|------|------|--------|
| 1 | GDPR transaction model (`Connection::transaction()`) + port trait update | 1.0 day |
| 2 | Frame file deletion (`FrameFileStorage::delete_all_files`) | 0.5 day |
| 3 | FTS5 + GDPR regression tests (initial batch) | 0.5 day |
| 4 | `cosine_similarity_int8` returns `Result` + `_unchecked` variant | 0.5 day |
| 5 | Migrate 7 caller sites | 0.5 day |
| 6 | Boundary validation at `quantize()` + `store_quantized()` | 0.25 day |
| 7 | `#[instrument]` on 14 P0 scheduler functions | 0.25 day |
| 8 | `tracing-appender` file rotation + cleanup + `WorkerGuard` | 0.5 day |
| 9 | GDPR regression tests (remaining) | 0.25 day |
| 10 | Final verification | 0.25 day |
| **Total** | | **4.5 days** |
