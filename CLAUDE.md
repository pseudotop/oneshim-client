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
├── docs/architecture/
│   └── ADR-001-rust-client-architecture-patterns.md
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

Apply `#[async_trait]` to all port traits. Required for `Arc<dyn PortTrait>` DI.

```rust
#[async_trait]
pub trait ApiClient: Send + Sync {
    async fn upload_context(&self, context: &ContextPayload) -> Result<(), CoreError>;
}
```

### DI Pattern (ADR-001 §3)

Constructor injection + `Arc<dyn T>`. No DI framework is used. Wiring is manually performed in `oneshim-app/src/main.rs`.

### Testing (ADR-001 §5)

Manual mock implementation (mockall is not used). Trait implementations inside `#[cfg(test)]` modules.

## Crate Summary

### oneshim-core (Foundation)
- `models/`: suggestion, event, frame, context, session, system_metrics, batch
- `ports/`: ApiClient, SseClient, StorageService, SystemMonitor, ProcessMonitor, ActivityMonitor, CaptureTrigger, FrameProcessor, DesktopNotifier, Compressor
- `error.rs`: `CoreError` (thiserror)
- `config.rs`: `AppConfig` + section settings (NotificationConfig, TelemetryConfig, PrivacyConfig, PiiFilterLevel, ScheduleConfig, FileAccessConfig) + `AiProviderType` (Anthropic/OpenAi/Generic)
- `consent.rs`: `ConsentManager`, `ConsentPermissions`, `ConsentRecord` — GDPR Article 17/20 compliant
- `config_manager.rs`: JSON-based config file manager + platform-specific paths

### oneshim-network (Network Adapter)
- `auth.rs`: `TokenManager` — JWT login/refresh/logout, `RwLock<TokenState>`
- `http_client.rs`: `HttpApiClient` — REST API (impl ApiClient)
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
- `system.rs`: `SysInfoMonitor` — CPU/Memory/Disk/Network (sysinfo 0.32)
- `process.rs`: `ProcessTracker` — active process/window + `get_detailed_processes()`
- `macos.rs`: macOS specific (`#[cfg(target_os = "macos")]`) — osascript
- `windows.rs`: Windows specific (`#[cfg(target_os = "windows")]`) — Win32 GetForegroundWindow + sysinfo
- `linux.rs`: Linux specific (`#[cfg(target_os = "linux")]`) — xdotool/xprintidle (X11), Wayland XWayland fallback
- `activity.rs`: `ActivityTracker` — Idle detection
- `input_activity.rs`: `InputActivityCollector` — Mouse/Keyboard pattern collection (atomic counters)
- `window_layout.rs`: `WindowLayoutTracker` — window layout change tracking

### oneshim-vision (Edge Image Processing)
- `capture.rs`: `ScreenCapture` — multi-monitor capture using xcap
- `trigger.rs`: `SmartCaptureTrigger` (impl CaptureTrigger) — event classification + importance + throttle
- `delta.rs`: 16x16 tile comparison → changed region extraction (pointer-based fast pixel access)
- `encoder.rs`: WebP encoding (Low/Medium/High quality) + stat-based quality prediction
- `thumbnail.rs`: fast_image_resize + LRU caching (100 entries, FNV-1a hash)
- `processor.rs`: `EdgeFrameProcessor` (impl FrameProcessor) — branches by importance
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
- `lib.rs`: `WebServer` — Axum 0.7 HTTP server + graceful shutdown
- `routes.rs`: REST API route definitions (16+ endpoints)
- `handlers/`: metrics, processes, idle, sessions, frames, events, stats, tags, focus
- `embedded.rs`: static file serving for React frontend using rust-embed
- `error.rs`: `ApiError` — JSON error responses
- `frontend/`: React 18 + Vite + Tailwind CSS + Recharts + FocusWidget

### oneshim-automation (Automation Control)
- `controller.rs`: `AutomationController` — policy validation + command execution + audit logging
- `policy.rs`: `PolicyClient` — server policy synchronization, token validation, argument pattern validation (glob matching)
- `audit.rs`: `AuditLogger` — local VecDeque buffer + batched audit logs transmission, buffer overflow management

### oneshim-app (Orchestrator, Binary)
- `main.rs`: tokio runtime + tracing + complete DI wiring + spawned tasks
- `scheduler.rs`: 9-loop scheduler, `should_run_now()` schedule-based conditional execution (monitor 1s, metrics 5s, process 10s, sync 10s, heartbeat 30s, aggregate 1h, notification 1m, focus analysis 1m, server events 30s)
- `notification_manager.rs`: Cooldown-based notification manager (idle, long session, high usage)
- `focus_analyzer.rs`: Focus analysis + local suggestion generation (break, focus time, context restore)
- `lifecycle.rs`: SIGINT/SIGTERM handling, `tokio::sync::watch` shutdown channel
- `event_bus.rs`: `tokio::broadcast` internal event routing
- `autostart.rs`: Run at login — macOS LaunchAgent + Windows Registry
- `updater.rs`: GitHub Releases based auto-updater — version check + download + binary replacement

