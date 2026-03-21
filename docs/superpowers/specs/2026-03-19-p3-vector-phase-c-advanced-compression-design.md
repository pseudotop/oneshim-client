# P3 Vector Phase C: Advanced Compression + Indexing — Design Spec

> Created: 2026-03-19
> Status: Implemented (Phase C.1-C.4)
> Depends on: Phase A (INT8 scalar quantization — implemented), Phase B (query expansion — implemented)
> Parent spec: [P3 Vector Compression + Embedding Optimization](2026-03-19-p3-vector-compression-embedding-optimization-design.md)

## 1. Goal

Extend the local vector store to handle 100K+ vectors without degraded search
latency or excessive memory use. Phase A delivered 4x compression via INT8
scalar quantization with brute-force scan. Phase C adds two capabilities that
become necessary as the vector count grows beyond what brute-force can serve
within a 10 ms latency budget on a desktop machine:

1. **2-bit Binary Quantization** -- 16x total compression for coarse-grained
   candidate filtering via Hamming distance.
2. **IVF (Inverted File Index)** -- sub-linear search by partitioning vectors
   into clusters and probing only the nearest partitions at query time.
3. **Adaptive Index Selection** -- auto-select the optimal search strategy
   based on the current collection size, so users never need to configure
   anything.

### Performance Targets

| Metric | Phase A (current) | Phase C target |
|--------|-------------------|----------------|
| Storage per vector (384-dim) | 384 B (INT8) | 96 B (2-bit) for filter index |
| Total storage at 200K vectors | ~75 MB (INT8) | ~19 MB (2-bit) + ~75 MB (INT8 for re-rank) |
| Search latency at 50K | ~4 ms (INT8 brute-force) | ~4 ms (unchanged, brute-force is fine) |
| Search latency at 200K | ~16 ms (INT8 brute-force) | ~3 ms (IVF + 2-bit filter) |
| Memory for index structures | 0 (no index) | ~30 MB at 200K vectors |
| Recall@10 vs float32 baseline | ~99% | >= 95% (2-bit filter + INT8 re-rank) |

### Desktop Constraints

This is a desktop client, not a vector database server. Hard constraints:

- **Memory budget**: <= 100 MB total for all vector index structures.
- **Build time budget**: index rebuild must complete in < 60 seconds for 200K
  vectors on a 4-core laptop CPU.
- **SQLite remains the storage backend**. No external database or extension.
- **No GPU required**. All computation is CPU-only.
- **Background-only index building**. Index construction must never block the
  UI thread or the main scheduler loops.

## 2. Design Decisions

### 2.1 2-Bit Binary Quantization -- chosen for coarse filtering

| Option | Bits/dim | Compression vs f32 | Distance metric | Recall@10 | Verdict |
|--------|----------|-------------------|-----------------|-----------|---------|
| **2-bit quantization** | **2** | **16x** | **Hamming** | **~92% (est.)** | **Selected for filtering stage** |
| 1-bit binary | 1 | 32x | Hamming | ~85-90% | Rejected -- quality loss too steep at 384 dims; needs >= 1024 dims |
| 4-bit quantization | 4 | 8x | INT4 dot product | ~97% | Rejected -- marginal gain over INT8 for 2x more storage; not worth the complexity |

**Approach**: Each f32 dimension is mapped to 2 bits encoding 4 levels. For 384
dimensions, this produces a 96-byte binary code per vector (384 * 2 / 8 = 96).

**Quantization scheme** (uniform 4-level):

```
Level 0: value < Q25 (25th percentile)     -> bits 00
Level 1: Q25 <= value < Q50 (median)       -> bits 01
Level 2: Q50 <= value < Q75                -> bits 10
Level 3: value >= Q75                      -> bits 11
```

Percentile thresholds are computed per-dimension across the entire collection
during index build (not per-vector like INT8). This is a global codebook
with 4 levels -- trivial to compute, no iterative training.

**Distance**: Bit-level Hamming distance on the packed 2-bit representation.
The XOR + popcount implementation counts the number of set bits in the XOR of
two binary codes. Because each dimension occupies 2 bits, a single-dimension
difference can contribute 1 or 2 set bits depending on how the levels differ.
This is the standard approach used by Qdrant for binary quantization filtering.
Hamming distance on packed bytes uses `popcount` (POPCNT instruction on x86,
CNT on ARM), which processes 8 bytes per cycle. For 96 bytes, this is ~12
cycles -- effectively free compared to INT8 dot product over 384 dims.

**Usage**: 2-bit codes are used ONLY for the initial candidate filtering stage.
The top-K' candidates (K' = K * oversample_factor, default oversample = 10)
are then re-ranked using INT8 cosine similarity for precision. This two-stage
approach recovers most of the recall lost by the coarse 2-bit encoding.

