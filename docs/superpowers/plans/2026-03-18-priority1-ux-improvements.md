# Priority 1: UX Improvements — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver timetable-based regime visualization dashboard, hybrid vector+FTS5 search, and daily digests with LLM feedback — making the analysis pipeline's value tangible to users.

**Architecture:** Backend: new models in `oneshim-core`, `TextSearchProvider` port, `DailyDigestGenerator`/`HybridSearchService` in `oneshim-analysis`, FTS5 adapter in `oneshim-storage`, REST handlers in `oneshim-web`. Frontend: React components (TimelineView, InsightCard, StatisticsPanel, SearchBar) in existing `oneshim-web/frontend/`.

**Tech Stack:** Rust (Axum, rusqlite FTS5), React 18, Recharts, Tailwind CSS, TypeScript

**Spec:** `docs/superpowers/specs/2026-03-18-priority1-ux-improvements-design.md`

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/models/daily_digest.rs` | DailyDigest, DailyInsight, TimelineEntry, DailyStatistics models |
| `crates/oneshim-core/src/ports/text_search.rs` | TextSearchProvider port trait (FTS5 boundary) |
| `crates/oneshim-analysis/src/daily_digest_generator.rs` | DailyDigestGenerator (pure aggregation) |
| `crates/oneshim-analysis/src/daily_insight_generator.rs` | DailyInsightGenerator (LLM narrative + highlights) |
| `crates/oneshim-analysis/src/hybrid_search_service.rs` | HybridSearchService (RRF fusion) |
| `crates/oneshim-storage/src/sqlite/fts_search_impl.rs` | TextSearchProvider SQLite/FTS5 implementation |
| `crates/oneshim-web/src/handlers/dashboard.rs` | Dashboard API handler |
| `crates/oneshim-web/src/handlers/daily_digest.rs` | Daily digest API handler |
| `crates/oneshim-web/frontend/src/pages/DashboardDay.tsx` | Timetable dashboard page |
| `crates/oneshim-web/frontend/src/components/InsightCard.tsx` | LLM insight card component |
| `crates/oneshim-web/frontend/src/components/TimelineView.tsx` | Timetable timeline component |
| `crates/oneshim-web/frontend/src/components/StatisticsPanel.tsx` | Statistics panel component |

### Modified files

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `"fts5"` to rusqlite features |
| `crates/oneshim-core/src/models/mod.rs` | Add `daily_digest` module |
| `crates/oneshim-core/src/ports/mod.rs` | Add `text_search` module |
| `crates/oneshim-core/src/ports/web_storage.rs` | Add daily digest CRUD methods |
| `crates/oneshim-storage/src/migration.rs` | V11: `search_fts` + `daily_digests` tables |
| `crates/oneshim-storage/src/sqlite/mod.rs` | Add `fts_search_impl` module |
| `crates/oneshim-storage/src/sqlite/web_storage_impl.rs` | Implement daily digest methods |
| `crates/oneshim-analysis/src/lib.rs` | Export new modules |
| `crates/oneshim-web/src/handlers/mod.rs` | Add `dashboard`, `daily_digest` modules |
| `crates/oneshim-web/src/handlers/semantic_search.rs` | Add `mode` parameter for hybrid search |
| `crates/oneshim-web/src/routes.rs` | Register dashboard + digest routes |
| `src-tauri/src/commands.rs` | Add dashboard + digest Tauri commands |
| `src-tauri/src/main.rs` | Register commands |
| `src-tauri/src/scheduler/loops.rs` | Daily digest auto-generation in aggregation loop |

---

## Phase A: Backend Foundation (Models + Ports + Storage)

### Task 1: DailyDigest domain models

**Files:**
- Create: `crates/oneshim-core/src/models/daily_digest.rs`
- Modify: `crates/oneshim-core/src/models/mod.rs`

- [ ] Create all daily digest models: `DailyDigest`, `DailyInsight`, `DigestHighlight`, `HighlightType`, `TimelineEntry`, `ContentBrief`, `DailyStatistics`, `DayComparison`. All derive `Debug, Clone, Serialize, Deserialize`.
- [ ] Add `pub mod daily_digest;` to models/mod.rs
- [ ] `cargo check -p oneshim-core`
- [ ] Commit: `feat(core): add DailyDigest domain models`

### Task 2: TextSearchProvider port trait

**Files:**
- Create: `crates/oneshim-core/src/ports/text_search.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] Create `TextSearchProvider` async trait: `search_fts(query, limit) -> Vec<TextSearchResult>`, `sync_segment(segment_id, text)`. Define `TextSearchResult` struct.
- [ ] Add to ports/mod.rs
- [ ] `cargo check -p oneshim-core`
- [ ] Commit: `feat(core): add TextSearchProvider port trait for FTS5`

