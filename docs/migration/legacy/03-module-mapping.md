[English](./03-module-mapping.md) | [한국어](./03-module-mapping.ko.md)

# 3. Python → Rust Module Mapping

[← Project Structure](./02-project-structure.md) | [Server API →](../04-server-api.md)

---

## Core Models (Pydantic → serde)

| Python (client/core/models/) | Rust (oneshim-core/src/models/) |
|-----|------|
| `context.py` — ContextData, WindowInfo, ProcessInfo | `context.rs` — #[derive(Serialize, Deserialize)] |
| `system.py` — SystemMetrics, NetworkInfo | `system.rs` |
| `event.py` — Event, UserEvent, SystemEvent | `event.rs` — enum Event { User(..), System(..) } |
| `telemetry.py` — Metric, SessionMetrics | `telemetry.rs` |
| `batch.py` — EventBatch | Included in event.rs |

## Interfaces (ABC → trait)

| Python (client/core/interfaces/) | Rust (oneshim-core/src/ports/) |
|-----|------|
| `monitor.py` — ISystemMonitor, IProcessMonitor | `monitor.rs` — trait SystemMonitor, trait ProcessMonitor |
| `communicator.py` — IAPIClient, IWebSocketClient | `api_client.rs` — trait ApiClient, trait SseClient |
| `storage.py` — IStorageService | `storage.rs` — trait StorageService |
| `compressor.py` — ICompressor | `compressor.rs` — trait Compressor |
| `batcher.py` — IBatchUploader | Included in api_client.rs |

## Services (Application Layer)

| Python (client/core/services/) | Rust Crate |
|-----|------|
| `context_service.py` | `oneshim-app/scheduler.rs` (monitoring loop) |
| `telemetry_service.py` | `oneshim-network/batch_uploader.rs` |
| `event_service.py` | `oneshim-app/event_bus.rs` |
| `screenshot_service.py` | `oneshim-vision/capture.rs` + `processor.rs` ★ |
| `rum_service.py` | `oneshim-app/` (telemetry integration) |

## Infrastructure Adapters

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
| `tracking/screenshot_trigger.py` | `oneshim-vision/trigger.rs` (smart capture trigger) |
| `tracking/pattern_analyzer.py` | **Removed** — Server handles analysis (only edge metadata extraction retained) |

## UI (CustomTkinter → iced/egui)

| Python (client/ui/) | Rust (oneshim-ui/) |
|-----|------|
| `views/main_window.py` | `views/main_window.rs` |
| `widgets/status_bar.py` | `views/status_bar.rs` |
| `widgets/context_display.py` | `views/context_panel.rs` |
| `widgets/command_input.py` | Included in main_window.rs |
| `widgets/tray_icon.py` | `tray.rs` (tray-icon crate) |
| `widgets/settings_dialog.py` | `views/settings.rs` |
| **N/A** (not implemented) | `views/suggestion_popup.rs` ★ New |
| **N/A** (not implemented) | `views/timeline_view.rs` ★ New (rewind) |
