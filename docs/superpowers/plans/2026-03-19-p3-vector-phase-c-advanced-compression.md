# P3 Vector Phase C: Advanced Compression + Indexing — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable sub-linear vector search over 100K+ vectors via 2-bit binary quantization (Hamming distance coarse filter), IVF (inverted file index) cluster partitioning, and an adaptive search coordinator that auto-selects the optimal strategy based on collection size. Target: <= 5 ms p95 search at 200K vectors with >= 95% recall@10.

**Architecture:** `BinaryQuantizer` (new, `oneshim-core`) maps INT8 vectors to 96-byte 2-bit codes for Hamming filtering. `IvfIndex` (new, `oneshim-core`) partitions vectors into sqrt(N) clusters via k-means++ / Lloyd's. `VectorIndex` port trait (new, `oneshim-core/ports`) defines index build + search operations. `SqliteVectorIndex` (new, `oneshim-storage`) implements the port with 4 new tables (V16 migration). `AdaptiveSearchCoordinator` (new, `oneshim-analysis`) auto-selects brute-force / IVF / IVF+binary based on vector count. `VectorRetriever` delegates to the coordinator when available.

> **Migration version:** Sync 3b uses V15; this plan uses V16.

**Tech Stack:** Rust, rusqlite, tokio::spawn_blocking, serde

**Spec:** `docs/superpowers/specs/2026-03-19-p3-vector-phase-c-advanced-compression-design.md`

---

## What is already done (DO NOT re-implement)

| Component | File | Status |
|-----------|------|--------|
| `ScalarQuantizer` + `QuantizedVector` | `crates/oneshim-core/src/quantization.rs` | Done (7 tests) |
| `VectorStore` trait (store, search, search_filtered, search_quantized, backfill_quantized, count_unquantized) | `crates/oneshim-core/src/ports/vector_store.rs` | Done |
| `SqliteVectorStore` (brute-force INT8 + f32 search) | `crates/oneshim-storage/src/sqlite/vector_store_impl.rs` | Done (25+ tests) |
| V14 migration (INT8 columns, sync columns) | `crates/oneshim-storage/src/migration.rs` | Done (CURRENT_VERSION = 14) |
| `EmbeddingConfig` with `quantization_enabled`, `quantization_float32_retention` | `crates/oneshim-core/src/config/sections/analysis.rs` | Done |
| `VectorRetriever` with quantized search + query expansion | `crates/oneshim-analysis/src/vector_retriever.rs` | Done |

---

## File Map

### New files

| File | Purpose |
|------|---------|
| `crates/oneshim-core/src/binary_quantizer.rs` | `BinaryQuantizer`, `BinaryCode`, `QuantileThresholds` — 2-bit encoding + Hamming distance |
| `crates/oneshim-core/src/ivf_index.rs` | `IvfIndex`, `IvfCentroid`, `IvfBuildConfig` — k-means++ clustering + partition search |
| `crates/oneshim-core/src/ports/vector_index.rs` | `VectorIndex` trait + `IndexMeta` struct — port for index build/search operations |
| `crates/oneshim-storage/src/sqlite/vector_index_impl.rs` | `SqliteVectorIndex` impl of `VectorIndex` — SQLite-backed index storage and indexed search |
| `crates/oneshim-analysis/src/adaptive_search.rs` | `AdaptiveSearchCoordinator`, `SearchStrategy`, `SearchConfig` — auto strategy selection + two-stage pipeline |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/lib.rs` | Export `binary_quantizer` and `ivf_index` modules |
| `crates/oneshim-core/src/ports/mod.rs` | Export `vector_index` module |
| `crates/oneshim-core/src/ports/vector_store.rs` | Add `count_active_vectors()` default method |
| `crates/oneshim-core/src/config/sections/analysis.rs` | Add `index_strategy`, `ivf_nprobe`, `binary_oversample_factor` to `EmbeddingConfig` |
| `crates/oneshim-storage/src/migration.rs` | V16 migration: 4 new tables |
| `crates/oneshim-storage/src/sqlite/mod.rs` | Export `vector_index_impl` module |
| `crates/oneshim-storage/src/sqlite/vector_store_impl.rs` | Implement `count_active_vectors()` override |
| `crates/oneshim-analysis/src/lib.rs` | Export `adaptive_search` module |
| `crates/oneshim-analysis/src/vector_retriever.rs` | Accept optional `AdaptiveSearchCoordinator`, delegate when present |

### No new crate. No new external dependency. No new cross-crate dependency violations.

---

## Phase C.1: 2-Bit Binary Quantizer (~6h)

### Task 1: Create `BinaryQuantizer` + `BinaryCode` + `QuantileThresholds`

**File:** `crates/oneshim-core/src/binary_quantizer.rs` (NEW)

- [ ] Create the file with the following public types:

```rust
use crate::error::CoreError;
use serde::{Deserialize, Serialize};

/// Per-dimension quantile thresholds computed across the entire collection.
/// Used by 2-bit binary quantization to map each f32 dimension to 2 bits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantileThresholds {
    pub q25: Vec<f32>,  // 25th percentile per dimension
    pub q50: Vec<f32>,  // 50th percentile (median)
    pub q75: Vec<f32>,  // 75th percentile
    pub dimensions: usize,
}

