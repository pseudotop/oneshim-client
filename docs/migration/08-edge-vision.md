# 8. Edge 이미지 처리 파이프라인 (oneshim-vision)

[← 코드 스케치](./07-code-sketches.md) | [테스트 전략 →](./09-testing.md)

---

> **원칙**: 클라이언트는 엣지 컴퓨터처럼 동작한다. 전체 이미지 분석은 서버가 담당하고, 클라이언트는 **필수 메타데이터 추출 + 전처리된 이미지 전송**만 수행한다. 동영상이 아닌 **이벤트 기반 프레임 캡처 + 델타 인코딩**으로 throughput 문제를 회피한다.

## 설계 철학: 동영상 ❌ → 스마트 프레임 ✅

```
동영상 (H.264/VP9):
  - 30fps × 1080p = ~2-5 Mbps (지속적 대역폭 소모)
  - 디코딩에 GPU 필요
  - 프레임 단위 분석 어려움 (GOP 단위 디코딩)
  - 저장 공간 폭증

ONESHIM Edge 방식:
  - 이벤트 트리거 시에만 캡처 (5초 쓰로틀)
  - 변경 영역만 전송 (델타 인코딩) → 프레임당 ~5-50KB
  - 각 프레임 독립적 → 리와인드/랜덤 접근 가능
  - CPU만으로 처리, GPU 불필요
```

## 전송량 비교

| 방식 | 프레임당 크기 | 분당 (12회 캡처) | 시간당 |
|------|-------------|-----------------|--------|
| 원본 PNG (1920×1080) | ~3-6MB | ~36-72MB | ~2-4GB |
| 원본 JPEG 85% | ~150-300KB | ~1.8-3.6MB | ~108-216MB |
| **WebP 썸네일 (480×270)** | **~10-30KB** | **~120-360KB** | **~7-22MB** |
| **델타 (변경 영역만)** | **~5-50KB** | **~60-600KB** | **~4-36MB** |
| 메타데이터만 | ~0.5-1KB | ~6-12KB | ~360-720KB |
| **ONESHIM 혼합 (적응적)** | **~10-100KB** | **~200KB-1MB** | **~12-60MB** |
| 동영상 H.264 30fps | N/A | ~15-37MB | **~900MB-2.2GB** |

ONESHIM 방식은 동영상 대비 **1/30 ~ 1/100 수준**의 대역폭만 사용한다.

---

## 전처리 오케스트레이터 (processor.rs)

```rust
/// Edge Processing: 캡처 → 중요도 판단 → 조건별 전처리 → 전송 페이로드 생성
pub struct FrameProcessor {
    prev_frame: Option<DynamicImage>,   // 델타 인코딩용 이전 프레임
    ocr_engine: Option<TesseractEngine>, // lazy init OCR
    config: VisionConfig,
}

impl FrameProcessor {
    /// 스크린샷 → 전처리 결과 (메타데이터 + 조건부 이미지)
    pub fn process(&mut self, raw: DynamicImage, trigger: &CaptureEvent) -> ProcessedFrame {
        // 1. 메타데이터는 항상 추출
        let metadata = FrameMetadata {
            timestamp: Utc::now(),
            trigger_type: trigger.trigger_type.clone(),
            app_name: trigger.app_name.clone(),
            window_title: sanitize_title(&trigger.window_title),
            resolution: (raw.width(), raw.height()),
            importance: trigger.importance_score,
        };

        // 2. 중요도에 따라 이미지 전처리 분기
        let image_payload = match trigger.importance_score {
            s if s >= 0.8 => {
                // Critical (에러, 중요 이벤트): 고품질 + OCR
                let ocr_text = self.run_local_ocr(&raw);
                let encoded = encode_webp(&raw, WebPQuality::High);  // ~80%
                Some(ImagePayload::Full {
                    data: encoded,
                    format: ImageFormat::WebP,
                    ocr_text,
                })
            }
            s if s >= 0.5 => {
                // High (컨텍스트 변경): 델타 인코딩
                let delta = self.compute_delta(&raw);
                if delta.changed_ratio > 0.05 {  // 5% 이상 변경 시에만
                    let encoded = encode_webp_region(&delta.image, WebPQuality::Medium);
                    Some(ImagePayload::Delta {
                        data: encoded,
                        region: delta.bounds,
                        changed_ratio: delta.changed_ratio,
                    })
                } else {
                    None  // 변경 미미 → 메타만
                }
            }
            s if s >= 0.3 => {
                // Normal: 썸네일만
                let thumb = fast_resize(&raw, 480, 270);
                let encoded = encode_webp(&thumb, WebPQuality::Low);  // ~60%
                Some(ImagePayload::Thumbnail {
                    data: encoded,
                    width: 480,
                    height: 270,
                })
            }
            _ => None,  // Low: 메타데이터만 전송
        };

        // 3. 현재 프레임을 다음 델타 비교용으로 저장
        self.prev_frame = Some(raw);

        ProcessedFrame { metadata, image_payload }
    }
}
```

