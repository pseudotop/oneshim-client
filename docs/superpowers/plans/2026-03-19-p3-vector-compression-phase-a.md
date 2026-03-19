# P3 Vector Compression Phase A: INT8 Scalar Quantization — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire INT8 scalar quantization into the existing embedding vector pipeline for 4x storage reduction and ~3x search speedup, with zero recall regression at 50K vectors.

**Architecture:** `ScalarQuantizer` (already in `oneshim-core/src/quantization.rs`) performs f32-to-i8 conversion. `VectorStore` trait gets 3 new default methods (`store_quantized`, `search_quantized`, `backfill_quantized`). `SqliteVectorStore` implements them using the V14 columns (`vector_int8`, `quant_scale`, `quant_offset` — already migrated). `EmbeddingPipeline` and `VectorRetriever` branch on `quantization_enabled` config flag.

**Tech Stack:** Rust, rusqlite, tokio::spawn_blocking, serde

**Spec:** `docs/superpowers/specs/2026-03-19-p3-vector-compression-embedding-optimization-design.md`

---

## What is already done (DO NOT re-implement)

| Component | File | Status |
|-----------|------|--------|
| `ScalarQuantizer` + `QuantizedVector` | `crates/oneshim-core/src/quantization.rs` | Done (7 tests) |
| V14 migration (`vector_int8`, `quant_scale`, `quant_offset` columns) | `crates/oneshim-storage/src/migration.rs` | Done (CURRENT_VERSION = 14) |
| `quantization` module exported from `oneshim-core/src/lib.rs` | `crates/oneshim-core/src/lib.rs` | Done |

---

## File Map

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/config/sections/analysis.rs` | Add `quantization_enabled: bool` to `EmbeddingConfig` |
| `crates/oneshim-core/src/ports/vector_store.rs` | Fix doc comment + add 3 default methods (store_quantized, search_quantized, backfill_quantized) |
| `crates/oneshim-storage/src/sqlite/vector_store_impl.rs` | Implement store_quantized, search_quantized, backfill_quantized |
| `crates/oneshim-analysis/src/embedding_pipeline.rs` | Branch on quantization_enabled: quantize on store |
| `crates/oneshim-analysis/src/vector_retriever.rs` | Use search_quantized when quantization_enabled |

### No new files

All changes fit into existing modules. No new crates, no new cross-crate dependencies.

---

## Task 1: Add `quantization_enabled` to `EmbeddingConfig`

**File:** `crates/oneshim-core/src/config/sections/analysis.rs`

- [ ] Add field to `EmbeddingConfig` struct, after the `digest_day` field:

```rust
    /// Enable INT8 scalar quantization for 4x storage reduction.
    /// When true, new vectors are stored in both f32 and INT8 formats,
    /// and search uses INT8 cosine similarity.
    #[serde(default)]
    pub quantization_enabled: bool,
```

- [ ] Add initialization in `Default` impl for `EmbeddingConfig`:

```rust
            quantization_enabled: false,
```

- [ ] Run: `cargo test -p oneshim-core`
- [ ] Verify existing config serde roundtrip tests still pass (the `#[serde(default)]` ensures backward compatibility with existing JSON configs missing this field).
- [ ] Commit: `feat(core): add quantization_enabled flag to EmbeddingConfig`

---

## Task 2: Fix VectorStore doc comment

**File:** `crates/oneshim-core/src/ports/vector_store.rs`

- [ ] Replace the trait doc comment. Change:

```rust
/// Port for storing and searching embedding vectors.
/// Primary adapter: sqlite-vec backed implementation in oneshim-storage.
```

to:

```rust
/// Port for storing and searching embedding vectors.
/// Primary adapter: brute-force cosine similarity implementation in oneshim-storage.
```

- [ ] Run: `cargo check -p oneshim-core`
- [ ] Commit: `fix(core): correct VectorStore doc comment (brute-force, not sqlite-vec)`

---

## Task 3: Add 3 default methods to VectorStore trait

**File:** `crates/oneshim-core/src/ports/vector_store.rs`

- [ ] Add import for `QuantizedVector` at the top of the file:

```rust
use crate::quantization::QuantizedVector;
```

- [ ] Add 3 default methods inside the `VectorStore` trait, after the existing `update_vector` method:

```rust
    /// Store a pre-quantized INT8 vector alongside its float32 original.
    async fn store_quantized(
        &self,
        _vector_f32: Vec<f32>,
        _vector_int8: &QuantizedVector,
        _metadata: EmbeddingMetadata,
    ) -> Result<(), CoreError> {
        Err(CoreError::Internal("store_quantized not implemented".into()))
    }

    /// Search using INT8 quantized cosine similarity (faster, approximate).
    /// Accepts SearchFilters for parity with search_filtered.
    async fn search_quantized(
        &self,
        _query_vector: &QuantizedVector,
        _limit: usize,
        _time_decay_hours: f32,
        _filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, CoreError> {
        Err(CoreError::Internal("search_quantized not implemented".into()))
    }

    /// Backfill INT8 quantization for existing float32-only vectors.
    /// Processes rows WHERE vector_int8 IS NULL LIMIT batch_size.
    /// Returns the number of rows backfilled.
    async fn backfill_quantized(&self, _batch_size: usize) -> Result<u64, CoreError> {
        Err(CoreError::Internal("backfill_quantized not implemented".into()))
    }
```

- [ ] Run: `cargo check -p oneshim-core`
- [ ] Verify the 3 existing mock `VectorStore` implementations (in `embedding_pipeline.rs`, `vector_retriever.rs`, `hybrid_search_service.rs`) still compile without changes — the default impls ensure backward compatibility.
- [ ] Run: `cargo test -p oneshim-core -p oneshim-analysis`
- [ ] Commit: `feat(core): add store_quantized, search_quantized, backfill_quantized to VectorStore trait`

---

## Task 4: Implement `store_quantized` in `SqliteVectorStore`

**File:** `crates/oneshim-storage/src/sqlite/vector_store_impl.rs`

- [ ] Add import at the top:

```rust
use oneshim_core::quantization::QuantizedVector;
```

- [ ] Add a helper function after the existing `bytes_to_f32_vec` function:

```rust
/// Convert a slice of i8 values to a byte vector (for SQLite BLOB storage).
fn i8_vec_to_bytes(v: &[i8]) -> Vec<u8> {
    v.iter().map(|&b| b as u8).collect()
}

/// Convert a byte slice back to a Vec<i8>.
fn bytes_to_i8_vec(b: &[u8]) -> Vec<i8> {
    b.iter().map(|&b| b as i8).collect()
}
```

- [ ] Add `store_quantized` method to the `impl VectorStore for SqliteVectorStore` block, after the existing `update_vector` method:

```rust
    async fn store_quantized(
        &self,
        vector_f32: Vec<f32>,
        vector_int8: &QuantizedVector,
        metadata: EmbeddingMetadata,
    ) -> Result<(), CoreError> {
        let f32_blob = f32_vec_to_bytes(&vector_f32);
        let int8_blob = i8_vec_to_bytes(&vector_int8.data);
        let scale = vector_int8.scale;
        let offset = vector_int8.offset;
        let content_type_str = content_type_to_str(&metadata.content_type).to_string();
        let timestamp_str = metadata.timestamp.to_rfc3339();

        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO embedding_vectors (segment_id, content_type, content_label, original_text, vector, model_id, timestamp, vector_int8, quant_scale, quant_offset)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    metadata.segment_id,
                    content_type_str,
                    metadata.content_label,
                    metadata.original_text,
                    f32_blob,
                    metadata.model_id,
                    timestamp_str,
                    int8_blob,
                    scale,
                    offset,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("Failed to store quantized vector: {e}")))?;

            debug!(
                "Stored quantized vector for segment {} (type={})",
                metadata.segment_id, content_type_str
            );
            Ok(())
        })
        .await
    }
```