## Key Dependencies

| Category | Crate | Version |
|----------|-------|---------|
| Runtime | tokio | 1 (full) |
| HTTP | reqwest | 0.12 |
| Web Server | axum + tower-http | 0.7 / 0.6 |
| SSE | eventsource-client | 0.13 |
| WebSocket | tokio-tungstenite | 0.24 |
| **gRPC** | tonic + prost | 0.12 / 0.13 |
| DB | rusqlite | 0.32 (bundled) |
| Monitoring | sysinfo | 0.32 |
| Image | image + fast_image_resize + webp + xcap | 0.25 / 4 / 0.3 / 0.8 |
| UI | iced | 0.13 |
| Windows API | windows-sys | 0.59 |
| Error | thiserror / anyhow | 2 / 1 |
| Serialization| serde + serde_json | 1 |
| Concurrency | crossbeam + parking_lot | 0.8 / 0.12 |
| Caching | lru | 0.12 |
| Auto Update | self_update + semver | 0.41 / 1 |
| Decompression| tar + zip | 0.4 / 2 |

## Coding Conventions

- Comments/Documentation: **English** (Note: Server is in Korean, but this OSS Client uses English)
- Rust edition: **2021**, Minimum version: **1.75**
- Code Formatting: `cargo fmt` (rustfmt default settings)
- Linting: `cargo clippy` — `dead_code` warnings are allowed only for variants intended for future use
- Testing: Write in `#[cfg(test)] mod tests` at the bottom of each module
- Logging: `tracing` macros (`debug!`, `info!`, `warn!`, `error!`)
- Serialization: `serde` derive — `Serialize, Deserialize` for all models

## Reference Documents

- [ADR-001: Rust Client Architecture Patterns](docs/architecture/ADR-001-rust-client-architecture-patterns.md)
- [Documentation Policy](docs/DOCUMENTATION_POLICY.md) — English-only docs + metrics consistency rules
- [Project Status](docs/STATUS.md) — single source of truth for mutable quality metrics
- [Migration Overview](docs/migration/README.md) — Migration plans and history
- [Server API](docs/migration/04-server-api.md) — 29 REST endpoints + gRPC RPCs
- [Migration Phases](docs/migration/05-migration-phases.md) — Phase 0-36 plans
- [Edge Vision](docs/migration/08-edge-vision.md) — Image processing details
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

### Phase 9 Additions (Local Web Dashboard)
- **New Crate**: `oneshim-web` — REST API based on Axum 0.7 + React frontend
- **REST API**: 11 endpoints (metrics, processes, idle, sessions, frames, events, stats)
- **Frontend**: React 18 + Vite + Tailwind CSS + Recharts
  - Dashboard Page: System summary, CPU/Memory charts, app usage time, Top 10 processes
  - Timeline Page: Screenshot thumbnail grid + detailed viewer
- **Static File Embedding**: Include React build results in binary using rust-embed
- **App Integration**: Auto start web server in main.rs, gui_runner.rs
- **Config**: `WebConfig` (enabled, port, allow_external)
- **Tests (+14)**: API handlers, routes, error handling tests

### Phase 10 Additions (Usability Improvements)
- **Settings Page**: Configuration UI (capture enable, idle threshold, metrics interval, web port)
- **Privacy Page**: View storage stats, delete data by date range, clear all data
- **API Pagination**: `PaginatedResponse<T>` + `PaginationMeta` (frames, events)
- **Date Filter**: `DateRangePicker` component (Today, 7 days, 30 days, Custom)
- **New Endpoints**:
  - GET/POST `/api/settings` — View/update settings
  - GET `/api/storage/stats` — Storage statistics
  - DELETE `/api/data/range` — Delete data within range
  - DELETE `/api/data/all` — Delete all data

### Phase 11 Additions (Search Feature)
- **Search API**: GET `/api/search` — Integrated search for frames + events (LIKE based)
- **Search Page**: Search form, result type filter (all/frames/events), highlighting, pagination
- **Global Search Bar**: Search input added to navigation (Enter to navigate to search page)
- **Search Results**: Screenshot thumbnails, event icons, time/app/window info, matched text

