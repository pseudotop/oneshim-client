# Core ML GUI Element Segmentation

**Date**: 2026-03-27
**Status**: Reviewed (revision 1)
**Scope**: `crates/oneshim-core/`, `crates/oneshim-vision/`, `src-tauri/`
**Builds on**: `docs/superpowers/specs/2026-03-26-native-detection-segmentation-design.md`

## Problem

The current GUI element detection relies solely on OCR text extraction + heuristic inference (`GuiElementDetector`). This approach:

1. **Misses non-text elements**: Icons, images, separators, scroll bars detected only by position heuristics
2. **No structural boundaries**: Cannot detect container edges, panel divisions, or toolbar regions
3. **Platform-dependent OCR quality**: Tesseract CPU-bound with inconsistent confidence across fonts/DPI
4. **No ML acceleration**: macOS Vision.framework and Windows WinRT provide GPU-accelerated detection natively — unused

## Scope

This spec implements the **infrastructure + built-in Apple Vision detection** layer. Custom Core ML model inference is deferred — the trait system supports future plug-in.

### In Scope
1. `RectangleDetector` port trait in `oneshim-core`
2. macOS adapter: `VNDetectRectanglesRequest` via objc2 FFI
3. Non-macOS fallback adapter (no-op, returns empty results)
4. `native-vision` feature flag in `oneshim-vision`
5. Integration: merge rectangle detection results into `UiScene` alongside OCR elements
6. `infer_element_type` visibility promotion (`pub(super)` → `pub`)

### Out of Scope
- Custom Core ML model training/loading (deferred — port trait ready for plug-in)
- ONNX Runtime inference adapter (deferred — `fastembed` only in embedding crate)
- Native OCR (macOS Vision.framework text recognition) — covered by existing spec, can be separate PR
- Windows WinRT rectangle detection

## Design

### Architecture

```
crates/oneshim-core/src/ports/
├── rectangle_detector.rs  (NEW — sync trait)

crates/oneshim-vision/src/
├── native_detect/
│   ├── mod.rs             (NEW — cross-platform dispatch)
│   ├── macos.rs           (NEW — VNDetectRectanglesRequest FFI)
│   └── fallback.rs        (NEW — no-op for non-macOS)
├── element_finder.rs      (MODIFY — merge rectangles into UiScene)
├── gui_detector/
│   └── inference.rs       (MODIFY — pub(super) → pub)
└── lib.rs                 (MODIFY — add native_detect module)

src-tauri/
├── src/automation_runtime.rs  (MODIFY — wire RectangleDetector)
```

### RectangleDetector Port Trait

```rust
// crates/oneshim-core/src/ports/rectangle_detector.rs

use crate::error::CoreError;
use crate::models::intent::ElementBounds;
use crate::models::ui_scene::NormalizedBounds;

/// Detected rectangle from ML/vision framework analysis.
#[derive(Debug, Clone)]
pub struct DetectedRectangle {
    pub bounds: ElementBounds,
    pub bounds_normalized: NormalizedBounds,
    pub confidence: f64,
    pub classification: Option<String>,
}

/// Synchronous rectangle detection from image data.
/// Apple Vision APIs are synchronous (run on internal dispatch queues);
/// callers wrap in `spawn_blocking` for async contexts.
pub trait RectangleDetector: Send + Sync {
    fn detect_rectangles(
        &self,
        image: &[u8],
        image_width: u32,
        image_height: u32,
        min_size: f32,
        max_results: usize,
    ) -> Result<Vec<DetectedRectangle>, CoreError>;

    fn provider_name(&self) -> &str;
}
```

**Key decisions:**
- **Sync trait** (not async) — Apple Vision APIs are synchronous, callers wrap in `spawn_blocking`
- **Image as raw bytes** — the same `&[u8]` buffer passed to OCR
- **min_size normalized** (0.0-1.0) — relative to image dimensions, e.g., 0.02 = 2% of image size
- **classification optional** — VNDetectRectangles doesn't classify; future Core ML models will

### macOS VNDetectRectanglesRequest Implementation

Uses `objc2` raw FFI to call Vision.framework:

```rust
// Simplified flow:
// 1. Create NSData from image bytes
// 2. Create VNImageRequestHandler initWithData:options:
// 3. Create VNDetectRectanglesRequest
// 4. Set properties: minimumSize, maximumObservations, minimumConfidence
// 5. performRequests:error:
// 6. Extract VNRectangleObservation results
// 7. Convert Vision coordinates (bottom-left, normalized) to pixel coordinates (top-left)
```

**Coordinate conversion** (Vision → pixel):
```
pixel_x = norm_x * image_width
pixel_y = (1.0 - norm_y - norm_height) * image_height  // flip Y axis
pixel_w = norm_width * image_width
pixel_h = norm_height * image_height
```

### Element Finder Integration

The `OcrElementFinder.analyze_scene_from_image()` currently builds `UiScene` from OCR results only. Extend it to:

1. Run rectangle detection in parallel with OCR
2. Merge results: rectangles that don't overlap significantly with OCR elements → add as new elements with `role: "container"` or `role: "region"`
3. Rectangles that match OCR elements → boost confidence of the OCR element

```rust
// In element_finder.rs analyze_scene_from_image():
let ocr_elements = self.ocr.extract_elements(image, format).await?;
let rectangles = tokio::task::spawn_blocking(|| {
    detector.detect_rectangles(image, w, h, 0.02, 100)
}).await??;

let merged = merge_ocr_and_rectangles(ocr_elements, rectangles);
```

### Merge Strategy

For each detected rectangle:
1. **Overlap check**: Calculate IoU (Intersection over Union) with each OCR element
2. **IoU > 0.5**: Rectangle matches an existing OCR element → boost confidence by 0.1 (capped at 1.0)
3. **IoU < 0.2 for all OCR elements**: New structural element → add with `role: "region"`, `label: ""`, confidence from rectangle
4. **0.2 ≤ IoU ≤ 0.5**: Ambiguous — skip (don't add duplicate)

### Feature Flag

```toml
# crates/oneshim-vision/Cargo.toml
[features]
default = ["native-vision"]
native-vision = []     # macOS Vision.framework rectangle detection
ocr = ["leptess"]      # Tesseract OCR
```

Module gating:
```rust
#[cfg(feature = "native-vision")]
pub mod native_detect;
```

### Wiring

`RectangleDetector` is created in `automation_runtime.rs`:
- macOS: `VisionRectangleDetector::new()`
- Other: `FallbackRectangleDetector` (returns empty vec)

Injected into `OcrElementFinder` as an optional field:
```rust
pub struct OcrElementFinder {
    ocr: Arc<dyn OcrProvider>,
    rectangle_detector: Option<Arc<dyn RectangleDetector>>,  // NEW
}
```

### Performance

- VNDetectRectanglesRequest: ~50-100ms on Apple Silicon (GPU-accelerated)
- Runs in parallel with OCR via `spawn_blocking` + `tokio::join!`
- Max 100 rectangles per frame (configurable)
- Only runs when scene analysis is requested (not every capture tick)

### Files Changed

| File | Change Type | Description |
|------|------------|-------------|
| `crates/oneshim-core/src/ports/rectangle_detector.rs` | NEW | Port trait + DetectedRectangle model |
| `crates/oneshim-core/src/ports/mod.rs` | MODIFY | Register module |
| `crates/oneshim-vision/src/native_detect/mod.rs` | NEW | Cross-platform dispatch |
| `crates/oneshim-vision/src/native_detect/macos.rs` | NEW | VNDetectRectanglesRequest FFI |
| `crates/oneshim-vision/src/native_detect/fallback.rs` | NEW | No-op fallback |
| `crates/oneshim-vision/src/element_finder.rs` | MODIFY | Accept + merge rectangle results |
| `crates/oneshim-vision/src/gui_detector/inference.rs` | MODIFY | `pub(super)` → `pub` |
| `crates/oneshim-vision/src/lib.rs` | MODIFY | Add `native_detect` module |
| `crates/oneshim-vision/Cargo.toml` | MODIFY | Add `native-vision` feature flag |
| `src-tauri/src/automation_runtime.rs` | MODIFY | Wire RectangleDetector |

### Testing Strategy

1. **Unit tests**: Coordinate conversion, IoU calculation, merge logic
2. **macOS integration test**: `#[cfg(target_os = "macos")]` test with a test image
3. **Fallback test**: Verify non-macOS returns empty vec without error
4. **Element finder test**: Verify merged scene contains both OCR and rectangle elements
