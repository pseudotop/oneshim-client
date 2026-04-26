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

## Phase 9 PR-B1: Autostart Foundation (v0.4.40-rc.1, 2026-04-25)

- **Cross-platform autostart IPC commands**: macOS `LaunchAgent`, Windows Registry `HKCU\...\Run`, Linux systemd user service + XDG autostart desktop file — wired to Settings UI toggle (`StartupSection` in `GeneralTab.tsx` between Language and Web Dashboard cards)
- **Single-instance enforcement**: `tauri-plugin-single-instance` v2 with D-Bus name `com.oneshim.client.SingleInstance` (Linux), named pipe (Windows), Unix socket (macOS); focus-grab callback (show → unminimize → set_focus); D-Bus presence check warn log on Linux for headless degradation
- **Opt-in onboarding prompt**: triggers after first 25-minute productive focus session; single-fire per app session via module-level guard; handlers for snooze/dismiss/enable; rendered in `DashboardLayout` above `<Outlet>` (per memory `feedback_shared_chrome_in_layout`)
- **AutostartConfig in oneshim-core**: zero-cache design — OS state is sole source of truth (Phase 1 review I4 requirement); no in-memory enabled flag; only stores `prompt_state` (Pending/Snoozed/Dismissed) + `productive_session_count` + `last_session_id` UUID for idempotency
- **Productive-session detection**: monitor.rs `FocusBlockState` integrated with `handle_idle_tick` flow — `Idle→Active` block start, `Active→Idle` block end with increment via `ConfigManager.update_with` (closure-based, idempotent via session_id UUID); Generic Runtime pattern (`<R: Runtime>`) for testability without `tauri::test`
- **IPC commands**: 6 total — `enable_autostart`, `disable_autostart`, `is_autostart_enabled`, `autostart_capabilities` (PR-B2 will populate real Linux env detection), `mark_autostart_prompt_state`, `get_autostart_config`
- **Wire codes**: 5 registered via `define_code_enum!` macro (ADR-019) — `autostart.{enable,disable,query,counter_increment,event_emit}_failed`
- **Tests**: +20 Rust unit tests (AutostartConfig serde/should_prompt 10, autostart_helper 5, IPC commands 2 + 1 ignored round-trip, AutostartCode 3) + 10 Vitest tests (StartupSection 5, AutostartOnboardingPrompt 5) + 1 integration smoke test (single_instance subprocess spawn, ignored)
- **Implementation**: 13 commits across Tasks 1-13 on branch `feature/phase9-autostart-foundation` + 2 fixes (Task 10 ADR-019 wire code registration + Task 14 i18n test count adjustment) + holistic review polish
- **Spec + plan**: `docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md` (v3) + `docs/superpowers/plans/2026-04-25-phase9-pr-b1-autostart-foundation.md` (v2.6)
- **Followup**: PR-B2 (Linux deep — sd-notify Type=notify, Snap/Flatpak/headless detection, capability-aware UI gating, Linux integration tests) on separate branch after PR-B1 merges

## TimeWindow Primitive Refactor (v0.4.42-rc.1, 2026-04-26)

Consolidated 9+ divergent absolute-timestamp time-range types — REST `from`/`to` query strings, SQL `from: &str, to: &str` parameter pairs, `(DateTime<Utc>, DateTime<Utc>)` tuples in `FocusMetrics` and `SessionMetrics`, ad-hoc `from_datetime()`/`to_datetime()` helpers — into a single canonical `TimeWindow { start: DateTime<Utc>, end: DateTime<Utc> }` primitive at `oneshim_core::types`. Closed-closed `[start, end]` semantic (matches existing SQL `BETWEEN` and Stripe-style business API expectations per spec U4).