### Phase 12 Additions (Real-time Updates)
- **SSE Endpoint**: GET `/api/stream` — Server-Sent Events real-time stream
- **Event Types**: `metrics`, `frame`, `idle`, `ping` (heartbeat)
- **Broadcast Channel**: Route events from scheduler to web server via tokio broadcast
- **React Hooks**: `useSSE` — EventSource connection, auto-reconnect, metric history management
- **Real-time Dashboard**:
  - Connection status indicator (connecting/connected/disconnected/error)
  - Real-time CPU/Memory display (5 sec interval)
  - Idle status badge display

### Phase 13 Additions (Activity Heatmap)
- **Heatmap API**: GET `/api/stats/heatmap` — Activity heatmap by day × time
- **ActivityHeatmap Component**: 7×24 grid, displays activity via color intensity
- **Dashboard Integration**: Heatmap widget added
- **Tests (+1)**: Heatmap response serialization test

### Phase 14 Additions (Config Persistence)
- **ConfigManager**: Manage JSON-based configuration files (`config_manager.rs`)
- **Platform-Specific Paths**:
  - macOS: `~/Library/Application Support/com.oneshim.agent/config.json`
  - Windows: `%APPDATA%\oneshim\agent\config.json`
  - Linux: `~/.config/oneshim/config.json`
- **API Integration**: GET/POST `/api/settings` saves to the actual file
- **Tests (+4)**: ConfigManager create/load/update/reload tests

### Phase 15 Additions (Enhanced Notifications)
- **NotificationConfig**: Notification settings model added
  - `enabled`: Global enable
  - `idle_notification`: Idle notification (default 30 mins)
  - `long_session_notification`: Long session notification (default 60 mins)
  - `high_usage_notification`: High usage notification (default 90%)
- **NotificationManager**: Cooldown-based notification manager (`notification_manager.rs`)
  - `check_idle()`: Idle notification (10 min cooldown)
  - `check_long_session()`: Long session notification (30 min cooldown)
  - `check_high_usage()`: High usage notification (5 min cooldown)
- **8-Loop Scheduler**: Notification loop added (1 min interval)
- **Web Dashboard**: Notification settings UI added to Settings page
- **Tests (+3)**: idle/disabled/high_usage notification trigger tests

### Phase 17 Additions (Dark/Light Theme)
- **ThemeContext**: Theme management via React Context API
  - `theme`: 'dark' | 'light' state
  - `toggleTheme()`: Theme toggle function
  - `localStorage` persist + system preference detection
- **Tailwind Dark Mode**: `darkMode: 'class'` configuration
- **Theme Toggle Button**: Sun/moon icon button on navigation
- **Component Updates**:
  - App.tsx: Navigation, search bar, status display
  - Dashboard.tsx: Metric cards, chart panels, real-time monitoring
  - Settings.tsx: Settings form, input fields, buttons
  - StatCard, ActivityHeatmap: Card components
- **Color Scheme**:
  - Light: bg-white, bg-slate-100, text-slate-900
  - Dark: bg-slate-900, bg-slate-800, text-white

### Phase 21 Additions (Code-Based Design System)
- **Design System Construction**: Ensure consistency enforced by code, not documents
- **Utilities**:
  - `src/lib/cn.ts`: clsx + tailwind-merge class merging utility
- **Design Tokens** (`src/styles/tokens.ts`):
  - Colors: primary, surface, text, semantic, status
  - Spacing: xs, sm, md, lg
  - Typography: h1-h4, body, small, stat
- **Component Variants** (`src/styles/variants.ts`):
  - buttonVariants: primary, secondary, ghost, danger, warning / sm, md, lg, icon
  - cardVariants: default, elevated, highlight, interactive, danger / none, sm, md, lg
  - inputVariants: default, error / sm, md, lg
  - badgeVariants: 7 colors / sm, md
  - selectVariants: default / sm, md
- **UI Components** (`src/components/ui/`):
  - Button: variant, size, isLoading props
  - Card, CardHeader, CardTitle, CardContent: variant, padding props
  - Input: variant, inputSize, error props
  - Badge: color, size props
  - Select: variant, selectSize props
  - Spinner: size prop
- **Migration Completed**:
  - Dashboard.tsx, Timeline.tsx, Settings.tsx, Privacy.tsx, StatCard.tsx refactored
- **Added Dependencies**: `clsx`, `tailwind-merge`
- **TypeScript Type Safety**: variant/size props autocomplete + compile time check

### Phase 22 Additions (Tags/Annotations)
- **Tag System**: Add tags/annotations to screenshots
- **Database Schema (V5)**:
  - `tags` table: id, name, color, created_at
  - `frame_tags` join table: frame_id, tag_id, created_at
  - Foreign Key constraint (ON DELETE CASCADE)
- **Storage Layer** (`oneshim-storage`):
  - `TagRecord` struct
  - 9 tag methods: create_tag, get_all_tags, get_tag, delete_tag, add_tag_to_frame, remove_tag_from_frame, get_tags_for_frame, get_frames_by_tag, update_tag