### Task 3: WebStorage daily digest methods

**Files:**
- Modify: `crates/oneshim-core/src/ports/web_storage.rs`

- [ ] Add 4 methods to WebStorage trait: `save_daily_digest`, `get_daily_digest`, `list_daily_digests`, `get_segments_for_date`
- [ ] Add default impls returning empty/error for backward compatibility
- [ ] `cargo check -p oneshim-core`
- [ ] Commit: `feat(core): add daily digest methods to WebStorage trait`

### Task 4: V11 migration — FTS5 + daily_digests

**Files:**
- Modify: `Cargo.toml` (workspace) — add `"fts5"` to rusqlite features
- Modify: `crates/oneshim-storage/src/migration.rs`

- [ ] Add `"fts5"` to rusqlite features in workspace Cargo.toml
- [ ] Update CURRENT_VERSION to 11
- [ ] Add `migrate_v11`: create `search_fts` FTS5 virtual table + `daily_digests` table
- [ ] Backfill existing segments into FTS5 index
- [ ] `cargo test -p oneshim-storage`
- [ ] Commit: `feat(storage): V11 migration — FTS5 search index + daily digests table`

### Task 5: FTS5 search SQLite implementation

**Files:**
- Create: `crates/oneshim-storage/src/sqlite/fts_search_impl.rs`
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs`

- [ ] Implement `TextSearchProvider for SqliteStorage`: FTS5 MATCH query with BM25 ranking, `sync_segment` INSERT into search_fts
- [ ] Tests: search returns ranked results, sync + search roundtrip, empty query
- [ ] `cargo test -p oneshim-storage`
- [ ] Commit: `feat(storage): implement FTS5 TextSearchProvider for SQLite`

### Task 6: Daily digest WebStorage implementation

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/web_storage_impl.rs`

- [ ] Implement `save_daily_digest`, `get_daily_digest`, `list_daily_digests`, `get_segments_for_date`
- [ ] `get_segments_for_date` queries `activity_segments` WHERE date range, parses JSON columns
- [ ] Tests: save + get roundtrip, list ordering
- [ ] `cargo test -p oneshim-storage`
- [ ] Commit: `feat(storage): implement daily digest WebStorage methods`

---

## Phase B: Analysis Logic (Digest + Search)

### Task 7: DailyDigestGenerator — pure aggregation

**Files:**
- Create: `crates/oneshim-analysis/src/daily_digest_generator.rs`

- [ ] Pure algorithm: `generate(segments, date, prev_digest) -> DailyDigest` — computes timeline, statistics, comparison. No LLM, no I/O.
- [ ] Logic: build TimelineEntry per segment, compute stats (deep_work_hours, comm_hours, context_switches, regime_distribution, longest focus), compare with prev_digest
- [ ] Tests: correct aggregation from test segments, empty day, comparison delta
- [ ] `cargo test -p oneshim-analysis`
- [ ] Commit: `feat(analysis): add DailyDigestGenerator for segment aggregation`

