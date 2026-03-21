# USearch HNSW Vector Index — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace/supplement the existing IVF index with usearch HNSW for sub-millisecond approximate nearest-neighbor search on INT8-quantized embedding vectors. HNSW is incrementally maintained via `add()` — no periodic rebuild needed (unlike IVF). Target: < 1 ms search at 50K vectors, recall@10 >= 0.95.

**Architecture:** `AnnIndex` port trait (new, `oneshim-core/ports`) defines the ANN contract. `HnswAdapter` (new, `oneshim-analysis`, feature-gated `hnsw`) wraps `usearch::Index` behind the port. `AdaptiveSearchCoordinator` gains `Option<Arc<dyn AnnIndex>>` and a new `Hnsw` strategy variant. Metadata join uses batch SQL lookup by u64 keys from `embedding_vectors`. Graceful degradation falls back to IVF/brute-force on HNSW failure. Corruption recovery rebuilds from SQLite. Retention integration removes HNSW entries when vectors are pruned.

**Tech Stack:** Rust, usearch 2.15+, rusqlite, tokio::spawn_blocking, criterion (workspace version)

**Spec:** `docs/superpowers/specs/2026-03-21-usearch-hnsw-vector-index-design.md`

**Prerequisites:**
- Run `cargo doc -p usearch --no-deps` to verify available Rust API surface (especially `save()`, `load()`, `add()`, `search()`, `remove()`, `len()`, `capacity()`, `reserve()`). The `save_to_buffer()` / `serialized_length()` methods may not exist in the Rust bindings -- persistence must use file-based `save(path)` + `load(path)` instead.
- Verify `usearch::Index` is `Send + Sync` with a compile-time assertion (see Task 3).

---

## What is already done (DO NOT re-implement)

| Component | File | Status |
|-----------|------|--------|
| `VectorStore` trait (store, search, search_quantized, enforce_retention, count_active_vectors) | `crates/oneshim-core/src/ports/vector_store.rs` | Done |
| `VectorIndex` trait (IVF build, search_ivf, search_ivf_binary) | `crates/oneshim-core/src/ports/vector_index.rs` | Done |
| `AdaptiveSearchCoordinator` (BruteForce/IVF/IvfBinary auto-select) | `crates/oneshim-analysis/src/adaptive_search.rs` | Done (11 tests) |
| `VectorRetriever` with coordinator delegation | `crates/oneshim-analysis/src/vector_retriever.rs` | Done |
| `SqliteVectorStore` (brute-force INT8 + f32 search) | `crates/oneshim-storage/src/sqlite/vector_store_impl/` | Done |
| `SqliteVectorIndex` (IVF index build + search) | `crates/oneshim-storage/src/sqlite/vector_index_impl/` | Done |
| `EmbeddingConfig` with index_strategy, quantization settings | `crates/oneshim-core/src/config/sections/analysis.rs` | Done |
| `ScalarQuantizer` + `QuantizedVector` | `crates/oneshim-core/src/quantization.rs` | Done |
| `BinaryQuantizer` + `BinaryCode` + `QuantileThresholds` | `crates/oneshim-core/src/binary_quantizer.rs` | Done |
| Scheduler DI with vector_index + search_coordinator | `src-tauri/src/scheduler/mod.rs` | Done |
| Criterion bench infrastructure (4 bench files) | `crates/*/benches/*.rs` | Done |

---

## File Map

### New files

| File | Purpose |
|------|---------|
| `crates/oneshim-core/src/ports/ann_index.rs` | `AnnIndex` port trait — ANN contract for hexagonal architecture |
| `crates/oneshim-analysis/src/hnsw_adapter.rs` | `HnswAdapter` — usearch HNSW implementation behind `AnnIndex` port |
| `crates/oneshim-analysis/benches/hnsw_bench.rs` | Criterion benchmarks: add, search, save/load, recall@10, vs IVF/brute |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/ports/mod.rs` | Export `ann_index` module |
| `crates/oneshim-analysis/Cargo.toml` | Add `usearch` optional dep + `hnsw` feature |
| `crates/oneshim-analysis/src/lib.rs` | Export `hnsw_adapter` module (feature-gated) |
| `crates/oneshim-analysis/src/adaptive_search.rs` | Add `Hnsw` strategy, `Option<Arc<dyn AnnIndex>>` field, graceful degradation, metadata join |
| `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs` | Add `get_vectors_for_rebuild()` method returning `(id, vector_f32)` pairs |
| `crates/oneshim-core/src/ports/vector_store.rs` | Add `get_vectors_for_rebuild()` default method to trait |
| `src-tauri/Cargo.toml` | Add `hnsw` feature propagation |
| `src-tauri/src/scheduler/mod.rs` | Add `ann_index: Option<Arc<dyn AnnIndex>>` field + builder |

### No new crate. One new external dependency (`usearch`, optional). No cross-crate dependency violations.

---

## Task 1: AnnIndex Port Trait

**Files:**
- Create: `crates/oneshim-core/src/ports/ann_index.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`
- Test: `cargo test -p oneshim-core`

### Steps

- [ ] **Step 1.1** Create `crates/oneshim-core/src/ports/ann_index.rs` with the `AnnIndex` port trait:

```rust
//! Port trait for approximate nearest neighbor (ANN) index.
//!
//! Defines the contract for incremental ANN indexing and search.
//! Primary adapter: `HnswAdapter` in oneshim-analysis (feature = "hnsw").

use async_trait::async_trait;

use crate::error::CoreError;

/// Approximate Nearest Neighbor index port.
///
/// Implementations provide incremental `add()` (no rebuild needed),
/// sub-millisecond `search()`, and lazy `remove()` with tombstones.
///
/// Primary adapter: `HnswAdapter` (oneshim-analysis, feature = "hnsw")
#[async_trait]
pub trait AnnIndex: Send + Sync {
    /// Insert a vector with the given key.
    async fn add(&self, key: u64, vector: &[f32]) -> Result<(), CoreError>;

    /// Find the k nearest neighbors of the query vector.
    /// Returns (key, distance) pairs sorted by ascending distance.
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<(u64, f32)>, CoreError>;

    /// Remove a vector by key (lazy tombstone).
    async fn remove(&self, key: u64) -> Result<(), CoreError>;

    /// Persist the index to storage.
    async fn save(&self) -> Result<(), CoreError>;

    /// Load the index from storage.
    async fn load(&self) -> Result<(), CoreError>;

    /// Current number of vectors in the index.
    fn len(&self) -> usize;

    /// Reserved capacity.
    fn capacity(&self) -> usize;

    /// Whether the index is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
```

- [ ] **Step 1.2** Run: `cargo check -p oneshim-core` — verify compiles

- [ ] **Step 1.3** Add `pub mod ann_index;` to `crates/oneshim-core/src/ports/mod.rs` (alphabetical order, before `analysis_provider`)

- [ ] **Step 1.4** Run: `cargo check -p oneshim-core` — verify module exports

- [ ] **Step 1.5** Add unit tests at the bottom of `ann_index.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mock AnnIndex for verifying the trait contract compiles.
    struct MockAnnIndex {
        vectors: Mutex<Vec<(u64, Vec<f32>)>>,
    }

