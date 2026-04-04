# GUI ML Detection Phase 2 — ONNX Classifier Infrastructure

**Date**: 2026-04-04
**Branch**: `feat/analysis-wiring`
**Scope**: `oneshim-core` (port), `oneshim-vision` (adapter), `src-tauri` (DI)

## 1. Problem Statement

Phase 1 (a8de4597) added `type_confidence` and multi-signal scored inference. The classifier still relies on heuristic rules. Phase 2 adds ONNX Runtime infrastructure so that:
- When a trained `.onnx` model is available, it's loaded and used for classification
- When no model is present, falls back to Phase 1 scored heuristics (zero degradation)

## 2. Key Insight: Zero Binary Overhead

`ort` v2.0.0-rc.11 is already a transitive dependency via `fastembed` v5.13.0. Adding `ort` as a direct dependency to `oneshim-vision` adds **zero binary size overhead**. The ONNX Runtime shared libraries are already compiled and linked.

## 3. Goals

1. **`GuiElementClassifier` port trait** in `oneshim-core`
2. **`OnnxGuiClassifier` adapter** in `oneshim-vision` behind `ml-detect` feature flag
3. **Integration into `GuiElementDetector`**: Use ML scores when classifier is available, fall back to heuristics otherwise
4. **Model loading**: Load `.onnx` model from configurable path; gracefully skip if absent

### Non-Goals

- Training an ML model (separate project)
- Bundling a pre-trained model in the binary
- Changing the existing `RectangleDetector` path
- Real-time frame classification (per-click crop classification only)

## 4. Design

### 4.1 Port Trait

```rust
// oneshim-core/src/ports/gui_element_classifier.rs
#[async_trait]
pub trait GuiElementClassifier: Send + Sync {
    /// Classify a GUI element from an image crop.
    /// Returns (element_type, confidence) or None if classification fails.
    async fn classify_crop(
        &self,
        crop_rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Option<(GuiElementType, f32)>, CoreError>;

    /// Check if the classifier has a loaded model.
    fn is_ready(&self) -> bool;
}
```

**Why crop-based**: The classifier receives a small image crop around the clicked region, not the full screen. This is faster (<10ms per crop) and more focused.

### 4.2 ONNX Adapter

```rust
// oneshim-vision/src/ml_classifier/mod.rs (behind #[cfg(feature = "ml-detect")])
pub struct OnnxGuiClassifier {
    session: Option<ort::Session>,
    labels: Vec<GuiElementType>,
}
```

**Model contract**:
- Input: `[1, 3, 64, 64]` float32 tensor (RGB, resized to 64x64)
- Output: `[1, 12]` float32 tensor (softmax probabilities for 12 GuiElementType variants)
- Labels ordered: Button, TextInput, Link, MenuItem, TabLabel, StatusBar, TitleBar, ToolbarIcon, TreeItem, ScrollBar, TextRegion, Unknown

**Loading**: `OnnxGuiClassifier::load(model_path: &Path)` → `Result<Self>`. If file doesn't exist, `session = None` and `is_ready()` returns `false`.

**Inference**:
1. Resize crop to 64×64 (already have `fast_image_resize` in workspace)
2. Normalize to [0, 1] float32
3. Run `session.run()` → softmax output
4. Return (argmax_type, max_probability) if max > 0.3 threshold, else None

### 4.3 Integration into GuiElementDetector

`GuiElementDetector` gains an optional `Arc<dyn GuiElementClassifier>`:

```rust
pub struct GuiElementDetector {
    screen_resolution: (u32, u32),
    pii_filter_level: PiiFilterLevel,
    proximity_threshold_px: u32,
    ml_classifier: Option<Arc<dyn GuiElementClassifier>>,  // NEW
}
```

**In `build_gui_element()`**:
1. Run heuristic scoring (existing) → `(heuristic_type, heuristic_conf)`
2. If `ml_classifier.is_some() && ml_classifier.is_ready()`:
   - Extract crop from OCR region bounds (needs frame data passed in)
   - Run `classify_crop()` → `(ml_type, ml_conf)`
   - If `ml_conf > heuristic_conf`: use ML result
   - Else: keep heuristic result
3. Set `type_confidence` to winner's confidence

**Important**: `build_gui_element()` currently takes only `&OcrRegion`. For ML classification, it would need the frame image too. Two approaches:
- **Option A**: Add `frame_data: Option<&[u8]>` parameter to `build_gui_element()` (breaking change)
- **Option B**: Make ML classification a separate step in the pipeline — after `build_gui_element()`, optionally refine type via classifier

**Decision**: Option B — keep `build_gui_element()` unchanged. Add a new method `refine_with_ml()` that takes a GuiElement + frame data and returns a refined GuiElement. This is called from the scheduler pipeline only when the classifier is available.

### 4.4 Feature Flag

```toml
# oneshim-vision/Cargo.toml
[features]
default = ["native-vision"]
ml-detect = ["dep:ort"]

[dependencies]
ort = { version = "2.0.0-rc.11", optional = true }
```

The `src-tauri` binary enables `ml-detect` when desired:
```toml
oneshim-vision = { workspace = true, features = ["ml-detect"] }
```

**Default**: OFF. Users opt in when they have a model file.

### 4.5 DI Wiring

In `src-tauri/src/agent_runtime_support.rs` (or `automation_runtime.rs`):
```rust
#[cfg(feature = "ml-detect")]
let ml_classifier: Option<Arc<dyn GuiElementClassifier>> = {
    let model_path = data_dir.join("models/gui-classifier.onnx");
    match OnnxGuiClassifier::load(&model_path) {
        Ok(c) if c.is_ready() => Some(Arc::new(c)),
        _ => None,
    }
};
```

Pass to `GuiElementDetector` via `with_ml_classifier()`.

## 5. Files Changed

| File | Change | Lines (~) |
|------|--------|-----------|
| `crates/oneshim-core/src/ports/gui_element_classifier.rs` | **NEW** — port trait | +20 |
| `crates/oneshim-core/src/ports/mod.rs` | Export module | +1 |
| `crates/oneshim-vision/Cargo.toml` | Add `ort` optional dep + `ml-detect` feature | +3 |
| `crates/oneshim-vision/src/ml_classifier/mod.rs` | **NEW** — `OnnxGuiClassifier` | +100 |
| `crates/oneshim-vision/src/ml_classifier/preprocess.rs` | **NEW** — image preprocessing | +40 |
| `crates/oneshim-vision/src/lib.rs` | Export `ml_classifier` module | +3 |
| `crates/oneshim-vision/src/gui_detector/mod.rs` | Add optional `ml_classifier` field + `with_ml_classifier()` | +15 |

**Estimated total**: ~180 lines new code + ~50 lines tests

## 6. Test Strategy

| Test | Type |
|------|------|
| OnnxGuiClassifier::load with missing file → is_ready() false | unit |
| Preprocessing: resize to 64×64, normalize to [0,1] | unit |
| GuiElementDetector with no classifier → same as Phase 1 | unit |
| Label ordering matches GuiElementType enum | unit |
| Feature flag: code compiles with and without ml-detect | build |

## 7. Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| ort version mismatch with fastembed | Low | Pin to same version (2.0.0-rc.11) from Cargo.lock |
| Model file not found → classifier disabled | None | Graceful fallback to heuristics |
| Inference latency > budget | Low | 64×64 crop + small CNN = <5ms; threshold to skip if slow |
| ort initialization conflicts with fastembed | Very Low | ort v2 handles multi-session correctly |
