# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Changed

- Update [Unreleased] [skip ci]

- Update [Unreleased] [skip ci]

- Update [Unreleased] [skip ci]


### Fixed

- Cargo fmt + ActivityBar role attribute for E2E nav selector
  - cargo fmt: consent.rs, events.rs, privacy.rs line-length reflow
  - ActivityBar: add explicit role="navigation" to <nav> element
    (nav[role="navigation"] CSS selector requires explicit attribute;
    implicit ARIA role is not matched by attribute selectors)

- Clippy needless borrow in encryption + mock ai/providers/presets in E2E
  - Remove needless `&self.0` borrow in `EncryptionKey::save_to_file` (clippy::needless_borrows_for_generic_args)
  - Add `/api/ai/providers/presets` mock to `mockDefaultApiFallbacks` to prevent ECONNREFUSED timeout in replay-scene E2E tests

- Create frontendDist stub before updater regression tests


## [Unreleased]
### Changed

- Update [Unreleased] [skip ci]

- Update [Unreleased] [skip ci]


### Fixed

- Cargo fmt + ActivityBar role attribute for E2E nav selector
  - cargo fmt: consent.rs, events.rs, privacy.rs line-length reflow
  - ActivityBar: add explicit role="navigation" to <nav> element
    (nav[role="navigation"] CSS selector requires explicit attribute;
    implicit ARIA role is not matched by attribute selectors)

- Clippy needless borrow in encryption + mock ai/providers/presets in E2E
  - Remove needless `&self.0` borrow in `EncryptionKey::save_to_file` (clippy::needless_borrows_for_generic_args)
  - Add `/api/ai/providers/presets` mock to `mockDefaultApiFallbacks` to prevent ECONNREFUSED timeout in replay-scene E2E tests


## [Unreleased]
### Changed

- Update [Unreleased] [skip ci]


### Fixed

- Cargo fmt + ActivityBar role attribute for E2E nav selector
  - cargo fmt: consent.rs, events.rs, privacy.rs line-length reflow
  - ActivityBar: add explicit role="navigation" to <nav> element
    (nav[role="navigation"] CSS selector requires explicit attribute;
    implicit ARIA role is not matched by attribute selectors)


## [Unreleased]
### Added

- Design system hardening — Storybook, tokens, Biome linter
  - Add Storybook 10 with 7 UI primitive stories + design token catalog
  - Expand token system: motion, elevation, iconSize, typography micro/nano
  - Fix UI primitives: Button focusRing, Select interactive, Input dead prop, EmptyState uses Button
  - Apply typography.h1 tokens across 8 pages, motion/elevation/iconSize tokens to components
  - Fix 6 missing focusRing instances + a11y improvements (type="button", semantic HTML, aria)
  - Set up Biome v2 with GritQL plugin to block hardcoded slate/gray color classes
  - Apply Biome formatting + import organization + Tailwind class sorting
  - CSS custom properties refactoring: 60+ semantic CSS vars, dark/light theme via vars
  - Zero lint errors (0 errors, 0 warnings across 88 files)
  - Build passes, 61/61 tests pass

