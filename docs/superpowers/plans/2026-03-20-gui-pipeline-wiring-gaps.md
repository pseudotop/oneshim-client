# GUI Pipeline Wiring Gaps — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire two dead-code paths so GUI activity data actually reaches the LLM: (1) call `detect_gui_patterns()` in the analysis pipeline and propagate results, (2) construct `SegmentStats` from `AdaptiveTriggerState` and pass it to `ContextAnalyzer` so the LLM receives the `current_segment` + future `gui` section.

**Architecture:** Both gaps are wiring-only — no new algorithms or data models. Gap 1 connects the existing `detect_gui_patterns()` function into the segment summarizer. Gap 2 builds a `SegmentStats` snapshot from the scheduler's `AdaptiveTriggerState` and passes it through `ContextAnalyzer.analyze()` → `ContextAssembler.build_with_history()`.

**Tech Stack:** Rust, oneshim-analysis (assembler, analyzer, segment_summarizer, gui_patterns), src-tauri scheduler

**Prerequisite:** Must be completed BEFORE `2026-03-20-gui-phase3-llm-context-advanced.md` Task 1 (structured GUI section), otherwise the `gui` section will never reach the LLM.

---

## File Map

### Task 1 — Wire `detect_gui_patterns()` into segment summarizer

| File | Change |
|------|--------|
| `crates/oneshim-analysis/src/segment_summarizer.rs` | Call `detect_gui_patterns()` when `ContentActivity.gui_summary` is present; store results in `ContentSummaryEntry.gui_patterns` |
| `crates/oneshim-analysis/src/assembler.rs` | Add `gui_patterns: Vec<String>` field to `ContentSummaryEntry` and `SegmentStats` |

### Task 2 — Wire `SegmentStats` into `ContextAnalyzer.analyze()`

| File | Change |
|------|--------|
| `crates/oneshim-analysis/src/analyzer.rs` | Accept `Option<&SegmentStats>` in `analyze()` and `analyze_if_changed()`; pass through to `build_with_history()` instead of `None` |
| `src-tauri/src/scheduler/loops.rs` | Build `SegmentStats` from `AdaptiveTriggerState` and pass to `ContextAnalyzer` calls |

---

## Task 1: Wire `detect_gui_patterns()` into segment summarizer

**Why:** `detect_gui_patterns()` exists in `gui_patterns.rs` with 5 patterns (FrequentSave, TestDrivenDevelopment, CodeReviewFlow, DebuggingLoop, ReferenceHopping) and 20 tests, but is never called outside tests. The segment summarizer already maps `ContentActivity` → `ContentSummaryEntry` with `gui_summary_line`. Adding `gui_patterns` here connects the detection to the downstream LLM payload.

**Files:**
- Modify: `crates/oneshim-analysis/src/assembler.rs`
- Modify: `crates/oneshim-analysis/src/segment_summarizer.rs`

- [ ] **Step 1.1: Add `gui_patterns` field to `ContentSummaryEntry`**

In `crates/oneshim-analysis/src/assembler.rs`, find `pub struct ContentSummaryEntry` (~line 33) and add after `gui_summary_line`:

```rust
/// GUI behavioral patterns detected from this content activity (e.g. "TestDrivenDevelopment").
pub gui_patterns: Vec<String>,
```

Fix all `ContentSummaryEntry` struct literals in the file (tests, etc.) to include `gui_patterns: vec![]`.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 1.2: Add `gui_patterns` field to `SegmentStats`**

In same file, find `pub struct SegmentStats` (~line 22) and add:

```rust
/// Aggregated GUI patterns across all content activities in this segment.
pub gui_patterns: Vec<String>,
```

Fix all `SegmentStats` struct literals in the file to include `gui_patterns: vec![]`.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 1.3: Call `detect_gui_patterns()` in `to_content_summary_entries()`**

In `crates/oneshim-analysis/src/segment_summarizer.rs`, find the mapping at ~line 188 that creates `ContentSummaryEntry`. Add gui_patterns detection:

```rust
use crate::pattern_miner::detect_gui_patterns;

// Inside the .map() closure:
let gui_patterns = ca.gui_summary.as_ref()
    .map(|gs| detect_gui_patterns(gs, ca.work_type)
        .into_iter()
        .map(|p| format!("{:?}", p))
        .collect::<Vec<_>>())
    .unwrap_or_default();

crate::assembler::ContentSummaryEntry {
    content: ca.content_label.clone(),
    content_type: format!("{:?}", ca.content_type),
    work_type: format!("{:?}", ca.work_type),
    mins: (ca.duration_secs / 60).max(1) as u32,
    gui_summary_line: ca.gui_summary.as_ref().map(|gs| gs.summary_line.clone()),
    gui_patterns,
}
```

```
cargo check -p oneshim-analysis
```

- [ ] **Step 1.4: Write test**

In `segment_summarizer.rs` tests, add:

```rust
#[test]
fn gui_patterns_populated_from_summary() {
    // Build a ContentActivity with gui_summary that triggers TDD pattern
    let mut summary = /* GuiActivitySummary with test_run_count=1, save_count=1 */;
    let activity = ContentActivity {
        work_type: WorkType::ActiveCoding,
        gui_summary: Some(summary),
        ..Default::default()
    };
    let entries = to_content_summary_entries(&[activity]);
    assert!(entries[0].gui_patterns.contains(&"TestDrivenDevelopment".to_string()));
}
```

```
cargo test -p oneshim-analysis -- segment_summarizer::tests::gui_patterns
```

- [ ] **Step 1.5: Commit**

```
git add crates/oneshim-analysis/src/assembler.rs crates/oneshim-analysis/src/segment_summarizer.rs
git commit -m "feat(analysis): wire detect_gui_patterns() into segment summarizer pipeline"
```

