# CLAUDE.md — client-rust

ONESHIM Rust desktop client. 10-crate Cargo workspace, Hexagonal Architecture.

## Essential Commands

```bash
# Build/Verify
cargo check --workspace
cargo test --workspace
cargo clippy --workspace
cargo fmt --check

# Release build
cargo build --release -p oneshim-app

# Run binary
cargo run -p oneshim-app

# Test specific crate
cargo test -p oneshim-core
cargo test -p oneshim-vision
```

## Workspace Structure

```
client-rust/
├── Cargo.toml              # Workspace root (resolver = "2")
├── .cargo/config.toml      # Build configuration
├── docs/
│   ├── architecture/   # ADR-only (ADR-001, ADR-002, ADR-003)
│   ├── guides/         # Playbooks/runbooks/how-to docs
│   └── research/       # Exploratory notes
└── crates/
    ├── oneshim-core/       # Domain models + port traits + errors + config
    ├── oneshim-network/    # JWT auth, HTTP/SSE/WebSocket, compression, batch upload
    ├── oneshim-suggestion/ # Suggestion reception (SSE), priority queue, feedback, history
    ├── oneshim-storage/    # SQLite storage + schema migration
    ├── oneshim-monitor/    # System metrics (sysinfo), active window, activity tracking
    ├── oneshim-vision/     # Screen capture, delta encoding, WebP, thumbnail, PII filter
    ├── oneshim-ui/         # iced UI, system tray, desktop notifications
    ├── oneshim-web/        # Local web dashboard — Axum REST API + React frontend
    ├── oneshim-automation/ # Automation control — policy-based command execution, audit logging
    └── oneshim-app/        # Binary entry point — DI wiring, scheduler, lifecycle
```

## Core Architecture Rules

### Hexagonal Architecture (Ports & Adapters)

`oneshim-core` defines all traits (ports) and models. The other 9 crates act as adapters.

```
oneshim-core  ←  oneshim-monitor
              ←  oneshim-vision
              ←  oneshim-network
              ←  oneshim-storage
              ←  oneshim-suggestion  ←  oneshim-network
              ←  oneshim-ui          ←  oneshim-suggestion
              ←  oneshim-automation
              ←  oneshim-app         ←  (all)
```

**Forbidden**: Direct dependency between adapter crates (e.g., monitor → storage). All cross-crate communication must go through `oneshim-core` traits.

**Exceptions**: `suggestion → network` (SSE reception), `ui → suggestion` (suggestion display)

### Error Strategy (ADR-001 §1)

- Library crates: `thiserror` — specific error enums
- Binary crate (`oneshim-app`): `anyhow::Result`
- External crate errors are wrapped using `#[from]`

### Async Trait Pattern (ADR-001 §2)

Apply `#[async_trait]` to all port traits. Required for `Arc<dyn PortTrait>` DI. All port traits use `&self` (not `&mut self`) — implementations needing mutable state use interior mutability (`Mutex`, `RwLock`, atomic types).

```rust
#[async_trait]
pub trait ApiClient: Send + Sync {
    async fn upload_context(&self, context: &ContextPayload) -> Result<(), CoreError>;
}
```

### DI Pattern (ADR-001 §3)

Constructor injection + `Arc<dyn T>`. No DI framework is used. Wiring is manually performed in `oneshim-app/src/main.rs`. All port implementations are wrapped directly in `Arc` — never `Arc<Mutex<Box<dyn T>>>`.

### Testing (ADR-001 §5)

Manual mock implementation (mockall is not used). Trait implementations inside `#[cfg(test)]` modules.

## Crate Summary

### oneshim-core (Foundation)
- `models/`: suggestion, event, frame, context, session, system_metrics, batch
- `ports/`: ApiClient, SseClient, StorageService, SystemMonitor, ProcessMonitor, ActivityMonitor, CaptureTrigger, FrameProcessor, DesktopNotifier, Compressor
- `error.rs`: `CoreError` (thiserror) — Network, RequestTimeout, RateLimit, ServiceUnavailable variants
- `config/`: `AppConfig` + section settings — directory module (ADR-003)
  - `mod.rs`: `AppConfig` struct + `Default` impl + helpers + re-exports
  - `enums.rs`: `PiiFilterLevel`, `Weekday`, `SandboxProfile`, `AiAccessMode`, `AiProviderType`, etc.
  - `sections.rs`: 20 config section structs (`NotificationConfig`, `TelemetryConfig`, `PrivacyConfig`, `ScheduleConfig`, `FileAccessConfig`, etc.) + `Default` impls
