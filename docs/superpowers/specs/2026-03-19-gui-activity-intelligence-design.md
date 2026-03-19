# GUI Activity Intelligence — Design Spec

> Created: 2026-03-19
> Status: Implemented (Phase 1-3)
> Depends on: OcrRegion extraction (parallel), Standalone LLM Analysis Pipeline (ADR-011)

## 1. Goal

Transform raw OCR bounding-box data and mouse/keyboard input events into
semantic GUI interaction events, then aggregate those events into structured
activity summaries. The system should upgrade content tracking from
"user used VSCode for 45 min" to "user edited verify_token() function in
auth.rs, clicked Save 3 times, ran `cargo test` in terminal panel."

This is a pure-algorithm layer — no new ports, no I/O. It consumes data
already collected by `oneshim-vision` (OCR), `oneshim-monitor` (input), and
the scheduler event stream, and produces enriched `ContentActivity` records
that flow into the existing `SegmentSummary` and `ContextAssembler` pipelines.

## 2. Data Flow

```
Screen capture → OCR with bounding boxes → Vec<OcrRegion>
                                                │
Mouse events → (x, y, click_type, timestamp)    │
Keyboard events → (keystrokes, timestamp)       │
                                                │
                    ┌───────────────────────────┐│
                    │ InputOcrCorrelator        ││
                    │ (Phase 1, parallel work)  ││
                    │                           ││
                    │ mouse (x,y) + OcrRegion[] ─┘
                    │ → nearest region match    │
                    │ → GuiInteractionEvent     │
                    └────────────┬──────────────┘
                                 │
                    ┌────────────▼──────────────┐
                    │ ElementTypeInferencer      │
                    │                           │
                    │ text + bbox + position     │
                    │ → GuiElementType           │
                    │   (Button, TextInput,      │
                    │    MenuItem, Tab, Link,     │
                    │    ToolbarIcon, etc.)       │
                    └────────────┬──────────────┘
                                 │
                    ┌────────────▼──────────────┐
                    │ GuiActivityAggregator      │
                    │                           │
                    │ stream of interaction      │
                    │ events per time window     │
                    │ → GuiActivitySummary       │
                    │                           │
                    │ "edited auth.rs: 15 min    │
                    │  coding, 3 saves, 2 test   │
                    │  runs"                     │
                    └────────────┬──────────────┘
                                 │
                    Feeds into existing pipelines:
                    • ContentActivity.gui_details
                    • ContextAssembler (LLM context)
                    • WorkTypeClassifier (refined)
```

## 3. Existing Foundation

### Already implemented (oneshim-core)

| Type | Location | Relevance |
|------|----------|-----------|
| `OcrRegion` | `models/frame.rs` | Text + BoundingBox + confidence |
| `BoundingBox` | `models/frame.rs` | `contains_point()`, `center()`, `area()` |
| `MouseActivity` | `models/event.rs` | `click_count`, `last_position`, `double_click_count`, `right_click_count` |
| `KeyboardActivity` | `models/event.rs` | `keystrokes_per_min`, `shortcut_count`, `correction_count` |
| `InputActivityEvent` | `models/event.rs` | Per-period aggregated input with `app_name` |
| `ContentActivity` | `tiered_memory/content.rs` | Content label + work type + engagement metrics |
| `WorkType` | `tiered_memory/content.rs` | ActiveCoding, CodeReview, Writing, Reading, etc. |

### Already implemented (oneshim-analysis)

| Type | Location | Relevance |
|------|----------|-----------|
| `TitleBarParser` | `title_bar_parser.rs` | Window title → content label + type |
| `WorkTypeClassifier` | `work_type_classifier.rs` | Input patterns → WorkType + EngagementMetrics |
| `ContentTracker` | `content_tracker.rs` | Accumulates ContentActivity over time |
| `ContextAssembler` | `assembler.rs` | Builds LLM analysis context with PII filter |
| `SegmentSummarizer` | `segment_summarizer.rs` | Produces SegmentSummary from events + content |

### Already implemented (oneshim-core, automation)

| Type | Location | Relevance |
|------|----------|-----------|
| `GuiInteractionSession` | `models/gui.rs` | Automation GUI session (distinct from this spec) |
| `UiSceneElement` | `models/ui_scene.rs` | Scene element with bbox, label, role, confidence |

### Gap