/// 2-bit binary code packed into bytes. For 384 dims = 96 bytes.
/// Each dimension occupies 2 bits: 00, 01, 10, 11 mapped to 4 quantile levels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BinaryCode {
    pub data: Vec<u8>,
}

/// Stateless 2-bit binary quantizer for coarse-grained Hamming distance filtering.
pub struct BinaryQuantizer;
```

- [ ] Implement `BinaryQuantizer::compute_thresholds(vectors: &[Vec<f32>], dimensions: usize) -> Result<QuantileThresholds, CoreError>`:

> **Spec reconciliation note:** The design spec uses `quantize_to_binary` and passes `QuantizedVector` input. This plan uses `encode` (for clarity: it produces a `BinaryCode`, not a `QuantizedVector`) and `compute_thresholds` takes `&[Vec<f32>]` (f32 input, since thresholds are computed in continuous space before discretization). The spec will be updated to match these names.
  - Validate: non-empty vectors, all vectors have correct `dimensions`
  - For each dimension (0..dimensions): collect all values into a temp Vec, sort, pick indices at 25%, 50%, 75% percentiles
  - For constant dimensions (all same value): set q25=q50=q75=that_value (all bits will be 01)
  - Memory: one dimension at a time = N_vectors * 4 bytes peak (800 KB at 200K)

- [ ] Implement `BinaryQuantizer::encode(vector: &[f32], thresholds: &QuantileThresholds) -> Result<BinaryCode, CoreError>`:
  - Validate: vector length == thresholds.dimensions
  - For each dimension, map to 2 bits: `00` if < q25, `01` if < q50, `10` if < q75, `11` if >= q75
  - Pack 4 two-bit codes per byte (MSB first): dims 0-3 in byte 0, dims 4-7 in byte 1, etc.
  - Output length: `ceil(dimensions * 2 / 8)` bytes = 96 for 384 dims

- [ ] Implement `BinaryQuantizer::hamming_distance(a: &BinaryCode, b: &BinaryCode) -> u32`:

```rust
pub fn hamming_distance(a: &BinaryCode, b: &BinaryCode) -> u32 {
    a.data.iter()
        .zip(b.data.iter())
        .map(|(&x, &y)| (x ^ y).count_ones())
        .sum()
}
```

- [ ] Run: `cargo check -p oneshim-core`

---

### Task 2: Export `binary_quantizer` module

**File:** `crates/oneshim-core/src/lib.rs`

- [ ] Add `pub mod binary_quantizer;` alongside the existing `pub mod quantization;`
- [ ] Run: `cargo check -p oneshim-core`

---

### Task 3: Unit tests for `BinaryQuantizer`

**File:** `crates/oneshim-core/src/binary_quantizer.rs`

- [ ] Add `#[cfg(test)] mod tests` at bottom of the file with these tests:

```
- threshold_computation_basic: 4 vectors, 3 dims, verify q25/q50/q75 values
- threshold_computation_single_vector: returns error (cannot compute quantiles on 1 vector)
- threshold_computation_empty: returns error
- threshold_computation_constant_dimension: all q25=q50=q75=same_value
- encode_basic: known thresholds, verify specific bit pattern
- encode_dimension_mismatch: returns error
- encode_all_below_q25: all bits 00
- encode_all_above_q75: all bits 11
- hamming_distance_identical: returns 0
- hamming_distance_opposite: returns max (each byte contributes count_ones(0xFF))
- hamming_distance_single_bit_diff: verify correct count
- hamming_distance_384_dims: 96-byte codes, verify correct distance
- encode_decode_roundtrip: encode two vectors, verify hamming distance correlates with actual distance
```

- [ ] Run: `cargo test -p oneshim-core -- binary_quantizer`
- [ ] Verify all tests pass
- [ ] Commit: `feat(core): add BinaryQuantizer with 2-bit encoding and Hamming distance`

---

## Phase C.2: IVF Index (~8h)

### Task 4: Create `IvfIndex` + k-means++ + Lloyd's iteration

**File:** `crates/oneshim-core/src/ivf_index.rs` (NEW)

- [ ] Create the file with the following public types:

```rust
use crate::error::CoreError;
use crate::quantization::{QuantizedVector, ScalarQuantizer};
use std::collections::HashMap;

pub struct IvfBuildConfig {
    pub n_clusters: usize,   // default: sqrt(n_vectors)
    pub n_iterations: usize, // default: 10
    pub seed: u64,           // for reproducible k-means++ init
}

pub struct IvfCentroid {
    pub id: usize,
    pub vector: QuantizedVector,   // INT8 centroid
    pub member_count: usize,
}

pub struct IvfIndex {
    centroids: Vec<IvfCentroid>,
    assignments: HashMap<i64, usize>,  // vector_id -> cluster_id
}
```

