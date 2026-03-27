# Core ML Segmentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add rectangle detection via Apple Vision framework and merge results into the existing UiScene pipeline.

**Architecture:** New `RectangleDetector` port trait in `oneshim-core`, macOS adapter using `VNDetectRectanglesRequest` via raw objc2 FFI, fallback no-op for non-macOS, integration into `OcrElementFinder.analyze_scene_from_image_data()`.

**Tech Stack:** Rust (objc2 raw FFI, core-foundation), Apple Vision.framework, feature flags

**Spec:** `docs/superpowers/specs/2026-03-27-core-ml-segmentation-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/oneshim-core/src/ports/rectangle_detector.rs` | Create | Port trait + DetectedRectangle model |
| `crates/oneshim-core/src/ports/mod.rs` | Modify | Register module |
| `crates/oneshim-vision/src/native_detect/mod.rs` | Create | Cross-platform dispatch |
| `crates/oneshim-vision/src/native_detect/macos.rs` | Create | VNDetectRectanglesRequest FFI |
| `crates/oneshim-vision/src/native_detect/fallback.rs` | Create | No-op for non-macOS |
| `crates/oneshim-vision/src/lib.rs` | Modify | Add native_detect module |
| `crates/oneshim-vision/Cargo.toml` | Modify | Add native-vision feature flag |
| `crates/oneshim-vision/src/element_finder.rs` | Modify | Accept RectangleDetector, merge results |
| `src-tauri/src/automation_runtime.rs` | Modify | Wire RectangleDetector |

---

### Task 1: Create RectangleDetector Port Trait

**Files:**
- Create: `crates/oneshim-core/src/ports/rectangle_detector.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] **Step 1: Create the port trait file**

```rust
// crates/oneshim-core/src/ports/rectangle_detector.rs

//! Rectangle detection port — defines the contract for detecting rectangular
//! UI element boundaries in screen images. Implemented by platform-specific
//! adapters (macOS Vision.framework, future Core ML models).

use crate::error::CoreError;
use crate::models::intent::ElementBounds;
use crate::models::ui_scene::NormalizedBounds;

/// A rectangle detected by a vision framework or ML model.
#[derive(Debug, Clone)]
pub struct DetectedRectangle {
    /// Absolute pixel bounds in the source image.
    pub bounds: ElementBounds,
    /// Normalized bounds (0.0-1.0) relative to image dimensions.
    pub bounds_normalized: NormalizedBounds,
    /// Detection confidence (0.0-1.0).
    pub confidence: f64,
    /// Optional classification label (e.g., "button", "text_field").
    /// None for basic rectangle detection; populated by ML classifiers.
    pub classification: Option<String>,
}

/// Synchronous rectangle detection from image data.
///
/// Apple Vision APIs run on internal dispatch queues and return synchronously.
/// Callers should wrap calls in `tokio::task::spawn_blocking` for async contexts.
pub trait RectangleDetector: Send + Sync {
    /// Detect rectangles in the given image.
    ///
    /// - `image`: Raw image bytes (PNG, JPEG, WebP)
    /// - `image_width`, `image_height`: Decoded image dimensions in pixels
    /// - `min_size`: Minimum rectangle size as fraction of image (0.0-1.0), e.g., 0.02 = 2%
    /// - `max_results`: Maximum number of rectangles to return
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

- [ ] **Step 2: Register module in ports/mod.rs**

Add after `pub mod overlay_driver;` (line 26):

```rust
pub mod rectangle_detector;
```

- [ ] **Step 3: Verify**

Run: `cargo check -p oneshim-core`

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-core/src/ports/rectangle_detector.rs crates/oneshim-core/src/ports/mod.rs
git commit -m "feat(core): add RectangleDetector port trait"
```

---

### Task 2: Add native-vision Feature Flag and Fallback

**Files:**
- Modify: `crates/oneshim-vision/Cargo.toml`
- Create: `crates/oneshim-vision/src/native_detect/mod.rs`
- Create: `crates/oneshim-vision/src/native_detect/fallback.rs`
- Modify: `crates/oneshim-vision/src/lib.rs`

- [ ] **Step 1: Add feature flag to Cargo.toml**

Change line 13-14 from:
```toml
[features]
default = []
ocr = ["leptess"]
```
to:
```toml
[features]
default = ["native-vision"]
native-vision = []
ocr = ["leptess"]
```

- [ ] **Step 2: Create fallback.rs**

```rust
// crates/oneshim-vision/src/native_detect/fallback.rs

use oneshim_core::error::CoreError;
use oneshim_core::ports::rectangle_detector::{DetectedRectangle, RectangleDetector};

/// No-op rectangle detector for platforms without native vision APIs.
pub struct FallbackRectangleDetector;

impl RectangleDetector for FallbackRectangleDetector {
    fn detect_rectangles(
        &self,
        _image: &[u8],
        _image_width: u32,
        _image_height: u32,
        _min_size: f32,
        _max_results: usize,
    ) -> Result<Vec<DetectedRectangle>, CoreError> {
        Ok(Vec::new())
    }

