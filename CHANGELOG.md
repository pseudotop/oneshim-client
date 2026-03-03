# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2] - 2026-03-03

### Fixed

- Stabilize GUI startup and shutdown paths to prevent runtime panic during tray bootstrap
- Harden cross-platform GUI smoke flows to avoid false negatives on headless runners
- Unblock release gates by addressing clippy bound checks and contract/vet workflow drift

### Changed

- Expand release reliability smoke coverage across Linux, macOS, and Windows in PR pipelines
- Upload GUI/bootstrap diagnostics on CI failures for faster release triage
- Align license policy and CI checks for release packaging consistency

## [0.1.1] - 2026-02-27

### Fixed

- **macOS installer naming**: Remove misleading `-unsigned` suffix from signed DMG/PKG artifacts
- **Notarize workflow**: Update artifact filenames to match signed installer names
- **Installer smoke test**: Align default filenames with release pipeline

### Changed

- **Build scripts**: Replace direct `cargo` calls with `cargo-cache.sh` wrapper across all CI workflows and scripts

## [0.1.0] - 2026-02-27

First public release of the ONESHIM Rust desktop client.

### Added

- **10-crate Cargo workspace** with Hexagonal Architecture (Ports & Adapters)
  - `oneshim-core`: Domain models, port traits, error types, config management
  - `oneshim-monitor`: System metrics (CPU/Memory/Disk/Network), active window, idle detection
  - `oneshim-vision`: Screen capture, delta encoding, WebP, thumbnail LRU caching, PII filter, OCR
  - `oneshim-network`: JWT auth, HTTP/SSE/WebSocket, adaptive compression, batch upload, gRPC client
  - `oneshim-storage`: SQLite (WAL mode), schema V1-V7, frame file storage, buffer pool
  - `oneshim-suggestion`: SSE suggestion reception, priority queue, feedback, history
  - `oneshim-ui`: iced GUI, system tray, desktop notifications, dark/light theme
  - `oneshim-web`: Local web dashboard (Axum REST API + embedded React frontend)
  - `oneshim-automation`: Policy-based command execution, audit logging, HMAC token validation
  - `oneshim-app`: Binary entry point, 9-loop scheduler, DI wiring, lifecycle management
- **Web Dashboard** at `http://localhost:9090` with React 18 + Vite + Tailwind CSS
  - Dashboard, Timeline, Search, Reports, Settings, Privacy, Session Replay pages
  - Real-time SSE updates, activity heatmap, focus analysis widget
  - Tag system, data export (JSON/CSV), backup/restore
  - i18n (Korean/English), dark mode, keyboard shortcuts, code-based design system
- **Edge image processing**: Smart capture trigger, delta encoding, WebP encoding, async OCR
- **Performance optimizations**: Lock-free batch queue, buffer pool, parallel I/O, LRU caching, compression stats
- **gRPC client** (`--features grpc`): Auth, Session, Context RPCs with server streaming and port fallback
- **REST standardization**: Resource-centric auth routes (`/api/v1/auth/tokens`)
- **Privacy & permission control**: 3-tier system (telemetry, privacy/schedule, consent/automation)
  - GDPR Article 17/20 compliant consent management
  - PII filter levels (Off/Basic/Standard/Strict)
  - App blacklist, schedule-based monitoring, sensitive app auto-detection
- **Auto-update**: GitHub Releases based version check, download, decompress, binary replacement
- **Cross-platform**: macOS (arm64/x64 + universal binary), Windows (x64), Linux (x64)
- **CI/CD**: GitHub Actions (fmt, clippy, test, 4-platform release builds, code signing)
- **831 tests** (0 failures) across all crates + 72 Playwright E2E tests
- **ADR-003**: Directory module pattern for large source files (>500 lines)
  - Split 9 files across 5 crates into focused directory modules
  - All external API paths preserved via `pub use` re-exports

## Version Management Rules

### Release Workflow
1. Update `version` in `Cargo.toml` workspace section
2. Add changelog entry under the new version heading
3. Commit: `release: v{version}`
4. Tag: `git tag v{version}` — triggers CI/CD release pipeline
5. Push: `git push origin main --tags`

### Versioning Policy
- **Patch** (0.0.x): Bug fixes, CI/CD fixes, documentation
- **Minor** (0.x.0): New features, new crates, API changes
- **Major** (x.0.0): Breaking changes, architecture redesign

### Changelog Entry Format
Each version entry must include:
- **Added**: New features or capabilities
- **Changed**: Changes to existing functionality
- **Fixed**: Bug fixes
- **Removed**: Removed features or capabilities

---

[Unreleased]: https://github.com/pseudotop/oneshim-client/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/pseudotop/oneshim-client/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/pseudotop/oneshim-client/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/pseudotop/oneshim-client/releases/tag/v0.1.0