- **REST API** (`oneshim-web`):
  - GET `/api/tags`, POST `/api/tags`, GET/PUT/DELETE `/api/tags/:id`
  - GET/POST/DELETE `/api/frames/:frame_id/tags` and `:tag_id` mappings
- **Frontend Components**:
  - `TagBadge.tsx`: Tag badge (color, delete button)
  - `TagInput.tsx`: Tag search, select, create dropdown
  - `TAG_COLORS`: 10 basic color palette
- **Timeline Page Integration**:
  - Tag filter, tag display, tag edit UI
- **i18n Support**: Translations added for tag features (Korean/English)
- **Tests (+10)**: tag CRUD, frame_tag linking, migration tests

### Phase 23 Additions (Tag-Based Search)
- **Search API Extended** (`handlers/search.rs`):
  - `tag_ids` query parameter added (comma-separated tag IDs)
  - Search by tags only without text search
  - Include tag info in search results (`TagInfo`)
- **Frontend Search Page Improvements**:
  - Tag filter UI, selected tags display, tag badges in results
  - Store tag IDs in URL query parameters
- **API Client Extended**:
  - `SearchParams` interface, `SearchTagInfo` type added
- **i18n Translations**: Tag filter translations added
- **Tests (+6)**: Tag search query parsing, dynamic SQL generation

### Phase 24 Additions (Reports/Stats)
- **Report API** (`handlers/reports.rs`):
  - GET `/api/reports` — Generate activity report
  - `ReportPeriod`: week, month, custom
- **Report Data Structure**:
  - `DailyStat`: Daily stats (active/idle time, CPU/memory, captures/events)
  - `AppStat`: App usage stats (time, ratio)
  - `HourlyActivity`: Activity by time of day
  - `ProductivityMetrics`: Productivity score, active ratio, peak hours
- **Frontend Reports Page** (`pages/Reports.tsx`):
  - Period selection, productivity score + trend
  - Summary stat cards, daily activity bar charts, hourly activity line charts
  - App usage table + pie chart, CPU/memory trend chart
- **Navigation Integration**: Reports tab added (Shortcut R)
- **i18n Translations**: 27 report-related translations added
- **Tests (+4)**: Deserialization/serialization tests

### Phase 27 Additions (Session Replay)
- **Unified Timeline API**: GET `/api/timeline` — Integrate events + frames + idle periods chronologically
  - `TimelineResponse`: session, items, segments
  - `TimelineItem` enum: Event, Frame, IdlePeriod
  - `AppSegment`: App-specific segments (hash-based colors)
- **React Components**:
  - `TimelineScrubber.tsx`: Playback controls, app segment bar, idle period display
  - `EventLog.tsx`: Event list, current time highlight
  - `SessionReplay.tsx`: Session replay page (viewer + event log)
- **Routing**: `/replay` page added
- **i18n Support**: Replay section translations added
- **Datadog/Clarity Style**: Horizontal scrubber, app segments, striped idle patterns
- **Tests (+7)**: Timeline API serialization, app colors, segment merging

### Phase 28 Additions (Edge Intelligence API + UI + Analyzer)
- **Edge Intelligence Storage**: SQLite 4 tables (V6)
  - `work_sessions`: Work sessions (app states, start/end, focus)
  - `interruptions`: Interruption events (prev/next app, resume time)
  - `focus_metrics`: Focus metrics (daily score, deep work/communication time)
  - `local_suggestions`: Local suggestions (type, priority, feedback)
- **FocusAnalyzer** (`focus_analyzer.rs`):
  - `on_app_switch()`: Detect app switch, auto-track work session/interruption
  - `analyze_periodic()`: Calculate focus score and generate suggestions every 1 min
  - Suggestion types: TakeBreak, NeedFocusTime, RestoreContext
- **8-Loop Scheduler Integration**: 1-minute `analyze_periodic()` loop
- **REST API** (`handlers/focus.rs`):
  - GET `/api/focus/metrics`, `/sessions`, `/interruptions`, `/suggestions`
  - POST `/api/focus/suggestions/:id/feedback`
- **Frontend Components**:
  - `FocusWidget.tsx`, `SuggestionBanner.tsx`, `Focus.tsx`
- **Tests (+4)**: App switch, interruption generation, focus score tests

### Phase 30 Additions (SQLite Performance Optimization)
- **Compound Indexes (V7 Migration)**:
  - `idx_events_sent_timestamp`: (is_sent, timestamp) — Query unsent events +25%
  - `idx_work_sessions_state_started`: (state, started_at) — Session state queries
  - `idx_interruptions_resumed`: (resumed_at) WHERE resumed_at IS NULL — Unresumed interruptions