The existing system knows *what app* is in focus and *what content label*
(from window title), but not *what GUI element* the user interacted with.
Mouse click coordinates exist in `MouseActivity` but are not correlated with
on-screen text. OCR runs but produces flat text — the new `OcrRegion` with
bounding boxes (parallel work) bridges this gap.

## 4. Components

### 4.1 InputOcrCorrelator (Phase 1) / GuiElementDetector (Phase 2)

> **Implementation note:** The Phase 1 implementation is named
> `InputOcrCorrelator` (in `oneshim-vision`). It will be expanded and
> renamed to `GuiElementDetector` in Phase 2 when additional capabilities
> (proximity fallback, screen resolution awareness) are added.

Correlates a mouse event position with the nearest OCR region to identify
which GUI element the user interacted with.

```
Phase 1 (InputOcrCorrelator):
Input:  click_position: (u32, u32)
        ocr_regions: &[OcrRegion]

Output: Option<GuiElement>

Phase 2 (GuiElementDetector) adds:
        screen_resolution: (u32, u32)   -- TODO Phase 2
```

**Algorithm:**
1. Find all `OcrRegion` whose `BoundingBox` contains the click point
   (use existing `BoundingBox::contains_point()`).
2. If multiple matches (overlapping regions), pick smallest by area
   (most specific element).
3. ~~If no direct hit, find nearest region within a 40px proximity threshold
   (buttons have padding beyond text bounds).~~ **Phase 2 TODO:** Proximity
   fallback (40px threshold) is not implemented in Phase 1. Phase 1 returns
   `None` when no bounding box directly contains the click point.
4. Return `None` if no region within threshold.

**Complexity:** O(n) scan over OCR regions per click. Typical frame has
50-200 regions — negligible cost. No spatial index needed for Phase 1.

### 4.2 ElementTypeInferencer

Heuristic classifier that infers the GUI element type from text content,
bounding box geometry, and screen position. No ML model — pure rules.

```
Input:  text: &str
        bbox: &BoundingBox
        screen_resolution: (u32, u32)
        click_type: ClickType  (Single, Double, Right)
        nearby_regions: &[OcrRegion]  (spatial context)

Output: GuiElementType
```

**Classification rules (ordered by priority):**

| Rule | Condition | Type |
|------|-----------|------|
| Menu bar | y < 30px (macOS) or in top 3% of screen | `MenuItem` |
| Tab bar | bbox.height 20-35px, horizontally aligned peers | `Tab` |
| Button | short text (1-3 words), bbox aspect ratio > 2:1, common labels (Save, OK, Cancel, Run, Build, Apply, Submit) | `Button` |
| Text input | right-clicked or double-clicked text, bbox.width > 200px | `TextInput` |
| Link | text contains "http" or starts with underline-style | `Link` |
| Toolbar icon | bbox near top, small area (< 1600px), icon-like dimensions | `ToolbarIcon` |
| Tree item | left-aligned, indented from siblings, file extension in text | `TreeItem` |
| Status bar | y > 95% screen height | `StatusBarItem` |
| Default | none of the above | `TextRegion` |

**App-specific overrides:**
- IDE (VSCode, IntelliJ, Xcode): file tabs at y~35-70px, terminal panel at bottom 30%
- Browser (Chrome, Safari, Firefox): tab bar, address bar, bookmark bar at known positions
- Slack/Teams: channel list on left, message input at bottom

### 4.3 GuiInteractionEvent

The output of correlating one input event with one OCR frame.

**Phase 1 (implemented) — flat interaction type:**
```
Fields:
  timestamp: DateTime<Utc>
  app_name: String
  element: GuiElement
    text: String           -- OCR text of the element
    element_type: GuiElementType
    bbox: BoundingBox
    confidence: f32        -- OCR confidence
  interaction: GuiInteractionType
    Click
    DoubleClick
    RightClick
    Type
    Hover
```

**Phase 2 additions:**
```
  window_title: String                    -- not in Phase 1
  screen_position: (u32, u32)             -- not in Phase 1
  interaction: InteractionType            -- replaces flat GuiInteractionType
    Click { button: ClickButton }
    KeyboardShortcut { keys: String }     -- e.g., "Cmd+S"
    TextEntry { char_count: u32, duration_ms: u64 }
    Scroll { direction: ScrollDirection, amount: u32 }
    DragDrop { from: (u32, u32), to: (u32, u32) }
```

