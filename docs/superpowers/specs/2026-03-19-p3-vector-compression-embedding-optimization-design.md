# P3: Vector Compression + Embedding Optimization — Design Spec

> Created: 2026-03-19
> Status: Draft
> Depends on: Layer 2 LLM Summary + Vector RAG (implemented)

## 1. Goal

Reduce storage footprint and improve search speed for the local vector store
(currently 384-dim float32 = 1,536 bytes per vector) while preserving retrieval
quality. Secondary goal: explore lightweight ways to improve embedding relevance
for desktop-activity data without requiring GPU-based fine-tuning.

Targets at 50K vectors (90-day retention):

| Metric | Current | Target |
|--------|---------|--------|
| Storage per vector | 1,536 B | ~384 B (4x reduction) |
| Total vector storage | ~75 MB | ~19 MB |
| Search latency (50K brute-force) | ~12 ms | ~4 ms |
| Recall@10 vs float32 baseline | 100% | >= 97% |

## 2. Design Decisions

### 2.1 Compression: Scalar Quantization (INT8) — chosen

| Option | Compression | Recall@10 | Complexity | Verdict |
|--------|-------------|-----------|------------|---------|
| **Scalar INT8** | **4x** | **~99%** | **Low** | **Selected** |
| Binary (1-bit) | 32x | ~85-90% | Low | Rejected — quality loss too steep for 384-dim vectors. Binary quantization needs >= 1024 dims to perform well. |
| Product Quantization | 64-97x | ~95% | High | Rejected — training phase needs 10K+ vectors minimum; adds codebook management complexity; overkill for < 50K vectors. |
| Matryoshka truncation | 3x (384 -> 128) | model-dependent | Low | Rejected — all-MiniLM-L6-v2 was NOT trained with Matryoshka loss; naive truncation degrades quality unpredictably. |
| 1.5-bit / 2-bit | 16-21x | ~92-95% | Medium | Future — interesting middle ground (Qdrant v1.15+ pioneered this), revisit if storage pressure grows. |

**Rationale:** Scalar INT8 provides a predictable 4x compression with negligible
recall loss (~99%) across all embedding models. It works well at any scale and
any dimensionality. No training phase, no codebook, no model dependency. The
`lnmp-quant` Rust crate provides a ready implementation.

### 2.2 Search Acceleration: Quantized Distance on INT8

Cosine similarity on INT8 vectors uses integer dot products, which SIMD auto-
vectorizes well. Expected 3-4x speedup over float32 brute-force for 384 dims.
No index structure needed at < 50K vectors.

### 2.3 Embedding Optimization: Retrieval-Augmented Query Expansion — chosen

| Option | Feasibility on desktop | Quality gain | Complexity | Verdict |
|--------|----------------------|--------------|------------|---------|
| **Query expansion** | **Excellent** | **Medium** | **Low** | **Selected** |
| LoRA fine-tuning | Poor — ONNX Runtime lacks gradient ops on GPU; CPU-only training is 100x slower | High | Very High | Rejected |
| Contrastive fine-tuning | Poor — same ONNX limitation; also needs curated pairs | High | Very High | Rejected |
| Domain vocabulary injection | Moderate — model retraining needed | Medium | High | Rejected |
| Adapter meta-learning | Moderate — no GPU needed, convex combination of pre-trained adapters | Medium-High | High | Future — promising when adapter banks mature |

**Rationale:** Fine-tuning embedding models on a desktop is not practical today.
ONNX Runtime does not support GPU-accelerated gradient computation, so training
falls back to CPU — prohibitively slow even with LoRA. Instead, we improve
retrieval quality at query time:

1. **Query expansion** — prepend activity context (current app, window title,
   recent segment labels) to the raw query before embedding. This biases the
   query vector toward the user's current work context without touching the model.
2. **Relevance feedback re-ranking** — after initial vector search, re-rank
   results using lightweight heuristics (content type match, recency boost,
   same-regime bonus). Already partially implemented via `time_decay`.
3. **Negative feedback filtering** — if the user dismisses a suggestion, store
   the segment_id and down-weight it in future searches.

### 2.4 Storage Format: Dual-store with lazy migration

Store INT8 quantized vectors alongside float32 originals during transition.
New vectors are written in both formats. Old vectors are quantized lazily in a
background maintenance task (same pattern as `mark_stale` / `get_stale_vectors`).

## 3. Architecture

### 3.1 Quantization Module (new, in `oneshim-core`)

```
oneshim-core/src/
└── quantization.rs    # Pure functions, no async, no dependencies
    ├── ScalarQuantizer
    │   ├── quantize(Vec<f32>) -> QuantizedVector
    │   ├── dequantize(&QuantizedVector) -> Vec<f32>
    │   └── cosine_similarity_quantized(&QuantizedVector, &QuantizedVector) -> f32
    └── QuantizedVector { data: Vec<i8>, scale: f32, offset: f32 }
```

**Why oneshim-core?** Quantization is a pure math operation with no I/O. It
belongs in the domain core alongside models and error types. Both `oneshim-storage`
(for persisting) and `oneshim-analysis` (for search) depend on core already.

### 3.2 VectorStore Port Extension

```rust
// Added to existing VectorStore trait in oneshim-core/src/ports/vector_store.rs

/// Store a pre-quantized INT8 vector alongside its float32 original.
async fn store_quantized(
    &self,
    vector_f32: Vec<f32>,
    vector_int8: &QuantizedVector,
    metadata: EmbeddingMetadata,
) -> Result<(), CoreError>;

/// Search using INT8 quantized distance (faster, approximate).
async fn search_quantized(
    &self,
    query_vector: &QuantizedVector,
    limit: usize,
    time_decay_hours: f32,
) -> Result<Vec<SearchResult>, CoreError>;

/// Background: quantize one batch of float32-only vectors. Returns count.
async fn backfill_quantized(&self, batch_size: usize) -> Result<u64, CoreError>;
```