- **PRAGMA Optimizations** (`sqlite/mod.rs`):
  - `cache_size = 8000`: 8000 pages (default -2000) — Expanded memory cache
  - `temp_store = MEMORY`: Temporary tables stored in memory
  - `mmap_size = 268435456`: 256MB memory mapping — Reduced I/O
- **Batch Save API** (`sqlite/events.rs`):
  - `save_events_batch(&[Event])`: Save multiple events in 1 transaction
  - `prepare_cached()`: Optimized repetitive queries
- **Vec Pre-allocation** (`frame_storage.rs`):
  - `Vec::with_capacity(365)`: Expect 1 year of folders — Prevent reallocation
- **N+1 Query Removal** (RETURNING clause):
  - `end_work_session()`: SELECT+UPDATE → 1 query (+50% performance)
  - `end_idle_period()`: SELECT+UPDATE → 1 query
  - Calculate duration_secs server-side using `julianday()` difference

### Phase 26 Additions (E2E Tests)
- **Playwright E2E Tests**: 72 test cases
- **Test Files** (`frontend/e2e/`):
  - `navigation.spec.ts`: Navigation links, keyboard shortcuts (9 tests)
  - `dashboard.spec.ts`: Dashboard UI, charts, connection status (8 tests)
  - `timeline.spec.ts`: Filtering, view modes, keyboard navigation (8 tests)
  - `settings.spec.ts`: Settings form, save, export (13 tests)
  - `privacy.spec.ts`: Data deletion, backup/restore (13 tests)
  - `search.spec.ts`: Search form, tag filters, result display (10 tests)
  - `reports.spec.ts`: Period selection, charts, stats (11 tests)
- **Scripts**:
  - `pnpm test:e2e` — Run all tests
  - `pnpm test:e2e:headed` — Browser visible mode
  - `pnpm test:e2e:ui` — Playwright UI mode
  - `pnpm test:e2e:report` — View test report
- **Config Files**:
  - `playwright.config.ts`: Chromium headless, 30s timeout, screenshot on failure
  - `e2e/tsconfig.json`: TypeScript config for E2E tests
- **Dependencies Added**: `@playwright/test`

### Phase 25 Additions (Backup/Restore)
- **Backup API** (`handlers/backup.rs`):
  - GET `/api/backup` — Create JSON backup of settings/tags/events/frames
  - POST `/api/backup/restore` — Restore JSON backup file
  - `BackupQuery`: Backup options (include_settings, include_tags, include_events, include_frames)
  - `BackupArchive`: Backup metadata + data structure
  - `RestoreResult`: Restore result (success, restored count, errors)
- **Backup Data Structure**:
  - `BackupMetadata`: Version, creation time, app version, included data types
  - `SettingsBackup`: All settings items
  - `TagBackup`, `FrameTagBackup`: Tags and frame-tag mappings
  - `EventBackup`, `FrameBackup`: Event/frame metadata
- **Restore Features**:
  - Settings: INSERT OR REPLACE to overwrite
  - Tags/events/frames: INSERT OR IGNORE to skip duplicates
  - Detailed restore result reporting (count per item)
- **Frontend** (`pages/Privacy.tsx`):
  - Backup/Restore section added
  - Backup option toggles (settings, tags, events, frames)
  - File selection for restore
  - Restore result messages (success/failure, item counts)
- **i18n Translations**: Backup section added (20+ keys)
- **Tests (+3)**: backup_query_defaults, backup_archive_serializes, restore_result_serializes

### Phase 20 Additions (Multilingual Support i18n)
- **i18next-based Internationalization**:
  - `i18next` + `react-i18next` libraries
  - `i18next-browser-languagedetector`: Auto-detect browser language
  - Save language setting in localStorage (`oneshim-language` key)
- **Supported Languages**: Korean (default), English
- **Translation Files**:
  - `i18n/locales/ko.json`: Korean translations
  - `i18n/locales/en.json`: English translations
- **Application Areas**:
  - Dashboard, Timeline, Settings, Privacy, Search, ActivityHeatmap, DateRangePicker, ShortcutsHelp
- **LanguageSelector Component**: Dropdown language selector (flag icons)
- **New Files**:
  - `src/i18n/index.ts`: i18n configuration
  - `src/i18n/locales/ko.json`, `en.json`: Translations (153 lines each)
  - `src/components/LanguageSelector.tsx`: Language selector

### Phase 19 Additions (Timeline Improvements + Keyboard Shortcuts)
- **Timeline Page Improvements**:
  - Keyboard navigation (← → frame movement, ESC deselect, Enter zoom)
  - Lightbox (fullscreen image viewer, prev/next navigation)
  - Filtering (by app, by importance - High/Medium/Low)
  - View mode toggle (Grid/List)
  - Dark mode support
  - Double-click to open lightbox