---

## 델타 인코딩 (delta.rs)

```rust
const TILE_SIZE: u32 = 16;  // 16×16 타일 단위 비교

/// 이전 프레임과 비교해서 변경된 영역만 추출
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

/// 타일 비교: 픽셀 차이 임계값 초과 시 변경으로 판정
fn tile_differs(prev: &RgbaImage, curr: &RgbaImage, tx: u32, ty: u32, size: u32) -> bool {
    let threshold: u32 = 30;  // RGB 차이 합산 임계값
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

## 인코딩/디코딩 (encoder.rs)

```rust
pub enum WebPQuality {
    Low,     // 60% — 썸네일용
    Medium,  // 75% — 델타용
    High,    // 85% — 전체 프레임용
}

/// WebP 인코딩 (JPEG 대비 ~30% 절약, 투명도 지원)
pub fn encode_webp(image: &DynamicImage, quality: WebPQuality) -> Vec<u8> {
    let q = match quality {
        WebPQuality::Low => 60.0,
        WebPQuality::Medium => 75.0,
        WebPQuality::High => 85.0,
    };
    let encoder = webp::Encoder::from_image(image).unwrap();
    encoder.encode(q).to_vec()
}

/// 적응적 포맷 선택: 크기에 따라 최적 포맷
pub fn encode_adaptive(image: &DynamicImage, max_bytes: usize) -> EncodedImage {
    // 1차: WebP 시도
    let webp_data = encode_webp(image, WebPQuality::High);
    if webp_data.len() <= max_bytes {
        return EncodedImage { data: webp_data, format: ImageFormat::WebP };
    }

    // 2차: WebP 품질 낮춤
    let webp_low = encode_webp(image, WebPQuality::Medium);
    if webp_low.len() <= max_bytes {
        return EncodedImage { data: webp_low, format: ImageFormat::WebP };
    }

    // 3차: 리사이즈 + WebP
    let half = fast_resize(image, image.width() / 2, image.height() / 2);
    let resized = encode_webp(&half, WebPQuality::Medium);
    EncodedImage { data: resized, format: ImageFormat::WebP }
}
```

---

## 로컬 OCR — Edge 메타데이터 추출 (ocr.rs)

```rust
/// Tesseract 기반 로컬 OCR — 전체 분석이 아닌 텍스트 메타 추출만 수행
/// 서버에서 추가 분석 (Entity Extraction, Document Type Detection) 진행
pub struct OcrEngine {
    tess: Tesseract,
}

impl OcrEngine {
    pub fn new() -> Result<Self, VisionError> {
        let tess = Tesseract::new(None, Some("eng+kor"))?;
        Ok(Self { tess })
    }

    /// 이미지에서 텍스트 + 신뢰도 추출
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

/// OCR 비활성화 시에도 동작하도록 Optional 처리
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

## 스마트 캡처 트리거 (trigger.rs)

```rust
/// 이벤트 기반 캡처 — 연속 캡처 아님
pub struct CaptureTrigger {
    last_capture: HashMap<TriggerType, Instant>,
    throttle: Duration,  // 기본 5초
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum TriggerType {
    WindowChange,         // 활성 창 변경
    ErrorDetected,        // 에러/예외 감지 (창 제목 패턴)
    SignificantAction,    // 더블클릭, 우클릭
    FormSubmission,       // Enter + form/input 컨텍스트
    ContextSwitch,        // 앱 전환 (IDE → 브라우저 등)
    ScheduledCheck,       // 주기적 상태 확인 (60초)
}

impl CaptureTrigger {
    /// 캡처 필요 여부 판단 + 중요도 점수 부여
    pub fn should_capture(&mut self, event: &ContextEvent) -> Option<CaptureEvent> {
        let trigger_type = classify_trigger(event);
        let importance = score_importance(&trigger_type, event);

        // 쓰로틀링: 동일 트리거 타입은 최소 5초 간격
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

/// 중요도 점수 (0.0-1.0)
fn score_importance(trigger: &TriggerType, event: &ContextEvent) -> f32 {
    let base = match trigger {
        TriggerType::ErrorDetected => 0.9,
        TriggerType::FormSubmission => 0.8,
        TriggerType::ContextSwitch => 0.6,
        TriggerType::SignificantAction => 0.7,
        TriggerType::WindowChange => 0.4,
        TriggerType::ScheduledCheck => 0.2,
    };

    // 창 제목에 에러 패턴 → 보정
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

## 리와인드 타임라인 (timeline.rs)

```rust
/// 로컬 SQLite에 프레임 인덱스 저장 → UI에서 타임라인 브라우징
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameIndex {
    pub frame_id: String,
    pub timestamp: DateTime<Utc>,
    pub trigger_type: TriggerType,
    pub app_name: String,
    pub window_title: String,
    pub thumbnail_path: PathBuf,        // 로컬 썸네일 (항상 저장)
    pub full_path: Option<PathBuf>,     // 원본 (보존 기간 내)
    pub ocr_text: Option<String>,       // Edge OCR 텍스트
    pub importance: f32,
    pub uploaded: bool,                 // 서버 전송 여부
}

pub struct Timeline {
    db: rusqlite::Connection,
    thumbnail_dir: PathBuf,
    retention: Duration,                // 기본 24시간
    max_storage_mb: u64,                // 기본 500MB
}

impl Timeline {
    /// 시간 범위로 프레임 조회 (리와인드)
    pub fn get_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<FrameIndex> {
        // SQLite: SELECT * FROM frames WHERE timestamp BETWEEN ? AND ? ORDER BY timestamp
    }

