[English](./02-project-structure.md) | [한국어](./02-project-structure.ko.md)

# 2. 프로젝트 구조 + Crate 의존성

[← 전환 근거](./01-rationale.ko.md) | [모듈 매핑 →](./03-module-mapping.ko.md)

---

## Workspace 구조 (8개 크레이트)

```
oneshim-client/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── .cargo/
│   └── config.toml               # 빌드 최적화 (LTO, strip 등)
├── crates/
│   ├── oneshim-core/             # 도메인 모델 + 인터페이스 (Port)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models/           # Pydantic → serde 구조체
│   │       │   ├── mod.rs
│   │       │   ├── context.rs    # UserContext, WindowInfo, ProcessInfo
│   │       │   ├── system.rs     # SystemMetrics, NetworkInfo, AlertInfo
│   │       │   ├── event.rs      # Event enum 계층
│   │       │   ├── telemetry.rs  # Metric, SessionMetrics
│   │       │   ├── suggestion.rs # Suggestion, SuggestionFeedback
│   │       │   ├── session.rs    # SessionInfo, ConnectionHealth
│   │       │   └── frame.rs      # FrameMetadata, ImagePayload, DeltaRegion
│   │       ├── ports/            # 인터페이스 (trait)
│   │       │   ├── mod.rs
│   │       │   ├── monitor.rs    # SystemMonitor, ProcessMonitor, ActivityMonitor trait
│   │       │   ├── api_client.rs # ApiClient, SseClient trait
│   │       │   ├── storage.rs    # StorageService trait
│   │       │   ├── compressor.rs # Compressor trait
│   │       │   ├── notifier.rs   # DesktopNotifier trait (트레이 알림)
│   │       │   └── vision.rs     # FrameProcessor, CaptureTrigger, Timeline trait
│   │       ├── config.rs         # 설정 구조체
│   │       └── error.rs          # 에러 타입 (thiserror)
│   │
│   ├── oneshim-monitor/          # 시스템 모니터링 어댑터
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── system.rs         # sysinfo 기반 CPU/메모리/디스크
│   │       ├── process.rs        # 활성 창 + 프로세스 정보
│   │       ├── macos.rs          # macOS: CoreGraphics/AppKit FFI
│   │       ├── windows.rs        # Windows: Win32 API (winapi)
│   │       └── linux.rs          # Linux: X11/Wayland
│   │
│   ├── oneshim-vision/            # Edge 이미지 처리 (캡처 + 전처리)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── capture.rs        # 스크린 캡처 (xcap, 멀티모니터)
│   │       ├── processor.rs      # 프레임 전처리 오케스트레이터
│   │       ├── delta.rs          # 델타 인코딩 (변경 영역만 추출)
│   │       ├── encoder.rs        # 인코딩/디코딩 (WebP, JPEG, PNG)
│   │       ├── thumbnail.rs      # 썸네일 생성 (리사이즈)
│   │       ├── ocr.rs            # 로컬 OCR (Tesseract FFI)
│   │       ├── trigger.rs        # 스마트 캡처 트리거 (이벤트 기반)
│   │       ├── timeline.rs       # 프레임 인덱스 + 리와인드 지원
│   │       └── privacy.rs        # PII 필터링 (창 제목 새니타이징)
│   │
│   ├── oneshim-network/          # HTTP/SSE/WebSocket 어댑터
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── http_client.rs    # reqwest 기반 API 클라이언트
│   │       ├── sse_client.rs     # SSE 수신 (eventsource-client)
│   │       ├── ws_client.rs      # WebSocket (tokio-tungstenite)
│   │       ├── auth.rs           # JWT 토큰 관리 (로그인/갱신)
│   │       ├── batch_uploader.rs # 배치 업로드 + 재시도
│   │       └── compression.rs    # 압축 (flate2, zstd, lz4)
│   │
│   ├── oneshim-storage/          # 로컬 저장소 어댑터
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── sqlite.rs         # rusqlite 기반 이벤트 저장
│   │       ├── migration.rs      # 스키마 마이그레이션
│   │       └── retention.rs      # 보존 정책 (30일, 500MB)
│   │
│   ├── oneshim-suggestion/       # 제안 파이프라인 (핵심!)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── receiver.rs       # SSE → 제안 수신 + 파싱
│   │       ├── presenter.rs      # 제안 → UI/트레이 알림 변환
│   │       ├── feedback.rs       # 수락/거절 피드백 전송
│   │       ├── queue.rs          # 로컬 제안 큐 (우선순위)
│   │       └── history.rs        # 로컬 제안 이력 캐시
│   │
│   ├── oneshim-ui/               # 순수 Rust UI
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs            # 메인 앱 상태 + 이벤트 루프
│   │       ├── tray.rs           # 시스템 트레이 (tray-icon)
│   │       ├── views/
│   │       │   ├── mod.rs
│   │       │   ├── main_window.rs     # 메인 창
│   │       │   ├── suggestion_popup.rs # 제안 팝업/토스트
│   │       │   ├── context_panel.rs   # 현재 컨텍스트 표시
│   │       │   ├── settings.rs        # 설정 화면
│   │       │   ├── status_bar.rs      # 상태바 (연결, 메트릭)
│   │       │   └── timeline_view.rs   # 스크린샷 리와인드 타임라인
│   │       └── theme.rs          # 다크/라이트 테마
│   │
│   └── oneshim-app/              # 앱 진입점 + DI + 오케스트레이션
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs           # 바이너리 진입점
│           ├── app.rs            # Application 구조체 (DI 역할)
│           ├── lifecycle.rs      # 시작/종료 관리
│           ├── scheduler.rs      # 주기적 작업 (모니터링, 동기화, 하트비트)
│           └── event_bus.rs      # 내부 이벤트 버스 (tokio::broadcast)
│
├── tests/                        # 통합 테스트
│   ├── api_integration_test.rs
│   ├── sse_integration_test.rs
│   ├── monitor_test.rs
│   ├── vision_pipeline_test.rs
│   └── suggestion_pipeline_test.rs
│
├── build.rs                      # 빌드 스크립트 (아이콘 임베딩 등)
└── README.md
```