**Migration scope (8 commits across 11 plan tasks, 2 atomic merges)**:
- `TimeRangeQuery::to_time_window(default_lookback)` + `TimelineQuery::to_time_window(default_lookback)` adapters bridging optional REST bounds → required `TimeWindow`
- `DeleteRangeRequest::period()` accessor (Option C — preserves frontend JSON shape trivially, no custom serde)
- 8 SQL port-trait methods migrated to `&TimeWindow` (`count_events_in_range`, `count_frames_in_range`, `list_frame_file_paths_in_range`, `delete_data_in_range`, `get_daily_active_secs`, `flag_noise_range`, `get_entries`, `list_segment_time_ranges` — last one consolidates 3-tuple `(String, DateTime, DateTime)` → 2-tuple `(String, TimeWindow)`)
- 5 SQLite impl files updated PRESERVE-BODY (sig swap + `let (from, to) = window.to_sql_pair();` only — SQL strings, lock-error wrapping, half-open `started_at < ?2` per NG6, calibration containment `start_time >= ?1 AND end_time <= ?2` all preserved bit-identical)
- 7 service layer files migrated (frames/events/metrics/focus/idle/processes/timeline) from `from_datetime()`+`to_datetime()` helpers to `params.to_time_window(default_lookback)?` validating conversion; 2 service files (data/reports) refactored to use accessor pattern
- `FocusMetrics` + `SessionMetrics` domain models: replaced `(period_start, period_end)` pair with `period: TimeWindow`; constructor returns `Result<Self, TimeWindowError>`; 10 caller sites migrated per per-fixture pattern (Pattern A constructor for default-fields sites, Pattern B renamed struct literal for custom-seeded test fixtures to preserve non-period values)
- `resolve_report_window` returns `Result<(TimeWindow, String), ApiError>`; downstream caller decomposes via `let TimeWindow { start: from, end: to } = window;` for out-of-plan-scope storage methods
- 30+ caller sites migrated lockstep (services, mocks, regime detection scheduler, internal SQLite tests)

**Wire codes (ADR-019)**: 2 new typed codes — `time_window.inverted_bounds` + `time_window.parse_failed` via `define_code_enum!` macro. Wire snapshot 47 → 49. i18n `wire-errors.{en,ko}.json` updated with both translations; `translateError.test.ts` `toHaveLength(47) → toHaveLength(49)` at lines 31 + 123 + describe titles. `CoreError::TimeWindow { code: TimeWindowCode, message: String }` follows ADR-019 §4.6 majority struct-variant pattern with manual `From<TimeWindowError>` impl. `ApiError::From<CoreError>` arm: `TimeWindow → 400 BadRequest`.

**Tests (+37)**: 13 TimeWindow primitive unit + 3 TimeWindowCode + 8 TimeRangeQuery adapter + 3 SQL boundary regression (closed-closed inclusion via canonical `+00:00` format because `chrono::DateTime::to_rfc3339()` emits `+00:00` not `Z`) + 4 E2E (closed-closed inclusion + DeleteRangeRequest external shape + inverted bounds → 400 + invalid RFC3339 → 400) + 2 ApiError mapping (InvertedBounds + ParseFailed → 400) + 4 api-contracts roundtrip (DeleteRangeRequest external_shape + period accessor variants).

**External API contract — query string shape preserved**: REST query strings unchanged (`?from=...&to=...&limit=...`); DeleteRangeRequest JSON shape preserved via accessor pattern; ReportQuery date-only `%Y-%m-%d` schema preserved per Phase 2 iter-11 corrected approach (TimeRangeQuery `to_time_window` would fail on date-only input).

**Behavior change #1 — invalid timestamp handling**: requests with malformed `from`/`to` query strings now return HTTP 400 BadRequest with parse error message. Previously: silently fell back to defaults (`from = Utc::now() - 24h`, `to = Utc::now()`) and returned 200 OK with default-window data. Strict API contract improvement (frontend should receive validation errors instead of empty results).

**Behavior preserved — default-window size**: `to_time_window(Duration::hours(24))` matches existing `from_datetime()` 24h fallback exactly. `TimelineQuery` uses `Duration::hours(1)` matching its existing 1h fallback. NO change for missing-bounds requests.

**Behavior preserved — half-open vs closed-closed boundaries (NG6)**: SQL `BETWEEN`-style queries use closed-closed; `work_sessions::get_daily_active_secs` half-open `started_at < ?2` preserved (intentional — work_sessions started_at is an instant; closing upper bound would double-count at day rollovers); `calibration::list_segment_time_ranges` containment semantic `start_time >= ?1 AND end_time <= ?2` (different columns) preserved per spec v15 §5.3.

**Helpers retained**: `TimeRangeQuery::from_datetime()` / `to_datetime()` / `limit_or_default()` / `offset_or_default()` kept (non-deprecated) for non-validating use cases (test fixtures, demos, internal tooling). New code uses `to_time_window` for validating conversion.

**Workspace sweep evaluation**: `ExportQuery` (export.rs) + `ListOverridesQuery` (recalibration.rs) retain `from`/`to: Option<String>` shape — their service callers use out-of-plan-scope storage methods (backup queries, override lists). Future PR can add `to_time_window`/`period()` accessors when those storage methods migrate. `ReportQuery` (date-only schema) intentionally not flattened. `RegimeChanged.from/to` are regime IDs not timestamps.

**Spec + plan**: `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md` (v15 — 28 documentation iterations: 15 spec versions + 13 plan versions; ~37 Critical + ~45 Important + ~12 Suggestion findings addressed via Phase 1 + Phase 2 deep reviews). `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (v13, ~2885 lines).