> **Note**: The ~92% recall figure is an estimate extrapolated from Qdrant's
> published benchmarks on higher-dimensional vectors. This must be validated
> with our 384-dim embeddings during Phase C.1 benchmarking. **Fallback**: if
> recall testing shows < 90% filter recall at oversample=10, the
> `AdaptiveSearchCoordinator` will automatically skip the binary filter stage
> and use IVF-only search for the > 100K tier.

### 2.2 IVF Index -- chosen for sub-linear search

| Option | Complexity | Memory overhead | Build time | Desktop fit | Verdict |
|--------|-----------|-----------------|------------|-------------|---------|
| **IVF (Inverted File)** | **Medium** | **Low (centroids only)** | **~10s at 200K** | **Good** | **Selected** |
| HNSW graph | High | High (graph edges) | ~30s at 200K | Marginal -- 200+ MB for graph at 200K | Rejected |
| Annoy (random projections) | Low | Medium (trees) | ~15s at 200K | Decent | Rejected -- immutable after build; IVF allows incremental updates |
| VP-tree | Low | Low | ~5s at 200K | Good | Rejected -- poor empirical recall vs IVF for high dimensions |

**Approach**: Simple k-means IVF:

1. **Build phase**: Run k-means clustering on all INT8 vectors to produce N
   centroids, where N = floor(sqrt(total_vectors)). For 200K vectors, N = 447.
   Each vector is assigned to its nearest centroid.
2. **Query phase**: Compute distance from the query to all N centroids. Select
   the nearest `nprobe` centroids (default: nprobe = max(1, N / 10)). Scan
   only the vectors in those partitions.
3. **Expected scan reduction**: At nprobe = N/10, we scan ~10% of vectors.
   For 200K vectors, that is ~20K instead of 200K -- a 10x speedup.

**K-means implementation**: Use Lloyd's algorithm with 10 iterations. At 200K
vectors of 384 INT8 dims, each iteration scans ~200K * 447 distances = ~90M
distance computations. With INT8 dot product this takes ~2 seconds per
iteration, ~20 seconds total. Well within the 60-second budget.

We do NOT use an external crate for k-means. The algorithm is 50 lines of Rust
(assign each vector to nearest centroid, recompute centroids as mean of
assigned vectors, repeat). Using the existing `ScalarQuantizer` infrastructure
for distance computation keeps the dependency count at zero.

### 2.3 Adaptive Index Selection -- automatic, no user config

| Collection size | Strategy | Rationale |
|----------------|----------|-----------|
| < 10,000 | Brute-force INT8 | Current path. Scan takes < 2 ms. Index overhead not justified. |
| 10,000 - 100,000 | IVF with INT8 scan | Sub-linear search via cluster pruning. 3-5x speedup over brute-force. |
| > 100,000 | IVF with 2-bit filter + INT8 re-rank | Two-stage search: Hamming filter reduces candidates by 10x before INT8 re-rank. |

The `AdaptiveSearchStrategy` selects the strategy at query time based on
`VectorStore::count_active_vectors()` (which maps to
`SELECT COUNT(*) FROM embedding_vectors WHERE is_stale = 0` in the SQLite
implementation). This count is cached and refreshed every 60 seconds
(piggybacks on the existing scheduler aggregate loop).

**No user-facing configuration**. The thresholds (10K, 100K) are compile-time
constants in `oneshim-core/src/quantization.rs`. Power users can override via
`analysis.embedding.index_strategy: "brute_force" | "ivf" | "ivf_binary"` in
the config file, but the default is `"auto"`.

### 2.4 Storage Layout

All index structures are stored in SQLite, not in-memory. This enables
persistence across app restarts without requiring a rebuild on every launch.

#### New Tables (V15 migration -- tentative)

> **Note**: V15 is a tentative migration number. The actual version should be
> assigned at implementation time based on `CURRENT_VERSION` in
> `crates/oneshim-storage/src/migration.rs`, since other features may land
> before Phase C and claim this slot.

```sql
-- 2-bit binary codes for Hamming distance filtering
CREATE TABLE IF NOT EXISTS vector_binary_codes (
    vector_id INTEGER PRIMARY KEY,
    binary_code BLOB NOT NULL,          -- 96 bytes for 384-dim 2-bit encoding
    FOREIGN KEY (vector_id) REFERENCES embedding_vectors(id) ON DELETE CASCADE
);

-- IVF cluster centroids
CREATE TABLE IF NOT EXISTS ivf_centroids (
    id INTEGER PRIMARY KEY,
    centroid_int8 BLOB NOT NULL,        -- 384 bytes (INT8 centroid vector)
    centroid_scale REAL NOT NULL,
    centroid_offset REAL NOT NULL,
    vector_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- IVF cluster assignments (which vectors belong to which cluster)
CREATE TABLE IF NOT EXISTS ivf_assignments (
    vector_id INTEGER NOT NULL,
    cluster_id INTEGER NOT NULL,
    PRIMARY KEY (vector_id),
    FOREIGN KEY (vector_id) REFERENCES embedding_vectors(id) ON DELETE CASCADE,
    FOREIGN KEY (cluster_id) REFERENCES ivf_centroids(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ivf_assign_cluster ON ivf_assignments(cluster_id);

-- Index metadata (tracks build state)
CREATE TABLE IF NOT EXISTS vector_index_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
-- Keys: 'ivf_built_at', 'ivf_vector_count', 'binary_built_at',
--        'binary_quantile_thresholds', 'index_strategy'
```