---

## Cargo.toml (workspace)

```toml
[workspace]
members = [
    "crates/oneshim-core",
    "crates/oneshim-monitor",
    "crates/oneshim-vision",
    "crates/oneshim-network",
    "crates/oneshim-storage",
    "crates/oneshim-suggestion",
    "crates/oneshim-ui",
    "crates/oneshim-app",
]
resolver = "2"

[workspace.dependencies]
# 비동기 런타임
tokio = { version = "1", features = ["full"] }

# 직렬화
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# HTTP 클라이언트
reqwest = { version = "0.12", features = ["json", "gzip", "rustls-tls"] }

# SSE 수신
eventsource-client = "0.13"
# 대안: reqwest-eventsource

# WebSocket
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"] }

# 시스템 모니터링
sysinfo = "0.32"

# 로컬 DB
rusqlite = { version = "0.32", features = ["bundled"] }

# 압축
flate2 = "1"           # gzip
zstd = "0.13"
lz4_flex = "0.11"

# 이미지 처리 (Edge Processing)
image = "0.25"                    # 인코딩/디코딩 (PNG, JPEG, WebP, AVIF)
fast_image_resize = "4"           # SIMD 최적화 고속 리사이즈
webp = "0.3"                      # WebP 인코딩 (JPEG 대비 30% 절약)
xcap = "0.0.14"                   # 크로스플랫폼 스크린 캡처
leptess = "0.14"                  # Tesseract OCR 바인딩 (로컬 텍스트 추출)
base64 = "0.22"                   # 이미지 바이너리 → Base64 전송

# UI
iced = { version = "0.13", features = ["tokio"] }
# 또는: egui + eframe (둘 중 선택)

# 시스템 트레이
tray-icon = "0.19"
# macOS/Windows 네이티브 알림
notify-rust = "4"

# 에러 처리
thiserror = "2"
anyhow = "1"

# 로깅
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# 설정
config = "0.14"
directories = "5"      # 플랫폼별 앱 디렉토리

# 유틸리티
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
```

## 플랫폼별 의존성

```toml
# macOS
[target.'cfg(target_os = "macos")'.dependencies]
cocoa = "0.26"
objc = "0.2"
core-graphics = "0.24"

# Windows
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.58", features = [
    "Win32_UI_WindowsAndMessaging",
    "Win32_Foundation",
    "Win32_System_Threading",
] }
```
