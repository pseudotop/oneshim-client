# GUI Activity Intelligence Phase 2 — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aggregate Phase 1 `GuiInteractionEvent`s into structured `GuiActivitySummary` records, integrate them into the existing `ContentActivity` → `SegmentSummary` → `ContextAssembler` pipeline, and refine `WorkType` classification using GUI signals. This upgrades LLM context from "used VSCode for 15 min" to "edited auth.rs: 15 min coding, 3 saves, 2 test runs."

**Architecture:** Pure-algorithm additions to `oneshim-core` (models + config), `oneshim-vision` (detector upgrade), and `oneshim-analysis` (aggregator + refiner). New `gui_pipeline.rs` in `src-tauri/src/scheduler/` follows the `analysis_pipeline::run_analysis_tick()` pattern. No new ports or I/O — data flows through existing scheduler state.

**Tech Stack:** Rust, serde, chrono, oneshim-core models, oneshim-vision PII filter

**Spec:** `docs/superpowers/specs/2026-03-19-gui-activity-intelligence-design.md`

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/models/gui_activity.rs` | `GuiActivitySummary` model + summary line generator |
| `crates/oneshim-analysis/src/gui_aggregator.rs` | `GuiActivityAggregator` — time-window event grouping + semantic action detection |
| `crates/oneshim-analysis/src/gui_work_type_refiner.rs` | `GuiWorkTypeRefiner` — post-hoc WorkType correction using GUI signals |
| `src-tauri/src/scheduler/gui_pipeline.rs` | `run_gui_tick()` — self-contained scheduler entry point |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/models/gui_interaction.rs` | Add Phase 2 variants to `GuiElementType` (`ToolbarIcon`, `TreeItem`, `ScrollBar`, `TextRegion`); add `InteractionType` structured enum; add `window_title` + `screen_position` to `GuiInteractionEvent` |
| `crates/oneshim-core/src/models/tiered_memory/content.rs` | Add `gui_summary: Option<GuiActivitySummary>` field to `ContentActivity` |
| `crates/oneshim-core/src/models/mod.rs` | Add `pub mod gui_activity;` |
| `crates/oneshim-core/src/config/sections/analysis.rs` | Add `gui_intelligence: GuiIntelligenceConfig` sub-section to `AnalysisConfig` |
| `crates/oneshim-vision/src/input_correlator.rs` | Rename to `GuiElementDetector`; add proximity fallback (40px); accept `screen_resolution` param |
| `crates/oneshim-vision/src/lib.rs` | Update module export (`input_correlator` → `gui_detector`) |
| `crates/oneshim-analysis/src/content_tracker.rs` | Accept + propagate `gui_summary` in `update()` and `finalize_current()` |
| `crates/oneshim-analysis/src/assembler.rs` | Enrich `ContentSummaryItem.content` with `gui_summary.summary_line` when present |
| `crates/oneshim-analysis/src/lib.rs` | Add `pub mod gui_aggregator; pub mod gui_work_type_refiner;` + re-exports |
| `src-tauri/src/scheduler/mod.rs` | Add `pub(super) mod gui_pipeline;` + `GuiPipelineState` to scheduler state |
| `src-tauri/src/scheduler/loops.rs` | Call `gui_pipeline::run_gui_tick()` after `run_analysis_tick()` in monitor loop |

---

## Task 1: Expand GuiElementType and add InteractionType (oneshim-core)

**Files:**
- Modify: `crates/oneshim-core/src/models/gui_interaction.rs`

- [ ] **Step 1: Add Phase 2 variants to GuiElementType**

Add after `Unknown`:

```rust
ToolbarIcon,
TreeItem,
ScrollBar,
TextRegion,
```

- [ ] **Step 2: Add InteractionType structured enum**

