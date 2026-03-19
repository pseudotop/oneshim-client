# Layer 3: Server Migration Feasibility Assessment

| Field | Value |
|-------|-------|
| Date | 2026-03-19 |
| Status | Assessment (pre-implementation) |
| Scope | Client analysis pipeline -> Server AI Intelligence domain migration |

## 1. Executive Summary

The client's `oneshim-analysis` crate implements a sophisticated standalone analysis pipeline (ADR-011/012/013) with 33+ source files covering pattern mining, adaptive segmentation, regime detection, embedding/RAG, and digest generation. The server has parallel but independent infrastructure in the `ai_intelligence` and `user_context` domains. Migration is feasible but requires significant API contract work and new server-side services. A hybrid approach (Option B) is recommended.

## 2. Client Component to Server Target Mapping

| Client Component | Server Target | Migration Complexity | Notes |
|-----------------|---------------|---------------------|-------|
| **PatternMiner** (Rust, pure algorithm) | `user_context/services/analysis/pattern_analysis_service.py` | **Medium** | Server already has `PatternAnalysisService` with similar goals (app patterns, time patterns, switching patterns). Logic is pure algorithmic -- straightforward port. Server version uses `ContextAnalysisDomainService`. |
| **ContextAssembler** (Rust, pure builder) | `ai_intelligence/services/ai_context_intelligence/context_analysis_service.py` | **Medium** | Server has `ContextAnalysisService` + `ContextUnderstandingDomainService`. The assembler's structured JSON output format must be standardized as a shared contract. |
| **ContextAnalyzer** (Rust, orchestrator) | New orchestrator in `user_context/services/analysis/` or `ai_intelligence` | **High** | Central orchestrator consuming PatternMiner + Assembler + AnalysisProvider + VectorRetriever. Server needs equivalent orchestration. May span two domains (user_context for data, ai_intelligence for LLM calls). |
| **AdaptiveTrigger** (Rust, real-time) | **Stays on client** | **N/A** | Real-time signal processing (Dual-EWMA, hysteresis gate) requires <100ms latency on desktop events. Cannot tolerate network round-trip. The trigger decisions feed into batch uploads as metadata. |
| **RegimeDetector** (Rust, k-means) | `ai_intelligence/services/ai_context_intelligence/predictive_insight_service.py` | **Medium** | Hand-rolled k-means is straightforward to port to Python (numpy/scikit-learn). Server could do better with cross-user regime learning. Maps to `PredictiveInsightService` pattern. |
| **RegimeClassifier** (Rust, real-time) | **Stays on client** (lite version) / server for batch reclassification | **Low (client)** / **Medium (server)** | Real-time nearest-centroid matching must stay on client. Server can provide regime model updates. |
| **EmbeddingPipeline** (Rust, fastembed-rs) | Server-side embedding via DSPy or direct API | **Low** | Server already has `prompt_embedding_port.py` in AI Intelligence. Neo4j/Memgraph supports vector indexes. Replace client's `sqlite-vec` with graph DB vector search. |
| **VectorRetriever** (Rust, sqlite-vec) | Neo4j/Memgraph vector search | **Low** | Server's graph DB already supports vector embeddings. Replace brute-force/sqlite-vec with native graph DB KNN. Better scalability. |
| **WeeklyDigestGenerator** (Rust, pure algorithm) | New service in `user_context/services/analysis/` | **Low** | Pure algorithm taking segments -> digest. Direct port to Python. Server can aggregate across sessions/devices. |
| **DailyDigestGenerator** (Rust, pure algorithm) | New service in `user_context/services/analysis/` | **Low** | Same as weekly -- pure algorithm, direct port. |
| **DailyInsightGenerator** (Rust, LLM-based) | `ai_intelligence/services/ai_context_intelligence/` | **Medium** | Requires LLM call. Server uses DSPy which is more sophisticated than client's raw `AnalysisProvider`. Good fit for DSPy prompt optimization. |
| **LlmSegmentSummarizer** (Rust, LLM-based) | New service in `ai_intelligence` | **Low** | Simple LLM summarization call. DSPy handles this natively. |
| **HybridSearchService** (Rust) | New service combining graph DB vector + text search | **Medium** | Server has both Neo4j full-text and vector capabilities. Needs unified search service. |
| **WorkTypeClassifier** (Rust, pure algorithm) | `user_context/services/analysis/` | **Low** | Pure algorithm mapping input patterns to work types. Direct port. |
| **SuggestionFilter** (Rust, regime-aware) | `user_context/services/suggestion/suggestion_ranking_service.py` | **Low** | Server already has priority ranking. Add regime-aware filtering rules. |