> **Note:** Phase 1 uses a simpler flat `GuiInteractionType` enum
> (`Click, DoubleClick, RightClick, Type, Hover`) without structured
> payloads. Phase 2 expands this to `InteractionType` with per-variant
> data, and adds `window_title` and `screen_position` fields.

### 4.4 GuiActivityAggregator

Consumes a stream of `GuiInteractionEvent` and produces periodic
`GuiActivitySummary` records, one per content-label change or time window
(whichever comes first).

**Aggregation strategy:**
- Group events by `(app_name, window_title)` within the time window
- Count interactions per element type (button clicks, text entries, tab switches)
- Detect action sequences:
  - "Save" = button click on "Save" or Cmd+S shortcut
  - "Test run" = terminal activation + "cargo test" / "pytest" / "npm test" text
  - "Code navigation" = tree item clicks + tab switches
  - "Search" = Cmd+F shortcut or click on search input
- Compute typing vs clicking ratio per window

**Output: GuiActivitySummary**

```
Fields:
  app_name: String
  window_title: String
  content_label: String    -- from TitleBarParser
  start_time: DateTime<Utc>
  end_time: DateTime<Utc>
  duration_secs: u64

  -- Interaction counts
  button_clicks: u32
  text_entries: u32
  tab_switches: u32
  menu_accesses: u32
  tree_navigations: u32
  scroll_events: u32

  -- Detected semantic actions
  save_count: u32
  test_run_count: u32
  search_count: u32
  build_count: u32
  undo_redo_count: u32
  copy_paste_count: u32

  -- Top interacted elements (by frequency)
  top_elements: Vec<(String, GuiElementType, u32)>  -- (text, type, count)

  -- Element finder coverage metrics
  unmatched_click_count: u32  -- clicks with no element match (accessibility or OCR)

  -- Human-readable summary line
  summary_line: String
  -- e.g., "edited auth.rs: 15 min coding, 3 saves, 2 test runs"
```

### 4.5 Summary Line Generator

Produces the human-readable `summary_line` from aggregated counts. Template:

```
"{verb} {content_label}: {duration} {work_type}, {actions}"

Examples:
  "edited auth.rs: 15 min coding, 3 saves, 2 test runs"
  "reviewed pull request #42: 8 min reading, 12 comments"
  "browsed Stack Overflow: 5 min, 3 tab switches, 2 copy-pastes"
  "wrote README.md: 22 min writing, 5 saves"
  "debugged main.rs: 10 min, 4 breakpoint toggles, 6 test runs"
```

Verb selection: based on `WorkType` + dominant interaction type.
Action list: top 2-3 most frequent semantic actions, omit if count is 0.

## 5. Integration Points

### 5.1 ContentActivity enrichment

`ContentTracker` currently produces `ContentActivity` with `content_label`,
`work_type`, and `engagement`. The aggregator adds a new optional field:

```
ContentActivity {
    ...existing fields...
    gui_summary: Option<GuiActivitySummary>,
}
```

When `gui_summary` is `Some`, the `SegmentSummarizer` uses its `summary_line`
instead of generating a generic description. The `content_label` may also be
refined — if title bar says "VSCode" but GUI activity shows the user was in
the terminal panel, the effective content shifts.

### 5.2 ContextAssembler (LLM context)

`ContextAssembler` currently builds `ContentSummaryEntry` with generic
`content`/`content_type`/`work_type` strings. With GUI intelligence:

```
Before: { content: "auth.rs", content_type: "File", work_type: "ActiveCoding", mins: 15 }
After:  { content: "auth.rs — edited verify_token(), 3 saves, 2 test runs",
           content_type: "File", work_type: "ActiveCoding", mins: 15 }
```

The `content` field becomes the `summary_line`, giving the LLM dramatically
more context for suggestion generation.

### 5.3 WorkTypeClassifier refinement

**Approach: Post-hoc refinement, not a new input channel.**

`WorkTypeClassifier` remains unchanged. A new `GuiWorkTypeRefiner` wraps it
and applies GUI-based corrections after the initial classification. This
keeps the existing classifier stable and testable in isolation.

**Refinement rules:**