**Storage overhead** at 200K vectors:

| Table | Per-row | Total at 200K |
|-------|---------|---------------|
| `vector_binary_codes` | 96 B + 8 B rowid | ~20 MB |
| `ivf_centroids` | 384 B + 12 B + 4 B | ~175 KB (447 rows) |
| `ivf_assignments` | 8 B + 4 B | ~2.3 MB |
| **Total index overhead** | | **~23 MB** |

Well within the 100 MB memory budget, and the data lives on disk (read into
memory only during search).

## 3. Architecture

### 3.1 Binary Quantizer (new, in `oneshim-core`)

```
oneshim-core/src/
├── quantization.rs          # Existing ScalarQuantizer
└── binary_quantizer.rs      # NEW: 2-bit quantization + Hamming distance
    ├── BinaryQuantizer
    │   ├── compute_thresholds(vectors: &[&QuantizedVector]) -> QuantileThresholds
    │   ├── quantize_to_binary(vector: &QuantizedVector, thresholds: &QuantileThresholds) -> BinaryCode
    │   └── hamming_distance(a: &BinaryCode, b: &BinaryCode) -> u32
    ├── BinaryCode { data: Vec<u8> }    -- 96 bytes for 384-dim
    └── QuantileThresholds { q25: Vec<f32>, q50: Vec<f32>, q75: Vec<f32> }
```

**Why `oneshim-core`?** Same rationale as `ScalarQuantizer` -- pure math, no
I/O, no async. Depends only on the existing `QuantizedVector` type.

**Edge cases**:

| Case | Behavior |
|------|----------|
| Empty vector set for threshold computation | Return error; cannot compute quantiles on empty set |
| Single-value dimension (all elements identical) | All bits = 01 (map to level 1); distance contribution is 0 for this dim |
| Vector shorter than expected dimensions | Return error; binary code requires consistent dimensionality |

**Hamming distance implementation**:

```rust
pub fn hamming_distance(a: &BinaryCode, b: &BinaryCode) -> u32 {
    // Bit-level Hamming distance on packed 2-bit representation.
    // XOR + popcount: counts set bits in the XOR of two binary codes.
    // Each byte contains 4 two-bit codes. A difference in any 2-bit
    // position produces 1 or 2 set bits after XOR depending on the
    // level difference. This is the standard approach used by Qdrant.
    a.data.iter()
        .zip(b.data.iter())
        .map(|(&x, &y)| (x ^ y).count_ones())
        .sum()
}
```

This compiles to POPCNT/CNT instructions with LLVM auto-vectorization. For
96 bytes, expected throughput is < 100 ns.

### 3.2 IVF Index (new, in `oneshim-core`)

```
oneshim-core/src/
└── ivf_index.rs             # NEW: IVF k-means clustering + partition search
    ├── IvfIndex
    │   ├── build(vectors: &[(i64, QuantizedVector)], n_clusters: usize) -> IvfIndex
    │   ├── assign(vector: &QuantizedVector) -> usize  // nearest centroid
    │   ├── nearest_centroids(query: &QuantizedVector, nprobe: usize) -> Vec<usize>
    │   └── get_cluster_vector_ids(cluster_id: usize) -> &[i64]
    ├── IvfCentroid { id: usize, vector: QuantizedVector, count: usize }
    └── IvfBuildConfig { n_clusters: usize, n_iterations: usize, seed: u64 }
```

**Why `oneshim-core`?** The k-means logic is pure computation over
`QuantizedVector`. No I/O. The storage adapter (`oneshim-storage`) handles
persistence of centroids and assignments to SQLite.

**K-means initialization**: K-means++ initialization (select first centroid
randomly, subsequent centroids with probability proportional to squared
distance from nearest existing centroid). This produces better initial
centroids than uniform random and converges faster.

**Incremental updates**: When new vectors arrive between full rebuilds, they
are assigned to the nearest existing centroid without retraining. A counter
tracks "unassigned since last build" vectors. When this exceeds 10% of total
count, a full rebuild is triggered in the background.

### 3.3 Adaptive Search Coordinator (new, in `oneshim-analysis`)

