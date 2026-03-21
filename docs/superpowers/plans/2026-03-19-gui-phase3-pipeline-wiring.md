# GUI Activity Intelligence Phase 3 — Pipeline Wiring Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire all Phase 2 GUI activity intelligence components into the live scheduler pipeline so GUI activity intelligence actually runs. Currently `gui_pipeline.rs` exists but is dead code behind `#[allow(dead_code)]`. After this plan, the pipeline is live: mouse clicks correlate with OCR regions, shortcut names are captured, `GuiActivitySummary` feeds into `ContentTracker` and `ContextAssembler`, and GUI interactions persist to SQLite.

**Architecture:** No new crates or ports. Wiring changes in `src-tauri/src/scheduler/` (construction + loop calls), field additions in `oneshim-monitor` (InputActivityCollector), and connection of existing pure-algorithm components (`GuiElementDetector`, `GuiActivityAggregator`, `GuiWorkTypeRefiner`, `ContextAssembler`). Data flows through existing scheduler state using owned (non-Arc) structs for mutation without interior-mutability overhead.

**Tech Stack:** Rust, serde, chrono, oneshim-core models, oneshim-analysis, oneshim-vision, oneshim-storage (SQLite V13)

**Spec:** `docs/superpowers/specs/2026-03-19-gui-activity-intelligence-design.md`

---

## File Map

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-monitor/src/input_activity.rs` | Add `last_click_position` atomic fields + `record_click_at()` method; add `recent_shortcuts` ring buffer + `record_shortcut_name()` + `take_recent_shortcuts()` methods |
| `src-tauri/src/scheduler/mod.rs` | Add `gui_pipeline_state: Option<GuiPipelineState>` and `gui_work_type_refiner: GuiWorkTypeRefiner` fields to `AdaptiveTriggerState`; remove `#[allow(dead_code)]` from `gui_pipeline` module declaration |
| `src-tauri/src/scheduler/loops.rs` | Construct `GuiPipelineState` in `run_scheduler_loops()` (between `Mutex::take()` and `spawn_monitor_loop()`); call `run_gui_tick()` inside monitor loop after `run_analysis_tick()`; pass returned `GuiActivitySummary` to `ContentTracker` on next tick; call `save_gui_interaction()` for persistence |
| `src-tauri/src/scheduler/gui_pipeline.rs` | Add `recent_shortcuts: &[String]` parameter to `run_gui_tick()`; use first shortcut name from the slice (or `"unknown"` fallback) instead of hardcoded `"unknown"` |
| `src-tauri/src/scheduler/analysis_pipeline.rs` | Call `GuiWorkTypeRefiner::refine()` after `WorkTypeClassifier::classify()` when `gui_summary` is available; propagate `gui_summary` into `ContentUpdateInput` |
| `src-tauri/src/agent_runtime.rs` | Add `gui_pipeline_state: None` and `gui_work_type_refiner: oneshim_analysis::GuiWorkTypeRefiner` to the `AdaptiveTriggerState` struct literal (line 234) |
| `crates/oneshim-analysis/src/segment_summarizer.rs` | Add `to_content_summary_entries()` helper to convert `ContentActivity` → `ContentSummaryEntry` (including `gui_summary_line` mapping) |

---

## Task 1: Wire mouse last_position in InputActivityCollector

**Why:** `MouseActivity.last_position` is always `None` (line 168 of `input_activity.rs`). The GUI pipeline reads this to correlate clicks with OCR regions. Without real coordinates, `GuiElementDetector` falls back to (0, 0) and matches nothing useful.

**Files:**
- Modify: `crates/oneshim-monitor/src/input_activity.rs`

- [ ] **Step 1.1: Add atomic storage for last click position**

Add two new fields to `InputActivityCollector` after `right_click_count`:

```rust
// Last click position — atomic i32 pair (x, y). Updated on record_click_at().
// Defaults to i32::MIN to distinguish "never set" from (0, 0).
last_click_x: AtomicI32,
last_click_y: AtomicI32,
```

Import `AtomicI32` from `std::sync::atomic`. Initialize both to `i32::MIN` in `new()`.

```
cargo check -p oneshim-monitor
```

- [ ] **Step 1.2: Add `record_click_at()` method**

Add a public method below `record_right_click()`:

```rust
/// Record a left click at the given screen coordinates.
/// Position recording is opt-in — callers that lack consent simply call
/// `record_click()` (which does not update position).
pub fn record_click_at(&self, x: i32, y: i32) {
    self.click_count.fetch_add(1, Ordering::Relaxed);
    self.last_click_x.store(x, Ordering::Relaxed);
    self.last_click_y.store(y, Ordering::Relaxed);
    self.record_activity();
}
```

