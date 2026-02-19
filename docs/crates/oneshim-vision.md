# oneshim-vision

Edge 이미지 처리를 담당하는 크레이트. 스크린 캡처, 델타 인코딩, 압축, OCR을 클라이언트에서 수행.

## 역할

- **스크린 캡처**: 멀티모니터 지원, 활성 창 캡처
- **델타 인코딩**: 변경 영역만 추출하여 전송량 최소화
- **적응형 처리**: 중요도에 따른 처리 수준 조절
- **개인정보 보호**: PII 필터링

## 디렉토리 구조

```
oneshim-vision/src/
├── lib.rs         # 크레이트 루트
├── capture.rs     # ScreenCapture - 화면 캡처
├── trigger.rs     # SmartCaptureTrigger - 캡처 결정
├── delta.rs       # DeltaEncoder - 변경 영역 추출
├── encoder.rs     # WebpEncoder - 이미지 압축
├── thumbnail.rs   # ThumbnailGenerator - 썸네일 생성
├── processor.rs   # EdgeFrameProcessor - 통합 처리
├── ocr.rs         # OcrExtractor - 텍스트 추출
├── privacy.rs     # PrivacySanitizer - PII 필터링
└── timeline.rs    # FrameTimeline - 프레임 이력
```

## 핵심 개념: 중요도 기반 처리

이벤트 중요도에 따라 처리 수준을 조절하여 리소스 효율화:

| 중요도 | 처리 방식 | 사용 상황 |
|--------|----------|----------|
| ≥ 0.8 | Full + OCR | 창 전환, 중요 입력 |
| ≥ 0.5 | Delta 인코딩 | 일반 활동 |
| ≥ 0.3 | Thumbnail | 유휴 상태 |
| < 0.3 | Metadata only | 백그라운드 |

## 주요 컴포넌트

### ScreenCapture (capture.rs)

`xcap` 기반 화면 캡처:

```rust
pub struct ScreenCapture {
    monitors: Vec<Monitor>,
}

impl ScreenCapture {
    /// 전체 화면 캡처
    pub fn capture_screen(&self, monitor_index: usize) -> Result<CapturedFrame, CoreError>;

    /// 특정 창 캡처
    pub fn capture_window(&self, window_id: u64) -> Result<CapturedFrame, CoreError>;

    /// 특정 영역 캡처
    pub fn capture_region(&self, x: i32, y: i32, w: u32, h: u32) -> Result<CapturedFrame, CoreError>;
}
```

### SmartCaptureTrigger (trigger.rs)

캡처 여부 및 처리 수준 결정 (`CaptureTrigger` 포트):

```rust
pub struct SmartCaptureTrigger {
    throttle_ms: u64,
    last_capture: RwLock<Option<Instant>>,
}

impl CaptureTrigger for SmartCaptureTrigger {
    async fn should_capture(&self, event: &ContextEvent) -> Result<CaptureDecision, CoreError> {
        // 1. 쓰로틀링 체크
        // 2. 이벤트 타입별 중요도 계산
        // 3. CaptureDecision 반환
    }
}
```

**이벤트 중요도 매핑**:
```rust
fn calculate_importance(event: &ContextEvent) -> f64 {
    match event.event_type {
        EventType::WindowFocus => 0.9,
        EventType::ApplicationSwitch => 0.85,
        EventType::KeyboardInput => 0.7,
        EventType::MouseClick => 0.6,
        EventType::MouseMove => 0.2,
        EventType::Idle => 0.1,
        _ => 0.5,
    }
}
```

### DeltaEncoder (delta.rs)

16x16 타일 기반 변경 영역 추출:

```rust
pub struct DeltaEncoder {
    tile_size: usize,  // 기본 16
    threshold: f64,    // 변경 감지 임계값
}

impl DeltaEncoder {
    /// 두 프레임 비교, 변경된 타일만 추출
    pub fn encode(&self, prev: &[u8], curr: &[u8], width: u32, height: u32)
        -> Result<DeltaFrame, CoreError>;
}

pub struct DeltaFrame {
    pub changed_tiles: Vec<Tile>,
    pub tile_positions: Vec<(u32, u32)>,
    pub compression_ratio: f64,
}
```

**알고리즘**:
1. 이미지를 16x16 타일로 분할
2. 각 타일의 해시 비교
3. 변경된 타일만 수집
4. 타일 위치 정보와 함께 반환

### WebpEncoder (encoder.rs)

WebP 포맷 인코딩:

```rust
pub struct WebpEncoder;

pub enum QualityLevel {
    Low,     // 50% - 썸네일용
    Medium,  // 75% - 일반
    High,    // 90% - 고품질
}

impl WebpEncoder {
    pub fn encode(&self, frame: &CapturedFrame, quality: QualityLevel)
        -> Result<Vec<u8>, CoreError>;
}
```