```
oneshim-analysis/src/
└── adaptive_search.rs       # NEW: strategy selection + two-stage pipeline
    ├── AdaptiveSearchCoordinator
    │   ├── new(vector_store, index_store, config) -> Self
    │   ├── search(query: &QuantizedVector, limit, filters) -> Vec<SearchResult>
    │   └── refresh_strategy() -> SearchStrategy  // called every 60s
    ├── SearchStrategy { BruteForceInt8, IvfInt8, IvfBinaryRerank }
    └── SearchConfig { brute_force_threshold, ivf_threshold, oversample_factor }
```

**Intermediate type for the Hamming filter stage**:

```rust
/// Candidate surviving the Hamming distance filter, before INT8 re-ranking.
pub struct HammingCandidate {
    pub vector_id: i64,
    pub hamming_dist: u32,
}
```

**Search flow for IVF + 2-bit re-rank** (> 100K vectors):

```
Query vector (f32)
    |
    v
ScalarQuantizer.quantize() -> QuantizedVector (INT8)
    |
    v
BinaryQuantizer.quantize_to_binary() -> BinaryCode (2-bit)
    |
    v
IvfIndex.nearest_centroids(nprobe) -> [cluster_id_1, cluster_id_2, ...]
    |
    v
For each cluster: load BinaryCodes from vector_binary_codes table
    |
    v
Hamming distance filter: keep top K * oversample_factor as Vec<HammingCandidate>
    |
    v
Load INT8 vectors for candidates from embedding_vectors table
    |
    v
INT8 cosine similarity re-rank -> top K final results
    |
    v
Apply time decay + return SearchResult[]
```

**Search flow for IVF only** (10K - 100K vectors):

```
Query vector (f32)
    |
    v
ScalarQuantizer.quantize() -> QuantizedVector (INT8)
    |
    v
IvfIndex.nearest_centroids(nprobe) -> [cluster_ids]
    |
    v
Load INT8 vectors for all vectors in selected clusters
    |
    v
INT8 cosine similarity brute-force within clusters -> top K
    |
    v
Apply time decay + return SearchResult[]
```

### 3.4 VectorStore Port Extension

The existing `VectorStore` trait gains one new default method. Phase C also
adds a separate port for index operations to maintain clean separation of
concerns.

**New default method on `VectorStore`**:

```rust
/// Count the number of active (non-stale) vectors in the store.
/// Used by AdaptiveSearchCoordinator to select search strategy.
/// Default implementation returns Ok(0) so existing implementations
/// continue to compile without changes.
async fn count_active_vectors(&self) -> Result<u64, CoreError> {
    Ok(0)
}
```

The `AdaptiveSearchCoordinator` calls `count_active_vectors()` every 60
seconds to determine which search strategy tier to use (brute-force, IVF,
or IVF+binary). `SqliteVectorStore` overrides this with
`SELECT COUNT(*) FROM embedding_vectors WHERE is_stale = 0`.

**New port for index operations**:

```rust
// NEW: oneshim-core/src/ports/vector_index.rs

/// Port for vector index construction and indexed search.
///
/// Separated from VectorStore because:
/// 1. Index operations are batch/offline, not per-vector.
/// 2. Not all VectorStore implementations need indexing (tests, small stores).
/// 3. The index can be backed by different storage than the vectors.
#[async_trait]
pub trait VectorIndex: Send + Sync {
    /// Build or rebuild the IVF index from all non-stale vectors.
    /// Returns the number of clusters created.
    async fn build_ivf_index(&self, n_clusters: usize, n_iterations: usize)
        -> Result<usize, CoreError>;

    /// Build or rebuild 2-bit binary codes for all quantized vectors.
    /// Returns the number of codes created.
    async fn build_binary_codes(&self) -> Result<u64, CoreError>;

    /// Search using IVF partitioning with INT8 distance.
    async fn search_ivf(
        &self,
        query_vector: &QuantizedVector,
        nprobe: usize,
        limit: usize,
        time_decay_hours: f32,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, CoreError>;

    /// Two-stage search: IVF + binary Hamming filter + INT8 re-rank.
    async fn search_ivf_binary(
        &self,
        query_vector: &QuantizedVector,
        query_binary: &BinaryCode,
        nprobe: usize,
        oversample_factor: usize,
        limit: usize,
        time_decay_hours: f32,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, CoreError>;

    /// Assign a single new vector to its nearest IVF cluster.
    /// Called on each insert when the IVF index exists.
    async fn assign_to_cluster(&self, vector_id: i64, vector: &QuantizedVector)
        -> Result<(), CoreError>;

    /// Store a 2-bit binary code for a single vector.
    async fn store_binary_code(&self, vector_id: i64, code: &BinaryCode)
        -> Result<(), CoreError>;

    /// Get the current index metadata (built_at, vector_count, strategy).
    async fn get_index_meta(&self) -> Result<IndexMeta, CoreError>;

    /// Get the count of vectors not yet assigned to a cluster.
    async fn count_unindexed(&self) -> Result<u64, CoreError>;

    /// Load the quantile thresholds used for binary quantization.
    async fn load_quantile_thresholds(&self) -> Result<Option<QuantileThresholds>, CoreError>;
}

/// Metadata about the current state of vector indexes.
pub struct IndexMeta {
    pub ivf_built_at: Option<DateTime<Utc>>,
    pub ivf_vector_count: u64,
    pub binary_built_at: Option<DateTime<Utc>>,
    pub total_vector_count: u64,
    pub unindexed_count: u64,
}
```