- `consent.rs`: `ConsentManager`, `ConsentPermissions`, `ConsentRecord` — GDPR Article 17/20 compliant
- `config_manager.rs`: JSON-based config file manager + platform-specific paths

### oneshim-network (Network Adapter)
- `auth.rs`: `TokenManager` — JWT login/refresh/logout, `RwLock<TokenState>`
- `http_client.rs`: `HttpApiClient` — REST API (impl ApiClient), timeout detection (`map_reqwest_error`)
- `sse_client.rs`: `SseStreamClient` — SSE stream + auto-reconnect (exponential backoff 1s→30s)
- `compression.rs`: `AdaptiveCompressor` — auto selection of gzip/zstd/lz4
- `batch_uploader.rs`: `BatchUploader` — Lock-free SegQueue + dynamic batch size + retry
- `ws_client.rs`: WebSocket client (tokio-tungstenite)
- `ai_llm_client.rs`: `RemoteLlmProvider` — AI LLM intent interpretation (branches based on AiProviderType)
- `ai_ocr_client.rs`: `RemoteOcrProvider` — AI OCR element extraction (branches based on AiProviderType)
- **gRPC Client** (`#[cfg(feature = "grpc")]`):
  - `grpc/mod.rs`: module exports + `GrpcConfig`
  - `grpc/auth_client.rs`: `GrpcAuthClient` — Login, Logout, RefreshToken, ValidateToken
  - `grpc/session_client.rs`: `GrpcSessionClient` — CreateSession, EndSession, Heartbeat
  - `grpc/context_client.rs`: `GrpcContextClient` — UploadBatch, SubscribeSuggestions, SendFeedback, ListSuggestions
  - `grpc/unified_client.rs`: `UnifiedClient` — gRPC + REST unified client, Feature Flag based switching

### oneshim-suggestion (Suggestion Pipeline)
- `receiver.rs`: SSE → `Suggestion` conversion + queue + notification
- `queue.rs`: `BTreeSet` priority queue (max 50, Critical > High > Medium > Low)
- `feedback.rs`: Accept/Reject → HTTP POST
- `presenter.rs`: `SuggestionView` — UI data mapping
- `history.rs`: FIFO history cache

### oneshim-storage (Local Storage)
- `sqlite.rs`: `SqliteStorage` (impl StorageService) — WAL mode + PRAGMA optimizations
- `migration.rs`: schema V1-V7 (events, frames, work_sessions, interruptions, focus_metrics, local_suggestions)
- `frame_storage.rs`: Frame image file storage + retention policy + buffer pool + parallel I/O
- Retention Policy: 30 days, 500MB
- Performance optimization: compound indexes, batch inserts, memory cache, ArrayQueue buffer pool

### oneshim-monitor (System Monitoring)
- `system.rs`: `SysInfoMonitor` — CPU/Memory/Disk/Network (sysinfo 0.38)
- `process.rs`: `ProcessTracker` — active process/window + `get_detailed_processes()`
- `macos.rs`: macOS specific (`#[cfg(target_os = "macos")]`) — osascript
- `windows.rs`: Windows specific (`#[cfg(target_os = "windows")]`) — Win32 GetForegroundWindow + sysinfo
- `linux.rs`: Linux specific (`#[cfg(target_os = "linux")]`) — xdotool/xprintidle (X11), Wayland XWayland fallback
- `activity.rs`: `ActivityTracker` — Idle detection
- `input_activity.rs`: `InputActivityCollector` — Mouse/Keyboard pattern collection (atomic counters)
- `window_layout.rs`: `WindowLayoutTracker` — window layout change tracking

### oneshim-vision (Edge Image Processing)
- `capture.rs`: `ScreenCapture` — multi-monitor capture using xcap
- `trigger.rs`: `SmartCaptureTrigger` (impl CaptureTrigger) — event classification + importance + throttle, interior mutability (`Mutex<TriggerState>`)
- `delta.rs`: 16x16 tile comparison → changed region extraction (pointer-based fast pixel access)
- `encoder.rs`: WebP encoding (Low/Medium/High quality) + stat-based quality prediction
- `thumbnail.rs`: fast_image_resize + LRU caching (100 entries, FNV-1a hash)
- `processor.rs`: `EdgeFrameProcessor` (impl FrameProcessor) — branches by importance, interior mutability (`Mutex<Option<DynamicImage>>` for prev_frame)
  - >= 0.8: Full + OCR
  - >= 0.5: Delta
  - >= 0.3: Thumbnail
  - < 0.3: Metadata only
