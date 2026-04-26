# TimeWindow Primitive Refactor Design Spec

**Date:** 2026-04-25
**Version:** v3 (Phase 1 iter-2 cleanup: 1 Critical + 3 Important stale-reference fixes)
**Review history:**
- v1 (2026-04-25): initial design
- v2 (2026-04-25): Phase 1 iter-1 review fixes (4 Critical + 5 Important + 4 Nice-to-have)
- v3 (2026-04-25): Phase 1 iter-2 stale-reference cleanup + Q-4/Q-6/Q-7 resolution
**Baseline:** main `2ba38cf5` (post-PR #510 docs naturalize)
**Target release:** v0.4.42-rc.1 (after PR-B1 #508 + PR-B2 land)
**Implementation gate:** PR-B1 (#508) MUST merge before Phase 3 starts (rebase pain on `oneshim-core/config/sections/`)
**Estimated effort:** ~3-4 day implementation + ~6-8h spec/plan deep review = ~1 week total
**Authoring source:** Brainstorming session 2026-04-25 (5 user-locked decisions U1-U5)

---

## 1. Background & Current State

### 1.1 The Problem

Investigation of the codebase identified **5 main + 4 supporting sites** where time-range / time-window types are defined or used in DIVERGENT formats. No canonical `TimeWindow` type exists.

**Format split observed**:
- **HH:MM string bounds** (wall-clock recurrence): `TrackingWindow` (PR-A merged), coaching `TimeRange`
- **ISO8601 string bounds**: `TimeRangeQuery`, `DeleteRangeRequest`, `ReportQuery`
- **DateTime<Utc> bounds**: `FocusMetrics period_start/end`, `IdlePeriod`, `SessionMetrics`, calibration `flag_noise_range`

### 1.2 Per-site catalog

| # | Site | File:line | Format | Domain |
|---|------|-----------|--------|--------|
| 1 | `TrackingWindow` | `crates/oneshim-core/src/config/sections/tracking_schedule.rs:97-108` | HH:MM strings + days_of_week | Wall-clock recurrence (PR-A) |
| 2 | `TimeRange` (coaching quiet hours) | `crates/oneshim-core/src/config/sections/coaching.rs:120-125` | HH:MM strings | Wall-clock recurrence |
| 3 | `TimeRangeQuery` | `crates/oneshim-api-contracts/src/common.rs:5-11` | RFC3339 ISO8601 strings, optional bounds | REST API params (6 production handlers per Phase 2 iter-1 I3 — events/metrics/idle/focus/processes/frames; mod.rs is test fixture only; sessions/interruptions don't use TimeRangeQuery) |
| 4 | `FocusMetrics period_*` | `crates/oneshim-core/src/models/work_session.rs:273-284` (struct), 286+ (impl block) | `DateTime<Utc>` pair | Daily/weekly aggregates. 10 call sites total per Phase 2 iter-12 enumeration. |
| 5 | `DeleteRangeRequest` | `crates/oneshim-api-contracts/src/data.rs:4-9` | ISO8601 strings (RFC3339) | GDPR data purge — Phase 2 iter-1 C9 resolves via Option C accessor pattern (NO custom serde) |
| 6 | `IdlePeriod` | `crates/oneshim-core/src/models/activity.rs:20-24` | `DateTime<Utc>` + `Option<DateTime<Utc>>` | Idle session tracking — NOT migrated per NG7 |
| 7 | `SessionMetrics period_*` | `crates/oneshim-core/src/models/telemetry.rs:14` (struct), 16-17 (fields) | `DateTime<Utc>` pair | Telemetry window. **Possibly dead code** — Phase 2 iter-1 I1 found zero production callers; migrating for consistency only. |
| 8 | `ReportQuery` | `crates/oneshim-api-contracts/src/reports.rs:13` (struct), 14-18 (fields) | **Date-only `%Y-%m-%d` strings** (NOT RFC3339) per Phase 2 iter-11 finding | Weekly/monthly reports — schema preserved per NG11 |
| 9 | SQL storage helpers | `crates/oneshim-storage/src/sqlite/{events,frames,calibration_store_impl,web_storage_impl,maintenance,edge_intelligence/work_sessions}.rs` | 8 specific port methods migrated (Task 4 enumeration); ~30 caller sites total (Step 4D.0 enumeration) including services + tests + mocks | Range queries — Phase 2 iter-1 C6/C7 expanded scope |

### 1.3 Why now

1. **Phase 9 PR-B (autostart) parallel work** revealed `oneshim-core/config/sections/` becoming hot zone — good time to consolidate similar primitives
2. **Reviewer cognitive load** scaling — 9+ slightly-different time range types is hard to onboard new contributors
3. **Future time-bucketing features** (sliding windows, hourly aggregates) will benefit from a clear primitive base

---

## 2. Goals & Non-Goals

### 2.1 Goals
1. **G1**: Single canonical `TimeWindow` type for all absolute-timestamp window/range needs across the workspace
2. **G2**: REST API external contract preserved (`?from=...&to=...` query params unchanged) — backward compat
3. **G3**: SQL storage helper signatures simplified (one `&TimeWindow` argument vs current `(from, to)` pair)
4. **G4**: Domain models (`FocusMetrics`, `IdlePeriod`, `SessionMetrics`, `DeleteRangeRequest`, etc.) use `TimeWindow` instead of separate fields
5. **G5**: Migration is atomic (Big-bang per U2) — no half-migrated state in main branch

### 2.2 Non-Goals
- **NG1**: Wall-clock recurrence types (`TrackingWindow`, coaching `TimeRange`) are NOT migrated. Different domain (recurrence vs absolute window). Per iCalendar precedent.
- **NG2**: Time-bucketing primitives (`TimeBucket { start: Utc, duration: Duration }` for sliding windows / 5-min aggregates) — defer to future PR if/when needed
- **NG3**: REST API external contract changes (`?from`/`?to` query string format stays)
- **NG4**: Frontend type changes — TypeScript types unchanged (boundary remains JSON ISO8601 strings)
- **NG5**: Time-zone handling overhaul — `TimeWindow` always uses `DateTime<Utc>` internally (existing convention preserved)
- **NG6**: SQL `BETWEEN` semantic changes — current closed-closed `WHERE timestamp >= ?1 AND timestamp <= ?2` preserved
- **NG7** (per Phase 1 iter-1 I4): `IdlePeriod` is NOT migrated. `IdlePeriod.end_time: Option<DateTime<Utc>>` represents ongoing idle (renewed each poll). Migrating to `TimeWindow` (always-bounded) would require either fragmenting into 2 types (overkill) or `end = now()` workaround (drift bug — values changes per poll, breaks equality + serialization stability). Left as-is.
- **NG8** (per Phase 1 iter-1 I1): `FocusMetrics` JSON shape change is internal-only. The REST contract serializes `FocusMetricsDto` (in `oneshim-api-contracts/src/focus.rs`) which has `date: String` + scalars — NO `period_start/period_end` fields. Verified frontend has zero references to `period_start/period_end`. **Option Z (break internal model JSON shape) is safe**. Q-1 resolved. Saves ~3h custom serde work.
- **NG9** (per Phase 2 iter-9 NEW-C1): REST handlers are NOT migrated. Migration happens at the **service layer** (`crates/oneshim-web/src/services/`). Handlers stay thin pass-through (`Service::new(ctx).method(&params)?`). 7 service files migrate (frames/events/metrics/focus/idle/processes/timeline_service); handlers/{frames,events,metrics,focus,idle,processes,data,reports}.rs require ZERO changes.
- **NG10** (per Phase 2 iter-10 NEW-C1): Default lookback in `to_time_window()` calls **preserved at `Duration::hours(24)`** to match existing `TimeRangeQuery::from_datetime()` fallback exactly. Plan v9 originally prescribed 7d/30d defaults — that would 7×/30× widen payloads when frontend sends no bounds. Any deliberate widening must be a separate PR with frontend coordination.
- **NG11** (per Phase 2 iter-11 NEW Critical): `ReportQuery` schema **preserved as-is** — `from: Option<String>, to: Option<String>` are date-only `%Y-%m-%d` strings (NOT RFC3339), parsed via `NaiveDate::parse_from_str`. NO `#[serde(flatten)] time_range: TimeRangeQuery` (would break Custom period parse since `to_time_window` expects RFC3339). Migration updates `resolve_report_window` in `reports_query_support.rs` to construct TimeWindow from existing NaiveDate parse logic.
- **NG12** (per Phase 2 iter-9 + iter-10 decision): `TimeRangeQuery` helper methods (`from_datetime`, `to_datetime`, `limit_or_default`, `offset_or_default`) **retained non-deprecated**. Useful for non-validating use cases (test fixtures, demos, internal tooling). New code uses `to_time_window` for validating conversion. Future deprecation is a separate concern.

---

## 3. User-Locked Decisions (U1-U5)

These decisions were made interactively during brainstorming and are FIXED.

| ID | Decision | Rationale |
|----|----------|-----------|
| **U1** | Scope = Option A (Absolute timestamps only) | Industry standard (iCalendar separates DTSTART/DTEND from RRULE; Prometheus/OTel time-series unify absolute, not recurrence). Wall-clock sites only 2 — YAGNI. |
| **U2** | Migration = Big-bang (single PR) | Deep review process (3-loop ralph-loop) absorbs large-PR risk. Avoids type-alias deprecation churn from gradual approach. |
| **U3** | Location = `oneshim-core` (`crates/oneshim-core/src/types/time_window.rs`) | Domain primitive home. SQL storage already depends on oneshim-core. Layering clean. |
| **U4** | Boundary = Closed-closed `[start, end]` | ONESHIM is event-driven business API (Stripe-style), not continuous time-series (Prometheus-style). User-facing date queries dominate. Existing SQL `BETWEEN` semantic preserved → migration risk zero. |
| **U5** | Optional bounds handling = `TimeRangeQuery::to_time_window(default_lookback)` adapter | Domain-specific defaults configurable per call site. **Per Phase 2 iter-10 NEW-C1: default value preserved at `Duration::hours(24)` everywhere** to match existing `from_datetime()` 24h fallback (see NG10). TimeWindow type stays simple (always bounded). REST contract unchanged. |

---

## 4. Architecture Overview

### 4.1 Component Layout

```
[NEW]
crates/oneshim-core/src/types/                  ← NEW directory (currently no `types/` dir)
  ├── mod.rs                                     ← `pub mod time_window;`
  └── time_window.rs                             ← TimeWindow struct + impl + 13 tests
crates/oneshim-core/src/error_codes/time_window.rs ← TimeWindowCode enum + 3 tests
crates/oneshim-web/tests/timewindow_integration.rs ← 4 E2E tests

[MODIFIED — registration in oneshim-core (Phase 1 iter-1 C2/C3/I5 + Phase 2 iter-1 C1)]
crates/oneshim-core/src/lib.rs                  ← add `pub mod types;`
crates/oneshim-core/src/error_codes/mod.rs      ← `pub mod time_window;` + `pub use TimeWindowCode;` + `for c in TimeWindowCode::all() ...` in `all_codes()`
crates/oneshim-core/src/error.rs                ← add **struct-variant** `TimeWindow { code: TimeWindowCode, message: String }` to CoreError (NOT `#[from]` tuple — matches ADR-019 §4.6 majority pattern) + manual `From<TimeWindowError>` impl + `Self::TimeWindow { code, .. } => code.as_str()` arm in code()
crates/oneshim-core/tests/wire_contract_snapshot.expected.txt ← +2 entries (`time_window.inverted_bounds`, `time_window.parse_failed`)

[MODIFIED — port traits in oneshim-core (Phase 2 iter-1 C6 + iter-2 N-C4/N-C5)]
crates/oneshim-core/src/ports/calibration_store.rs ← 3 methods: flag_noise_range (sync, Result<u64>), get_entries (async), list_segment_time_ranges (async, returns Vec<(String, TimeWindow)> preserving segment_id)
crates/oneshim-core/src/ports/web_storage.rs ← 5 methods: count_frames_in_range, list_frame_file_paths_in_range, count_events_in_range, delete_data_in_range (5 bool flags preserved), get_daily_active_secs (returns Vec<(String, i64)>)

[MODIFIED — domain models (NG7/NG8 + Phase 2 iter-12 Pattern A/B)]
crates/oneshim-core/src/models/
  ├── work_session.rs:273-284 (FocusMetrics struct), 286+ (impl block) ← period_* → period: TimeWindow (Option Z per NG8 — internal model only, NOT in REST DTO). 10 call sites with Pattern A (constructor) vs Pattern B (struct-literal preserves custom seeded fields) per Phase 2 iter-12.
  └── telemetry.rs:14 (struct), 16-17 (fields) ← SessionMetrics: period_* → period: TimeWindow (possibly dead code per Phase 2 iter-1 I1)

  (activity.rs IdlePeriod is NOT migrated per NG7)

[MODIFIED — API contracts (Phase 2 iter-1 C4/C9 + iter-11)]
crates/oneshim-api-contracts/src/
  ├── common.rs:5-11                             ← TimeRangeQuery: add `Default` derive (per Phase 2 iter-1 C4) + `to_time_window(&self, Duration)` adapter (Phase 1 iter-1 C4: non-consuming `&self`)
  ├── data.rs:4-9                                ← DeleteRangeRequest: keep `from: String, to: String` UNCHANGED + add `period() -> Result<TimeWindow, TimeWindowError>` accessor (Option C per Phase 2 iter-1 C9). NO custom serde. Frontend DataSection.tsx unchanged.
  └── reports.rs:13                              ← ReportQuery: keep date-only `%Y-%m-%d` schema UNCHANGED per NG11 (Phase 2 iter-11 NEW Critical — flatten of TimeRangeQuery would break Custom period parse)

[MODIFIED — REST handlers UNCHANGED per NG9; service layer migrates (Phase 2 iter-9 NEW-C1)]
crates/oneshim-web/src/services/                 ← 7 service-layer files migrate (Phase 2 iter-9):
  ├── frames_service.rs                          ← get_frames uses params.to_time_window(Duration::hours(24))
  ├── events_service.rs                          ← get_events same pattern (also covers Step 4D.3 events_service.rs:35)
  ├── metrics_service.rs                         ← daily aggregates same pattern
  ├── focus_service.rs                           ← 4 sites (get_work_sessions + get_interruptions)
  ├── idle_service.rs                            ← idle queries (model NOT migrated per NG7)
  ├── processes_service.rs                       ← process queries
  ├── timeline_service.rs                        ← timeline queries
  ├── data_web_service.rs                        ← uses request.period()? accessor (Option C), 2 caller sites at lines 36+51
  ├── reports_service.rs:30                      ← consumes resolve_report_window result
  ├── reports_query_support.rs:14-44             ← resolve_report_window updated (Phase 2 iter-11): returns Result<(TimeWindow, String), ApiError>; preserves NaiveDate parse logic
  ├── stats_query_support.rs:112                 ← total_active_secs_for_range uses let-else (returns u64, no Result)
  └── (handlers themselves: ZERO changes per NG9 — thin pass-through to services)

[MODIFIED — SQL storage (Phase 2 iter-1 C6/C7 expanded; PRESERVE-BODY for complex methods)]
crates/oneshim-storage/src/sqlite/
  ├── events.rs:14                               ← count_events_in_range (SAFE-SYNTHETIC) + 4 internal test sites
  ├── frames.rs:10                               ← count_frames_in_range (SAFE-SYNTHETIC) + 2 internal test sites
  ├── maintenance.rs:253, 286                    ← list_frame_file_paths_in_range, delete_data_in_range (PRESERVE-BODY: 5 bool flags + system_metrics_hourly companion DELETE + idle_periods.start_time column) + ~9 internal test sites
  ├── calibration_store_impl.rs:120, 148, 237    ← flag_noise_range (sync) + get_entries (async with_conn) + list_segment_time_ranges (async with_conn + table_exists guard) PRESERVE-BODY + 5 internal test sites at lines 400/414/420/425/443
  ├── web_storage_impl.rs:82, 105, 126, 169, 246 ← 5 thin wrappers (delegate to inherent pub fn)
  └── edge_intelligence/work_sessions.rs:216     ← get_daily_active_secs PRESERVE-BODY (half-open `started_at < ?2` per NG6)

[MODIFIED — src-tauri scheduler caller sites (Phase 2 iter-1 C6 + iter-2 N-C2)]
src-tauri/src/scheduler/analysis_pipeline/
  ├── regime.rs:44                               ← get_entries call (run_periodic_regime_detection, () return → use .expect())
  ├── regime.rs:174                              ← list_segment_time_ranges call (run_constrained_clustering, () return)
  ├── regime.rs:184                              ← second get_entries call (re-fetch for index mapping; reuses window from line 174)
  ├── regime.rs:194                              ← destructure (seg_id, seg_window) using TimeWindow::contains(e.timestamp)
  └── tests.rs:12-31                             ← NoopCalibrationWriter (sync flag_noise_range) + NoopCalibrationReader (async get_entries; list_segment_time_ranges uses trait default)

[MODIFIED — test mocks]
crates/oneshim-web/tests/support/failing_storage.rs ← 5 sites (delegation pattern preserved per Phase 2 iter-3 NEW-C1): each method delegates to self.inner.method(window).map_err(Into::into)
src-tauri/src/focus_analyzer/mod.rs:384, 420, 442 ← 3 FocusMetrics test fixtures (Pattern B per Phase 2 iter-12)
crates/oneshim-web/tests/grpc_dashboard_integration.rs:461 ← FocusMetrics test fixture with 10+ custom seeded values (Pattern B — MUST preserve struct literal)
```

### 4.2 What is NOT touched

Verified via cross-layer audit (Phase 2 iter-9 + post-iter-13 grep across all caller layers):

- `TrackingWindow` in `tracking_schedule.rs` — wall-clock recurrence (different domain) — NG1
- coaching `TimeRange` in `coaching.rs` — wall-clock recurrence — NG1
- All frontend TypeScript code — REST API JSON shape unchanged (NG3+NG4) + DeleteRangeRequest preserves shape via Option C accessor (NG12 helpers retained)
- **Tauri IPC commands** (`src-tauri/src/commands/`) — verified ZERO TimeRangeQuery / migrated SQL helper consumers via grep audit
- **gRPC server handlers** (`crates/oneshim-web/src/grpc/`) — verified ZERO TimeRangeQuery / migrated SQL helper consumers via grep audit
- **Network crate** (`crates/oneshim-network/src/`) — verified ZERO TimeRangeQuery / from_datetime / to_datetime consumers via grep audit
- gRPC streaming `MetricBucket` (different concept — bucketed time series, deferred per NG2)
- REST handler files (`crates/oneshim-web/src/handlers/`) — handlers stay thin pass-through per NG9; service layer migrates instead
- IdlePeriod ongoing-idle model (`activity.rs`) per NG7 — open-ended `Option<DateTime<Utc>>` end_time can't be represented as bounded TimeWindow without semantic drift

---

## 5. Components Detail

### 5.1 `TimeWindow` Type Definition

**File**: `crates/oneshim-core/src/types/time_window.rs` (NEW)

```rust
//! Canonical time window primitive — closed-closed `[start, end]` absolute window.
//!
//! Per spec U4: ONESHIM is event-driven business API (Stripe-style), not
//! continuous time-series. Closed-closed semantic matches existing SQL `BETWEEN`
//! and user-facing date range expectations.
//!
//! Wall-clock recurrence types (`TrackingWindow`, coaching `TimeRange`) are
//! intentionally NOT unified — different domain (recurrence vs absolute window).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Closed-bounded absolute time window. Both `start` and `end` are inclusive.
///
/// Validates `start <= end` at construction. Internally always uses `DateTime<Utc>`.
/// External serialization round-trips via RFC3339 ISO8601 strings.
///
/// Per Phase 1 iter-1 N1: `Hash` derive removed (no current use case as HashMap key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TimeWindowError {
    #[error("start ({start}) must be <= end ({end})")]
    InvertedBounds { start: DateTime<Utc>, end: DateTime<Utc> },
    #[error("failed to parse RFC3339 timestamp: {0}")]
    ParseFailed(String),
}

impl TimeWindowError {
    /// Wire code for ADR-019 observability grouping.
    pub fn code(&self) -> TimeWindowCode {
        match self {
            Self::InvertedBounds { .. } => TimeWindowCode::InvertedBounds,
            Self::ParseFailed(_) => TimeWindowCode::ParseFailed,
        }
    }
}

// Per Phase 1 iter-1 C2 + Phase 2 iter-1 C1: integrate TimeWindowError into CoreError
// chain so handlers can use `?` operator with existing `From<CoreError> for ApiError` impl.
//
// **CoreError uses STRUCT-VARIANT pattern** matching ADR-019 §4.6 majority style
// (`Storage { code, message }`, `Network { code, message }`, etc.) — NOT `#[from]` tuple.
//
// Add to `crates/oneshim-core/src/error.rs`:
//
// ```rust
// // In CoreError enum (alphabetical position between Storage and Validation):
// #[error("Time window error [{code}]: {message}")]
// TimeWindow {
//     code: crate::error_codes::TimeWindowCode,
//     message: String,
// },
//
// // In CoreError::code() method:
// Self::TimeWindow { code, .. } => code.as_str(),
//
// // Manual From impl that maps each TimeWindowError variant to its wire code:
// impl From<crate::types::TimeWindowError> for CoreError {
//     fn from(err: crate::types::TimeWindowError) -> Self {
//         Self::TimeWindow {
//             code: err.code(),
//             message: err.to_string(),
//         }
//     }
// }
// ```
//
// Then in `crates/oneshim-web/src/error.rs` `From<CoreError> for ApiError`, add the
// explicit BadRequest arm BEFORE the wildcard `_ => ApiError::Internal`:
// ```rust
// CoreError::TimeWindow { message, .. } => ApiError::BadRequest(message),
// ```

impl TimeWindow {
    /// Construct a TimeWindow with bound validation.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, TimeWindowError> {
        if start > end {
            return Err(TimeWindowError::InvertedBounds { start, end });
        }
        Ok(Self { start, end })
    }

    /// Returns true if `instant` is within `[start, end]` (both inclusive).
    pub fn contains(&self, instant: DateTime<Utc>) -> bool {
        instant >= self.start && instant <= self.end
    }

    /// Returns the duration between start and end (always non-negative).
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }

    /// Returns RFC3339 (start, end) pair for SQL parameter binding.
    /// Compatible with existing `WHERE timestamp >= ?1 AND timestamp <= ?2` patterns.
    pub fn to_sql_pair(&self) -> (String, String) {
        (self.start.to_rfc3339(), self.end.to_rfc3339())
    }

    /// Construct a TimeWindow from RFC3339 string pair.
    /// Used when migrating from `(from: &str, to: &str)` storage helpers.
    pub fn from_rfc3339_pair(from: &str, to: &str) -> Result<Self, TimeWindowError> {
        let start = DateTime::parse_from_rfc3339(from)
            .map_err(|e| TimeWindowError::ParseFailed(e.to_string()))?
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339(to)
            .map_err(|e| TimeWindowError::ParseFailed(e.to_string()))?
            .with_timezone(&Utc);
        Self::new(start, end)
    }
}
```

### 5.2 `TimeRangeQuery::to_time_window` Adapter

**File**: `crates/oneshim-api-contracts/src/common.rs` (modify existing)

```rust
use chrono::{Duration, Utc};
use oneshim_core::types::TimeWindow;

impl TimeRangeQuery {
    /// Convert REST query optional bounds into a bounded TimeWindow.
    ///
    /// - If `to` is None: defaults to `now()`
    /// - If `from` is None: defaults to `to - default_lookback`
    /// - `default_lookback` is the fallback window size when bounds are missing.
    ///   **Per Phase 2 iter-10 NEW-C1: callers use `Duration::hours(24)` everywhere**
    ///   to preserve existing `from_datetime()` 24h fallback. NOT 7d/30d.
    ///   See NG10 for rationale (avoid 7×/30× payload widening).
    ///
    /// Per spec U5: this is the boundary where Optional bounds become
    /// Required bounds. Internal code (storage, models) work with TimeWindow.
    ///
    /// Per Phase 1 iter-1 C4: takes `&self` (not `self`) so the 7 service sites
    /// that pass `&TimeRangeQuery` and continue to use `limit`/`offset`/`min_importance`
    /// fields don't need to clone or restructure.
    pub fn to_time_window(&self, default_lookback: Duration) -> Result<TimeWindow, TimeWindowError> {
        let now = Utc::now();
        let end = match self.to.as_deref() {
            Some(s) => DateTime::parse_from_rfc3339(s)
                .map_err(|e| TimeWindowError::ParseFailed(e.to_string()))?
                .with_timezone(&Utc),
            None => now,
        };
        let start = match self.from.as_deref() {
            Some(s) => DateTime::parse_from_rfc3339(s)
                .map_err(|e| TimeWindowError::ParseFailed(e.to_string()))?
                .with_timezone(&Utc),
            None => end - default_lookback,
        };
        TimeWindow::new(start, end)
    }
}
```

**Existing TimeRangeQuery struct preserved** — only added new method. REST API contract unchanged.

### 5.3 SQL Storage Helper Migration Pattern

**Two sub-patterns** based on method complexity:

**(a) SAFE-SYNTHETIC** for simple methods (events.rs, frames.rs body fits in <30 lines, single `query_row` / `prepare` invocation, `lock().unwrap()` shape):

**Before** (`crates/oneshim-storage/src/sqlite/frames.rs`):
```rust
pub fn count_frames_in_range(&self, from: &str, to: &str) -> Result<u64, StorageError> {
    let conn = self.conn.lock().unwrap();
    let count: u64 = conn.query_row(
        "SELECT COUNT(*) FROM frames WHERE timestamp >= ?1 AND timestamp <= ?2",
        rusqlite::params![from, to],
        |row| row.get(0),
    )?;
    Ok(count)
}
```

**After**:
```rust
use oneshim_core::types::TimeWindow;

pub fn count_frames_in_range(&self, window: &TimeWindow) -> Result<u64, StorageError> {
    let conn = self.conn.lock().unwrap();
    let (from, to) = window.to_sql_pair();
    let count: u64 = conn.query_row(
        "SELECT COUNT(*) FROM frames WHERE timestamp >= ?1 AND timestamp <= ?2",
        rusqlite::params![&from, &to],
        |row| row.get(0),
    )?;
    Ok(count)
}
```

(Per Phase 1 iter-1 N4: standardize on `rusqlite::params!` macro across all migrated helpers.)

**(b) PRESERVE-BODY** for complex methods (calibration_store_impl, maintenance.rs `delete_data_in_range`, work_sessions.rs `get_daily_active_secs`):
- Async `with_conn(move |conn| {...}).await` patterns
- Fallible `lock().map_err(|e| CoreError::Storage { code, message })?`
- `table_exists` migration guards
- Per-row error wrapping with custom `StorageError::Internal(format!(...))`
- Half-open `< ?2` boundaries (NG6 — work_sessions.rs)
- Containment `start_time >= ?1 AND end_time <= ?2` (different columns — calibration list_segment_time_ranges)
- Multiple SQL statements per call (`delete_metrics` triggers DELETE on both system_metrics + system_metrics_hourly)

For these, **DO NOT rewrite** the function body. Plan v6/v7 prescribes minimal-diff:
```rust
- pub fn complex_helper(&self, from: &str, to: &str, ...other_params) -> Result<...>
+ pub fn complex_helper(&self, window: &TimeWindow, ...other_params) -> Result<...>
+     let (from, to) = window.to_sql_pair();
      // ... ENTIRE existing body unchanged: existing SQL, lock-error mapping, parsing, async with_conn, etc.
  }