## 3. Server Infrastructure Readiness

### 3.1 Batch Data Reception -- READY

The server already has the complete pipeline:

- **REST**: `POST /user_context/batches` (`BatchUploadRequest` with events + frames + session_id + compression)
- **gRPC**: `ClientContext.UploadBatch` RPC (proto contract in `api/proto/oneshim/client/v1/context.proto`)
- **Service**: `BatchUploadService` processes events and frames with sync sequence management
- **Format alignment**: Client `BatchUploader` (oneshim-network) already speaks this format

**Gap**: The current batch format transmits raw events/frames but does NOT include:
- Segment summaries (from AdaptiveTrigger)
- Regime metadata (current regime_id, regime features)
- Pattern mining results (ActivityPatterns)
- Content activity labels (from ContentTracker/TitleBarParser)

These must be added to the batch payload or as a separate enriched upload endpoint.

### 3.2 AI Intelligence Domain -- PARTIALLY READY

The server's AI Intelligence domain has:
- `ContextAnalysisService` -- analyzes context with `ContextUnderstandingDomainService`
- `PredictiveInsightService` -- generates predictive insights
- `WorkflowRecommendationService` -- recommends workflows
- DSPy pipeline infrastructure with prompt optimization
- `prompt_embedding_port.py` -- embedding generation port
- 8 domain services reused across 14 application services

**Gap**: No equivalent of:
- `ContextAnalyzer` orchestrator (the end-to-end analysis cycle)
- Segment-based analysis (server works with raw events, not segments)
- Regime-aware suggestion generation
- Time-decayed vector RAG retrieval

### 3.3 Vector/Embedding Infrastructure -- READY

- Neo4j/Memgraph natively supports vector indexes
- Server has `prompt_embedding_port.py` for embedding generation
- Graph DB vector search replaces client's sqlite-vec entirely
- Better scalability: no 500MB local storage limit

### 3.4 Suggestion Delivery -- READY

- **SSE**: `GET /user_context/suggestions/stream` -- existing endpoint
- **gRPC**: `ClientSuggestion.Subscribe` -- server-streaming RPC
- **Service**: `GrpcSuggestionStreamService` -- real-time suggestion delivery
- **Suggestion priority**: Full priority orchestration pipeline (`SuggestionPriorityOrchestrator`, `PriorityCalculationService`, `PriorityRuleEngine`, `DynamicPriorityAdjustmentService`, `SuggestionRankingService`)

**Status**: Ready to deliver server-generated suggestions to clients.

### 3.5 DSPy vs Client AnalysisProvider

| Aspect | Client (AnalysisProvider) | Server (DSPy) |
|--------|--------------------------|---------------|
| Interface | `analyze(context_json, system_prompt) -> Vec<Suggestion>` | DSPy modules with typed signatures |
| Prompt management | Hardcoded `ANALYSIS_SYSTEM_PROMPT` | Template-based, optimizable |
| Optimization | None | DSPy teleprompt optimization |
| Multi-model | Single provider (OpenAI/local) | Multi-model orchestration |
| Cost | Per-client LLM calls | Centralized, batch-optimizable |

**Conclusion**: Server's DSPy is strictly superior. Migration eliminates redundant per-client LLM costs and enables prompt optimization across users.

## 4. Data Flow Analysis

### 4.1 Current Flow (Standalone)
```
Desktop Events -> Monitor -> Storage (SQLite)
                                  |
                          ContextAnalyzer (local LLM)
                                  |
                          Suggestion -> UI
```