- [ ] Implement `IvfIndex::build(vectors: &[(i64, QuantizedVector)], config: &IvfBuildConfig) -> Result<IvfIndex, CoreError>`:
  - Validate: vectors.len() >= config.n_clusters, n_clusters >= 1
  - K-means++ initialization:
    1. Pick first centroid uniformly at random (seeded RNG)
    2. For each subsequent centroid: compute distance to nearest existing centroid for each vector, pick next centroid with probability proportional to squared distance
  - Lloyd's iteration (config.n_iterations rounds):
    1. Assign each vector to nearest centroid (using `ScalarQuantizer::cosine_similarity_int8`)
    2. Recompute centroids: dequantize assigned vectors to f32, compute component-wise mean, L2-normalize, re-quantize to INT8
    3. Update member_count for each centroid
  - Build assignments HashMap
  - **Note on cosine distance**: use `1.0 - cosine_similarity_int8()` as the distance metric for centroid selection, since cosine similarity is higher for closer vectors

- [ ] Implement `IvfIndex::nearest_centroids(&self, query: &QuantizedVector, nprobe: usize) -> Vec<usize>`:
  - Compute cosine similarity to all centroids
  - Sort by similarity descending
  - Return top `nprobe` cluster IDs

- [ ] Implement `IvfIndex::assign(&self, vector: &QuantizedVector) -> usize`:
  - Find nearest centroid, return its ID

- [ ] Implement `IvfIndex::get_cluster_members(&self, cluster_id: usize) -> Vec<i64>`:
  - Filter assignments by cluster_id, collect vector IDs

- [ ] Implement accessor methods:
  - `pub fn centroids(&self) -> &[IvfCentroid]`
  - `pub fn assignments(&self) -> &HashMap<i64, usize>`
  - `pub fn n_clusters(&self) -> usize`

- [ ] Run: `cargo check -p oneshim-core`

---

### Task 5: Export `ivf_index` module

**File:** `crates/oneshim-core/src/lib.rs`

- [ ] Add `pub mod ivf_index;` alongside the existing modules
- [ ] Run: `cargo check -p oneshim-core`

---

### Task 6: Unit tests for `IvfIndex`

**File:** `crates/oneshim-core/src/ivf_index.rs`

- [ ] Add `#[cfg(test)] mod tests` with these tests:

```
- build_basic_clustering: 100 synthetic vectors in 3 natural clusters (10-dim),
  verify all 3 clusters have non-zero membership, centroids are near expected positions
- build_single_cluster: n_clusters=1, all vectors assigned to cluster 0
- build_too_few_vectors: vectors.len() < n_clusters, returns error
- build_empty_vectors: returns error
- nearest_centroids_returns_correct_order: build index, query with vector near known cluster,
  verify nearest cluster is first in result
- nearest_centroids_nprobe_limits_results: nprobe=2, verify only 2 cluster IDs returned
- assign_to_nearest: build index, assign new vector, verify it maps to expected cluster
- get_cluster_members_returns_correct_ids: build index, verify member lists are disjoint and
  union equals all input vector IDs
- deterministic_with_seed: build twice with same seed, verify identical assignments
- build_config_defaults: verify sqrt(N) cluster count computation
```

- [ ] Run: `cargo test -p oneshim-core -- ivf_index`
- [ ] Verify all tests pass
- [ ] Commit: `feat(core): add IvfIndex with k-means++ clustering and partition search`

---

## Phase C.3: Storage + Port Layer (~6h)

### Task 7: Create `VectorIndex` port trait

**File:** `crates/oneshim-core/src/ports/vector_index.rs` (NEW)

- [ ] Create the file with the `VectorIndex` trait as specified in the design spec (Section 3.4):
  - `build_ivf_index(&self, n_clusters, n_iterations) -> Result<usize, CoreError>`
  - `build_binary_codes(&self) -> Result<u64, CoreError>`
  - `search_ivf(&self, query_vector, nprobe, limit, time_decay_hours, filters) -> Result<Vec<SearchResult>, CoreError>`
  - `search_ivf_binary(&self, query_vector, query_binary, nprobe, oversample_factor, limit, time_decay_hours, filters) -> Result<Vec<SearchResult>, CoreError>`
  - `assign_to_cluster(&self, vector_id, vector) -> Result<(), CoreError>`
  - `store_binary_code(&self, vector_id, code) -> Result<(), CoreError>`
  - `get_index_meta(&self) -> Result<IndexMeta, CoreError>`
  - `count_unindexed(&self) -> Result<u64, CoreError>`
  - `load_quantile_thresholds(&self) -> Result<Option<QuantileThresholds>, CoreError>`

- [ ] Add `IndexMeta` struct:

```rust
pub struct IndexMeta {
    pub ivf_built_at: Option<String>,
    pub ivf_vector_count: u64,
    pub binary_built_at: Option<String>,
    pub total_vector_count: u64,
    pub unindexed_count: u64,
}
```

- [ ] All methods should have default implementations returning `CoreError::Internal("not implemented")` so test mocks compile without change.

- [ ] Add required imports: `QuantizedVector`, `BinaryCode`, `QuantileThresholds`, `SearchResult`, `SearchFilters`

---

### Task 8: Export `vector_index` from ports module

**File:** `crates/oneshim-core/src/ports/mod.rs`

- [ ] Add `pub mod vector_index;`
- [ ] Run: `cargo check -p oneshim-core`

---

### Task 9: Add `count_active_vectors` to `VectorStore` trait

**File:** `crates/oneshim-core/src/ports/vector_store.rs`

- [ ] Add default method after the existing `count_unquantized` method:

```rust
    /// Count the number of active (non-stale) vectors in the store.
    /// Used by AdaptiveSearchCoordinator to select search strategy.
    async fn count_active_vectors(&self) -> Result<u64, CoreError> {
        Ok(0)
    }
```

- [ ] Run: `cargo check --workspace` (verify all existing VectorStore impls and mocks still compile)
- [ ] Commit: `feat(core): add VectorIndex port trait and count_active_vectors default method`

---

### Task 10: V16 migration — 4 new tables

**File:** `crates/oneshim-storage/src/migration.rs`

- [ ] Update `CURRENT_VERSION` from 15 to 16 (V15 is reserved for Sync 3b `lan_peer_pins`):

```rust
const CURRENT_VERSION: u32 = 16;
```

- [ ] Add `if current < 16 { migrate_v16(conn)?; }` in `run_migrations`

- [ ] Implement `migrate_v16`:

```rust
fn migrate_v16(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V16 execution: IVF index + 2-bit binary codes for vector search");

    conn.execute_batch(
        "
        -- 2-bit binary codes for Hamming distance filtering
        CREATE TABLE IF NOT EXISTS vector_binary_codes (
            vector_id INTEGER PRIMARY KEY,
            binary_code BLOB NOT NULL,
            FOREIGN KEY (vector_id) REFERENCES embedding_vectors(id) ON DELETE CASCADE
        );

        -- IVF cluster centroids (INT8 format)
        CREATE TABLE IF NOT EXISTS ivf_centroids (
            id INTEGER PRIMARY KEY,
            centroid_int8 BLOB NOT NULL,
            centroid_scale REAL NOT NULL,
            centroid_offset REAL NOT NULL,
            vector_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- IVF cluster memberships
        CREATE TABLE IF NOT EXISTS ivf_assignments (
            vector_id INTEGER PRIMARY KEY,
            cluster_id INTEGER NOT NULL,
            FOREIGN KEY (vector_id) REFERENCES embedding_vectors(id) ON DELETE CASCADE,
            FOREIGN KEY (cluster_id) REFERENCES ivf_centroids(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_ivf_assign_cluster ON ivf_assignments(cluster_id);

        -- Index build metadata (key-value store)
        CREATE TABLE IF NOT EXISTS vector_index_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- version record
        INSERT INTO schema_version (version) VALUES (16);
        ",
    )?;

    info!("migration V16 completed");
    Ok(())
}
```

- [ ] Update the `migration_all_versions` test to assert:
  - `version == 16`
  - Tables `vector_binary_codes`, `ivf_centroids`, `ivf_assignments`, `vector_index_meta` exist
  - Index `idx_ivf_assign_cluster` exists

- [ ] Run: `cargo test -p oneshim-storage -- migration`
- [ ] Commit: `feat(storage): V16 migration — IVF index and binary code tables`

---

### Task 11: Implement `count_active_vectors` in `SqliteVectorStore`

**File:** `crates/oneshim-storage/src/sqlite/vector_store_impl.rs`

- [ ] Add override for `count_active_vectors` in the `impl VectorStore for SqliteVectorStore` block:

```rust
    async fn count_active_vectors(&self) -> Result<u64, CoreError> {
        self.with_conn(move |conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM embedding_vectors WHERE is_stale = 0",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| {
                    CoreError::Internal(format!("Failed to count active vectors: {e}"))
                })?;
            Ok(count as u64)
        })
        .await
    }
```

- [ ] Add test `count_active_vectors_basic`:

```rust
    #[tokio::test]
    async fn count_active_vectors_basic() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        assert_eq!(store.count_active_vectors().await.unwrap(), 0);

        // Store 3 vectors
        for i in 0..3 {
            store.store(vec![1.0, 0.0], EmbeddingMetadata {
                segment_id: format!("seg-{i}"),
                content_type: EmbeddingContentType::ContentActivity,
                content_label: None,
                timestamp: Utc::now(),
                original_text: format!("text-{i}"),
                model_id: "test-model".to_string(),
            }).await.unwrap();
        }
        assert_eq!(store.count_active_vectors().await.unwrap(), 3);

        // Mark one stale
        store.mark_stale("test-model").await.unwrap();
        assert_eq!(store.count_active_vectors().await.unwrap(), 0);
    }
```

- [ ] Run: `cargo test -p oneshim-storage`
- [ ] Commit: `feat(storage): implement count_active_vectors in SqliteVectorStore`

---

### Task 12: Create `SqliteVectorIndex`

**File:** `crates/oneshim-storage/src/sqlite/vector_index_impl.rs` (NEW)

- [ ] Create the file with struct and constructor:

```rust
use async_trait::async_trait;
use oneshim_core::binary_quantizer::{BinaryCode, BinaryQuantizer, QuantileThresholds};
use oneshim_core::error::CoreError;
use oneshim_core::ivf_index::{IvfBuildConfig, IvfIndex};
use oneshim_core::models::embedding::{SearchFilters, SearchResult};
use oneshim_core::ports::vector_index::{IndexMeta, VectorIndex};
use oneshim_core::quantization::{QuantizedVector, ScalarQuantizer};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex, RwLock};
use tracing::{debug, info};

pub struct SqliteVectorIndex {
    conn: Arc<Mutex<Connection>>,
    centroid_cache: RwLock<Option<Vec<oneshim_core::ivf_index::IvfCentroid>>>,
}
```

