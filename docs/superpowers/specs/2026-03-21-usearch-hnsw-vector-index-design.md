# usearch HNSW Vector Index — Design Spec

> Created: 2026-03-21
> Revised: 2026-03-21 (post-review)
> Priority: P1
> Effort: 8 days
> Status: Proposed
> Scope: oneshim-core (new port trait), oneshim-analysis, oneshim-storage
> Reference: IVF index in oneshim-storage, vector pipeline in oneshim-analysis

## 1. Goal

Replace or supplement the existing IVF index with usearch HNSW for sub-millisecond approximate nearest-neighbor search on INT8-quantized embedding vectors. HNSW is incrementally maintained via `add()` — no periodic rebuild needed (unlike IVF).

## 2. Current State

### 2.1 Existing Vector Infrastructure

- `oneshim-embedding`: `EmbeddingService` — vector embedding generation, INT8 scalar quantization, similarity search
- `oneshim-storage`: SQLite-based IVF index (V14 migration), periodic rebuild via `build.rs`
- `oneshim-analysis`: `AdaptiveSearchCoordinator` — auto strategy selection (brute-force / IVF / IVF+binary)

### 2.2 IVF Limitations

- Requires periodic rebuild (expensive on large datasets)
- Accuracy degrades between rebuilds as new vectors accumulate
- Rebuild blocks WAL checkpoint (only checkpoint location is in `build.rs:176,295`)

### 2.3 Known Issues