- **Global Keyboard Shortcuts**:
  - `D` Dashboard, `T` Timeline, `S` Settings, `P` Privacy
  - `?` Shortcut help modal
  - `ESC` Deselect / Close modal
- **New Components**:
  - `useKeyboardShortcuts.ts`: Global keyboard shortcut hook
  - `Lightbox.tsx`: Fullscreen image viewer
  - `ShortcutsHelp.tsx`: Shortcut help modal
- **Responsive Improvements**: Shortened navigation display on mobile

### Phase 18 Additions (Frontend Build Embedding + Auto Port Finding)
- **rust-embed**: Embed React build results (dist/) into Rust binary
- **embedded.rs**: Static file serving + SPA routing support
  - Auto-detect Content-Type per file (mime_guess)
  - Cache-Control: assets 1 year, html no-cache
  - Return index.html on 404 (SPA routing)
- **Auto Port Finding**: Try next port automatically on conflict
  - Try default port (9090) → if fail, try 9091, 9092... sequentially
  - Error if all 10 attempts fail
  - Log warning when fallback port used
- **build.rs**: Check frontend build status + warn if unbuilt
- **Build Scripts**: `scripts/build-frontend.sh` — pnpm install + build
- **Tests (+2)**: max_port_attempts_is_reasonable, port_overflow_protection
- **Build Process**:
  ```bash
  # 1. Build frontend
  cd crates/oneshim-web/frontend && pnpm install && pnpm build
  # or
  ./scripts/build-frontend.sh

  # 2. Build Rust binary (auto embeds frontend)
  cargo build --release -p oneshim-app
  ```

### Phase 16 Additions (Data Export)
- **Export API**: 3 endpoints
  - GET `/api/export/metrics` — System metrics (CPU, Memory, Disk, Network)
  - GET `/api/export/events` — Event logs (App switch, Window change)
  - GET `/api/export/frames` — Frame metadata (Screenshots info, excluding images)
- **Format Support**: JSON (default), CSV selectable
- **Query Parameters**: `from`, `to` (RFC3339), `format` (json/csv)
- **CSV Conversion**: Auto JSON → CSV conversion, special character escaping
- **Frontend**: Export UI added to Settings page
  - Format selection (JSON/CSV)
  - Export buttons by data type
  - Auto filter for recent 7 days
- **Tests (+3)**: csv_escapes_special_chars, empty_records, default_format

### Phase 8 Additions (Additional Data Storage)
- **System Metrics Storage**: Collect CPU/Memory/Disk/Network every 5s + SQLite save
- **Process Snapshots**: Save Top 10 processes JSON every 10s
- **Idle Detection**: Platform-specific (macOS IOKit, Windows GetLastInputInfo) idle detection + logging
- **Session Stats**: Event/frame/idle time counters
- **Window Position/Size**: macOS AppleScript, Windows GetWindowRect
- **Hourly Aggregation**: 1-hour metric aggregation + old detail data deletion
- **Schema V3-V4**: system_metrics, system_metrics_hourly, process_snapshots, idle_periods, session_stats tables
- **6-Loop Scheduler**: Monitor(1s), Metrics(5s), Process(10s), Sync(10s), Heartbeat(30s), Aggregate(1h)
- **New Modules**: `idle.rs` (IdleTracker), `activity.rs` (Models)
- **Tests (+51)**: migration, metrics, idle, session tests

### Phase 6 Additions (GA Preparation)
- **CI/CD**: GitHub Actions workflows (`rust-ci.yml`, `rust-release.yml`)
  - fmt + clippy checks
  - run tests
  - 4 platform builds (macOS arm64/x64, Windows x64, Linux x64)
  - Auto-create macOS Universal Binary
  - Auto release on tag push
- **Cross Compilation**: `.cargo/config.toml` 4 target configurations
- **Installers**: cargo-bundle (macOS .app), cargo-wix (Windows .msi), cargo-deb (Linux .deb)
- **Documentation**: README.md, CHANGELOG.md, User Guide

### Phase 5 Additions (Auto Update)
- **Auto Update**: Version check + download + binary replacement via GitHub Releases API (`updater.rs`)
- **UpdateConfig**: Added update settings in `oneshim-core/config.rs` (repo, interval, prerelease options)
- **Platform Support**: Auto detect assets for macOS (arm64/x64), Windows (x64/arm64), Linux (x64/arm64)
- **Decompression**: Support for tar.gz, zip archives
- **New Dependencies**: `self_update`, `semver`, `tar`, `zip`
- **Tests (+13)**: mockito based GitHub API tests, version comparison, platform patterns, error handling

