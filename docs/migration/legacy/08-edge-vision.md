[English](./08-edge-vision.md) | [한국어](./08-edge-vision.ko.md)

# 8. Edge Image Processing Pipeline (oneshim-vision)

[← Code Sketches](./07-code-sketches.md) | [Testing Strategy →](./09-testing.md)

---

> **Principle**: The client operates like an edge computer. Full image analysis is handled by the server; the client only performs **essential metadata extraction + preprocessed image transmission**. Instead of video, we use **event-based frame capture + delta encoding** to avoid throughput issues.

## Design Philosophy: Video ❌ → Smart Frames ✅

```
Video (H.264/VP9):
  - 30fps × 1080p = ~2-5 Mbps (continuous bandwidth consumption)
  - GPU needed for decoding
  - Difficult per-frame analysis (GOP-level decoding)
  - Storage requirements explode

ONESHIM Edge approach:
  - Capture only on event triggers (5s throttle)
  - Transmit only changed regions (delta encoding) → ~5-50KB per frame
  - Each frame is independent → rewind/random access possible
  - CPU-only processing, no GPU required
```

## Transfer Volume Comparison

| Method | Size Per Frame | Per Minute (12 captures) | Per Hour |
|--------|---------------|--------------------------|----------|
| Raw PNG (1920×1080) | ~3-6MB | ~36-72MB | ~2-4GB |
| Raw JPEG 85% | ~150-300KB | ~1.8-3.6MB | ~108-216MB |
| **WebP Thumbnail (480×270)** | **~10-30KB** | **~120-360KB** | **~7-22MB** |
| **Delta (changed regions only)** | **~5-50KB** | **~60-600KB** | **~4-36MB** |
| Metadata only | ~0.5-1KB | ~6-12KB | ~360-720KB |
| **ONESHIM Mixed (adaptive)** | **~10-100KB** | **~200KB-1MB** | **~12-60MB** |
| Video H.264 30fps | N/A | ~15-37MB | **~900MB-2.2GB** |

The ONESHIM approach uses only **1/30 to 1/100** of the bandwidth compared to video.

---

## Preprocessing Orchestrator (processor.rs)

```rust
/// Edge Processing: Capture → Importance judgment → Conditional preprocessing → Transmission payload generation
pub struct FrameProcessor {
    prev_frame: Option<DynamicImage>,   // Previous frame for delta encoding
    ocr_engine: Option<TesseractEngine>, // Lazy init OCR
    config: VisionConfig,
}

impl FrameProcessor {
    /// Screenshot → Preprocessed result (metadata + conditional image)
    pub fn process(&mut self, raw: DynamicImage, trigger: &CaptureEvent) -> ProcessedFrame {
        // 1. Metadata is always extracted
        let metadata = FrameMetadata {
            timestamp: Utc::now(),
            trigger_type: trigger.trigger_type.clone(),
            app_name: trigger.app_name.clone(),
            window_title: sanitize_title(&trigger.window_title),
            resolution: (raw.width(), raw.height()),
            importance: trigger.importance_score,
        };

        // 2. Image preprocessing branching by importance
        let image_payload = match trigger.importance_score {
            s if s >= 0.8 => {
                // Critical (error, important event): High quality + OCR
                let ocr_text = self.run_local_ocr(&raw);
                let encoded = encode_webp(&raw, WebPQuality::High);  // ~80%
                Some(ImagePayload::Full {
                    data: encoded,
                    format: ImageFormat::WebP,
                    ocr_text,
                })
            }
            s if s >= 0.5 => {
                // High (context change): Delta encoding
                let delta = self.compute_delta(&raw);
                if delta.changed_ratio > 0.05 {  // Only if >5% changed
                    let encoded = encode_webp_region(&delta.image, WebPQuality::Medium);
                    Some(ImagePayload::Delta {
                        data: encoded,
                        region: delta.bounds,
                        changed_ratio: delta.changed_ratio,
                    })
                } else {
                    None  // Minimal change → metadata only
                }
            }
            s if s >= 0.3 => {
                // Normal: Thumbnail only
                let thumb = fast_resize(&raw, 480, 270);
                let encoded = encode_webp(&thumb, WebPQuality::Low);  // ~60%
                Some(ImagePayload::Thumbnail {
                    data: encoded,
                    width: 480,
                    height: 270,
                })
            }
            _ => None,  // Low: Metadata only
        };

        // 3. Save current frame for next delta comparison
        self.prev_frame = Some(raw);

        ProcessedFrame { metadata, image_payload }
    }
}
```