- `ocr.rs`: `OcrExtractor` — leptess(Tesseract) OCR (`#[cfg(feature = "ocr")]`), async support
- `privacy.rs`: PII filter levels (Off/Basic/Standard/Strict cascaded inheritance), sensitive app auto-detection, phone/API key/IP/email/credit card/SSN/file path masking
- `timeline.rs`: In-memory frame timeline + filters

### oneshim-ui (Desktop UI)
- `tray.rs`: System tray (tray-icon)
- `notifier.rs`: `DesktopNotifierImpl` (impl DesktopNotifier) — notify-rust
- `theme.rs`: Dark/Light theme
- `views/`: suggestion_popup, main_window, status_bar, context_panel, timeline_view, settings

### oneshim-web (Local Web Dashboard)
- `lib.rs`: `WebServer` — Axum 0.8 HTTP server + graceful shutdown
- `routes.rs`: REST API route definitions (16+ endpoints)
- `handlers/`: metrics, processes, idle, sessions, frames, events, stats, tags, focus
- `embedded.rs`: static file serving for React frontend using rust-embed
- `error.rs`: `ApiError` — JSON error responses
- `frontend/`: React 18 + Vite + Tailwind CSS + Recharts + FocusWidget

### oneshim-automation (Automation Control)
- `controller/`: `AutomationController` — directory module (ADR-003)
  - `mod.rs`: struct + builders + validators + re-exports
  - `types.rs`: `AutomationCommand`, `CommandResult`, `WorkflowResult`, etc.
  - `intent.rs`: intent execution + scene analysis methods
  - `preset.rs`: workflow/preset execution methods
- `policy/`: `PolicyClient` — directory module (ADR-003)
  - `mod.rs`: public API + re-exports
  - `models.rs`: `AuditLevel`, `ExecutionPolicy`, `PolicyCache`, `ProcessOutput`
  - `token.rs`: token generation, parsing, signature verification
- `audit.rs`: `AuditLogger` — local VecDeque buffer + batched audit logs transmission, buffer overflow management

### oneshim-app (Orchestrator, Binary)
- `main.rs`: tokio runtime + tracing + complete DI wiring + spawned tasks
- `scheduler/`: 9-loop scheduler — directory module (ADR-003)
  - `mod.rs`: `Scheduler` struct + `run()` orchestrator + re-exports
  - `config.rs`: `SchedulerConfig`, `PlatformEgressPolicy`, constants
  - `loops.rs`: 9 loop body functions (monitor, metrics, process, sync, heartbeat, aggregate, notification, focus, event snapshot)
- `notification_manager.rs`: Cooldown-based notification manager (idle, long session, high usage)
- `focus_analyzer/`: Focus analysis + local suggestion generation — directory module (ADR-003)
  - `mod.rs`: `FocusAnalyzer` struct + public API + re-exports
  - `models.rs`: `FocusAnalyzerConfig`, `SuggestionCooldowns`, `SessionTracker`
  - `suggestions.rs`: suggestion generators + cooldown logic + focus score calculation
- `lifecycle.rs`: SIGINT/SIGTERM handling, `tokio::sync::watch` shutdown channel
- `event_bus.rs`: `tokio::broadcast` internal event routing
- `autostart.rs`: Run at login — macOS LaunchAgent + Windows Registry
- `updater/`: GitHub Releases based auto-updater — directory module (ADR-003)
  - `mod.rs`: `Updater` struct + orchestrator + re-exports
  - `github.rs`: GitHub API: fetch releases, select asset, version floor
  - `install.rs`: download + decompress + binary replacement + signature verification
  - `state.rs`: last check time, update interval, version persistence

## Key Dependencies

| Category | Crate | Version |
|----------|-------|---------|
| Runtime | tokio | 1 (full) |
| HTTP | reqwest | 0.13 |
| Web Server | axum + tower-http | 0.8 / 0.6 |
| SSE | eventsource-stream | 0.2 |
| WebSocket | tokio-tungstenite | 0.28 |
| **gRPC** | tonic + tonic-prost + prost | 0.14 / 0.14 / 0.14 |
| DB | rusqlite | 0.38 (bundled, fallible_uint) |
| Monitoring | sysinfo | 0.38 |
| Image | image + fast_image_resize + webp + xcap | 0.25 / 6 / 0.3 / 0.8 |
| UI | iced | 0.14 |
| Windows API | windows-sys | 0.61 |
| Error | thiserror / anyhow | 2 / 1 |
| Serialization| serde + serde_json | 1 |
| Concurrency | crossbeam + parking_lot | 0.8 / 0.12 |
| Caching | lru | 0.16 |
| Auto Update | self_update + semver | 0.42 / 1 |
| Decompression| tar + zip | 0.4 / 2 |