| Initial classification | GUI signal | Refined result |
|------------------------|-----------|----------------|
| `ActiveCoding` | frequent Save clicks | confirmed `ActiveCoding` |
| `Unknown` | heavy form interaction (TextInput clicks, Type events) | `FormFilling` |
| `Reading` | scroll-only, no clicks | confirmed `Reading` |
| `ActiveCoding` | typing in terminal area (bottom 30%) | may refine to `DevOps` |
| `Browsing` | text input interactions dominate | refine to `FormFilling` |
| `Reading` | frequent clicking on UI elements | refine to `Navigation` |

**Signal table (preserved from original):**

| Signal | Current inference | With GUI intelligence |
|--------|-------------------|----------------------|
| High keystrokes in IDE | ActiveCoding | Confirmed: typing in editor area |
| High keystrokes in IDE | ActiveCoding | Corrected: typing in terminal (could be DevOps) |
| Low keystrokes, scrolling | Reading | Confirmed: scrolling through code |
| Low keystrokes, clicking | Reading | Corrected: clicking through UI = Navigation |
| Mixed input in browser | Browsing | Refined: FormFilling (text inputs) vs Reading (scrolling) |

### 5.4 SegmentSummary patterns

`PatternMiner` currently detects activity patterns from raw events. GUI
interactions unlock new pattern types:

- `FrequentSave` — save count > 5 in 10 minutes (anxiety pattern)
- `TestDrivenDevelopment` — alternating code edit + test run cycles
- `CodeReviewFlow` — file navigation + inline comment typing
- `DebuggingLoop` — run + inspect + edit cycles
- `ReferenceHopping` — frequent tab switches between doc and code

## 6. Privacy

### 6.1 PII filtering

All OCR text in `GuiInteractionEvent` and `GuiActivitySummary` passes through
the existing `PiiFilter` (from `ContextAssembler`) before:
- Storage in SQLite
- Inclusion in LLM context
- Display in web dashboard

The filter already handles email addresses, phone numbers, API keys, IPs,
credit card numbers, SSNs, and file paths (see `oneshim-vision/src/privacy.rs`).
GUI element text is treated identically to OCR text — same filter, same level.

> **Implementation note:** `InputOcrCorrelator` should apply the PII filter
> to `GuiElement.text` at creation time (when constructing the
> `GuiInteractionEvent`), not deferred to storage or display. This ensures
> PII-filtered text is the only representation that ever exists in memory
> beyond the correlator.

### 6.2 Coordinate sensitivity

Mouse coordinates alone are not PII. However, coordinates correlated with
OCR text can reveal:
- Password field locations (mitigated: sensitive app detection already exists)
- Form field contents (mitigated: PII filter on OCR text)
- Financial data positions (mitigated: PII filter catches numbers)

No additional mitigation needed beyond existing PII filter pipeline.

### 6.3 Consent model

GUI Activity Intelligence reuses existing consent permissions:
- `screen_capture` — already required for OCR
- `input_activity` — already required for mouse/keyboard

No new consent permission needed. If either is revoked, the corresponding
data stream stops and GUI correlation produces no events.

### 6.4 Storage retention

`GuiInteractionEvent` raw events follow the same 30-day / 500MB retention
policy as frames and raw events. `GuiActivitySummary` records are stored
alongside `ContentActivity` in the segment store — same lifecycle.

## 7. Crate Placement

| Component | Crate | Rationale |
|-----------|-------|-----------|
| `GuiInteractionEvent`, `GuiActivitySummary`, `GuiElementType` | `oneshim-core/src/models/gui_interaction.rs` | Domain models (being created in parallel) |
| `InputOcrCorrelator` (Phase 1) / `GuiElementDetector` (Phase 2) | `oneshim-vision/src/gui_detector.rs` | OCR region correlation is vision-layer logic |
| `ElementTypeInferencer` | `oneshim-vision/src/element_inferencer.rs` | Heuristic classification from visual features |
| `GuiActivityAggregator` | `oneshim-analysis/src/gui_aggregator.rs` | Temporal aggregation is analysis-layer logic |
| Summary line generation | `oneshim-analysis/src/gui_aggregator.rs` | Part of aggregation |
| Scheduler wiring | `src-tauri/src/scheduler/loops.rs` | Existing event processing loops |

This placement follows the hexagonal architecture: vision crate handles
spatial correlation (adapter-level), analysis crate handles temporal
aggregation (adapter-level), core crate holds the domain models.

No new ports are introduced. `GuiElementDetector` and `GuiActivityAggregator`
are concrete structs (like `PatternMiner` and `ContentTracker`), not port
traits. They are pure-algorithm components with no I/O.