---

## Delta Encoding (delta.rs)

```rust
const TILE_SIZE: u32 = 16;  // 16×16 tile-based comparison

/// Extracts only changed regions by comparing with previous frame
pub fn compute_delta(prev: &DynamicImage, curr: &DynamicImage) -> DeltaRegion {
    let prev_buf = prev.to_rgba8();
    let curr_buf = curr.to_rgba8();

    let mut changed_tiles: Vec<(u32, u32)> = Vec::new();
    let total_tiles = (curr_buf.width() / TILE_SIZE) * (curr_buf.height() / TILE_SIZE);

    for ty in (0..curr_buf.height()).step_by(TILE_SIZE as usize) {
        for tx in (0..curr_buf.width()).step_by(TILE_SIZE as usize) {
            if tile_differs(&prev_buf, &curr_buf, tx, ty, TILE_SIZE) {
                changed_tiles.push((tx, ty));
            }
        }
    }

    let bounds = bounding_box(&changed_tiles, TILE_SIZE);
    let cropped = curr.crop_imm(bounds.x, bounds.y, bounds.w, bounds.h);

    DeltaRegion {
        bounds,
        image: cropped,
        changed_ratio: changed_tiles.len() as f32 / total_tiles as f32,
    }
}

/// Tile comparison: Determined as changed when pixel difference exceeds threshold
fn tile_differs(prev: &RgbaImage, curr: &RgbaImage, tx: u32, ty: u32, size: u32) -> bool {
    let threshold: u32 = 30;  // RGB difference sum threshold
    for dy in 0..size.min(curr.height() - ty) {
        for dx in 0..size.min(curr.width() - tx) {
            let p = prev.get_pixel(tx + dx, ty + dy);
            let c = curr.get_pixel(tx + dx, ty + dy);
            let diff = (p[0] as i32 - c[0] as i32).unsigned_abs()
                     + (p[1] as i32 - c[1] as i32).unsigned_abs()
                     + (p[2] as i32 - c[2] as i32).unsigned_abs();
            if diff > threshold {
                return true;
            }
        }
    }
    false
}
```

---

## Encoding/Decoding (encoder.rs)

```rust
pub enum WebPQuality {
    Low,     // 60% — For thumbnails
    Medium,  // 75% — For delta
    High,    // 85% — For full frames
}

/// WebP encoding (~30% savings vs JPEG, transparency support)
pub fn encode_webp(image: &DynamicImage, quality: WebPQuality) -> Vec<u8> {
    let q = match quality {
        WebPQuality::Low => 60.0,
        WebPQuality::Medium => 75.0,
        WebPQuality::High => 85.0,
    };
    let encoder = webp::Encoder::from_image(image).unwrap();
    encoder.encode(q).to_vec()
}

/// Adaptive format selection: Optimal format based on size
pub fn encode_adaptive(image: &DynamicImage, max_bytes: usize) -> EncodedImage {
    // 1st attempt: WebP
    let webp_data = encode_webp(image, WebPQuality::High);
    if webp_data.len() <= max_bytes {
        return EncodedImage { data: webp_data, format: ImageFormat::WebP };
    }

    // 2nd attempt: Lower WebP quality
    let webp_low = encode_webp(image, WebPQuality::Medium);
    if webp_low.len() <= max_bytes {
        return EncodedImage { data: webp_low, format: ImageFormat::WebP };
    }

    // 3rd attempt: Resize + WebP
    let half = fast_resize(image, image.width() / 2, image.height() / 2);
    let resized = encode_webp(&half, WebPQuality::Medium);
    EncodedImage { data: resized, format: ImageFormat::WebP }
}
```

---

## Local OCR — Edge Metadata Extraction (ocr.rs)