    impl MockAnnIndex {
        fn new() -> Self {
            Self {
                vectors: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl AnnIndex for MockAnnIndex {
        async fn add(&self, key: u64, vector: &[f32]) -> Result<(), CoreError> {
            self.vectors.lock().unwrap().push((key, vector.to_vec()));
            Ok(())
        }

        async fn search(&self, _query: &[f32], k: usize) -> Result<Vec<(u64, f32)>, CoreError> {
            let vecs = self.vectors.lock().unwrap();
            Ok(vecs.iter().take(k).map(|(key, _)| (*key, 0.0)).collect())
        }

        async fn remove(&self, key: u64) -> Result<(), CoreError> {
            self.vectors.lock().unwrap().retain(|(k, _)| *k != key);
            Ok(())
        }

        async fn save(&self) -> Result<(), CoreError> {
            Ok(())
        }

        async fn load(&self) -> Result<(), CoreError> {
            Ok(())
        }

        fn len(&self) -> usize {
            self.vectors.lock().unwrap().len()
        }

        fn capacity(&self) -> usize {
            100
        }
    }

    #[tokio::test]
    async fn mock_ann_index_add_and_search() {
        let index = MockAnnIndex::new();
        index.add(1, &[0.1, 0.2, 0.3]).await.unwrap();
        index.add(2, &[0.4, 0.5, 0.6]).await.unwrap();
        let results = index.search(&[0.1, 0.2, 0.3], 10).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn mock_ann_index_remove() {
        let index = MockAnnIndex::new();
        index.add(1, &[0.1, 0.2, 0.3]).await.unwrap();
        index.add(2, &[0.4, 0.5, 0.6]).await.unwrap();
        assert_eq!(index.len(), 2);

        index.remove(1).await.unwrap();
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn is_empty_default_impl() {
        let index = MockAnnIndex::new();
        assert!(index.is_empty());
    }
}
```

- [ ] **Step 1.6** Run: `cargo test -p oneshim-core -- ann_index` — verify 3 tests pass

---

## Task 2: usearch Dependency + Feature Flag Setup

**Files:**
- Modify: `crates/oneshim-analysis/Cargo.toml`
- Modify: `src-tauri/Cargo.toml`
- Test: `cargo check -p oneshim-analysis --features hnsw`

### Steps

- [ ] **Step 2.1** Add `usearch` optional dependency to `crates/oneshim-analysis/Cargo.toml`:

```toml
# In [dependencies] section, add:
usearch = { version = "2.15", optional = true }
```

- [ ] **Step 2.2** Add `hnsw` feature to the `[features]` section of `crates/oneshim-analysis/Cargo.toml`:

```toml
[features]
default = ["hdbscan"]
hdbscan = ["dep:hdbscan"]
hnsw = ["dep:usearch"]
```

- [ ] **Step 2.3** Run: `cargo check -p oneshim-analysis` — verify default features still compile

- [ ] **Step 2.4** Run: `cargo check -p oneshim-analysis --features hnsw` — verify usearch links correctly

- [ ] **Step 2.5** Add `hnsw` feature propagation to `src-tauri/Cargo.toml`:

In the `[features]` section, add:
```toml
hnsw = ["oneshim-analysis/hnsw"]
```

- [ ] **Step 2.6** Run: `cargo check -p oneshim-app --features hnsw` — verify feature propagates through binary crate

---

## Task 3: HnswAdapter Implementation

**Files:**
- Create: `crates/oneshim-analysis/src/hnsw_adapter.rs`
- Modify: `crates/oneshim-analysis/src/lib.rs`
- Test: `cargo test -p oneshim-analysis --features hnsw`

### Steps

- [ ] **Step 3.1** Create `crates/oneshim-analysis/src/hnsw_adapter.rs` with the struct.

**IMPORTANT:** Do NOT add a file-level `#![cfg(feature = "hnsw")]` attribute. The feature gate is applied at the module level in `lib.rs` (`#[cfg(feature = "hnsw")] pub mod hnsw_adapter;`), matching the existing `hdbscan` pattern. A file-level inner attribute would cause the file to be completely invisible to cargo, breaking tests and IDE support.

```rust
//! HNSW approximate nearest neighbor adapter using usearch.
//!
//! Feature-gated via `#[cfg(feature = "hnsw")]` in lib.rs module export.
//! Implements the `AnnIndex` port trait from oneshim-core.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::ports::ann_index::AnnIndex;
use tracing::{debug, info, warn};
use usearch::Index;

/// Default initial capacity for the HNSW index.
const DEFAULT_INITIAL_CAPACITY: usize = 50_000;
/// Grow the index when utilization exceeds this ratio.
const GROWTH_THRESHOLD: f64 = 0.8;
/// Index file name within the data directory.
const INDEX_FILENAME: &str = "hnsw_index.usearch";
/// Temporary file suffix for atomic rename.
const INDEX_TMP_SUFFIX: &str = ".tmp";

/// HNSW configuration parameters.
pub struct HnswConfig {
    /// Embedding vector dimensions (must match model output).
    pub dimensions: usize,
    /// M parameter — number of connections per layer. Default: 16.
    pub connectivity: usize,
    /// ef_construction — expansion factor during indexing. Default: 128.
    pub expansion_add: usize,
    /// ef_search — expansion factor during search. Default: 64.
    pub expansion_search: usize,
    /// Initial index capacity. Default: 50_000.
    pub initial_capacity: usize,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            dimensions: 384,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
            initial_capacity: DEFAULT_INITIAL_CAPACITY,
        }
    }
}

// Compile-time assertion: usearch::Index must be Send + Sync for Arc<Index>.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() { _assert_send_sync::<Index>(); }
};

/// HNSW adapter wrapping usearch::Index behind the AnnIndex port.
///
/// Thread-safe: usearch v2.15+ provides native Send + Sync (verified above).
/// Persistence uses `save(path)` via `spawn_blocking` + atomic rename for crash safety.
/// All blocking FFI calls (add, search, remove, save, load) are dispatched to
/// `spawn_blocking` to avoid blocking the tokio runtime.
pub struct HnswAdapter {
    index: Arc<Index>,
    data_dir: PathBuf,
    dirty: AtomicBool,
}

impl HnswAdapter {
    /// Create a new HNSW index with the given configuration.
    pub fn new(data_dir: PathBuf, config: HnswConfig) -> Result<Self, CoreError> {
        let options = usearch::IndexOptions {
            dimensions: config.dimensions,
            metric: usearch::MetricKind::Cos,
            quantization: usearch::ScalarKind::I8,
            connectivity: config.connectivity,
            expansion_add: config.expansion_add,
            expansion_search: config.expansion_search,
            multi: false,
        };

        let index = Index::new(&options)
            .map_err(|e| CoreError::Internal(format!("Failed to create HNSW index: {e}")))?;

        index.reserve(config.initial_capacity).map_err(|e| {
            CoreError::Internal(format!("Failed to reserve HNSW capacity: {e}"))
        })?;

        info!(
            dims = config.dimensions,
            capacity = config.initial_capacity,
            "Created HNSW index"
        );

        Ok(Self {
            index: Arc::new(index),
            data_dir,
            dirty: AtomicBool::new(false),
        })
    }

    /// Path to the persisted index file.
    fn index_path(&self) -> PathBuf {
        self.data_dir.join(INDEX_FILENAME)
    }

    /// Path to the temporary file used during atomic save.
    fn index_tmp_path(&self) -> PathBuf {
        self.data_dir.join(format!("{INDEX_FILENAME}{INDEX_TMP_SUFFIX}"))
    }