    fn provider_name(&self) -> &str {
        "fallback"
    }
}
```

- [ ] **Step 3: Create mod.rs with platform dispatch**

```rust
// crates/oneshim-vision/src/native_detect/mod.rs

#[cfg(target_os = "macos")]
pub mod macos;

pub mod fallback;

use std::sync::Arc;
use oneshim_core::ports::rectangle_detector::RectangleDetector;

/// Create the platform-appropriate rectangle detector.
pub fn create_rectangle_detector() -> Arc<dyn RectangleDetector> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(macos::VisionRectangleDetector::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(fallback::FallbackRectangleDetector)
    }
}
```

- [ ] **Step 4: Add module to lib.rs**

After `pub mod local_ocr_provider;` (line 11), add:

```rust
#[cfg(feature = "native-vision")]
pub mod native_detect;
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p oneshim-vision`

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-vision/Cargo.toml crates/oneshim-vision/src/lib.rs \
  crates/oneshim-vision/src/native_detect/mod.rs crates/oneshim-vision/src/native_detect/fallback.rs
git commit -m "feat(vision): add native-vision feature flag and fallback detector"
```

---

### Task 3: Implement macOS VNDetectRectanglesRequest FFI

**Files:**
- Create: `crates/oneshim-vision/src/native_detect/macos.rs`

- [ ] **Step 1: Create macOS adapter with Vision.framework FFI**