### 4.2 Target Flow (Server Mode)
```
Desktop Events -> Monitor -> AdaptiveTrigger (stays local)
                                  |
                          Segment Close -> Enriched Batch
                                  |
                          BatchUploader -> Server /user_context/batches
                                  |
                          Server: user_context receives + stores in Neo4j
                                  |
                          Server: ai_intelligence analyzes (DSPy)
                                  |
                          Server: generates Suggestions
                                  |
                          gRPC Stream / SSE -> Client SuggestionReceiver
                                  |
                          Client: priority queue -> UI
```

### 4.3 Client Changes Required

1. **Already exists**: `SuggestionSource::LlmServer` enum variant
2. **Already exists**: Coexistence rule in ADR-011 section 5 -- "When server is active... local LLM analysis is suppressed"
3. **New**: Enriched batch payload with segment metadata + regime info + patterns
4. **New**: Config flag to switch between standalone/server/hybrid modes
5. **Modify**: `ContextAnalyzer` to skip LLM call when server mode active (keep PatternMiner + Assembler for local enrichment of batch data)

### 4.4 Server Changes Required

1. **New endpoint or enriched batch**: Accept segment summaries + regime data alongside raw events
2. **New orchestrator**: Server-side `ContextAnalyzer` equivalent in `user_context/services/analysis/`
3. **New services**: WeeklyDigest, DailyDigest generators (port from Rust)
4. **New**: Regime detection as a Celery background task (daily, cross-user)
5. **Modify**: `BatchUploadService` to trigger AI Intelligence analysis pipeline after batch processing
6. **Modify**: Suggestion generation to use DSPy with segment-enriched context

### 4.5 API Contract Changes

The current `UploadBatchRequest` proto needs extension:

```protobuf
message UploadBatchRequest {
  string session_id = 1;
  repeated ClientEvent events = 2;
  repeated FrameMetadata frames = 3;
  // NEW fields for enriched batch
  repeated SegmentSummary segments = 4;       // Closed segments since last batch
  RegimeInfo current_regime = 5;              // Active regime metadata
  repeated ActivityPattern patterns = 6;      // Detected patterns
  ClientCapabilities capabilities = 7;        // What the client can do locally
}
```

## 5. Migration Strategy Assessment

### Option A: Full Migration (Client becomes thin collector)

**Client keeps**: Monitor, Vision, AdaptiveTrigger, BatchUploader
**Server takes**: All analysis, pattern mining, regime detection, embedding, RAG, digests, suggestions

| Aspect | Estimate |
|--------|----------|
| Server-side changes | 4-6 weeks (new orchestrator, port algorithms, DSPy integration) |
| Client-side changes | 1-2 weeks (strip analysis, enriched batch, mode switch) |
| API contract work | 1 week (proto extension, REST schema update) |
| Testing | 2 weeks (integration, E2E, latency validation) |
| **Total** | **8-11 weeks** |

**Pros**: Single source of truth for analysis; cross-user insights; no per-client LLM cost.
**Cons**: Higher latency for suggestions (batch cycle + server processing); offline mode degraded; requires server availability.

### Option B: Hybrid (Recommended)

**Client keeps**: AdaptiveTrigger (real-time), RegimeClassifier (real-time), PatternMiner (local enrichment), ContextAssembler (batch prep), FocusAnalyzer (rule-based suggestions)
**Server takes**: LLM analysis (DSPy), regime detection (daily), embedding/RAG (graph DB), weekly/daily digests, cross-user insights, suggestion generation

| Aspect | Estimate |
|--------|----------|
| Server-side changes | 3-4 weeks (analysis orchestrator, DSPy pipeline, digest services) |
| Client-side changes | 1 week (mode switch, enriched batch, suppress local LLM) |
| API contract work | 1 week (proto extension) |
| Testing | 1.5 weeks (integration, mode switching, fallback) |
| **Total** | **6.5-7.5 weeks** |

**Pros**: Best of both worlds; real-time local + intelligent server; graceful offline fallback; client already has standalone capability.
**Cons**: Two codepaths to maintain (but already architected for this via `SuggestionSource`).

### Option C: Federation (Both analyze, server adds cross-user)

**Client keeps**: Everything (standalone pipeline fully operational)
**Server adds**: Cross-user pattern aggregation, organizational insights, centralized regime library

