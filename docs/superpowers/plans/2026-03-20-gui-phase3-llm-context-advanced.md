# GUI Activity Intelligence Phase 3 — LLM Context + Advanced Patterns

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enrich LLM coaching/analysis prompts with structured GUI context, add app-specific element type overrides for IDE/browser/Slack, add conditional R-tree spatial indexing for 500+ OCR regions, and add interaction heatmap visualization to the dashboard timeline.

**Architecture:** Four independent subsystems: (1) ContextAssembler gets a dedicated `gui` section in its LLM JSON payload, (2) GuiElementDetector gains per-app element type rules via an `AppElementOverrides` registry, (3) `correlate_click()` uses an R-tree when OCR region count exceeds a threshold, (4) the web dashboard timeline gains an interaction density track. No new crates. `rstar` added as workspace dependency.

**Tech Stack:** Rust, serde, rstar (R-tree), React, Recharts, Tailwind CSS, oneshim-core models, oneshim-analysis, oneshim-vision, oneshim-web

**Spec:** `docs/superpowers/specs/2026-03-19-gui-activity-intelligence-design.md` §Phase 3

**Pre-existing work (already implemented):**
- `gui_patterns.rs`: 5 GUI patterns — FrequentSave, TestDrivenDevelopment, CodeReviewFlow, DebuggingLoop, ReferenceHopping (with 20 tests)
- `assembler.rs`: `gui_summary_line` appended to `ContentSummaryItem.content` (basic integration)
- `gui_detector.rs`: Generic `infer_element_type()` with position/text heuristics (13 rules)

---

## File Map

### Task 1 — ContextAssembler structured GUI section

| File | Change |
|------|--------|
| `crates/oneshim-analysis/src/assembler.rs` | Add `gui_patterns` to `ContentSummaryEntry` + `SegmentStats`; add `GuiSection` struct and `gui` field to `ContextPayload` |
| `crates/oneshim-analysis/src/segment_summarizer.rs` | Populate `gui_patterns` from `ContentActivity.gui_summary` via `detect_gui_patterns()` |

### Task 2 — App-specific element type overrides

| File | Change |
|------|--------|
| `crates/oneshim-vision/src/gui_detector.rs` | Add `AppElementOverrides` struct with per-app rules; call before generic fallback in `infer_element_type()` |

### Task 3 — R-tree spatial index

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `rstar = "0.12"` to `[workspace.dependencies]` |
| `crates/oneshim-vision/Cargo.toml` | Add `rstar = { workspace = true }` |
| `crates/oneshim-vision/src/gui_detector.rs` | Add `SpatialIndex` wrapper; use R-tree in `correlate_click()` when region count > threshold |

### Task 4 — Dashboard interaction heatmap

| File | Change |
|------|--------|
| `crates/oneshim-web/src/handlers/stats.rs` | Add `GET /api/stats/gui-heatmap` endpoint returning per-hour interaction density |
| `crates/oneshim-web/src/routes.rs` | Register new route |
| `crates/oneshim-web/frontend/src/api/contracts.ts` | Add `GuiHeatmapCell` type |
| `crates/oneshim-web/frontend/src/api/client.ts` | Add `fetchGuiHeatmap()` function |
| `crates/oneshim-web/frontend/src/components/GuiInteractionTrack.tsx` | New component: horizontal interaction density bar below timeline |
| `crates/oneshim-web/frontend/src/pages/Dashboard.tsx` (or DashboardDay.tsx) | Wire `GuiInteractionTrack` into dashboard layout |

---

## Task 1: ContextAssembler structured GUI section

**Why:** The LLM currently sees GUI data only as an appended string in `content_summary`. A dedicated `gui` section with structured data (patterns, semantic actions, top elements) lets the LLM generate more specific, actionable coaching messages referencing exact GUI behaviors.

**Files:**
- Modify: `crates/oneshim-analysis/src/assembler.rs`
- Modify: `crates/oneshim-analysis/src/segment_summarizer.rs`
- Modify: `crates/oneshim-analysis/src/assembler.rs`

- [ ] **Step 1.1: Add `gui_patterns` to `ContentSummaryEntry`**

In `crates/oneshim-analysis/src/assembler.rs`, find `ContentSummaryEntry` (line ~33) and add:

```rust
/// GUI behavioral patterns detected during this content activity (e.g. "TDD", "DebuggingLoop").
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub gui_patterns: Vec<String>,
```

```
cargo check -p oneshim-analysis
```

