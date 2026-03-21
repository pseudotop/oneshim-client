# usearch HNSW Vector Index — Design Spec

> Created: 2026-03-21
> Status: Proposed
> Scope: oneshim-analysis, oneshim-embedding, oneshim-storage
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

> **Thread init required:** Must call `reserve_capacity_and_threads(capacity, num_cpus::get())` at construction to avoid crashes ([#389](https://github.com/unum-cloud/usearch/issues/389)).

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
| `search` | `search(vector: &[f32], count: usize)` | Find k-nearest neighbors |
| `remove` | `remove(key: u64)` | Lazy-delete (marks tombstone) |
| `save` | `save(path: &str)` | Persist to file |
| `load` | `load(path: &str)` | Load from file |
| `save_to_buffer` | `save_to_buffer(buffer: &mut [u8])` | Thread-safe serialization to buffer |
| `reserve` | `reserve(capacity: usize)` | Pre-allocate capacity |
| `len` | `len() -> usize` | Current vector count |
| `capacity` | `capacity() -> usize` | Reserved capacity |

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

// Search returns (keys, distances)
let results = index.search(&embedding, 10)?;
for (key, distance) in results.keys.iter().zip(results.distances.iter()) {
    println!("key={key}, distance={distance}");
}
```

## 3. Architecture

### 3.1 Integration Point

Add HNSW as an alternative backend in `AdaptiveSearchCoordinator`:

```rust
pub enum VectorIndexBackend {
    BruteForce,
    Ivf,
    Hnsw(Arc<Index>),  // Arc<Index> works directly — no Mutex needed
}
```

HNSW is incrementally maintained via `add()` — no periodic rebuild needed (unlike IVF). This eliminates the rebuild-blocks-checkpoint problem.

### 3.2 Feature Flag

Reference `hdbscan` pattern in `oneshim-analysis` as exact template for the `usearch` feature flag. No `oneshim-core` propagation needed — the feature is local to `oneshim-analysis`.

```toml
# crates/oneshim-analysis/Cargo.toml
[dependencies]
usearch = { version = "2.15", optional = true }

[features]
default = []
hnsw = ["usearch"]
```

### 3.3 Persistence

- File size: ~27MB for 50K vectors / INT8 quantization
- Storage path: `{data_dir}/hnsw_index.usearch`
- Use `save_to_buffer()` + async file write for thread-safe persistence
- Load at startup via `Index::load()` (blocking, run in `spawn_blocking`)

### 3.4 Lifecycle

1. **Startup**: Load from `{data_dir}/hnsw_index.usearch` if exists, else create empty
2. **Insert**: `index.add(key, vector)` on each new embedding (incremental)
3. **Search**: `index.search(query, k)` — lock-free reads via `Arc<Index>`
4. **Persist**: Periodic `save_to_buffer()` on sync loop (every 60s) or on graceful shutdown
5. **Delete**: `index.remove(key)` — lazy tombstone (compaction not yet supported by usearch)

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
- Property tests: search results match brute-force for small datasets
- Integration: `AdaptiveSearchCoordinator` routing with HNSW backend
- Benchmark: Criterion 0.8 comparison vs IVF and brute-force

## 6. Risks

- usearch C++ core linkage — ensure `cc` build works on all 3 platforms
- Index file corruption on crash during save — mitigate with atomic rename
- Memory usage: ~27MB resident for 50K/INT8 — acceptable for desktop
- Tombstone accumulation from `remove()` — monitor and rebuild if >20% tombstones

## 7. Migration Path

1. Add `hnsw` feature flag (off by default)
2. Implement `HnswIndex` wrapper in `oneshim-analysis`
3. Wire into `AdaptiveSearchCoordinator` as third backend
4. Benchmark against IVF — if HNSW wins, make it default
5. Eventually deprecate IVF rebuild path
