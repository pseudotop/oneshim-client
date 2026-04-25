# TimeWindow Primitive Refactor Design Spec

**Date:** 2026-04-25
**Version:** v1 (initial — awaiting Phase 1 deep review)
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
| 3 | `TimeRangeQuery` | `crates/oneshim-api-contracts/src/common.rs:5-11` | RFC3339 ISO8601 strings, optional bounds | REST API params (8+ handlers) |
| 4 | `FocusMetrics period_*` | `crates/oneshim-core/src/models/work_session.rs:287-299` | `DateTime<Utc>` pair | Daily/weekly aggregates |
| 5 | `DeleteRangeRequest` | `crates/oneshim-api-contracts/src/data.rs:4-9` | ISO8601 strings | GDPR data purge |
| 6 | `IdlePeriod` | `crates/oneshim-core/src/models/activity.rs:20-24` | `DateTime<Utc>` + `Option<DateTime<Utc>>` | Idle session tracking |
| 7 | `SessionMetrics period_*` | `crates/oneshim-core/src/models/telemetry.rs:16-17` | `DateTime<Utc>` pair | Telemetry window |
| 8 | `ReportQuery` | `crates/oneshim-api-contracts/src/reports.rs:13-18` | ISO8601 strings + `period: ReportPeriod` enum | Weekly/monthly reports |
| 9 | SQL storage helpers | `crates/oneshim-storage/src/sqlite/{events,frames,calibration,web_storage_impl}.rs` (10+ methods) | mixed: `&str` pair (RFC3339) or `DateTime<Utc>` pair | Range queries |

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

---

## 3. User-Locked Decisions (U1-U5)

These decisions were made interactively during brainstorming and are FIXED.

| ID | Decision | Rationale |
|----|----------|-----------|
| **U1** | Scope = Option A (Absolute timestamps only) | Industry standard (iCalendar separates DTSTART/DTEND from RRULE; Prometheus/OTel time-series unify absolute, not recurrence). Wall-clock sites only 2 — YAGNI. |
| **U2** | Migration = Big-bang (single PR) | Deep review process (3-loop ralph-loop) absorbs large-PR risk. Avoids type-alias deprecation churn from gradual approach. |
| **U3** | Location = `oneshim-core` (`crates/oneshim-core/src/types/time_window.rs`) | Domain primitive home. SQL storage already depends on oneshim-core. Layering clean. |
| **U4** | Boundary = Closed-closed `[start, end]` | ONESHIM is event-driven business API (Stripe-style), not continuous time-series (Prometheus-style). User-facing date queries dominate. Existing SQL `BETWEEN` semantic preserved → migration risk zero. |
| **U5** | Optional bounds handling = `TimeRangeQuery::to_time_window(default_lookback)` adapter | Domain-specific defaults possible (frames 7d, reports 30d, GDPR no default). TimeWindow type stays simple (always bounded). REST contract unchanged. |

---

## 4. Architecture Overview

### 4.1 Component Layout

```
[NEW]
crates/oneshim-core/src/types/                  ← NEW directory (currently no `types/` dir)
  ├── mod.rs                                     ← `pub mod time_window;`
  └── time_window.rs                             ← TimeWindow struct + impl + tests
                ▲
                │ (consumed by)
                │
[MODIFIED — domain models]
crates/oneshim-core/src/models/
  ├── work_session.rs:287-299                   ← FocusMetrics: period_* → period: TimeWindow
  ├── activity.rs:20-24                          ← IdlePeriod: start_time + Option<end_time> → period: TimeWindow (with `is_completed` flag if needed)
  └── telemetry.rs:16-17                         ← SessionMetrics: period_* → period: TimeWindow

[MODIFIED — API contracts]
crates/oneshim-api-contracts/src/
  ├── common.rs:5-11                             ← TimeRangeQuery: + to_time_window(default_lookback) adapter
  ├── data.rs:4-9                                ← DeleteRangeRequest: from/to → period: TimeWindow
  └── reports.rs:13-18                           ← ReportQuery: from/to → period: Option<TimeWindow> (period field stays as ReportPeriod enum override)

[MODIFIED — REST handlers in oneshim-web]
crates/oneshim-web/src/handlers/
  ├── frames.rs                                  ← get_frames(window: &TimeWindow)
  ├── events.rs                                  ← count_events_in_range(window: &TimeWindow)
  ├── metrics.rs                                 ← daily aggregates by TimeWindow
  ├── focus.rs                                   ← focus session queries
  ├── sessions.rs                                ← session listings
  ├── interruptions.rs                           ← interruption queries
  ├── data.rs                                    ← GDPR delete using period: TimeWindow
  └── reports.rs                                 ← weekly/monthly aggregates

[MODIFIED — SQL storage]
crates/oneshim-storage/src/sqlite/
  ├── events.rs:14                               ← count_events_in_range(window: &TimeWindow)
  ├── frames.rs:10                               ← count_frames_in_range(window: &TimeWindow)
  ├── calibration_store_impl.rs:120-130          ← flag_noise_range(window: &TimeWindow)
  └── web_storage_impl.rs:245                    ← get_daily_active_secs(window: &TimeWindow)
                                                   plus 5-6 other range query helpers identified during impl
```