### Phase 31-33 Additions (Edge Processing Performance Optimization)
- **Thumbnail LRU Caching** (`thumbnail.rs`):
  - `lru::LruCache` + `parking_lot::Mutex` based thread-safe caching
  - FNV-1a hash (8x8 pixel sampling) → Cache key generation
  - 100 entry cache → Eliminated redundant resize operations
  - ~2x performance boost at 50% cache hit rate
- **Lock-free Batch Queue** (`batch_uploader.rs`):
  - `crossbeam::SegQueue` — CAS-based MPSC Lock-free queue
  - `AtomicUsize` — Track queue size without locks
  - Dynamic batch size: Send immediately under 10, double batch over 50
  - Auto requeue on failure
- **Buffer Pool + Parallel I/O** (`frame_storage.rs`):
  - `crossbeam::ArrayQueue` based reusable buffer pool (10 items)
  - `save_frames_batch()` — Parallel file saving (tokio::spawn)
  - `load_frames_batch()` — Parallel file loading (tokio::spawn)
  - Reduced memory reallocation + parallelized I/O
- **Compression Stats Based Encoding** (`encoder.rs`):
  - `CompressionStats` — Collect atomic compression ratio statistics
  - `estimate_quality_from_stats()` — History-based quality prediction
  - Encoding passes reduced from 3 to 1-2
- **Delta Encoding Pointer Optimization** (`delta.rs`):
  - `get_pixel()` → Direct byte access via `as_raw()`
  - 20-30% performance boost by removing bounds checking
- **Async OCR** (`ocr.rs`):
  - `extract_async()` — Prevent main thread blocking using `spawn_blocking`
  - `extract_roi_async()` — Process center region only (CPU saving)
- **New Dependencies**: `lru 0.12`, `crossbeam 0.8`, `parking_lot 0.12`
- **Tests**: 322 all passed

### Phase 34 Additions (Server Integration Enhancement)
- **CoreError Extended** (`error.rs`):
  - `Network(String)` — Network connection error
  - `RateLimit { retry_after: Option<Duration> }` — 429 Too Many Requests
  - `ServiceUnavailable` — 503 Service Unavailable
- **HTTP Retry Logic** (`http_client.rs`):
  - `execute_with_retry()` — Exponential backoff (1s → 2s → 4s, max 30s)
  - Retry targets: Network, RateLimit, ServiceUnavailable
  - Parse + respect `Retry-After` headers
- **Session Management API** (`api_client.rs`):
  - `SessionCreateResponse` — Session ID, User ID, Client ID, Permissions
  - `create_session()` — Start session (Client ID + metadata)
  - `end_session()` — End session
- **Tests**: 340 all passed

### Phase 35 Additions (Event Payload for Server Pattern Analysis)
- **Event enum Extended** (`models/event.rs`):
  - `Input(InputActivityEvent)` — Mouse/keyboard patterns
  - `Process(ProcessSnapshotEvent)` — Detailed process info
  - `Window(WindowLayoutEvent)` — Window layout info
- **InputActivityEvent**: Input activity patterns
  - `MouseActivity`: click_count, move_distance, scroll_count, double_click, right_click
  - `KeyboardActivity`: keystrokes_per_min, typing_bursts, shortcut_count, correction_count
  - Mouse position: 0.0-1.0 ratio (Privacy)
  - Key content excluded (Patterns only)
- **ProcessSnapshotEvent**: Process snapshots
  - `ProcessDetail`: name, pid, cpu_percent, memory_mb, window_count, is_foreground
  - running_secs, executable_path (anonymized)
- **WindowLayoutEvent**: Window layout
  - `WindowLayoutEventType`: Focus, Resize, Move, Maximize, Minimize, Restore
  - `WindowInfo`: position, size, screen_ratio, is_fullscreen, z_order
  - screen_resolution, monitor_index
- **Handler Updates**: Updated all match statements in storage, reports, stats, timeline, events
- **Event Collection Logic Implementation**:
  - `ProcessMonitor::get_detailed_processes()` — Foreground + Top 10 strategy (deduplicated)
  - `InputActivityCollector` (`input_activity.rs`) — Atomic counter-based thread-safe collection
    - `estimate_from_idle_change()` — Estimate activity via idle time changes
    - `take_snapshot()` — Create accumulated data snapshot and reset
  - `WindowLayoutTracker` (`window_layout.rs`) — Detect window state changes
    - Threshold-based noise filter (5px position, 10px size)
    - Auto-detect fullscreen/maximize
- **9-Loop Scheduler**: Loop 9 added (collect server events every 30s)
- **Tests**: 340 all passed (+12)