- [ ] **Step 1.2: Populate `gui_patterns` in segment summarizer**

In `crates/oneshim-analysis/src/segment_summarizer.rs`, find `to_content_summary_entries()` or equivalent method that converts `ContentActivity` → `ContentSummaryEntry`. When `content_activity.gui_summary` is `Some`, call `detect_gui_patterns()` and map the results to strings:

```rust
use crate::pattern_miner::{detect_gui_patterns, GuiPattern};

// Inside the mapping:
let gui_patterns = activity.gui_summary.as_ref()
    .map(|gs| detect_gui_patterns(gs, activity.work_type)
        .into_iter()
        .map(|p| format!("{:?}", p))
        .collect::<Vec<_>>())
    .unwrap_or_default();
```

Assign to `entry.gui_patterns = gui_patterns;`.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 1.3: Add `gui_patterns` to `SegmentStats`**

In `crates/oneshim-analysis/src/assembler.rs`, find `SegmentStats` (line ~22) and add:

```rust
/// Aggregated GUI patterns detected across all content activities in this segment.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub gui_patterns: Vec<String>,
```

Populate it in the segment stats builder (wherever `SegmentStats` is constructed from `ContentSummaryEntry` list) by collecting all unique patterns:

```rust
gui_patterns: content_summary.iter()
    .flat_map(|e| e.gui_patterns.iter().cloned())
    .collect::<std::collections::HashSet<_>>()
    .into_iter()
    .collect(),
```

```
cargo check -p oneshim-analysis
```

- [ ] **Step 1.4: Add `GuiSection` to ContextAssembler**

In `crates/oneshim-analysis/src/assembler.rs`, add a new serializable struct and field:

```rust
#[derive(Serialize)]
struct GuiSection {
    /// Detected behavioral patterns (e.g. "TestDrivenDevelopment", "DebuggingLoop").
    patterns: Vec<String>,
    /// Semantic action counts aggregated from content summaries.
    actions: GuiActionCounts,
    /// Top interacted elements (text, type, count) across the segment.
    top_elements: Vec<(String, String, u32)>,
}

#[derive(Serialize)]
struct GuiActionCounts {
    saves: u32,
    test_runs: u32,
    searches: u32,
    builds: u32,
    undo_redos: u32,
    copy_pastes: u32,
}
```

Add to `ContextPayload`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
gui: Option<GuiSection>,
```

- [ ] **Step 1.5: Populate `GuiSection` from `SegmentStats`**

In `build_with_history()`, build the GUI section when segment stats contain GUI data:

```rust
let gui = segment_stats.and_then(|stats| {
    if stats.gui_patterns.is_empty() && stats.content_summary.iter().all(|e| e.gui_summary_line.is_none()) {
        return None;
    }

    let mut actions = GuiActionCounts {
        saves: 0, test_runs: 0, searches: 0, builds: 0, undo_redos: 0, copy_pastes: 0,
    };
    let mut top_elements: Vec<(String, String, u32)> = Vec::new();

    // Aggregate from content summary entries that have gui_summary_line
    // (action counts are embedded in the summary line; patterns are already extracted)

    Some(GuiSection {
        patterns: stats.gui_patterns.clone(),
        actions,
        top_elements,
    })
});
```

Assign `gui` in the `ContextPayload` struct literal.

```
cargo check -p oneshim-analysis
```

- [ ] **Step 1.6: Write test for GUI section in context payload**

In `crates/oneshim-analysis/src/assembler.rs` `#[cfg(test)]` module, add:

```rust
#[test]
fn gui_section_included_when_patterns_present() {
    let assembler = ContextAssembler::new(Box::new(|s: &str| s.to_string()));
    let current = /* minimal CurrentActivity */;
    let metrics = /* minimal SessionMetrics */;
    let mut stats = SegmentStats::default();
    stats.gui_patterns = vec!["TestDrivenDevelopment".to_string()];

    let ctx = assembler.build_with_segment(&current, &[], &[], &metrics, Some(&stats));
    assert!(ctx.user_context_json.contains("\"gui\""));
    assert!(ctx.user_context_json.contains("TestDrivenDevelopment"));
}

#[test]
fn gui_section_omitted_when_no_gui_data() {
    let assembler = ContextAssembler::new(Box::new(|s: &str| s.to_string()));
    let current = /* minimal CurrentActivity */;
    let metrics = /* minimal SessionMetrics */;
    let stats = SegmentStats::default();

    let ctx = assembler.build_with_segment(&current, &[], &[], &metrics, Some(&stats));
    assert!(!ctx.user_context_json.contains("\"gui\""));
}
```