- [ ] Implement `with_conn` helper (same pattern as `SqliteVectorStore`)

- [ ] Implement `build_ivf_index`:
  1. Load all non-stale INT8 vectors from `embedding_vectors` (id, vector_int8, quant_scale, quant_offset)
  2. Build `IvfIndex` in memory using `IvfIndex::build()` (release SQLite mutex during computation)
  3. In a transaction: DELETE FROM ivf_assignments; DELETE FROM ivf_centroids; INSERT centroids; UPDATE vector_index_meta
  4. Batch INSERT assignments in chunks of 1000 (acquire/release lock per batch)
  5. Invalidate centroid_cache
  6. Run `PRAGMA wal_checkpoint(TRUNCATE)` after completion

- [ ] Implement `build_binary_codes`:
  1. Load all non-stale INT8 vectors, dequantize to f32
  2. Compute `QuantileThresholds` via `BinaryQuantizer::compute_thresholds()`
  3. Encode each vector to `BinaryCode` via `BinaryQuantizer::encode()`
  4. In batches of 1000: INSERT OR REPLACE INTO vector_binary_codes
  5. Store thresholds as JSON in `vector_index_meta` (key: 'binary_quantile_thresholds')
  6. Update `vector_index_meta` (key: 'binary_built_at')

- [ ] Implement `search_ivf`:
  1. Load centroids (from cache or DB)
  2. Find nearest `nprobe` centroids via `IvfIndex::nearest_centroids()`
  3. SELECT embedding_vectors JOIN ivf_assignments WHERE cluster_id IN (...) AND is_stale = 0 + filter conditions
  4. Brute-force INT8 cosine similarity on the subset
  5. Apply time decay, sort, truncate to limit

- [ ] Implement `search_ivf_binary`:
  1. Load centroids, find nearest `nprobe` centroids
  2. SELECT binary_code FROM vector_binary_codes JOIN ivf_assignments WHERE cluster_id IN (...)
  3. Hamming distance filter: keep top `limit * oversample_factor` candidates
  4. Load INT8 vectors for surviving candidate IDs
  5. INT8 cosine similarity re-rank
  6. Apply time decay, sort, truncate to limit

- [ ] Implement remaining methods:
  - `assign_to_cluster`: find nearest centroid, INSERT OR REPLACE INTO ivf_assignments
  - `store_binary_code`: INSERT OR REPLACE INTO vector_binary_codes
  - `get_index_meta`: SELECT from vector_index_meta + COUNT queries
  - `count_unindexed`: COUNT(*) WHERE id NOT IN (SELECT vector_id FROM ivf_assignments) AND is_stale = 0
  - `load_quantile_thresholds`: SELECT from vector_index_meta key='binary_quantile_thresholds', deserialize JSON

---

### Task 13: Export `vector_index_impl` from sqlite module

**File:** `crates/oneshim-storage/src/sqlite/mod.rs`

- [ ] Add `pub mod vector_index_impl;`
- [ ] Run: `cargo check -p oneshim-storage`

---

### Task 14: Integration tests for `SqliteVectorIndex`

**File:** `crates/oneshim-storage/src/sqlite/vector_index_impl.rs`

- [ ] Add `#[cfg(test)] mod tests` with:

```
- build_ivf_and_search_roundtrip:
  Store 100 synthetic INT8 vectors (10 clusters of 10 each in 8-dim space).
  Build IVF index with n_clusters=10. Verify centroids stored (10 rows in ivf_centroids).
  Verify all 100 vectors assigned (100 rows in ivf_assignments).
  Search with a query near one cluster. Verify results come from expected cluster.

- build_binary_codes_and_search:
  Store 50 INT8 vectors. Build binary codes. Verify 50 rows in vector_binary_codes.
  Build IVF index. Use search_ivf_binary with oversample_factor=5.
  Verify results are non-empty and properly scored.

- search_ivf_respects_filters:
  Store vectors with different content types and time ranges.
  Search with content_type filter. Verify only matching results returned.

- assign_to_cluster_incremental:
  Build IVF index. Store a new vector. Call assign_to_cluster.
  Verify the new vector appears in ivf_assignments.

- get_index_meta_reflects_build:
  Before build: ivf_built_at is None. After build: ivf_built_at is Some.

- count_unindexed_tracks_new_vectors:
  Build index. Add new vectors. count_unindexed returns the new count.

- empty_store_build_returns_error:
  No vectors. build_ivf_index returns error (cannot cluster empty set).

- centroid_cache_invalidated_on_rebuild:
  Build index. Search (populates cache). Rebuild with different data.
  Search again. Verify results reflect the new index.
```

- [ ] Run: `cargo test -p oneshim-storage -- vector_index`
- [ ] Commit: `feat(storage): implement SqliteVectorIndex with IVF + binary code search`

---

## Phase C.4: Adaptive Search + Integration (~5h)

### Task 15: Add config fields to `EmbeddingConfig`

**File:** `crates/oneshim-core/src/config/sections/analysis.rs`