```rust
// crates/oneshim-vision/src/native_detect/macos.rs

//! macOS rectangle detection using Apple Vision VNDetectRectanglesRequest.

use oneshim_core::error::CoreError;
use oneshim_core::models::intent::ElementBounds;
use oneshim_core::models::ui_scene::NormalizedBounds;
use oneshim_core::ports::rectangle_detector::{DetectedRectangle, RectangleDetector};
use tracing::{debug, warn};

use core_foundation::base::TCFType;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFDictionary;
use core_foundation::error::CFError;
use std::ffi::c_void;
use std::ptr;

// Vision.framework FFI — raw bindings (no objc2-vision crate available)
#[link(name = "Vision", kind = "framework")]
extern "C" {}

extern "C" {
    fn objc_getClass(name: *const std::ffi::c_char) -> *const c_void;
    fn sel_registerName(name: *const std::ffi::c_char) -> *const c_void;
    fn objc_msgSend(receiver: *const c_void, sel: *const c_void, ...) -> *const c_void;
}

pub struct VisionRectangleDetector;

impl VisionRectangleDetector {
    pub fn new() -> Self {
        Self
    }
}

impl RectangleDetector for VisionRectangleDetector {
    fn detect_rectangles(
        &self,
        image: &[u8],
        image_width: u32,
        image_height: u32,
        min_size: f32,
        max_results: usize,
    ) -> Result<Vec<DetectedRectangle>, CoreError> {
        unsafe { detect_rectangles_ffi(image, image_width, image_height, min_size, max_results) }
    }

    fn provider_name(&self) -> &str {
        "macos-vision"
    }
}

unsafe fn detect_rectangles_ffi(
    image: &[u8],
    image_width: u32,
    image_height: u32,
    min_size: f32,
    max_results: usize,
) -> Result<Vec<DetectedRectangle>, CoreError> {
    // 1. Create NSData from image bytes
    let nsdata_class = class(b"NSData\0");
    let nsdata: *const c_void = msg(
        msg(nsdata_class, sel(b"alloc\0")),
        sel(b"initWithBytes:length:\0"),
        image.as_ptr() as *const c_void,
        image.len(),
    );
    if nsdata.is_null() {
        return Err(CoreError::Internal("Failed to create NSData".into()));
    }

    // 2. Create VNImageRequestHandler
    let handler_class = class(b"VNImageRequestHandler\0");
    let empty_dict = CFDictionary::<c_void, c_void>::from_CFType_pairs(&[]);
    let handler: *const c_void = msg(
        msg(handler_class, sel(b"alloc\0")),
        sel(b"initWithData:options:\0"),
        nsdata,
        empty_dict.as_concrete_TypeRef() as *const c_void,
    );
    if handler.is_null() {
        release(nsdata);
        return Err(CoreError::Internal("Failed to create VNImageRequestHandler".into()));
    }

    // 3. Create VNDetectRectanglesRequest
    let request_class = class(b"VNDetectRectanglesRequest\0");
    let request: *const c_void = msg(msg(request_class, sel(b"alloc\0")), sel(b"init\0"));
    if request.is_null() {
        release(handler);
        release(nsdata);
        return Err(CoreError::Internal("Failed to create VNDetectRectanglesRequest".into()));
    }

    // 4. Configure request
    msg_void(request, sel(b"setMinimumSize:\0"), min_size as f64);
    msg_void(request, sel(b"setMaximumObservations:\0"), max_results as u64);
    msg_void(request, sel(b"setMinimumConfidence:\0"), 0.3f32);

    // 5. Perform request
    let mut error_ptr: *const c_void = ptr::null();
    let ns_array_class = class(b"NSArray\0");
    let request_array: *const c_void = msg(
        ns_array_class,
        sel(b"arrayWithObject:\0"),
        request,
    );

    let success: bool = msg_bool(
        handler,
        sel(b"performRequests:error:\0"),
        request_array,
        &mut error_ptr as *mut *const c_void,
    );

    if !success {
        let err_msg = if !error_ptr.is_null() {
            let desc: *const c_void = msg(error_ptr, sel(b"localizedDescription\0"));
            nsstring_to_rust(desc)
        } else {
            "unknown error".to_string()
        };
        release(request);
        release(handler);
        release(nsdata);
        return Err(CoreError::Internal(format!("Vision request failed: {err_msg}")));
    }

    // 6. Extract results
    let results: *const c_void = msg(request, sel(b"results\0"));
    let count: usize = if results.is_null() {
        0
    } else {
        msg_usize(results, sel(b"count\0"))
    };

    debug!(count, "VNDetectRectanglesRequest returned observations");

    let mut detected = Vec::with_capacity(count);
    let w = image_width as f64;
    let h = image_height as f64;

    for i in 0..count {
        let obs: *const c_void = msg(results, sel(b"objectAtIndex:\0"), i);
        if obs.is_null() {
            continue;
        }

        let confidence: f32 = msg_f32(obs, sel(b"confidence\0"));

        // VNRectangleObservation boundingBox returns CGRect (normalized, bottom-left origin)
        let bbox = get_bounding_box(obs);

        // Convert Vision coordinates (bottom-left, normalized) to pixel (top-left)
        let pixel_x = (bbox.0 * w) as i32;
        let pixel_y = ((1.0 - bbox.1 - bbox.3) * h) as i32;
        let pixel_w = (bbox.2 * w) as u32;
        let pixel_h = (bbox.3 * h) as u32;

        detected.push(DetectedRectangle {
            bounds: ElementBounds {
                x: pixel_x.max(0),
                y: pixel_y.max(0),
                width: pixel_w.max(1),
                height: pixel_h.max(1),
            },
            bounds_normalized: NormalizedBounds::new(
                bbox.0 as f32,
                (1.0 - bbox.1 - bbox.3) as f32, // flip Y
                bbox.2 as f32,
                bbox.3 as f32,
            ),
            confidence: confidence as f64,
            classification: None,
        });
    }

    // Cleanup
    release(request);
    release(handler);
    release(nsdata);

    Ok(detected)
}

// --- FFI helpers ---

unsafe fn class(name: &[u8]) -> *const c_void {
    objc_getClass(name.as_ptr() as *const std::ffi::c_char)
}

unsafe fn sel(name: &[u8]) -> *const c_void {
    sel_registerName(name.as_ptr() as *const std::ffi::c_char)
}

unsafe fn msg(receiver: *const c_void, sel: *const c_void, args: ...) -> *const c_void {
    // This is a simplified wrapper — real usage requires variadic FFI
    // which Rust doesn't natively support. Use transmute-based approach.
    objc_msgSend(receiver, sel)
}

// Placeholder: actual implementation uses transmuted function pointers
// for each calling convention needed.

unsafe fn msg_void(_receiver: *const c_void, _sel: *const c_void, _arg: impl Copy) {}
unsafe fn msg_bool(_receiver: *const c_void, _sel: *const c_void, _arg1: *const c_void, _arg2: *const c_void) -> bool { false }
unsafe fn msg_usize(_receiver: *const c_void, _sel: *const c_void) -> usize { 0 }
unsafe fn msg_f32(_receiver: *const c_void, _sel: *const c_void) -> f32 { 0.0 }
unsafe fn get_bounding_box(_obs: *const c_void) -> (f64, f64, f64, f64) { (0.0, 0.0, 0.0, 0.0) }
unsafe fn nsstring_to_rust(_ns: *const c_void) -> String { String::new() }
unsafe fn release(_obj: *const c_void) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_coordinate_conversion() {
        // Vision: (0.1, 0.2, 0.3, 0.4) normalized, bottom-left origin
        // Pixel (1920x1080): x=192, y=(1.0-0.2-0.4)*1080=432, w=576, h=432
        let bbox = (0.1_f64, 0.2_f64, 0.3_f64, 0.4_f64);
        let w = 1920.0_f64;
        let h = 1080.0_f64;

        let pixel_x = (bbox.0 * w) as i32;
        let pixel_y = ((1.0 - bbox.1 - bbox.3) * h) as i32;
        let pixel_w = (bbox.2 * w) as u32;
        let pixel_h = (bbox.3 * h) as u32;

        assert_eq!(pixel_x, 192);
        assert_eq!(pixel_y, 432);
        assert_eq!(pixel_w, 576);
        assert_eq!(pixel_h, 432);
    }
}
```