## Coding Conventions

- Comments/Documentation: **English-first** (public docs require Korean companion docs for key guides)
- Rust edition: **2021**, Minimum version: **1.75**
- Code Formatting: `cargo fmt` (rustfmt default settings)
- Linting: `cargo clippy` — `dead_code` warnings are allowed only for variants intended for future use
- Testing: Write in `#[cfg(test)] mod tests` at the bottom of each module
- Logging: `tracing` macros (`debug!`, `info!`, `warn!`, `error!`)
- Serialization: `serde` derive — `Serialize, Deserialize` for all models

## Reference Documents

- [Docs Index](docs/README.md) — Document map by intent
- [ADR-001: Rust Client Architecture Patterns](docs/architecture/ADR-001-rust-client-architecture-patterns.md)
- [ADR-002: OS GUI Interaction Boundary and Runtime Split](docs/architecture/ADR-002-os-gui-interaction-boundary.md)
- [ADR-003: Directory Module Pattern for Large Source Files](docs/architecture/ADR-003-directory-module-pattern.md)
- [Documentation Policy](docs/DOCUMENTATION_POLICY.md) — English-primary + Korean companion docs + metrics consistency rules
- [Project Status](docs/STATUS.md) — single source of truth for mutable quality metrics
- [Migration Overview](docs/migration/README.md) — Migration plans and history
- [Server API](docs/migration/04-server-api.md) — 29 REST endpoints + gRPC RPCs
- [Migration Phases](docs/migration/05-migration-phases.md) — Phase 0-36 plans
- [Edge Vision](docs/migration/legacy/08-edge-vision.md) — Image processing details
- [gRPC Client Guide](docs/guides/grpc-client.md) — Rust gRPC client usage
- [Contributing Guide](CONTRIBUTING.md) — Rust development guide
- [Code of Conduct](CODE_OF_CONDUCT.md) — Contributor Covenant v2.1
- [Security Policy](SECURITY.md) — Vulnerability reporting process

## Current Status

- Phase 0-35 + Privacy & Permission Control System implementation completed (10 crates)
- Current quality metrics (test counts, pass/fail, lint/build status) are maintained in `docs/STATUS.md` as the single source of truth
- Actual adapters connected to all ports: `SmartCaptureTrigger`, `EdgeFrameProcessor`, `DesktopNotifierImpl`
- Windows active window detection implemented (`windows-sys` + `sysinfo`)
- 32 cross-crate integration tests (`crates/oneshim-app/tests/`)
- `cargo check/test/clippy/fmt` pass status is tracked in `docs/STATUS.md`
- **GA Ready**: CI/CD, installers, documentation completed
- **Web Dashboard**: Available at http://localhost:9090
- **Notification System**: Desktop notifications (idle, long session, high usage)

### Implementation Phases

Phase 4.5–37 + Privacy & Permission Control System completed. For detailed phase-by-phase changelog, see [docs/PHASE-HISTORY.md](docs/PHASE-HISTORY.md).

Key capabilities by phase:
- **Web Dashboard** (P9-13): Axum REST API, React frontend, SSE real-time, heatmap
- **Config & Notifications** (P14-15): JSON config persistence, cooldown notifications
- **Data Export & Theme** (P16-17): JSON/CSV export, dark/light theme
- **Frontend Polish** (P18-21): rust-embed SPA, i18n (ko/en), keyboard shortcuts, design system
- **Tags & Search** (P22-23): Tag CRUD, tag-based search
- **Reports & Backup** (P24-25): Activity reports, JSON backup/restore
- **E2E & Replay** (P26-27): 72 Playwright tests, session replay
- **Edge Intelligence** (P28, P30-33): Focus analyzer, SQLite perf, LRU cache, lock-free queue
- **Server Integration** (P34-37): HTTP retry, gRPC client, REST standardization, port fallback
- **Privacy** (Tier 1-3): Telemetry control, PII filter, GDPR consent, automation crate