### Phase 36 Additions (gRPC Client)
- **gRPC Client Module** (`oneshim-network/src/grpc/`):
  - `GrpcConfig`: gRPC/REST toggle flag + endpoint config + **Port Fallback** ⭐
  - `GrpcAuthClient`: Login, Logout, RefreshToken, ValidateToken RPCs
  - `GrpcSessionClient`: CreateSession, EndSession, Heartbeat RPCs
  - `GrpcContextClient`: UploadBatch, SubscribeSuggestions, SendFeedback, ListSuggestions, Heartbeat RPCs
  - `UnifiedClient`: Unified gRPC + REST client (Feature Flag based switch)
- **Feature Flag**: `--features grpc` conditional compilation
- **Proto Gen Code**: `src/proto/` — tonic + prost based
- **Server Streaming RPC**: `SubscribeSuggestions` — Real-time suggestion stream (SSE alternative)
- **gRPC Port Fallback** (2026-02-05):
  - `grpc_fallback_ports`: Fallback port list (default: `[50052, 50053]`)
  - `all_endpoints()`: Returns all gRPC endpoints (primary + fallbacks)
  - Connection attempt order: primary → fallback[0] → fallback[1] → REST
- **REST Fallback**: Auto fallback to REST API when gRPC fails
  - Support for industrial environments (HTTP/2 blocked)
  - Batch upload: Warning that frames are unsupported via REST
  - Suggestion list: Returns empty list when REST unsupported
- **Industrial Environment ASCII Output**: Disable emojis with `NO_EMOJI=1` env var
- **New Dependencies**: `tonic 0.12`, `prost 0.13`, `prost-types 0.13`
- **Tests**: Mock server communication verified (All 8 RPCs successful)

### Phase 37 Additions (REST Standardization + gRPC Port Fallback) ⭐ NEW (2026-02-05)
- **Auth Route REST Standardization**:
  - Login: `POST /api/v1/auth/tokens` (Resource-centric)
  - Logout: `DELETE /api/v1/auth/tokens`
  - Token Refresh: `POST /api/v1/auth/tokens/refresh`
  - Token Verify: `GET /api/v1/auth/tokens/verify`
  - Logout All: `DELETE /api/v1/auth/tokens/all`
- **Extended gRPC Config** (`oneshim-core/config.rs`):
  - Added `grpc_fallback_ports: Vec<u16>` field
  - Default: `[50052, 50053]`
- **Improved Client Connection Logic**:
  - Applied fallback logic to all gRPC clients (`auth`, `session`, `context`, `health`)
  - Sequential attempts on each endpoint → Use on successful connection
- **Mock Server Update**:
  - Switched test endpoints to REST standard routes
  - Session heartbeat route added: `POST /user_context/sessions/{id}/heartbeat`
- **Legacy Route Removal**:
  - Removed backward compatibility routes like `/login/json`, `/logout`, `/refresh`, `/verify`
- **Tests**: 340 all passed

### Privacy & Permission Control System Additions
- **Tier 1: Telemetry/Monitoring Control**
  - `TelemetryConfig`: Telemetry on/off, crash reports, usage stats, performance metrics
  - `MonitorConfig` Extended: Monitoring enable toggle
  - Scheduler `should_run_now()`: Schedule-based conditional execution
  - Web UI: Monitor/Telemetry toggle section on Settings page
- **Tier 2: Privacy/Schedule Config**
  - `PrivacyConfig`: App blacklist, window title pattern exclusion, sensitive app auto-detection
  - `PiiFilterLevel`: Off → Basic → Standard → Strict (Cascaded inheritance)
  - `ScheduleConfig`: Active hours, active days, pause on lock screen/battery saver
  - `FileAccessConfig`: Temp/cache file access control
  - Web UI: Privacy/Schedule section on Settings page
- **Tier 3: Consent/Automation**
  - `ConsentManager`: GDPR Article 17 (Right to erasure) / Article 20 (Data portability) compliance
  - `ConsentPermissions`, `ConsentRecord`: User consent tracking and management
  - `oneshim-automation` New Crate: Policy-based command execution, audit logging
    - `AutomationController`: Policy validation + command execution + audit logging
    - `PolicyClient`: Server policy sync, token validation, argument pattern validation
    - `AuditLogger`: Local buffer + batched transmission audit logs
- **Tests**: 340 → **381** (+41 tests), 10-crate workspace

### Phase 4.5 Additions
- **Auto Start**: macOS LaunchAgent + Windows Registry support (`autostart.rs`)
- **OCR Module**: `leptess` based Tesseract OCR (`#[cfg(feature = "ocr")]`)
- **Test Enhancements** (+36): mockito HTTP tests, compression errors, SQLite errors, processors, cross-crate error paths
- **Install Scripts**: `scripts/install-macos.sh`, `scripts/uninstall-macos.sh`, `scripts/install-windows.ps1`, `scripts/uninstall-windows.ps1`