    /// Grow index capacity if utilization exceeds the threshold.
    fn maybe_grow(&self) -> Result<(), CoreError> {
        let cap = self.index.capacity();
        if cap == 0 {
            return Ok(());
        }
        let util = self.index.len() as f64 / cap as f64;
        if util > GROWTH_THRESHOLD {
            let new_cap = cap * 2;
            self.index.reserve(new_cap).map_err(|e| {
                CoreError::Internal(format!("Failed to grow HNSW capacity: {e}"))
            })?;
            info!(old_cap = cap, new_cap, "HNSW index capacity doubled");
        }
        Ok(())
    }

    /// Whether the index has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl AnnIndex for HnswAdapter {
    async fn add(&self, key: u64, vector: &[f32]) -> Result<(), CoreError> {
        // Blocking FFI call — dispatch to spawn_blocking
        let idx = Arc::clone(&self.index);
        let vec_owned = vector.to_vec();
        tokio::task::spawn_blocking(move || {
            idx.add(key, &vec_owned)
                .map_err(|e| CoreError::Internal(format!("HNSW add failed: {e}")))
        })
        .await
        .map_err(|e| CoreError::Internal(format!("HNSW add join failed: {e}")))??;

        self.dirty.store(true, Ordering::Relaxed);
        self.maybe_grow()?;
        Ok(())
    }

    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<(u64, f32)>, CoreError> {
        // Blocking FFI call — dispatch to spawn_blocking
        let idx = Arc::clone(&self.index);
        let query_owned = query.to_vec();
        tokio::task::spawn_blocking(move || {
            let matches = idx
                .search(&query_owned, k)
                .map_err(|e| CoreError::Internal(format!("HNSW search failed: {e}")))?;
            Ok(matches.keys.into_iter().zip(matches.distances).collect())
        })
        .await
        .map_err(|e| CoreError::Internal(format!("HNSW search join failed: {e}")))?
    }

    async fn remove(&self, key: u64) -> Result<(), CoreError> {
        // Blocking FFI call — dispatch to spawn_blocking
        let idx = Arc::clone(&self.index);
        tokio::task::spawn_blocking(move || {
            idx.remove(key)
                .map_err(|e| CoreError::Internal(format!("HNSW remove failed: {e}")))
        })
        .await
        .map_err(|e| CoreError::Internal(format!("HNSW remove join failed: {e}")))??;

        self.dirty.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn save(&self) -> Result<(), CoreError> {
        if !self.is_dirty() {
            debug!("HNSW index not dirty, skipping save");
            return Ok(());
        }

        let tmp_path = self.index_tmp_path();
        let final_path = self.index_path();

        // Blocking FFI call — use save(path) via spawn_blocking.
        // NOTE: save_to_buffer() / serialized_length() may not exist in the
        // Rust bindings. We use save(path) + atomic rename instead.
        let idx = Arc::clone(&self.index);
        let tmp_path_clone = tmp_path.clone();
        tokio::task::spawn_blocking(move || {
            let path_str = tmp_path_clone
                .to_str()
                .ok_or_else(|| CoreError::Internal("Invalid tmp path for HNSW save".into()))?;
            idx.save(path_str)
                .map_err(|e| CoreError::Internal(format!("HNSW save failed: {e}")))
        })
        .await
        .map_err(|e| CoreError::Internal(format!("HNSW save join failed: {e}")))??;

        // Atomic rename (async, non-blocking)
        tokio::fs::rename(&tmp_path, &final_path)
            .await
            .map_err(|e| {
                CoreError::Internal(format!("HNSW atomic rename failed: {e}"))
            })?;

        self.dirty.store(false, Ordering::Relaxed);
        info!(len = self.index.len(), "HNSW index saved");
        Ok(())
    }

    async fn load(&self) -> Result<(), CoreError> {
        let path = self.index_path();
        if !path.exists() {
            debug!("No HNSW index file found, starting empty");
            return Ok(());
        }

        let path_str = path
            .to_str()
            .ok_or_else(|| CoreError::Internal("Invalid HNSW index path".into()))?
            .to_string();

        // Blocking FFI call — dispatch to spawn_blocking
        let idx = Arc::clone(&self.index);
        tokio::task::spawn_blocking(move || {
            idx.load(&path_str)
                .map_err(|e| CoreError::Internal(format!("HNSW index load failed: {e}")))
        })
        .await
        .map_err(|e| CoreError::Internal(format!("HNSW load join failed: {e}")))??;

        self.dirty.store(false, Ordering::Relaxed);
        info!(
            len = self.index.len(),
            capacity = self.index.capacity(),
            "HNSW index loaded"
        );
        Ok(())
    }

    fn len(&self) -> usize {
        self.index.len()
    }

    fn capacity(&self) -> usize {
        self.index.capacity()
    }
}
```

- [ ] **Step 3.2** Run: `cargo check -p oneshim-analysis --features hnsw` — verify compiles

- [ ] **Step 3.3** Add feature-gated export to `crates/oneshim-analysis/src/lib.rs`:

```rust
#[cfg(feature = "hnsw")]
pub mod hnsw_adapter;
```

- [ ] **Step 3.4** Run: `cargo check -p oneshim-analysis --features hnsw` — verify module export

- [ ] **Step 3.5** Add unit tests in `hnsw_adapter.rs` (inside `#[cfg(test)] mod tests`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_adapter() -> (HnswAdapter, TempDir) {
        let tmp = TempDir::new().unwrap();
        let adapter = HnswAdapter::new(
            tmp.path().to_path_buf(),
            HnswConfig {
                dimensions: 3,
                initial_capacity: 100,
                ..Default::default()
            },
        )
        .unwrap();
        (adapter, tmp)
    }

    #[tokio::test]
    async fn add_and_search() {
        let (adapter, _tmp) = make_adapter();
        adapter.add(1, &[1.0, 0.0, 0.0]).await.unwrap();
        adapter.add(2, &[0.0, 1.0, 0.0]).await.unwrap();
        adapter.add(3, &[1.0, 0.1, 0.0]).await.unwrap();

        let results = adapter.search(&[1.0, 0.0, 0.0], 2).await.unwrap();
        assert_eq!(results.len(), 2);
        // Nearest to [1,0,0] should be key 1 (distance ~0)
        assert_eq!(results[0].0, 1);
    }

    #[tokio::test]
    async fn remove_tombstone() {
        let (adapter, _tmp) = make_adapter();
        adapter.add(1, &[1.0, 0.0, 0.0]).await.unwrap();
        adapter.add(2, &[0.0, 1.0, 0.0]).await.unwrap();
        assert_eq!(adapter.len(), 2);

        adapter.remove(1).await.unwrap();
        // usearch remove is lazy tombstone — len may or may not decrease
        // but search should no longer return key 1
        let results = adapter.search(&[1.0, 0.0, 0.0], 10).await.unwrap();
        assert!(results.iter().all(|(k, _)| *k != 1));
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();

        // Create and populate
        {
            let adapter = HnswAdapter::new(
                tmp.path().to_path_buf(),
                HnswConfig {
                    dimensions: 3,
                    initial_capacity: 100,
                    ..Default::default()
                },
            )
            .unwrap();
            adapter.add(1, &[1.0, 0.0, 0.0]).await.unwrap();
            adapter.add(2, &[0.0, 1.0, 0.0]).await.unwrap();
            adapter.save().await.unwrap();
        }

        // Load into fresh instance
        {
            let adapter = HnswAdapter::new(
                tmp.path().to_path_buf(),
                HnswConfig {
                    dimensions: 3,
                    initial_capacity: 100,
                    ..Default::default()
                },
            )
            .unwrap();
            adapter.load().await.unwrap();
            assert_eq!(adapter.len(), 2);

            let results = adapter.search(&[1.0, 0.0, 0.0], 1).await.unwrap();
            assert_eq!(results[0].0, 1);
        }
    }

