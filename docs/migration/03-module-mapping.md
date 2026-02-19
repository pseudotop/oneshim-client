# 3. Python → Rust 모듈 매핑

[← 프로젝트 구조](./02-project-structure.md) | [Server API →](./04-server-api.md)

---

## 핵심 모델 (Pydantic → serde)

| Python (client/core/models/) | Rust (oneshim-core/src/models/) |
|-----|------|
| `context.py` — ContextData, WindowInfo, ProcessInfo | `context.rs` — #[derive(Serialize, Deserialize)] |
| `system.py` — SystemMetrics, NetworkInfo | `system.rs` |
| `event.py` — Event, UserEvent, SystemEvent | `event.rs` — enum Event { User(..), System(..) } |
| `telemetry.py` — Metric, SessionMetrics | `telemetry.rs` |
| `batch.py` — EventBatch | event.rs 내 포함 |

## 인터페이스 (ABC → trait)

| Python (client/core/interfaces/) | Rust (oneshim-core/src/ports/) |
|-----|------|
| `monitor.py` — ISystemMonitor, IProcessMonitor | `monitor.rs` — trait SystemMonitor, trait ProcessMonitor |
| `communicator.py` — IAPIClient, IWebSocketClient | `api_client.rs` — trait ApiClient, trait SseClient |
| `storage.py` — IStorageService | `storage.rs` — trait StorageService |
| `compressor.py` — ICompressor | `compressor.rs` — trait Compressor |
| `batcher.py` — IBatchUploader | api_client.rs 내 포함 |

## 서비스 (Application Layer)

| Python (client/core/services/) | Rust Crate |
|-----|------|
| `context_service.py` | `oneshim-app/scheduler.rs` (모니터링 루프) |
| `telemetry_service.py` | `oneshim-network/batch_uploader.rs` |
| `event_service.py` | `oneshim-app/event_bus.rs` |
| `screenshot_service.py` | `oneshim-vision/capture.rs` + `processor.rs` ★ |
| `rum_service.py` | `oneshim-app/` (텔레메트리 통합) |

## 인프라 어댑터

| Python (client/infrastructure/) | Rust Crate |
|-----|------|
| `monitoring/base_monitor.py` (psutil) | `oneshim-monitor/system.rs` (sysinfo) |
| `monitoring/macos_monitor.py` (AppleScript) | `oneshim-monitor/macos.rs` (CoreGraphics FFI) |
| `monitoring/windows_monitor.py` (Win32) | `oneshim-monitor/windows.rs` (windows-rs) |
| `communication/api_service_impl.py` (aiohttp) | `oneshim-network/http_client.rs` (reqwest) |
| `communication/websocket_service.py` | `oneshim-network/ws_client.rs` (tokio-tungstenite) |
| `storage/sqlite_storage.py` (sqlite3) | `oneshim-storage/sqlite.rs` (rusqlite) |
| `compression/adaptive_compressor.py` | `oneshim-network/compression.rs` |
| `network/batch_uploader.py` | `oneshim-network/batch_uploader.rs` |
| `tracking/screenshot_trigger.py` | `oneshim-vision/trigger.rs` (스마트 캡처 트리거) |
| `tracking/pattern_analyzer.py` | **삭제** — 서버가 분석 담당 (Edge 메타추출만 유지) |

## UI (CustomTkinter → iced/egui)

| Python (client/ui/) | Rust (oneshim-ui/) |
|-----|------|
| `views/main_window.py` | `views/main_window.rs` |
| `widgets/status_bar.py` | `views/status_bar.rs` |
| `widgets/context_display.py` | `views/context_panel.rs` |
| `widgets/command_input.py` | main_window.rs 내 포함 |
| `widgets/tray_icon.py` | `tray.rs` (tray-icon 크레이트) |
| `widgets/settings_dialog.py` | `views/settings.rs` |
| **없음** (미구현) | `views/suggestion_popup.rs` ★ 신규 |
| **없음** (미구현) | `views/timeline_view.rs` ★ 신규 (리와인드) |