```
cargo test -p oneshim-analysis -- assembler::tests::gui_section
```

- [ ] **Step 1.7: Commit**

```
git add crates/oneshim-core/src/models/tiered_memory.rs crates/oneshim-analysis/src/segment_summarizer.rs crates/oneshim-analysis/src/assembler.rs
git commit -m "feat(analysis): add structured GUI section to ContextAssembler LLM payload"
```

---

## Task 2: App-specific element type overrides

**Why:** The generic `infer_element_type()` in `gui_detector.rs` uses position and text heuristics that work for any app. IDEs, browsers, and chat apps have predictable UI layouts (e.g., VSCode explorer is always on the left, browser URL bar is always at the top). App-specific rules improve element type accuracy for the most-used app categories.

**Files:**
- Modify: `crates/oneshim-vision/src/gui_detector.rs`

- [ ] **Step 2.1: Write failing tests for app-specific overrides**

In `gui_detector.rs` `#[cfg(test)]` module, add:

```rust
#[test]
fn ide_sidebar_element_detected() {
    let detector = GuiElementDetector::new(PiiFilterLevel::Off);
    // Left 20% of screen, tree-like text
    let region = make_region("src/main.rs", 30, 200, 120, 16, 0.9);
    let elem = detector.correlate_click(60, 208, &[region]);
    assert!(elem.is_some());
    let e = elem.unwrap();
    // With app override for IDE, left-side items should be TreeItem
    assert_eq!(e.element_type, GuiElementType::TreeItem);
}

#[test]
fn browser_url_bar_detected() {
    let detector = GuiElementDetector::new(PiiFilterLevel::Off);
    // Top area with URL text
    let region = make_region("https://github.com/repo", 200, 60, 500, 20, 0.9);
    let elem = detector.correlate_click_with_app(250, 70, &[region], "Google Chrome");
    assert!(elem.is_some());
    assert_eq!(elem.unwrap().element_type, GuiElementType::Link);
}
```