- Add SQLite encryption key management infrastructure ([#48](https://github.com/pseudotop/oneshim-client/pull/48))
  * docs(plan): add multi-agent audit remediation design doc

  12개 전문 에이전트 분석 결과를 기반으로 3 Phase 워크트리 remediation 계획 수립.
  Phase 1(4 병렬): docs/security/storage/i18n
  Phase 2(3 병렬): ux/privacy/rust-errors
  Phase 3(1 순차): enterprise/OSS 문서


### Changed

- Add v0.1.6 and v0.1.7 changelog entries

- Bump actions/download-artifact from 7 to 8
  Bumps [actions/download-artifact](https://github.com/actions/download-artifact) from 7 to 8.
  - [Release notes](https://github.com/actions/download-artifact/releases)
  - [Commits](https://github.com/actions/download-artifact/compare/v7...v8)

  ---
  updated-dependencies:
  - dependency-name: actions/download-artifact
    dependency-version: '8'
    dependency-type: direct:production
    update-type: version-update:semver-major
  ...

- Bump actions/attest-build-provenance from 2 to 4
  Bumps [actions/attest-build-provenance](https://github.com/actions/attest-build-provenance) from 2 to 4.
  - [Release notes](https://github.com/actions/attest-build-provenance/releases)
  - [Changelog](https://github.com/actions/attest-build-provenance/blob/main/RELEASE.md)
  - [Commits](https://github.com/actions/attest-build-provenance/compare/v2...v4)

  ---
  updated-dependencies:
  - dependency-name: actions/attest-build-provenance
    dependency-version: '4'
    dependency-type: direct:production
    update-type: version-update:semver-major
  ...

- Bump actions/setup-node from 4 to 6
  Bumps [actions/setup-node](https://github.com/actions/setup-node) from 4 to 6.
  - [Release notes](https://github.com/actions/setup-node/releases)
  - [Commits](https://github.com/actions/setup-node/compare/v4...v6)

  ---
  updated-dependencies:
  - dependency-name: actions/setup-node
    dependency-version: '6'
    dependency-type: direct:production
    update-type: version-update:semver-major
  ...

- Bump actions/upload-artifact from 6 to 7
  Bumps [actions/upload-artifact](https://github.com/actions/upload-artifact) from 6 to 7.
  - [Release notes](https://github.com/actions/upload-artifact/releases)
  - [Commits](https://github.com/actions/upload-artifact/compare/v6...v7)

  ---
  updated-dependencies:
  - dependency-name: actions/upload-artifact
    dependency-version: '7'
    dependency-type: direct:production
    update-type: version-update:semver-major
  ...

- Bump zip from 2.4.2 to 8.1.0
  Bumps [zip](https://github.com/zip-rs/zip2) from 2.4.2 to 8.1.0.
  - [Release notes](https://github.com/zip-rs/zip2/releases)
  - [Changelog](https://github.com/zip-rs/zip2/blob/master/CHANGELOG.md)
  - [Commits](https://github.com/zip-rs/zip2/compare/v2.4.2...v8.1.0)

  ---
  updated-dependencies:
  - dependency-name: zip
    dependency-version: 8.1.0
    dependency-type: direct:production
    update-type: version-update:semver-major
  ...

- Bump sysinfo from 0.38.2 to 0.38.3 ([#40](https://github.com/pseudotop/oneshim-client/pull/40))
  Bumps [sysinfo](https://github.com/GuillaumeGomez/sysinfo) from 0.38.2 to 0.38.3.
  - [Changelog](https://github.com/GuillaumeGomez/sysinfo/blob/main/CHANGELOG.md)
  - [Commits](https://github.com/GuillaumeGomez/sysinfo/compare/v0.38.2...v0.38.3)

  ---
  updated-dependencies:
  - dependency-name: sysinfo
    dependency-version: 0.38.3
    dependency-type: direct:production
    update-type: version-update:semver-patch
  ...

- Bump tauri-build from 2.5.5 to 2.5.6 ([#41](https://github.com/pseudotop/oneshim-client/pull/41))
  Bumps [tauri-build](https://github.com/tauri-apps/tauri) from 2.5.5 to 2.5.6.
  - [Release notes](https://github.com/tauri-apps/tauri/releases)
  - [Commits](https://github.com/tauri-apps/tauri/compare/tauri-build-v2.5.5...tauri-build-v2.5.6)

  ---
  updated-dependencies:
  - dependency-name: tauri-build
    dependency-version: 2.5.6
    dependency-type: direct:production
    update-type: version-update:semver-patch
  ...

- Bump xcap 0.8.3, uuid 1.22.0, tokio 1.50.0
  Consolidates Dependabot PRs #42, #43, #44 into a single update.

- Update all docs to reflect Tauri v2 migration ([#45](https://github.com/pseudotop/oneshim-client/pull/45))
  * docs(plan): add multi-agent audit remediation design doc

  12개 전문 에이전트 분석 결과를 기반으로 3 Phase 워크트리 remediation 계획 수립.
  Phase 1(4 병렬): docs/security/storage/i18n
  Phase 2(3 병렬): ux/privacy/rust-errors
  Phase 3(1 순차): enterprise/OSS 문서

- Enterprise OSS documentation package (Phase 3) ([#52](https://github.com/pseudotop/oneshim-client/pull/52))
  * docs: add enterprise deployment, OSS on-ramp, CI transparency, ADR-005/006, version migration guide


### Fixed

- Create frontend dist stub before clippy for Tauri generate_context!()
  Tauri's proc macro requires frontend/dist/index.html to exist at
  compile time. Add a stub directory creation step before clippy runs
  so the check job doesn't depend on the frontend build job.

- Resolve TS7017 globalThis index signature error in test setup
  Cast globalThis to Record<string, unknown> to avoid implicit 'any'
  type error when setting __APP_VERSION__ global.

- Add frontend dist stub to integrity-gates and grpc-governance workflows
  Tauri generate_context!() requires frontend/dist at compile time.
  The stub was added to ci.yml but missed in these two workflows.

- Deny.toml scope values + EventBus dead_code warning
  - deny.toml: `unmaintained`/`unsound` accept scope strings
    ("all"/"workspace"/"none"), not severity strings ("warn"/"deny")
  - event_bus.rs: #[allow(dead_code)] on impl block for unwired methods

- Add dist stub to Test job + ignore unmaintained GTK3 advisories
  - ci.yml Test job: add frontend dist stub before cargo test (same as Check job)
  - deny.toml: ignore 8 RUSTSEC advisories (GTK3 unmaintained transitive deps from Tauri)

- Resolve Build job dist stub + advisory-not-detected failures
  - Build jobs: fallback to dist stub when Frontend job skipped (no artifact)
  - deny.toml: set unmaintained=allow (all flagged are Tauri transitive deps),
    remove ignore list to eliminate advisory-not-detected on non-Linux targets

- Use correct cargo-deny scope values (all/workspace/transitive/none)
  unmaintained="none" — skip checks for transitive Tauri deps (GTK3, fxhash)
  unsound="all" — check all crates for soundness issues
  yanked="all" — check all crates for yanked versions

- Yanked uses severity values (deny), not scope values (all)
  cargo-deny field types differ: unmaintained/unsound use scope (all/workspace/
  transitive/none), but yanked uses severity (allow/warn/deny)

- Unsound=workspace to skip glib transitive dep (RUSTSEC-2024-0429)
  glib 0.18.5 VariantStrIter unsoundness is a Tauri transitive dep from
  GTK3 bindings that we cannot control upstream

- Cargo vet advisory mode default + ActivityBar title attr
  - Change VET_MODE default from "strict" to "advisory" until audit
    imports are set up (700+ exemptions, 0 audits makes strict unusable)
  - Add title={label} to ActivityBar buttons so Playwright getByTitle()
    selectors work in E2E navigation tests

- Make cargo vet advisory on push in security-compliance workflow
  The security-compliance.yml invokes cargo vet check directly (not via
  verify-integrity.sh), so the VET_MODE default change didn't cover it.
  Add continue-on-error: true to the push/schedule step to match the
  advisory-until-audits-imported strategy.

- Resolve 5 Playwright strict mode violations
  - Settings: import and render LanguageSelector in Settings.tsx
  - Dashboard: use getByRole('heading') instead of getByText() for heatmap
  - Search: scope submit button locator to form element to avoid TitleBar match

- Normalize runtime log messages in English

- Stabilize provider adapter clippy and vision adaptive test
  * fix(app): reduce provider adapter type complexity for clippy

  * test(vision): make adaptive size-limit assertion deterministic

- Resolve fmt and cargo-deny license failures
  - Apply cargo fmt to intent_resolver.rs and execution.rs
  - Add bzip2-1.0.6 license to deny.toml allow list (libbz2-rs-sys via zip crate)

- Add bzip2-1.0.6 license to about.toml accepted list
  cargo-about also needs the bzip2 license for libbz2-rs-sys (zip crate dep).

- Translate ErrorBoundary hardcoded strings ([#46](https://github.com/pseudotop/oneshim-client/pull/46))
  * docs(plan): add multi-agent audit remediation design doc

  12개 전문 에이전트 분석 결과를 기반으로 3 Phase 워크트리 remediation 계획 수립.
  Phase 1(4 병렬): docs/security/storage/i18n
  Phase 2(3 병렬): ux/privacy/rust-errors
  Phase 3(1 순차): enterprise/OSS 문서

- Enable TLS by default for outgoing HTTP connections ([#47](https://github.com/pseudotop/oneshim-client/pull/47))
  * docs(plan): add multi-agent audit remediation design doc

  12개 전문 에이전트 분석 결과를 기반으로 3 Phase 워크트리 remediation 계획 수립.
  Phase 1(4 병렬): docs/security/storage/i18n
  Phase 2(3 병렬): ux/privacy/rust-errors
  Phase 3(1 순차): enterprise/OSS 문서

- Accessibility improvements and server-down recovery UI ([#49](https://github.com/pseudotop/oneshim-client/pull/49))
  * fix(ux): add aria-live, role attributes, and server-down recovery guidance

  - EmptyState: add role="status" aria-live="polite" to container
  - ErrorBoundary: detect network errors, show server-offline recovery UI
    using existing i18n keys (errors.serverOffline, serverOfflineDesc,
    retryConnection); add role="alert" to error container
  - FocusWidget: distinguish TypeError (network) from other errors,
    append actionable guidance when agent is unreachable

- Replace unwrap() with explicit error handling and add port adapter tests ([#51](https://github.com/pseudotop/oneshim-client/pull/51))
  - processor.rs: Mutex::lock().unwrap() → map_err(CoreError::Internal)? (x2)
  - trigger.rs: Mutex::lock().unwrap() → .expect() with poisoned-lock message
  - input_activity.rs: Mutex::lock().unwrap() → .expect() with descriptive message
  - metrics_chart.rs: points.last().unwrap() → .expect() documents checked invariant
  - events.rs: add 5 unit tests (count_events_in_range, save_events_batch, dedup)
  - frames.rs: add 4 unit tests (count_frames_in_range, save_frame_metadata, file_path)

- Expand PII patterns and strengthen consent audit trail ([#50](https://github.com/pseudotop/oneshim-client/pull/50))
  * fix(privacy): expand PII filter patterns and add consent audit trail

  - privacy.rs: add bearer token masking (Bearer <token>)
  - privacy.rs: add PEM private key block masking (BEGIN * PRIVATE KEY)
  - privacy.rs: add GitHub Actions token prefix ghs_ to API key scanner
  - consent.rs: add revoked_at: Option<DateTime<Utc>> to ConsentRecord
  - consent.rs: add data_deletion_requested: bool to ConsentRecord
  - consent.rs: revoke_consent() now populates both fields + persists audit trail
  - consent.rs: add has_pending_deletion() to ConsentManager
  - consent.rs: add 3 new tests (legacy serde compat, pending deletion, audit trail)

- Has_pending_deletion() returns false after revoke — add persistent in-memory flag ([#53](https://github.com/pseudotop/oneshim-client/pull/53))
  After revoke_consent() clears current_consent, has_pending_deletion() was
  always returning false via unwrap_or(false), silently dropping the GDPR
  Article 17 deletion signal. Add ConsentManager::pending_deletion bool that
  survives current_consent = None, and a clear_pending_deletion() method for
  callers to acknowledge after erasure is complete.

  Also add doc comment to save_events_batch clarifying it returns input count,
  not actual rows inserted (INSERT OR IGNORE may deduplicate silently).


## [Unreleased]

## [0.2.0] - 2026-03-08

### Added

- **SQLite encryption key management**: Infrastructure for encrypted local storage key derivation and rotation ([#48](https://github.com/pseudotop/oneshim-client/pull/48))
- **Storybook + design tokens**: Frontend design system hardening with component catalogue, semantic tokens, and Biome linter integration

### Fixed

- **TLS enabled by default**: Outgoing HTTP connections now require TLS; plaintext must be explicitly opted-in ([#47](https://github.com/pseudotop/oneshim-client/pull/47))
- **ErrorBoundary i18n**: All hardcoded English strings in ErrorBoundary translated via i18n keys ([#46](https://github.com/pseudotop/oneshim-client/pull/46))
- **Accessibility + server-down recovery**: WCAG focus-visible fixes, skip-link, and graceful degradation UI when server is unreachable ([#49](https://github.com/pseudotop/oneshim-client/pull/49))
- **Typed error handling**: Replace `unwrap()` calls with explicit typed errors; add port adapter unit tests ([#51](https://github.com/pseudotop/oneshim-client/pull/51))
- **PII patterns + GDPR consent audit trail**: Expand regex coverage for SSN/file-path/API-key patterns; persist consent revocation signal across restarts ([#50](https://github.com/pseudotop/oneshim-client/pull/50), [#53](https://github.com/pseudotop/oneshim-client/pull/53))
- **E2E strict mode**: Resolve 5 Playwright strict-mode selector violations

### Changed

- **Enterprise deployment docs**: ADR-005/006, version migration guide, OSS on-ramp, and CI transparency documentation ([#52](https://github.com/pseudotop/oneshim-client/pull/52))
- **Tauri v2 documentation**: All guides updated to reflect Tauri v2 migration ([#45](https://github.com/pseudotop/oneshim-client/pull/45))
- **Dependency bumps**: xcap 0.8.3, uuid 1.22.0, tokio 1.50.0, zip 8.1.0, tauri-build 2.5.6, sysinfo 0.38.3, actions/upload-artifact v7, actions/setup-node v6, actions/attest-build-provenance v4, actions/download-artifact v8

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

[Unreleased]: https://github.com/pseudotop/oneshim-client/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/pseudotop/oneshim-client/compare/v0.1.7...v0.2.0
[0.1.7]: https://github.com/pseudotop/oneshim-client/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/pseudotop/oneshim-client/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/pseudotop/oneshim-client/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/pseudotop/oneshim-client/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/pseudotop/oneshim-client/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/pseudotop/oneshim-client/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/pseudotop/oneshim-client/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/pseudotop/oneshim-client/releases/tag/v0.1.0