Below `GuiInteractionType`, add `InteractionType` with structured payloads per spec §9.3:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InteractionType {
    Click { button: ClickType },
    KeyboardShortcut { keys: String },
    TextEntry { char_count: u32, duration_ms: u64 },
    Scroll { direction: ScrollDirection, amount: u32 },
    DragDrop { from: (u32, u32), to: (u32, u32) },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClickType { Single, Double, Right }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScrollDirection { Up, Down, Left, Right }
```

- [ ] **Step 3: Add window_title and screen_position to GuiInteractionEvent**

Add fields (both `Option` to preserve Phase 1 backward compat):

```rust
pub window_title: Option<String>,
pub screen_position: Option<(u32, u32)>,
pub interaction: Option<InteractionType>,  // Phase 2 structured; coexists with interaction_type
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p oneshim-core`

---

## Task 2: GuiActivitySummary model (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/models/gui_activity.rs`
- Modify: `crates/oneshim-core/src/models/mod.rs`

- [ ] **Step 1: Create gui_activity.rs**

Define `GuiActivitySummary` per spec §4.4 output:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuiActivitySummary {
    pub app_name: String,
    pub window_title: String,
    pub content_label: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_secs: u64,
    // Interaction counts
    pub button_clicks: u32,
    pub text_entries: u32,
    pub tab_switches: u32,
    pub menu_accesses: u32,
    pub tree_navigations: u32,
    pub scroll_events: u32,
    // Semantic actions
    pub save_count: u32,
    pub test_run_count: u32,
    pub search_count: u32,
    pub build_count: u32,
    pub undo_redo_count: u32,
    pub copy_paste_count: u32,
    // Top elements
    pub top_elements: Vec<(String, GuiElementType, u32)>,
    // Human-readable
    pub summary_line: String,
}
```

Include a `generate_summary_line()` method implementing the template from spec §4.5.

- [ ] **Step 2: Register module in mod.rs**

Add `pub mod gui_activity;` to `crates/oneshim-core/src/models/mod.rs`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-core`

---

## Task 3: GuiIntelligenceConfig (oneshim-core)

**Files:**
- Modify: `crates/oneshim-core/src/config/sections/analysis.rs`

- [ ] **Step 1: Add GuiIntelligenceConfig struct**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiIntelligenceConfig {
    #[serde(default = "default_gui_enabled")]
    pub enabled: bool,
    #[serde(default = "default_aggregation_window_secs")]
    pub aggregation_window_secs: u64,
    #[serde(default = "default_max_events_per_segment")]
    pub max_events_per_segment: u32,
}
```

Defaults: `enabled: true`, `aggregation_window_secs: 60`, `max_events_per_segment: 1000`.

- [ ] **Step 2: Add field to AnalysisConfig**

Add `#[serde(default)] pub gui_intelligence: GuiIntelligenceConfig` to `AnalysisConfig`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-core`

---

## Task 4: ContentActivity gui_summary field (oneshim-core)

**Files:**
- Modify: `crates/oneshim-core/src/models/tiered_memory/content.rs`

- [ ] **Step 1: Add gui_summary field**

Add to `ContentActivity`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub gui_summary: Option<GuiActivitySummary>,
```

Import `GuiActivitySummary` from `super::super::gui_activity`.

- [ ] **Step 2: Fix all construction sites**

Search for `ContentActivity {` across the workspace and add `gui_summary: None` to each. Key locations:
- `crates/oneshim-analysis/src/content_tracker.rs` (`finalize_current`)
- `crates/oneshim-analysis/src/segment_summarizer.rs` (if any)
- Test files

- [ ] **Step 3: Verify compilation**

Run: `cargo check --workspace`

---

## Task 5: Upgrade InputOcrCorrelator → GuiElementDetector (oneshim-vision)

**Files:**
- Modify: `crates/oneshim-vision/src/input_correlator.rs` (rename to `gui_detector.rs`)
- Modify: `crates/oneshim-vision/src/lib.rs`

- [ ] **Step 1: Rename file**

```bash
mv crates/oneshim-vision/src/input_correlator.rs crates/oneshim-vision/src/gui_detector.rs
```

- [ ] **Step 2: Rename struct and add proximity fallback**

Rename `InputOcrCorrelator` → `GuiElementDetector`. Add `screen_resolution: (u32, u32)` parameter to `correlate_click`. Add 40px proximity fallback per spec §4.1:

```rust
pub fn correlate_click(
    click_x: u32,
    click_y: u32,
    regions: &[OcrRegion],
    screen_resolution: (u32, u32),
) -> Option<GuiElement> {
    // 1. Direct hit (existing logic)
    // 2. If no hit, find nearest within 40px threshold
}
```

Update `infer_element_type` to use `screen_resolution` for proportional thresholds (status bar = y > 95% height) instead of hardcoded pixel values.

- [ ] **Step 3: Apply PII filter to GuiElement.text at creation**

Import `sanitize_title_with_level` from `crate::privacy`. Apply to `GuiElement.text` in both `correlate_click` and `correlate_typing`, using the configured `PiiFilterLevel`.

- [ ] **Step 4: Update lib.rs module export**

Replace `pub mod input_correlator;` with `pub mod gui_detector;`. Add re-export alias for backward compat if needed.

- [ ] **Step 5: Fix all import sites**

Search for `InputOcrCorrelator` across workspace and update to `GuiElementDetector`. Update import paths from `input_correlator` to `gui_detector`.

- [ ] **Step 6: Verify compilation + run existing tests**

Run: `cargo test -p oneshim-vision`

---

## Task 6: GuiActivityAggregator (oneshim-analysis)

**Files:**
- New: `crates/oneshim-analysis/src/gui_aggregator.rs`
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] **Step 1: Create gui_aggregator.rs**

Implement `GuiActivityAggregator` per spec §4.4:

```rust
pub struct GuiActivityAggregator {
    current_window: Option<AggregationWindow>,
    window_duration_secs: u64,
    max_events: u32,
}

struct AggregationWindow {
    app_name: String,
    window_title: String,
    content_label: String,
    start_time: DateTime<Utc>,
    events: Vec<GuiInteractionEvent>,
}
```

Methods:
- `new(config: &GuiIntelligenceConfig)` — from config
- `push(&mut self, event: GuiInteractionEvent, content_label: &str)` — buffer event, flush on content change or window expiry
- `flush(&mut self) -> Option<GuiActivitySummary>` — aggregate buffered events into summary
- `detect_semantic_actions(events: &[GuiInteractionEvent]) -> SemanticActionCounts` — detect Save, TestRun, Search, Build, UndoRedo, CopyPaste from event patterns

Semantic action detection rules from spec §4.4:
- Save = button click with text containing "save" (case-insensitive) OR KeyboardShortcut "Cmd+S"/"Ctrl+S"
- Test run = terminal activation + text matching `cargo test|pytest|npm test`
- Search = KeyboardShortcut "Cmd+F"/"Ctrl+F" or click on element with "search"/"find" text
- Build = text matching "build"|"compile"|"cargo build"
- UndoRedo = KeyboardShortcut "Cmd+Z"/"Ctrl+Z"/"Cmd+Shift+Z"
- CopyPaste = KeyboardShortcut "Cmd+C"/"Cmd+V"/"Ctrl+C"/"Ctrl+V"

- [ ] **Step 2: Implement summary_line via GuiActivitySummary::generate_summary_line()**

Call `GuiActivitySummary::generate_summary_line()` (from Task 2) during `flush()`.

- [ ] **Step 3: Register in lib.rs**

Add `pub mod gui_aggregator;` and re-export `GuiActivityAggregator`.

- [ ] **Step 4: Write unit tests**

Test cases:
- Single event produces correct counts
- Window flush on content label change
- Window flush on time expiry
- Semantic action detection (Save, TestRun, Search)
- Top elements extraction (sorted by frequency, top 5)
- Summary line format verification

Run: `cargo test -p oneshim-analysis -- gui_aggregator`

---

## Task 7: GuiWorkTypeRefiner (oneshim-analysis)

**Files:**
- New: `crates/oneshim-analysis/src/gui_work_type_refiner.rs`
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] **Step 1: Create gui_work_type_refiner.rs**

```rust
pub struct GuiWorkTypeRefiner;

impl GuiWorkTypeRefiner {
    pub fn refine(
        &self,
        initial: WorkType,
        summary: &GuiActivitySummary,
    ) -> WorkType { ... }
}
```

Refinement rules from spec §5.3:
- `Unknown` + heavy TextInput interactions → `FormFilling`
- `Reading` + frequent element clicking → `Navigation`
- `Browsing` + TextInput dominance → `FormFilling`
- `ActiveCoding` + terminal-area typing (check top_elements for terminal-like text) → keep or refine to `ActiveCoding` (confirmed)
- All others: return initial unchanged

- [ ] **Step 2: Register in lib.rs**

Add `pub mod gui_work_type_refiner;` and re-export `GuiWorkTypeRefiner`.

- [ ] **Step 3: Write unit tests**

Test each refinement rule + no-change passthrough.

Run: `cargo test -p oneshim-analysis -- gui_work_type_refiner`

---

## Task 8: ContentTracker gui_summary propagation (oneshim-analysis)

**Files:**
- Modify: `crates/oneshim-analysis/src/content_tracker.rs`

- [ ] **Step 1: Add gui_summary parameter to update()**

Extend signature:

```rust
pub fn update(
    &mut self,
    content_label: &str,
    content_type: ContentType,
    work_type: WorkType,
    engagement: EngagementMetrics,
    confidence: f32,
    timestamp: DateTime<Utc>,
    gui_summary: Option<GuiActivitySummary>,  // NEW
) -> Option<ContentActivity> {
```

Store latest `gui_summary` in `ActiveContent`. Propagate to `ContentActivity` in `finalize_current()`.

- [ ] **Step 2: Update all call sites**

Update `analysis_pipeline::run_analysis_tick()` to pass `None` initially (will be replaced in Task 10). Search for other `content_tracker.update(` calls and add the parameter.

- [ ] **Step 3: Verify compilation**

Run: `cargo check --workspace`

---

## Task 9: ContextAssembler enrichment (oneshim-analysis)

**Files:**
- Modify: `crates/oneshim-analysis/src/assembler.rs`

- [ ] **Step 1: Enrich ContentSummaryItem with gui_summary**

In the method that builds `ContentSummaryItem` from `ContentSummaryEntry`, check if the source `ContentActivity` has a `gui_summary`. When present, use `gui_summary.summary_line` as the `content` field instead of the bare content label:

```rust
// Before: content: entry.content.clone()
// After:
content: if let Some(ref gs) = activity.gui_summary {
    gs.summary_line.clone()
} else {
    entry.content.clone()
}
```

- [ ] **Step 2: Verify existing assembler tests still pass**

Run: `cargo test -p oneshim-analysis -- assembler`

---

## Task 10: gui_pipeline.rs scheduler integration (src-tauri)

**Files:**
- New: `src-tauri/src/scheduler/gui_pipeline.rs`
- Modify: `src-tauri/src/scheduler/mod.rs`
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] **Step 1: Create gui_pipeline.rs**

Follow the `analysis_pipeline.rs` pattern — a `run_gui_tick()` function:

```rust
pub(super) fn run_gui_tick(
    gui_state: &mut GuiPipelineState,
    ocr_regions: &[OcrRegion],
    input_snap: &InputSnapshot,
    app_name: &str,
    window_title: &str,
    content_label: &str,
    screen_resolution: (u32, u32),
) -> Option<GuiActivitySummary> {
    // 1. Correlate clicks with OCR regions via GuiElementDetector
    // 2. Push GuiInteractionEvents into GuiActivityAggregator
    // 3. If aggregator flushes, run GuiWorkTypeRefiner on the summary
    // 4. Return summary (caller feeds into ContentTracker)
}
```

Define `GuiPipelineState`:

```rust
pub(super) struct GuiPipelineState {
    pub aggregator: GuiActivityAggregator,
    pub refiner: GuiWorkTypeRefiner,
}
```

- [ ] **Step 2: Register module in scheduler/mod.rs**

Add `pub(super) mod gui_pipeline;` and `GuiPipelineState` field to the scheduler state struct.

- [ ] **Step 3: Wire into monitor loop (loops.rs)**

After the `run_analysis_tick()` call inside the monitor loop, add:

```rust
// ── GUI Activity Intelligence pipeline ──
if config.analysis.gui_intelligence.enabled {
    if let Some(summary) = gui_pipeline::run_gui_tick(
        &mut gui_state,
        &latest_ocr_regions,
        &input_snap,
        &app_name,
        &focus_window_title,
        content_label,
        screen_resolution,
    ) {
        // Feed summary into content tracker on next tick
        gui_state_summary = Some(summary);
    }
}
```

Pass the GUI summary into `content_tracker.update()` (wiring from Task 8).

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p oneshim-tauri` (or the src-tauri crate name)

---

## Task 11: End-to-end integration tests

**Files:**
- New: `crates/oneshim-analysis/src/gui_aggregator.rs` (append `#[cfg(test)] mod tests`)
- New: `crates/oneshim-vision/src/gui_detector.rs` (append integration tests)

- [ ] **Step 1: Full pipeline integration test in gui_aggregator**

Test: synthetic `OcrRegion` grid → `GuiElementDetector::correlate_click` → build `GuiInteractionEvent` → push into `GuiActivityAggregator` → flush → verify `GuiActivitySummary` fields + `summary_line` content.

- [ ] **Step 2: ContentTracker + GUI summary propagation test**

Test: `ContentTracker::update()` with `gui_summary: Some(...)` → finalize → verify `ContentActivity.gui_summary` is preserved.

- [ ] **Step 3: GuiWorkTypeRefiner integration test**

Test: initial `WorkType::Unknown` + summary with heavy text_entries → refined to `FormFilling`.

- [ ] **Step 4: Run full workspace tests**

Run: `cargo test --workspace`
Run: `cargo clippy --workspace`
Run: `cargo fmt --check`

---

## Task 12: Commit and verify

- [ ] **Step 1: Final cargo check + test + clippy + fmt**

```bash
cargo check --workspace && cargo test --workspace && cargo clippy --workspace && cargo fmt --check
```

- [ ] **Step 2: Commit per-task or as a single feature commit**

Suggested commit message:

```
feat(analysis): add GUI Activity Intelligence Phase 2

- GuiActivityAggregator: time-window event grouping + semantic actions
- GuiActivitySummary model with summary_line generator
- GuiWorkTypeRefiner: post-hoc WorkType correction from GUI signals
- GuiElementDetector: upgraded InputOcrCorrelator with proximity fallback
- ContentActivity.gui_summary integration
- ContextAssembler enrichment with GUI summary lines
- gui_pipeline::run_gui_tick() scheduler entry point
- GuiIntelligenceConfig toggleable sub-section
```