```
cargo check -p oneshim-monitor
```

- [ ] **Step 1.3: Populate `last_position` in `take_snapshot()`**

In `take_snapshot()`, replace the hardcoded `last_position: None` (line 168) with:

```rust
last_position: {
    let lx = self.last_click_x.swap(i32::MIN, Ordering::Relaxed);
    let ly = self.last_click_y.swap(i32::MIN, Ordering::Relaxed);
    if lx != i32::MIN && ly != i32::MIN {
        Some((lx as f32, ly as f32))
    } else {
        None
    }
},
```

```
cargo test -p oneshim-monitor -- input_activity
```

- [ ] **Step 1.4: Add test `record_click_at_sets_position`**

Add to the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn record_click_at_sets_position() {
    let collector = InputActivityCollector::new();
    collector.record_click_at(150, 300);

    let snapshot = collector.take_snapshot();
    assert_eq!(snapshot.mouse.click_count, 1);
    assert_eq!(snapshot.mouse.last_position, Some((150.0, 300.0)));
}

#[test]
fn position_resets_after_snapshot() {
    let collector = InputActivityCollector::new();
    collector.record_click_at(100, 200);
    let _ = collector.take_snapshot();

    let second = collector.take_snapshot();
    assert_eq!(second.mouse.last_position, None);
}
```

```
cargo test -p oneshim-monitor -- record_click_at
cargo test -p oneshim-monitor -- position_resets
```

**Commit:** `feat(monitor): wire mouse last_position in InputActivityCollector`

---

## Task 2: Add shortcut key name recording to InputActivityCollector

**Why:** `gui_pipeline.rs` line 122 hardcodes `keys: "unknown"` because `InputActivityCollector` only exposes `shortcut_count`, not the actual key names. The GUI pipeline needs key names (e.g., "Cmd+S") to detect semantic actions like Save, Undo, Search.

**Files:**
- Modify: `crates/oneshim-monitor/src/input_activity.rs`

- [ ] **Step 2.1: Add ring buffer for recent shortcut names**

Add a new field to `InputActivityCollector`:

```rust
/// Small ring buffer of recent shortcut key strings (e.g., "Cmd+S").
/// Capacity: 16. Protected by Mutex (low contention — written on shortcut,
/// drained on snapshot).
recent_shortcuts: Mutex<Vec<String>>,
```

Initialize in `new()` with `Mutex::new(Vec::with_capacity(16))`.

```
cargo check -p oneshim-monitor
```

- [ ] **Step 2.2: Add `record_shortcut_name()` method**

Add below `record_keystroke()`:

```rust
/// Record a keyboard shortcut with its human-readable name (e.g., "Cmd+S").
/// Also increments shortcut_count and total_keystrokes.
pub fn record_shortcut_name(&self, name: &str) {
    self.total_keystrokes.fetch_add(1, Ordering::Relaxed);
    self.shortcut_count.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut buf) = self.recent_shortcuts.lock() {
        if buf.len() >= 16 {
            buf.remove(0);
        }
        buf.push(name.to_string());
    }
    self.record_activity();
}
```

```
cargo check -p oneshim-monitor
```

- [ ] **Step 2.3: Add `take_recent_shortcuts()` method**

Add a public drain method:

```rust
/// Drain and return all recent shortcut names since last call.
pub fn take_recent_shortcuts(&self) -> Vec<String> {
    self.recent_shortcuts
        .lock()
        .map(|mut buf| std::mem::take(&mut *buf))
        .unwrap_or_default()
}
```

```
cargo check -p oneshim-monitor
```

- [ ] **Step 2.4: Add tests for shortcut name recording**

```rust
#[test]
fn records_shortcut_names() {
    let collector = InputActivityCollector::new();
    collector.record_shortcut_name("Cmd+S");
    collector.record_shortcut_name("Cmd+Z");

    let shortcuts = collector.take_recent_shortcuts();
    assert_eq!(shortcuts, vec!["Cmd+S", "Cmd+Z"]);

    let snapshot = collector.take_snapshot();
    assert_eq!(snapshot.keyboard.shortcut_count, 2);
    assert_eq!(snapshot.keyboard.total_keystrokes, 2);
}