> **Send + Sync: Resolved in usearch v2.15+ (PR #492).** `unsafe impl Send/Sync` provided natively. `Arc<Index>` works directly without Mutex wrapper. No performance penalty.

> **Thread init required:** Must call `reserve(capacity)` at construction to pre-allocate memory. The Rust API exposes `reserve(capacity: usize)` — do NOT use the C++ API name `reserve_capacity_and_threads`.

> **`save()` thread safety:** Not documented as safe during concurrent `search()`. Use `save_to_buffer()` + async file write for persistence.

## 2.X usearch API Reference

### IndexOptions Constructor

```rust
use usearch::Index;

let options = usearch::IndexOptions {
    dimensions: 384,           // Match embedding model output
    metric: usearch::MetricKind::Cos,
    quantization: usearch::ScalarKind::I8,
    connectivity: 16,          // M parameter (default 16)
    expansion_add: 128,        // ef_construction
    expansion_search: 64,      // ef_search
    multi: false,              // Single vector per key
};

let index = Index::new(&options).expect("failed to create index");
index.reserve(50_000).expect("failed to reserve capacity");
```

### Key Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `add` | `add(key: u64, vector: &[f32])` | Insert vector (incremental, no rebuild) |
| `search` | `search(vector: &[f32], count: usize) -> Matches` | Find k-nearest neighbors; returns `Matches { keys: Vec<u64>, distances: Vec<f32> }` |
| `remove` | `remove(key: u64)` | Lazy-delete (marks tombstone) |
| `save` | `save(path: &str)` | Persist to file |
| `load` | `load(path: &str)` | Load from file |
| `save_to_buffer` | `save_to_buffer(buffer: &mut [u8])` | Thread-safe serialization to buffer |
| `reserve` | `reserve(capacity: usize)` | Pre-allocate capacity (Rust API) |
| `len` | `len() -> usize` | Current vector count |
| `capacity` | `capacity() -> usize` | Reserved capacity |

### Search Result Mapping

usearch `search()` returns `Matches { keys: Vec<u64>, distances: Vec<f32> }`. These must be joined with metadata from SQLite:

```sql
SELECT id, segment_id, content_type, content_label, timestamp, original_text
FROM embedding_vectors
WHERE id IN (?, ?, ?, ...)
```

Distance-to-similarity conversion: `similarity = 1.0 - distance` (for cosine metric).

Time decay is applied post-search via the existing `score_and_rank()` pattern in `VectorRetriever`.

### INT8 Index Creation Example

```rust
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

let options = IndexOptions {
    dimensions: 384,
    metric: MetricKind::Cos,
    quantization: ScalarKind::I8,  // INT8 quantization built-in
    connectivity: 16,
    expansion_add: 128,
    expansion_search: 64,
    multi: false,
};

let index = Index::new(&options)?;
index.reserve(50_000)?;

// Add vector (usearch handles INT8 quantization internally)
let embedding: Vec<f32> = vec![0.1; 384];
index.add(42, &embedding)?;

// Search returns Matches { keys, distances }
let results = index.search(&embedding, 10)?;
for (key, distance) in results.keys.iter().zip(results.distances.iter()) {
    let similarity = 1.0 - distance;
    println!("key={key}, similarity={similarity:.4}");
}
```

## 3. Architecture

### 3.1 AnnIndex Port Trait (KEY ARCHITECTURAL FIX)

Define a port trait in `oneshim-core` to maintain hexagonal architecture. The HNSW implementation is an adapter behind this port.

```rust
// oneshim-core/src/ports/ann_index.rs
use async_trait::async_trait;

/// Approximate Nearest Neighbor index port.
/// Implementations: HnswAdapter (oneshim-analysis, feature = "hnsw")
#[async_trait]
pub trait AnnIndex: Send + Sync {
    /// Insert a vector with the given key.
    async fn add(&self, key: u64, vector: &[f32]) -> Result<(), CoreError>;

    /// Find the k nearest neighbors of the query vector.
    /// Returns (keys, distances) pairs.
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

### 3.2 HnswAdapter Implementation

```rust
// oneshim-analysis/src/hnsw_adapter.rs (behind #[cfg(feature = "hnsw")])
use usearch::Index;
use oneshim_core::ports::AnnIndex;

pub struct HnswAdapter {
    index: Index,
    data_path: PathBuf,
}

#[async_trait]
impl AnnIndex for HnswAdapter {
    async fn add(&self, key: u64, vector: &[f32]) -> Result<(), CoreError> {
        self.index.add(key, vector).map_err(|e| CoreError::Storage(e.to_string()))
    }
    // ... other methods
}
```

### 3.3 Error Handling

Map `usearch::Error` to `CoreError`:

```rust
impl From<usearch::Error> for CoreError {
    fn from(e: usearch::Error) -> Self {
        CoreError::Storage(format!("HNSW index error: {}", e))
    }
}
```

Or, if `CoreError` variants should remain specific, use manual mapping in the adapter.

### 3.4 Integration with AdaptiveSearchCoordinator

The coordinator takes `Option<Arc<dyn AnnIndex>>` instead of a concrete usearch type:

```rust
pub struct AdaptiveSearchCoordinator {
    // Existing fields...
    ann_index: Option<Arc<dyn AnnIndex>>,
}

#[cfg(feature = "hnsw")]
pub enum VectorIndexBackend {
    BruteForce,
    Ivf,
    Hnsw,  // Uses ann_index field — no concrete type here
}

#[cfg(not(feature = "hnsw"))]
pub enum VectorIndexBackend {
    BruteForce,
    Ivf,
}
```

HNSW is incrementally maintained via `add()` — no periodic rebuild needed (unlike IVF). This eliminates the rebuild-blocks-checkpoint problem.

### 3.5 Feature Flag

Reference `hdbscan` pattern in `oneshim-analysis` as exact template for the `usearch` feature flag.

```toml
# crates/oneshim-analysis/Cargo.toml
[dependencies]
usearch = { version = "2.15", optional = true }

[features]
default = []
hnsw = ["usearch"]
```

Feature propagation in binary crate:

```toml
# src-tauri/Cargo.toml
[dependencies]
oneshim-analysis = { path = "../crates/oneshim-analysis", features = ["hnsw"] }
```

The `AnnIndex` port trait lives in `oneshim-core` and has no feature flag — it is always available. Only the `HnswAdapter` implementation in `oneshim-analysis` is gated behind `#[cfg(feature = "hnsw")]`.

### 3.6 Graceful Degradation

If HNSW search fails (index corruption, load failure, etc.), fall back to IVF or brute-force:

```rust
async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>, CoreError> {
    if let Some(ref ann) = self.ann_index {
        match ann.search(query, k).await {
            Ok(results) => return self.join_metadata(results).await,
            Err(e) => {
                warn!("HNSW search failed, falling back to IVF: {}", e);
                // Fall through to IVF/brute-force
            }
        }
    }
    // Existing IVF or brute-force path
    self.ivf_or_brute_search(query, k).await
}
```

### 3.7 Retention Integration

When `VectorStore::enforce_retention()` deletes vectors from SQLite, the HNSW index must also be updated:

```rust
// In enforce_retention() or wherever vectors are deleted
for deleted_id in deleted_vector_ids {
    if let Some(ref ann) = self.ann_index {
        let _ = ann.remove(deleted_id).await; // Best-effort; tombstone is acceptable
    }
}
```

### 3.8 Corruption Recovery

On `load()` failure, rebuild the index from SQLite `embedding_vectors` table:

```rust
async fn load_or_rebuild(&self) -> Result<(), CoreError> {
    match self.ann_index.load().await {
        Ok(()) => Ok(()),
        Err(e) => {
            warn!("HNSW index load failed, rebuilding from SQLite: {}", e);
            let vectors = self.storage.get_all_embeddings().await?;
            for (id, vector) in vectors {
                self.ann_index.add(id, &vector).await?;
            }
            self.ann_index.save().await?;
            Ok(())
        }
    }
}
```

### 3.9 Capacity Growth

Double capacity at 80% utilization to avoid reallocation during normal operation:

```rust
fn maybe_grow(&self) -> Result<(), CoreError> {
    let util = self.ann_index.len() as f64 / self.ann_index.capacity() as f64;
    if util > 0.8 {
        let new_cap = self.ann_index.capacity() * 2;
        self.index.reserve(new_cap)?;
    }
    Ok(())
}
```

### 3.10 Persistence

- File size: ~27MB for 50K vectors / INT8 quantization
- Storage path: `{data_dir}/hnsw_index.usearch`
- Use `save_to_buffer()` + async file write for thread-safe persistence
- Load at startup via `Index::load()` (blocking, run in `spawn_blocking`)
- Atomic rename pattern: write to `.tmp`, then `rename()` to final path

### 3.11 Lifecycle

1. **Startup**: Load from `{data_dir}/hnsw_index.usearch` if exists; on failure, rebuild from SQLite; if no data, create empty
2. **Insert**: `ann_index.add(key, vector)` on each new embedding (incremental) + call `maybe_grow()`
3. **Search**: `ann_index.search(query, k)` + batch SQL metadata join + `score_and_rank()` time decay
4. **Persist**: Periodic `save_to_buffer()` on sync loop (every 60s) or on graceful shutdown
5. **Delete**: `ann_index.remove(key)` — lazy tombstone (compaction not yet supported by usearch)
6. **Retention**: When `enforce_retention()` deletes SQLite vectors, also remove from HNSW

### 3.12 Scope: HybridSearchService

Explicitly out of scope for this spec. HNSW is accessed only via `AdaptiveSearchCoordinator`. A future `HybridSearchService` (combining HNSW + FTS5 + metadata filters) is a separate design.

## 4. Benchmarking

Criterion 0.8 infrastructure already exists (4 bench files in workspace). Create `crates/oneshim-analysis/benches/hnsw_bench.rs`.

### Benchmark Cases

| Benchmark | Parameters | Target |
|-----------|-----------|--------|
| `hnsw_add_single` | 1 vector, 384 dims | <0.1ms |
| `hnsw_add_batch_1000` | 1000 vectors, 384 dims | <100ms |
| `hnsw_search_top10` | 50K index, k=10 | <1ms |
| `hnsw_search_top50` | 50K index, k=50 | <5ms |
| `hnsw_save_50k` | 50K vectors to buffer | <500ms |
| `hnsw_load_50k` | Load from 27MB file | <200ms |
| `hnsw_vs_ivf_search` | 50K index, k=10 | Compare latency |
| `hnsw_vs_brute_search` | 1K/10K/50K, k=10 | Crossover point |
| `hnsw_recall_at_10` | 50K index, k=10 | recall@10 vs brute-force ground truth |

### Recall@10 Benchmark

Compare HNSW approximate results against brute-force exact results to measure accuracy:

```rust
fn recall_at_10_benchmark(c: &mut Criterion) {
    let (index, brute_vectors) = build_test_index_with_ground_truth(50_000, 384);
    let queries: Vec<Vec<f32>> = (0..100).map(|_| random_vector(384)).collect();

    let mut total_recall = 0.0;
    for query in &queries {
        let hnsw_results = index.search(query, 10).unwrap();
        let brute_results = brute_force_search(&brute_vectors, query, 10);
        let overlap = hnsw_results.keys.iter()
            .filter(|k| brute_results.contains(k))
            .count();
        total_recall += overlap as f64 / 10.0;
    }
    let avg_recall = total_recall / queries.len() as f64;
    // Target: recall@10 >= 0.95
    assert!(avg_recall >= 0.95, "recall@10 = {avg_recall:.3}, expected >= 0.95");
}
```

### Benchmark Setup

```rust
// crates/oneshim-analysis/benches/hnsw_bench.rs
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn hnsw_search_benchmark(c: &mut Criterion) {
    let index = build_test_index(50_000, 384);
    let query = random_vector(384);

    c.bench_function("hnsw_search_top10_50k", |b| {
        b.iter(|| index.search(&query, 10).unwrap())
    });
}

criterion_group!(benches, hnsw_search_benchmark);
criterion_main!(benches);
```

## 5. Testing Strategy

- Unit tests: Index creation, add, search, remove, persistence round-trip
- Property tests: search results match brute-force for small datasets (recall@10 >= 0.95)
- Integration: `AdaptiveSearchCoordinator` routing with HNSW backend via `Arc<dyn AnnIndex>`
- Degradation: HNSW failure falls back to IVF/brute-force without panic
- Corruption recovery: Delete index file, verify rebuild from SQLite
- Retention: Delete vectors from SQLite, verify HNSW `remove()` called
- Benchmark: Criterion 0.8 comparison vs IVF and brute-force

## 6. Risks

| Risk | Mitigation |
|------|------------|
| usearch C++ core linkage — ensure `cc` build works on all 3 platforms | CI matrix: macOS, Windows, Linux. usearch publishes pre-built binaries. |
| Index file corruption on crash during save | Atomic rename pattern (write `.tmp`, then `rename()`) |
| Memory usage: ~27MB resident for 50K/INT8 | Acceptable for desktop. Monitor with `sysinfo` metrics. |
| Tombstone accumulation from `remove()` | Monitor tombstone ratio; rebuild if >20% tombstones |
| `usearch::Error` not `Send` or has unusual types | Wrap in `CoreError::Storage(String)` at adapter boundary |
| Capacity exhaustion without `maybe_grow()` | Grow at 80% utilization; initial reserve 50K is generous |

## 7. Migration Path

1. Add `AnnIndex` port trait to `oneshim-core/src/ports/` (always available, no feature flag)
2. Add `hnsw` feature flag to `oneshim-analysis` (off by default)
3. Implement `HnswAdapter` in `oneshim-analysis` (behind `#[cfg(feature = "hnsw")]`)
4. Wire `Option<Arc<dyn AnnIndex>>` into `AdaptiveSearchCoordinator`
5. Add `features = ["hnsw"]` to `src-tauri/Cargo.toml` dependency on `oneshim-analysis`
6. Implement metadata join (batch SQL lookup from `embedding_vectors`)
7. Implement graceful degradation, retention integration, corruption recovery
8. Benchmark against IVF — if HNSW wins, make it default
9. Eventually deprecate IVF rebuild path

## 8. Effort (8 days)

| Task | Days |
|------|------|
| AnnIndex port trait in oneshim-core | 0.5 |
| HnswAdapter implementation + error mapping | 1.5 |
| AdaptiveSearchCoordinator integration + feature flag | 1.0 |
| SearchResult metadata join (batch SQL lookup) | 1.0 |
| Graceful degradation + corruption recovery | 1.0 |
| Retention integration (remove on delete) | 0.5 |
| Capacity growth + persistence (atomic rename) | 0.5 |
| Benchmarks (criterion + recall@10) | 1.0 |
| Testing (unit + integration + degradation) | 1.0 |
| **Total** | **8.0** |