### 4.2 What is NOT touched

- `TrackingWindow` in `tracking_schedule.rs` — wall-clock recurrence (different domain)
- coaching `TimeRange` in `coaching.rs` — wall-clock recurrence
- All frontend TypeScript code — REST API JSON shape unchanged
- Tauri IPC commands — no IPC time-range parameters identified in current scope
- gRPC streaming `MetricBucket` (different concept — bucketed time series, deferred)

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// - `default_lookback` is domain-specific (frames=7d, reports=30d, etc.)
    ///
    /// Per spec U5: this is the boundary where Optional bounds become
    /// Required bounds. Internal code (storage, models) work with TimeWindow.
    pub fn to_time_window(self, default_lookback: Duration) -> Result<TimeWindow, TimeWindowError> {
        let now = Utc::now();
        let end = match self.to {
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map_err(|e| TimeWindowError::ParseFailed(e.to_string()))?
                .with_timezone(&Utc),
            None => now,
        };
        let start = match self.from {
            Some(s) => DateTime::parse_from_rfc3339(&s)
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

**Before** (`crates/oneshim-storage/src/sqlite/frames.rs`):
```rust
pub fn count_frames_in_range(&self, from: &str, to: &str) -> Result<u64, StorageError> {
    let conn = self.conn.lock().unwrap();
    let count: u64 = conn.query_row(
        "SELECT COUNT(*) FROM frames WHERE timestamp >= ?1 AND timestamp <= ?2",
        [from, to],
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
        [&from, &to],
        |row| row.get(0),
    )?;
    Ok(count)
}
```

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

**Mitigation options** (Phase 1 deep review must decide):
- Option X: Use `#[serde(flatten)]` on `period: TimeWindow` field → JSON keys become `start`/`end` (still different from `period_start`/`period_end`) — partial compat
- Option Y: Custom serde with `period_start`/`period_end` external names — preserves JSON shape exactly
- Option Z: Accept JSON shape change + update frontend types

**Tentative recommendation**: Option Y (preserve JSON shape via custom serde). Avoid frontend churn. Defer Z to a future API versioning effort.

### 5.5 REST Handler Migration Pattern

**Before** (`crates/oneshim-web/src/handlers/frames.rs`):
```rust
pub async fn get_frames(
    Query(q): Query<TimeRangeQuery>,
    State(ctx): State<...>,
) -> Result<Json<Vec<FrameDto>>, ApiError> {
    let q = q.with_defaults(7);  // 7 days
    let from = q.from.unwrap();   // safe after with_defaults
    let to = q.to.unwrap();
    let frames = ctx.storage.get_frames(from.parse()?, to.parse()?, 100)?;
    Ok(Json(frames))
}
```

**After**:
```rust
pub async fn get_frames(
    Query(q): Query<TimeRangeQuery>,
    State(ctx): State<...>,
) -> Result<Json<Vec<FrameDto>>, ApiError> {
    let window = q.to_time_window(Duration::days(7))?;
    let frames = ctx.storage.get_frames(&window, 100)?;
    Ok(Json(frames))
}
```

### 5.6 GDPR `DeleteRangeRequest` Migration

`DeleteRangeRequest` is special: GDPR requirement says user must explicitly specify range. **No default lookback** — bounds are required.

```rust
// Before
pub struct DeleteRangeRequest {
    pub from: String,
    pub to: String,
    pub data_types: Vec<String>,
}

// After
pub struct DeleteRangeRequest {
    pub period: TimeWindow,  // required, no Option
    pub data_types: Vec<String>,
}
```

REST handler validates strict parsing (no default applied for unbounded request) — returns 400 Bad Request if `from` or `to` missing.

---

## 6. Data Flow

### 6.1 REST query → Storage path

```
HTTP GET /api/frames?from=2026-04-20T00:00:00Z&to=2026-04-25T00:00:00Z
  → Axum extracts Query<TimeRangeQuery> { from: Some("..."), to: Some("...") }
  → handler: q.to_time_window(Duration::days(7))?
    → parses RFC3339 strings → DateTime<Utc>
    → constructs TimeWindow::new(start, end)? — validates start <= end
  → handler calls storage.get_frames(&window, 100)
    → storage uses window.to_sql_pair() → ("2026-04-20T00:00:00+00:00", "2026-04-25T00:00:00+00:00")
    → SQL: WHERE timestamp >= ?1 AND timestamp <= ?2 (closed-closed preserved)
  → Returns Vec<FrameDto> → JSON response (frame fields unchanged from before)
```

### 6.2 Default lookback application

```
HTTP GET /api/events  (no query params)
  → TimeRangeQuery { from: None, to: None }
  → handler: q.to_time_window(Duration::days(30))?  // events default = 30d
    → end = now()
    → start = end - 30 days
    → TimeWindow::new(start, end) — always valid
  → Storage query
```

### 6.3 GDPR delete (no default applied)

```
HTTP POST /api/data/delete-range  body: { "period": { "start": "...", "end": "..." }, "data_types": ["frames"] }
  → DeleteRangeRequest { period: TimeWindow, data_types }
  → handler: ctx.storage.delete_frames_in_range(&req.period)?
  → If body missing period.start or period.end: serde fails → 400 Bad Request (no silent default applied)
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

Total wire codes after PR: current 51 (post-PR-B2) + 2 = **53**.

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
- Verify response JSON shape unchanged (especially for `FocusMetrics` if Option Y serde used)
- Verify default lookback values match prior code

### 8.4 Pass criteria

- All unit tests GREEN
- All existing integration tests still pass (no regression)
- `cargo check/test/clippy/fmt --workspace` GREEN
- Wire snapshot test GREEN (53 codes)
- i18n CI GREEN (53 codes per locale)

---

## 9. Delivery Plan

### 9.1 PR commit structure (~3-4 day implementation, ~10-12 commits)

| # | Commit | Estimate |
|---|--------|----------|
| 1 | `feat(time): add TimeWindow primitive type + TimeWindowError + tests in oneshim-core::types` | 2h |
| 2 | `feat(error-codes): add TimeWindowCode wire codes (inverted_bounds + parse_failed)` | 1h |
| 3 | `test(error-codes): wire-error i18n CI gate update for 2 new variants (en+ko)` | 0.5h |
| 4 | `feat(api): TimeRangeQuery::to_time_window adapter + tests` | 1.5h |
| 5 | `refactor(storage): migrate SQL helpers (events/frames/calibration/web_storage_impl) to TimeWindow` | 3h |
| 6 | `test(storage): regression tests for migrated helpers` | 1h |
| 7 | `refactor(handlers): migrate REST handlers (frames/events/metrics/focus/sessions/interruptions) to TimeWindow` | 4h |
| 8 | `refactor(handlers): migrate data.rs (GDPR delete-range) + reports.rs to TimeWindow` | 1.5h |
| 9 | `refactor(models): migrate FocusMetrics + IdlePeriod + SessionMetrics + custom serde for FocusMetrics JSON compat` | 3h |
| 10 | `refactor(api-contracts): migrate DeleteRangeRequest period field` | 1h |
| 11 | `test(integration): end-to-end TimeWindow flow tests (REST→handler→storage→response)` | 2h |
| 12 | `docs(time-window): STATUS.md + PHASE-HISTORY entry + module-level rustdoc` | 1h |

**Total**: ~21h ≈ 3 working days. Add buffer for unexpected issues = ~4 days.

### 9.2 Branch naming

Branch: `refactor/timewindow-primitive` (already created in this worktree)

### 9.3 Release plan

After merge → `0.4.42-rc.1` (or batch with PR-B2 into single RC).

---

## 10. Migration & Backward Compatibility

### 10.1 External API contracts

- **REST API query strings** (`?from=X&to=Y`) — UNCHANGED
- **REST API response JSON** for `FocusMetrics` — preserved via Option Y serde (custom field names `period_start`/`period_end`)
- **REST API response JSON** for `DeleteRangeRequest` — CHANGED (new `period: { start, end }` shape vs old `from/to` flat). Frontend update needed.
- **Tauri IPC** — none affected (no time-range IPC commands identified)

### 10.2 Internal API (Rust) — breaking changes

- All SQL helpers signatures change: `(from: &str, to: &str)` → `(window: &TimeWindow)`
- All REST handlers internal logic changes — but external HTTP API unchanged
- Domain models field reorganization: 2 fields → 1 nested struct

These are internal — no external consumers (this is a desktop client, not a library).

### 10.3 Downgrade safety

- TimeWindow type is internal — downgrade restores old field-pair model.
- One JSON shape change (DeleteRangeRequest API) requires coordinated frontend rollback.

---

## 11. Open Questions for Phase 1 Deep Review

| # | Question | Resolution path |
|---|----------|-----------------|
| Q-1 | `FocusMetrics` JSON shape compat — Option X (flatten), Y (custom serde), or Z (break)? | Spec recommends Y. iter-1 review verifies whether frontend actually consumes `period_start`/`period_end` keys — if not used, Z is simpler. |
| Q-2 | `IdlePeriod` is currently `start_time + Option<end_time>` (open-ended for ongoing idle). TimeWindow always bounded. | Choose: (a) keep open-ended via separate `OngoingIdlePeriod` type, or (b) use TimeWindow with `end = now` for ongoing (renewed every poll). iter-1 review decides. |
| Q-3 | `ReportQuery` has `period: ReportPeriod` enum (Weekly/Monthly) AS WELL as from/to. Migration approach? | iter-1 review: does ReportPeriod enum override or supplement from/to? Possibly: `ReportQuery { window: Option<TimeWindow>, period_preset: Option<ReportPeriod> }`. |
| Q-4 | Some SQL helpers use `&str` (RFC3339) and others use `DateTime<Utc>` directly. Do we standardize call sites or keep both via `from_rfc3339_pair` / direct construction? | iter-1 review: prefer storage layer takes `&TimeWindow` (consistent), constructed at handler boundary. Detail commit-by-commit. |
| Q-5 | `flag_noise_range` in calibration_store_impl uses `(from: DateTime<Utc>, to: DateTime<Utc>)`. Migrate to `&TimeWindow`? | Yes per spec. iter-1 review confirms no test breakage. |
| Q-6 | Should `TimeWindow::new` accept `start == end` (zero-duration window)? | Spec says yes (single-instant query). Verify with iter-1 — any handler that would be confused by zero-duration? |
| Q-7 | `TimeWindow` field visibility: `pub start, pub end` vs accessor methods (`.start()`, `.end()`). | Spec uses `pub`. Trade-off: pub allows direct match but bypasses validation if someone constructs `TimeWindow { start: ..., end: ... }` directly without `new()`. Verify if needed. |
| Q-8 | Wire code prefix `time_window.*` — alphabetical position in `wire_contract_snapshot.expected.txt`? | Goes between `tag.*` and `update.*` (if those exist). Verify with current snapshot during impl. |
| Q-9 | gRPC `MetricBucket` (in src-tauri/src/grpc/) is bucketing primitive — explicitly NG2 (NOT in scope). Verify NG2 is correctly noted. | Document in §2.2 NG2 (already done). |

---

## 12. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `FocusMetrics` JSON shape break crashes frontend Dashboard | Medium | High | Spec Option Y (custom serde) — preserve `period_start`/`period_end` keys. Verified in Phase 1 iter-1. |
| `DeleteRangeRequest` JSON shape change breaks frontend GDPR UI | Low | Medium | Frontend likely doesn't have GDPR UI yet (or trivial migration). Document in PR description. |
| `IdlePeriod` `Option<end_time>` for ongoing idle — TimeWindow can't represent | Medium | Medium | Per Q-2: use `OngoingIdlePeriod` separate type OR use TimeWindow with `end = now` (renewed each poll). Decide in iter-1. |
| Big-bang PR cognitive load for reviewer | Medium | Low | Commit structure splits by domain (storage / handlers / models / GDPR). Reviewer can commit-by-commit. Deep review process catches issues. |
| Rebase pain if PR-B1 (#508) lands during impl | High | Medium | Implementation gate: wait for #508 merge before Phase 3 starts. |
| Wire code count drift if PR-B1/B2 ship between spec and impl | Medium | Low | Spec uses "current 51 (post-PR-B2)" as baseline. Adjust in impl based on actual count at merge time. |
| Unexpected SQL helper not in §1.2 catalog | Low | Low | iter-1 review sweeps with `grep` confirming all `*_in_range` and similar patterns. |
| Frontend TypeScript drift on FocusMetrics serde Y option | Medium | Low | If serde Y is correct, frontend types unchanged. Verify in Phase 1 iter-1 by checking frontend code consuming FocusMetrics. |

---

## 13. Cross-Consumer Dependencies

| Branch | Status | Files | Conflict |
|--------|--------|-------|----------|
| `feature/phase9-autostart-foundation` (PR #508) | OPEN, in review | `oneshim-core/config/sections/`, scheduler, frontend autostart | **Hard dep** for impl gate (rebase risk) |
| `feature/phase9-autostart-linux-deep` | Local plan ready, BLOCKED | Same as PR-B1 | After PR-B1 merge |
| `refactor/serve-external-inner-extraction` (PR #506) | OPEN | `oneshim-web/src/grpc/external/` | Disjoint |
| `ci/clippy-195-field-reassign-detection` (PR #509) | OPEN | `lefthook.yml` + scripts | Disjoint |

### 13.1 Recommended merge order

1. PR #506 (serve_external_inner) — disjoint, can merge anytime
2. PR #509 (clippy 1.95) — disjoint
3. PR #508 (PR-B1 autostart) — required before Phase 3 start of TimeWindow
4. PR-B2 (autostart Linux deep) — after PR-B1
5. **TimeWindow refactor PR** (this spec) — after PR-B2 (or parallel if PR-B2 not yet ready)

---

## 14. Spec Self-Review (v1)

### 14.1 Placeholder scan
- ⚠ Q-1 through Q-9 are intentional open questions for Phase 1 iter-1
- ⚠ §13 wire code count baseline assumes current state — adjust during impl based on actual merge timing
- ✅ No "TBD" in spec body

### 14.2 Internal consistency
- ✅ U1-U5 decisions consistently applied across §3, §4, §5
- ✅ Closed-closed semantic preserved in `to_sql_pair` (§5.1) and SQL pattern (§5.3)

### 14.3 Scope check
- ✅ Single PR scope (Big-bang per U2)
- ⚠ §11 Q-2 (IdlePeriod) could expand scope if `OngoingIdlePeriod` separate type chosen — defer to iter-1 decision

### 14.4 Ambiguity check
- ⚠ §5.4 "Option Y custom serde" — exact serde derive macro syntax not shown. iter-1 should specify (e.g., `#[serde(rename = "period_start")]`)
- ⚠ §10.2 "Frontend update needed" for DeleteRangeRequest — verify if frontend actually has GDPR UI (Q-1 supplement)

---

## 15. Implementation Status

- **Spec v1**: 2026-04-25 (this document)
- **Phase 1 deep review**: PENDING (next ralph-loop iteration)
- **Phase 2 plan creation**: PENDING (after Phase 1 closes)
- **Phase 3 implementation**: BLOCKED on PR-B1 (#508) merge
- **Worktree**: `.claude/worktrees/timewindow-primitive` on `refactor/timewindow-primitive`
- **Base**: `2ba38cf5` (origin/main)

---

**End of spec v1.**