- [ ] Add test `store_quantized_roundtrip` in the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn store_quantized_roundtrip() {
        use oneshim_core::quantization::ScalarQuantizer;

        let conn = setup_db();
        let store = SqliteVectorStore::new(conn.clone());

        let vector = vec![0.1, 0.5, 0.9, -0.3, 0.7];
        let quantized = ScalarQuantizer::quantize(&vector).unwrap();

        let meta = EmbeddingMetadata {
            segment_id: "seg-q001".to_string(),
            content_type: EmbeddingContentType::ContentActivity,
            content_label: Some("VSCode: test.rs".to_string()),
            timestamp: Utc::now(),
            original_text: "VSCode: test.rs".to_string(),
            model_id: "test-model".to_string(),
        };

        store
            .store_quantized(vector.clone(), &quantized, meta)
            .await
            .unwrap();

        // Verify both f32 and INT8 columns are populated
        let guard = conn.lock().unwrap();
        let (has_f32, has_int8): (bool, bool) = guard
            .query_row(
                "SELECT vector IS NOT NULL, vector_int8 IS NOT NULL FROM embedding_vectors WHERE segment_id = 'seg-q001'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(has_f32);
        assert!(has_int8);
    }
```

- [ ] Run: `cargo test -p oneshim-storage`
- [ ] Commit: `feat(storage): implement store_quantized in SqliteVectorStore`

---

## Task 5: Implement `search_quantized` in `SqliteVectorStore`

**File:** `crates/oneshim-storage/src/sqlite/vector_store_impl.rs`

- [ ] Add a new struct for INT8 vector rows, after the existing `VectorRow` struct:

```rust
/// Row fetched for INT8 brute-force search.
struct QuantizedVectorRow {
    segment_id: String,
    content_type: String,
    content_label: Option<String>,
    original_text: String,
    vector_int8: Vec<i8>,
    quant_scale: f32,
    quant_offset: f32,
    timestamp: DateTime<Utc>,
}
```

- [ ] Add a brute-force INT8 search helper, after the existing `brute_force_search` function:

```rust
/// Execute brute-force search on INT8 quantized rows.
fn brute_force_search_quantized(
    rows: Vec<QuantizedVectorRow>,
    query_vector: &QuantizedVector,
    limit: usize,
    time_decay_hours: f32,
) -> Vec<SearchResult> {
    let now = Utc::now();
    let mut scored: Vec<SearchResult> = rows
        .into_iter()
        .map(|row| {
            let row_qv = QuantizedVector {
                data: row.vector_int8,
                scale: row.quant_scale,
                offset: row.quant_offset,
            };
            let similarity =
                oneshim_core::quantization::ScalarQuantizer::cosine_similarity_int8(
                    query_vector, &row_qv,
                );
            let age_hours = (now - row.timestamp).num_seconds().max(0) as f32 / 3600.0;
            let time_decay = if time_decay_hours > 0.0 {
                (-age_hours / time_decay_hours).exp()
            } else {
                1.0
            };
            let score = similarity * time_decay;
            SearchResult {
                segment_id: row.segment_id,
                content_type: parse_content_type(&row.content_type),
                content_label: row.content_label,
                score,
                similarity,
                time_decay,
                timestamp: row.timestamp,
                original_text: row.original_text,
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    scored
}
```

- [ ] Add a row mapper for quantized rows, after the existing `map_vector_row` function:

```rust
/// Map a SQLite row (segment_id, content_type, content_label, original_text,
/// vector_int8, quant_scale, quant_offset, timestamp at positions 0..7)
/// to a QuantizedVectorRow.
fn map_quantized_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QuantizedVectorRow> {
    let ts_str: String = row.get(7)?;
    let timestamp = DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let blob: Vec<u8> = row.get(4)?;
    Ok(QuantizedVectorRow {
        segment_id: row.get(0)?,
        content_type: row.get(1)?,
        content_label: row.get(2)?,
        original_text: row.get(3)?,
        vector_int8: bytes_to_i8_vec(&blob),
        quant_scale: row.get(5)?,
        quant_offset: row.get(6)?,
        timestamp,
    })
}
```

- [ ] Add `search_quantized` method to the `impl VectorStore for SqliteVectorStore` block:

```rust
    async fn search_quantized(
        &self,
        query_vector: &QuantizedVector,
        limit: usize,
        time_decay_hours: f32,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, CoreError> {
        let qv = query_vector.clone();
        let filters = filters.clone();

        self.with_conn(move |conn| {
            let mut conditions = vec![
                "is_stale = 0".to_string(),
                "vector_int8 IS NOT NULL".to_string(),
            ];
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(ref after) = filters.after {
                conditions.push(format!("timestamp >= ?{}", param_values.len() + 1));
                param_values.push(Box::new(after.to_rfc3339()));
            }
            if let Some(ref before) = filters.before {
                conditions.push(format!("timestamp <= ?{}", param_values.len() + 1));
                param_values.push(Box::new(before.to_rfc3339()));
            }
            if let Some(ref content_types) = filters.content_types {
                if !content_types.is_empty() {
                    let placeholders: Vec<String> = content_types
                        .iter()
                        .map(|_| {
                            let idx = param_values.len() + 1;
                            format!("?{idx}")
                        })
                        .collect();
                    conditions.push(format!("content_type IN ({})", placeholders.join(", ")));
                    for ct in content_types {
                        param_values.push(Box::new(content_type_to_str(ct).to_string()));
                    }
                }
            }

            let where_clause = conditions.join(" AND ");
            let sql = format!(
                "SELECT segment_id, content_type, content_label, original_text, vector_int8, quant_scale, quant_offset, timestamp
                 FROM embedding_vectors
                 WHERE {where_clause}"
            );

            let mut stmt = conn.prepare(&sql).map_err(|e| {
                CoreError::Internal(format!("Failed to prepare quantized search: {e}"))
            })?;

            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let rows: Vec<QuantizedVectorRow> = stmt
                .query_map(params_ref.as_slice(), map_quantized_row)
                .map_err(|e| CoreError::Internal(format!("Failed to query quantized vectors: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(brute_force_search_quantized(rows, &qv, limit, time_decay_hours))
        })
        .await
    }
```

- [ ] Add test `search_quantized_finds_similar`:

```rust
    #[tokio::test]
    async fn search_quantized_finds_similar() {
        use oneshim_core::quantization::ScalarQuantizer;

        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        let now = Utc::now();

        // Store two quantized vectors: one similar, one different
        let v_close = vec![1.0, 0.1, 0.0, 0.0, 0.0];
        let v_far = vec![0.0, 0.0, 0.0, 0.1, 1.0];
        let q_close = ScalarQuantizer::quantize(&v_close).unwrap();
        let q_far = ScalarQuantizer::quantize(&v_far).unwrap();

        store
            .store_quantized(
                v_close,
                &q_close,
                EmbeddingMetadata {
                    segment_id: "close".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("close".to_string()),
                    timestamp: now,
                    original_text: "close".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .store_quantized(
                v_far,
                &q_far,
                EmbeddingMetadata {
                    segment_id: "far".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("far".to_string()),
                    timestamp: now,
                    original_text: "far".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        // Search with a query similar to "close"
        let query = vec![1.0, 0.0, 0.0, 0.0, 0.0];
        let q_query = ScalarQuantizer::quantize(&query).unwrap();

        let results = store
            .search_quantized(&q_query, 10, 24.0, &SearchFilters::default())
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].segment_id, "close");
        assert!(results[0].score > results[1].score);
    }
```

- [ ] Add test `search_quantized_respects_filters`:

```rust
    #[tokio::test]
    async fn search_quantized_respects_filters() {
        use oneshim_core::quantization::ScalarQuantizer;

        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        let now = Utc::now();
        let v = vec![1.0, 0.0, 0.0];
        let qv = ScalarQuantizer::quantize(&v).unwrap();

        store
            .store_quantized(
                v.clone(),
                &qv,
                EmbeddingMetadata {
                    segment_id: "summary-seg".to_string(),
                    content_type: EmbeddingContentType::SegmentSummary,
                    content_label: None,
                    timestamp: now,
                    original_text: "summary".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .store_quantized(
                v.clone(),
                &qv,
                EmbeddingMetadata {
                    segment_id: "activity-seg".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("activity".to_string()),
                    timestamp: now,
                    original_text: "activity".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        let query_qv = ScalarQuantizer::quantize(&[1.0, 0.0, 0.0]).unwrap();
        let filters = SearchFilters {
            content_types: Some(vec![EmbeddingContentType::SegmentSummary]),
            ..Default::default()
        };
        let results = store
            .search_quantized(&query_qv, 10, 0.0, &filters)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].segment_id, "summary-seg");
    }
```

- [ ] Add test `search_quantized_skips_non_quantized_rows`:

```rust
    #[tokio::test]
    async fn search_quantized_skips_non_quantized_rows() {
        use oneshim_core::quantization::ScalarQuantizer;

        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        // Store one vector via plain store() — no INT8 columns
        store
            .store(
                vec![1.0, 0.0, 0.0],
                EmbeddingMetadata {
                    segment_id: "no-int8".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("old".to_string()),
                    timestamp: Utc::now(),
                    original_text: "old".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        let query_qv = ScalarQuantizer::quantize(&[1.0, 0.0, 0.0]).unwrap();
        let results = store
            .search_quantized(&query_qv, 10, 0.0, &SearchFilters::default())
            .await
            .unwrap();

        // The non-quantized row should be excluded
        assert!(results.is_empty());
    }
```

- [ ] Run: `cargo test -p oneshim-storage`
- [ ] Commit: `feat(storage): implement search_quantized with INT8 brute-force cosine`

---

## Task 6: Implement `backfill_quantized` in `SqliteVectorStore`

**File:** `crates/oneshim-storage/src/sqlite/vector_store_impl.rs`

- [ ] Add `backfill_quantized` method to the `impl VectorStore for SqliteVectorStore` block:

```rust
    async fn backfill_quantized(&self, batch_size: usize) -> Result<u64, CoreError> {
        self.with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, vector FROM embedding_vectors WHERE vector_int8 IS NULL LIMIT ?1",
                )
                .map_err(|e| CoreError::Internal(format!("Failed to prepare backfill query: {e}")))?;

            let rows: Vec<(i64, Vec<u8>)> = stmt
                .query_map(params![batch_size as i64], |row| {
                    Ok((row.get(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .map_err(|e| CoreError::Internal(format!("Failed to query backfill rows: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            let mut count: u64 = 0;
            for (id, blob) in &rows {
                let f32_vec = bytes_to_f32_vec(blob);
                let quantized = oneshim_core::quantization::ScalarQuantizer::quantize(&f32_vec)
                    .map_err(|e| CoreError::Internal(format!("Backfill quantize failed for id={id}: {e}")))?;
                let int8_blob = i8_vec_to_bytes(&quantized.data);

                conn.execute(
                    "UPDATE embedding_vectors SET vector_int8 = ?1, quant_scale = ?2, quant_offset = ?3 WHERE id = ?4",
                    params![int8_blob, quantized.scale, quantized.offset, id],
                )
                .map_err(|e| CoreError::Internal(format!("Backfill update failed for id={id}: {e}")))?;

                count += 1;
            }

            debug!("Backfilled {count} vectors to INT8 quantized format");
            Ok(count)
        })
        .await
    }
```

- [ ] Add test `backfill_quantized_converts_existing`:

```rust
    #[tokio::test]
    async fn backfill_quantized_converts_existing() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn.clone());

        // Store 3 vectors via plain store() — no INT8 columns
        for i in 0..3 {
            store
                .store(
                    vec![1.0, 0.0, i as f32 * 0.1],
                    EmbeddingMetadata {
                        segment_id: format!("seg-{i}"),
                        content_type: EmbeddingContentType::ContentActivity,
                        content_label: Some(format!("label-{i}")),
                        timestamp: Utc::now(),
                        original_text: format!("text-{i}"),
                        model_id: "test-model".to_string(),
                    },
                )
                .await
                .unwrap();
        }

        // Backfill batch of 2
        let filled = store.backfill_quantized(2).await.unwrap();
        assert_eq!(filled, 2);

        // One remaining
        let filled = store.backfill_quantized(10).await.unwrap();
        assert_eq!(filled, 1);

        // None left
        let filled = store.backfill_quantized(10).await.unwrap();
        assert_eq!(filled, 0);

        // All should now be searchable via search_quantized
        use oneshim_core::quantization::ScalarQuantizer;
        let query_qv = ScalarQuantizer::quantize(&[1.0, 0.0, 0.0]).unwrap();
        let results = store
            .search_quantized(&query_qv, 10, 0.0, &SearchFilters::default())
            .await
            .unwrap();
        assert_eq!(results.len(), 3);
    }
```

- [ ] Run: `cargo test -p oneshim-storage`
- [ ] Commit: `feat(storage): implement backfill_quantized for lazy INT8 migration`

---

## Task 7: Wire quantized path into `EmbeddingPipeline`

**File:** `crates/oneshim-analysis/src/embedding_pipeline.rs`

- [ ] Add `quantization_enabled: bool` field to the `EmbeddingPipeline` struct:

```rust
pub struct EmbeddingPipeline {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    pii_filter: PiiFilter,
    vector_store: Arc<dyn VectorStore>,
    quantization_enabled: bool,
}
```

- [ ] Update the constructor to accept the flag:

```rust
    pub fn new(
        provider: Arc<dyn EmbeddingProvider>,
        pii_filter: PiiFilter,
        store: Arc<dyn VectorStore>,
        quantization_enabled: bool,
    ) -> Self {
        Self {
            embedding_provider: provider,
            pii_filter,
            vector_store: store,
            quantization_enabled,
        }
    }
```

- [ ] Add import at the top of the file:

```rust
use oneshim_core::quantization::ScalarQuantizer;
```

- [ ] Modify `process_content_activities`: replace the store loop body. Change:

```rust
        for (vector, meta) in vectors.into_iter().zip(metadata) {
            self.vector_store.store(vector, meta).await?;
        }
```

to:

```rust
        for (vector, meta) in vectors.into_iter().zip(metadata) {
            if self.quantization_enabled {
                let quantized = ScalarQuantizer::quantize(&vector)?;
                self.vector_store
                    .store_quantized(vector, &quantized, meta)
                    .await?;
            } else {
                self.vector_store.store(vector, meta).await?;
            }
        }
```

- [ ] Modify `process_llm_summary`: replace the store call. Change:

```rust
        self.vector_store
            .store(
                vector,
                EmbeddingMetadata {
```

to:

```rust
        let metadata = EmbeddingMetadata {
            segment_id: segment_id.to_string(),
            content_type: EmbeddingContentType::SegmentSummary,
            content_label: None,
            timestamp,
            original_text: filtered,
            model_id: self.embedding_provider.model_id().to_string(),
        };

        if self.quantization_enabled {
            let quantized = ScalarQuantizer::quantize(&vector)?;
            self.vector_store
                .store_quantized(vector, &quantized, metadata)
                .await
        } else {
            self.vector_store.store(vector, metadata).await
        }
```

And remove the old inline `EmbeddingMetadata` construction and the trailing `.await`.

- [ ] Update all existing test constructors — add `false` as the fourth argument to `EmbeddingPipeline::new()`:

```rust
        let pipeline = EmbeddingPipeline::new(provider, identity_filter(), store.clone(), false);
```

- [ ] Add the `MockVectorStore` tracking for `store_quantized` calls. Add a new field:

```rust
    struct MockVectorStore {
        stored: Mutex<Vec<(Vec<f32>, EmbeddingMetadata)>>,
        stored_quantized: Mutex<Vec<(Vec<f32>, EmbeddingMetadata)>>,
    }
```

Update `new()`:

```rust
        fn new() -> Self {
            Self {
                stored: Mutex::new(Vec::new()),
                stored_quantized: Mutex::new(Vec::new()),
            }
        }
```

Add `stored_quantized_count()`:

```rust
        fn stored_quantized_count(&self) -> usize {
            self.stored_quantized.lock().unwrap().len()
        }
```

Add the `store_quantized` impl inside the `impl VectorStore for MockVectorStore` block:

```rust
        async fn store_quantized(
            &self,
            vector_f32: Vec<f32>,
            _vector_int8: &oneshim_core::quantization::QuantizedVector,
            metadata: EmbeddingMetadata,
        ) -> Result<(), CoreError> {
            self.stored_quantized.lock().unwrap().push((vector_f32, metadata));
            Ok(())
        }
```

- [ ] Add test `quantization_enabled_uses_store_quantized`:

```rust
    #[tokio::test]
    async fn quantization_enabled_uses_store_quantized() {
        let provider = Arc::new(MockEmbeddingProvider { dims: 5 });
        let store = Arc::new(MockVectorStore::new());
        let pipeline = EmbeddingPipeline::new(provider, identity_filter(), store.clone(), true);

        let segment =
            make_segment_with_activities(vec![make_activity("main.rs"), make_activity("lib.rs")]);

        let count = pipeline.process_content_activities(&segment).await.unwrap();
        assert_eq!(count, 2);
        // store() should NOT be called
        assert_eq!(store.stored_count(), 0);
        // store_quantized() should be called
        assert_eq!(store.stored_quantized_count(), 2);
    }
```

- [ ] Add test `quantization_enabled_llm_summary_uses_store_quantized`:

```rust
    #[tokio::test]
    async fn quantization_enabled_llm_summary_uses_store_quantized() {
        let provider = Arc::new(MockEmbeddingProvider { dims: 5 });
        let store = Arc::new(MockVectorStore::new());
        let pipeline = EmbeddingPipeline::new(provider, identity_filter(), store.clone(), true);

        pipeline
            .process_llm_summary("seg-001", "Focused work on auth module", Utc::now())
            .await
            .unwrap();

        assert_eq!(store.stored_count(), 0);
        assert_eq!(store.stored_quantized_count(), 1);
    }
```

- [ ] Run: `cargo test -p oneshim-analysis`
- [ ] Commit: `feat(analysis): wire quantized store path into EmbeddingPipeline`

---

## Task 8: Wire quantized search into `VectorRetriever`

**File:** `crates/oneshim-analysis/src/vector_retriever.rs`

- [ ] Add `quantization_enabled: bool` field to the `VectorRetriever` struct:

```rust
pub struct VectorRetriever {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    vector_store: Arc<dyn VectorStore>,
    pii_filter: PiiFilter,
    max_results: usize,
    time_decay_hours: f32,
    quantization_enabled: bool,
}
```

- [ ] Update the constructor:

```rust
    pub fn new(
        embedding_provider: Arc<dyn EmbeddingProvider>,
        vector_store: Arc<dyn VectorStore>,
        pii_filter: PiiFilter,
        max_results: usize,
        time_decay_hours: f32,
        quantization_enabled: bool,
    ) -> Self {
        Self {
            embedding_provider,
            vector_store,
            pii_filter,
            max_results,
            time_decay_hours,
            quantization_enabled,
        }
    }
```

- [ ] Add import at the top:

```rust
use oneshim_core::quantization::ScalarQuantizer;
use oneshim_core::models::embedding::SearchFilters;
```

(Note: `SearchFilters` is already imported; just add `ScalarQuantizer`.)

- [ ] Modify `retrieve_for_context`: after obtaining `query_vector`, branch on quantization. Replace:

```rust
        self.vector_store
            .search(&query_vector, self.max_results, self.time_decay_hours)
            .await
```

with:

```rust
        if self.quantization_enabled {
            let quantized = ScalarQuantizer::quantize(&query_vector)?;
            self.vector_store
                .search_quantized(
                    &quantized,
                    self.max_results,
                    self.time_decay_hours,
                    &SearchFilters::default(),
                )
                .await
        } else {
            self.vector_store
                .search(&query_vector, self.max_results, self.time_decay_hours)
                .await
        }
```

- [ ] Modify `search_natural_language`: after obtaining `query_vector`, branch on quantization. Replace the entire if/else block:

```rust
        if self.quantization_enabled {
            let quantized = ScalarQuantizer::quantize(&query_vector)?;
            let filters = filters.unwrap_or_default();
            self.vector_store
                .search_quantized(
                    &quantized,
                    self.max_results,
                    self.time_decay_hours,
                    &filters,
                )
                .await
        } else if let Some(filters) = filters {
            self.vector_store
                .search_filtered(
                    &query_vector,
                    self.max_results,
                    self.time_decay_hours,
                    &filters,
                )
                .await
        } else {
            self.vector_store
                .search(&query_vector, self.max_results, self.time_decay_hours)
                .await
        }
```

- [ ] Update all existing test constructors — add `false` as the sixth argument to `VectorRetriever::new()`:

```rust
        VectorRetriever::new(
            Arc::new(MockEmbeddingProvider),
            Arc::new(MockVectorStore::new(results)),
            pii_filter,
            5,
            168.0,
            false,
        )
```

Also update the `empty_store_returns_empty` test's inline constructor.

- [ ] Add a `quantized_search_called` tracking field to the test `MockVectorStore`:

```rust
    struct MockVectorStore {
        results: Vec<SearchResult>,
        quantized_search_called: std::sync::atomic::AtomicBool,
    }

    impl MockVectorStore {
        fn new(results: Vec<SearchResult>) -> Self {
            Self {
                results,
                quantized_search_called: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn empty() -> Self {
            Self {
                results: vec![],
                quantized_search_called: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn was_quantized_search_called(&self) -> bool {
            self.quantized_search_called.load(std::sync::atomic::Ordering::Relaxed)
        }
    }
```

Add `search_quantized` impl to the mock:

```rust
        async fn search_quantized(
            &self,
            _query_vector: &oneshim_core::quantization::QuantizedVector,
            _limit: usize,
            _time_decay_hours: f32,
            _filters: &SearchFilters,
        ) -> Result<Vec<SearchResult>, CoreError> {
            self.quantized_search_called
                .store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(self.results.clone())
        }
```

- [ ] Add test `quantization_enabled_uses_search_quantized`:

```rust
    #[tokio::test]
    async fn quantization_enabled_uses_search_quantized() {
        let results = vec![make_search_result("Quantized result", 0.9)];
        let store = Arc::new(MockVectorStore::new(results));
        let retriever = VectorRetriever::new(
            Arc::new(MockEmbeddingProvider),
            store.clone(),
            noop_filter(),
            5,
            168.0,
            true,
        );

        let found = retriever
            .retrieve_for_context("VSCode", "main.rs", None)
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
        assert!(store.was_quantized_search_called());
    }
```

- [ ] Add test `quantization_enabled_natural_language_uses_search_quantized`:

```rust
    #[tokio::test]
    async fn quantization_enabled_natural_language_uses_search_quantized() {
        let results = vec![make_search_result("NL quantized", 0.85)];
        let store = Arc::new(MockVectorStore::new(results));
        let retriever = VectorRetriever::new(
            Arc::new(MockEmbeddingProvider),
            store.clone(),
            noop_filter(),
            5,
            168.0,
            true,
        );

        let found = retriever
            .search_natural_language("what did I work on", None)
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
        assert!(store.was_quantized_search_called());
    }
```

- [ ] Run: `cargo test -p oneshim-analysis`
- [ ] Commit: `feat(analysis): wire quantized search into VectorRetriever`

---

## Task 9: Update callers of `EmbeddingPipeline::new` and `VectorRetriever::new`

After Tasks 7 and 8 changed the constructor signatures, any callers outside of test modules must be updated.

- [ ] Search for all non-test callers:

```bash
cargo check --workspace 2>&1 | head -50
```

Likely affected files (check output of cargo check):
- `crates/oneshim-analysis/src/hybrid_search_service.rs` (if it constructs VectorRetriever)
- `src-tauri/src/agent_runtime.rs` or `src-tauri/src/scheduler/` (DI wiring)

- [ ] For each caller of `EmbeddingPipeline::new()`: add `false` (or wire from `config.analysis.embedding.quantization_enabled`) as the fourth argument.
- [ ] For each caller of `VectorRetriever::new()`: add `false` (or wire from config) as the sixth argument.
- [ ] Run: `cargo check --workspace`
- [ ] Run: `cargo test --workspace`
- [ ] Commit: `refactor: update EmbeddingPipeline and VectorRetriever callers for quantization flag`

---

## Task 10: Final verification

- [ ] Run full test suite:

```bash
cargo test --workspace
```

- [ ] Run lints:

```bash
cargo fmt --check
cargo clippy --workspace
```

- [ ] Verify test counts increased (expected: +8 new tests minimum across oneshim-storage and oneshim-analysis).
- [ ] Commit: `test: verify P3 Phase A — INT8 scalar quantization pipeline`

---

## Exception handling notes

- **ScalarQuantizer fails on edge-case vector** (zero-length, NaN): `quantize()` returns `CoreError::Internal`. The `EmbeddingPipeline` propagates this error — the caller can decide to fall back to f32-only store. This is acceptable because edge-case vectors should not appear in normal operation (the embedding model always produces finite 384-dim vectors).
- **backfill_quantized encounters corrupt BLOB**: The f32 BLOB parsing via `bytes_to_f32_vec` does not validate — if bytes are not 4-aligned, it silently truncates. The `ScalarQuantizer::quantize` call will catch zero-length vectors. Corrupt rows are skipped via `filter_map(|r| r.ok())` at the query level.
- **Existing mock VectorStore impls unchanged**: The 3 default methods have `default` implementations returning errors, so the 3 existing mock `VectorStore` impls in test modules continue to compile. Only the mocks in `embedding_pipeline.rs` and `vector_retriever.rs` are explicitly updated (Tasks 7-8) to track quantized calls.
- **Config flag defaults to false**: No behavioral change for existing users until they explicitly set `quantization_enabled: true`.

## Deferred (Phase A.5 / Phase B)

- **Float32 column removal** (Phase A.5): After all vectors backfilled + 7 days validation, drop f32 column via V15 migration.
- **Query expansion** (Phase B): `QueryExpander` module in oneshim-analysis for context-aware query enrichment.
- **Negative feedback filtering** (Phase B): Query-time JOIN against `suggestions.dismissed_at`.
- **2-bit quantization** (Phase C): Revisit if storage exceeds 100 MB.