    /// 앱별 필터링
    pub fn get_by_app(&self, app_name: &str, limit: usize) -> Vec<FrameIndex> {
        // SQLite: SELECT * FROM frames WHERE app_name = ? ORDER BY timestamp DESC LIMIT ?
    }

    /// 텍스트 검색 (OCR 결과에서)
    pub fn search_text(&self, query: &str, limit: usize) -> Vec<FrameIndex> {
        // SQLite FTS5: SELECT * FROM frames WHERE ocr_text MATCH ?
    }

    /// 보존 정책 적용 (오래된 프레임 + 썸네일 삭제)
    pub fn enforce_retention(&self) -> usize {
        // 24시간 초과 + 500MB 초과 시 가장 오래된 것부터 삭제
    }
}
```

---

## 서버 전송 페이로드

```rust
/// 서버로 전송하는 컨텍스트 데이터 (메타 + 조건부 이미지)
#[derive(Debug, Serialize)]
pub struct ContextUpload {
    // 항상 포함
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: FrameMetadata,

    // 조건부: Edge OCR 텍스트
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_text: Option<String>,

    // 조건부: 전처리된 이미지
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImagePayload>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ImagePayload {
    /// 전체 프레임 (에러, 중요 이벤트) — WebP ~80%
    Full {
        data: String,  // Base64
        format: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ocr_text: Option<String>,
    },
    /// 변경 영역만 (델타) — WebP ~75%
    Delta {
        data: String,  // Base64
        region: Rect,
        changed_ratio: f32,
    },
    /// 썸네일 (일반 컨텍스트) — WebP ~60%
    Thumbnail {
        data: String,  // Base64
        width: u32,
        height: u32,
    },
}
```

---

## Crate 의존성 (oneshim-vision)

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

# OCR (optional — Tesseract 미설치 환경에서도 빌드 가능)
[dependencies.leptess]
version = "0.14"
optional = true

[features]
default = []
ocr = ["leptess"]  # cargo build --features ocr
```

---

## 전체 파이프라인 흐름

```
[스케줄러] 모니터링 루프 (1초)
     │
     ├─ 컨텍스트 변경 감지
     │      │
     │      └─ [CaptureTrigger] 캡처 필요? (5초 쓰로틀)
     │              │
     │              ├─ No → 메타데이터만 큐잉
     │              │
     │              └─ Yes → [xcap] 스크린 캡처
     │                         │
     │                    [FrameProcessor] Edge 전처리
     │                         │
     │                    ┌────┴────────────────┐
     │                    │  중요도별 분기       │
     │                    ├─ ≥0.8: Full + OCR   │ ~50-150KB
     │                    ├─ ≥0.5: Delta        │ ~5-50KB
     │                    ├─ ≥0.3: Thumbnail    │ ~10-30KB
     │                    └─ <0.3: Meta only    │ ~0.5KB
     │                         │
     │                    [Timeline] 로컬 저장
     │                    (썸네일 + 프레임 인덱스)
     │                         │
     └─ [BatchUploader] 배치 전송 (10초 주기)
              │
              ├─ HTTP POST /user_context/sync/batch
              │  { metadata + ocr_text + image_payload }
              │
              └─ 서버: 분석 → 제안 생성 → SSE 푸시
                                              │
                                         [SseClient] 수신
                                              │
                                    [SuggestionReceiver] 처리
                                              │
                                    트레이 알림 / 제안 팝업
```

---

## 오픈소스 라이선스 호환성

| 크레이트 | 라이선스 | 오픈소스 호환 |
|---------|---------|-------------|
| `image` | MIT/Apache-2.0 | ✅ |
| `fast_image_resize` | MIT | ✅ |
| `webp` | MIT | ✅ |
| `xcap` | MIT | ✅ |
| `leptess` (Tesseract) | Apache-2.0 | ✅ |
| `base64` | MIT/Apache-2.0 | ✅ |
| `rusqlite` | MIT | ✅ |

전체 의존성 MIT/Apache-2.0 — **GPL 오염 없음**.
