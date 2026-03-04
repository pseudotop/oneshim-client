# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.7] - 2026-03-04

### Added

- **Vitest test infrastructure**: Complete unit test setup for React web dashboard
  - `vitest.config.private.ts` with jsdom environment, `@src/` path alias
  - 61 smoke tests covering all major UI components (Dashboard, Timeline, Settings, Events, Sessions, Shell layout)
  - Shared test helpers: `SpyEventSource` for SSE mocking, render utilities
  - CI-compatible runner script (`run-frontend.sh`) with per-test metadata tracking

## [0.1.6] - 2026-03-04

### Added

- **Desktop shell layout** (VS Code-style): TitleBar, ActivityBar, SidePanel, TreeView, CommandPalette, ShortcutsHelp, StatusBar
- **3 new hooks**: `useShellLayout`, `useCommandPalette`, `useKeyboardShortcuts`
- **Accessibility (WCAG 2.1 AA)**: Skip navigation, focus-visible policy, full ARIA coverage, keyboard-navigable resize
- **i18n**: 40+ new translation keys (en/ko parity)

## [0.1.5] - 2026-03-04

### Changed

- **Desktop runtime**: Migrate from iced 0.13 to Tauri v2 for integrated WebView, tray, and IPC
- **Project structure**: Replace `oneshim-ui` + `oneshim-app` crates with `src-tauri/` Tauri project
- **Window behavior**: Close-to-tray (hide on close instead of quit)
- **Security**: `emit_to("main", ...)` targeted events instead of global `emit()`
- **Tray menu**: Direct AppState access for approve/defer update actions
- **Graceful shutdown**: `watch::Sender<bool>` via `RunEvent::Exit`
- **IPC commands**: 7 Tauri commands (get_metrics, get_settings, update_setting, get_update_status, approve_update, defer_update, get_automation_status)
- **Web router**: Extract `build_router()` from `WebServer` for in-process API routing

### Added

- **Tauri v2 scaffold**: `src-tauri/` with main.rs, setup.rs, tray.rs, commands.rs
- **macOS entitlements**: WKWebView JIT + unsigned memory + localhost network
- **Release checks**: tauri.conf.json consistency validation, NSAppTransportSecurity
- **CI**: `src-tauri/**` path filter, Tauri cargo-vet exemptions

### Removed

- **iced dependency**: iced 0.13, cosmic-text, wgpu stack (~16 crate exemptions removed)
- **oneshim-ui crate**: Replaced by Tauri WebView + React dashboard
- **oneshim-app crate**: Migrated to src-tauri binary

### Fixed

- **cargo-vet**: Remove 16 stale iced/wgpu exemptions
- **dead_code warnings**: Suppress 19 warnings in migrated-but-unwired modules
- **deny.toml**: `unsound = "deny"` (was `"none"`), `unmaintained = "warn"` (was `"none"`)
- **verify-deb-metadata.sh**: Update path from `crates/oneshim-app/` to `src-tauri/`

## [0.1.4] - 2026-03-03

### Fixed

- Run macOS GUI bootstrap smoke with `ONESHIM_DISABLE_TRAY=1` to avoid WindowServer/tray aborts on headless CI runners

## [0.1.3] - 2026-03-03

### Fixed

- Prevent macOS release pipeline failure during DMG creation by reclaiming runner disk space before `hdiutil`
- Allocate DMG build workspace size based on app bundle size to avoid `No space left on device` in GitHub Actions

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

[Unreleased]: https://github.com/pseudotop/oneshim-client/compare/v0.1.7...HEAD
[0.1.7]: https://github.com/pseudotop/oneshim-client/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/pseudotop/oneshim-client/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/pseudotop/oneshim-client/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/pseudotop/oneshim-client/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/pseudotop/oneshim-client/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/pseudotop/oneshim-client/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/pseudotop/oneshim-client/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/pseudotop/oneshim-client/releases/tag/v0.1.0