- [ ] Add 3 new fields to `EmbeddingConfig` after `quantization_float32_retention`:

```rust
    /// Index strategy for vector search.
    /// "auto" (default): select based on collection size.
    /// "brute_force": always use brute-force INT8 scan.
    /// "ivf": always use IVF partitioning.
    /// "ivf_binary": always use IVF + 2-bit binary filter + INT8 re-rank.
    #[serde(default = "default_index_strategy")]
    pub index_strategy: String,

    /// Number of IVF partitions to probe at query time.
    /// Default 0 = auto-select (N / 10 where N = number of clusters).
    #[serde(default)]
    pub ivf_nprobe: usize,

    /// Oversample factor for 2-bit binary filter stage.
    /// Candidates = limit * oversample_factor, then re-ranked with INT8.
    /// Default 10.
    #[serde(default = "default_oversample_factor")]
    pub binary_oversample_factor: usize,
```

- [ ] Add default functions:

```rust
fn default_index_strategy() -> String {
    "auto".to_string()
}
fn default_oversample_factor() -> usize {
    10
}
```

- [ ] Update `Default` impl for `EmbeddingConfig`:

```rust
            index_strategy: default_index_strategy(),
            ivf_nprobe: 0,
            binary_oversample_factor: default_oversample_factor(),
```

- [ ] Run: `cargo test -p oneshim-core`
- [ ] Commit: `feat(core): add index_strategy, ivf_nprobe, binary_oversample_factor to EmbeddingConfig`

---

### Task 16: Create `AdaptiveSearchCoordinator`

**File:** `crates/oneshim-analysis/src/adaptive_search.rs` (NEW)

- [ ] Create the file with the coordinator struct:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use oneshim_core::error::CoreError;
use oneshim_core::models::embedding::{SearchFilters, SearchResult};
use oneshim_core::ports::vector_store::VectorStore;
use oneshim_core::ports::vector_index::VectorIndex;
use oneshim_core::quantization::{QuantizedVector, ScalarQuantizer};
use oneshim_core::binary_quantizer::BinaryQuantizer;

/// Search strategies selected by the coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStrategy {
    BruteForceInt8,
    IvfInt8,
    IvfBinaryRerank,
}

/// Configuration for the adaptive search coordinator.
pub struct SearchConfig {
    pub brute_force_threshold: u64,     // default: 10_000
    pub ivf_threshold: u64,             // default: 100_000
    pub oversample_factor: usize,       // default: 10
    pub default_nprobe: usize,          // default: 0 (auto)
    pub forced_strategy: Option<String>,// from config: "brute_force", "ivf", "ivf_binary", or None for "auto"
}

pub struct AdaptiveSearchCoordinator {
    vector_store: Arc<dyn VectorStore>,
    vector_index: Arc<dyn VectorIndex>,
    config: SearchConfig,
    /// Cached active vector count, refreshed periodically by the scheduler.
    /// `determine_strategy()` reads this atomically (sync, no await).
    cached_vector_count: AtomicU64,
}
```

- [ ] Implement `AdaptiveSearchCoordinator::new(vector_store, vector_index, config) -> Self`
  - Initialize `cached_vector_count` to `AtomicU64::new(0)`

- [ ] Implement `refresh_count(&self) -> Result<(), CoreError>` (async):
  - Calls `self.vector_store.count_active_vectors().await?`
  - Stores the result into `self.cached_vector_count` via `Ordering::Relaxed`
  - This method is called from the scheduler aggregate loop (not from the search hot path)

- [ ] Implement `determine_strategy(&self) -> SearchStrategy` (**sync, not async**):
  - If `config.forced_strategy` is Some: map to corresponding SearchStrategy
  - Else: read `self.cached_vector_count.load(Ordering::Relaxed)`, return:
    - < brute_force_threshold => BruteForceInt8
    - < ivf_threshold => IvfInt8
    - >= ivf_threshold => IvfBinaryRerank

- [ ] Implement `search(&self, query_f32: &[f32], limit: usize, time_decay_hours: f32, filters: &SearchFilters) -> Result<Vec<SearchResult>, CoreError>`:
  - Quantize query to INT8 via `ScalarQuantizer::quantize()`
  - Determine strategy via `determine_strategy()`
  - Match on strategy:
    - **BruteForceInt8**: delegate to `vector_store.search_quantized()`
    - **IvfInt8**: compute nprobe (auto or config), delegate to `vector_index.search_ivf()`
    - **IvfBinaryRerank**: encode query to BinaryCode via `BinaryQuantizer::encode()` (load thresholds from `vector_index.load_quantile_thresholds()`), delegate to `vector_index.search_ivf_binary()`
  - Log the selected strategy at debug level

---

### Task 17: Export `adaptive_search` module

**File:** `crates/oneshim-analysis/src/lib.rs`

- [ ] Add `pub mod adaptive_search;`
- [ ] Run: `cargo check -p oneshim-analysis`

---

### Task 18: Wire `AdaptiveSearchCoordinator` into `VectorRetriever`

**File:** `crates/oneshim-analysis/src/vector_retriever.rs`

- [ ] Add an optional `search_coordinator` field to `VectorRetriever`:

```rust
use crate::adaptive_search::AdaptiveSearchCoordinator;