Default implementations return `CoreError::Internal("not implemented")` so
mock VectorStore implementations in tests continue to compile.

### 3.5 SQLite Schema (V15 migration -- tentative)

```sql
-- V15 migration (tentative; assign actual version at implementation time
-- based on CURRENT_VERSION in crates/oneshim-storage/src/migration.rs):
-- IVF index + 2-bit binary codes

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

INSERT INTO schema_version (version) VALUES (15);  -- tentative; see note above
```

### 3.6 Config Extension

Add to the existing `EmbeddingConfig` in `oneshim-core/src/config/sections/analysis.rs`:

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

### 3.7 Crate Dependency Map

```
oneshim-core
    quantization.rs          (existing: ScalarQuantizer, QuantizedVector)
    binary_quantizer.rs      (NEW: BinaryQuantizer, BinaryCode, QuantileThresholds)
    ivf_index.rs             (NEW: IvfIndex, IvfCentroid, IvfBuildConfig)
    ports/vector_store.rs    (existing, +count_active_vectors default method)
    ports/vector_index.rs    (NEW: VectorIndex trait)
    ↑
    ├── oneshim-storage
    │   sqlite/vector_index_impl.rs  (NEW: SqliteVectorIndex impl VectorIndex)
    │   migration.rs                 (V15: new tables)
    │
    └── oneshim-analysis
        adaptive_search.rs           (NEW: AdaptiveSearchCoordinator)
        vector_retriever.rs          (existing, updated to use coordinator)
```

No new crate. No new external dependency. No new cross-crate dependency
violations. `oneshim-analysis` already depends on both `oneshim-core` and
(transitively via DI) `oneshim-storage`.

## 4. Detailed Component Design

### 4.1 BinaryQuantizer

```rust
// oneshim-core/src/binary_quantizer.rs

/// Per-dimension quantile thresholds computed across the entire collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantileThresholds {
    pub q25: Vec<f32>,  // 25th percentile per dimension
    pub q50: Vec<f32>,  // 50th percentile (median)
    pub q75: Vec<f32>,  // 75th percentile
    pub dimensions: usize,
}

/// 2-bit binary code packed into bytes. For 384 dims = 96 bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryCode {
    pub data: Vec<u8>,
}

pub struct BinaryQuantizer;

impl BinaryQuantizer {
    /// Compute per-dimension quantile thresholds from a sample of vectors.
    ///
    /// For memory efficiency, accepts dequantized f32 values streamed through
    /// a callback rather than holding all vectors in memory simultaneously.
    /// Uses the P-square algorithm for approximate quantiles in O(1) memory.
    ///
    /// If the collection is small enough (<= 50K vectors), uses exact sorting.
    pub fn compute_thresholds(
        vectors: &[Vec<f32>],
        dimensions: usize,
    ) -> Result<QuantileThresholds, CoreError>;

    /// Encode a single vector to 2-bit binary code.
    ///
    /// Each dimension is mapped to 2 bits:
    ///   00 if value < q25
    ///   01 if q25 <= value < q50
    ///   10 if q50 <= value < q75
    ///   11 if value >= q75
    ///
    /// For 384 dimensions, output is 96 bytes (384 * 2 bits / 8).
    pub fn encode(
        vector: &[f32],
        thresholds: &QuantileThresholds,
    ) -> Result<BinaryCode, CoreError>;

    /// Bit-level Hamming distance between two binary codes.
    ///
    /// Counts set bits in the XOR of the two packed 2-bit codes. This is
    /// the standard approach used by Qdrant. Uses XOR + popcount which
    /// auto-vectorizes to POPCNT (x86) or CNT (ARM) instructions.
    /// For 96-byte codes: ~12 clock cycles.
    pub fn hamming_distance(a: &BinaryCode, b: &BinaryCode) -> u32;
}
```

**Threshold computation strategy**: For desktop-scale data (< 500K vectors), we
use exact quantile computation. Load all values for each dimension into a
temporary Vec, sort, and pick the 25th/50th/75th percentile indices. Memory
usage: one dimension at a time = 200K * 4 bytes = 800 KB peak. Total time:
384 dimensions * sort(200K) = 384 * ~3 ms = ~1.2 seconds. Acceptable for a
background build.

