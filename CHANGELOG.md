# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Local Web Dashboard** (`oneshim-web`) — Phase 9-27
  - Axum 0.7-based REST API (16+ endpoints)
  - React 18 + Vite + Tailwind CSS frontend
  - Real-time SSE stream (metrics, frames, idle state)
  - Multilingual support (Korean/English)
  - Dark/Light theme
  - Tag system (screenshot tagging/search)
  - Weekly/monthly activity reports
  - Session replay (Datadog/Clarity style)
  - Data backup/restore
  - 72 E2E tests (Playwright)

- **Edge Intelligence** — Phase 28
  - `FocusAnalyzer`: Focus analysis + local suggestion generation
  - Automatic work session/interruption tracking
  - Focus suggestions: break recommendations, focus time scheduling, context restoration
  - Focus API (5 endpoints)

- **Enhanced Server Integration** — Phase 34-35
  - HTTP retry logic (exponential backoff)
  - Session management API
  - Event payloads for server-side pattern analysis
    - `ProcessSnapshotEvent`: Foreground + Top 10 processes
    - `InputActivityEvent`: Mouse/keyboard patterns
    - `WindowLayoutEvent`: Window layout changes

- **Desktop Notification System** — Phase 15
  - Idle notification (30 minutes)
  - Long session notification (60 minutes)
  - High usage notification (CPU/memory 90%)
  - Cooldown-based deduplication

- **Privacy & Permission Control System** — Tier 1-3
  - TelemetryConfig: telemetry on/off, crash reports, usage statistics, performance metrics
  - PrivacyConfig: app blacklist, window title pattern exclusion, sensitive app auto-detection
  - PII filter levels: Off → Basic → Standard → Strict (cascading inheritance)
  - ScheduleConfig: active hours, weekday settings, pause on screen lock/battery saver
  - ConsentManager: GDPR Article 17 (right to erasure) / Article 20 (right to portability) compliance
  - `oneshim-automation` new crate: automation control, policy client, audit logging
  - Web dashboard Settings: monitor/privacy/schedule/telemetry configuration UI

### Changed
- 8-loop → **9-loop scheduler** (server event collection every 30 seconds)
- 163 tests → **381** (Rust) + **72** (E2E)
- 9-crate → **10-crate** workspace (added `oneshim-automation`)
- `LazyLock` → `once_cell::Lazy` (MSRV 1.75 compatibility)

### Performance
- **SQLite Optimization** — Phase 30
  - Composite indexes (V7 migration)
  - N+1 query elimination (RETURNING clause)
  - PRAGMA optimization (cache_size, mmap_size)

- **Edge Processing Optimization** — Phase 31-33
  - Thumbnail LRU caching (100 entries)
  - Lock-free batch queue (crossbeam)
  - Buffer pool + parallel I/O
  - Delta encoding pointer optimization

### Fixed
- All Clippy warnings resolved (10 → 0)
- Added warning log when error response read fails

## [0.1.3] - 2026-01-29

### Fixed
- CI: Fixed macOS Universal Binary and release workflow paths

## [0.1.2] - 2026-01-29

### Fixed
- macOS arm64: Fully removed `ring` crate dependency (eventsource-client → reqwest-eventsource)

### Changed
- SSE client: Migrated from `eventsource-client` to `reqwest-eventsource` (native-tls based)

## [0.1.1] - 2026-01-29

### Fixed
- Windows: Fixed `windows-sys` 0.59 HKEY type compatibility (`autostart.rs`)
- macOS arm64: Resolved `ring` crate build error (switched to native-tls)
- Linux: Added `libgbm`, `libxcb` system dependencies

### Changed
- TLS backend: `rustls-tls` → `native-tls` (using platform-native TLS)
- CI/CD: Successful builds for 4 platforms (macOS arm64/x64, Windows x64, Linux x64)

## [0.1.0] - 2026-01-28

### Added
- Initial release
- 8-crate Cargo workspace (Hexagonal Architecture)
- Core features:
  - JWT authentication (`oneshim-network/auth.rs`)
  - SSE suggestion reception (`oneshim-network/sse_client.rs`)
  - Batch upload (`oneshim-network/batch_uploader.rs`)
  - Adaptive compression (`oneshim-network/compression.rs`)
  - SQLite local storage (`oneshim-storage/sqlite.rs`)
  - System monitoring (`oneshim-monitor/`)
  - Edge image processing (`oneshim-vision/`)
  - Desktop UI (`oneshim-ui/`)
- Auto-start: macOS LaunchAgent + Windows Registry (`autostart.rs`)
- OCR module: leptess-based Tesseract OCR (`ocr.rs`)
- 163 tests, 0 failures

### Security
- PII filtering: email, credit card, SSN, file paths (`privacy.rs`)
- JWT token auto-renewal and secure storage

---

[Unreleased]: https://github.com/pseudotop/oneshim-client/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/pseudotop/oneshim-client/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/pseudotop/oneshim-client/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/pseudotop/oneshim-client/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/pseudotop/oneshim-client/releases/tag/v0.1.0
