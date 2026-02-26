[English](./02-project-structure.md) | [한국어](./02-project-structure.ko.md)

# 2. Project Structure + Crate Dependencies

[← Rationale](./01-rationale.md) | [Module Mapping →](./03-module-mapping.md)

---

## Workspace Structure (8 Crates)

```
oneshim-client/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── .cargo/
│   └── config.toml               # Build optimizations (LTO, strip, etc.)
├── crates/
│   ├── oneshim-core/             # Domain models + interfaces (Ports)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models/           # Pydantic → serde structs
│   │       │   ├── mod.rs
│   │       │   ├── context.rs    # UserContext, WindowInfo, ProcessInfo
│   │       │   ├── system.rs     # SystemMetrics, NetworkInfo, AlertInfo
│   │       │   ├── event.rs      # Event enum hierarchy
│   │       │   ├── telemetry.rs  # Metric, SessionMetrics
│   │       │   ├── suggestion.rs # Suggestion, SuggestionFeedback
│   │       │   ├── session.rs    # SessionInfo, ConnectionHealth
│   │       │   └── frame.rs      # FrameMetadata, ImagePayload, DeltaRegion
│   │       ├── ports/            # Interfaces (traits)
│   │       │   ├── mod.rs
│   │       │   ├── monitor.rs    # SystemMonitor, ProcessMonitor, ActivityMonitor traits
│   │       │   ├── api_client.rs # ApiClient, SseClient traits
│   │       │   ├── storage.rs    # StorageService trait
│   │       │   ├── compressor.rs # Compressor trait
│   │       │   ├── notifier.rs   # DesktopNotifier trait (tray notifications)
│   │       │   └── vision.rs     # FrameProcessor, CaptureTrigger, Timeline traits
│   │       ├── config.rs         # Configuration structs
│   │       └── error.rs          # Error types (thiserror)
│   │
│   ├── oneshim-monitor/          # System monitoring adapter
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── system.rs         # sysinfo-based CPU/memory/disk
│   │       ├── process.rs        # Active window + process info
│   │       ├── macos.rs          # macOS: CoreGraphics/AppKit FFI
│   │       ├── windows.rs        # Windows: Win32 API (winapi)
│   │       └── linux.rs          # Linux: X11/Wayland
│   │
│   ├── oneshim-vision/            # Edge image processing (capture + preprocessing)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── capture.rs        # Screen capture (xcap, multi-monitor)
│   │       ├── processor.rs      # Frame preprocessing orchestrator
│   │       ├── delta.rs          # Delta encoding (extract changed regions only)
│   │       ├── encoder.rs        # Encoding/decoding (WebP, JPEG, PNG)
│   │       ├── thumbnail.rs      # Thumbnail generation (resize)
│   │       ├── ocr.rs            # Local OCR (Tesseract FFI)
│   │       ├── trigger.rs        # Smart capture trigger (event-based)
│   │       ├── timeline.rs       # Frame index + rewind support
│   │       └── privacy.rs        # PII filtering (window title sanitization)
│   │
│   ├── oneshim-network/          # HTTP/SSE/WebSocket adapter
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── http_client.rs    # reqwest-based API client
│   │       ├── sse_client.rs     # SSE reception (eventsource-client)
│   │       ├── ws_client.rs      # WebSocket (tokio-tungstenite)
│   │       ├── auth.rs           # JWT token management (login/refresh)
│   │       ├── batch_uploader.rs # Batch upload + retry
│   │       └── compression.rs    # Compression (flate2, zstd, lz4)
│   │
│   ├── oneshim-storage/          # Local storage adapter
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── sqlite.rs         # rusqlite-based event storage
│   │       ├── migration.rs      # Schema migration
│   │       └── retention.rs      # Retention policy (30 days, 500MB)
│   │
│   ├── oneshim-suggestion/       # Suggestion pipeline (core!)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── receiver.rs       # SSE → suggestion reception + parsing
│   │       ├── presenter.rs      # Suggestion → UI/tray notification conversion
│   │       ├── feedback.rs       # Accept/reject feedback submission
│   │       ├── queue.rs          # Local suggestion queue (priority)
│   │       └── history.rs        # Local suggestion history cache
│   │
│   ├── oneshim-ui/               # Pure Rust UI
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs            # Main app state + event loop
│   │       ├── tray.rs           # System tray (tray-icon)
│   │       ├── views/
│   │       │   ├── mod.rs
│   │       │   ├── main_window.rs     # Main window
│   │       │   ├── suggestion_popup.rs # Suggestion popup/toast
│   │       │   ├── context_panel.rs   # Current context display
│   │       │   ├── settings.rs        # Settings screen
│   │       │   ├── status_bar.rs      # Status bar (connection, metrics)
│   │       │   └── timeline_view.rs   # Screenshot rewind timeline
│   │       └── theme.rs          # Dark/light theme
│   │
│   └── oneshim-app/              # App entry point + DI + orchestration
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs           # Binary entry point
│           ├── app.rs            # Application struct (DI role)
│           ├── lifecycle.rs      # Start/shutdown management
│           ├── scheduler.rs      # Periodic tasks (monitoring, sync, heartbeat)
│           └── event_bus.rs      # Internal event bus (tokio::broadcast)
│
├── tests/                        # Integration tests
│   ├── api_integration_test.rs
│   ├── sse_integration_test.rs
│   ├── monitor_test.rs
│   ├── vision_pipeline_test.rs
│   └── suggestion_pipeline_test.rs
│
├── build.rs                      # Build script (icon embedding, etc.)
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
# Async runtime
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# HTTP client
reqwest = { version = "0.12", features = ["json", "gzip", "rustls-tls"] }

# SSE reception
eventsource-client = "0.13"
# Alternative: reqwest-eventsource

# WebSocket
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"] }

# System monitoring
sysinfo = "0.32"

# Local DB
rusqlite = { version = "0.32", features = ["bundled"] }

# Compression
flate2 = "1"           # gzip
zstd = "0.13"
lz4_flex = "0.11"

# Image processing (Edge Processing)
image = "0.25"                    # Encoding/decoding (PNG, JPEG, WebP, AVIF)
fast_image_resize = "4"           # SIMD-optimized high-speed resize
webp = "0.3"                      # WebP encoding (30% savings vs JPEG)
xcap = "0.0.14"                   # Cross-platform screen capture
leptess = "0.14"                  # Tesseract OCR bindings (local text extraction)
base64 = "0.22"                   # Image binary → Base64 transfer

# UI
iced = { version = "0.13", features = ["tokio"] }
# Or: egui + eframe (choose one)

# System tray
tray-icon = "0.19"
# macOS/Windows native notifications
notify-rust = "4"

# Error handling
thiserror = "2"
anyhow = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Configuration
config = "0.14"
directories = "5"      # Platform-specific app directories

# Utilities
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
```

## Platform-Specific Dependencies

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