## 8. Monitor Loop Decomposition

The GUI pipeline should be self-contained, following the pattern established
by `analysis_pipeline::tick()`.

**Recommendation:**

- Introduce `gui_pipeline::tick()` as the single entry point for all GUI
  intelligence processing per scheduler cycle.
- The monitor loop calls `gui_pipeline::tick()` alongside
  `analysis_pipeline::tick()` — both are self-contained.
- `gui_pipeline::tick()` internally orchestrates:
  1. `InputOcrCorrelator` — correlate pending input events with latest OCR frame
  2. `ElementTypeInferencer` — classify matched elements
  3. `GuiActivityAggregator` — accumulate into current time window
  4. `GuiWorkTypeRefiner` — post-hoc refinement of WorkType (see §5.3)

**Configuration:**

Anticipate a `GuiIntelligenceConfig` sub-section within `AnalysisConfig`:

```
AnalysisConfig {
    ...existing fields...
    gui_intelligence: GuiIntelligenceConfig {
        enabled: bool,              // default: true
        aggregation_window_secs: u64, // default: 60
        max_events_per_segment: u32,  // default: 1000
    }
}
```

This keeps GUI intelligence toggleable without affecting other analysis
pipelines.

## 9. Data Model Details

### 9.1 GuiElementType enum

**Phase 1 (implemented):**
```
Button          -- clickable action trigger
TextInput       -- editable text field
Link            -- hyperlink
MenuItem        -- menu bar or context menu entry
TabLabel        -- tab bar item
StatusBar       -- status bar element
TitleBar        -- title bar element
Unknown         -- none of the above (fallback)
```

**Phase 2 additions:**
```
ToolbarIcon     -- toolbar button/icon
TreeItem        -- file tree or outline tree node
ScrollBar       -- scroll interaction target
TextRegion      -- general text (code, document content)
```

> **Note:** Phase 1 uses `TabLabel`/`StatusBar`/`TitleBar`/`Unknown` instead
> of the original spec's `Tab`/`StatusBarItem`/`TextRegion`. The Phase 2
> variants will be added when app-specific overrides and richer heuristics
> are implemented.

### 9.2 ClickType enum

```
Single
Double
Right
```

### 9.3 GuiInteractionType (Phase 1) / InteractionType (Phase 2)

**Phase 1 (implemented) — flat enum, no payloads:**
```
Click
DoubleClick
RightClick
Type
Hover
```

**Phase 2 — structured enum with per-variant data:**
```
Click { button: ClickType }
KeyboardShortcut { keys: String }
TextEntry { char_count: u32, duration_ms: u64 }
Scroll { direction: ScrollDirection, amount: u32 }
DragDrop { from: (u32, u32), to: (u32, u32) }
```

## 10. Phased Implementation

### Phase 1: Foundation (parallel work, in progress)

- `OcrRegion` extraction with bounding boxes from OCR engine
- `InputOcrCorrelator`: match click positions to OCR regions + element type
  inference (combined in single component for Phase 1)
- Flat `GuiInteractionType` enum (`Click, DoubleClick, RightClick, Type, Hover`)
- Phase 1 element types: `Button, TextInput, Link, MenuItem, TabLabel, StatusBar, TitleBar, Unknown`
- PII filter applied to `GuiElement.text` at creation time
- Basic `GuiInteractionEvent` emission (no `window_title` or `screen_position`)
- Unit tests: synthetic OCR regions + click positions

**Deliverable:** Stream of `GuiInteractionEvent` per frame capture cycle.

### Phase 2: Aggregation + Integration

- Rename `InputOcrCorrelator` → `GuiElementDetector` with expanded API:
  - Add `screen_resolution` parameter
  - Add 40px proximity fallback for near-miss clicks
  - Add element types: `ToolbarIcon, TreeItem, ScrollBar, TextRegion`