#[test]
fn shortcut_ring_buffer_caps_at_16() {
    let collector = InputActivityCollector::new();
    for i in 0..20 {
        collector.record_shortcut_name(&format!("Key+{i}"));
    }

    let shortcuts = collector.take_recent_shortcuts();
    assert_eq!(shortcuts.len(), 16);
    // Oldest 4 evicted, first remaining is "Key+4"
    assert_eq!(shortcuts[0], "Key+4");
}
```

```
cargo test -p oneshim-monitor -- records_shortcut_names
cargo test -p oneshim-monitor -- shortcut_ring_buffer
```

**Commit:** `feat(monitor): add shortcut key name recording to InputActivityCollector`

---

## Task 3: Construct GuiPipelineState in scheduler setup

**Why:** `GuiPipelineState` (from `gui_pipeline.rs`) holds `GuiElementDetector` and `GuiActivityAggregator`, but nothing instantiates them. The scheduler must construct this state during setup so the monitor loop can use it.

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs`
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] **Step 3.1: Remove `#[allow(dead_code)]` from gui_pipeline module**

In `src-tauri/src/scheduler/mod.rs`, change:

```rust
/// GUI Activity Intelligence pipeline. Not yet wired into the scheduler loop.
/// Will be integrated after the accessibility API adapter stabilizes (Batch 4),
/// which provides the OCR region stream needed by `run_gui_tick()`.
#[allow(dead_code)]
pub(crate) mod gui_pipeline;
```

To:

```rust
/// GUI Activity Intelligence pipeline — wired into the monitor loop.
/// Called after `run_analysis_tick()` each cycle when `gui_intelligence.enabled`.
pub(crate) mod gui_pipeline;
```

```
cargo check -p oneshim-tauri 2>&1 | head -30
```

(Expect warnings about unused imports until later steps wire the calls.)

- [ ] **Step 3.2: Add `gui_pipeline_state` to `AdaptiveTriggerState`**

In `src-tauri/src/scheduler/mod.rs`, add a new field to `AdaptiveTriggerState`:

```rust
// --- GUI Activity Intelligence pipeline state ---
pub(crate) gui_pipeline_state: Option<gui_pipeline::GuiPipelineState>,
```

Update the `with_adaptive_trigger()` builder or any construction site of `AdaptiveTriggerState` to initialize this field. The field will be `None` by default and set during scheduler setup.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 3.3: Construct GuiPipelineState in `run_scheduler_loops()`**

In `src-tauri/src/scheduler/loops.rs`, inside `run_scheduler_loops()`, the construction must occur **between** the `Mutex::take()` call (line 1142-1146, which extracts the `AdaptiveTriggerState`) and the `spawn_monitor_loop()` call (line 1148, which consumes it). This is the only window where `adaptive_trigger_state` is an `Option<AdaptiveTriggerState>` in the local scope. Insert the following block immediately after `let adaptive_trigger_state = ... .take();` and before `let monitor_task = self.spawn_monitor_loop(...)`:


```rust
// Construct GUI pipeline state if enabled
if let Some(ref mut ts) = adaptive_trigger_state {
    let gui_config = self.config_manager
        .as_ref()
        .map(|cm| cm.get().analysis.gui_intelligence.clone())
        .unwrap_or_default();

    if gui_config.enabled {
        use oneshim_vision::gui_detector::GuiElementDetector;
        use oneshim_analysis::gui_aggregator::GuiActivityAggregator;
        use super::gui_pipeline::GuiPipelineState;

        let detector = GuiElementDetector::new(
            (0, 0), // screen resolution — updated per tick from WindowLayoutEvent
            oneshim_core::config::PiiFilterLevel::Standard,
        );
        let aggregator = GuiActivityAggregator::new(&gui_config);
        ts.gui_pipeline_state = Some(GuiPipelineState { detector, aggregator });
        info!("GUI Activity Intelligence pipeline enabled");
    }
}
```

Add `use tracing::info;` import if not already present (it is).

```
cargo check -p oneshim-tauri
```

- [ ] **Step 3.4: Initialize `gui_pipeline_state` to `None` at all existing construction sites**

`AdaptiveTriggerState` is constructed as a struct literal at **one known site**: `src-tauri/src/agent_runtime.rs` line 234 (inside an `if self.config.analysis.tiered_memory.enabled && consent_ok` block). The struct literal spans lines 234-287 and is passed to `scheduler.with_adaptive_trigger(state)` at line 288. Add `gui_pipeline_state: None,` to this struct literal after the `embedding_pipeline` field.

To guard against future construction sites, search the workspace:

```bash
grep -rn "AdaptiveTriggerState {" src-tauri/
```

There is no builder pattern for `AdaptiveTriggerState` — the `with_adaptive_trigger()` method on `Scheduler` (mod.rs line 195) accepts the fully-constructed struct, so the only place to update is the `agent_runtime.rs` literal.

```
cargo check -p oneshim-tauri
cargo test -p oneshim-tauri
```

**Commit:** `feat(scheduler): construct GuiPipelineState during scheduler setup`

---

## Task 4: Wire `run_gui_tick()` into the monitor loop

