# Architecture Improvements Design Spec

**Date:** 2026-03-21
**Priority:** P1 (items 1-2), P2 (items 3-4)
**Status:** Proposed
**MCP Server:** Deferred (security concerns + Skills trend)

---

## Table of Contents

1. [USearch HNSW Vector Index (P1)](#1-usearch-hnsw-vector-index-p1)
2. [Tauri IPC Optimization (P1)](#2-tauri-ipc-optimization-p1)
3. [Audio Capture + Whisper STT (P2)](#3-audio-capture--whisper-stt-p2)
4. [SQLite Performance Tuning (P2)](#4-sqlite-performance-tuning-p2)

---

## 1. USearch HNSW Vector Index (P1)

### 1.1 Current State

The codebase implements a 3-tier adaptive vector search system managed by
`AdaptiveSearchCoordinator` in `crates/oneshim-analysis/src/adaptive_search.rs`:

| Strategy | Vector Count Range | Implementation |
|---|---|---|
| `BruteForceInt8` | < 10,000 | Full table scan with INT8 cosine similarity |
| `IvfInt8` | 10,000 -- 100,000 | IVF k-means++ clustering with nprobe partition scan |
| `IvfBinaryRerank` | >= 100,000 | IVF + 2-bit Hamming filter + INT8 re-rank |

**Port traits involved:**

- `VectorStore` (`crates/oneshim-core/src/ports/vector_store.rs`) -- CRUD + brute-force search + quantized search (109 LOC trait)
- `VectorIndex` (`crates/oneshim-core/src/ports/vector_index.rs`) -- IVF build/search + binary codes (135 LOC trait)

**Storage adapters:**

- `SqliteVectorStore` (`crates/oneshim-storage/src/sqlite/vector_store_impl/`) -- directory module with `mod.rs`, `trait_impl.rs`, `helpers.rs`, `tests.rs`
- `SqliteVectorIndex` (`crates/oneshim-storage/src/sqlite/vector_index_impl/`) -- directory module with `mod.rs`, `build.rs`, `search.rs`, `metadata.rs`, `tests.rs`

**IVF index infrastructure (V16 migration):**

- `ivf_centroids` table -- INT8 centroid BLOBs with scale/offset
- `ivf_assignments` table -- vector_id -> cluster_id mapping
- `vector_binary_codes` table -- 2-bit Hamming codes per vector
- `vector_index_meta` table -- build timestamps, vector counts

**Key observations from code audit:**

1. IVF builds load ALL non-stale INT8 vectors into memory, run k-means++, then persist. This is O(n) memory.
2. Search loads rows from probed clusters, then brute-forces within. Effective for 10K-100K, degrades at scale.
3. The centroid cache uses `tokio::sync::RwLock` and is invalidated after rebuild.
4. Binary codes add a Hamming pre-filter but are built from full dequantized f32 vectors (memory spike).

### 1.2 Proposed Change

**Add HNSW as a fourth strategy that COMPLEMENTS IVF rather than replaces it.**

The `usearch` crate provides a C++-backed HNSW implementation with sub-millisecond recall at desktop scale. It should be integrated as an alternative in-memory index that lives alongside the existing SQLite-backed IVF pipeline.

**Strategy selection update:**

| Strategy | Vector Count Range | Index Type |
|---|---|---|
| `BruteForceInt8` | < 5,000 | None (scan all) |
| `HnswFloat32` | 5,000 -- 50,000 | In-memory HNSW graph |
| `IvfInt8` | 50,000 -- 100,000 | SQLite-persisted IVF |
| `IvfBinaryRerank` | >= 100,000 | IVF + Hamming + re-rank |

**Why HNSW complements IVF rather than replacing it:**

- HNSW excels at low-to-mid vector counts (5K-50K) where IVF cluster selection adds overhead without proportional benefit.
- IVF remains better at very high counts (>100K) because the graph construction cost of HNSW becomes significant and the memory overhead of the full graph is ~40 bytes/vector vs. ~4 bytes/vector for IVF assignments.
- On a desktop with 16GB RAM, HNSW with 50K 384-dim f32 vectors uses ~75MB (vectors) + ~30MB (graph) = ~105MB, acceptable. At 200K vectors the graph alone would exceed 240MB, which is too aggressive for a background agent.

**Desktop memory crossover analysis (384-dim embeddings):**

| Vector Count | HNSW Memory | IVF Memory | Winner |
|---|---|---|---|
| 5,000 | ~11MB | ~8MB | IVF (marginal) |
| 10,000 | ~21MB | ~9MB | HNSW (search quality) |
| 25,000 | ~53MB | ~12MB | HNSW (latency) |
| 50,000 | ~105MB | ~16MB | HNSW (latency, but memory ceiling) |
| 100,000 | ~210MB | ~20MB | IVF (memory) |

**Recommendation:** HNSW becomes worthwhile at ~5,000 vectors where brute-force starts to take >5ms, and should yield to IVF at ~50,000 where its memory footprint exceeds ~100MB.

### 1.3 Architecture Impact

**New port trait: NO.** HNSW is an implementation detail of `AdaptiveSearchCoordinator`, not a new port. The coordinator already holds `Arc<dyn VectorStore>` + `Arc<dyn VectorIndex>`. HNSW will be an internal field.

**Modified files:**

| File | Change |
|---|---|
| `crates/oneshim-analysis/src/adaptive_search.rs` | Add `HnswFloat32` variant to `SearchStrategy`, add `HnswIndex` wrapper field, populate from VectorStore on refresh |
| `crates/oneshim-analysis/Cargo.toml` | Add `usearch = { version = "2", optional = true }` |
| `crates/oneshim-core/src/config/sections.rs` | Add `hnsw_enabled: bool` + `hnsw_max_vectors: usize` to `SearchConfig` |

**New files:**

| File | Purpose |
|---|---|
| `crates/oneshim-analysis/src/hnsw_index.rs` | `HnswIndex` wrapper -- build from VectorStore rows, search, serialize/deserialize graph |

**No new crates.** No new port traits. No schema migration. The HNSW graph lives in memory only and is rebuilt on startup from the SQLite `embedding_vectors` table.

### 1.4 Migration/Compatibility

- Feature-gated behind `hnsw` cargo feature. Default: off.
- When disabled, `AdaptiveSearchCoordinator` uses the existing 3-strategy ladder (no behavior change).
- When enabled, the coordinator lazily builds the HNSW graph on first `refresh_count()` call if vector count is in the HNSW range.
- Existing IVF tables (`ivf_centroids`, `ivf_assignments`, `vector_binary_codes`) remain untouched.
- `SearchConfig.forced_strategy` gains a new valid value: `"hnsw"`.

### 1.5 Effort Estimate

| Task | Estimate |
|---|---|
| `HnswIndex` wrapper + integration | 2 days |
| `AdaptiveSearchCoordinator` strategy update | 1 day |
| Config section updates + feature flag | 0.5 day |
| Tests (unit + integration) | 1 day |
| Benchmarks (brute-force vs HNSW vs IVF) | 0.5 day |
| **Total** | **5 days** |

### 1.6 Phased Rollout

| Phase | Scope |
|---|---|
| **A** | `HnswIndex` wrapper with build/search/serialize. Unit tests. Feature-gated. |
| **B** | Integration into `AdaptiveSearchCoordinator`. Strategy selection logic. |
| **C** | Benchmarks. Threshold tuning. Documentation. |
| **D** | Optional: persist serialized HNSW graph to disk for faster restart. |

---

## 2. Tauri IPC Optimization (P1)

### 2.1 Current State

The Tauri IPC layer is defined in `src-tauri/src/commands/` as a directory module with 6 sub-modules:

| Module | Commands | Payload Characteristics |
|---|---|---|
| `dashboard.rs` | `get_dashboard_day`, `get_daily_digest`, `get_weekly_digest`, `semantic_search`, `create_override`, `delete_override`, `list_overrides`, `trigger_recluster` | Daily digest JSON (timeline + statistics), override lists |
| `coaching.rs` | `dismiss_coaching_message`, `submit_coaching_feedback`, `set_overlay_mode`, `get_overlay_state`, `toggle_overlay_interactive`, `get_coaching_history`, `get_goal_progress`, `update_regime_goals` | Coaching event rows (paginated), goal progress arrays |
| `settings.rs` | `get_settings`, `update_setting`, `get_allowed_setting_keys`, `get_web_port` | Full AppConfig JSON (redacted), setting patches |
| `analysis.rs` | `get_analysis_config`, `update_analysis_config`, `get_analysis_status` | AnalysisConfig structs |
| `system.rs` | `get_metrics`, `get_update_status`, `approve_update`, `defer_update`, `get_automation_status`, `get_secret_backend_capabilities`, `get_feature_capabilities`, `probe_provider_surface_endpoint` | MetricsResponse, FeatureCapabilitySnapshot |
| `integration.rs` | `integration_auth_status`, `integration_start_device_authorization`, `integration_poll_device_authorization`, `integration_cancel_device_authorization`, `integration_reset_auth_state`, `oauth_*` (5 commands) | OAuth flow handles, connection status |

**Largest payload analysis:**

1. **`get_dashboard_day`** -- Generates a full `DailyDigest` from segments on-demand. Contains `timeline_json` (array of segment timetable entries, potentially 50+ segments/day with app breakdowns), `statistics_json` (aggregate stats), `insight_json`. Estimated 10-50 KB per response.

2. **`get_settings`** -- Returns the full `AppConfig` struct serialized to JSON. With 20+ config sections, this is roughly 5-15 KB. Sent on every settings page open.

3. **`get_coaching_history`** -- Returns `Vec<CoachingEventRow>` with default limit=50. Each row includes personalized_message text. Estimated 5-20 KB.

4. **`get_weekly_digest`** -- Contains `stats_json` with per-day breakdown, `comparison_json`, and optional `llm_narrative`. Estimated 5-30 KB.

5. **Frame thumbnails are NOT sent through IPC.** They are served via the Axum REST API (`crates/oneshim-web/src/handlers/frames.rs`). The `capture_thumbnail()` method in `processor.rs` returns raw WebP bytes, served at `http://localhost:10090/api/frames/:id/thumbnail`.

**Current serialization path:** All IPC commands use Tauri's default JSON serialization (`serde_json`). Every response goes through `serde_json::to_value()` or Tauri's auto-serialize. There is no binary transport, no compression, no streaming.

**Coaching overlay updates** use Tauri's event system (not IPC commands). The overlay state is small (mode + visibility boolean).

### 2.2 Proposed Change

Three optimization tiers:

**Tier 1: Incremental Updates (largest win)**

Replace full `get_dashboard_day` responses with delta-based updates. The dashboard currently fetches the entire day's data on each call. Instead:

- Cache the last response hash in AppState
- Add a `get_dashboard_day_if_changed(last_hash: String)` command that returns either `null` (unchanged) or the full payload
- For settings, add `get_settings_version()` returning a monotonic counter, and only fetch full settings when the version changes

**Tier 2: Pagination Enforcement**

- `get_coaching_history` already supports `limit`/`offset` -- enforce smaller default (20 instead of 50)
- Add pagination to `list_overrides` (currently returns all overrides in a 7-day window)

**Tier 3: Binary Transport for Large Payloads (optional)**

- For `get_weekly_digest`, consider using Tauri's `invoke` with `ArrayBuffer` return type via `tauri::ipc::Response::body(bytes)` to avoid JSON serialization overhead
- This requires Tauri v2's raw response API and TypeScript-side `ArrayBuffer` handling

### 2.3 Architecture Impact

**No new crates. No new ports.**

**Modified files:**

| File | Change |
|---|---|
| `src-tauri/src/commands/dashboard.rs` | Add `get_dashboard_day_if_changed` with hash comparison |
| `src-tauri/src/commands/settings.rs` | Add `get_settings_version` command |
| `src-tauri/src/runtime_state.rs` | Add `dashboard_hash: AtomicU64` and `settings_version: AtomicU64` to `AppState` |
| Frontend `src/lib/api.ts` (or equivalent) | Conditional fetch logic based on version/hash |

### 2.4 Migration/Compatibility

- All existing IPC commands remain unchanged (backward compatible).
- New `*_if_changed` commands are additive.
- Frontend can adopt incrementally -- old pages continue using full-fetch commands.

### 2.5 Effort Estimate

| Task | Estimate |
|---|---|
| Tier 1: Delta-based dashboard + settings version | 1.5 days |
| Tier 2: Pagination enforcement | 0.5 day |
| Tier 3: Binary transport (optional) | 1.5 days |
| Frontend integration | 1 day |
| **Total (Tier 1+2)** | **3 days** |
| **Total (all tiers)** | **4.5 days** |

### 2.6 Phased Rollout

| Phase | Scope |
|---|---|
| **A** | Tier 1 -- hash-based conditional fetch for dashboard and settings |
| **B** | Tier 2 -- pagination defaults, override list limits |
| **C** | Tier 3 -- binary transport for weekly digest (if profiling justifies) |
| **D** | Measure IPC latency before/after with Tauri DevTools |

---

## 3. Audio Capture + Whisper STT (P2)

### 3.1 Current State

The `oneshim-monitor` crate (`crates/oneshim-monitor/src/lib.rs`) provides system monitoring through 11 modules:

- `activity.rs` -- `ActivityTracker` (idle detection)
- `clipboard.rs` -- `ClipboardMonitor` (clipboard change tracking)
- `file_access.rs` -- `FileAccessMonitor` (file system events)
- `input_activity.rs` -- `InputActivityCollector` (mouse/keyboard counters)
- `input_detail.rs` -- detailed input analysis
- `key_hook/` -- global key hook (macOS/Windows/Linux)
- `process.rs` -- `ProcessTracker` (active process/window)
- `system.rs` -- `SysInfoMonitor` (CPU/memory/disk)
- `window_layout.rs` -- `WindowLayoutTracker` (window position changes)
- Platform-specific: `macos.rs`, `windows.rs`, `linux.rs`

The event model (`crates/oneshim-core/src/models/event.rs`) defines 8 event types:

```
Event::User | Event::System | Event::Context | Event::Input
Event::Process | Event::Window | Event::Clipboard | Event::FileAccess
```

There is no audio event type. No audio-related port traits exist.

**Port traits relevant to capture pipelines:**

- `SystemMonitor` -- CPU/memory/disk snapshots
- `ProcessMonitor` -- active window/process info
- `ActivityMonitor` -- idle/active state
- `CaptureTrigger` -- visual capture importance scoring
- `FrameProcessor` -- screen capture pipeline

### 3.2 Proposed Change

**Create a new `oneshim-audio` crate** rather than extending `oneshim-monitor`. Rationale:

1. Audio capture has heavy native dependencies (`cpal` for cross-platform audio, `whisper-rs` for STT) that would bloat the monitor crate's compile time.
2. The audio pipeline has a fundamentally different data flow: continuous stream -> VAD -> chunk -> transcribe -> emit event. This is unlike the polling model of system/process monitors.
3. Feature-gating audio at the crate level is cleaner than conditional compilation within monitor.

**Architecture placement:**

```
oneshim-core  <--  oneshim-audio (new)
                     |
                     +-- capture.rs   (cpal audio stream, ring buffer)
                     +-- vad.rs       (Voice Activity Detection, energy + zero-crossing)
                     +-- transcribe.rs (whisper-rs STT, beam search)
                     +-- privacy.rs   (speaker diarization opt-out, PII masking)
```

**New port trait in oneshim-core:**

```rust
#[async_trait]
pub trait AudioCapture: Send + Sync {
    /// Start capturing audio from the default input device.
    /// Returns a receiver for transcription events.
    async fn start(&self) -> Result<(), CoreError>;

    /// Stop capturing.
    async fn stop(&self) -> Result<(), CoreError>;

    /// Check if audio capture is currently active.
    fn is_active(&self) -> bool;
}
```

**New event variant:**

```rust
pub enum Event {
    // ... existing variants ...
    Audio(AudioTranscriptEvent),
}

pub struct AudioTranscriptEvent {
    pub timestamp: DateTime<Utc>,
    pub duration_secs: f32,
    pub transcript: String,
    pub confidence: f32,
    pub language: String,
    pub is_meeting: bool,
}
```

**Integration with analysis pipeline:**

Transcribed text feeds into the analysis pipeline via two paths:

1. **ContentTracker** (`crates/oneshim-analysis/src/content_tracker.rs`) -- audio transcripts are treated as content activities alongside OCR text and window titles. The `ContentActivity` model already has a `content_type` field; add `AudioTranscript` variant.

2. **ContextAssembler** (`crates/oneshim-analysis/src/context_assembler.rs` or equivalent) -- audio context enriches the segment summary. When a segment contains audio transcripts, the LLM summary prompt includes them as supplementary context.

3. **FTS indexing** -- transcribed text is indexed in the `search_fts` table via `sync_segment_enriched`, which already gathers window titles, GUI element text, and suggestion content. Audio transcripts would be a fourth source.

### 3.3 Architecture Impact

**New crate:** `crates/oneshim-audio/`

**New port trait:** `AudioCapture` in `crates/oneshim-core/src/ports/audio.rs`

**New event variant:** `Event::Audio(AudioTranscriptEvent)` in `crates/oneshim-core/src/models/event.rs`

**Modified files:**

| File | Change |
|---|---|
| `Cargo.toml` (workspace) | Add `oneshim-audio` member |
| `crates/oneshim-core/src/ports/mod.rs` | Add `pub mod audio;` |
| `crates/oneshim-core/src/models/event.rs` | Add `Audio(AudioTranscriptEvent)` variant |
| `crates/oneshim-core/src/config/sections.rs` | Add `AudioConfig` section |
| `crates/oneshim-storage/src/migration.rs` | V18: `audio_transcripts` table |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | Add `collect_audio_transcripts()` to enriched FTS sync |
| `src-tauri/src/main.rs` | Conditional DI wiring for audio adapter |
| `src-tauri/src/scheduler/loops.rs` | Add audio processing loop (if audio enabled) |

**New dependencies (oneshim-audio):**

| Crate | Purpose | Size Impact |
|---|---|---|
| `cpal` | Cross-platform audio capture | ~500KB |
| `whisper-rs` | Whisper.cpp bindings for STT | ~5MB (with model download) |
| `rubato` | Sample rate conversion | ~100KB |

### 3.4 Migration/Compatibility

- Feature-gated behind `audio` cargo feature. Default: off. The `oneshim-audio` crate is only compiled and linked when the feature is enabled.
- Whisper model download is handled at first-run via the existing `updater/` infrastructure. The `tiny.en` model (75MB) is sufficient for meeting transcription. Larger models (`base.en` at 142MB) can be selected via config.
- Privacy: audio capture is disabled by default and requires explicit user opt-in via config. GDPR consent check integrates with the existing `ConsentManager`.
- Schema migration V18 is additive (new table). No existing data affected.
- The `Event::Audio` variant uses `#[serde(tag = "type")]` like all existing variants; old clients that don't understand this tag will skip it via `serde(other)`.

### 3.5 Effort Estimate

| Task | Estimate |
|---|---|
| `oneshim-audio` crate skeleton + port trait | 1 day |
| Audio capture with `cpal` + ring buffer | 2 days |
| VAD (Voice Activity Detection) | 1 day |
| Whisper STT integration | 2 days |
| Privacy controls (PII masking, consent) | 1 day |
| Analysis pipeline integration (ContentTracker, FTS) | 1 day |
| Schema migration V18 + storage adapter | 1 day |
| DI wiring + scheduler loop | 0.5 day |
| Tests (unit + integration) | 1.5 days |
| **Total** | **11 days** |

### 3.6 Phased Rollout

| Phase | Scope |
|---|---|
| **A** | Port trait + event model + config section. Crate skeleton. Feature flag. |
| **B** | `cpal` audio capture + ring buffer + VAD. Manual testing on macOS/Windows. |
| **C** | Whisper STT integration with `tiny.en` model. Transcription accuracy validation. |
| **D** | Analysis pipeline integration (ContentTracker + FTS enrichment). |
| **E** | Privacy controls, consent integration, schema migration. |
| **F** | Meeting detection heuristics (multi-speaker, duration thresholds). |

---

## 4. SQLite Performance Tuning (P2)

### 4.1 Current State

**PRAGMA configuration** (from `crates/oneshim-storage/src/sqlite/mod.rs` line 63-72):

```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA cache_size=8000;       -- 8000 pages = ~32MB at default 4KB page size
PRAGMA temp_store=MEMORY;
PRAGMA mmap_size=268435456;   -- 256MB memory-mapped I/O
PRAGMA page_size=4096;        -- 4KB pages
```

**Assessment of each PRAGMA:**

| PRAGMA | Current Value | Assessment |
|---|---|---|
| `journal_mode=WAL` | Correct | Optimal for concurrent read/write workload |
| `synchronous=NORMAL` | Correct | Good balance of safety/performance for WAL mode |
| `cache_size=8000` | Adequate | 32MB cache. Could increase to 16000 (64MB) on desktops with >8GB RAM |
| `temp_store=MEMORY` | Correct | Avoids temp file I/O for sorts and joins |
| `mmap_size=268435456` | Set (256MB) | Already configured -- good for large DBs |
| `page_size=4096` | OK | 4KB is default. Note: this PRAGMA only takes effect on new databases; existing DBs ignore it |

**Missing PRAGMAs:**

| PRAGMA | Recommended Value | Reason |
|---|---|---|
| `PRAGMA wal_autocheckpoint` | `1000` (default) | Currently using default. Could tune based on write frequency |
| `PRAGMA busy_timeout` | `5000` | Not set. Without this, concurrent writes get SQLITE_BUSY immediately |
| `PRAGMA optimize` | Run periodically | Not currently called. SQLite's built-in query planner optimization |

**VACUUM status:** There is no scheduled VACUUM in the codebase. The `maintenance.rs` file (`crates/oneshim-storage/src/sqlite/maintenance.rs`) contains backup/export operations but no VACUUM logic. Manual WAL checkpoints are performed in the IVF index build (`build.rs` line 176: `PRAGMA wal_checkpoint(TRUNCATE)`).

**FTS5 indexing strategy** (from `crates/oneshim-storage/src/sqlite/fts_search_impl.rs`):

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
    segment_id UNINDEXED,
    content_type,
    searchable_text,
    tokenize='porter unicode61'
);
```

- Uses Porter stemming + Unicode tokenization -- good for English
- `segment_id` is `UNINDEXED` (metadata column, not searchable) -- correct
- Enriched indexing gathers from 4 sources: base text, window titles, GUI element text, suggestion content
- FTS sync uses DELETE-then-INSERT (not INSERT OR REPLACE) due to FTS5's append-only architecture
- Table existence is checked before every query (`SELECT COUNT(*) > 0 FROM sqlite_master`) -- unnecessary overhead after first check

**Schema status:** V1-V17 (17 migrations). 25+ tables. Notable indexes:

- V7 added composite indexes for common query patterns (events sent+timestamp, work_sessions state+started)
- V7 added partial indexes (`WHERE resumed_at IS NULL`)
- Embedding vectors have indexes on segment_id, timestamp, model_id, is_stale
- IVF assignments indexed on cluster_id

### 4.2 Proposed Change

**A. Add missing PRAGMAs:**

```sql
PRAGMA busy_timeout=5000;     -- Wait up to 5s on lock contention
PRAGMA optimize;              -- Run at startup after migrations
```

`busy_timeout` is critical for preventing SQLITE_BUSY errors when the scheduler's write loops and the web dashboard's read queries contend. Currently, the single-connection Mutex serializes access, but if a read-only connection is added in the future (as noted in the code comments), `busy_timeout` becomes essential.

**B. Scheduled incremental VACUUM:**

Add `PRAGMA incremental_vacuum(100);` to the scheduler's aggregate loop (runs every 60 seconds). This reclaims up to 100 free pages per cycle without the full-table lock of `VACUUM`.

Also add a monthly full `VACUUM` triggered by checking `PRAGMA freelist_count` -- if free pages exceed 20% of total pages, run a full vacuum during idle time.

**C. Optimize FTS5 operations:**

1. **Cache table existence check.** The `search_fts` and `gui_interactions` tables are checked for existence on every FTS operation. Instead, check once at startup and store the result in `SqliteStorage`:

```rust
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
    retention_days: u32,
    fts_available: bool,        // Checked once at open()
    gui_table_available: bool,  // Checked once at open()
}
```

2. **Use FTS5 `optimize` command** periodically to merge b-tree segments:

```sql
INSERT INTO search_fts(search_fts) VALUES('optimize');
```

This should run during the daily aggregate cycle, not on every sync.

**D. Dynamic cache_size based on available RAM:**

```rust
let available_mb = sysinfo::System::new_all().available_memory() / 1_048_576;
let cache_pages = if available_mb > 8192 {
    16000  // 64MB cache on machines with 8GB+ free
} else if available_mb > 4096 {
    12000  // 48MB cache
} else {
    8000   // 32MB cache (current default)
};
```

**E. `ANALYZE` after bulk operations:**

Run `ANALYZE` after IVF index builds and after bulk retention enforcement to keep the query planner's statistics current.

### 4.3 Architecture Impact

**No new crates. No new ports. No schema migration.**

**Modified files:**

| File | Change |
|---|---|
| `crates/oneshim-storage/src/sqlite/mod.rs` | Add `busy_timeout`, `PRAGMA optimize`, FTS availability cache |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | Remove per-query existence checks, use cached flag |
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | Add `incremental_vacuum()` and `full_vacuum_if_needed()` methods |
| `src-tauri/src/scheduler/loops.rs` (or `crates/oneshim-app/src/scheduler/loops.rs`) | Call incremental vacuum in aggregate loop, monthly full vacuum check |
| `crates/oneshim-storage/src/sqlite/vector_index_impl/build.rs` | Add `ANALYZE` after IVF build |

### 4.4 Migration/Compatibility

- All PRAGMA changes are applied at connection open time. No schema migration needed.
- `busy_timeout` only matters if a second connection is opened in the future (currently single-connection design).
- Incremental vacuum is a no-op if there are no free pages.
- FTS5 `optimize` is safe to call concurrently with reads.
- Dynamic `cache_size` is strictly better than static -- no backward compatibility concern.

### 4.5 Effort Estimate

| Task | Estimate |
|---|---|
| PRAGMA additions (busy_timeout, optimize) | 0.25 day |
| FTS existence caching | 0.5 day |
| Incremental + conditional full VACUUM | 1 day |
| Dynamic cache_size | 0.25 day |
| ANALYZE integration | 0.25 day |
| FTS5 periodic optimize | 0.25 day |
| Tests | 0.5 day |
| **Total** | **3 days** |

### 4.6 Phased Rollout

| Phase | Scope |
|---|---|
| **A** | PRAGMA additions + FTS existence caching (quick wins) |
| **B** | Incremental vacuum in scheduler + conditional full vacuum |
| **C** | Dynamic cache_size + ANALYZE after bulk ops |
| **D** | FTS5 optimize scheduling + performance measurement |

---

## Summary

| Improvement | Priority | Effort | New Crate | New Port | Schema Change |
|---|---|---|---|---|---|
| USearch HNSW Vector Index | P1 | 5 days | No | No | No |
| Tauri IPC Optimization | P1 | 3-4.5 days | No | No | No |
| Audio Capture + Whisper STT | P2 | 11 days | Yes (`oneshim-audio`) | Yes (`AudioCapture`) | Yes (V18) |
| SQLite Performance Tuning | P2 | 3 days | No | No | No |
| **Total** | | **22-23.5 days** | | | |

### Dependency Graph

```
SQLite Perf Tuning (independent, can start immediately)
    |
    v
USearch HNSW (depends on stable vector store, benefits from tuned SQLite)
    |
    v
Tauri IPC (independent, can parallelize with HNSW)

Audio Capture (fully independent, long lead time)
```

### Recommended Execution Order

1. **SQLite Performance Tuning** -- Quick wins, low risk, improves foundation
2. **Tauri IPC Optimization (Tier 1+2)** -- Immediate UX improvement
3. **USearch HNSW Vector Index** -- Search quality improvement on existing data
4. **Audio Capture + Whisper STT** -- New capability, longest implementation