### 4.2 IVF Index

```rust
// oneshim-core/src/ivf_index.rs

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

impl IvfIndex {
    /// Build IVF index using k-means clustering on INT8 vectors.
    ///
    /// Algorithm (spherical k-means):
    /// 1. K-means++ initialization to select n_clusters initial centroids.
    /// 2. Lloyd's iteration: assign each vector to nearest centroid,
    ///    recompute centroids as the component-wise mean, then
    ///    L2-normalize each centroid to unit length.
    /// 3. Repeat for n_iterations rounds.
    ///
    /// All distance computations use ScalarQuantizer::cosine_similarity_int8.
    pub fn build(
        vectors: &[(i64, QuantizedVector)],
        config: &IvfBuildConfig,
    ) -> Result<IvfIndex, CoreError>;

    /// Find the nearest nprobe centroids to the query vector.
    pub fn nearest_centroids(
        &self,
        query: &QuantizedVector,
        nprobe: usize,
    ) -> Vec<usize>;  // cluster IDs, sorted by distance (nearest first)

    /// Assign a single new vector to its nearest centroid.
    /// Returns the cluster ID.
    pub fn assign(&self, vector: &QuantizedVector) -> usize;

    /// Get all vector IDs belonging to a cluster.
    pub fn get_cluster_members(&self, cluster_id: usize) -> Vec<i64>;
}
```

**Centroid recomputation in INT8 (spherical k-means)**: Centroids must be
computed as the mean of assigned vectors. Since vectors are stored as INT8, we
dequantize each vector back to f32 for the mean computation. After computing
the component-wise mean, normalize the centroid to unit length (L2-normalize).
This is required because cosine distance on non-unit vectors produces
inconsistent clustering -- without normalization, centroids drift toward the
origin over iterations, and cluster assignments become dominated by vector
magnitude rather than direction. After normalization, re-quantize the
resulting centroid back to INT8. This avoids precision loss from averaging in
the quantized domain.

**Memory usage during build**: The build method loads all vector IDs and INT8
data into memory. At 200K vectors * (8 bytes ID + 384 bytes INT8 + 8 bytes
scale/offset) = ~78 MB. The `HashMap<i64, usize>` for cluster assignments adds
~6-8 MB at 200K entries (~48 bytes per entry including HashMap bucket
overhead). Total peak: ~86 MB. This is within budget but close to the limit.
For collections > 200K, we sample 200K vectors for clustering and assign the
rest post-build.

### 4.3 SqliteVectorIndex

```rust
// oneshim-storage/src/sqlite/vector_index_impl.rs

pub struct SqliteVectorIndex {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteVectorIndex {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self;
}

#[async_trait]
impl VectorIndex for SqliteVectorIndex {
    async fn build_ivf_index(&self, n_clusters: usize, n_iterations: usize)
        -> Result<usize, CoreError>
    {
        // 1. SELECT id, vector_int8, quant_scale, quant_offset
        //    FROM embedding_vectors WHERE is_stale = 0 AND vector_int8 IS NOT NULL
        // 2. Build IvfIndex fully in memory (no lock held during computation)
        // 3. Acquire Mutex lock:
        //    BEGIN TRANSACTION;
        //      DELETE FROM ivf_assignments;
        //      DELETE FROM ivf_centroids;
        //      INSERT INTO ivf_centroids (all centroids in one batch);
        //      UPDATE vector_index_meta;
        //    COMMIT;
        //    Release lock.
        // 4. Batch INSERT assignments in chunks of 1000, each chunk in its
        //    own transaction (acquire lock, BEGIN, INSERT 1000, COMMIT,
        //    release lock). This prevents holding the Mutex for more than
        //    ~100ms at a time, allowing scheduler loops to interleave.
    }

    async fn search_ivf(&self, ...) -> Result<Vec<SearchResult>, CoreError> {
        // 1. Load centroids from ivf_centroids
        // 2. Find nearest nprobe centroids
        // 3. SELECT ... FROM embedding_vectors e
        //    JOIN ivf_assignments a ON e.id = a.vector_id
        //    WHERE a.cluster_id IN (?, ?, ...) AND e.is_stale = 0
        //    [+ filter conditions]
        // 4. Brute-force INT8 cosine similarity within the fetched subset
        // 5. Apply time decay, sort, truncate to limit
    }

    async fn search_ivf_binary(&self, ...) -> Result<Vec<SearchResult>, CoreError> {
        // 1. Load centroids, find nearest nprobe centroids
        // 2. SELECT bc.vector_id, bc.binary_code
        //    FROM vector_binary_codes bc
        //    JOIN ivf_assignments a ON bc.vector_id = a.vector_id
        //    WHERE a.cluster_id IN (?, ?, ...)
        // 3. Hamming distance filter: keep top (limit * oversample_factor)
        // 4. Load INT8 vectors for surviving candidates
        //    SELECT ... FROM embedding_vectors WHERE id IN (?, ?, ...)
        // 5. INT8 cosine similarity re-rank
        // 6. Apply time decay, sort, truncate to limit
    }
}
```