| Aspect | Estimate |
|--------|----------|
| Server-side changes | 5-7 weeks (new cross-user analysis domain, aggregation pipeline) |
| Client-side changes | 0.5 weeks (upload analysis results as telemetry) |
| API contract work | 1 week (new analysis result upload endpoint) |
| Testing | 2 weeks (privacy validation, aggregation accuracy) |
| **Total** | **8.5-10.5 weeks** |

**Pros**: No client degradation; additive value; privacy-preserving aggregation possible.
**Cons**: Redundant LLM costs (every client still calls LLM); complexity of merging local + server suggestions; no cost savings.

## 6. Blockers and Prerequisites

### Must-Have Before Migration

1. **Enriched batch proto contract** -- Extend `UploadBatchRequest` with segment/regime/pattern fields. Both client and server must agree on the schema. Proto-first design.

2. **Server-side segment storage** -- Neo4j/Memgraph schema for `SegmentSummary`, `Regime`, `CalibrationEntry`. Currently the server stores raw events but not processed segments.

3. **DSPy pipeline for desktop context analysis** -- The server's DSPy infrastructure exists but no pipeline specifically for desktop activity analysis + suggestion generation. Need to create a DSPy module that replaces the client's `ANALYSIS_SYSTEM_PROMPT`.

4. **Mode negotiation protocol** -- Client must discover server capabilities at session start. New field in session handshake: `server_analysis_capabilities: [pattern_mining, regime_detection, llm_analysis, digest_generation]`. Client disables local equivalents accordingly.

5. **Suggestion deduplication** -- When both local (rule-based) and server (LLM) produce suggestions, deduplication logic is needed. Client already has priority-based dedup in `oneshim-suggestion/queue.rs` -- server suggestions should carry `source: LLM_SERVER` for proper handling.

### Nice-to-Have

6. **Celery task for regime detection** -- Background daily task aggregating calibration data across sessions. Leverages server's existing Celery infrastructure.

7. **Cross-user regime sharing** -- Server can offer organization-wide regime templates. Privacy-sensitive: only regime parameters (centroid vectors), not raw user data.

8. **Vector embedding migration** -- One-time migration of client's local sqlite-vec data to server's Neo4j vector index. Optional: client can re-upload historical segments.

## 7. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Suggestion latency increase (batch cycle 10s + server processing) | High | Medium | Client keeps rule-based suggestions for immediate feedback; server suggestions arrive asynchronously |
| Server unavailability degrades analysis | Medium | High | Client maintains full standalone capability; automatic fallback to local LLM when server unreachable |
| Batch payload size increase (segments + regime data) | Medium | Low | Compression already supported (gzip/zstd/lz4); segment summaries are ~1KB each |
| DSPy prompt quality vs hardcoded prompt | Low | Medium | A/B test server DSPy vs client's `ANALYSIS_SYSTEM_PROMPT`; DSPy optimization should exceed static prompts |
| Privacy: regime data reveals work patterns | Medium | Medium | Regime features are aggregated (no raw content); GDPR consent gating already in place |

## 8. Recommendation

**Proceed with Option B (Hybrid)** as the migration strategy:

1. **Phase 1 (Week 1-2)**: Extend proto contracts and batch payload. Add segment/regime/pattern fields to `UploadBatchRequest`. Server stores enriched batch data in Neo4j.

2. **Phase 2 (Week 3-4)**: Build server-side analysis orchestrator. Port `PatternMiner` logic to `user_context/services/analysis/`. Create DSPy pipeline for context analysis. Wire `BatchUploadService` to trigger analysis after batch processing.

3. **Phase 3 (Week 5-6)**: Implement server suggestion generation. Connect analysis results to `GrpcSuggestionStreamService`. Port digest generators. Add mode negotiation to session handshake.

4. **Phase 4 (Week 6.5-7.5)**: Client mode switching. Suppress local LLM when server active. Integration testing. Latency and accuracy benchmarking.

The hybrid approach leverages the client's existing standalone capability as a fallback while centralizing the expensive LLM and embedding operations on the server. The architecture already supports this via `SuggestionSource::LlmServer` and the coexistence rules defined in ADR-011 section 5.