**Why:** `run_gui_tick()` exists but is never called. The monitor loop must call it after `run_analysis_tick()`, passing OCR regions + input snapshot, and feed the returned `GuiActivitySummary` into `ContentTracker` on the next tick.

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] **Step 4.1: Add mutable state for last GUI summary**

Inside the `spawn_monitor_loop` async block (after the `let ring_buffer = ...` line around 160), add a local variable to carry the GUI summary across ticks:

```rust
let mut last_gui_summary: Option<oneshim_core::models::gui_activity::GuiActivitySummary> = None;
```

```
cargo check -p oneshim-tauri
```

- [ ] **Step 4.2: Add mutable state for last OCR regions**

OCR regions come from frame captures (the `handle_frame_capture` path). We need to stash them across ticks. After the `last_gui_summary` variable, add:

```rust
let mut last_ocr_regions: Vec<oneshim_core::models::frame::OcrRegion> = Vec::new();
```

Note: In the current codebase, `handle_frame_capture` returns `Option<String>` (OCR text), not `Vec<OcrRegion>`. For Phase 3, we use empty regions as a fallback. A separate task can pipe actual `OcrRegion` extraction from the frame processor. For now, the pipeline is wired but produces events only when `last_position` lands on previously-cached regions.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 4.3: Call `run_gui_tick()` after `run_analysis_tick()`**

In the monitor loop body, after the `run_analysis_tick()` call block (around line 324-333), add:

```rust
// ── GUI Activity Intelligence pipeline ──
if let Some(ref mut ts) = adaptive_trigger_state {
    if let Some(ref mut gui_state) = ts.gui_pipeline_state {
        // Drain shortcut names before passing to GUI pipeline (Task 4.4)
        let recent_shortcuts = input_collector.take_recent_shortcuts();

        let gui_summary = super::gui_pipeline::run_gui_tick(
            gui_state,
            &last_ocr_regions,
            &input_snap,         // shared snapshot from Task 4.5
            &recent_shortcuts,   // shortcut names from Task 4.4
            &app_name,
            &focus_window_title,
            &parsed_content_label,
        );

        if gui_summary.is_some() {
            last_gui_summary = gui_summary;
        }
    }
}
```

This requires `parsed_content_label` to be available. Extract it from the title bar parser output that already runs during `run_analysis_tick`. Add a local variable before the analysis tick block:

```rust
let parsed_content_label = adaptive_trigger_state
    .as_ref()
    .and_then(|ts| {
        ts.title_bar_parser
            .parse(&app_name, &focus_window_title)
            .map(|c| c.content_label)
    })
    .unwrap_or_default();
```

**Important:** `take_snapshot()` drains counters (resets to zero). The analysis pipeline already calls `take_snapshot()` at line 57 of `analysis_pipeline.rs`. To avoid the GUI pipeline getting an empty snapshot, we must take the snapshot **once** in the loop body and share it between both pipelines:

Instead of calling `input_collector.take_snapshot()` inside `run_analysis_tick()`, take it in the loop body and pass a reference. This requires changing `run_analysis_tick` to accept `&InputActivityEvent` instead of `&Arc<InputActivityCollector>`.

**Alternative (minimal change):** Use `peek_activity_level()` for the analysis pipeline's purposes (it does not drain), and reserve `take_snapshot()` for the GUI pipeline. However, the analysis pipeline uses the full snapshot for `WorkTypeClassifier`. The cleanest approach: take the snapshot once in the loop body, pass it to both pipelines.

Concretely:
1. In the loop body, before the analysis tick, add: `let input_snap = input_collector.take_snapshot();`
2. Change `run_analysis_tick` signature to accept `input_snap: &InputActivityEvent` instead of `input_collector: &Arc<InputActivityCollector>` (Step 4.5).
3. Pass the same `input_snap` to `run_gui_tick`.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 4.4: Wire shortcut names into `run_gui_tick()`**

The File Map states that `gui_pipeline.rs` will read `recent_shortcuts` instead of the hardcoded `"unknown"` (line 123), but no step implements this wiring. Add a `recent_shortcuts: &[String]` parameter to `run_gui_tick()`:

In `src-tauri/src/scheduler/gui_pipeline.rs`, change the signature:

```rust
pub(crate) fn run_gui_tick(
    state: &mut GuiPipelineState,
    ocr_regions: &[OcrRegion],
    input_snap: &InputActivityEvent,
    recent_shortcuts: &[String],   // NEW
    app_name: &str,
    window_title: &str,
    content_label: &str,
) -> Option<GuiActivitySummary> {
```

Then replace the hardcoded `"unknown"` (line 122-124) with:

```rust
interaction: Some(InteractionType::KeyboardShortcut {
    keys: recent_shortcuts.first()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string()),
}),
```

In the monitor loop body (Task 4.3), before calling `run_gui_tick()`, drain the shortcut names:

```rust
let recent_shortcuts = input_collector.take_recent_shortcuts();
```

Pass `&recent_shortcuts` to `run_gui_tick()`.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 4.5: Refactor `run_analysis_tick` to accept `&InputActivityEvent`**

In `src-tauri/src/scheduler/analysis_pipeline.rs`, change the signature:

```rust
pub(super) async fn run_analysis_tick(
    ts: &mut AdaptiveTriggerState,
    app_name: &str,
    window_title: &str,
    prev_app: &Option<String>,
    app_changed: bool,
    input_snap: &oneshim_core::models::event::InputActivityEvent,  // was &Arc<InputActivityCollector>
    storage: &Arc<dyn StorageService>,
)
```

Remove the `input_collector.take_snapshot()` call at line 57 and use the passed-in `input_snap` directly. Update the call site in `loops.rs` to pass `&input_snap`.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 4.6: Feed `last_gui_summary` into ContentTracker on next tick**

In the analysis pipeline, the `ContentTracker::update()` call (analysis_pipeline.rs line 77-85) currently passes `gui_summary: None`. Change it to accept the GUI summary from the caller:

Update `run_analysis_tick` signature to also accept `gui_summary: Option<&GuiActivitySummary>`:

```rust
pub(super) async fn run_analysis_tick(
    ts: &mut AdaptiveTriggerState,
    app_name: &str,
    window_title: &str,
    prev_app: &Option<String>,
    app_changed: bool,
    input_snap: &oneshim_core::models::event::InputActivityEvent,
    gui_summary: Option<&oneshim_core::models::gui_activity::GuiActivitySummary>,
    storage: &Arc<dyn StorageService>,
)
```

Then at line 84:

```rust
gui_summary: gui_summary.cloned(),  // was None
```

In the loop body call site, pass `last_gui_summary.as_ref()`.

After the call, clear it so it is only consumed once:

```rust
last_gui_summary = None;
```

Wait -- the GUI tick runs AFTER the analysis tick in the same cycle. So `last_gui_summary` from the **previous** cycle feeds into the **current** analysis tick. This is correct: the analysis tick for cycle N uses the GUI summary produced in cycle N-1.

```
cargo check -p oneshim-tauri
cargo test -p oneshim-tauri
```

**Commit:** `feat(scheduler): wire run_gui_tick() into the monitor loop`

---

## Task 5: Wire GuiWorkTypeRefiner into the analysis pipeline

**Why:** `GuiWorkTypeRefiner` exists in `oneshim-analysis` but is never called. It should run after `WorkTypeClassifier::classify()` to correct the initial work type using GUI signals (e.g., "Unknown" + save clicks = "ActiveCoding").

**Files:**
- Modify: `src-tauri/src/scheduler/analysis_pipeline.rs`

- [ ] **Step 5.1: Add GuiWorkTypeRefiner to AdaptiveTriggerState**

In `src-tauri/src/scheduler/mod.rs`, add to `AdaptiveTriggerState`:

```rust
// --- GUI Work Type Refiner ---
pub(crate) gui_work_type_refiner: oneshim_analysis::GuiWorkTypeRefiner,
```

Initialize it at the construction site in `src-tauri/src/agent_runtime.rs` (line 234, the same struct literal updated in Task 3.4):

```rust
gui_work_type_refiner: oneshim_analysis::GuiWorkTypeRefiner,
```

(`GuiWorkTypeRefiner` is a unit struct -- no constructor args needed. There is only one construction site for `AdaptiveTriggerState`: the struct literal in `agent_runtime.rs`.)

```
cargo check -p oneshim-tauri
```

- [ ] **Step 5.2: Call `refine()` after `classify()`**

In `analysis_pipeline.rs`, after the work type classification block (lines 59-72), add:

```rust
// 4b. Refine work type using GUI signals (if available)
let work_type = if let Some(ref gui) = gui_summary {
    ts.gui_work_type_refiner.refine(work_type, gui)
} else {
    work_type
};
```

This shadows the `work_type` binding with the refined version.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 5.3: Verify with existing tests**

```
cargo test -p oneshim-tauri
cargo test -p oneshim-analysis -- gui_work_type_refiner
```

**Commit:** `feat(scheduler): wire GuiWorkTypeRefiner into analysis pipeline`

---

## Task 6: Persist GUI interactions to SQLite

