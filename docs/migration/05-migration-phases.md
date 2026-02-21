[English](./05-migration-phases.md) | [한국어](./05-migration-phases.ko.md)

# 5. Migration Phases + Success Criteria

[← Server API](./04-server-api.md) | [UI Framework →](./06-ui-framework.md)

---

## Phase 0: Project Foundation

**Goal**: Create Rust workspace, set up build pipeline

```
[ ] Initialize Cargo workspace (8 crates)
[ ] Set up CI/CD (cargo test, cargo clippy, cargo fmt)
[ ] Cross-compilation setup (macOS universal + Windows x64)
[ ] .cargo/config.toml build optimizations
```

## Phase 1: Core Models + Network (P0 — SSE Connection)

**Goal**: Minimal pipeline to receive suggestions from server via SSE

```
[ ] oneshim-core: Model structs (serde), trait definitions, error types
[ ] oneshim-network/auth.rs: Login → JWT token storage → auto refresh
[ ] oneshim-network/http_client.rs: reqwest-based API client
[ ] oneshim-network/sse_client.rs: SSE stream connection + event parsing
[ ] oneshim-suggestion/receiver.rs: SSE event → Suggestion struct conversion
[ ] oneshim-suggestion/feedback.rs: Accept/reject HTTP POST
[ ] oneshim-app/main.rs: Minimal execution — login → SSE connect → stdout output
```

**Verification**: `cargo run` → Receive SSE suggestions from server → Print to terminal

## Phase 2: Local Storage + Monitoring + Edge Vision

**Goal**: Context collection + image Edge processing → local storage → batch upload

```
[ ] oneshim-storage/sqlite.rs: Event log + frame index tables, CRUD, retention policy
[ ] oneshim-storage/migration.rs: Schema version management
[ ] oneshim-monitor/system.rs: sysinfo-based CPU/memory/disk/network
[ ] oneshim-monitor/process.rs: Active window info (platform branching)
[ ] oneshim-monitor/macos.rs: CoreGraphics front app detection
[ ] oneshim-monitor/windows.rs: Win32 GetForegroundWindow
[ ] oneshim-vision/capture.rs: xcap-based screen capture (multi-monitor)
[ ] oneshim-vision/trigger.rs: Smart capture trigger (event-based, 5s throttle)
[ ] oneshim-vision/processor.rs: Edge preprocessing orchestrator (importance-based branching)
[ ] oneshim-vision/delta.rs: Delta encoding (extract changed regions vs previous frame)
[ ] oneshim-vision/encoder.rs: WebP/JPEG encoding + automatic quality selection
[ ] oneshim-vision/thumbnail.rs: 480×270 thumbnail generation
[ ] oneshim-vision/ocr.rs: Tesseract FFI local OCR (text metadata extraction)
[ ] oneshim-vision/timeline.rs: Frame index management (SQLite integration, rewind support)
[ ] oneshim-vision/privacy.rs: PII filtering (window title sanitization)
[ ] oneshim-network/batch_uploader.rs: Batch queue + retry + compression (metadata+image mixed)
[ ] oneshim-network/compression.rs: Selective compression (gzip/zstd/lz4)
[ ] oneshim-app/scheduler.rs: Monitor loop (1s), sync loop (10s), heartbeat
```

**Verification**: Context collection + screenshot Edge processing → SQLite storage → metadata+preprocessed image batch upload → SSE reception

## Phase 3: UI Foundation

**Goal**: System tray + suggestion notifications + main window + rewind timeline

```
[ ] oneshim-ui/tray.rs: System tray icon + menu (Show/Hide, Settings, Quit)
[ ] oneshim-ui/views/suggestion_popup.rs: Suggestion toast/popup (accept/reject buttons)
[ ] oneshim-ui/views/main_window.rs: Current context + status display
[ ] oneshim-ui/views/status_bar.rs: Connection status, metrics
[ ] oneshim-ui/views/context_panel.rs: Active app, system resources
[ ] oneshim-ui/views/timeline_view.rs: Screenshot rewind timeline (thumbnail scroll)
[ ] oneshim-ui/theme.rs: Dark/light theme
[ ] oneshim-suggestion/presenter.rs: Suggestion → UI data conversion (pipeline preview included)
[ ] oneshim-suggestion/queue.rs: Local suggestion queue (max 50, priority)
```

**Verification**: Tray icon → SSE suggestion received → Desktop notification/popup → Accept/reject → Timeline rewind

## Phase 4: Completeness