**Centroid caching**: Centroids are loaded from SQLite on first query and
cached in an `Arc<RwLock<Option<Vec<IvfCentroid>>>>`. The cache is invalidated
when `build_ivf_index` completes. At 447 centroids * 400 bytes = ~175 KB,
this is trivially small.

### 4.4 Background Index Builder

Index building runs in the existing scheduler infrastructure. A new loop is
added to the `src-tauri/src/scheduler/` or `oneshim-app/src/scheduler/`:

```rust
// New loop: index_maintenance (runs every 5 minutes)
async fn index_maintenance_loop(
    vector_store: Arc<dyn VectorStore>,
    vector_index: Arc<dyn VectorIndex>,
    config: &EmbeddingConfig,
) {
    let meta = vector_index.get_index_meta().await?;
    let total = meta.total_vector_count;
    let unindexed = meta.unindexed_count;

    // Determine if rebuild is needed
    let needs_rebuild = match &config.index_strategy {
        "brute_force" => false,
        _ => {
            if total < 10_000 { false }
            else if meta.ivf_built_at.is_none() { true }
            else { unindexed as f64 / total as f64 > 0.10 }  // > 10% new vectors
        }
    };

    if needs_rebuild {
        let n_clusters = (total as f64).sqrt() as usize;
        vector_index.build_ivf_index(n_clusters, 10).await?;

        if total > 100_000 {
            vector_index.build_binary_codes().await?;
        }
    } else if unindexed > 0 && meta.ivf_built_at.is_some() {
        // Incremental: assign new vectors to existing clusters
        // + generate binary codes for new vectors
    }
}
```

**Concurrency**: The index build runs on `tokio::task::spawn_blocking` (same
pattern as all SQLite operations). The existing `with_conn` pattern acquires
the Mutex for the SQLite connection. During the build, the Mutex is held for
batch operations (insert centroids, insert assignments) but released between
batches so normal queries are not blocked for more than ~100 ms at a time.

## 5. Implementation Plan

### Phase C.1: 2-Bit Binary Quantizer (~6h)

| Task | Crate | Estimate |
|------|-------|----------|
| `BinaryQuantizer` + `BinaryCode` + `QuantileThresholds` in core | oneshim-core | 2h |
| Exact quantile computation for <= 500K vectors | oneshim-core | 1h |
| `hamming_distance` with popcount optimization | oneshim-core | 0.5h |
| Unit tests: encode/decode, hamming distance, threshold edge cases | oneshim-core | 1.5h |
| Benchmark: Hamming vs INT8 distance at 384 dims | oneshim-core | 1h |

### Phase C.2: IVF Index (~8h)

| Task | Crate | Estimate |
|------|-------|----------|
| `IvfIndex` + k-means++ init + Lloyd's iteration | oneshim-core | 3h |
| `IvfBuildConfig` + auto cluster count | oneshim-core | 0.5h |
| Incremental assignment for new vectors | oneshim-core | 1h |
| Unit tests: clustering quality, assignment, nearest centroids | oneshim-core | 2h |
| Benchmark: build time at 10K / 50K / 100K vectors (synthetic) | oneshim-core | 1.5h |

### Phase C.3: Storage + Port Layer (~6h)

| Task | Crate | Estimate |
|------|-------|----------|
| `VectorIndex` port trait in core | oneshim-core | 1h |
| Migration (tentatively V15; 4 new tables) | oneshim-storage | 1h |
| `SqliteVectorIndex` implementation: build_ivf, search_ivf | oneshim-storage | 2h |
| `SqliteVectorIndex`: search_ivf_binary (two-stage) | oneshim-storage | 1.5h |
| Integration tests: build + search roundtrip via SQLite | oneshim-storage | 1.5h |

### Phase C.4: Adaptive Search + Integration (~5h)

| Task | Crate | Estimate |
|------|-------|----------|
| `AdaptiveSearchCoordinator` with strategy selection | oneshim-analysis | 2h |
| Wire into `VectorRetriever` (replace direct store calls) | oneshim-analysis | 1h |
| Config extension: `index_strategy`, `ivf_nprobe`, `binary_oversample_factor` | oneshim-core | 0.5h |
| Background index maintenance loop in scheduler | src-tauri / oneshim-app | 1h |
| End-to-end test: insert 10K+ vectors, verify adaptive strategy kicks in | oneshim-analysis | 1.5h |

### Phase C.5: Acceptance Testing (~3h)