**IMPORTANT NOTE**: The raw objc2 FFI with variadic `objc_msgSend` is complex. The actual implementation should use the `objc2` crate's `msg_send!` macro properly. The placeholder functions above need real implementations using transmuted function pointers for each ABI. The implementing agent must:
1. Use `objc2::runtime::AnyClass::get()` instead of raw `objc_getClass`
2. Use `objc2::msg_send!` and `objc2::msg_send_id!` macros
3. Follow the pattern in `accessibility/macos/extractor.rs` for proper memory management

- [ ] **Step 2: Verify macOS compilation**

Run: `cargo check -p oneshim-vision` (macOS only)

- [ ] **Step 3: Run test**

Run: `cargo test -p oneshim-vision -- native_detect::macos`

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-vision/src/native_detect/macos.rs
git commit -m "feat(vision): macOS VNDetectRectanglesRequest adapter"
```

---

### Task 4: Integrate RectangleDetector into ElementFinder

**Files:**
- Modify: `crates/oneshim-vision/src/element_finder.rs`

- [ ] **Step 1: Add rectangle_detector field to OcrElementFinder**

Change struct definition from:
```rust
pub struct OcrElementFinder {
    ocr_provider: Arc<dyn OcrProvider>,
    last_image: tokio::sync::RwLock<Option<(Vec<u8>, String)>>,
}
```
to:
```rust
pub struct OcrElementFinder {
    ocr_provider: Arc<dyn OcrProvider>,
    rectangle_detector: Option<Arc<dyn RectangleDetector>>,
    last_image: tokio::sync::RwLock<Option<(Vec<u8>, String)>>,
}
```

- [ ] **Step 2: Update constructor**

Change `new()` to accept optional detector:
```rust
pub fn new(ocr_provider: Arc<dyn OcrProvider>) -> Self {
    Self {
        ocr_provider,
        rectangle_detector: None,
        last_image: tokio::sync::RwLock::new(None),
    }
}