```

The shadowed `from`/`to` String locals match the previous parameter names exactly — every existing `params![from, to]` invocation continues to work unchanged. SQL strings, table names, column names, error messages stay bit-identical.

### 5.4 Domain Model Migration Pattern

**Before** (`crates/oneshim-core/src/models/work_session.rs`):
```rust
pub struct FocusMetrics {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub deep_work_secs: u64,
    // ...
}
```

**After**:
```rust
use crate::types::TimeWindow;

pub struct FocusMetrics {
    pub period: TimeWindow,
    pub deep_work_secs: u64,
    // ...
}
```

**JSON serialization compatibility** — Critical:
The `TimeWindow` serde struct produces `{"start": "...", "end": "..."}`. If `FocusMetrics` is serialized as part of API response, the JSON shape changes from `period_start/period_end` to nested `period: {start, end}`. This affects frontend consumers!

**RESOLVED via NG8** (Phase 1 iter-1 I1): **Option Z** chosen — accept JSON shape change on internal `FocusMetrics` model.

Rationale: `FocusMetrics` is internal domain model only. The REST contract serializes `FocusMetricsDto` (in `oneshim-api-contracts/src/focus.rs`) which has DIFFERENT fields (`date: String` + scalars, NO `period_start/period_end`). Verified frontend has zero references to `period_start`/`period_end`. Internal JSON shape change has no external impact. **No custom serde needed**. Saves ~3h of unnecessary work.

**Pattern A vs B distinction** (Phase 2 iter-12 NEW Critical fix): FocusMetrics has 10+ call sites with TWO migration patterns:

- **Pattern A (constructor-default)**: caller cares only about `period`; other fields can default to zeros. Use `FocusMetrics::new(start, end).expect("...") -> Result<Self, TimeWindowError>` constructor.
- **Pattern B (struct-literal-with-custom-fields)**: caller seeds custom values for non-period fields (e.g., `total_active_secs: 3600, deep_work_secs: 2400`). MUST use renamed struct literal:
  ```rust
  FocusMetrics {
      period: TimeWindow::new(start, end).unwrap(),
      total_active_secs: *active,
      deep_work_secs: *deep,
      // ... other custom values
  }
  ```
- **DO NOT use constructor for Pattern B sites** — it would zero out the custom values silently.

Of the 10 call sites: 7 are Pattern B (production SQL row mapping at focus_metrics.rs:55+217 + 4 test fixtures at focus_analyzer/mod.rs:384/420/442 + 1 at grpc_dashboard_integration.rs:461 with 10+ custom seeded values), 3 are Pattern A (work_session.rs:317 internal duration calc + work_session.rs:446 test + tests.rs:76 test). Plan v13 enumerates all 10 with explicit Pattern classification.

### 5.5 REST Handler + Service-Layer Migration Pattern

**ARCHITECTURE NOTE** (Phase 2 iter-9 NEW-C1 + iter-10 NEW-C1 corrections):
- ONESHIM REST handlers are THIN pass-through to service layer. They do NOT call storage directly.
- Migration happens at the **service layer**, not handler layer. Handler files require ZERO changes.
- Services internally use `params.from_datetime()` / `params.to_datetime()` helpers — these silently swallow parse errors and use a hardcoded **24-hour** fallback (NOT 7d/30d).
- Migration replaces helper calls with `params.to_time_window(Duration::hours(24))?` — **preserve existing 24h default exactly** to avoid 7×/30× payload widening.
- BEHAVIOR CHANGE: invalid timestamps now return HTTP 400 (was: silently fall back to default-window data with HTTP 200).

**Handler — UNCHANGED** (`crates/oneshim-web/src/handlers/frames.rs`):
```rust
pub async fn get_frames(
    State(context): State<StorageWebContext>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<PaginatedResponse<FrameResponse>>, ApiError> {
    Ok(Json(FramesQueryService::new(context).get_frames(&params)?))
}
```

**Service Before** (`crates/oneshim-web/src/services/frames_service.rs`):
```rust
pub fn get_frames(&self, params: &TimeRangeQuery) -> Result<PaginatedResponse<FrameResponse>, ApiError> {
    let from = params.from_datetime();  // hardcoded 24h fallback if missing/invalid
    let to = params.to_datetime();
    let limit = params.limit_or_default();
    let offset = params.offset_or_default();
    // ... uses from, to as DateTime<Utc>
}
```

**Service After**:
```rust
use chrono::Duration;
use oneshim_core::types::TimeWindow;

pub fn get_frames(&self, params: &TimeRangeQuery) -> Result<PaginatedResponse<FrameResponse>, ApiError> {
    let window = params.to_time_window(Duration::hours(24))   // ← preserves existing 24h default
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let limit = params.limit_or_default();
    let offset = params.offset_or_default();
    // ... pass &window to storage methods (Task 4 already migrated 8 specific methods)
}
```

**Storage decomposition for non-migrated methods**: Task 4 only migrated 8 specific storage methods to `&TimeWindow` (count_events_in_range, count_frames_in_range, list_frame_file_paths_in_range, delete_data_in_range, get_daily_active_secs, flag_noise_range, get_entries, list_segment_time_ranges). For OTHER storage methods that still take `DateTime<Utc>` (get_frames, get_events, get_metrics, etc.), service decomposes:
```rust
let TimeWindow { start: from, end: to } = window;
self.ctx.storage.get_frames(from, to, limit).await?
```

### 5.6 GDPR `DeleteRangeRequest` Migration (Option C — accessor pattern)

`DeleteRangeRequest` is special: GDPR requirement says user must explicitly specify range. **No default lookback** — bounds are required.

**Phase 1 iter-1 I3 + Phase 2 iter-1 C9 RESOLVED via Option C** (accessor pattern): keep `from: String, to: String` fields untouched + add `period() -> Result<TimeWindow, TimeWindowError>` accessor. Preserves frontend `DataSection.tsx` JSON shape exactly — NO frontend code changes required. NO custom serde module needed (the `flatten + with` combo is invalid serde syntax anyway).

```rust
// Before AND After (struct unchanged):
#[derive(Debug, Deserialize)]
pub struct DeleteRangeRequest {
    pub from: String,           // YYYY-MM-DDTHH:MM:SSZ RFC3339
    pub to: String,             // YYYY-MM-DDTHH:MM:SSZ RFC3339
    #[serde(default)]
    pub data_types: Vec<String>,
}

impl DeleteRangeRequest {
    /// Construct a TimeWindow from the request's from/to string fields.
    /// Frontend sends from/to keys — NO change to JSON shape required.
    pub fn period(&self) -> Result<TimeWindow, TimeWindowError> {
        TimeWindow::from_rfc3339_pair(&self.from, &self.to)
    }
}
```

Service layer (NOT handler — handler thin-delegates to `DataCommandService::delete_data_range(&request)`) constructs the TimeWindow once via `request.period()?` and passes `&window` to storage methods. Returns 400 Bad Request if from/to malformed. (Spec §6.3 details the data flow.)

---

## 6. Data Flow

### 6.1 REST query → Service → Storage path (Phase 2 iter-9 architectural correction)

```
HTTP GET /api/frames?from=2026-04-20T00:00:00Z&to=2026-04-25T00:00:00Z
  → Axum extracts Query<TimeRangeQuery> { from: Some("..."), to: Some("...") }
  → Handler thin-delegates: FramesQueryService::new(context).get_frames(&params)?
  → Service: params.to_time_window(Duration::hours(24))?
    → parses RFC3339 strings → DateTime<Utc>
    → constructs TimeWindow::new(start, end)? — validates start <= end
  → Service calls storage.count_frames_in_range(&window) (Task-4-migrated method) directly,
    OR for non-migrated methods: storage.get_frames(window.start, window.end, limit) with decomposition
    → migrated storage method uses window.to_sql_pair() → ("2026-04-20T00:00:00+00:00", "...")
    → SQL: WHERE timestamp >= ?1 AND timestamp <= ?2 (closed-closed preserved per NG6)
  → Returns Vec<FrameDto> → JSON response (frame fields unchanged from before)
```

### 6.2 Default lookback application (24h preserved per Phase 2 iter-10 NEW-C1)

```
HTTP GET /api/events  (no query params)
  → TimeRangeQuery { from: None, to: None }
  → Handler thin-delegates: EventsQueryService::new(context).get_events(&params)?
  → Service: params.to_time_window(Duration::hours(24))?  // ← preserves existing 24h default
    → end = now()
    → start = end - 24 hours
    → TimeWindow::new(start, end) — always valid
  → Service → storage query
```

**Behavior preservation**: 24h default matches existing `from_datetime()` fallback exactly. Plan v9 originally prescribed 7d/30d (would 7×/30× widen payloads); v10 reverted to 24h. Any deliberate widening should be a separate PR with frontend coordination.

### 6.3 GDPR delete (no default applied + accessor pattern)

```
HTTP POST /api/data/delete-range  body: { "from": "2026-04-20T00:00:00Z", "to": "2026-04-25T00:00:00Z", "data_types": ["frames"] }
  → DeleteRangeRequest { from: String, to: String, data_types: Vec<String> }   ← Option C (Phase 2 iter-1 C9): unchanged JSON shape
  → Handler thin-delegates: DataCommandService::new(context).delete_data_range(&request)?
  → Service: let window = request.period().map_err(|e| ApiError::BadRequest(e.to_string()))?
    → period() calls TimeWindow::from_rfc3339_pair(&self.from, &self.to)
    → If from/to malformed: returns Err(TimeWindowError::ParseFailed) → ApiError::BadRequest → 400 Bad Request
  → Service calls storage.delete_data_in_range(&window, ...flags) once for hoisted window
```

---

## 7. Error Handling

### 7.1 Failure modes

| Scenario | Detection | Behavior |
|----------|-----------|----------|
| start > end (manually constructed) | `TimeWindow::new` returns Err | Caller propagates as 400 Bad Request via existing IpcError/ApiError chain |
| Invalid RFC3339 string in REST query | `to_time_window` Err propagates | Handler returns 400 with parse error message |
| GDPR delete missing required bound | serde Deserialize Err | 400 Bad Request before handler runs |
| Storage layer receives valid TimeWindow | Cannot fail at type level (always bounded + validated) | Pre-validation eliminated null-bound bugs |
| Default lookback applied but caller intended explicit None | Caller's responsibility | Documented per-handler default in code |

### 7.2 Wire codes (per ADR-019)

NEW wire code variants (added to existing or new enum):

```rust
// NEW: TimeWindowCode in crates/oneshim-core/src/error_codes/time_window.rs
define_code_enum! {
    pub enum TimeWindowCode {
        InvertedBounds => "time_window.inverted_bounds",
        ParseFailed => "time_window.parse_failed",
    }
}
```

Total wire codes after PR (per Phase 1 iter-1 C1 + PF3 baseline 2026-04-25): current **42** (worktree base `2ba38cf5`, pre-PR-B1, PF3-verified) + 2 = **44** (if both PR-B1 and PR-B2 are still pending). If PR-B1 (#508, expected +5 codes for tracking_schedule.*) merges before TimeWindow PR, baseline becomes 47 + 2 = **49**. If PR-B2 also merges (estimated +4 codes for autostart.*), baseline becomes 51 + 2 = **53**. **DO NOT trust pre-merge estimates** — recompute actual count at impl time via `wc -l crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`. Plan PF3 (`docs/superpowers/plans/...`) captures this baseline procedure.

**Per Phase 1 iter-1 C3**: `error_codes/mod.rs` requires 3 wire-up steps for `TimeWindowCode`:
1. `pub mod time_window;`
2. `pub use time_window::TimeWindowCode;`
3. Add `for c in TimeWindowCode::all() { codes.push(c.as_str()); }` to `all_codes()` aggregator

---

## 8. Testing Strategy

### 8.1 Unit tests (in `time_window.rs`)

- `new_accepts_valid_bounds` — start < end, start == end (edge)
- `new_rejects_inverted_bounds` — start > end → Err
- `contains_includes_both_bounds` — closed-closed semantic verification
- `contains_excludes_outside` — instant < start AND instant > end → false
- `duration_returns_difference` — verify Duration math
- `to_sql_pair_round_trips_via_from_rfc3339_pair` — `from_rfc3339_pair(window.to_sql_pair())` == window
- `from_rfc3339_pair_rejects_invalid_strings` — non-RFC3339 → Err
- `from_rfc3339_pair_handles_timezone_offset` — `+09:00` parsed and converted to Utc
- `serde_roundtrip_json` — `serde_json::to_string` then `from_str` → equal
- `same_start_end_is_valid_zero_duration_window` — start == end allowed (single instant query)

### 8.2 Adapter tests (in `oneshim-api-contracts`)

- `to_time_window_with_both_bounds_provided` — uses both as-is
- `to_time_window_default_to_when_to_missing` — `to = now()`
- `to_time_window_default_lookback_when_from_missing` — `start = end - lookback`
- `to_time_window_default_both_when_neither_provided` — default lookback applied
- `to_time_window_rejects_invalid_iso8601` — Err propagated

### 8.3 Migration regression tests

For each migrated SQL helper:
- Verify behavior identical to pre-migration (same row counts, same returned data)
- Use existing test fixtures where available

For each REST handler:
- Verify response JSON shape unchanged (`FocusMetricsDto` REST contract — internal `FocusMetrics` shape change is OK per NG8)
- Verify default lookback values match prior code

### 8.4 Pass criteria

- All unit tests GREEN (~37 NEW tests per plan v13: 13 TimeWindow unit + 3 TimeWindowCode + 8 TimeRangeQuery adapter + 3 SQL boundary regression + 4 E2E + 2 ApiError mapping + 4 api-contracts roundtrip)
- All existing integration tests still pass (no regression)
- `cargo check/test/clippy/fmt --workspace` GREEN (clippy run ONCE at PC1 per Phase 2 iter-1 I6 — not per-task)
- Wire snapshot test GREEN — count is **BASELINE_AT_IMPL_TIME + 2** (not hardcoded; recompute via `wc -l crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` per Plan PF3)
- i18n CI GREEN (`bash scripts/check-wire-error-i18n-coverage.sh`) — same count both locales

---

## 9. Delivery Plan

### 9.1 PR commit structure (~30h, ~4 working days, 11 tasks)

**Source of truth**: `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (v13, 2885 lines). Spec table below summarizes; Plan supersedes for any discrepancy.

| # | Task / Commit (plan v13) | Estimate |
|---|--------|----------|
| 1 | `feat(core): TimeWindow primitive + TimeWindowError + TimeWindowCode + CoreError struct-variant + ApiError BadRequest arm + lib.rs/error_codes/mod.rs registration + wire snapshot + 18 tests` (was 2 separate tasks in spec v3 — merged per Phase 2 iter-1 I9 to avoid circular compile dep) | 4.5h |
| 2 | `test(i18n): wire-error translations for TimeWindow codes (en+ko)` — updates BOTH `toHaveLength()` assertions at lines 30+122 (Phase 2 iter-1 C8) | 0.5h |
| 3 | `feat(api-contracts): TimeRangeQuery::to_time_window adapter + Default derive (per Phase 2 iter-1 C4) + 8 adapter tests` | 1.5h |
| 4 | `refactor(storage): migrate 8 SQL range helpers + 14+ caller sites to &TimeWindow` — 3 calibration_store + 5 web_storage port trait sigs + 5 SQLite impls + 5 web_storage_impl wrappers + 5 service callers + 5 FailingStorage mocks + 3 regime.rs callers + ~14 internal SQLite test sites + NoopCalibrationReader/Writer mocks. PRESERVE-BODY for complex methods (Phase 2 iter-6 NEW-C1). | 5h |
| 5 | `test(storage): boundary regression tests for migrated SQL helpers (closed-closed + delete flag preservation)` — 3 boundary tests with actual `DeletedRangeCounts` field names (`events_deleted` etc.) | 1.5h |
| 6 | `refactor(web-services): migrate 7 service-layer files to to_time_window adapter` — 7 services: frames/events/metrics/focus/idle/processes/timeline. **Service layer NOT handler layer** (Phase 2 iter-9 NEW-C1). Default lookback `Duration::hours(24)` (Phase 2 iter-10 NEW-C1). Decompose `&window` for non-migrated storage methods. | 5h |
| 7 | `refactor(api-contracts): DeleteRangeRequest period() accessor + ReportQuery date-only preserved` — Option C accessor (Phase 2 iter-1 C9 + iter-11 NEW Critical: ReportQuery is date-only `%Y-%m-%d`, NOT RFC3339; resolve_report_window in reports_query_support.rs updated to return `Result<(TimeWindow, String), ApiError>`) | 1.5h |
| 8 | `refactor(core): FocusMetrics + SessionMetrics use TimeWindow primitive (NG8 internal-only)` — 10 sites with explicit Pattern A vs Pattern B classification (Phase 2 iter-12 NEW Critical — struct-literal preservation for sites with custom seeded values like grpc_dashboard_integration.rs:461) | 2h |
| 9 | `refactor(workspace): sweep remaining absolute-timestamp range pairs to TimeWindow` (if any remain after Tasks 1-8) | 1h |
| 10 | `test(integration): TimeWindow E2E — closed-closed boundary + 400 error mapping (no code body field per ApiError schema)` — 4 E2E tests (Phase 2 iter-1 C3 — no body["code"] assertion) | 2h |
| 11 | `docs(time-window): STATUS.md + PHASE-HISTORY entry for TimeWindow refactor` — PHASE-HISTORY documents 1 behavior change + 3 behaviors preserved + helpers retained (Phase 2 iter-10 NEW-I2) | 1h |

**Total**: ~30h (~4 working days). Up from spec v3's ~21h estimate due to scope expansions in Phase 2 iter-1 (C6/C7 port scope), iter-9 (NEW-C1 service-layer), iter-10 (NEW-C2 decomposition).

**Notes**:
- Per Phase 1 iter-1 NG7: `IdlePeriod` NOT migrated. `activity.rs` removed from touched files.
- Per Phase 1 iter-1 NG8: `FocusMetricsDto` not affected (Option Z); only internal `FocusMetrics` changes.
- Per Phase 2 iter-1 C9: `DeleteRangeRequest` external JSON preserved via Option C accessor pattern (NOT custom serde — `flatten + with` combo is invalid serde syntax).
- Per Phase 2 iter-11: ReportQuery date-only `%Y-%m-%d` preserved (no flatten of TimeRangeQuery — would break Custom period parse).
- Per Phase 2 iter-12: FocusMetrics has 2 migration patterns (A constructor / B struct-literal) — Pattern B sites MUST preserve struct literal to avoid silent custom-field zero-out.

### 9.2 Branch naming

Branch: `refactor/timewindow-primitive` (already created in this worktree)

### 9.3 Release plan

After merge → `0.4.42-rc.1` (or batch with PR-B2 into single RC).

---

## 10. Migration & Backward Compatibility

### 10.1 External API contracts

- **REST API query strings** (`?from=X&to=Y`) — UNCHANGED
- **REST API response JSON** for `FocusMetrics` — internal model JSON shape changes (`period_start/period_end → period: {start, end}`); REST DTO (`FocusMetricsDto` in api-contracts) is NOT affected. Frontend unaffected per NG8.
- **REST API request JSON** for `DeleteRangeRequest` — preserved via Option C accessor pattern (Phase 2 iter-1 C9): keeps `from: String, to: String` fields untouched + adds `period() -> Result<TimeWindow, TimeWindowError>` accessor. NO custom serde module (the `flatten + with` combo is invalid serde syntax anyway). Frontend `DataSection.tsx` unchanged. External API contract preserved.
- **Tauri IPC** — none affected (no time-range IPC commands identified)

### 10.2 Internal API (Rust) — breaking changes

- 8 specific SQL helper signatures change: `(from: &str, to: &str)` → `(window: &TimeWindow)` (per plan v13 Task 4 enumeration: count_events_in_range, count_frames_in_range, list_frame_file_paths_in_range, delete_data_in_range, get_daily_active_secs, flag_noise_range, get_entries, list_segment_time_ranges)
- 7 service-layer files change internal logic (frames/events/metrics/focus/idle/processes/timeline_service) — REST **handlers UNCHANGED** (Phase 2 iter-9 NEW-C1: handlers thin-delegate to services)
- Other storage methods (get_frames, get_events, get_metrics, etc.) stay on `DateTime<Utc>` signatures — services decompose `&window` to `(window.start, window.end)` for those
- Domain models field reorganization: `period_start, period_end` → single `period: TimeWindow` (FocusMetrics, SessionMetrics)

These are internal — no external consumers (this is a desktop client, not a library).

### 10.3 Downgrade safety

- TimeWindow type is internal — downgrade restores old field-pair model
- **Zero external JSON shape changes** to roll back: REST query strings (?from=&to=) unchanged; DeleteRangeRequest preserved via Option C accessor; FocusMetricsDto (REST contract) was never affected per NG8
- Database schema unchanged (RFC3339 strings → SQL via `to_sql_pair()`)
- BEHAVIOR CHANGE: invalid timestamp inputs now return HTTP 400 (was: silently default to 24h-window data with HTTP 200) — frontend should already handle 400 errors gracefully; if not, downgrade restores silent-default behavior

---

## 11. Open Questions for Phase 1 Deep Review

| # | Question | Resolution path |
|---|----------|-----------------|
| Q-1 | ✅ RESOLVED (Phase 1 iter-1 I1): `FocusMetrics` is internal model only — REST serializes `FocusMetricsDto` (different fields). Frontend has zero references to `period_start/period_end`. Use **Option Z** (break internal JSON shape). Saves ~3h custom serde work. |
| Q-2 | ✅ RESOLVED (Phase 1 iter-1 I4): `IdlePeriod` NOT migrated. `end_time: Option<DateTime<Utc>>` represents ongoing idle. Migration would require either two types or `end = now()` workaround (drift bug). Add NG7. |
| Q-3 | ✅ RESOLVED v2 (Phase 2 iter-11 NEW Critical correction): keep `ReportQuery` schema **unchanged** — `from: Option<String>, to: Option<String>` are date-only `%Y-%m-%d` strings (NOT RFC3339), parsed via `NaiveDate::parse_from_str(s, "%Y-%m-%d")`. Plan v9/v10 originally prescribed `#[serde(flatten)] time_range: TimeRangeQuery + to_time_window` — but `TimeRangeQuery::to_time_window` parses RFC3339 via `DateTime::parse_from_rfc3339`, which would FAIL on date-only inputs and BREAK the reports endpoint. Instead, update `resolve_report_window` in `reports_query_support.rs` to construct TimeWindow from existing NaiveDate parse logic + return `Result<(TimeWindow, String), ApiError>`. **Original Phase 1 iter-1 I2 resolution was wrong** — caught by iter-11 audit. |
| Q-4 | ✅ RESOLVED (Phase 1 iter-2): TimeWindow is always constructed at the **handler boundary** (REST handler calls `q.to_time_window(default)?` once). Storage layer ONLY accepts `&TimeWindow` (never `&str` pair or `(DateTime, DateTime)` pair). Domain models (FocusMetrics, SessionMetrics) embed `period: TimeWindow` field. Single canonical construction site enforces validation discipline. |
| Q-5 | ✅ RESOLVED: yes, migrate `flag_noise_range`. Per Phase 1 iter-1 N3, also update port trait at `oneshim-core/src/ports/calibration_store.rs`. |
| Q-6 | ✅ RESOLVED (Phase 1 iter-2): `start == end` (zero-duration window) is valid per §5.1 — represents single-instant query. Handlers pass through to SQL `WHERE timestamp >= start AND timestamp <= end` which correctly returns events at exactly that instant. No special case needed in any handler. |
| Q-7 | ✅ RESOLVED (Phase 1 iter-2): keep `pub start, pub end` for convenient pattern matching (Rust idiom for value types like `chrono::DateTime`). Document in module rustdoc: "`TimeWindow::new` is the validation-safe constructor; direct struct literal construction bypasses bound validation — use only when both bounds are known to satisfy `start <= end`." |
| Q-8 | ✅ RESOLVED (PF3 captured 2026-04-25): wire-code baseline = **42** (worktree base, pre-PR-B1). Alphabetical block: `storage.failed → ui.element_missing → validation.*`. After insertion: `storage.failed → time_window.inverted_bounds → time_window.parse_failed → ui.element_missing`. If PR-B1 merges first (+5 tracking_schedule.* codes): `storage.failed → time_window.* (2 codes) → tracking_schedule.* (5 codes) → ui.*` (since `ti` < `tr` lexicographically). Plan PF3 procedure recomputes at impl time. |
| Q-9 | ✅ RESOLVED: gRPC `MetricBucket` excluded (NG2). Verified. |
| Q-10 (NEW iter-1) | ✅ RESOLVED (Phase 2 iter-1 C9): **Option C accessor pattern** — keep `from: String, to: String` fields untouched + add `period() -> Result<TimeWindow, TimeWindowError>` accessor. Preserves frontend `DataSection.tsx` JSON shape exactly. NO custom serde module (the `flatten + with` combo proposed in option (b) is invalid serde syntax). DataSection.tsx requires ZERO changes. |

---

## 12. Risk Register

| Risk | Likelihood | Impact | Mitigation / Resolution |
|------|-----------|--------|--------------------------|
| `FocusMetrics` JSON shape break crashes frontend Dashboard | ✅ RESOLVED | n/a | Per NG8 + Q-1: `FocusMetrics` not serialized to REST. `FocusMetricsDto` (different shape) is the REST contract. Frontend zero references to `period_start/period_end`. Option Z safe. |
| `DeleteRangeRequest` JSON shape change breaks frontend GDPR UI | ✅ RESOLVED (Phase 2 iter-1 C9) | n/a | Option C accessor pattern: keeps `from: String, to: String` fields untouched. Frontend `DataSection.tsx` requires ZERO changes. NO custom serde needed. |
| `IdlePeriod` `Option<end_time>` for ongoing idle — TimeWindow can't represent | ✅ RESOLVED (NG7) | n/a | IdlePeriod NOT migrated. Open-ended ongoing idle period stays on `Option<DateTime<Utc>> end_time`. |
| Big-bang PR cognitive load for reviewer | Medium | Low | Plan v13 commit structure splits by domain (foundation / i18n / adapter / storage / regression / services / data+reports / models / sweep / E2E / docs). Reviewer can commit-by-commit. 13 plan iterations of deep review caught all impl-blocking issues. |
| Rebase pain if PR-B1 (#508) lands during impl | ✅ MITIGATED | n/a | Plan ABORT GUARD at PF1: implementation cannot start until #508 is MERGED. PF2 does rebase first. Drift audit (post-iter-13) confirms 2 commits behind origin/main both touch only out-of-scope files. |
| Wire code count drift if PR-B1/B2 ship between spec and impl | ✅ RESOLVED via PF3 | n/a | Plan PF3 captures actual baseline at impl time via `wc -l crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`. Spec §7.2 + Q-8 document dynamic recompute procedure. |
| Unexpected SQL helper not in §1.2 catalog | ✅ RESOLVED via plan v13 enumeration | n/a | Plan v13 Step 4D.0 enumerates 30 caller sites + cross-layer audit confirms zero unmigrated callers in src-tauri/commands, oneshim-web/grpc, oneshim-network. |
| Frontend TypeScript drift on FocusMetrics serde | ✅ RESOLVED (NG8) | n/a | FocusMetricsDto (REST contract) is never affected. Internal FocusMetrics → API mapper unchanged. |
| Service-layer migration scope undercount (caught Phase 2 iter-9) | ✅ RESOLVED | n/a | Plan v9 added 7 service files + decomposition pattern for non-migrated storage methods. Plan v10 preserved 24h default lookback to avoid 7×/30× payload widening. |
| ReportQuery date-only vs RFC3339 mismatch (caught Phase 2 iter-11) | ✅ RESOLVED | n/a | Plan v11 keeps ReportQuery as-is (date-only %Y-%m-%d) + updates `resolve_report_window` in reports_query_support.rs to construct TimeWindow from existing NaiveDate parse logic. |
| FocusMetrics struct-literal silent zero-out (caught Phase 2 iter-12) | ✅ RESOLVED | n/a | Plan v12 distinguishes Pattern A (constructor) vs Pattern B (struct literal) per call site. 7 of 10 sites use Pattern B with custom seeded values. Plan v13 enumerates all definitively. |

---

## 13. Cross-Consumer Dependencies

| Branch | Status | Files | Conflict |
|--------|--------|-------|----------|
| `feature/phase9-autostart-foundation` (PR #508) | **OPEN — clippy FAILURE + BEHIND main** (verified 2026-04-25 17:00) | `oneshim-core/config/sections/`, scheduler, frontend autostart | **Hard dep** for impl gate (rebase risk). Phase 3 cannot start until #508 reaches MERGED state per plan ABORT GUARD at PF1. |
| `feature/phase9-autostart-linux-deep` (PR-B2) | Local plan ready, BLOCKED on PR-B1 merge | Same as PR-B1 | After PR-B1 merge |
| `refactor/serve-external-inner-extraction` (PR #506) | ✅ **MERGED** (commit `89ab7910` on origin/main) | `oneshim-web/src/grpc/external/` | Disjoint — already in worktree base |
| `ci/clippy-195-field-reassign-detection` (PR #509) | ✅ **MERGED** (commit `54c894d5` on origin/main) | `lefthook.yml` + scripts | Disjoint — already in worktree base |

### 13.1 Recommended merge order

1. PR #506 (serve_external_inner) — disjoint, can merge anytime
2. PR #509 (clippy 1.95) — disjoint
3. PR #508 (PR-B1 autostart) — required before Phase 3 start of TimeWindow
4. PR-B2 (autostart Linux deep) — after PR-B1
5. **TimeWindow refactor PR** (this spec) — after PR-B2 (or parallel if PR-B2 not yet ready)

---

## 14. Spec Self-Review (v9)

### 14.1 Placeholder scan
- ✅ All Q-1 through Q-10 RESOLVED (Q-1, Q-2, Q-4, Q-5, Q-6, Q-7, Q-9 in Phase 1; Q-3 corrected in Phase 2 iter-11; Q-8 captured at PF3; Q-10 resolved Phase 2 iter-1 C9)
- ✅ §7.2 wire code count baseline → dynamic via PF3 procedure
- ✅ No "TBD" in spec body

### 14.2 Internal consistency (v9 audit)
- ✅ U1-U5 decisions consistently applied across §3, §4, §5
- ✅ Closed-closed semantic preserved in `to_sql_pair` (§5.1) and SQL pattern (§5.3a)
- ✅ Half-open boundary preserved in work_sessions (§5.3 PRESERVE-BODY) per NG6
- ✅ Containment semantic preserved in calibration list_segment_time_ranges per Phase 2 iter-8
- ✅ Service-layer architecture across §5.5 + §6.1 + §9.1
- ✅ Option C accessor for DeleteRangeRequest across §5.6 + §6.3 + §10.1 + Q-10
- ✅ ReportQuery date-only across §9.1 Task 7 + Q-3
- ✅ FocusMetrics Pattern A/B distinction across §5.4 + §9.1 Task 8

### 14.3 Scope check
- ✅ Single PR scope (Big-bang per U2)
- ✅ Q-2 IdlePeriod NOT migrated per NG7 (no scope expansion)

### 14.4 Ambiguity check
- ✅ §5.4 FocusMetrics — Pattern A/B explicitly distinguishes constructor vs struct-literal preservation
- ✅ §10.1 DeleteRangeRequest — Option C accessor pattern; frontend zero-change verified
- ✅ §5.5 service-layer migration explicit (handlers thin pass-through)

### 14.5 Phase 2 corrections summary

After 13 plan iterations + spec v3→v9 alignments, the following were caught beyond Phase 1's initial review:
- iter-1 (9C+11I): CoreError struct-variant pattern, ApiError chain, port scope expansion (8 methods + 14 callers)
- iter-2 (6 NEW C + 5 NEW I): Default derive, hand-computed timestamps, list_segment_time_ranges 3-tuple, sync flag_noise_range, Vec<(String, i64)> return
- iter-3 (2 NEW C + 1 NEW I): FailingStorage delegation pattern, ?-vs-.expect() in `()` returning fns
- iter-4-8 (cleanup): MockCalibration→Noop names, DeletedRangeCounts field names, half-open boundary preservation, containment semantic
- iter-9 (NEW C): Service-layer architectural correction
- iter-10 (NEW C): Default lookback preservation (24h NOT 7d/30d)
- iter-11 (NEW C): ReportQuery date-only NOT RFC3339
- iter-12 (NEW C): FocusMetrics struct-literal preservation (Pattern A vs B)
- iter-13: Pattern A/B definitive verification

Cumulative: 23 Critical + 28 Important + 2 Suggestion fixes integrated into spec v4-v9 + plan v2-v13.

---

## 15. Implementation Status (v9 — 2026-04-25)

- **Spec v9**: ALIGNED with plan v13 (this document — 9 spec versions iteratively corrected)
- **Phase 1 deep review**: ✅ CLOSED (3 iter, spec v1 → v3)
- **Phase 2 plan creation + deep review**: ✅ CLOSED (13 iter, plan v1 → v13; spec v3 → v9 alignment)
- **Phase 3 implementation**: 🔒 BLOCKED on PR-B1 (#508) merge — currently OPEN with clippy FAILURE + BEHIND main
- **Worktree**: `.claude/worktrees/timewindow-primitive` on `refactor/timewindow-primitive`
- **Base**: `2ba38cf5` (origin/main, pre-PR-B1)
- **Drift audit (post-iter-13)**: 0 unmigrated TimeWindow consumers in src-tauri/src/commands/, oneshim-web/src/grpc/, oneshim-network/src/ — bounded scope
- **PF5 dep verification**: 5/5 PASS (oneshim-core dep + CoreError struct-variant + From<CoreError> for ApiError + define_code_enum! macro + ErrorResponse no `code` field)

---

**End of spec v9.**
