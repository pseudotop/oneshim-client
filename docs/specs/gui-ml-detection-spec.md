# GUI ML Detection Enhancement Spec — Phase 1

**Date**: 2026-04-04
**Branch**: `feat/analysis-wiring`
**Scope**: `oneshim-vision` (gui_detector), `oneshim-core` (models)

## 1. Problem Statement

The GUI element classification uses **first-match heuristic rules** returning a bare `GuiElementType`. Two gaps:

| Issue | Current State | Impact |
|-------|--------------|--------|
| **No type confidence** | `GuiElement.confidence` = OCR page-level confidence, not classification confidence | Can't distinguish "definitely a button" from "maybe a button" |
| **Single-signal rules** | Position OR text → first match wins | Misses cases where multiple weak signals should combine to a strong classification |

`GuiElement` already has `confidence: f32` but it carries OCR confidence from `region.confidence`, not classification certainty.

## 2. Goals

1. **Add `type_confidence: f32` to `GuiElement`**: Classification certainty separate from OCR confidence
2. **Multi-signal scoring**: Replace first-match inference with weighted scoring across position, text, and size signals
3. **Backward compatibility**: Existing `infer_element_type()` unchanged, new `infer_element_type_scored()` added alongside

### Non-Goals

- ONNX Runtime or ML model integration (future Phase 2)
- Training data collection pipeline
- New element types
- Cross-platform rectangle detection changes
- Scheduler or DI wiring changes

## 3. Design

### 3.1 Add `type_confidence` to `GuiElement`

**Current** (`gui_interaction.rs:18-27`):
```rust
pub struct GuiElement {
    pub text: String,
    pub bbox: BoundingBox,
    pub element_type: GuiElementType,
    pub confidence: f32,  // OCR page-level confidence
}
```

**Change**:
```rust
pub struct GuiElement {
    pub text: String,
    pub bbox: BoundingBox,
    pub element_type: GuiElementType,
    pub confidence: f32,          // OCR confidence (unchanged)
    pub type_confidence: f32,     // Classification confidence (0.0–1.0)
}
```

Default `type_confidence: 1.0` at all existing construction sites for backward compatibility.

### 3.2 Multi-Signal Scored Inference

New method `infer_element_type_scored(&self, text: &str, bbox: &BoundingBox) -> (GuiElementType, f32)`.

**Algorithm**: For each candidate type, compute a score from independent signals. The type with the highest score wins.

**Signal functions** (each returns `Option<f32>` for a specific type):

| Signal | Types it scores | Logic |
|--------|----------------|-------|
| `position_score` | TitleBar, TabLabel, StatusBar, ScrollBar, ToolbarIcon | Y-position thresholds (existing rules, converted to scores) |
| `text_score` | Link, MenuItem, Button, TreeItem, TextRegion | Text pattern matching (existing rules, converted to scores) |
| `size_score` | ToolbarIcon, Button | Aspect ratio + absolute dimensions |

**Score aggregation**: Sum all signal contributions per type. Winner = highest sum. `type_confidence = winner_score / (winner_score + runner_up_score)`. Minimum confidence = 0.5 (when winner barely edges out runner-up).

**Example**:
- Element at y=50 (8% from top), text="settings.json", in VSCode sidebar
  - `position_score`: TabLabel=0.7
  - `text_score`: (no match)
  - With app override: TreeItem=0.8
  - Winner: TreeItem (0.8), runner_up: TabLabel (0.7)
  - `type_confidence = 0.8 / (0.8 + 0.7) = 0.53`

### 3.3 Integration

In `build_gui_element()` (`gui_detector/mod.rs:80`):

```rust
// Before:
element_type: self.infer_element_type(&region.text, &region.bbox),

// After:
let (element_type, type_conf) = self.infer_element_type_scored(&region.text, &region.bbox);
// ... then set element_type and type_confidence
```

`infer_element_type()` kept as a thin wrapper that calls scored version and drops the confidence (backward compat for any external callers).

## 4. Files Changed

| File | Change | Lines (~) |
|------|--------|-----------|
| `crates/oneshim-core/src/models/gui_interaction.rs` | Add `type_confidence: f32` to `GuiElement` | +3 |
| `crates/oneshim-vision/src/gui_detector/inference.rs` | Add `infer_element_type_scored()` + signal functions | +90 |
| `crates/oneshim-vision/src/gui_detector/mod.rs` | Use scored inference in `build_gui_element()` | +5, -2 |
| `crates/oneshim-vision/src/gui_detector/tests.rs` | Tests for confidence scoring | +50 |

Fix all `GuiElement` construction sites to include `type_confidence: 1.0`:
| File | Change |
|------|--------|
| `crates/oneshim-vision/src/gui_detector/tests.rs` | Test helpers |
| `src-tauri/src/scheduler/gui_pipeline.rs` | Any manual GuiElement construction |
| Other files constructing `GuiElement` directly | Add field |

**Estimated total**: ~150 lines new/modified

## 5. Test Strategy

| Test | Type |
|------|------|
| Title bar at y=20 → TitleBar with high confidence (>0.8) | unit |
| Ambiguous element (tab region + text matches button) → lower confidence | unit |
| App-specific override increases TreeItem confidence | unit |
| Backward compat: `infer_element_type()` returns same types as before | unit |
| Link with "http://" → very high confidence (>0.9) | unit |

## 6. Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Compilation errors from missing `type_confidence` field | Low | Search all `GuiElement { ... }` construction sites |
| Changed type inference order | Low | Scored system should produce same top type for clear cases |
| Performance | Very Low | O(12 types × 3 signals) = 36 comparisons per element |