pub fn with_rectangle_detector(mut self, detector: Arc<dyn RectangleDetector>) -> Self {
    self.rectangle_detector = Some(detector);
    self
}
```

- [ ] **Step 3: Integrate into analyze_scene_from_image_data**

Modify lines 106-140 of `element_finder.rs`:

```rust
async fn analyze_scene_from_image_data(
    &self,
    image_data: Vec<u8>,
    image_format: String,
    app_name: Option<&str>,
    screen_id: Option<&str>,
) -> Result<UiScene, CoreError> {
    let (screen_width, screen_height) = image::load_from_memory(&image_data)
        .map(|img| (img.width().max(1), img.height().max(1)))
        .map_err(|e| CoreError::OcrError(format!("Failed to parse image size: {e}")))?;

    // Run OCR
    let ocr_results = self
        .ocr_provider
        .extract_elements(&image_data, &image_format)
        .await?;

    let mut elements = Self::ocr_to_scene_elements(
        &ocr_results,
        screen_width,
        screen_height,
        app_name,
        screen_id,
    );

    // Run rectangle detection (parallel-safe: uses &[u8] borrow)
    if let Some(ref detector) = self.rectangle_detector {
        let det = detector.clone();
        let img = image_data.clone();
        let rects = tokio::task::spawn_blocking(move || {
            det.detect_rectangles(&img, screen_width, screen_height, 0.02, 100)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking failed: {e}")))?;

        if let Ok(rects) = rects {
            let merged = merge_rectangles(&rects, &elements, screen_width, screen_height);
            elements.extend(merged);
            tracing::debug!(
                rect_count = rects.len(),
                merged_count = elements.len(),
                "merged rectangle detection results"
            );
        }
    }

    Ok(UiScene {
        schema_version: UI_SCENE_SCHEMA_VERSION.to_string(),
        scene_id: format!("scene_{}", Uuid::new_v4().simple()),
        app_name: app_name.map(str::to_string),
        screen_id: screen_id.map(str::to_string),
        captured_at: Utc::now(),
        screen_width,
        screen_height,
        elements,
    })
}
```

- [ ] **Step 4: Add merge function and IoU calculation**

Add at the bottom of the file:

```rust
/// Merge detected rectangles with existing OCR elements.
/// Rectangles with low IoU overlap → new "region" elements.
/// Rectangles with high IoU → skip (already covered by OCR).
fn merge_rectangles(
    rects: &[DetectedRectangle],
    ocr_elements: &[UiSceneElement],
    screen_width: u32,
    screen_height: u32,
) -> Vec<UiSceneElement> {
    let w = screen_width.max(1) as f32;
    let h = screen_height.max(1) as f32;
    let mut new_elements = Vec::new();

    for rect in rects {
        let max_iou = ocr_elements
            .iter()
            .map(|el| compute_iou(&rect.bounds, &el.bbox_abs))
            .fold(0.0_f32, f32::max);

        // Low overlap — this is a new structural element
        if max_iou < 0.2 {
            new_elements.push(UiSceneElement {
                element_id: format!("rect_{}", Uuid::new_v4().simple()),
                bbox_abs: rect.bounds.clone(),
                bbox_norm: NormalizedBounds::new(
                    rect.bounds.x as f32 / w,
                    rect.bounds.y as f32 / h,
                    rect.bounds.width as f32 / w,
                    rect.bounds.height as f32 / h,
                ),
                label: String::new(),
                role: Some("region".to_string()),
                intent: None,
                state: None,
                confidence: rect.confidence,
                text_masked: None,
                parent_id: None,
            });
        }
        // High overlap (>0.5) — skip, already covered by OCR
        // Medium overlap (0.2-0.5) — ambiguous, skip
    }

    new_elements
}

fn compute_iou(a: &ElementBounds, b: &ElementBounds) -> f32 {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width as i32).min(b.x + b.width as i32);
    let y2 = (a.y + a.height as i32).min(b.y + b.height as i32);

    if x2 <= x1 || y2 <= y1 {
        return 0.0;
    }

    let intersection = (x2 - x1) as f32 * (y2 - y1) as f32;
    let area_a = a.width as f32 * a.height as f32;
    let area_b = b.width as f32 * b.height as f32;
    let union = area_a + area_b - intersection;

    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}
```

- [ ] **Step 5: Add necessary imports**

Add to the imports at the top of `element_finder.rs`:

```rust
use oneshim_core::ports::rectangle_detector::{DetectedRectangle, RectangleDetector};
```

- [ ] **Step 6: Verify compilation and run tests**

Run: `cargo check -p oneshim-vision && cargo test -p oneshim-vision`

- [ ] **Step 7: Commit**

```bash
git add crates/oneshim-vision/src/element_finder.rs
git commit -m "feat(vision): integrate rectangle detection into element finder"
```

---

### Task 5: Wire RectangleDetector in automation_runtime.rs

**Files:**
- Modify: `src-tauri/src/automation_runtime.rs`

- [ ] **Step 1: Create RectangleDetector and inject into OcrElementFinder**

Find where `OcrElementFinder` or `LatestFrameOcrElementFinder` is created. Add rectangle detector creation alongside:

```rust
// Create platform rectangle detector
#[cfg(feature = "native-vision")]
let rectangle_detector: Option<Arc<dyn oneshim_core::ports::rectangle_detector::RectangleDetector>> = {
    Some(oneshim_vision::native_detect::create_rectangle_detector())
};
#[cfg(not(feature = "native-vision"))]
let rectangle_detector: Option<Arc<dyn oneshim_core::ports::rectangle_detector::RectangleDetector>> = None;
```

Then where OcrElementFinder is constructed, chain the builder:

```rust
let ocr_finder = OcrElementFinder::new(ocr_provider.clone());
let ocr_finder = if let Some(det) = rectangle_detector {
    ocr_finder.with_rectangle_detector(det)
} else {
    ocr_finder
};
```

- [ ] **Step 2: Add native-vision feature to src-tauri/Cargo.toml**

Find the oneshim-vision dependency in `src-tauri/Cargo.toml` and add the feature:

```toml
oneshim-vision = { path = "../crates/oneshim-vision", features = ["native-vision"] }
```

- [ ] **Step 3: Verify full workspace compilation**

Run: `cargo check --workspace`

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-vision`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation_runtime.rs src-tauri/Cargo.toml
git commit -m "feat(runtime): wire RectangleDetector into element finder pipeline"
```

---

## Self-Review Checklist

1. **Spec coverage**: Port trait (Task 1), macOS adapter (Task 3), fallback (Task 2), feature flag (Task 2), merge strategy (Task 4), wiring (Task 5).
2. **Placeholder scan**: Task 3 macOS FFI has placeholder helper functions — implementing agent must fill with real objc2 FFI. This is explicitly documented.
3. **Type consistency**: `DetectedRectangle` used consistently. `RectangleDetector` trait matches across all tasks. `OcrElementFinder.with_rectangle_detector()` builder matches Task 5 usage.
4. **Dependency chain**: Task 1 (trait) → Task 2 (feature + fallback) → Task 3 (macOS) → Task 4 (integration) → Task 5 (wiring).