- Upgrade `GuiInteractionType` → structured `InteractionType` with payloads
- Add `window_title` and `screen_position` fields to `GuiInteractionEvent`
- `GuiActivityAggregator` with time-window grouping
- `GuiActivitySummary` production with semantic action detection
- `GuiWorkTypeRefiner`: post-hoc WorkType refinement using GUI signals (see §5.3)
- `ContentTracker` integration: `gui_summary` field on `ContentActivity`
- `ContextAssembler` enrichment: summary lines in LLM context
- `gui_pipeline::tick()` as self-contained entry point (see §8)
- `GuiIntelligenceConfig` sub-section in `AnalysisConfig`
- SQLite storage: `gui_interactions` table (event log) + summary in segments
- **Accessibility API adapters** (macOS AXUIElement, Windows UIAutomation):
  - Implement `ElementFinder` trait for each platform (`#[cfg(target_os)]` gated)
  - `ChainedElementFinder` pattern: Accessibility primary, OCR fallback
  - `AXUIElementCopyElementAtPosition(x, y)` (macOS) / `ElementFromPoint(x, y)` (Windows)
  - Returns: element role, title, frame rect, enabled/focused state
  - Track `unmatched_click_count` in `GuiActivitySummary` for coverage metrics

**Deliverable:** Enriched `SegmentSummary` with GUI-level detail flowing
into LLM analysis pipeline. Accessibility API provides higher-fidelity
element identification where OS permissions allow, with OCR as universal fallback.

### Phase 3: LLM Context + Advanced Patterns

- `PatternMiner` extension: TDD detection, debugging loops, review flows
- `ContextAssembler` structured GUI section in LLM prompt
- App-specific element type overrides (IDE, browser, Slack)
- Dashboard visualization: interaction heatmap overlay on timeline
- Performance: spatial index (R-tree) if OCR region count exceeds 500/frame

**Deliverable:** LLM suggestions that reference specific GUI actions.

## 11. Performance Budget

| Operation | Budget | Approach |
|-----------|--------|----------|
| Element detection per click | < 1ms | Linear scan over 50-200 OCR regions |
| Element type inference | < 0.5ms | Rule cascade, no allocation |
| Aggregation per window | < 2ms | HashMap accumulation |
| Summary line generation | < 0.1ms | String formatting |
| Memory per active window | < 4KB | Bounded event buffer (last 100 interactions) |
| Storage per hour | < 200KB | ~500 interaction events at ~400 bytes each |

The entire pipeline runs synchronously within the existing scheduler
`monitor_loop` tick. No additional async tasks or threads needed.

## 12. Testing Strategy

### Unit tests (per component)

- `GuiElementDetector`: synthetic OcrRegion grid + click at known positions
- `ElementTypeInferencer`: known text/bbox/position combinations
- `GuiActivityAggregator`: event stream → expected summary counts
- Summary line generator: count combinations → expected text

### Integration tests (cross-component)

- Full pipeline: OcrRegions + InputActivityEvent → GuiActivitySummary
- ContentTracker with GUI enrichment: verify summary_line propagation
- ContextAssembler with GUI data: verify enriched content field

### Property tests

- Any click within a BoundingBox must match that region
- Aggregator event count must equal sum of per-type counts
- Summary line must contain content_label

## 13. Open Questions

| Question | Decision |
|----------|----------|
| Should we track mouse hover (not just clicks)? | **Decided:** No for Phase 1-3 — hover events add noise without proportional insight; revisit if future analytics require dwell-time heatmaps |
| OCR frame rate vs input event rate mismatch? | **Decided:** Use nearest frame within 2s window — interpolation adds complexity with marginal accuracy gain |
| Should element type inference learn from user corrections? | **Decided:** Static rules for Phase 1-3 — feedback loops require UI surface and storage that are out of scope |
| Maximum interaction events per segment? | **Decided:** 1000 with oldest-eviction — sufficient for 30min segments at observed interaction rates |

## 14. Non-Goals

- **Pixel-level UI element detection** — computer vision models (YOLO, etc.) add ~100MB model weight. OCR + heuristics is sufficient for text-based UIs.
- **Keystroke logging** — we track aggregate counts and shortcuts, never individual key sequences (privacy).
- **Automation** — this spec is read-only observation. The existing `oneshim-automation` crate handles GUI execution. No overlap.

> **Note:** Accessibility tree parsing (macOS AX, Windows UIA) was originally
> listed as a non-goal. It has been promoted to Phase 2 scope following the
> OCR deep review, which found that the existing architecture already has
> `FinderSource::Accessibility`, `ChainedElementFinder`, and the `ElementFinder`
> port. The `ChainedElementFinder` pattern (Accessibility primary, OCR fallback)
> provides higher-fidelity element identification where OS permissions allow.