**Why:** `gui_interactions` table exists (V13 migration) and `WebStorage::save_gui_interaction()` is implemented in `SqliteStorage`, but nothing calls it. The GUI pipeline should persist interaction events for the web dashboard and historical analysis.

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs`
- Modify: `src-tauri/src/scheduler/config.rs` (if `SchedulerStorage` needs extension)

- [ ] **Step 6.1: Add `save_gui_interaction` to `SchedulerStorage` trait**

`save_gui_interaction` currently lives on `WebStorage`. The scheduler uses `SchedulerStorage`. Add a delegation method to `SchedulerStorage` in `config.rs`:

```rust
/// Save a GUI interaction event (delegates to WebStorage V13 table).
fn save_gui_interaction(
    &self,
    input: &oneshim_core::models::storage_records::NewGuiInteraction<'_>,
) -> Result<(), oneshim_core::error::CoreError>;
```

And implement it for `SqliteStorage`:

```rust
fn save_gui_interaction(
    &self,
    input: &oneshim_core::models::storage_records::NewGuiInteraction<'_>,
) -> Result<(), oneshim_core::error::CoreError> {
    oneshim_core::ports::web_storage::WebStorage::save_gui_interaction(self, input)
}
```

Add the necessary import for `NewGuiInteraction` at the top of `config.rs`.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 6.2: Persist events from `run_gui_tick()` output**

In the monitor loop body, after the `run_gui_tick()` call in Task 4.3, when a GUI interaction event is produced, persist it. The simplest approach is to persist when a summary flushes. But summaries aggregate many events -- individual events are more valuable.

Instead, enhance `run_gui_tick` to return both the summary AND the individual events. But that changes the existing API. The minimal approach: persist inside `run_gui_tick` by passing a callback, or persist from the loop body using the input snapshot data.

**Cleanest approach:** Persist inside the same `if let` block where `run_gui_tick()` is called (Task 4.3), so `input_snap` is in scope. The persistence code must live within the `if let Some(ref mut gui_state) = ts.gui_pipeline_state` block, not in a separate block. This avoids the scoping issue where `gui_input_snap` would be inaccessible.

In the loop body, **inside** the existing GUI tick block from Task 4.3:

```rust
// ── GUI Activity Intelligence pipeline ──
if let Some(ref mut ts) = adaptive_trigger_state {
    if let Some(ref mut gui_state) = ts.gui_pipeline_state {
        let recent_shortcuts = input_collector.take_recent_shortcuts();
        let gui_summary = super::gui_pipeline::run_gui_tick(
            gui_state,
            &last_ocr_regions,
            &input_snap,
            &recent_shortcuts,
            &app_name,
            &focus_window_title,
            &parsed_content_label,
        );

        if gui_summary.is_some() {
            last_gui_summary = gui_summary;
        }

        // Persist GUI interaction to SQLite (same block — input_snap in scope)
        if input_snap.mouse.click_count > 0 {
            let event_id = uuid::Uuid::new_v4().to_string();
            // segment_id: use None when no segment is active. When a segment
            // is active, we lack the segment UUID here (it's only generated
            // at segment close in handle_segment_close). Pass None for now;
            // a follow-up can thread the segment ID through AdaptiveTriggerState.
            // IMPORTANT: do NOT use ts.current_regime_id — that is the regime
            // ID, not the segment ID.
            let segment_id: Option<String> = None;
            let timestamp_str = chrono::Utc::now().to_rfc3339();

            let input = oneshim_core::models::storage_records::NewGuiInteraction {
                event_id: &event_id,
                segment_id: segment_id.as_deref(),
                timestamp: &timestamp_str,
                element_text: None,
                element_type: Some("Click"),
                interaction_type: "Click",
                bbox_json: None,
                app_name: &app_name,
            };

            if let Err(e) = sqlite1.save_gui_interaction(&input) {
                warn!("GUI interaction save failure: {e}");
            }
        }
    }
}
```

Note: `sqlite1` is the `Arc<dyn SchedulerStorage>` clone in the monitor loop.

```
cargo check -p oneshim-tauri
```

- [ ] **Step 6.3: Add test for `save_gui_interaction` on SchedulerStorage**

```rust
#[test]
fn scheduler_storage_delegates_gui_save() {
    // Verify the SchedulerStorage trait has save_gui_interaction
    // This is a compile-time check — if the trait method exists, it compiles
    fn _assert_method<T: SchedulerStorage>(s: &T) {
        let input = oneshim_core::models::storage_records::NewGuiInteraction {
            event_id: "test-id",
            segment_id: None,
            timestamp: "2026-03-19T00:00:00Z",
            element_text: None,
            element_type: None,
            interaction_type: "Click",
            bbox_json: None,
            app_name: "Test",
        };
        let _ = s.save_gui_interaction(&input);
    }
}
```

```
cargo test -p oneshim-tauri -- scheduler_storage_delegates
```

**Commit:** `feat(scheduler): persist GUI interactions to SQLite via SchedulerStorage`

---

## Task 7: Propagate gui_summary_line to ContextAssembler

**Why:** `ContextAssembler` already supports `gui_summary_line` on `ContentSummaryEntry` (assembler.rs lines 38-40, 238-239) and enriches the LLM context `content` field when present. But nothing populates it. The analysis pipeline must extract `summary_line` from `GuiActivitySummary` and pass it through `ContentActivity` to the assembler.

**Files:**
- Modify: `src-tauri/src/scheduler/analysis_pipeline.rs`
- Modify: `crates/oneshim-analysis/src/segment_summarizer.rs` (add `to_content_summary_entries()` conversion helper)

- [ ] **Step 7.1: Verify ContentActivity carries gui_summary through to SegmentSummary**

Check that `ContentActivity.gui_summary` (set in Task 4.6 via `ContentUpdateInput`) is preserved when `ContentTracker::drain_all()` produces `ContentActivity` records, and that those records are available when `SegmentSummarizer::summarize()` builds `SegmentSummary.content_activities`.

This should already work because `ContentTracker` stores the `gui_summary` on `ActiveContent` and propagates it in `finalize_current()`. Verify with a targeted test:

```
cargo test -p oneshim-analysis -- content_tracker::tests::gui_summary_propagates
```

If this test does not exist, add it in `crates/oneshim-analysis/src/content_tracker.rs`:

```rust
#[test]
fn gui_summary_propagates_through_content_change() {
    use oneshim_core::models::gui_activity::GuiActivitySummary;

    let mut tracker = ContentTracker::new();
    let now = Utc::now();

    let summary = GuiActivitySummary {
        app_name: "VSCode".to_string(),
        summary_line: "edited main.rs: 3 saves".to_string(),
        ..Default::default()
    };

    // First update with GUI summary
    tracker.update(input(
        "main.rs", ContentType::File, WorkType::ActiveCoding,
        EngagementMetrics::default(), 0.9, now,
        Some(summary.clone()),
    ));

    // Switch content to trigger finalization
    let finalized = tracker.update(input(
        "lib.rs", ContentType::File, WorkType::Reading,
        EngagementMetrics::default(), 0.8,
        now + chrono::Duration::minutes(5), None,
    ));

    let finalized = finalized.expect("should finalize previous content");
    assert!(finalized.gui_summary.is_some());
    assert_eq!(
        finalized.gui_summary.unwrap().summary_line,
        "edited main.rs: 3 saves"
    );
}
```

```
cargo test -p oneshim-analysis -- gui_summary_propagates
```

- [ ] **Step 7.2: Implement ContentActivity → ContentSummaryEntry conversion**

**Problem:** No production code currently converts `ContentActivity` records into `ContentSummaryEntry`. The `SegmentSummarizer` produces `SegmentSummary` which contains `content_activities: Vec<ContentActivity>`, but `SegmentStats` (consumed by `ContextAssembler`) expects `content_summary: Vec<ContentSummaryEntry>`. This conversion does not exist yet and must be implemented.

**Where to add it:** The conversion should live in `SegmentSummarizer` (or as a standalone helper in `oneshim-analysis`) because it bridges the segment domain model (`ContentActivity`) to the assembler domain model (`ContentSummaryEntry`). Add a helper method:

```rust
/// Convert a slice of ContentActivity records into ContentSummaryEntry values
/// for the ContextAssembler.
pub fn to_content_summary_entries(
    activities: &[ContentActivity],
) -> Vec<ContentSummaryEntry> {
    activities
        .iter()
        .map(|ca| ContentSummaryEntry {
            content: ca.content_label.clone(),
            content_type: format!("{:?}", ca.content_type),
            work_type: format!("{:?}", ca.work_type),
            mins: (ca.duration_secs / 60).max(1) as u32,
            gui_summary_line: ca.gui_summary.as_ref().map(|gs| gs.summary_line.clone()),
        })
        .collect()
}
```

The key mapping for the GUI summary line is:
`ContentActivity.gui_summary.map(|gs| gs.summary_line)` → `ContentSummaryEntry.gui_summary_line`

Then, wherever `SegmentStats` is constructed from a `SegmentSummary` (this construction site also needs to be created — likely in the analysis pipeline or a scheduler helper), call this conversion:

```rust
let stats = SegmentStats {
    duration_mins: (summary.duration_secs / 60) as u32,
    regime_label: summary.regime_id.clone(),
    event_count: summary.event_count,
    context_switches: summary.context_switch_count,
    dominant_category: summary.dominant_category.clone(),
    content_summary: to_content_summary_entries(&summary.content_activities),
};
```

The assembler already handles the downstream enrichment (lines 238-239 of `assembler.rs`):

```rust
let content = if let Some(ref gui_line) = e.gui_summary_line {
    format!("{} ({})", e.content, gui_line)
} else {
    e.content.clone()
};
```

So once this conversion populates `gui_summary_line`, the enrichment flows through automatically.

```
cargo test -p oneshim-analysis -- assembler
```

- [ ] **Step 7.3: Add integration test for enriched LLM context**

In `crates/oneshim-analysis/src/assembler.rs` tests, add:

```rust
#[test]
fn build_with_segment_enriches_gui_summary_line() {
    let assembler = ContextAssembler::new(noop_filter());
    let stats = SegmentStats {
        duration_mins: 15,
        regime_label: None,
        event_count: 30,
        context_switches: 2,
        dominant_category: "Development".to_string(),
        content_summary: vec![ContentSummaryEntry {
            content: "auth.rs".to_string(),
            content_type: "File".to_string(),
            work_type: "ActiveCoding".to_string(),
            mins: 15,
            gui_summary_line: Some("3 saves, 2 test runs".to_string()),
        }],
    };
    let ctx = assembler.build_with_segment(
        &make_current(), &[], &[], &make_metrics(), Some(&stats),
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&ctx.user_context_json).unwrap();
    let content = parsed["current_segment"]["content_summary"][0]["content"]
        .as_str()
        .unwrap();
    assert_eq!(content, "auth.rs (3 saves, 2 test runs)");
}
```

```
cargo test -p oneshim-analysis -- build_with_segment_enriches
```

**Commit:** `feat(scheduler): propagate gui_summary_line to ContextAssembler`

---

## Verification

After all 7 tasks, run the full verification suite:

```bash
# Build check — no dead_code warnings for gui_pipeline
cargo check --workspace 2>&1 | grep -i "dead_code.*gui"