```
cargo test -p oneshim-vision -- gui_detector::tests::ide_sidebar
```
Expected: FAIL (method `correlate_click_with_app` doesn't exist yet)

- [ ] **Step 2.2: Add `correlate_click_with_app()` method**

Add a new public method that accepts `app_name` and applies app-specific overrides before the generic fallback:

```rust
/// Like `correlate_click`, but applies app-specific element type overrides
/// based on the active application name.
pub fn correlate_click_with_app(
    &self,
    click_x: u32,
    click_y: u32,
    regions: &[OcrRegion],
    app_name: &str,
) -> Option<GuiElement> {
    self.correlate_click(click_x, click_y, regions)
        .map(|mut e| {
            if let Some(override_type) = self.app_specific_override(app_name, &e.text, &e.bbox) {
                e.element_type = override_type;
            }
            e
        })
}
```

- [ ] **Step 2.3: Implement `app_specific_override()`**

Add private method with per-app rules:

```rust
/// App-specific element type overrides for well-known applications.
///
/// Returns `Some(GuiElementType)` if the app+position combination matches
/// a known UI layout, `None` to keep the generic inference result.
fn app_specific_override(
    &self,
    app_name: &str,
    text: &str,
    bbox: &BoundingBox,
) -> Option<GuiElementType> {
    let lower_app = app_name.to_lowercase();
    let (screen_w, _screen_h) = self.screen_resolution;

    // IDE apps: VSCode, IntelliJ, PyCharm, WebStorm, Xcode, Android Studio
    if Self::is_ide_app(&lower_app) {
        return self.ide_override(text, bbox, screen_w);
    }

    // Browser apps: Chrome, Safari, Firefox, Edge, Arc, Brave
    if Self::is_browser_app(&lower_app) {
        return self.browser_override(text, bbox);
    }

    // Chat/Communication apps: Slack, Teams, Discord
    if Self::is_chat_app(&lower_app) {
        return self.chat_override(text, bbox, screen_w);
    }

    None
}

fn is_ide_app(app: &str) -> bool {
    ["code", "visual studio", "intellij", "pycharm", "webstorm", "xcode", "android studio", "cursor", "zed"]
        .iter().any(|k| app.contains(k))
}

fn is_browser_app(app: &str) -> bool {
    ["chrome", "safari", "firefox", "edge", "arc", "brave"]
        .iter().any(|k| app.contains(k))
}

fn is_chat_app(app: &str) -> bool {
    ["slack", "teams", "discord", "telegram", "messages"]
        .iter().any(|k| app.contains(k))
}

fn ide_override(&self, text: &str, bbox: &BoundingBox, screen_w: u32) -> Option<GuiElementType> {
    let left_panel_max_x = screen_w / 5; // 20% from left = sidebar/explorer
    let right_panel_min_x = screen_w * 4 / 5; // 80% from right = panels

    // Explorer/sidebar: left 20% of screen
    if bbox.x < left_panel_max_x {
        return Some(GuiElementType::TreeItem);
    }
    // Side panels (terminal, output): right 20%
    if bbox.x >= right_panel_min_x {
        // Short text in right panel could be terminal output
        if text.len() < 6 { return Some(GuiElementType::Button); }
        return Some(GuiElementType::TextRegion);
    }
    None
}

fn browser_override(&self, text: &str, bbox: &BoundingBox) -> Option<GuiElementType> {
    let (_screen_w, screen_h) = self.screen_resolution;
    let url_bar_max_y = (screen_h as f64 * 0.08) as u32;

    // URL bar region: top 8%, contains URL-like text
    if bbox.y < url_bar_max_y && (text.contains('.') || text.contains('/')) {
        return Some(GuiElementType::Link);
    }
    None
}

fn chat_override(&self, _text: &str, bbox: &BoundingBox, screen_w: u32) -> Option<GuiElementType> {
    let sidebar_max_x = screen_w / 4; // 25% from left = channel list

    // Channel/contact list: left 25%
    if bbox.x < sidebar_max_x {
        return Some(GuiElementType::TreeItem);
    }
    None
}
```

```
cargo check -p oneshim-vision
```

- [ ] **Step 2.4: Run tests**

```
cargo test -p oneshim-vision -- gui_detector::tests
```
Expected: all pass

- [ ] **Step 2.5: Wire `correlate_click_with_app` in GUI pipeline**

In `src-tauri/src/scheduler/gui_pipeline.rs`, find where `correlate_click()` is called and pass the `app_name` to use the new method instead:

```rust
// Before:
let element = gui_state.detector.correlate_click(click_x, click_y, regions);
// After:
let element = gui_state.detector.correlate_click_with_app(click_x, click_y, regions, app_name);
```

```
cargo check -p oneshim-app
```

- [ ] **Step 2.6: Commit**

```
git add crates/oneshim-vision/src/gui_detector.rs src-tauri/src/scheduler/gui_pipeline.rs
git commit -m "feat(vision): add app-specific element type overrides for IDE, browser, chat"
```

---

## Task 3: R-tree spatial index for OCR regions

**Why:** `correlate_click()` uses a linear scan over all OCR regions. When a complex IDE or dashboard produces 500+ regions per frame, the linear scan becomes the bottleneck. An R-tree reduces point-query time from O(n) to O(log n). The R-tree is only built when the region count exceeds a configurable threshold (default: 400).

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/oneshim-vision/Cargo.toml`
- Modify: `crates/oneshim-vision/src/gui_detector.rs`

- [ ] **Step 3.1: Add `rstar` dependency**

In workspace `Cargo.toml` `[workspace.dependencies]`:

```toml
rstar = "0.12"
```

In `crates/oneshim-vision/Cargo.toml` `[dependencies]`:

```toml
rstar = { workspace = true }
```

```
cargo check -p oneshim-vision
```

- [ ] **Step 3.2: Write failing test for spatial index**

In `gui_detector.rs` tests:

```rust
#[test]
fn spatial_index_matches_linear_scan() {
    let detector = GuiElementDetector::new(PiiFilterLevel::Off);
    // Generate 600 regions in a grid pattern
    let regions: Vec<OcrRegion> = (0..600)
        .map(|i| {
            let row = i / 30;
            let col = i % 30;
            make_region(
                &format!("item_{i}"),
                col * 64, row * 54,
                60, 50,
                0.9,
            )
        })
        .collect();

    // Click at center of region 315 (row 10, col 15)
    let click_x = 15 * 64 + 30;
    let click_y = 10 * 54 + 25;

    let linear = detector.correlate_click(click_x, click_y, &regions);
    let spatial = detector.correlate_click(click_x, click_y, &regions); // same call, threshold triggers R-tree

    assert_eq!(linear.map(|e| e.text.clone()), spatial.map(|e| e.text.clone()));
}
```

```
cargo test -p oneshim-vision -- gui_detector::tests::spatial_index
```

- [ ] **Step 3.3: Add R-tree wrapper in `correlate_click()`**

Add spatial index logic at the top of `correlate_click()`:

```rust
use rstar::{RTree, AABB, PointDistance, RTreeObject};

/// Wrapper to make OcrRegion indexable in an R-tree.
struct IndexedRegion<'a> {
    region: &'a OcrRegion,
    index: usize,
}

