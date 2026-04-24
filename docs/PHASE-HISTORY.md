# Phase Implementation History

Detailed changelog of each implementation phase. For current crate structure and capabilities, see the Crate Summary in [CLAUDE.md](../CLAUDE.md).

## Phase 9 (Local Web Dashboard)
- `oneshim-web` crate: Axum 0.8 REST API + React frontend
- 11 endpoints, rust-embed static file serving, `WebConfig`

## Phase 10 (Usability Improvements)
- Settings/Privacy pages, API pagination, date filter
- GET/POST `/api/settings`, storage stats, data deletion endpoints

## Phase 11 (Search Feature)
- GET `/api/search` — integrated frames + events search (LIKE based)
- Search page with type filter, highlighting, pagination

## Phase 12 (Real-time Updates)
- GET `/api/stream` SSE endpoint (metrics, frame, idle, ping)
- `useSSE` React hook, connection status indicator

## Phase 13 (Activity Heatmap)
- GET `/api/stats/heatmap` — 7x24 grid activity heatmap

## Phase 14 (Config Persistence)
- `ConfigManager`: JSON config files with platform-specific paths

## Phase 15 (Enhanced Notifications)
- `NotificationManager`: cooldown-based (idle, long session, high usage)
- 8-loop scheduler with notification loop

## Phase 16 (Data Export)
- 3 export endpoints (metrics, events, frames) — JSON/CSV format

## Phase 17 (Dark/Light Theme)
- ThemeContext, Tailwind dark mode, localStorage persistence

## Phase 18 (Frontend Build Embedding + Auto Port)
- rust-embed for SPA, auto port finding (10090-10099)

## Phase 19 (Timeline + Keyboard Shortcuts)
- Lightbox, keyboard navigation, global shortcuts (D/T/S/P/?)

## Phase 20 (i18n)
- i18next: Korean (default) + English, browser language detection

## Phase 21 (Code-Based Design System)
- Design tokens, component variants (button/card/input/badge/select)
- `clsx` + `tailwind-merge` based variant system

## Phase 22 (Tags/Annotations)
- Tag system: SQLite V5 schema, 9 tag CRUD methods, REST API, frontend components

## Phase 23 (Tag-Based Search)
- `tag_ids` search parameter, tag filter UI, `TagInfo` in results

## Phase 24 (Reports/Stats)
- GET `/api/reports` — DailyStat, AppStat, HourlyActivity, ProductivityMetrics

## Phase 25 (Backup/Restore)
- GET `/api/backup`, POST `/api/backup/restore` — JSON backup/restore

## Phase 26 (E2E Tests)
- Playwright: 72 test cases across 7 spec files

## Phase 27 (Session Replay)
- GET `/api/timeline` — unified timeline (events + frames + idle)
- TimelineScrubber, EventLog, SessionReplay components

## Phase 28 (Edge Intelligence)
- SQLite V6: work_sessions, interruptions, focus_metrics, local_suggestions
- `FocusAnalyzer`: app switch detection, focus score, suggestion generation

## Phase 30 (SQLite Performance)
- V7 compound indexes, PRAGMA optimizations, batch save, N+1 removal

## Phase 31-33 (Edge Processing Performance)
- LRU thumbnail caching, lock-free batch queue, buffer pool + parallel I/O
- Compression stats encoding, delta pointer optimization, async OCR

## Phase 34 (Server Integration)
- CoreError extended (Network, RateLimit, ServiceUnavailable)
- HTTP retry with exponential backoff, session management API

## Phase 35 (Event Payload)
- Event enum: Input/Process/Window variants
- InputActivityCollector, WindowLayoutTracker, 9-loop scheduler

## Phase 36 (gRPC Client)
- `oneshim-network/src/grpc/`: auth, session, context, unified clients
- Feature flag `--features grpc`, port fallback, REST fallback

## Phase 37 (REST Standardization)
- Auth routes: `/api/v1/auth/tokens` resource-centric REST
- gRPC fallback ports config, legacy route removal

## Privacy & Permission Control System
- Tier 1: TelemetryConfig, MonitorConfig
- Tier 2: PrivacyConfig, PiiFilterLevel, ScheduleConfig, FileAccessConfig
- Tier 3: ConsentManager (GDPR), oneshim-automation crate

## Phase 4.5
- Auto start (macOS LaunchAgent, Windows Registry), OCR module, install scripts

## Phase 5 (Auto Update)
- GitHub Releases API updater, platform detection, tar.gz/zip decompression

## Phase 6 (GA Preparation)
- CI/CD: GitHub Actions, 4 platform builds, installers (app/msi/deb)

## Phase 8 (Additional Data Storage)
- System metrics, process snapshots, idle detection, session stats
- Schema V3-V4, 6-loop scheduler

## Phase 38: Tauri v2 마이그레이션 (v0.1.5, 2026-03-04)

- iced GUI 제거, Tauri v2 + React WebView로 전환
- `oneshim-ui` 크레이트 제거
- `src-tauri/` 디렉토리 신설 (main binary entry point)
- System tray: iced tray → Tauri MenuBuilder
- 데스크탑 알림: iced notification → Tauri notification plugin

## Phase 39: Desktop Shell Layout (v0.1.6)

- Tauri WebView 내 React shell 레이아웃 구현
- Sidebar navigation, command palette (Cmd+K)
- Dark/light theme 3-mode 지원
- 접근성 기초: skip-to-content, semantic HTML

## Phase 40: Vitest Infrastructure (v0.1.7)

- Frontend 테스트 인프라: Vitest + React Testing Library
- MSW (Mock Service Worker) API mocking
- E2E: Playwright screenshot capture