### Task 8: DailyInsightGenerator — LLM narrative + highlights

**Files:**
- Create: `crates/oneshim-analysis/src/daily_insight_generator.rs`

- [ ] Uses `Arc<dyn AnalysisProvider>` to generate `DailyInsight` from segment data
- [ ] Builds prompt with day's segments, calls `summarize_text()`, parses JSON response
- [ ] JSON extraction: strip markdown fences, find `{` to `}`, parse as `DailyInsight`
- [ ] Fallback on parse failure: basic stats-only narrative
- [ ] Fallback on LLM unavailable: return None
- [ ] Tests with mock provider: successful parse, markdown-wrapped response, malformed response fallback
- [ ] `cargo test -p oneshim-analysis`
- [ ] Commit: `feat(analysis): add DailyInsightGenerator with LLM narrative and fallback`

### Task 9: HybridSearchService — RRF fusion

**Files:**
- Create: `crates/oneshim-analysis/src/hybrid_search_service.rs`

- [ ] Accepts `Arc<dyn TextSearchProvider>` + `Arc<dyn VectorStore>` + `Arc<dyn EmbeddingProvider>`
- [ ] `search(query, mode, limit) -> Vec<SearchResult>` where mode = Hybrid/Semantic/Keyword
- [ ] RRF: `score = α/(k+rank_vector) + β/(k+rank_fts5)`, k=60, α=0.6, β=0.4
- [ ] Missing-rank penalty: `limit + 1`
- [ ] Deduplication by segment_id (keep highest score)
- [ ] Tests: hybrid merges both sources, semantic-only ignores FTS5, keyword-only ignores vector, dedup works
- [ ] `cargo test -p oneshim-analysis`
- [ ] Commit: `feat(analysis): add HybridSearchService with Reciprocal Rank Fusion`

### Task 10: Export new modules

**Files:**
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] Export: `daily_digest_generator`, `daily_insight_generator`, `hybrid_search_service`
- [ ] `cargo check -p oneshim-analysis`
- [ ] Commit: `feat(analysis): export Priority 1 modules`

---

## Phase C: REST API + Scheduler

### Task 11: Dashboard REST handler

**Files:**
- Create: `crates/oneshim-web/src/handlers/dashboard.rs`
- Modify: `crates/oneshim-web/src/handlers/mod.rs`
- Modify: `crates/oneshim-web/src/routes.rs`

- [ ] `GET /api/dashboard/day?date=YYYY-MM-DD` — assembles timetable + insight + stats
- [ ] Logic: check daily_digests cache → if miss, generate from segments, optionally LLM, cache
- [ ] Response: `DashboardDayResponse` with insight, timeline, statistics
- [ ] Register route, add handler module
- [ ] `cargo check -p oneshim-web`
- [ ] Commit: `feat(web): add dashboard day endpoint with timetable + insight`

### Task 12: Daily digest REST handler

**Files:**
- Create: `crates/oneshim-web/src/handlers/daily_digest.rs`
- Modify: `crates/oneshim-web/src/routes.rs`

- [ ] `GET /api/digests/daily?date=YYYY-MM-DD` — returns cached or generates
- [ ] `GET /api/digests/daily/today` — shortcut for current date
- [ ] Register routes
- [ ] `cargo check -p oneshim-web`
- [ ] Commit: `feat(web): add daily digest endpoints`

### Task 13: Hybrid search mode in semantic_search handler

**Files:**
- Modify: `crates/oneshim-web/src/handlers/semantic_search.rs`

- [ ] Add `mode: Option<String>` to `SemanticSearchQuery` (hybrid/semantic/keyword)
- [ ] When `mode=hybrid` or default: use HybridSearchService
- [ ] When `mode=semantic`: existing vector-only path
- [ ] When `mode=keyword`: FTS5-only via TextSearchProvider
- [ ] Update `oneshim-api-contracts/src/search.rs` with mode field
- [ ] `cargo check -p oneshim-web`
- [ ] Commit: `feat(web): add hybrid search mode to semantic search endpoint`