# All tests pass
cargo test --workspace

# Clippy clean
cargo clippy --workspace

# Format check
cargo fmt --check
```

---

## Dependency Graph

```
Task 1 (mouse position)  ──┐
                            ├── Task 4 (wire run_gui_tick)
Task 2 (shortcut names)  ──┘         │
                                      ├── Task 6 (persist to SQLite)
Task 3 (construct state) ────────────┘         │
                                               │
Task 5 (refiner wiring) ──── Task 4 ──────────┤
                                               │
Task 7 (assembler wiring) ── Task 4 ──────────┘
```

Tasks 1, 2, 3 are independent and can be done in parallel.
Task 4 depends on Tasks 1, 2, 3.
Tasks 5, 6, 7 depend on Task 4 and can be done in parallel.

---

## Risk Notes

1. **Snapshot double-drain (monitor loop):** `InputActivityCollector::take_snapshot()` resets counters. Step 4.5 refactors `run_analysis_tick` to accept a pre-taken snapshot so both pipelines (analysis + GUI) share the same data within the monitor loop. This is the highest-risk refactor -- if any other code calls `take_snapshot()` on the same collector in the same tick, counts are lost.

2. **Triple-drain with event_snapshot_loop (pre-existing race):** `spawn_event_snapshot_loop` (loops.rs:908) independently calls `take_snapshot()` on a separate clone of the same `InputActivityCollector`. After the monitor loop's `take_snapshot()` drains counters, the event_snapshot_loop may get empty/partial data on an overlapping tick. This is a **pre-existing race condition** that is not introduced by this plan. It is acceptable because: (a) the event_snapshot_loop uses its own clone of the `Arc<InputActivityCollector>`, so atomic counter reads are consistent per-call; (b) the two loops run at different intervals (30s for event_snapshot vs 10s for monitor loop); (c) the event_snapshot_loop is used for persistence/telemetry, not for the analysis pipeline's real-time decisions. No fix is needed in this plan, but a future consolidation could merge the snapshot calls.

3. **OCR regions not yet piped:** The current `handle_frame_capture` returns `Option<String>` (OCR text), not `Vec<OcrRegion>`. Phase 3 wires the pipeline with `last_ocr_regions` but initially gets empty vectors. A follow-up task should pipe actual OCR regions from the frame processor. The pipeline still produces events for keyboard shortcuts and text entry even without OCR regions.

4. **Screen resolution (0, 0):** `GuiElementDetector::new()` is called with `(0, 0)` screen resolution. The detector uses this for element type inference heuristics (e.g., "y < 30px = menu bar"). A follow-up should update resolution from `WindowLayoutEvent.screen_resolution` on each tick. For now, the heuristic degrades gracefully (some element types may be classified as `Unknown`).
