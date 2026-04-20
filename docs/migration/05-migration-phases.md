[English](./05-migration-phases.md) | [한국어](./05-migration-phases.ko.md)

# 5. Migration Phases + Success Criteria

[← Server API](./04-server-api.md) | [UI Framework →](./legacy/06-ui-framework.md)

---

## Phase 0: Project Foundation ✅ COMPLETE

**Goal**: Create Rust workspace, set up build pipeline

- [x] Cargo workspace (now 15 packages per `cargo metadata --no-deps`)
- [x] CI/CD (cargo test, cargo clippy, cargo fmt)
- [x] Cross-compilation (macOS universal + Windows x64 + Linux)
- [x] `.cargo/config.toml` build optimizations

## Phase 1: Core Models + Network (P0 — SSE Connection) ✅ COMPLETE

**Goal**: Minimal pipeline to receive suggestions from server via SSE

- [x] `oneshim-core` models + trait definitions + error types (now typed-code per ADR-019)
- [x] `oneshim-network/auth.rs` JWT login + auto refresh
- [x] `oneshim-network/http_client.rs` reqwest-based API client
- [x] `oneshim-network/sse_client.rs` SSE stream + event parsing + auto-reconnect
- [x] `oneshim-suggestion/receiver.rs` SSE event → Suggestion conversion
- [x] `oneshim-suggestion/feedback.rs` Accept/reject HTTP POST + FeedbackRetryQueue

**Verification**: `cargo run -p oneshim-app` → Receive SSE suggestions → surface in UI.

## Phase 2: Local Storage + Monitoring + Edge Vision ✅ COMPLETE

**Goal**: Context collection + image Edge processing → local storage → batch upload

- [x] `oneshim-storage/sqlite.rs` + V1–V22 migrations (events, frames, work_sessions, focus_metrics, IVF index, coaching, etc.)
- [x] `oneshim-monitor/{system,process,macos,windows,linux,activity,input_activity,window_layout}.rs` — platform-branched active window + metrics + idle + layout tracking
- [x] `oneshim-vision/{capture,trigger,processor,delta,encoder,thumbnail,ocr,timeline,privacy}.rs` — Edge pipeline (xcap multi-monitor capture → WebP encoder → thumbnail LRU cache → importance-branched delta/OCR → PII filter with Off/Basic/Standard/Strict levels)
- [x] `oneshim-network/{batch_uploader,compression}.rs` — lock-free SegQueue + gzip/zstd/lz4 auto selection
- [x] `src-tauri/src/scheduler/` — 16-loop background scheduler (replaces the originally-planned single `scheduler.rs`)

**Verification**: Context collection + screenshot Edge processing → SQLite retention → metadata+image batch upload → SSE reception.

## Phase 3: UI Foundation ✅ COMPLETE (via Tauri — see ADR-004)

**Goal**: System tray + suggestion notifications + main window + rewind timeline

> The originally-planned `oneshim-ui` crate (iced) was replaced by **Tauri v2 + React** per [ADR-004](../architecture/ADR-004-tauri-v2-migration.md). The logical deliverables shipped under the new surface:

- [x] System tray (`src-tauri/src/tray.rs`)
- [x] Suggestion popup / toast — delivered via desktop notifications + MagicOverlay (ADR-002 M3)
- [x] Main window + status bar + context panel — React pages under `crates/oneshim-web/frontend/src/pages/`
- [x] Timeline rewind — frame timeline (in-memory + SQLite-backed)
- [x] Dark/light theme — React `useTheme` hook
- [x] `oneshim-suggestion/{presenter,queue}.rs` — SuggestionView + BTreeSet priority queue (max 50)

**Verification**: Tray → SSE suggestion → desktop notification/popup → accept/reject → timeline rewind.

## Phase 4: Completeness ✅ COMPLETE

**Goal**: Feature completeness + deployment readiness

- [x] Settings screen — React setting tabs (GeneralTab, NotificationsTab, PermissionsTab, PrivacyTab, etc.)
- [x] `oneshim-suggestion/history.rs` FIFO history cache
- [x] Lifecycle (start/shutdown + resource cleanup) — `src-tauri/src/lifecycle/`
- [x] Internal event bus — tokio::broadcast throughout the scheduler
- [x] Auto-start (launchd / registry)
- [x] Auto-update mechanism — `src-tauri/src/updater/` with D9 multi-key Ed25519 trust + D10 defensive rollout + D11 self-healthy probe with automatic rollback
- [x] Installer builds (.dmg, .exe/.msi, .deb/.AppImage) via `cargo tauri build`
- [x] README + user guide + Korean companion docs

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

### Phase 1 Completion Criteria (MVP) ✅
- [x] `cargo run` → Server login → SSE connection → Suggestion reception
- [x] Suggestion accept/reject feedback sent → Confirmed on server

### Phase 2 Completion Criteria ✅
- [x] Context collection (active window, CPU, memory) → SQLite storage → Batch upload
- [x] Screen capture → Edge preprocessing (delta/thumbnail/OCR) → Metadata+image batch transmission
- [x] Frame index SQLite storage + retention policy (30 days / 500MB)
- [x] Server generates context-based suggestion → Received via SSE

### Phase 3 Completion Criteria ✅ (via Tauri per ADR-004)
- [x] System tray icon + menu (`src-tauri/src/tray.rs`)
- [x] Suggestion received → Desktop notification → Accept/reject UI
- [x] Main window: React-based current context + status display
- [x] Timeline rewind: thumbnail scroll + text search (FTS5)

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