### Task 14: Tauri commands

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] `get_dashboard_day(date)` → DashboardDayResponse
- [ ] `get_daily_digest(date)` → DailyDigest
- [ ] Register in generate_handler
- [ ] `cargo check -p oneshim-app` (needs frontend dist stub)
- [ ] Commit: `feat(tauri): add dashboard and daily digest IPC commands`

### Task 15: Scheduler — daily digest auto-generation

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] In aggregation loop: at midnight (configurable), check if daily digest exists for today, if not → generate + store
- [ ] Similar pattern to weekly digest auto-generation
- [ ] `cargo check -p oneshim-app`
- [ ] Commit: `feat(scheduler): add daily digest auto-generation at midnight`

### Task 16: HTTP interface manifest + OpenAPI update

**Files:**
- Modify: `docs/contracts/http-interface-manifest.v1.json`

- [ ] Add new routes: `/dashboard/day`, `/digests/daily`, `/digests/daily/today`
- [ ] Run `scripts/generate-http-openapi.sh`
- [ ] Run `scripts/verify-http-interface-manifest.sh` — must pass
- [ ] Commit: `docs: update HTTP interface manifest with dashboard and digest routes`

---

## Phase D: React Frontend

### Task 17: InsightCard component

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/InsightCard.tsx`

- [ ] Props: `insight: DailyInsight | null`
- [ ] Renders: narrative text + highlights with icons (🏆⚠️💡)
- [ ] Null state: "No insight available" placeholder
- [ ] Tailwind styling: card with blue-left-border, icon chips for highlights
- [ ] Commit: `feat(frontend): add InsightCard component`

### Task 18: TimelineView component

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/TimelineView.tsx`

- [ ] Props: `timeline: TimelineEntry[]`
- [ ] Renders: time axis (left) + colored blocks (right) proportional to duration
- [ ] Regime colors from mapping table
- [ ] Content activities shown inside each block
- [ ] Inline annotations as speech bubbles
- [ ] Click to expand segment detail
- [ ] Tailwind: flex layout, color-coded backgrounds
- [ ] Commit: `feat(frontend): add TimelineView timetable component`

### Task 19: StatisticsPanel component

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/StatisticsPanel.tsx`

- [ ] Props: `statistics: DailyStatistics`
- [ ] Renders: KPI cards (deep work, comm, switches) with delta arrows
- [ ] Regime distribution bar (stacked horizontal bar)
- [ ] Longest focus block highlight
- [ ] Use Recharts for distribution bar
- [ ] Commit: `feat(frontend): add StatisticsPanel with KPIs and regime distribution`

### Task 20: DashboardDay page + search integration

**Files:**
- Create: `crates/oneshim-web/frontend/src/pages/DashboardDay.tsx`
- Modify: `crates/oneshim-web/frontend/src/App.tsx` (add route)

- [ ] Page layout: InsightCard (top) → TimelineView (middle) → StatisticsPanel (bottom)
- [ ] Date picker for navigation
- [ ] React Query: `useQuery(['dashboard-day', date], fetchDashboardDay)`
- [ ] Add search bar with mode toggle (hybrid/semantic/keyword)
- [ ] Register route in App.tsx
- [ ] i18n keys for all text
- [ ] Commit: `feat(frontend): add DashboardDay page with timetable layout`

### Task 21: Final verification + push

- [ ] `cargo test --workspace`
- [ ] `cargo fmt --check && cargo clippy --workspace`
- [ ] `scripts/verify-http-interface-manifest.sh`
- [ ] Frontend build: `cd crates/oneshim-web/frontend && pnpm build`
- [ ] `git push`

---

## Deferred
- GUI activity intelligence (OCR + mouse correlation)
- Multi-day comparison view
- Custom regime color themes
- Export digest as PDF/image
