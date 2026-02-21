# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.1] - 2026-02-21

### Core
- 10-crate Hexagonal Architecture workspace (oneshim-core, monitor, vision, network, storage, suggestion, ui, web, automation, app)
- Domain models, port traits, error handling (oneshim-core)
- Constructor injection + `Arc<dyn T>` DI pattern (no framework)
- `thiserror` error enums (library crates) + `anyhow::Result` (binary crate)

### Desktop Agent
- Real-time context monitoring (active windows, system metrics via sysinfo)
- Edge image processing (delta encoding, WebP, OCR via leptess)
- Desktop notifications (idle, long session, high usage) with cooldown-based deduplication
- System tray integration (tray-icon + notify-rust)
- Auto-update via GitHub Releases (self_update + semver)
- Auto-start: macOS LaunchAgent, Windows Registry
- 9-loop scheduler (monitor 1s, metrics 5s, process 10s, sync 10s, heartbeat 30s, aggregate 1h, notification 1m, focus 1m, server events 30s)

### Local Web Dashboard
- Axum 0.7 REST API (16+ endpoints) + React 18 + Vite + Tailwind CSS frontend
- Dashboard, Timeline, Reports, Session Replay, Focus Analytics, Search, Settings, Privacy pages
- Tag system (screenshot tagging/search) with 10-color palette
- Data backup/restore (JSON export/import)
- i18n (Korean/English) via i18next, Dark/Light theme
- Code-based design system (tokens, variants, UI components)
- SSE real-time stream (metrics, frames, idle state)
- Static file embedding via rust-embed
- 72 E2E tests (Playwright)

### Edge Intelligence
- `FocusAnalyzer`: Focus analysis + local suggestion generation
- Work session / interruption tracking with automatic app switch detection
- Suggestion types: TakeBreak, NeedFocusTime, RestoreContext
- Focus API (5 endpoints)

### Privacy & Permissions
- PII filter levels (Off/Basic/Standard/Strict) with cascading inheritance
- Sensitive app auto-detection, app blacklist, window title pattern exclusion
- GDPR Article 17 (right to erasure) / Article 20 (data portability) compliance via ConsentManager
- TelemetryConfig, ScheduleConfig (active hours, weekday settings, pause on lock/battery)
- `oneshim-automation` crate: policy-based command execution, audit logging, server policy sync

### Network
- JWT auth with token auto-renewal (`TokenManager`)
- HTTP retry logic (exponential backoff 1s to 30s, Retry-After header support)
- SSE real-time suggestions with auto-reconnect
- WebSocket client (tokio-tungstenite)
- gRPC client (tonic) with auth, session, context RPCs and server streaming
- gRPC port fallback (primary + configurable fallback ports + REST fallback)
- Adaptive compression (gzip/zstd/lz4 auto-selection)
- Lock-free batch upload (crossbeam SegQueue, dynamic batch size)
- REST standardized auth routes (`/api/v1/auth/tokens`)

### Performance
- SQLite optimization: composite indexes (V7), PRAGMA tuning (cache_size, mmap_size), N+1 elimination (RETURNING clause)
- Thumbnail LRU caching (100 entries, FNV-1a hash)
- Buffer pool + parallel I/O (crossbeam ArrayQueue)
- Compression stats-based quality prediction
- Delta encoding pointer optimization (20-30% speedup)
- Async OCR via spawn_blocking

### Cross-Platform
- macOS (arm64/x64/Universal Binary), Windows (x64), Linux (x64)
- Platform-specific: macOS LaunchAgent + AppleScript, Windows Registry + Win32 API, Linux X11/Wayland (xdotool)
- CI/CD: GitHub Actions (fmt, clippy, test, 4-platform build, auto-release on tag push)
- Installers: macOS .app (cargo-bundle), Windows .msi (cargo-wix), Linux .deb (cargo-deb)

---

[Unreleased]: https://github.com/pseudotop/oneshim-client/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/pseudotop/oneshim-client/releases/tag/v0.0.1