## Phase 2: Config Change Bus + Telemetry Exporter (v0.4.x, 2026-04-17)

- `ConfigManager` 내부 `tokio::sync::watch::Sender<Arc<AppConfig>>` 기반 runtime config broadcast bus
- `TelemetryHandle` + `tracing_subscriber::reload::Layer` — OTel exporter 를 재시작 없이 swap
- `src-tauri` 의 `telemetry` Cargo feature 뒤로 OTLP 의존성/런타임 export 머신어리 격리 (기본 빌드 오버헤드 0)
- Design + plan: `docs/reviews/2026-04-17-phase2-config-telemetry-{design,plan}.md`

## Phase 3: FeedbackSignalSink + regime_id + RegimeManager Persistence (v0.4.x, 2026-04-18)

- `FeedbackSignalSink` port + `CompositeFeedbackSink` — user feedback 를 `CoachingEngine` + `RegimeClassifier` 로 fan-out (fire-and-forget, ~10ms 지연 예산)
- `search_filtered` / `search_quantized` 의 silent-ignore warning → 실제 `WHERE activity_segments.regime_id` 필터
- `RegimeManager` state 를 startup hydrate + graceful shutdown persist
- Design + plan: `docs/reviews/2026-04-18-phase3-regime-feedback-learning-{spec,plan}.md`

## Phase 4: Updater Hardening (D9 + D10 + D11) (v0.4.x, 2026-04-18)

- **D9** Multi-key Ed25519 trust array (`TRUSTED_PUBLIC_KEYS` in `src-tauri/src/updater/trusted_keys.rs`) — built-in 키 리스트가 권위; 사용자 override 보다 선행
- **D10** Defensive rollout — `check_for_updates_from` 은 `installation_id` 부재 시 rollout-EXCLUDED 처리; `<!-- rollout:N -->` 규약
- **D11** Post-install self-healthy probe + automatic rollback (`.install_pending_{VER}`/`.boot_count_{VER}`/`.self_healthy_{VER}` state files; 2회 연속 실패 시 이전 binary 복원, macOS + Linux 에서만; Windows 는 문서화된 no-op)
- Design + plan: `docs/reviews/2026-04-18-phase4-updater-hardening-{design,plan}.md`

## Phase 5-D8: Storage Test Backfill (v0.4.x, 2026-04-18)

- 저장소 crate 의 테스트 gap backfill — CHECK violation 유발 기법, invalid payload injection, mutex poisoning tractable path 커버 (+27 new tests)
- Design: `docs/reviews/2026-04-18-phase5-d8-storage-test-backfill-spec.md`

## ADR-019: Error Code Infrastructure + C5 AWS Bedrock Skip (v0.4.39-rc.1, 2026-04-19)

- 18 typed code enums under `crates/oneshim-core/src/error_codes/` via single-source `define_code_enum!` macro
- 30 `CoreError` variants (28 struct + 2 `#[from]`-wrapped) + 8 `GuiInteractionError` variants with typed `code: XxxCode` field; `err.code() -> &'static str` unified accessor
- Wire-format contract locked via `tests/wire_contract_snapshot.{rs,expected.txt}` — **41 codes** (57 → 41 after YAGNI cleanup)
- C5: AWS Bedrock catalog-deleted; 8 match arms + 2 defense-in-depth guards return `ConfigCode::UnsupportedProviderBedrock`; OCR silent no-auth fallthrough closed
- ~1,030 callsites retrofitted via 4-phase soft-migration (V1→V2→V1-rename); post-merge drift audit iter 87~177 removed 16 orphan wire codes + 15 dead adapter-error variants + ~138 `Internal.Generic` re-routes + 62/62 port `# Errors` doc standardization + HTTP status mapping canonicalization + doc-org convergence
- ADR + companion: `docs/architecture/ADR-019-error-code-infrastructure.{md,ko.md}`; CHANGELOG `[Unreleased]` line 12 carries the full summary

## Phase 9: Tracking Schedule Privacy Primitive (2026-04-24)

Phase 9 PR-A introduces a **tracking-schedule** privacy primitive — a configurable allow-list of day-of-week + time-of-day windows during which the agent is permitted to capture context. When the current instant falls outside all configured windows the agent silently suppresses capture, batch upload, and notifications without any user friction.

The implementation spans 21 commits across the full client stack: `TrackingScheduleConfig` types with `chrono-tz`-aware window matching (A.1–A.3), a 4-term composite gate `capture_permitted_now` that ANDs consent + active_hours + tracking_schedule + pause (A.5), extraction of `tracking_schedule_helper.rs` from the monitor loop to respect the 500-line guardrail (A.7), gating of all 9 data-producing scheduler loops + batch-upload flush (A.9), a `BatchUploader::with_suppression_predicate` builder for real-time upload gating (A.10–A.12), Tauri IPC commands `get_tracking_schedule`/`set_tracking_schedule` with settings allowlist wiring (A.13–A.14), three REST endpoints (`GET/PUT /api/tracking-schedule`, `GET /api/tracking-schedule/status`) with http-interface-manifest + OpenAPI registration (A.15–A.16), tray tooltip propagation via `ConfigManager::subscribe` (A.17), `DesktopNotifier` window-enter/exit notifications with 60 s debounce (A.18), and a `TrackingScheduleSettings` React component wired into the Settings layout (A.19–A.20). Total new tests: +147 (3,651 → 3,798 workspace-passed).

- Scope: `oneshim-core` config types, `oneshim-network` uploader, `src-tauri` scheduler + IPC + tray, `oneshim-web` REST handlers + React frontend
- Design + plan: `docs/reviews/2026-04-24-phase9-tracking-schedule-{spec,plan}.md`