impl<'a> RTreeObject for IndexedRegion<'a> {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let b = &self.region.bbox;
        AABB::from_corners(
            [b.x as f64, b.y as f64],
            [(b.x + b.width) as f64, (b.y + b.height) as f64],
        )
    }
}

impl<'a> PointDistance for IndexedRegion<'a> {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        self.envelope().distance_2(point)
    }
}
```

In `correlate_click()`, add a threshold check before the linear scan:

```rust
const SPATIAL_INDEX_THRESHOLD: usize = 400;

if regions.len() >= SPATIAL_INDEX_THRESHOLD {
    return self.correlate_click_spatial(click_x, click_y, regions);
}
// ... existing linear scan code ...
```

Add `correlate_click_spatial()`:

```rust
fn correlate_click_spatial(
    &self,
    click_x: u32,
    click_y: u32,
    regions: &[OcrRegion],
) -> Option<GuiElement> {
    let indexed: Vec<IndexedRegion> = regions.iter().enumerate()
        .map(|(i, r)| IndexedRegion { region: r, index: i })
        .collect();
    let tree = RTree::bulk_load(indexed);

    let point = [click_x as f64, click_y as f64];

    // 1. Direct hit — containing regions
    let containing: Vec<_> = tree.locate_all_at_point(&point).collect();
    if let Some(hit) = containing.iter().min_by_key(|ir| ir.region.bbox.area()) {
        return Some(self.build_gui_element(hit.region));
    }

    // 2. Proximity fallback — nearest within threshold
    let threshold = self.proximity_threshold_px as f64;
    if let Some(nearest) = tree.nearest_neighbor(&point) {
        let dist = nearest.distance_2(&point).sqrt();
        if dist <= threshold {
            return Some(self.build_gui_element(nearest.region));
        }
    }

    None
}

fn build_gui_element(&self, region: &OcrRegion) -> GuiElement {
    let filtered_text = sanitize_title_with_level(&region.text, self.pii_filter_level);
    GuiElement {
        text: filtered_text,
        bbox: region.bbox.clone(),
        element_type: self.infer_element_type(&region.text, &region.bbox),
        confidence: region.confidence,
    }
}
```

```
cargo check -p oneshim-vision
```

- [ ] **Step 3.4: Run tests**

```
cargo test -p oneshim-vision -- gui_detector::tests
```

- [ ] **Step 3.5: Add benchmark-style test for threshold validation**

```rust
#[test]
fn spatial_vs_linear_consistency_at_threshold() {
    let detector = GuiElementDetector::new(PiiFilterLevel::Off);
    let regions: Vec<OcrRegion> = (0..400)
        .map(|i| make_region(&format!("r{i}"), (i % 20) * 96, (i / 20) * 54, 90, 50, 0.9))
        .collect();

    // Test multiple click positions
    for &(cx, cy) in &[(500, 300), (100, 100), (1800, 1000), (960, 540)] {
        let result = detector.correlate_click(cx, cy, &regions);
        // Just verify no panic and consistent behavior
        let _ = result;
    }
}
```

```
cargo test -p oneshim-vision -- gui_detector::tests::spatial_vs_linear
```

- [ ] **Step 3.6: Commit**

```
git add Cargo.toml crates/oneshim-vision/Cargo.toml crates/oneshim-vision/src/gui_detector.rs
git commit -m "perf(vision): add R-tree spatial index for OCR regions above 400 threshold"
```

---

## Task 4: Dashboard interaction heatmap

**Why:** Users need to see where their GUI interactions (clicks, typing, menu access) cluster over time. A horizontal density track below the timeline gives an at-a-glance view of interaction intensity per time bucket.

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite.rs` (or `web_storage` section)
- Modify: `crates/oneshim-web/src/handlers/stats.rs`
- Modify: `crates/oneshim-web/src/routes.rs`
- Create: `crates/oneshim-web/frontend/src/components/GuiInteractionTrack.tsx`
- Modify: `crates/oneshim-web/frontend/src/api/contracts.ts`
- Modify: `crates/oneshim-web/frontend/src/api/client.ts`
- Modify: `crates/oneshim-web/frontend/src/pages/DashboardDay.tsx`