```rust
/// Tesseract-based local OCR — performs text metadata extraction only, not full analysis
/// Server handles additional analysis (Entity Extraction, Document Type Detection)
pub struct OcrEngine {
    tess: Tesseract,
}

impl OcrEngine {
    pub fn new() -> Result<Self, VisionError> {
        let tess = Tesseract::new(None, Some("eng+kor"))?;
        Ok(Self { tess })
    }

    /// Extracts text + confidence from image
    pub fn extract_text(&mut self, image: &DynamicImage) -> OcrResult {
        let png_bytes = image_to_png_bytes(image);
        self.tess.set_image_from_mem(&png_bytes);
        self.tess.recognize();

        OcrResult {
            text: self.tess.get_text().trim().to_string(),
            confidence: self.tess.mean_text_conf() as f32 / 100.0,
        }
    }
}

/// Optional handling so it works even when OCR is disabled
pub fn try_ocr(engine: &mut Option<OcrEngine>, image: &DynamicImage) -> Option<String> {
    engine.as_mut()
        .and_then(|e| {
            let result = e.extract_text(image);
            if result.confidence > 0.3 && !result.text.is_empty() {
                Some(result.text)
            } else {
                None
            }
        })
}
```

---

## Smart Capture Trigger (trigger.rs)

```rust
/// Event-based capture — not continuous capture
pub struct CaptureTrigger {
    last_capture: HashMap<TriggerType, Instant>,
    throttle: Duration,  // Default 5 seconds
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum TriggerType {
    WindowChange,         // Active window changed
    ErrorDetected,        // Error/exception detected (window title pattern)
    SignificantAction,    // Double-click, right-click
    FormSubmission,       // Enter + form/input context
    ContextSwitch,        // App switch (IDE → browser, etc.)
    ScheduledCheck,       // Periodic status check (60s)
}

impl CaptureTrigger {
    /// Determines whether capture is needed + assigns importance score
    pub fn should_capture(&mut self, event: &ContextEvent) -> Option<CaptureEvent> {
        let trigger_type = classify_trigger(event);
        let importance = score_importance(&trigger_type, event);

        // Throttling: Same trigger type requires minimum 5s interval
        if let Some(last) = self.last_capture.get(&trigger_type) {
            if last.elapsed() < self.throttle {
                return None;
            }
        }

        self.last_capture.insert(trigger_type.clone(), Instant::now());

        Some(CaptureEvent {
            trigger_type,
            importance_score: importance,
            app_name: event.app_name.clone(),
            window_title: event.window_title.clone(),
        })
    }
}

/// Importance score (0.0-1.0)
fn score_importance(trigger: &TriggerType, event: &ContextEvent) -> f32 {
    let base = match trigger {
        TriggerType::ErrorDetected => 0.9,
        TriggerType::FormSubmission => 0.8,
        TriggerType::ContextSwitch => 0.6,
        TriggerType::SignificantAction => 0.7,
        TriggerType::WindowChange => 0.4,
        TriggerType::ScheduledCheck => 0.2,
    };

    // Window title contains error pattern → boost
    let title_boost = if event.window_title.to_lowercase()
        .contains("error") || event.window_title.contains("exception") {
        0.2
    } else {
        0.0
    };

    (base + title_boost).min(1.0)
}
```

---

## Rewind Timeline (timeline.rs)