**Goal**: Feature completeness + deployment readiness

```
[ ] oneshim-ui/views/settings.rs: Settings screen
[ ] oneshim-network/ws_client.rs: WebSocket (conversation mode)
[ ] oneshim-suggestion/history.rs: Suggestion history local cache
[ ] oneshim-app/lifecycle.rs: Start/shutdown, resource cleanup
[ ] oneshim-app/event_bus.rs: Internal events (tokio::broadcast)
[ ] Auto-start setup (launchd / registry)
[ ] Auto-update mechanism
[ ] Installer builds (.dmg, .exe/.msi)
[ ] README, user guide
```

## Phase 5: Auto-Update

**Goal**: GitHub Releases-based auto-update

```
[x] oneshim-app/updater.rs: Version check + download + binary replacement
[x] UpdateConfig: repo, interval, prerelease options
[x] Platform-specific asset auto-detection (macOS arm64/x64, Windows, Linux)
[x] tar.gz, zip decompression
```

## Phase 6: GA Preparation

**Goal**: CI/CD + Installers + Documentation

```
[x] GitHub Actions (rust-ci.yml, rust-release.yml)
[x] 4-platform builds (macOS arm64/x64, Windows x64, Linux x64)
[x] macOS Universal Binary auto-generation
[x] Auto-release on tag push
[x] cargo-bundle, cargo-wix, cargo-deb installers
```

## Phase 8-35: Feature Enhancements

**Details**: See CLAUDE.md "Phase N Additions" sections

- **Phase 8**: System metrics storage, idle detection, session statistics
- **Phase 9-14**: Local web dashboard (Axum + React)
- **Phase 15-19**: Notifications, export, Dark/Light theme, keyboard shortcuts
- **Phase 20-24**: i18n, design system, tags, reports
- **Phase 25-27**: Backup/restore, E2E tests, session replay
- **Phase 28-30**: Edge Intelligence, SQLite performance optimization
- **Phase 31-33**: Thumbnail caching, lock-free queue, buffer pool
- **Phase 34-35**: Server integration hardening, event payload extension

## Phase 36: gRPC Client ★ NEW

**Goal**: gRPC API integration (SSE replacement)

```
[x] oneshim-network/grpc/mod.rs: GrpcConfig + module export
[x] oneshim-network/grpc/auth_client.rs: Login, Logout, RefreshToken, ValidateToken
[x] oneshim-network/grpc/session_client.rs: CreateSession, EndSession, Heartbeat
[x] oneshim-network/grpc/context_client.rs: UploadBatch, SubscribeSuggestions, SendFeedback, ListSuggestions
[x] oneshim-network/grpc/unified_client.rs: gRPC + REST unified client
[x] Feature Flag: --features grpc
[x] REST Fallback: Automatic switch on gRPC failure
[x] Industrial ASCII output: NO_EMOJI=1 environment variable
```

**Verification**: Mock server communication test — all 8 RPCs successful

---

## Success Criteria

### Phase 1 Completion Criteria (MVP)
- [ ] `cargo run` → Server login → SSE connection → Suggestion reception → Terminal output
- [ ] Suggestion accept/reject feedback sent → Confirmed on server

### Phase 2 Completion Criteria
- [ ] Context collection (active window, CPU, memory) → SQLite storage → Batch upload
- [ ] Screen capture → Edge preprocessing (delta/thumbnail/OCR) → Metadata+image batch transmission
- [ ] Frame index SQLite storage + retention policy working
- [ ] Server generates context-based suggestion → Received via SSE

### Phase 3 Completion Criteria
- [ ] System tray icon + menu
- [ ] Suggestion received → Desktop notification → Accept/reject UI
- [ ] Main window: current context + status display
- [ ] Timeline rewind: thumbnail scroll + text search

### Phase 4 Completion Criteria (GA)
- [x] .dmg / .exe single binary distribution
- [x] Auto-start + auto-update
- [x] Full replacement of Python Client
- [x] All tests passing (cargo test --workspace)

### Phase 36 Completion Criteria (gRPC)
- [x] gRPC auth RPCs: Login, Logout, RefreshToken, ValidateToken
- [x] gRPC session RPCs: CreateSession, EndSession, Heartbeat
- [x] gRPC context RPCs: UploadBatch, SubscribeSuggestions, SendFeedback, ListSuggestions
- [x] Server Streaming: Real-time suggestion reception (SSE replacement)
- [x] REST Fallback: Industrial environment support
- [x] Mock server communication verification complete