Existing `store()` / `search()` methods remain unchanged for backward compatibility.

### 3.3 SQLite Schema Addition

```sql
-- V8 migration: add INT8 quantized column
ALTER TABLE embedding_vectors
  ADD COLUMN vector_int8 BLOB;          -- INT8 quantized data
ALTER TABLE embedding_vectors
  ADD COLUMN quant_scale REAL;          -- scalar quantization scale factor
ALTER TABLE embedding_vectors
  ADD COLUMN quant_offset REAL;         -- scalar quantization offset
```

### 3.4 Query Expansion Module (in `oneshim-analysis`)

```
oneshim-analysis/src/
└── query_expansion.rs
    ├── QueryExpander
    │   ├── expand(raw_query: &str, context: &ActivityContext) -> String
    │   └── expand_with_feedback(raw_query: &str, context: &ActivityContext,
    │                            negative_ids: &[String]) -> String
    └── ActivityContext { app_name, window_title, recent_labels, regime }
```

**Flow:**
```
User query: "meeting notes"
           ↓
QueryExpander.expand() → "Slack Zoom meeting notes standup daily"
           ↓
EmbeddingProvider.embed() → query vector (384-dim f32)
           ↓
ScalarQuantizer.quantize() → query vector (384-dim i8)
           ↓
VectorStore.search_quantized() → top-k candidates
           ↓
Re-rank with relevance heuristics → final results
```

### 3.5 Crate Dependency Map

```
oneshim-core  (quantization.rs, QuantizedVector model)
    ↑
    ├── oneshim-storage   (SqliteVectorStore: store_quantized, search_quantized)
    ├── oneshim-embedding (unchanged — produces f32 vectors as before)
    └── oneshim-analysis  (query_expansion.rs, uses quantizer + vector store)
```

No new crate. No new cross-crate dependencies. Clean hexagonal layering.

## 4. Phase Scope

### Phase A: Scalar Quantization (storage + search)

| Task | Crate | Estimate |
|------|-------|----------|
| `ScalarQuantizer` + `QuantizedVector` in core | oneshim-core | 2h |
| Unit tests: quantize/dequantize roundtrip, similarity accuracy | oneshim-core | 1h |
| SQLite V8 migration: add INT8 columns | oneshim-storage | 1h |
| `store_quantized` / `search_quantized` implementation | oneshim-storage | 3h |
| `backfill_quantized` background task | oneshim-storage | 1h |
| Wire quantized path into `embedding_pipeline.rs` | oneshim-analysis | 1h |
| Integration tests: store → search roundtrip with INT8 | oneshim-storage | 1h |
| Config: `analysis.embedding.quantization_enabled` flag | oneshim-core | 0.5h |

**Deliverable:** 4x storage reduction, ~3x search speedup. Zero recall regression
on acceptance test (compare top-10 results between f32 and INT8 paths).

### Phase B: Query Expansion + Relevance Feedback

| Task | Crate | Estimate |
|------|-------|----------|
| `QueryExpander` with activity context injection | oneshim-analysis | 2h |
| `ActivityContext` model in core | oneshim-core | 0.5h |
| Negative feedback store (dismissed suggestion → segment_id blocklist) | oneshim-storage | 1h |
| Re-ranking heuristics (content type, regime, recency) | oneshim-analysis | 2h |
| Wire into `HybridSearchService` | oneshim-analysis | 1h |
| Unit + integration tests | oneshim-analysis | 2h |

**Deliverable:** Measurably better relevance for activity-context queries vs
raw embedding search. Feedback loop for suggestion dismissals.

### Phase C (Future): Advanced Compression

- Evaluate 2-bit quantization if storage exceeds 100 MB
- Evaluate IVF index if vector count exceeds 100K
- Revisit adapter meta-learning when Rust adapter banks become available

## 5. Crate Placement Summary

| Component | Crate | Rationale |
|-----------|-------|-----------|
| `ScalarQuantizer`, `QuantizedVector` | `oneshim-core` | Pure domain logic, no I/O |
| `ActivityContext` model | `oneshim-core` | Shared model |
| INT8 storage + search | `oneshim-storage` | SQLite adapter |
| `QueryExpander` | `oneshim-analysis` | Analysis pipeline |
| Re-ranking heuristics | `oneshim-analysis` | Search post-processing |
| Negative feedback store | `oneshim-storage` | Persistence adapter |
| Config flags | `oneshim-core` (`EmbeddingConfig`) | Existing config section |

## 6. Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| INT8 quantization hurts recall for edge-case queries | Medium | Keep float32 path as fallback; A/B comparison in tests |
| Query expansion adds noise for short queries | Low | Only expand when context is available; passthrough for explicit searches |
| SQLite BLOB size increase from dual-store | Low | INT8 adds 384 bytes (25% overhead); backfill task drops f32 column eventually |
| `lnmp-quant` crate unmaintained | Low | Quantization is < 50 lines of code; inline if needed |

## 7. References

- [Qdrant Quantization Guide](https://qdrant.tech/documentation/guides/quantization/)
- [HuggingFace Embedding Quantization Blog](https://huggingface.co/blog/embedding-quantization)
- [lnmp-quant crate](https://crates.io/crates/lnmp-quant) — Rust INT8/INT4/binary quantization
- [vq crate](https://github.com/CogitatorTech/vq) — Rust BQ/SQ/PQ library
- [ONNX Runtime Training Limitations](https://github.com/microsoft/onnxruntime/discussions/21447)
- [Sentence Transformers Matryoshka Docs](https://sbert.net/examples/sentence_transformer/training/matryoshka/README.html)