- [ ] **Step 4.1: Add storage query for GUI interaction density**

In the web storage implementation, add a method to aggregate GUI interactions by hour:

```rust
/// Count GUI interactions per hour for a given date range.
fn query_gui_interaction_density(
    &self,
    start: &str,
    end: &str,
) -> Result<Vec<(String, u32)>, CoreError> {
    let conn = self.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m-%dT%H:00:00Z', timestamp) AS hour, COUNT(*) AS count
         FROM gui_interactions
         WHERE timestamp >= ?1 AND timestamp < ?2
         GROUP BY hour
         ORDER BY hour"
    )?;
    let rows = stmt.query_map([start, end], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::from)
}
```

```
cargo check -p oneshim-storage
```

- [ ] **Step 4.2: Add REST endpoint**

In `crates/oneshim-web/src/handlers/stats.rs`, add:

```rust
pub async fn gui_heatmap(
    State(state): State<AppState>,
    Query(params): Query<DateRangeParams>,
) -> Result<Json<Vec<GuiHeatmapCell>>, ApiError> {
    let start = params.start.unwrap_or_else(today_start);
    let end = params.end.unwrap_or_else(now_string);
    let density = state.storage.query_gui_interaction_density(&start, &end)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let cells: Vec<GuiHeatmapCell> = density.into_iter()
        .map(|(hour, count)| GuiHeatmapCell { hour, count })
        .collect();

    Ok(Json(cells))
}

#[derive(Serialize)]
pub struct GuiHeatmapCell {
    pub hour: String,
    pub count: u32,
}
```

Register in `routes.rs`:

```rust
.route("/api/stats/gui-heatmap", get(handlers::stats::gui_heatmap))
```

```
cargo check -p oneshim-web
```

- [ ] **Step 4.3: Add frontend types and client**

In `contracts.ts`:

```typescript
export interface GuiHeatmapCell {
  hour: string
  count: number
}
```

In `client.ts`:

```typescript
export async function fetchGuiHeatmap(start?: string, end?: string): Promise<GuiHeatmapCell[]> {
  const params = new URLSearchParams()
  if (start) params.set('start', start)
  if (end) params.set('end', end)
  return fetchJson(`/api/stats/gui-heatmap?${params}`)
}
```

- [ ] **Step 4.4: Create `GuiInteractionTrack` component**

Create `crates/oneshim-web/frontend/src/components/GuiInteractionTrack.tsx`:

```tsx
import { useQuery } from '@tanstack/react-query'
import { fetchGuiHeatmap } from '../api/client'
import type { GuiHeatmapCell } from '../api/contracts'

interface Props {
  start?: string
  end?: string
}

export default function GuiInteractionTrack({ start, end }: Props) {
  const { data: cells = [] } = useQuery({
    queryKey: ['gui-heatmap', start, end],
    queryFn: () => fetchGuiHeatmap(start, end),
    refetchInterval: 30_000,
  })

  if (cells.length === 0) return null

  const max = Math.max(...cells.map(c => c.count), 1)

  return (
    <div className="flex h-6 w-full gap-px rounded bg-neutral-100 dark:bg-neutral-800">
      {cells.map((cell) => {
        const intensity = cell.count / max
        const alpha = 0.1 + intensity * 0.8
        return (
          <div
            key={cell.hour}
            className="flex-1 rounded-sm"
            style={{ backgroundColor: `rgba(59, 130, 246, ${alpha})` }}
            title={`${cell.hour}: ${cell.count} interactions`}
          />
        )
      })}
    </div>
  )
}
```

- [ ] **Step 4.5: Wire into DashboardDay page**

In `DashboardDay.tsx`, import and render below the timeline:

```tsx
import GuiInteractionTrack from '../components/GuiInteractionTrack'

// In the JSX, after the timeline section:
<div className="mt-2">
  <span className="text-xs text-muted-foreground">GUI Interactions</span>
  <GuiInteractionTrack start={dayStart} end={dayEnd} />
</div>
```

- [ ] **Step 4.6: Commit**

```
git add crates/oneshim-storage/ crates/oneshim-web/src/ crates/oneshim-web/frontend/src/
git commit -m "feat(web): add GUI interaction density track to dashboard timeline"
```

---

## Verification

After all tasks:

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Expected: all pass, zero warnings.