```rust
/// Frame index stored in local SQLite → Timeline browsing in UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameIndex {
    pub frame_id: String,
    pub timestamp: DateTime<Utc>,
    pub trigger_type: TriggerType,
    pub app_name: String,
    pub window_title: String,
    pub thumbnail_path: PathBuf,        // Local thumbnail (always saved)
    pub full_path: Option<PathBuf>,     // Original (within retention period)
    pub ocr_text: Option<String>,       // Edge OCR text
    pub importance: f32,
    pub uploaded: bool,                 // Whether sent to server
}

pub struct Timeline {
    db: rusqlite::Connection,
    thumbnail_dir: PathBuf,
    retention: Duration,                // Default 24 hours
    max_storage_mb: u64,                // Default 500MB
}

impl Timeline {
    /// Query frames by time range (rewind)
    pub fn get_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<FrameIndex> {
        // SQLite: SELECT * FROM frames WHERE timestamp BETWEEN ? AND ? ORDER BY timestamp
    }

    /// Filter by app
    pub fn get_by_app(&self, app_name: &str, limit: usize) -> Vec<FrameIndex> {
        // SQLite: SELECT * FROM frames WHERE app_name = ? ORDER BY timestamp DESC LIMIT ?
    }

    /// Text search (from OCR results)
    pub fn search_text(&self, query: &str, limit: usize) -> Vec<FrameIndex> {
        // SQLite FTS5: SELECT * FROM frames WHERE ocr_text MATCH ?
    }

    /// Apply retention policy (delete old frames + thumbnails)
    pub fn enforce_retention(&self) -> usize {
        // Delete oldest first when exceeding 24 hours or 500MB
    }
}
```

---

## Server Transmission Payload

```rust
/// Context data sent to server (metadata + conditional image)
#[derive(Debug, Serialize)]
pub struct ContextUpload {
    // Always included
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: FrameMetadata,

    // Conditional: Edge OCR text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_text: Option<String>,

    // Conditional: Preprocessed image
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImagePayload>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ImagePayload {
    /// Full frame (error, important events) — WebP ~80%
    Full {
        data: String,  // Base64
        format: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ocr_text: Option<String>,
    },
    /// Changed regions only (delta) — WebP ~75%
    Delta {
        data: String,  // Base64
        region: Rect,
        changed_ratio: f32,
    },
    /// Thumbnail (normal context) — WebP ~60%
    Thumbnail {
        data: String,  // Base64
        width: u32,
        height: u32,
    },
}
```

---

## Crate Dependencies (oneshim-vision)

```toml
[dependencies]
oneshim-core = { path = "../oneshim-core" }
image = { workspace = true }
fast_image_resize = { workspace = true }
webp = { workspace = true }
xcap = { workspace = true }
base64 = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
tracing = { workspace = true }

# OCR (optional — builds even without Tesseract installed)
[dependencies.leptess]
version = "0.14"
optional = true

[features]
default = []
ocr = ["leptess"]  # cargo build --features ocr
```

---

## Full Pipeline Flow

```
[Scheduler] Monitor loop (1 second)
     │
     ├─ Context change detected
     │      │
     │      └─ [CaptureTrigger] Capture needed? (5s throttle)
     │              │
     │              ├─ No → Queue metadata only
     │              │
     │              └─ Yes → [xcap] Screen capture
     │                         │
     │                    [FrameProcessor] Edge preprocessing
     │                         │
     │                    ┌────┴────────────────┐
     │                    │ Importance branching  │
     │                    ├─ ≥0.8: Full + OCR    │ ~50-150KB
     │                    ├─ ≥0.5: Delta         │ ~5-50KB
     │                    ├─ ≥0.3: Thumbnail     │ ~10-30KB
     │                    └─ <0.3: Meta only     │ ~0.5KB
     │                         │
     │                    [Timeline] Local storage
     │                    (thumbnail + frame index)
     │                         │
     └─ [BatchUploader] Batch transmission (10s interval)
              │
              ├─ HTTP POST /user_context/sync/batch
              │  { metadata + ocr_text + image_payload }
              │
              └─ Server: Analysis → Suggestion generation → SSE push
                                              │
                                         [SseClient] Reception
                                              │
                                    [SuggestionReceiver] Processing
                                              │
                                    Tray notification / Suggestion popup
```

---

## Open-Source License Compatibility

| Crate | License | OSS Compatible |
|-------|---------|---------------|
| `image` | MIT/Apache-2.0 | ✅ |
| `fast_image_resize` | MIT | ✅ |
| `webp` | MIT | ✅ |
| `xcap` | MIT | ✅ |
| `leptess` (Tesseract) | Apache-2.0 | ✅ |
| `base64` | MIT/Apache-2.0 | ✅ |
| `rusqlite` | MIT | ✅ |

All dependencies are MIT/Apache-2.0 — **no GPL contamination**.