---

## Task 2: Wire `SegmentStats` into `ContextAnalyzer.analyze()`

**Why:** `ContextAnalyzer.analyze()` at `analyzer.rs:146` passes `None` for `segment_stats`, which means the `current_segment` section is never included in the LLM JSON payload. The scheduler's `AdaptiveTriggerState` already has all the data needed to construct a `SegmentStats` — it just needs to be built and passed through.

**Files:**
- Modify: `crates/oneshim-analysis/src/analyzer.rs`
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] **Step 2.1: Add `segment_stats` parameter to `ContextAnalyzer.analyze()`**

In `crates/oneshim-analysis/src/analyzer.rs`, change the `analyze()` method signature to accept optional segment stats:

```rust
pub async fn analyze(
    &self,
    segment_stats: Option<&SegmentStats>,
) -> Result<Vec<Suggestion>, CoreError> {
```

Update the `build_with_history()` call at ~line 141 to pass `segment_stats` instead of `None`:

```rust
let ctx = self.context_assembler.build_with_history(
    &current,
    &events,
    &patterns,
    &metrics,
    segment_stats,  // was: None
    &relevant_history,
);
```

Do the same for `analyze_if_changed()` if it exists — add `segment_stats: Option<&SegmentStats>` parameter and forward it.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 2.2: Fix call sites in scheduler**

In `src-tauri/src/scheduler/loops.rs`, find all calls to `analyzer.analyze()` and `analyzer.analyze_if_changed()`. They need to pass `None` initially (compile fix), then we wire real data:

Search for:
- `analyzer.analyze().await` (~line 1490) → change to `analyzer.analyze(None).await`
- `analyzer.analyze_if_changed().await` (~line 1492) → change to `analyzer.analyze_if_changed(None).await`

Also update `handle_event_analysis()` (~line 43-76) if it calls `analyze()`.

```
cargo check -p oneshim-app
```

- [ ] **Step 2.3: Build `SegmentStats` from `AdaptiveTriggerState`**

Add a helper function in `src-tauri/src/scheduler/loops.rs` or `analysis_pipeline.rs`:

```rust
fn build_segment_stats(ts: &AdaptiveTriggerState) -> Option<oneshim_analysis::assembler::SegmentStats> {
    use oneshim_analysis::assembler::SegmentStats;

    let entries = ts.segment_buffer.current_content_summary()?;
    if entries.is_empty() {
        return None;
    }

    let duration_mins = ts.segment_buffer.current_duration_secs().map(|s| (s / 60) as u32).unwrap_or(0);
    let regime_label = ts.current_regime_id.clone();
    let event_count = ts.segment_buffer.current_event_count().unwrap_or(0);
    let context_switches = ts.segment_buffer.current_context_switches().unwrap_or(0);

    // Aggregate gui_patterns across all content summaries
    let gui_patterns: Vec<String> = entries.iter()
        .flat_map(|e| e.gui_patterns.iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    Some(SegmentStats {
        duration_mins,
        regime_label,
        event_count,
        context_switches,
        dominant_category: entries.first().map(|e| e.content_type.clone()).unwrap_or_default(),
        content_summary: entries,
        gui_patterns,
    })
}
```

Note: The exact `SegmentBuffer` API methods (`current_content_summary()`, `current_duration_secs()`, etc.) need to be verified — adjust method names to match the actual API.

```
cargo check -p oneshim-app
```

- [ ] **Step 2.4: Pass `SegmentStats` to analysis calls**

In the `spawn_analysis_loop()` (~line 1424), capture `adaptive_trigger_state` and build stats before each `analyze()` call:

```rust
// Before the analyze() call:
let stats = adaptive_trigger_state.as_ref().and_then(|ts| build_segment_stats(ts));
let result = if force_full {
    last_full = std::time::Instant::now();
    analyzer.analyze(stats.as_ref()).await
} else {
    analyzer.analyze_if_changed(stats.as_ref()).await
};
```

Note: `adaptive_trigger_state` may not be accessible from the analysis loop. If not, pass the required data through a shared `Arc` or add the state to the `Scheduler` struct. This step may need architectural adjustment depending on what data is available in the analysis loop scope.

```
cargo check -p oneshim-app
```

- [ ] **Step 2.5: Write test**

In `analyzer.rs` tests, add:

```rust
#[tokio::test]
async fn analyze_with_segment_stats_includes_current_segment() {
    // Build ContextAnalyzer with mock provider
    // Call analyze(Some(&stats)) with a populated SegmentStats
    // Verify the resulting JSON contains "current_segment" key
}
```

```
cargo test -p oneshim-analysis -- analyzer::tests::analyze_with_segment
```

- [ ] **Step 2.6: Commit**

```
git add crates/oneshim-analysis/src/analyzer.rs src-tauri/src/scheduler/loops.rs
git commit -m "feat(analysis): wire SegmentStats into ContextAnalyzer for LLM segment context"
```

---

## Verification

```bash
cargo check --workspace
cargo test -p oneshim-analysis
cargo test -p oneshim-app --bin oneshim
cargo clippy --workspace -- -D warnings
```

---

## Execution Order

```
This plan (wiring gaps)     →  gui-phase3-llm-context-advanced.md (Task 1-4)
  Task 1: gui_patterns field     Task 1: structured GUI section in ContextPayload
  Task 2: SegmentStats wiring    Task 2: app-specific overrides
                                 Task 3: R-tree spatial index
                                 Task 4: dashboard heatmap
```

Task 1 of this plan is a prerequisite for the Phase 3 plan's Task 1 (GUI section). Task 2 of this plan ensures the `current_segment` data (including GUI) reaches the LLM at all.