| Task | Detail |
|------|--------|
| Recall test | Compare top-10 results: brute-force f32 vs IVF+INT8 vs IVF+binary+INT8 at 50K vectors. Target: >= 95% overlap. |
| Latency test | Measure p50/p95 search latency at 50K, 100K, 200K. Target: <= 5 ms p95 at 200K. |
| Memory test | Measure RSS during index build at 200K. Target: peak < 150 MB total process. |
| Backward compat | Verify existing brute-force path works unchanged with config `index_strategy: "brute_force"`. |

**Total estimate: ~28 hours**

## 6. Backward Compatibility

Phase C is fully backward compatible:

1. **New port, minimal changes to existing**: `VectorIndex` is a separate
   trait. The existing `VectorStore` trait gains only one default method
   (`count_active_vectors() -> Ok(0)`), so all existing implementations
   continue to compile without modification.
2. **Auto strategy defaults to brute-force at low counts**: For users with
   < 10K vectors (the vast majority during initial rollout), Phase C code is
   entirely dormant.
3. **Config defaults**: All new config fields have safe defaults
   (`index_strategy: "auto"`, `ivf_nprobe: 0`, `binary_oversample_factor: 10`).
4. **Migration is additive**: The new migration (tentatively V15; actual
   version assigned at implementation time) creates new tables; no existing
   columns or tables are modified.
5. **VectorRetriever remains the entry point**: The adaptive coordinator is
   wired behind the existing `VectorRetriever` interface; callers (dashboard,
   Tauri commands, context analyzer) see no API change.

## 7. Risks and Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| K-means clustering produces unbalanced partitions (one huge cluster, many empty) | Search degrades to near-brute-force for queries hitting the big cluster | Medium | Cap max partition size at 2x average; reassign overflow vectors. Monitor partition size distribution in index metadata. |
| 2-bit quantization recall is worse than expected at 384 dims | < 90% recall even with oversample=10 | Low | Parent spec already noted 1-bit needs >= 1024 dims; 2-bit at 384 should be adequate. **Automatic fallback**: if Phase C.1 benchmarking shows < 90% filter recall at oversample=10, `AdaptiveSearchCoordinator` skips the binary filter stage and uses IVF-only for the > 100K tier. Manual override: increase `binary_oversample_factor` in config. |
| Index build blocks SQLite mutex too long, causing UI jank | Scheduler loops stall waiting for lock | Medium | Break build into batches (1000 rows per transaction). Release and re-acquire lock between batches. |
| Memory spike during k-means build at 200K+ vectors | OOM on low-RAM machines (8 GB) | Low | Sample 200K vectors for clustering if total exceeds 200K. Peak memory for 200K is ~78 MB for vector data plus ~6-8 MB for the `HashMap<i64, usize>` assignments (~48 bytes per entry with HashMap overhead). Total ~86 MB, well below even 8 GB. |
| Centroid cache becomes stale after rebuild | Search uses old centroids | Low | Cache is invalidated atomically when build completes (RwLock write). |
| SQLite WAL file grows large during bulk index inserts | Disk space spike | Low | Run `PRAGMA wal_checkpoint(TRUNCATE)` after build completes. |

## 8. Future Considerations

- **HNSW**: If collections grow to 1M+ vectors (unlikely for desktop 90-day
  retention), HNSW would be more appropriate. Would require an in-memory graph
  structure (~500 MB at 1M vectors) -- out of scope for desktop.
- **Product Quantization**: Could replace the 2-bit scheme for even higher
  compression. Requires training a codebook, adds significant complexity.
  Revisit only if storage becomes a critical constraint.
- **SIMD-explicit**: The current implementation relies on LLVM auto-
  vectorization. Explicit SIMD (via `std::simd` or `core_arch`) could provide
  2-3x additional speedup for distance computations, but adds platform-specific
  code. Consider only if latency targets are missed.
- **Incremental centroid update**: Instead of full rebuild, update centroids
  incrementally using online k-means. Reduces rebuild cost but adds complexity.
  Implement only if rebuild frequency becomes a problem.

## 9. References

- [P3 Vector Compression + Embedding Optimization](2026-03-19-p3-vector-compression-embedding-optimization-design.md) -- parent spec
- [Qdrant Binary Quantization Guide](https://qdrant.tech/articles/binary-quantization/) -- 2-bit approach inspiration
- [Faiss IVF Documentation](https://github.com/facebookresearch/faiss/wiki/Faiss-indexes#cell-probe-methods-indexivf-indexes) -- IVF design reference
- [K-means++ Initialization](https://theory.stanford.edu/~sergei/papers/kMeansPP-soda.pdf) -- Arthur & Vassilvitskii, 2007
- [SQLite BLOB Performance](https://www.sqlite.org/intern-v-extern-blob.html) -- guidance on BLOB sizing for our use case
- [ADR-013: LLM Segment Summary + Vector RAG](../../architecture/ADR-013-llm-summary-vector-rag.md) -- parent ADR