pub struct VectorRetriever {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    vector_store: Arc<dyn VectorStore>,
    pii_filter: PiiFilter,
    max_results: usize,
    time_decay_hours: f32,
    quantization_enabled: bool,
    search_coordinator: Option<Arc<AdaptiveSearchCoordinator>>,
}
```

- [ ] Add a new constructor method (keep existing `new` unchanged for backward compat):

```rust
    pub fn with_coordinator(
        embedding_provider: Arc<dyn EmbeddingProvider>,
        vector_store: Arc<dyn VectorStore>,
        pii_filter: PiiFilter,
        max_results: usize,
        time_decay_hours: f32,
        quantization_enabled: bool,
        coordinator: Arc<AdaptiveSearchCoordinator>,
    ) -> Self {
        Self {
            embedding_provider,
            vector_store,
            pii_filter,
            max_results,
            time_decay_hours,
            quantization_enabled,
            search_coordinator: Some(coordinator),
        }
    }
```

- [ ] Update `new` to set `search_coordinator: None`

- [ ] In `retrieve_for_context`: before the existing quantization branch, check for coordinator:

```rust
        if let Some(ref coordinator) = self.search_coordinator {
            return coordinator
                .search(
                    &query_vector,
                    self.max_results,
                    self.time_decay_hours,
                    &SearchFilters::default(),
                )
                .await;
        }
        // ... existing quantization_enabled branch ...
```

- [ ] In `search_natural_language_with_context`: similarly, delegate to coordinator if present:

```rust
        if let Some(ref coordinator) = self.search_coordinator {
            let filters = filters.unwrap_or_default();
            return coordinator
                .search(&query_vector, self.max_results, self.time_decay_hours, &filters)
                .await;
        }
        // ... existing branches ...
```

- [ ] Verify existing tests still pass (they construct `VectorRetriever::new` which sets coordinator=None, so no behavior change)
- [ ] Run: `cargo test -p oneshim-analysis`
- [ ] Commit: `feat(analysis): add AdaptiveSearchCoordinator with auto strategy selection`

---

### Task 19: Unit tests for `AdaptiveSearchCoordinator`

**File:** `crates/oneshim-analysis/src/adaptive_search.rs`

- [ ] Add `#[cfg(test)] mod tests` with mock VectorStore and mock VectorIndex:

```
- strategy_auto_brute_force: cached_vector_count < 10K -> BruteForceInt8
- strategy_auto_ivf: 10K <= cached_vector_count < 100K -> IvfInt8
- strategy_auto_ivf_binary: cached_vector_count >= 100K -> IvfBinaryRerank
- strategy_forced_brute_force: forced_strategy="brute_force" overrides auto
- strategy_forced_ivf: forced_strategy="ivf" overrides auto
- strategy_forced_ivf_binary: forced_strategy="ivf_binary" overrides auto
- refresh_count_updates_atomic: call refresh_count, verify cached_vector_count reflects store
- search_delegates_to_brute_force: small count, verify vector_store.search_quantized called
- search_delegates_to_ivf: medium count, verify vector_index.search_ivf called
- search_delegates_to_ivf_binary: large count, verify vector_index.search_ivf_binary called
```

> **Note:** Tests for `determine_strategy` set `cached_vector_count` directly via `AtomicU64::store()` rather than calling the async `refresh_count()`, since `determine_strategy()` is sync.

- [ ] Run: `cargo test -p oneshim-analysis -- adaptive_search`
- [ ] Commit: `test(analysis): unit tests for AdaptiveSearchCoordinator strategy selection`

---

### Task 20: Background index maintenance loop

**File:** DI wiring location — `src-tauri/src/scheduler/` or `crates/oneshim-app/src/scheduler/` (whichever is the active scheduler)

- [ ] Add a new async function `index_maintenance_loop` (or add to existing aggregate loop):

```rust
async fn index_maintenance_step(
    vector_store: &Arc<dyn VectorStore>,
    vector_index: &Arc<dyn VectorIndex>,
    config: &EmbeddingConfig,
) -> Result<(), CoreError> {
    if config.index_strategy == "brute_force" {
        return Ok(());
    }

    let meta = vector_index.get_index_meta().await?;
    let total = meta.total_vector_count;

    if total < 10_000 {
        return Ok(()); // too small to index
    }

    let needs_rebuild = meta.ivf_built_at.is_none()
        || (meta.unindexed_count as f64 / total.max(1) as f64 > 0.10);

    if needs_rebuild {
        let n_clusters = (total as f64).sqrt() as usize;
        info!("Rebuilding IVF index: {total} vectors, {n_clusters} clusters");
        vector_index.build_ivf_index(n_clusters, 10).await?;

        if total > 100_000 {
            info!("Building binary codes for {total} vectors");
            vector_index.build_binary_codes().await?;
        }
    } else if meta.unindexed_count > 0 {
        // Incremental: assign new vectors + generate binary codes
        debug!("Incremental index update: {} unindexed vectors", meta.unindexed_count);
        // Load new vectors, call assign_to_cluster + store_binary_code for each
    }

    Ok(())
}
```

- [ ] Wire the maintenance step into the scheduler at a 5-minute interval (or piggyback on the existing aggregate loop at a reduced frequency)

