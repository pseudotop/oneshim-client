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
- rust-embed for SPA, auto port finding (9090-9099)

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