### ThumbnailGenerator (thumbnail.rs)

`fast_image_resize` 기반 고속 리사이즈:

```rust
pub struct ThumbnailGenerator {
    width: u32,   // 기본 480
    height: u32,  // 기본 270
}

impl ThumbnailGenerator {
    pub fn generate(&self, frame: &CapturedFrame) -> Result<Vec<u8>, CoreError>;
}
```

### EdgeFrameProcessor (processor.rs)

통합 처리 파이프라인 (`FrameProcessor` 포트):

```rust
pub struct EdgeFrameProcessor {
    delta_encoder: DeltaEncoder,
    webp_encoder: WebpEncoder,
    thumbnail_gen: ThumbnailGenerator,
    ocr_extractor: Option<OcrExtractor>,
    privacy_sanitizer: PrivacySanitizer,
    prev_frame: RwLock<Option<Vec<u8>>>,
}

impl FrameProcessor for EdgeFrameProcessor {
    async fn process(&self, frame: CapturedFrame) -> Result<ProcessedFrame, CoreError> {
        let importance = frame.metadata.importance;

        let processed = if importance >= 0.8 {
            self.process_full_with_ocr(frame).await?
        } else if importance >= 0.5 {
            self.process_delta(frame).await?
        } else if importance >= 0.3 {
            self.process_thumbnail(frame).await?
        } else {
            self.process_metadata_only(frame)?
        };

        Ok(self.privacy_sanitizer.sanitize(processed)?)
    }
}
```

### OcrExtractor (ocr.rs)

Tesseract 기반 OCR (선택적 기능):

```rust
#[cfg(feature = "ocr")]
pub struct OcrExtractor {
    tesseract: leptess::LepTess,
}

impl OcrExtractor {
    pub fn extract_text(&self, image: &[u8]) -> Result<String, CoreError>;
}
```

### PrivacySanitizer (privacy.rs)

PII 자동 필터링:

```rust
pub struct PrivacySanitizer {
    patterns: Vec<Regex>,
}

impl PrivacySanitizer {
    pub fn sanitize(&self, frame: ProcessedFrame) -> Result<ProcessedFrame, CoreError>;
}
```

**필터링 대상**:
- 이메일 주소
- 신용카드 번호
- 주민등록번호 (한국)
- 파일 경로 내 사용자명

## 처리 파이프라인

```
┌─────────────┐    ┌────────────────┐    ┌───────────────┐
│  ContextEvent │──▶│ CaptureTrigger │──▶│ ScreenCapture │
└─────────────┘    └────────────────┘    └───────────────┘
                          │                      │
                          ▼                      ▼
                   CaptureDecision         CapturedFrame
                          │                      │
                          └───────────┬──────────┘
                                      ▼
                            ┌─────────────────┐
                            │ FrameProcessor  │
                            │  (importance별)  │
                            └─────────────────┘
                                      │
                    ┌─────────────────┼─────────────────┐
                    ▼                 ▼                 ▼
              ┌──────────┐    ┌─────────────┐    ┌───────────┐
              │ Full+OCR │    │ Delta Only  │    │ Thumbnail │
              └──────────┘    └─────────────┘    └───────────┘
                    │                 │                 │
                    └─────────────────┼─────────────────┘
                                      ▼
                            ┌─────────────────┐
                            │ PrivacySanitizer│
                            └─────────────────┘
                                      │
                                      ▼
                              ProcessedFrame
```

## 의존성

- `xcap`: 크로스플랫폼 화면 캡처
- `image`: 이미지 처리
- `fast_image_resize`: 고속 리사이즈
- `webp`: WebP 인코딩
- `leptess`: Tesseract OCR (optional)
- `regex`: PII 패턴 매칭

## 성능 최적화

1. **타일 기반 델타**: 전체 프레임 대신 변경 영역만 전송
2. **적응형 품질**: 중요도에 따른 압축률 조절
3. **쓰로틀링**: 최소 캡처 간격으로 CPU 부하 제한
4. **비동기 처리**: OCR 등 무거운 작업은 별도 태스크

## 테스트

```rust
#[test]
fn test_delta_encoding() {
    let encoder = DeltaEncoder::new(16, 0.01);

    // 동일 프레임: 변경 없음
    let frame1 = vec![0u8; 1024];
    let frame2 = frame1.clone();
    let delta = encoder.encode(&frame1, &frame2, 32, 32).unwrap();
    assert!(delta.changed_tiles.is_empty());

    // 일부 변경: 해당 타일만 포함
    let mut frame3 = frame1.clone();
    frame3[0] = 255;
    let delta = encoder.encode(&frame1, &frame3, 32, 32).unwrap();
    assert_eq!(delta.changed_tiles.len(), 1);
}
```
