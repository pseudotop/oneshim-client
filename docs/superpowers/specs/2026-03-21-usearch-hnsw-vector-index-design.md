# USearch HNSW Vector Index Design Spec

**Date:** 2026-03-21
**Priority:** P1
**Effort:** 5 days
**Status:** Proposed
**Impact:** No new crates, no schema migration. Feature-gated (`hnsw` cargo feature).

---

## 1. Current State

3-tier adaptive vector search via `AdaptiveSearchCoordinator` (`crates/oneshim-analysis/src/adaptive_search.rs`):

| Strategy | Range | Implementation |
|---|---|---|
| `BruteForceInt8` | < 10,000 | Full scan, INT8 cosine |
| `IvfInt8` | 10K-100K | IVF k-means++ |
| `IvfBinaryRerank` | >= 100K | IVF + 2-bit Hamming + re-rank |

**Ports:** `VectorStore` (109 LOC) + `VectorIndex` (135 LOC, IVF-centric).
**Storage:** `SqliteVectorStore` + `SqliteVectorIndex` (directory modules).
**IVF tables (V16):** `ivf_centroids`, `ivf_assignments`, `vector_binary_codes`, `vector_index_meta`.

---

## 2. Library Selection

**Primary: `usearch v2.24.0`**

| Criterion | usearch | hnswlib-rs (fallback) |
|---|---|---|
| Type | C++ core + Rust FFI | Pure Rust |
| SIMD | AVX2/AVX-512/NEON/SVE (SimSIMD) | LLVM auto-vectorization |
| INT8 | Native i8, f16, bf16 | f32, f16, per-vector int8 |
| Apple Silicon | Auto NEON (3-8x speedup) | No explicit NEON |
| Concurrency | Concurrent add + search | Lock-free reads + mutation |
| Binary size | +2-5MB | +200-500KB |

**NOT recommended:** `instant-distance` (dormant), `hora` (abandoned), `sqlite-vec` (no ANN, 17x slower).

### Known Issues

- **Send + Sync:** USearch `Index` not natively `Send + Sync` ([#482](https://github.com/unum-cloud/usearch/issues/482)). Wrap with `Mutex<Index>` or `unsafe impl`.
- **Thread crash:** Exceeding `hardware_concurrency()` ([#389](https://github.com/unum-cloud/usearch/issues/389)). Cap to `num_cpus::get() - 1`.

---

## 3. Proposed Change

**4th strategy complementing IVF:**

| Strategy | Range | Index Type |
|---|---|---|
| `BruteForceInt8` | < 5,000 | None (scan all) |
| **`HnswInt8`** | **5K-50K** | **In-memory HNSW (usearch)** |
| `IvfInt8` | 50K-100K | SQLite IVF |
| `IvfBinaryRerank` | >= 100K | IVF + Hamming |

**Memory analysis (384-dim INT8):**

| Vectors | Graph | INT8 copy | Total | IVF equiv |
|---|---|---|---|---|
| 5K | 0.7MB | 1.8MB | **2.5MB** | 8MB |
| 25K | 3.4MB | 9.2MB | **13MB** | 12MB |
| 50K | 6.7MB | 18.4MB | **25MB** | 16MB |

---

## 4. Architecture Impact

Add HNSW methods to existing `VectorIndex` trait (no new trait).

| File | Change |
|---|---|
| `crates/oneshim-analysis/src/adaptive_search.rs` | `HnswInt8` variant, `HnswIndex` wrapper field |
| `crates/oneshim-analysis/Cargo.toml` | `usearch = { version = "2", optional = true }` |
| `crates/oneshim-core/src/config/sections.rs` | `hnsw_enabled`, `hnsw_max_vectors` in `SearchConfig` |
| `crates/oneshim-core/src/ports/vector_store.rs` | Dimensionality validation |

**New:** `crates/oneshim-analysis/src/hnsw_index.rs` — build, search, serialize/load, Send+Sync shim.

## 5. Migration/Compatibility

- Feature `hnsw` disabled: unchanged 3-strategy ladder.
- Graph in-memory only, rebuilt on startup from `embedding_vectors`.
- `forced_strategy` gains `"hnsw"`.
- IVF tables untouched.

## 6. Effort

| Task | Estimate |
|---|---|
| `HnswIndex` wrapper (build/search/serialize + Send+Sync) | 2 days |
| `AdaptiveSearchCoordinator` strategy update | 1 day |
| Config + feature flag + dimensionality validation | 0.5 day |
| Tests (unit + integration) | 1 day |
| Benchmarks | 0.5 day |
| **Total** | **5 days** |

## 7. Phased Rollout

| Phase | Scope |
|---|---|
| **A** | `HnswIndex` wrapper + unit tests. Feature-gated. |
| **B** | Coordinator integration + strategy selection. |
| **C** | Benchmarks + threshold tuning. |
| **D** | Optional: persist serialized graph to disk. |

## 8. Sources

- [USearch GitHub](https://github.com/unum-cloud/USearch), [#482](https://github.com/unum-cloud/usearch/issues/482), [#389](https://github.com/unum-cloud/usearch/issues/389)
- [hnswlib-rs](https://github.com/jean-pierreBoth/hnswlib-rs)
- [sqlite-vec](https://github.com/asg017/sqlite-vec), [ANN #25](https://github.com/asg017/sqlite-vec/issues/25)