- [ ] Call `coordinator.refresh_count().await` from the scheduler aggregate loop (same loop that calls `index_maintenance_step`). This keeps `cached_vector_count` fresh so `determine_strategy()` can stay sync on the search hot path.

- [ ] Ensure the maintenance runs on `tokio::task::spawn_blocking` or uses the existing `with_conn` pattern so it does not block the main scheduler

- [ ] Run: `cargo check --workspace`
- [ ] Commit: `feat(scheduler): add background index maintenance loop for IVF + binary codes`

---

## Phase C.5: Acceptance Testing (~3h)

### Task 21: Recall test

**File:** `crates/oneshim-storage/src/sqlite/vector_index_impl.rs` (or a new integration test file)

- [ ] Write a test that:
  1. Generates 1000 random 384-dim f32 vectors (use a seeded RNG for reproducibility)
  2. Stores all vectors with INT8 quantization
  3. Builds IVF index (n_clusters = 31 for sqrt(1000))
  4. Builds binary codes
  5. For 20 random query vectors:
     a. Run brute-force INT8 search (via `search_quantized`), collect top-10 segment_ids
     b. Run IVF INT8 search (via `search_ivf`), collect top-10 segment_ids
     c. Run IVF+binary search (via `search_ivf_binary`), collect top-10 segment_ids
     d. Compute recall = |intersection| / 10 for each indexed path vs brute-force
  6. Average recall across 20 queries must be >= 0.90 for IVF, >= 0.85 for IVF+binary
  7. *Note*: using 1000 vectors instead of 50K for CI speed. The spec targets >= 95% at 50K which should be higher than 1000.

- [ ] Run the recall test
- [ ] Commit: `test(storage): recall validation for IVF and IVF+binary vs brute-force`

---

### Task 22: Backward compatibility test

**File:** `crates/oneshim-analysis/src/adaptive_search.rs` (within existing tests)

- [ ] Add test `brute_force_config_skips_indexing`:
  - Set `forced_strategy = Some("brute_force".to_string())`
  - Verify `determine_strategy()` returns `BruteForceInt8` regardless of count
  - Verify `search()` delegates to `vector_store.search_quantized()`

- [ ] Add test `retriever_without_coordinator_works_unchanged`:
  - Construct `VectorRetriever::new(...)` (no coordinator)
  - Verify search works via existing brute-force path
  - This confirms no regression for users who have not enabled indexing

---

### Task 23: Full workspace build + lint

- [ ] Run: `cargo fmt --check`
- [ ] Run: `cargo clippy --workspace`
- [ ] Run: `cargo test --workspace`
- [ ] Verify test count increased by >= 30 new tests across oneshim-core, oneshim-storage, and oneshim-analysis
- [ ] Commit: `test: verify P3 Phase C — advanced compression and indexing pipeline`

---

## Exception handling notes

- **K-means convergence**: If a cluster becomes empty during Lloyd's iteration (all vectors migrated to other centroids), reassign the empty centroid to the vector furthest from its current centroid. This prevents degenerate clusters.
- **Hamming filter yields zero candidates**: If oversample produces fewer candidates than `limit` after Hamming filtering within the probed clusters, fall back to IVF-only search for that query (skip the binary filter, use INT8 brute-force within the probed clusters).
- **Quantile thresholds missing at query time**: If `load_quantile_thresholds()` returns None (binary codes not built yet), `AdaptiveSearchCoordinator` falls back from IvfBinaryRerank to IvfInt8 automatically.
- **SQLite lock contention during index build**: All batch operations (centroids, assignments, binary codes) use chunked transactions of 1000 rows with lock release between chunks. Maximum lock hold time is ~100 ms per chunk.
- **Memory during build at > 200K vectors**: The build loads all INT8 vectors into memory (~86 MB at 200K). For collections exceeding 200K, sample 200K vectors for clustering and assign the remainder post-build. This cap is enforced in `SqliteVectorIndex::build_ivf_index`.
- **WAL file growth**: Run `PRAGMA wal_checkpoint(TRUNCATE)` after bulk index builds to reclaim WAL space.
- **Config defaults**: All new config fields have safe defaults (`index_strategy: "auto"`, `ivf_nprobe: 0`, `binary_oversample_factor: 10`). No behavioral change for existing users until vector count exceeds 10K.
- **Existing mock VectorStore/VectorIndex impls**: All new trait methods have default implementations returning errors, so test mocks and existing code compile without modification.

## Deferred (not in this phase)

- **HNSW graph index**: Only needed if collections reach 1M+ vectors (unlikely for desktop 90-day retention). Would require ~500 MB in-memory graph.
- **Product quantization**: Higher compression than 2-bit but requires trained codebook. Revisit if storage exceeds 100 MB.
- **Explicit SIMD**: Current implementation relies on LLVM auto-vectorization. Manual SIMD via `std::simd` or `core_arch` would add ~2-3x speedup but requires platform-specific code. Consider only if latency targets are missed.
- **Incremental centroid update**: Online k-means to avoid full rebuild. Adds complexity. Implement only if rebuild frequency becomes a problem.
- **Latency/memory benchmarks at 200K scale**: Deferred to actual deployment validation. Unit tests use 1000 vectors for CI speed.