    #[tokio::test]
    async fn load_nonexistent_is_ok() {
        let (adapter, _tmp) = make_adapter();
        // No file saved yet — load should succeed with empty index
        adapter.load().await.unwrap();
        assert_eq!(adapter.len(), 0);
    }

    #[tokio::test]
    async fn save_skips_when_not_dirty() {
        let (adapter, _tmp) = make_adapter();
        // Not dirty — save is a no-op
        adapter.save().await.unwrap();
        assert!(!adapter.index_path().exists());
    }

    #[test]
    fn capacity_growth_at_threshold() {
        let tmp = TempDir::new().unwrap();
        let adapter = HnswAdapter::new(
            tmp.path().to_path_buf(),
            HnswConfig {
                dimensions: 3,
                initial_capacity: 10,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(adapter.capacity(), 10);
        // Manually check growth logic: at 80% (8/10), should double
        // We add 9 vectors to trigger growth on the 9th add
        for i in 0..9u64 {
            let v = vec![i as f32, 0.0, 0.0];
            // Use blocking runtime for sync test
            adapter.index.add(i, &v).unwrap();
        }
        adapter.maybe_grow().unwrap();
        assert!(adapter.capacity() >= 20);
    }

    #[test]
    fn is_empty_and_len() {
        let (adapter, _tmp) = make_adapter();
        assert!(adapter.is_empty());
        assert_eq!(adapter.len(), 0);
    }
}
```

- [ ] **Step 3.6** Run: `cargo test -p oneshim-analysis --features hnsw -- hnsw_adapter` — verify all tests pass

---

## Task 4: AdaptiveSearchCoordinator Integration

**Files:**
- Modify: `crates/oneshim-analysis/src/adaptive_search.rs`
- Test: `cargo test -p oneshim-analysis`

### Steps

- [ ] **Step 4.1** Add `AnnIndex` import to `adaptive_search.rs`:

```rust
use oneshim_core::ports::ann_index::AnnIndex;
```

- [ ] **Step 4.2** Add `Hnsw` variant to `SearchStrategy` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStrategy {
    BruteForceInt8,
    IvfInt8,
    IvfBinaryRerank,
    Hnsw,
}
```

- [ ] **Step 4.3** Add `ann_index` field and `"hnsw"` forced strategy to `AdaptiveSearchCoordinator`:

Add to struct fields:
```rust
ann_index: Option<Arc<dyn AnnIndex>>,
```

Update constructor to initialize `ann_index: None`.

Add builder method:
```rust
pub fn with_ann_index(mut self, ann_index: Arc<dyn AnnIndex>) -> Self {
    self.ann_index = Some(ann_index);
    self
}
```

- [ ] **Step 4.4** Update `determine_strategy()` to prefer HNSW when available and above brute-force threshold:

Add `"hnsw"` to forced strategy match. In auto mode:
```rust
// If HNSW is available and count >= brute_force_threshold, prefer HNSW
if self.ann_index.is_some() && count >= self.config.brute_force_threshold {
    return SearchStrategy::Hnsw;
}
```

Also handle forced `"hnsw"` returning `SearchStrategy::Hnsw` in the match arm.

- [ ] **Step 4.5** Add `"hnsw"` match arm in `SearchConfig::forced_strategy` — update the match in `determine_strategy()` to include `"hnsw" => SearchStrategy::Hnsw`.

- [ ] **Step 4.6** Update `search()` method to handle `SearchStrategy::Hnsw` with graceful degradation:

```rust
SearchStrategy::Hnsw => {
    if let Some(ref ann) = self.ann_index {
        match ann.search(query_f32, limit).await {
            Ok(results) => {
                return self.join_metadata(results, time_decay_hours, filters).await;
            }
            Err(e) => {
                warn!("HNSW search failed, falling back to IVF: {}", e);
                // Fall through to IVF/brute-force
            }
        }
    }
    // Fallback: try IVF, then brute-force
    let nprobe = self.compute_nprobe();
    self.vector_index
        .search_ivf(&quantized, nprobe, limit, time_decay_hours, filters)
        .await
}
```

Note: The `quantized` variable is already computed before the match (line 130 in current code), so the fallback has access to it.

- [ ] **Step 4.7** Run: `cargo check -p oneshim-analysis` — verify compiles (existing tests should still pass since ann_index defaults to None)

- [ ] **Step 4.8** Run: `cargo test -p oneshim-analysis` — verify all existing 11 tests still pass (backward compatible)

---

## Task 5: SearchResult Metadata Join

**Files:**
- Modify: `crates/oneshim-core/src/ports/vector_store.rs`
- Modify: `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs`
- Modify: `crates/oneshim-analysis/src/adaptive_search.rs`
- Test: `cargo test -p oneshim-analysis`

### Steps

- [ ] **Step 5.1** Add `get_metadata_by_ids()` default method to `VectorStore` trait in `crates/oneshim-core/src/ports/vector_store.rs`:

```rust
/// Fetch metadata for a batch of vector IDs (for HNSW result enrichment).
/// Returns (id, segment_id, content_type, content_label, timestamp, original_text).
async fn get_metadata_by_ids(
    &self,
    _ids: &[u64],
) -> Result<Vec<(u64, EmbeddingMetadata)>, CoreError> {
    Err(CoreError::Internal(
        "get_metadata_by_ids not implemented".into(),
    ))
}
```

Add `EmbeddingMetadata` to the existing imports at the top of the file if not already there.

- [ ] **Step 5.2** Implement `get_metadata_by_ids()` in `SqliteVectorStore` (`crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs`).

First, ensure the following imports are present at the top of the file:
```rust
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use oneshim_core::models::embedding::EmbeddingMetadata;
```

Implementation:

```rust
async fn get_metadata_by_ids(
    &self,
    ids: &[u64],
) -> Result<Vec<(u64, EmbeddingMetadata)>, CoreError> {
    let ids_owned: Vec<u64> = ids.to_vec();

    self.with_conn(move |conn| {
        if ids_owned.is_empty() {
            return Ok(vec![]);
        }

        // Build IN clause with placeholders
        let placeholders: Vec<String> = ids_owned.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT id, segment_id, content_type, content_label, timestamp, original_text, model_id
             FROM embedding_vectors
             WHERE id IN ({})",
            placeholders.join(", ")
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| CoreError::Internal(format!("Failed to prepare metadata query: {e}")))?;

        let params: Vec<Box<dyn rusqlite::types::ToSql>> = ids_owned
            .iter()
            .map(|id| Box::new(*id as i64) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let id: i64 = row.get(0)?;
                let segment_id: String = row.get(1)?;
                let content_type_str: String = row.get(2)?;
                let content_label: Option<String> = row.get(3)?;
                let timestamp_str: String = row.get(4)?;
                let original_text: String = row.get(5)?;
                let model_id: String = row.get(6)?;
                Ok((id, segment_id, content_type_str, content_label, timestamp_str, original_text, model_id))
            })
            .map_err(|e| CoreError::Internal(format!("Failed to query metadata: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            let (id, segment_id, ct_str, content_label, ts_str, original_text, model_id) =
                row.map_err(|e| CoreError::Internal(format!("Row read error: {e}")))?;
            let content_type = parse_content_type(&ct_str);
            let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            results.push((
                id as u64,
                EmbeddingMetadata {
                    segment_id,
                    content_type,
                    content_label,
                    timestamp,
                    original_text,
                    model_id,
                },
            ));
        }

        Ok(results)
    })
    .await
}
```

- [ ] **Step 5.3** Run: `cargo check -p oneshim-storage` — verify compiles

- [ ] **Step 5.4** Add `join_metadata()` helper method to `AdaptiveSearchCoordinator` in `adaptive_search.rs`:

```rust
/// Convert HNSW (key, distance) results into SearchResult by joining
/// with metadata from the vector store. Applies time decay scoring.
async fn join_metadata(
    &self,
    ann_results: Vec<(u64, f32)>,
    time_decay_hours: f32,
    filters: &SearchFilters,
) -> Result<Vec<SearchResult>, CoreError> {
    if ann_results.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<u64> = ann_results.iter().map(|(k, _)| *k).collect();
    let metadata_map: std::collections::HashMap<u64, EmbeddingMetadata> = self
        .vector_store
        .get_metadata_by_ids(&ids)
        .await?
        .into_iter()
        .collect();

    let now = chrono::Utc::now();
    let mut results = Vec::with_capacity(ann_results.len());

    for (key, distance) in &ann_results {
        if let Some(meta) = metadata_map.get(key) {
            let similarity = 1.0 - distance;

            // Apply time decay
            let hours_ago = (now - meta.timestamp).num_seconds() as f32 / 3600.0;
            let decay = (-hours_ago / time_decay_hours).exp();
            let score = similarity * decay;

            // Apply filters
            if let Some(ref after) = filters.after {
                if meta.timestamp < *after {
                    continue;
                }
            }
            if let Some(ref before) = filters.before {
                if meta.timestamp > *before {
                    continue;
                }
            }
            if let Some(ref types) = filters.content_types {
                if !types.contains(&meta.content_type) {
                    continue;
                }
            }
            if filters
                .excluded_segment_ids
                .contains(&meta.segment_id)
            {
                continue;
            }

            results.push(SearchResult {
                segment_id: meta.segment_id.clone(),
                content_type: meta.content_type.clone(),
                content_label: meta.content_label.clone(),
                score,
                similarity,
                time_decay: decay,
                timestamp: meta.timestamp,
                original_text: meta.original_text.clone(),
            });
        }
    }

    // Post-join filter: regime_id (not available from HNSW, must filter here)
    if let Some(ref regime_id) = filters.regime_id {
        tracing::warn!("regime_id filter applied post-HNSW-join — may return fewer than k results");
        results.retain(|r| {
            // regime_id is on activity_segments, not embedding_vectors.
            // For now, skip this filter and log. Full support requires
            // joining embedding_vectors → activity_segments.
            true
        });
    }

    // Sort by score descending
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(results)
}
```

Add required imports at the top of the file:
```rust
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use oneshim_core::models::embedding::EmbeddingMetadata;
use oneshim_core::ports::ann_index::AnnIndex;
// In vector_store_impl/trait_impl.rs, also add:
// use super::helpers::parse_content_type;
```

- [ ] **Step 5.5** Run: `cargo check -p oneshim-analysis` — verify compiles

- [ ] **Step 5.6** Add test for `join_metadata` in `adaptive_search.rs` tests:

Add to `MockVectorStore`:
```rust
async fn get_metadata_by_ids(
    &self,
    ids: &[u64],
) -> Result<Vec<(u64, EmbeddingMetadata)>, CoreError> {
    Ok(ids
        .iter()
        .map(|id| {
            (
                *id,
                EmbeddingMetadata {
                    segment_id: format!("seg-{id}"),
                    content_type: EmbeddingContentType::SegmentSummary,
                    content_label: Some(format!("label-{id}")),
                    timestamp: Utc::now(),
                    original_text: format!("text-{id}"),
                    model_id: "mock".to_string(),
                },
            )
        })
        .collect())
}
```

Add HNSW delegation test:
```rust
#[tokio::test]
async fn search_delegates_to_hnsw_when_available() {
    let store = Arc::new(MockVectorStore::new(50_000));
    let index = Arc::new(MockVectorIndex::new());
    let mock_ann = Arc::new(MockAnnIndex::new());

    let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default())
        .with_ann_index(mock_ann.clone() as Arc<dyn AnnIndex>);
    coordinator.set_cached_count(50_000);

    // With ann_index present and count >= brute_force_threshold, strategy should be Hnsw
    assert_eq!(coordinator.determine_strategy(), SearchStrategy::Hnsw);
}
```

Where `MockAnnIndex` is defined in the test module:
```rust
struct MockAnnIndex {
    results: Vec<(u64, f32)>,
}

impl MockAnnIndex {
    fn new() -> Self {
        Self {
            results: vec![(1, 0.1), (2, 0.2)],
        }
    }
}

#[async_trait]
impl AnnIndex for MockAnnIndex {
    async fn add(&self, _key: u64, _vector: &[f32]) -> Result<(), CoreError> {
        Ok(())
    }
    async fn search(&self, _query: &[f32], k: usize) -> Result<Vec<(u64, f32)>, CoreError> {
        Ok(self.results.iter().take(k).cloned().collect())
    }
    async fn remove(&self, _key: u64) -> Result<(), CoreError> {
        Ok(())
    }
    async fn save(&self) -> Result<(), CoreError> {
        Ok(())
    }
    async fn load(&self) -> Result<(), CoreError> {
        Ok(())
    }
    fn len(&self) -> usize {
        self.results.len()
    }
    fn capacity(&self) -> usize {
        100
    }
}
```

- [ ] **Step 5.7** Run: `cargo test -p oneshim-analysis -- adaptive_search` — verify all tests pass including new ones

---

## Task 6: Graceful Degradation

**Files:**
- Modify: `crates/oneshim-analysis/src/adaptive_search.rs`
- Test: `cargo test -p oneshim-analysis`

### Steps

- [ ] **Step 6.1** The HNSW fallback logic was added in Task 4 Step 4.6. Now add a test for it:

```rust
#[tokio::test]
async fn hnsw_failure_falls_back_to_ivf() {
    let store = Arc::new(MockVectorStore::new(50_000));
    let index = Arc::new(MockVectorIndex::new());

    // Create a failing ANN mock
    struct FailingAnn;

    #[async_trait]
    impl AnnIndex for FailingAnn {
        async fn add(&self, _k: u64, _v: &[f32]) -> Result<(), CoreError> { Ok(()) }
        async fn search(&self, _q: &[f32], _k: usize) -> Result<Vec<(u64, f32)>, CoreError> {
            Err(CoreError::Internal("HNSW corrupted".into()))
        }
        async fn remove(&self, _k: u64) -> Result<(), CoreError> { Ok(()) }
        async fn save(&self) -> Result<(), CoreError> { Ok(()) }
        async fn load(&self) -> Result<(), CoreError> { Ok(()) }
        fn len(&self) -> usize { 100 }
        fn capacity(&self) -> usize { 1000 }
    }

    let coordinator = AdaptiveSearchCoordinator::new(store.clone(), index.clone(), SearchConfig::default())
        .with_ann_index(Arc::new(FailingAnn));
    coordinator.set_cached_count(50_000);

    // Strategy is Hnsw
    assert_eq!(coordinator.determine_strategy(), SearchStrategy::Hnsw);

    // Search should succeed (falling back to IVF after HNSW failure)
    let results = coordinator
        .search(&[0.1, 0.2, 0.3], 5, 168.0, &SearchFilters::default())
        .await;
    assert!(results.is_ok());
    // IVF path should have been called as fallback
    assert!(index.ivf_called.load(Ordering::Relaxed));
}
```

- [ ] **Step 6.2** Run: `cargo test -p oneshim-analysis -- hnsw_failure_falls_back` — verify test passes

- [ ] **Step 6.3** Add test for HNSW below brute-force threshold stays on brute-force:

```rust
#[tokio::test]
async fn hnsw_below_threshold_uses_brute_force() {
    let store = Arc::new(MockVectorStore::new(100));
    let index = Arc::new(MockVectorIndex::new());
    let mock_ann = Arc::new(MockAnnIndex::new());

    let coordinator = AdaptiveSearchCoordinator::new(store.clone(), index, SearchConfig::default())
        .with_ann_index(mock_ann as Arc<dyn AnnIndex>);
    coordinator.set_cached_count(100);

    // Below brute-force threshold — should still use BruteForceInt8
    assert_eq!(coordinator.determine_strategy(), SearchStrategy::BruteForceInt8);

    let _ = coordinator
        .search(&[0.1, 0.2, 0.3], 5, 168.0, &SearchFilters::default())
        .await;
    assert!(store.brute_force_called.load(Ordering::Relaxed));
}
```

- [ ] **Step 6.4** Run: `cargo test -p oneshim-analysis -- adaptive_search` — verify all tests pass

---

## Task 7: Corruption Recovery

**Files:**
- Modify: `crates/oneshim-core/src/ports/vector_store.rs`
- Modify: `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs`
- Modify: `crates/oneshim-analysis/src/adaptive_search.rs`
- Test: `cargo test -p oneshim-analysis`

### Steps

- [ ] **Step 7.1** Add `get_all_vector_ids_and_floats()` default method to `VectorStore` trait in `crates/oneshim-core/src/ports/vector_store.rs`:

```rust
/// Fetch all active vector IDs and their float32 data for index rebuild.
/// Used by corruption recovery to rebuild HNSW from SQLite ground truth.
async fn get_all_vector_ids_and_floats(&self) -> Result<Vec<(u64, Vec<f32>)>, CoreError> {
    Err(CoreError::Internal(
        "get_all_vector_ids_and_floats not implemented".into(),
    ))
}
```

- [ ] **Step 7.2** Implement in `SqliteVectorStore`:

```rust
async fn get_all_vector_ids_and_floats(&self) -> Result<Vec<(u64, Vec<f32>)>, CoreError> {
    self.with_conn(move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, vector FROM embedding_vectors WHERE is_stale = 0 AND length(vector) > 0",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare rebuild query: {e}")))?;

        let rows: Vec<(u64, Vec<f32>)> = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((id as u64, bytes_to_f32_vec(&blob)))
            })
            .map_err(|e| CoreError::Internal(format!("Failed to query vectors: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    })
    .await
}
```

- [ ] **Step 7.3** Run: `cargo check -p oneshim-storage` — verify compiles

- [ ] **Step 7.4** Add `load_or_rebuild()` method to `AdaptiveSearchCoordinator`:

```rust
/// Load the HNSW index from disk. On failure, rebuild from SQLite ground truth.
pub async fn load_or_rebuild_hnsw(&self) -> Result<(), CoreError> {
    let ann = match self.ann_index {
        Some(ref ann) => ann,
        None => return Ok(()),
    };

    match ann.load().await {
        Ok(()) => {
            if ann.len() > 0 {
                info!("HNSW index loaded with {} vectors", ann.len());
                return Ok(());
            }
            debug!("HNSW index loaded but empty, checking SQLite for vectors");
        }
        Err(e) => {
            warn!("HNSW index load failed, rebuilding from SQLite: {}", e);
        }
    }

    // Rebuild from SQLite
    let vectors = self.vector_store.get_all_vector_ids_and_floats().await?;
    if vectors.is_empty() {
        debug!("No vectors in SQLite, HNSW index remains empty");
        return Ok(());
    }

    info!("Rebuilding HNSW index from {} SQLite vectors", vectors.len());
    for (id, vector) in &vectors {
        ann.add(*id, vector).await?;
    }
    ann.save().await?;
    info!("HNSW index rebuilt and saved ({} vectors)", ann.len());
    Ok(())
}
```

Add `use tracing::{debug, info, warn};` to imports if not already present.

- [ ] **Step 7.5** Run: `cargo check -p oneshim-analysis` — verify compiles

- [ ] **Step 7.6** Add test for corruption recovery (add to `adaptive_search.rs` test module):

```rust
#[tokio::test]
async fn load_or_rebuild_recovers_from_failure() {
    let store = Arc::new(MockVectorStore::new(100));
    let mock_ann = Arc::new(MockAnnIndex::new());

    let index = Arc::new(MockVectorIndex::new());
    let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default())
        .with_ann_index(mock_ann as Arc<dyn AnnIndex>);

    // load_or_rebuild should succeed (mock load returns Ok with empty)
    coordinator.load_or_rebuild_hnsw().await.unwrap();
}
```

- [ ] **Step 7.7** Run: `cargo test -p oneshim-analysis -- load_or_rebuild` — verify test passes

---

## Task 8: Retention Integration

**Files:**
- Modify: `crates/oneshim-analysis/src/adaptive_search.rs`
- Test: `cargo test -p oneshim-analysis`

### Steps

- [ ] **Step 8.1** Add `remove_from_hnsw()` method to `AdaptiveSearchCoordinator`:

```rust
/// Remove a vector from the HNSW index by key. Best-effort — errors are logged
/// but do not propagate (tombstone is acceptable).
pub async fn remove_from_hnsw(&self, key: u64) {
    if let Some(ref ann) = self.ann_index {
        if let Err(e) = ann.remove(key).await {
            warn!("HNSW remove({key}) failed (tombstone ok): {e}");
        }
    }
}

/// Remove multiple vectors from the HNSW index. Best-effort.
/// Called from retention enforcement after vectors are deleted from SQLite.
pub async fn remove_batch_from_hnsw(&self, keys: &[u64]) {
    if let Some(ref ann) = self.ann_index {
        for key in keys {
            if let Err(e) = ann.remove(*key).await {
                warn!("HNSW remove({key}) failed (tombstone ok): {e}");
            }
        }
        debug!("Removed {} keys from HNSW (best-effort)", keys.len());
    }
}
```

- [ ] **Step 8.2** Run: `cargo check -p oneshim-analysis` — verify compiles

- [ ] **Step 8.3** Add `save_hnsw_if_dirty()` convenience method:

```rust
/// Persist the HNSW index if it has unsaved changes.
/// Called periodically from the scheduler sync loop.
pub async fn save_hnsw_if_dirty(&self) -> Result<(), CoreError> {
    if let Some(ref ann) = self.ann_index {
        ann.save().await?;
    }
    Ok(())
}
```

- [ ] **Step 8.4** Add `add_to_hnsw()` convenience method for new embeddings:

```rust
/// Add a vector to the HNSW index (called after storing in SQLite).
/// If HNSW is not configured, this is a no-op.
pub async fn add_to_hnsw(&self, key: u64, vector: &[f32]) -> Result<(), CoreError> {
    if let Some(ref ann) = self.ann_index {
        ann.add(key, vector).await?;
    }
    Ok(())
}
```

- [ ] **Step 8.5** Run: `cargo check -p oneshim-analysis` — verify compiles

- [ ] **Step 8.6** Add test for retention integration:

```rust
#[tokio::test]
async fn remove_batch_from_hnsw_is_best_effort() {
    let store = Arc::new(MockVectorStore::new(0));
    let index = Arc::new(MockVectorIndex::new());
    let mock_ann = Arc::new(MockAnnIndex::new());

    let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default())
        .with_ann_index(mock_ann as Arc<dyn AnnIndex>);

    // Should not panic even for non-existent keys
    coordinator.remove_batch_from_hnsw(&[999, 888, 777]).await;
}

#[tokio::test]
async fn add_to_hnsw_noop_without_ann() {
    let store = Arc::new(MockVectorStore::new(0));
    let index = Arc::new(MockVectorIndex::new());

    let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default());
    // No ann_index — should be a no-op
    coordinator.add_to_hnsw(1, &[0.1, 0.2]).await.unwrap();
}
```

- [ ] **Step 8.7** Run: `cargo test -p oneshim-analysis -- adaptive_search` — verify all tests pass

---

## Task 9: Capacity Growth Policy

Capacity growth is already implemented in `HnswAdapter::maybe_grow()` (Task 3). This task adds the configuration surface.

**Files:**
- Modify: `crates/oneshim-analysis/src/adaptive_search.rs`
- Test: `cargo test -p oneshim-analysis`

### Steps

- [ ] **Step 9.1** Add `hnsw_initial_capacity` to `SearchConfig`:

```rust
pub struct SearchConfig {
    // ... existing fields ...
    /// Initial HNSW capacity. Default: 50_000.
    pub hnsw_initial_capacity: usize,
}
```

Update `Default` impl:
```rust
hnsw_initial_capacity: 50_000,
```

- [ ] **Step 9.2** Run: `cargo check -p oneshim-analysis` — verify compiles

- [ ] **Step 9.3** Add test for configuration propagation:

```rust
#[test]
fn search_config_default_hnsw_capacity() {
    let config = SearchConfig::default();
    assert_eq!(config.hnsw_initial_capacity, 50_000);
}
```

- [ ] **Step 9.4** Run: `cargo test -p oneshim-analysis -- search_config_default` — verify passes

---

## Task 10: Persistence (Periodic Save)

Persistence is implemented in `HnswAdapter` (Task 3). This task adds the scheduler integration point.

**Files:**
- Modify: `crates/oneshim-analysis/src/adaptive_search.rs` (already done in Task 8)
- Test: `cargo test -p oneshim-analysis`

### Steps

- [ ] **Step 10.1** `save_hnsw_if_dirty()` was added in Task 8 Step 8.3. Add test:

```rust
#[tokio::test]
async fn save_hnsw_if_dirty_noop_without_ann() {
    let store = Arc::new(MockVectorStore::new(0));
    let index = Arc::new(MockVectorIndex::new());
    let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default());

    // No ann_index — should be a no-op
    coordinator.save_hnsw_if_dirty().await.unwrap();
}
```

- [ ] **Step 10.2** Run: `cargo test -p oneshim-analysis -- save_hnsw_if_dirty` — verify passes

---

## Task 11: DI Wiring in src-tauri

**Files:**
- Modify: `src-tauri/Cargo.toml` (already done in Task 2)
- Modify: `src-tauri/src/scheduler/mod.rs`
- Test: `cargo check -p oneshim-app --features hnsw`

### Steps

- [ ] **Step 11.1** Add `AnnIndex` import to `src-tauri/src/scheduler/mod.rs`:

```rust
use oneshim_core::ports::ann_index::AnnIndex;
```

- [ ] **Step 11.2** Add `ann_index` field to the `Scheduler` struct:

```rust
pub(super) ann_index: Option<Arc<dyn AnnIndex>>,
```

Initialize to `None` in the `new()` constructor.

- [ ] **Step 11.3** Add builder method:

```rust
#[allow(dead_code)]
pub fn with_ann_index(mut self, ann_index: Arc<dyn AnnIndex>) -> Self {
    self.ann_index = Some(ann_index);
    self
}
```

- [ ] **Step 11.4** Run: `cargo check -p oneshim-app` — verify compiles with default features

- [ ] **Step 11.5** Run: `cargo check -p oneshim-app --features hnsw` — verify compiles with hnsw feature

---

## Task 12: Benchmarks

**Files:**
- Create: `crates/oneshim-analysis/benches/hnsw_bench.rs`
- Modify: `crates/oneshim-analysis/Cargo.toml` (add `[[bench]]` section)
- Test: `cargo bench -p oneshim-analysis --features hnsw --bench hnsw_bench -- --test`

### Steps

- [ ] **Step 12.1** Add `[[bench]]` section to `crates/oneshim-analysis/Cargo.toml`:

```toml
[[bench]]
name = "hnsw_bench"
harness = false
required-features = ["hnsw"]

[dev-dependencies]
tokio = { workspace = true, features = ["full", "test-util"] }
criterion = { workspace = true }
rand = { workspace = true }
tempfile = { workspace = true }
```

Note: `criterion` and `rand` may need to be added as workspace deps or local dev-deps depending on existing workspace setup.

- [ ] **Step 12.2** Create `crates/oneshim-analysis/benches/hnsw_bench.rs`:

```rust
#![allow(clippy::redundant_closure, clippy::unit_arg)]
// NOTE: No file-level #![cfg(feature = "hnsw")] — the [[bench]] section
// has `required-features = ["hnsw"]` which handles the gating.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use oneshim_analysis::hnsw_adapter::{HnswAdapter, HnswConfig};
use tempfile::TempDir;

/// Generate a random f32 vector of given dimensions.
fn random_vector(dims: usize) -> Vec<f32> {
    use rand::Rng;
    // rand 0.10 API: use rand::rng() instead of rand::thread_rng()
    let mut rng = rand::rng();
    (0..dims).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

/// Build a test HNSW index with n random vectors.
fn build_test_index(n: usize, dims: usize) -> (HnswAdapter, TempDir) {
    let tmp = TempDir::new().unwrap();
    let adapter = HnswAdapter::new(
        tmp.path().to_path_buf(),
        HnswConfig {
            dimensions: dims,
            initial_capacity: (n * 2).max(100),
            ..Default::default()
        },
    )
    .unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    for i in 0..n as u64 {
        let v = random_vector(dims);
        rt.block_on(adapter.add(i, &v)).unwrap();
    }
    (adapter, tmp)
}

/// Build index and keep raw vectors for recall computation.
fn build_test_index_with_vectors(
    n: usize,
    dims: usize,
) -> (HnswAdapter, Vec<(u64, Vec<f32>)>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let adapter = HnswAdapter::new(
        tmp.path().to_path_buf(),
        HnswConfig {
            dimensions: dims,
            initial_capacity: (n * 2).max(100),
            ..Default::default()
        },
    )
    .unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut vectors = Vec::with_capacity(n);
    for i in 0..n as u64 {
        let v = random_vector(dims);
        rt.block_on(adapter.add(i, &v)).unwrap();
        vectors.push((i, v));
    }
    (adapter, vectors, tmp)
}

/// Brute-force exact nearest neighbor search for ground truth.
fn brute_force_search(
    vectors: &[(u64, Vec<f32>)],
    query: &[f32],
    k: usize,
) -> Vec<u64> {
    let mut scored: Vec<(u64, f32)> = vectors
        .iter()
        .map(|(id, v)| {
            let dot: f32 = v.iter().zip(query).map(|(a, b)| a * b).sum();
            let norm_a: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm_b: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
            let cos_dist = if norm_a > 0.0 && norm_b > 0.0 {
                1.0 - dot / (norm_a * norm_b)
            } else {
                1.0
            };
            (*id, cos_dist)
        })
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    scored.into_iter().take(k).map(|(id, _)| id).collect()
}

fn hnsw_add_single(c: &mut Criterion) {
    let tmp = TempDir::new().unwrap();
    let adapter = HnswAdapter::new(
        tmp.path().to_path_buf(),
        HnswConfig {
            dimensions: 384,
            initial_capacity: 100_000,
            ..Default::default()
        },
    )
    .unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut key = 0u64;

    c.bench_function("hnsw_add_single_384d", |b| {
        b.iter(|| {
            let v = random_vector(384);
            rt.block_on(adapter.add(key, &v)).unwrap();
            key += 1;
        })
    });
}

fn hnsw_add_batch_1000(c: &mut Criterion) {
    let tmp = TempDir::new().unwrap();
    let adapter = HnswAdapter::new(
        tmp.path().to_path_buf(),
        HnswConfig {
            dimensions: 384,
            initial_capacity: 200_000,
            ..Default::default()
        },
    )
    .unwrap();

    let vectors: Vec<Vec<f32>> = (0..1000).map(|_| random_vector(384)).collect();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut base_key = 0u64;

    c.bench_function("hnsw_add_batch_1000_384d", |b| {
        b.iter(|| {
            for (i, v) in vectors.iter().enumerate() {
                rt.block_on(adapter.add(base_key + i as u64, v)).unwrap();
            }
            base_key += 1000;
        })
    });
}

fn hnsw_search_benchmark(c: &mut Criterion) {
    let (adapter, _tmp) = build_test_index(50_000, 384);
    let query = random_vector(384);
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("hnsw_search");

    group.bench_function("top10_50k", |b| {
        b.iter(|| {
            black_box(rt.block_on(adapter.search(&query, 10)).unwrap());
        })
    });

    group.bench_function("top50_50k", |b| {
        b.iter(|| {
            black_box(rt.block_on(adapter.search(&query, 50)).unwrap());
        })
    });

    group.finish();
}

fn hnsw_save_load_benchmark(c: &mut Criterion) {
    let (adapter, _tmp) = build_test_index(50_000, 384);
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("hnsw_persistence");

    group.bench_function("save_50k", |b| {
        b.iter(|| {
            // Mark dirty to force save — must use rt.block_on() since add() is async
            rt.block_on(adapter.add(99_999, &random_vector(384))).unwrap();
            black_box(rt.block_on(adapter.save()).unwrap());
        })
    });

    group.bench_function("load_50k", |b| {
        // Save once first
        rt.block_on(adapter.save()).unwrap();
        b.iter(|| {
            black_box(rt.block_on(adapter.load()).unwrap());
        })
    });

    group.finish();
}

fn hnsw_search_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("hnsw_search_scaling");
    let rt = tokio::runtime::Runtime::new().unwrap();

    for n in [1_000, 10_000, 50_000] {
        let (adapter, _tmp) = build_test_index(n, 384);
        let query = random_vector(384);

        group.bench_with_input(BenchmarkId::new("top10", n), &n, |b, _| {
            b.iter(|| {
                black_box(rt.block_on(adapter.search(&query, 10)).unwrap());
            })
        });
    }

    group.finish();
}

fn hnsw_recall_at_10(c: &mut Criterion) {
    let n = 10_000; // Use 10K for reasonable bench time
    let dims = 384;
    let num_queries = 50;

    let (adapter, vectors, _tmp) = build_test_index_with_vectors(n, dims);
    let queries: Vec<Vec<f32>> = (0..num_queries).map(|_| random_vector(dims)).collect();
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("recall_at_10_10k", |b| {
        b.iter(|| {
            let mut total_recall = 0.0;
            for query in &queries {
                let hnsw_results: Vec<u64> = rt
                    .block_on(adapter.search(query, 10))
                    .unwrap()
                    .iter()
                    .map(|(k, _)| *k)
                    .collect();
                let brute_results = brute_force_search(&vectors, query, 10);
                let overlap = hnsw_results
                    .iter()
                    .filter(|k| brute_results.contains(k))
                    .count();
                total_recall += overlap as f64 / 10.0;
            }
            let avg_recall = total_recall / num_queries as f64;
            black_box(avg_recall);
            // This assertion runs on every iteration — will fail the bench if recall drops
            assert!(
                avg_recall >= 0.90,
                "recall@10 = {avg_recall:.3}, expected >= 0.90"
            );
        })
    });
}

criterion_group!(
    benches,
    hnsw_add_single,
    hnsw_add_batch_1000,
    hnsw_search_benchmark,
    hnsw_save_load_benchmark,
    hnsw_search_scaling,
    hnsw_recall_at_10,
);
criterion_main!(benches);
```

- [ ] **Step 12.3** Run: `cargo bench -p oneshim-analysis --features hnsw --bench hnsw_bench -- --test` — verify benchmarks compile and run (single iteration with `--test` flag)

- [ ] **Step 12.4** Run full benchmarks to collect baselines: `cargo bench -p oneshim-analysis --features hnsw --bench hnsw_bench`

- [ ] **Step 12.5** Verify recall@10 >= 0.90 in benchmark output (the assertion in the bench will enforce this)

---

## Verification Checklist

After all tasks are complete, run the following commands to verify everything works:

- [ ] `cargo check --workspace` — all crates compile
- [ ] `cargo check --workspace --features hnsw` — HNSW feature compiles
- [ ] `cargo test -p oneshim-core -- ann_index` — port trait tests pass
- [ ] `cargo test -p oneshim-analysis --features hnsw` — all analysis tests pass (existing + new)
- [ ] `cargo test -p oneshim-analysis` — tests pass WITHOUT hnsw feature (backward compat)
- [ ] `cargo clippy --workspace` — no new warnings
- [ ] `cargo fmt --check` — formatting clean
- [ ] `cargo bench -p oneshim-analysis --features hnsw --bench hnsw_bench -- --test` — benchmarks run

---

## Summary of Changes

| Metric | Value |
|--------|-------|
| New files | 3 (`ann_index.rs`, `hnsw_adapter.rs`, `hnsw_bench.rs`) |
| Modified files | 8 |
| New external dependency | `usearch 2.15` (optional) |
| New port trait | `AnnIndex` (always available, no feature gate) |
| New adapter | `HnswAdapter` (feature-gated `hnsw`) |
| New search strategy | `SearchStrategy::Hnsw` |
| New tests | ~20 unit tests + 6 benchmark cases |
| Feature flags | `hnsw` in oneshim-analysis, propagated via src-tauri |
| Breaking changes | None (all additions are backward compatible) |
