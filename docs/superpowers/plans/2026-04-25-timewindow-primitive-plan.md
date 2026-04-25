# TimeWindow Primitive Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate 5 main + 4 supporting divergent absolute-timestamp time-range types across the workspace into a single canonical `TimeWindow { start: DateTime<Utc>, end: DateTime<Utc> }` primitive in `oneshim-core::types`.

**Architecture:** Closed-closed `[start, end]` semantic (matches existing SQL `BETWEEN`, Stripe-style business API pattern). Big-bang single PR migration covering REST handlers + SQL storage + domain models + GDPR API + custom serde for backward-compat. Wall-clock recurrence types (TrackingWindow, coaching TimeRange) intentionally unmigrated. IdlePeriod intentionally unmigrated (per NG7).

**Tech Stack:**
- Rust + chrono 0.4.44 (`DateTime<Utc>`, `Duration`, `TimeZone::with_ymd_and_hms`)
- thiserror (existing convention for error types)
- serde — DeleteRangeRequest preserves external `from`/`to` keys via accessor pattern (Option C, NOT custom flatten serde — see Task 7 / Phase 2 iter-1 C9)
- ADR-019 wire codes via `define_code_enum!` macro
- rusqlite `params!` macro (existing pattern, prefer over slice)

**Source spec:** `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md` (v3, commit `f495dfbd`)

**Worktree:** `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/timewindow-primitive` on branch `refactor/timewindow-primitive`

**Total estimate:** ~28h across 11 tasks (~3.5-4 working days). Revised upward from v1 (was ~21h/12 tasks) to absorb Phase 2 iter-1 C6/C7 scope expansion (port trait + maintenance.rs caller enumeration).

**⚠ ABORT GUARD**: PR-B1 (#508) MUST merge before Task 1 begins. PR-B1 modifies `oneshim-core/config/sections/` and `oneshim-core/src/error_codes/` — overlapping crate areas. Implementing TimeWindow before #508 merges will cause significant rebase conflicts.

**Plan version:** v2 (Phase 2 iter-2 — addresses 9 Critical + 11 Important findings from `.claude/timewindow-review/phase2-iter1-findings.md`). Key v2 changes: Tasks 1+2 merged (avoids circular compile dep); CoreError uses struct-variant `{ code, message }` matching ADR-019 §4.6; explicit ApiError mapping in Task 1 (was missing); port trait scope expanded to 8 methods + 4+ caller sites; DeleteRangeRequest preserved via `period()` accessor (Option C — no custom serde); ReportQuery uses `#[serde(flatten)] time_range: TimeRangeQuery`.

---

## Pre-Flight Checks (before Task 1)

- [ ] **PF1: Verify PR-B1 (#508) is merged**

```bash
gh pr view 508 --json state | jq -r .state
```
Expected: `MERGED`. If `OPEN` or `CLOSED`: HALT. Resume after merge.

- [ ] **PF2: Rebase onto post-PR-B1 main**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/timewindow-primitive
git fetch origin
git rebase origin/main
# Resolve any conflicts
```

- [ ] **PF3: Capture wire snapshot baseline (Q-8 + Phase 2 C8)**

```bash
COUNT=$(wc -l < crates/oneshim-core/tests/wire_contract_snapshot.expected.txt)
echo "Current wire-code baseline: $COUNT"
```

Record the count. **Do NOT trust pre-merge estimates** ("post-PR-B1 = 47" was speculative). The actual baseline depends on whichever PRs merged into main since spec authoring (2ba38cf5 was 42). Compute actual count and use `BASELINE_COUNT + 2` everywhere wire-code total assertions appear.

Identify alphabetical insertion position. `time_window` < `tracking_schedule` (because `i` < `r` at index 5):
```bash
grep -n "^st\|^ti\|^tr\|^ui\|^update" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt
```

Expected post-insertion alphabetical block (assuming PR-B1 merged with `tracking_schedule.*`):
```
storage.failed
time_window.inverted_bounds        ← NEW (TimeWindow)
time_window.parse_failed           ← NEW (TimeWindow)
tracking_schedule.invalid_window   ← from PR-B1 if merged
tracking_schedule.overlap_detected ← from PR-B1 if merged
ui.element_missing
```

Also identify the i18n test count assertion lines (Phase 2 iter-1 C8 — there are TWO):
```bash
grep -n "toHaveLength\|expect.*Codes.*).*toHaveLength" crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts
```
Record both line numbers. As of plan-write time, lines 30 + 122 (both literal `42`).

- [ ] **PF4: Verify baseline GREEN**

```bash
cargo check --workspace
cargo test -p oneshim-core --test wire_contract_snapshot
cd crates/oneshim-web/frontend && pnpm test src/i18n/__tests__/translateError.test.ts --run && cd -
```
All expected GREEN.

- [ ] **PF5: Required reading + dep verification (Phase 2 iter-1 I4)**

```bash
# Verify oneshim-core workspace dep on api-contracts (will be needed by Task 3)
grep -E "^oneshim-core\s*=" crates/oneshim-api-contracts/Cargo.toml
```
If empty: HALT and add `oneshim-core = { workspace = true }` before proceeding.

```bash
# Verify CoreError struct-variant pattern (ADR-019 §4.6) in current source
grep -n -A 3 "Storage {\|Network {" crates/oneshim-core/src/error.rs | head -20
```
Confirm `Storage { code: StorageCode, message: String }` style — Task 1 mirrors this.

```bash
# Verify From<CoreError> for ApiError exists + has wildcard arm (Phase 2 iter-1 C2)
grep -n "From<CoreError> for ApiError\|=> ApiError::Internal" crates/oneshim-web/src/error.rs
```
Confirm the wildcard `other => ApiError::Internal(...)` exists. Task 1 must add an explicit BadRequest arm BEFORE the wildcard.

Also re-read these files before starting:

1. `crates/oneshim-core/src/lib.rs` — module registration pattern
2. `crates/oneshim-core/src/error.rs` — full `CoreError` struct-variants + `code()` method + `from_variants_display_includes_wire_code` regression test (ADR-019 invariant)
3. `crates/oneshim-core/src/error_codes/mod.rs` — `all_codes()` aggregator pattern
4. `crates/oneshim-core/src/error_codes/audio.rs` — `define_code_enum!` macro example
5. `crates/oneshim-api-contracts/src/common.rs:5-11` — current `TimeRangeQuery` struct
6. `crates/oneshim-storage/src/sqlite/frames.rs` — current `count_frames_in_range` signature
7. `crates/oneshim-storage/src/sqlite/web_storage_impl.rs` — thin wrapper layer + per-method delegation
8. `crates/oneshim-storage/src/sqlite/maintenance.rs` — `delete_data_in_range` (7+ params, NOT just from/to)
9. `crates/oneshim-web/src/handlers/frames.rs` — current handler using `TimeRangeQuery::with_defaults`
10. `crates/oneshim-core/src/ports/web_storage.rs` — 5 sub-trait `*_in_range` methods (Phase 2 iter-1 C6)
11. `crates/oneshim-core/src/ports/calibration_store.rs` — `flag_noise_range` + `get_entries` + `list_segment_time_ranges` (3 methods total per Phase 2 iter-1 C6)
12. `crates/oneshim-web/src/error.rs` — `From<CoreError> for ApiError` impl + `ErrorResponse` schema (no `code` field — Phase 2 iter-1 C3)
13. `src-tauri/src/scheduler/analysis_pipeline/regime.rs` — 3 `CalibrationReader` caller sites (Phase 2 iter-1 C6)
14. **Spec v3**: `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md`
15. **Phase 2 iter-1 findings**: `.claude/timewindow-review/phase2-iter1-findings.md`

---

## File Structure

### Files to be created

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/types/mod.rs` | Re-export `TimeWindow` and `TimeWindowError` |
| `crates/oneshim-core/src/types/time_window.rs` | `TimeWindow` struct + `TimeWindowError` enum + impl + tests |
| `crates/oneshim-core/src/error_codes/time_window.rs` | `TimeWindowCode` enum via `define_code_enum!` macro |
| `crates/oneshim-web/tests/timewindow_integration.rs` | E2E integration tests |

### Files to be modified — `oneshim-core`

| File | What changes |
|------|--------------|
| `crates/oneshim-core/src/lib.rs` | Add `pub mod types;` |
| `crates/oneshim-core/src/error_codes/mod.rs` | `pub mod time_window;` + `pub use time_window::TimeWindowCode;` + `for c in TimeWindowCode::all() { codes.push(c.as_str()); }` in `all_codes()` |
| `crates/oneshim-core/src/error.rs` | Add **struct-variant** `TimeWindow { code: TimeWindowCode, message: String }` to `CoreError` enum + manual `From<TimeWindowError> for CoreError` impl + `Self::TimeWindow { code, .. } => code.as_str()` arm in `CoreError::code()` (Phase 2 iter-1 C1 — match ADR-019 §4.6 majority pattern, NOT `#[from]` tuple) |
| `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` | Insert `time_window.inverted_bounds` + `time_window.parse_failed` in alphabetical position (between `storage.failed` and `tracking_schedule.*` if PR-B1 merged) |
| `crates/oneshim-core/src/ports/web_storage.rs` | **5 trait method signatures** updated to `&TimeWindow` (Phase 2 iter-1 C6): `FrameQueryStorage::count_frames_in_range`, `FrameQueryStorage::list_frame_file_paths_in_range`, `EventQueryStorage::count_events_in_range`, `StorageMaintenanceStorage::delete_data_in_range` (also has `delete_events/frames/metrics: bool` flags — see Task 4), `ActivityStatsStorage::get_daily_active_secs` |
| `crates/oneshim-core/src/ports/calibration_store.rs` | **3 trait method signatures** updated (Phase 2 iter-1 C6): `CalibrationWriter::flag_noise_range`, `CalibrationReader::get_entries`, `CalibrationReader::list_segment_time_ranges` |
| `crates/oneshim-core/src/models/work_session.rs` | `FocusMetrics::new(start, end) -> Result<Self, TimeWindowError>` + `period: TimeWindow` field (per NG8 — internal model only, REST DTO unchanged) |
| `crates/oneshim-core/src/models/telemetry.rs` | `SessionMetrics`: `period_*` → `period: TimeWindow`. **Note**: per Phase 2 iter-1 I1, this struct may be dead code — migrating for consistency, follow-up cleanup PR may delete |

### Files to be modified — `oneshim-web` (handler + ApiError)

| File | What changes |
|------|--------------|
| `crates/oneshim-web/src/error.rs` | **New explicit arm** `CoreError::TimeWindow { message, .. } => ApiError::BadRequest(message)` placed before wildcard `_ => ApiError::Internal` arm + regression test `time_window_inverted_bounds_maps_to_bad_request` (Phase 2 iter-1 C2) |
| `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json` | Add 2 new wire-error translations |
| `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json` | Add 2 Korean translations |
| `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts` | Update **TWO** `toHaveLength(BASELINE)` assertions on lines ~30 + ~122 (Phase 2 iter-1 C8) |
| `crates/oneshim-web/src/handlers/frames.rs` | Use `q.to_time_window(Duration::days(7))?` |
| `crates/oneshim-web/src/handlers/events.rs` | Use `q.to_time_window(Duration::days(7))?` |
| `crates/oneshim-web/src/handlers/metrics.rs` | Use `q.to_time_window(Duration::days(7))?` |
| `crates/oneshim-web/src/handlers/focus.rs` | Use `q.to_time_window(...)` (default_lookback `Duration::days(30)` for daily aggregate) |
| `crates/oneshim-web/src/handlers/idle.rs` | Use `q.to_time_window(...)` (handler only — IdlePeriod model NOT migrated per NG7) |
| `crates/oneshim-web/src/handlers/processes.rs` | Use `q.to_time_window(...)` |
| `crates/oneshim-web/src/handlers/data.rs` | Use `req.period()?` accessor (Option C — fields stay as `from: String` + `to: String`) |
| `crates/oneshim-web/src/handlers/reports.rs` | Use `req.time_range.to_time_window(Duration::days(30))?` (flatten pattern — Phase 2 iter-1 I11) |

**NOTE — handlers NOT touched** (Phase 2 iter-1 I3): `sessions.rs`, `interruptions.rs` were listed in spec §4.1 but `grep -rn "TimeRangeQuery" crates/oneshim-web/src/handlers/` confirms they don't use `TimeRangeQuery` directly. Plan excludes them. If Task 5 grep finds additional handlers using `TimeRangeQuery`, expand scope inline.

### Files to be modified — `oneshim-api-contracts`

| File | What changes |
|------|--------------|
| `crates/oneshim-api-contracts/src/common.rs` | (a) Add `Default` to `TimeRangeQuery` derive list (Phase 2 iter-1 C4) + (b) Add `to_time_window(&self, default_lookback) -> Result<TimeWindow, TimeWindowError>` impl |
| `crates/oneshim-api-contracts/src/data.rs` | Add `period() -> Result<TimeWindow, TimeWindowError>` accessor on `DeleteRangeRequest` (Option C, Phase 2 iter-1 C9 — keeps existing `from: String`, `to: String` fields → frontend JSON shape preserved trivially) |
| `crates/oneshim-api-contracts/src/reports.rs` | `ReportQuery { period: ReportPeriod, #[serde(flatten)] time_range: TimeRangeQuery }` (Phase 2 iter-1 I11 — flatten works for struct-typed fields, unlike C9's invalid combo). Add `to_time_window(default_lookback) -> Result<TimeWindow, ...>` accessor |

### Files to be modified — `oneshim-storage` SQLite adapters

Per Phase 2 iter-1 C6 + C7, the migration scope on SQL helpers is:

| File | Methods to migrate | Notes |
|------|---|---|
| `crates/oneshim-storage/src/sqlite/events.rs` | `count_events_in_range` | direct from/to → window |
| `crates/oneshim-storage/src/sqlite/frames.rs` | `count_frames_in_range`, `list_frame_file_paths_in_range` (if defined inherent) | direct from/to → window |
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | `list_frame_file_paths_in_range`, `delete_data_in_range` | **`delete_data_in_range` has 7+ params** (`from`, `to`, `delete_events: bool`, `delete_frames: bool`, `delete_metrics: bool`, ...). Replace ONLY `from`+`to` with `&TimeWindow`; preserve all other params (Phase 2 iter-1 C7) |
| `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs` | `flag_noise_range`, `get_entries`, `list_segment_time_ranges` | port-trait + impl in lockstep |
| `crates/oneshim-storage/src/sqlite/web_storage_impl.rs` | thin wrappers for ALL of the above + `get_daily_active_secs` | Each wrapper signature must match the new port trait sig |

### Files to be modified — `src-tauri` caller sites (Phase 2 iter-1 C6)

| File | Caller site | Change |
|------|-------------|--------|
| `src-tauri/src/scheduler/analysis_pipeline/regime.rs:44` | `calibration.get_entries(from, to, exclude_noise)` | Build `TimeWindow::new(from, to)?` then pass `&window` |
| `src-tauri/src/scheduler/analysis_pipeline/regime.rs:174` | `calibration.list_segment_time_ranges(from, to)` | Same pattern |
| `src-tauri/src/scheduler/analysis_pipeline/regime.rs:184` | `calibration.flag_noise_range(from, to)` | Same pattern |
| `src-tauri/src/scheduler/analysis_pipeline/tests.rs:19` | `MockCalibration` impl | Update mock signatures to match port trait change |

### Docs

| File | What changes |
|------|--------------|
| `docs/STATUS.md` | Test count update + version note |
| `docs/PHASE-HISTORY.md` | TimeWindow refactor entry |

---

## Task 1: TimeWindow Foundation — Primitive + Wire Codes + CoreError + ApiError Integration

**Estimate:** 4.5h | **Spec ref:** §5.1 + §7.2 + Phase 1 iter-1 C2/C3/I5 + Phase 2 iter-1 C1/C2/C5/I9 | **Files:** Create `crates/oneshim-core/src/types/mod.rs`, `crates/oneshim-core/src/types/time_window.rs`, `crates/oneshim-core/src/error_codes/time_window.rs`, modify `crates/oneshim-core/src/lib.rs`, `crates/oneshim-core/src/error_codes/mod.rs`, `crates/oneshim-core/src/error.rs`, `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`, `crates/oneshim-web/src/error.rs`

> **Why merged**: Per Phase 2 iter-1 I9, splitting into `Task 1 (TimeWindow)` then `Task 2 (TimeWindowCode)` creates a circular compile dependency — `time_window.rs` imports `crate::error_codes::TimeWindowCode` for its `code()` method. Both must land together. Per Phase 2 iter-1 C2 + I5, the ApiError mapping (`oneshim-web::error::From<CoreError>`) must also land in this commit so the wire-code → HTTP 400 chain is complete and the regression test in Step 1.10 passes.

- [ ] **Step 1.1: Create types/ directory + mod.rs**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/timewindow-primitive
mkdir -p crates/oneshim-core/src/types
```

Create `crates/oneshim-core/src/types/mod.rs`:
```rust
//! Domain primitive types shared across the workspace.
//!
//! Currently contains:
//! - `TimeWindow` — closed-bounded absolute time window for SQL/REST/domain
//!   model time-range needs. See `time_window.rs` for full documentation.

pub mod time_window;

pub use time_window::{TimeWindow, TimeWindowError};
```

- [ ] **Step 1.2: Create time_window.rs with type + impl**

Create `crates/oneshim-core/src/types/time_window.rs`:

```rust
//! Canonical time window primitive — closed-closed `[start, end]` absolute window.
//!
//! Per spec U4: ONESHIM is event-driven business API (Stripe-style), not
//! continuous time-series. Closed-closed semantic matches existing SQL `BETWEEN`
//! and user-facing date range expectations.
//!
//! Wall-clock recurrence types (`TrackingWindow`, coaching `TimeRange`) are
//! intentionally NOT unified — different domain (recurrence vs absolute window).
//!
//! `TimeWindow::new` is the validation-safe constructor. Direct struct literal
//! construction bypasses bound validation — use only when both bounds are known
//! to satisfy `start <= end`.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error_codes::TimeWindowCode;

/// Closed-bounded absolute time window. Both `start` and `end` are inclusive.
///
/// Validates `start <= end` at construction. Internally always uses `DateTime<Utc>`.
/// External serialization round-trips via RFC3339 ISO8601 strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TimeWindowError {
    #[error("start ({start}) must be <= end ({end})")]
    InvertedBounds {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
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

- [ ] **Step 1.3: Register `pub mod types;` in lib.rs**

Open `crates/oneshim-core/src/lib.rs`. Find existing `pub mod` declarations (alphabetical or grouped). Add:

```rust
pub mod types;
```

Pick the alphabetical position (likely between `tests` and `transport` or near `models`).

- [ ] **Step 1.4: Verify compile**

```bash
cargo check -p oneshim-core 2>&1 | tail -10
```
Expected: clean, OR error about `TimeWindowCode` not found (will be added in Task 2). If only `TimeWindowCode` error: comment out the `crate::error_codes::TimeWindowCode` import + `code()` method temporarily and Task 2 will restore.

- [ ] **Step 1.5: Add unit tests**

Append to `crates/oneshim-core/src/types/time_window.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    #[test]
    fn new_accepts_valid_bounds() {
        let w = TimeWindow::new(dt(2026, 4, 1), dt(2026, 4, 25)).unwrap();
        assert_eq!(w.start, dt(2026, 4, 1));
        assert_eq!(w.end, dt(2026, 4, 25));
    }

    #[test]
    fn new_accepts_zero_duration_window() {
        // Per spec Q-6 RESOLVED: start == end is valid (single-instant query)
        let same = dt(2026, 4, 25);
        let w = TimeWindow::new(same, same).unwrap();
        assert_eq!(w.duration(), Duration::zero());
    }

    #[test]
    fn new_rejects_inverted_bounds() {
        let result = TimeWindow::new(dt(2026, 4, 25), dt(2026, 4, 1));
        assert!(matches!(result, Err(TimeWindowError::InvertedBounds { .. })));
    }

    #[test]
    fn contains_includes_both_bounds() {
        let w = TimeWindow::new(dt(2026, 4, 1), dt(2026, 4, 25)).unwrap();
        assert!(w.contains(dt(2026, 4, 1)));   // start inclusive
        assert!(w.contains(dt(2026, 4, 15)));  // middle
        assert!(w.contains(dt(2026, 4, 25)));  // end inclusive
    }

    #[test]
    fn contains_excludes_outside() {
        let w = TimeWindow::new(dt(2026, 4, 1), dt(2026, 4, 25)).unwrap();
        assert!(!w.contains(dt(2026, 3, 31)));
        assert!(!w.contains(dt(2026, 4, 26)));
    }

    #[test]
    fn duration_returns_difference() {
        let w = TimeWindow::new(dt(2026, 4, 1), dt(2026, 4, 25)).unwrap();
        assert_eq!(w.duration(), Duration::days(24));
    }

    #[test]
    fn to_sql_pair_round_trips_via_from_rfc3339_pair() {
        let original = TimeWindow::new(dt(2026, 4, 1), dt(2026, 4, 25)).unwrap();
        let (from, to) = original.to_sql_pair();
        let restored = TimeWindow::from_rfc3339_pair(&from, &to).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn from_rfc3339_pair_accepts_z_suffix() {
        // Per spec §8.1 N2: verify both Z and +00:00 work
        let w = TimeWindow::from_rfc3339_pair(
            "2026-04-01T00:00:00Z",
            "2026-04-25T00:00:00Z",
        ).unwrap();
        assert_eq!(w.start, dt(2026, 4, 1));
    }

    #[test]
    fn from_rfc3339_pair_handles_timezone_offset() {
        let w = TimeWindow::from_rfc3339_pair(
            "2026-04-01T09:00:00+09:00",  // KST
            "2026-04-25T09:00:00+09:00",
        ).unwrap();
        // 09:00 KST = 00:00 UTC
        assert_eq!(w.start, dt(2026, 4, 1));
        assert_eq!(w.end, dt(2026, 4, 25));
    }

    #[test]
    fn from_rfc3339_pair_rejects_invalid_strings() {
        let result = TimeWindow::from_rfc3339_pair("not-a-date", "2026-04-25T00:00:00Z");
        assert!(matches!(result, Err(TimeWindowError::ParseFailed(_))));
    }

    #[test]
    fn serde_roundtrip_json() {
        let w = TimeWindow::new(dt(2026, 4, 1), dt(2026, 4, 25)).unwrap();
        let json = serde_json::to_string(&w).unwrap();
        let parsed: TimeWindow = serde_json::from_str(&json).unwrap();
        assert_eq!(w, parsed);
    }

    #[test]
    fn time_window_error_code_inverted_bounds() {
        let err = TimeWindow::new(dt(2026, 4, 25), dt(2026, 4, 1)).unwrap_err();
        assert_eq!(err.code(), TimeWindowCode::InvertedBounds);
    }

    #[test]
    fn time_window_error_code_parse_failed() {
        let err = TimeWindow::from_rfc3339_pair("invalid", "valid").unwrap_err();
        assert_eq!(err.code(), TimeWindowCode::ParseFailed);
    }
}
```

- [ ] **Step 1.6: Create TimeWindowCode wire-code enum**

Create `crates/oneshim-core/src/error_codes/time_window.rs`:

```rust
//! TimeWindowCode — TimeWindow 카테고리 에러 코드. `time_window.*` 접두사.

define_code_enum! {
    /// TimeWindow 카테고리 에러 코드.
    pub enum TimeWindowCode {
        /// start > end 검증 실패.
        InvertedBounds => "time_window.inverted_bounds",
        /// RFC3339 timestamp 파싱 실패.
        ParseFailed => "time_window.parse_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn as_str_round_trip_unique() {
        let codes: Vec<&str> = TimeWindowCode::all().iter().map(|c| c.as_str()).collect();
        let unique: HashSet<_> = codes.iter().collect();
        assert_eq!(codes.len(), unique.len());
    }

    #[test]
    fn naming_convention() {
        for c in TimeWindowCode::all() {
            let s = c.as_str();
            assert!(s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '.' || ch == '_'));
            assert!(s.starts_with("time_window."));
        }
    }

    #[test]
    fn display_matches_as_str() {
        for c in TimeWindowCode::all() {
            assert_eq!(format!("{c}"), c.as_str());
        }
    }
}
```

- [ ] **Step 1.7: Register TimeWindowCode in error_codes/mod.rs**

Open `crates/oneshim-core/src/error_codes/mod.rs`. Add:

1. After existing `pub mod` declarations, alphabetical position (`time_window` < `tracking_schedule` if PR-B1 merged):
```rust
pub mod time_window;
```

2. After existing `pub use` re-exports (alphabetical):
```rust
pub use time_window::TimeWindowCode;
```

3. In the `all_codes()` function, alphabetical position in the iteration list:
```rust
for c in TimeWindowCode::all() {
    codes.push(c.as_str());
}
```

- [ ] **Step 1.8: Add CoreError struct-variant + manual From impl (Phase 2 iter-1 C1)**

Open `crates/oneshim-core/src/error.rs`. Verify the **majority struct-variant pattern** by reading neighbors like `Storage { code: StorageCode, message: String }` and `Network { code: NetworkCode, message: String }`. The new variant follows that pattern exactly:

```rust
// In CoreError enum, alphabetical position (between Storage and Validation):
#[error("Time window error [{code}]: {message}")]
TimeWindow {
    code: crate::error_codes::TimeWindowCode,
    message: String,
},
```

In `CoreError::code()` method, add the match arm:
```rust
Self::TimeWindow { code, .. } => code.as_str(),
```

**Add manual `From<TimeWindowError>` impl** (NOT `#[from]` — Phase 2 iter-1 C1 — because each `TimeWindowError` variant needs to map to the correct `TimeWindowCode`):

```rust
// At the end of error.rs (or near other From impls):
impl From<crate::types::TimeWindowError> for CoreError {
    fn from(err: crate::types::TimeWindowError) -> Self {
        Self::TimeWindow {
            code: err.code(),
            message: err.to_string(),
        }
    }
}
```

This matches the existing `Storage { code, message }` / `Network { code, message }` pattern and lets `CoreError::code()` return the correct wire code per `TimeWindowError` variant. The ADR-019 `from_variants_display_includes_wire_code` regression invariant only applies to `#[from]` arms, NOT to manual `From` impls into struct-variants — so we're compliant.

- [ ] **Step 1.9: Add ApiError mapping (Phase 2 iter-1 C2 + I5)**

Open `crates/oneshim-web/src/error.rs`. Find the existing `From<CoreError> for ApiError` impl. It has a closed match with wildcard `other => ApiError::Internal(...)`. Add an explicit arm BEFORE the wildcard, near other 400 mappings (`Validation`, `InvalidArguments`):

```rust
impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        match err {
            // ... existing arms ...
            CoreError::Validation { message, .. } => ApiError::BadRequest(message),
            CoreError::InvalidArguments { message, .. } => ApiError::BadRequest(message),
            // NEW: TimeWindow validation errors are 400, not 500
            CoreError::TimeWindow { message, .. } => ApiError::BadRequest(message),
            // ... existing arms ...
            other => ApiError::Internal(other.to_string()),
        }
    }
}
```

Add a regression test (mirrors existing `permission_denied_maps_to_forbidden` pattern):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::error::CoreError;
    use oneshim_core::error_codes::TimeWindowCode;

    #[test]
    fn time_window_inverted_bounds_maps_to_bad_request() {
        let core = CoreError::TimeWindow {
            code: TimeWindowCode::InvertedBounds,
            message: "start > end".to_string(),
        };
        let api: ApiError = core.into();
        assert!(matches!(api, ApiError::BadRequest(_)));
    }

    #[test]
    fn time_window_parse_failed_maps_to_bad_request() {
        let core = CoreError::TimeWindow {
            code: TimeWindowCode::ParseFailed,
            message: "not a date".to_string(),
        };
        let api: ApiError = core.into();
        assert!(matches!(api, ApiError::BadRequest(_)));
    }
}
```

- [ ] **Step 1.10: Update wire_contract_snapshot.expected.txt**

```bash
grep -n "^st\|^ti\|^tr\|^ui\|^update" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt
```

Insert in alphabetical position (between `storage.failed` and either `tracking_schedule.*` from PR-B1 or `ui.element_missing`):

```
time_window.inverted_bounds
time_window.parse_failed
```

- [ ] **Step 1.11: Register types module in lib.rs (per Phase 1 iter-1 I5)**

Open `crates/oneshim-core/src/lib.rs`. Find the existing `pub mod` declarations block and add (alphabetical position):

```rust
pub mod types;
```

- [ ] **Step 1.12: Verify compile**

```bash
cargo check -p oneshim-core 2>&1 | tail -20
cargo check -p oneshim-web 2>&1 | tail -10
```
Both expected: clean.

- [ ] **Step 1.13: Run all new tests**

```bash
cargo test -p oneshim-core --lib types::time_window::tests 2>&1 | tail -20
cargo test -p oneshim-core --lib error_codes::time_window::tests 2>&1 | tail -10
cargo test -p oneshim-core --test wire_contract_snapshot 2>&1 | tail -10
cargo test -p oneshim-web --lib error::tests 2>&1 | tail -10
```
All expected GREEN — 13 TimeWindow tests + 3 TimeWindowCode tests + wire snapshot pass + 2 ApiError mapping tests.

- [ ] **Step 1.14: Commit**

Per Phase 2 iter-1 I10 (conventional commit scope alignment with existing repo convention `feat(core)`/`feat(error-codes)` not `feat(time)`):

```bash
git add crates/oneshim-core/src/types/ \
        crates/oneshim-core/src/lib.rs \
        crates/oneshim-core/src/error_codes/time_window.rs \
        crates/oneshim-core/src/error_codes/mod.rs \
        crates/oneshim-core/src/error.rs \
        crates/oneshim-core/tests/wire_contract_snapshot.expected.txt \
        crates/oneshim-web/src/error.rs
git commit -m "$(cat <<'EOF'
feat(core): add TimeWindow primitive + TimeWindowCode wire codes + CoreError::TimeWindow integration

Closes Phase 2 Task 1 of TimeWindow refactor. Includes:
- TimeWindow struct (closed-closed [start, end] absolute window) + constructor validation
- TimeWindowError (InvertedBounds, ParseFailed) with ADR-019 code() routing
- TimeWindowCode enum via define_code_enum! macro (2 wire codes)
- CoreError::TimeWindow struct-variant + manual From<TimeWindowError> impl (matches ADR-019 §4.6 majority pattern)
- ApiError::From<CoreError> arm: TimeWindow → 400 BadRequest (not Internal)
- Wire snapshot updated with 2 new alphabetical entries
- 13 unit tests for TimeWindow + 3 TimeWindowCode tests + 2 ApiError mapping regression tests
EOF
)"
```

---

## Task 2: Wire-Error i18n Translations

**Estimate:** 0.5h | **Spec ref:** §7.2 ADR-019 i18n CI gate + Phase 2 iter-1 C8 + N3 | **Files:** `crates/oneshim-web/frontend/src/i18n/wire-errors.{en,ko}.json`, `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts`

- [ ] **Step 2.1: Add en translations**

Open `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json`. Add (alphabetical position):
```json
  "time_window.inverted_bounds": "Invalid time range — start must be before end",
  "time_window.parse_failed": "Invalid timestamp format: {message}",
```

- [ ] **Step 2.2: Add ko translations (Phase 2 iter-1 N3 — naturalized phrasing)**

Open `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json`:
```json
  "time_window.inverted_bounds": "시간 범위가 올바르지 않습니다 — 시작 시각이 종료 시각보다 앞서야 합니다",
  "time_window.parse_failed": "타임스탬프 형식이 올바르지 않습니다: {message}",
```

- [ ] **Step 2.3: Update BOTH Vitest count expectations (Phase 2 iter-1 C8)**

Open `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts`. Find ALL count assertions:
```bash
grep -n "toHaveLength" crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts
```

There are **TWO** assertions (lines ~30 and ~122 as of plan-write time):
```typescript
// Line ~30:
expect(registry).toHaveLength(BASELINE)
// Line ~122:
expect(translatedCodes('en')).toHaveLength(BASELINE)
```

Replace BOTH with `BASELINE + 2` where `BASELINE` is the actual count captured in PF3. Also update the trailing comment that documents prior addition (e.g., `// 41 → 42 with D7 addition`) to mention TimeWindow:
```typescript
// 42 → 44 with TimeWindow primitive addition (or whatever BASELINE+2 is)
```

If the file has additional `expect(...).toHaveLength(\d+)` assertions discovered by the grep, update those too. **Do not assume only two — re-grep after PR-B1 merge to be sure.**

- [ ] **Step 2.4: Run CI gate + Vitest**

```bash
bash scripts/check-wire-error-i18n-coverage.sh 2>&1 | tail -5
cd crates/oneshim-web/frontend && pnpm test src/i18n/__tests__/translateError.test.ts --run 2>&1 | tail -10
```
Both expected GREEN.

- [ ] **Step 2.5: Commit**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/timewindow-primitive
git add crates/oneshim-web/frontend/src/i18n/wire-errors.en.json \
         crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json \
         crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts
git commit -m "test(i18n): wire-error translations for TimeWindow codes (en+ko)"
```

---

## Task 3: TimeRangeQuery::to_time_window Adapter

**Estimate:** 1.5h | **Spec ref:** §5.2 + Phase 1 iter-1 C4 + Phase 2 iter-1 C4/C5 | **Files:** Modify `crates/oneshim-api-contracts/src/common.rs`

- [ ] **Step 3.1: Add `Default` derive to TimeRangeQuery (Phase 2 iter-1 C4)**

Open `crates/oneshim-api-contracts/src/common.rs`. Find the existing `TimeRangeQuery` struct definition:

```rust
#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub min_importance: Option<f64>,
}
```

Add `Default` to the derive list. All fields are `Option<T>` so derive is zero-risk:

```rust
#[derive(Debug, Default, Deserialize)]
pub struct TimeRangeQuery { ... }
```

This unblocks `..Default::default()` in Step 3.3 tests.

- [ ] **Step 3.2: Add adapter method (non-consuming &self per Phase 1 iter-1 C4)**

Append `impl TimeRangeQuery { ... }` block:

```rust
use chrono::{DateTime, Duration, Utc};
use oneshim_core::types::{TimeWindow, TimeWindowError};

impl TimeRangeQuery {
    /// Convert REST query optional bounds into a bounded TimeWindow.
    ///
    /// - If `to` is None: defaults to `now()`
    /// - If `from` is None: defaults to `to - default_lookback`
    /// - `default_lookback` is domain-specific (frames=7d, reports=30d, etc.)
    ///
    /// Per spec U5: this is the boundary where Optional bounds become
    /// Required bounds. Internal code (storage, models) work with TimeWindow.
    ///
    /// Per Phase 1 iter-1 C4: takes `&self` (not `self`) so the 6 service sites
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

- [ ] **Step 3.3: Add tests using `Utc.with_ymd_and_hms` (Phase 2 iter-1 C5)**

Append `#[cfg(test)] mod time_window_adapter_tests` block. **Use chrono helpers — NOT hand-computed Unix timestamps** (Phase 2 iter-1 C5 found 6-day errors in v1's hand-computed integers):

```rust
#[cfg(test)]
mod time_window_adapter_tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn dt(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    #[test]
    fn to_time_window_with_both_bounds_provided() {
        let q = TimeRangeQuery {
            from: Some("2026-04-01T00:00:00Z".to_string()),
            to: Some("2026-04-25T00:00:00Z".to_string()),
            ..Default::default()
        };
        let w = q.to_time_window(Duration::days(7)).unwrap();
        assert_eq!(w.start, dt(2026, 4, 1));
        assert_eq!(w.end, dt(2026, 4, 25));
    }

    #[test]
    fn to_time_window_default_to_when_to_missing() {
        let q = TimeRangeQuery {
            from: Some("2026-04-01T00:00:00Z".to_string()),
            to: None,
            ..Default::default()
        };
        let before = Utc::now();
        let w = q.to_time_window(Duration::days(7)).unwrap();
        let after = Utc::now();
        assert!(w.end >= before && w.end <= after);
    }

    #[test]
    fn to_time_window_default_lookback_when_from_missing() {
        let q = TimeRangeQuery {
            from: None,
            to: Some("2026-04-25T00:00:00Z".to_string()),
            ..Default::default()
        };
        let w = q.to_time_window(Duration::days(7)).unwrap();
        // start = to - 7 days = 2026-04-18
        assert_eq!(w.end, dt(2026, 4, 25));
        assert_eq!(w.start, dt(2026, 4, 18));
        assert_eq!(w.duration(), Duration::days(7));
    }

    #[test]
    fn to_time_window_default_both_when_neither_provided() {
        let q = TimeRangeQuery {
            from: None,
            to: None,
            ..Default::default()
        };
        let w = q.to_time_window(Duration::days(7)).unwrap();
        assert_eq!(w.duration(), Duration::days(7));
    }

    #[test]
    fn to_time_window_rejects_invalid_iso8601_from() {
        let q = TimeRangeQuery {
            from: Some("not-a-date".to_string()),
            to: None,
            ..Default::default()
        };
        let result = q.to_time_window(Duration::days(7));
        assert!(matches!(result, Err(TimeWindowError::ParseFailed(_))));
    }

    #[test]
    fn to_time_window_rejects_invalid_iso8601_to() {
        let q = TimeRangeQuery {
            from: None,
            to: Some("also-not-a-date".to_string()),
            ..Default::default()
        };
        let result = q.to_time_window(Duration::days(7));
        assert!(matches!(result, Err(TimeWindowError::ParseFailed(_))));
    }

    #[test]
    fn to_time_window_rejects_inverted_bounds() {
        let q = TimeRangeQuery {
            from: Some("2026-04-25T00:00:00Z".to_string()),
            to: Some("2026-04-01T00:00:00Z".to_string()),
            ..Default::default()
        };
        let result = q.to_time_window(Duration::days(7));
        assert!(matches!(result, Err(TimeWindowError::InvertedBounds { .. })));
    }

    #[test]
    fn to_time_window_takes_ref_so_caller_keeps_other_fields() {
        // Phase 1 iter-1 C4 verification: &self adapter doesn't consume q
        let q = TimeRangeQuery {
            from: Some("2026-04-01T00:00:00Z".to_string()),
            to: Some("2026-04-25T00:00:00Z".to_string()),
            limit: Some(50),
            ..Default::default()
        };
        let _w = q.to_time_window(Duration::days(7)).unwrap();
        // q still usable after adapter call
        assert_eq!(q.limit, Some(50));
    }
}
```

- [ ] **Step 3.4: Verify compile + tests**

```bash
cargo test -p oneshim-api-contracts --lib common::time_window_adapter_tests 2>&1 | tail -20
```
Expected: 8 tests pass.

- [ ] **Step 3.5: Commit**

```bash
git add crates/oneshim-api-contracts/src/common.rs
git commit -m "feat(api-contracts): TimeRangeQuery::to_time_window adapter + Default derive"
```

---

## Task 4: SQL Storage Helper Migration + Calibration Port Trait — EXPANDED SCOPE

**Estimate:** 5h | **Spec ref:** §5.3 + Phase 1 iter-1 N3 + Phase 2 iter-1 C6/C7 | **Files:** Modify `crates/oneshim-core/src/ports/{web_storage,calibration_store}.rs`, `crates/oneshim-storage/src/sqlite/{events,frames,calibration_store_impl,web_storage_impl,maintenance}.rs`, `src-tauri/src/scheduler/analysis_pipeline/{regime,tests}.rs`

> **Phase 2 iter-1 C6 + C7 expansion**: scope grew from "1 port method (`flag_noise_range`) + 4 SQL impl files" to "8 port methods + 5 SQL impl files + 4 caller sites". Plan v1's 3h estimate was insufficient. Revised to 5h.

### Sub-task 4A: Update calibration_store.rs port trait (3 methods)

- [ ] **Step 4A.1: Add TimeWindow import to port file**

Open `crates/oneshim-core/src/ports/calibration_store.rs`. At top:
```rust
use crate::types::TimeWindow;
```

- [ ] **Step 4A.2: Update CalibrationWriter::flag_noise_range trait sig**

```rust
// Before:
async fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<(), CoreError>;
// After:
async fn flag_noise_range(&self, window: &TimeWindow) -> Result<(), CoreError>;
```

- [ ] **Step 4A.3: Update CalibrationReader::get_entries trait sig**

```rust
// Before:
async fn get_entries(&self, from: DateTime<Utc>, to: DateTime<Utc>, exclude_noise: bool) -> Result<Vec<CalibrationEntry>, CoreError>;
// After:
async fn get_entries(&self, window: &TimeWindow, exclude_noise: bool) -> Result<Vec<CalibrationEntry>, CoreError>;
```

- [ ] **Step 4A.4: Update CalibrationReader::list_segment_time_ranges trait sig**

```rust
// Before:
async fn list_segment_time_ranges(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>, CoreError>;
// After:
async fn list_segment_time_ranges(&self, window: &TimeWindow) -> Result<Vec<TimeWindow>, CoreError>;
```

(Optional refinement — return `Vec<TimeWindow>` instead of tuple list for consistency. Adjust if call-sites expect raw tuples.)

### Sub-task 4B: Update web_storage.rs port trait (5 methods)

- [ ] **Step 4B.1: Add TimeWindow import to port file**

Open `crates/oneshim-core/src/ports/web_storage.rs`. At top:
```rust
use crate::types::TimeWindow;
```

- [ ] **Step 4B.2: Update FrameQueryStorage trait (2 methods)**

```rust
// Before:
fn count_frames_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError>;
fn list_frame_file_paths_in_range(&self, from: &str, to: &str, /*other params*/) -> Result<Vec<String>, CoreError>;
// After:
fn count_frames_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError>;
fn list_frame_file_paths_in_range(&self, window: &TimeWindow, /*other params*/) -> Result<Vec<String>, CoreError>;
```

- [ ] **Step 4B.3: Update EventQueryStorage::count_events_in_range trait sig**

```rust
// Before:
fn count_events_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError>;
// After:
fn count_events_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError>;
```

- [ ] **Step 4B.4: Update StorageMaintenanceStorage::delete_data_in_range trait sig (Phase 2 iter-1 C7)**

**WARNING**: this method has **7+ parameters** (`from`, `to`, `delete_events: bool`, `delete_frames: bool`, `delete_metrics: bool`, plus possibly more). Replace ONLY the `from` + `to` pair with `&TimeWindow`. Preserve all boolean flags + other params:

```rust
// Before:
fn delete_data_in_range(
    &self,
    from: &str,
    to: &str,
    delete_events: bool,
    delete_frames: bool,
    delete_metrics: bool,
    /*possibly more*/
) -> Result<DeleteSummary, CoreError>;

// After:
fn delete_data_in_range(
    &self,
    window: &TimeWindow,
    delete_events: bool,
    delete_frames: bool,
    delete_metrics: bool,
    /*possibly more*/
) -> Result<DeleteSummary, CoreError>;
```

- [ ] **Step 4B.5: Update ActivityStatsStorage::get_daily_active_secs trait sig**

```rust
// Before:
fn get_daily_active_secs(&self, from: &str, to: &str) -> Result<u64, CoreError>;
// After:
fn get_daily_active_secs(&self, window: &TimeWindow) -> Result<u64, CoreError>;
```

### Sub-task 4C: Update SQLite impls + thin wrappers

- [ ] **Step 4C.1: Migrate calibration_store_impl.rs (3 methods)**

Open `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs`. For each of `flag_noise_range`, `get_entries`, `list_segment_time_ranges`:

```rust
use oneshim_core::types::TimeWindow;

async fn flag_noise_range(&self, window: &TimeWindow) -> Result<(), CoreError> {
    let conn = self.conn.lock().unwrap();
    let (from, to) = window.to_sql_pair();
    conn.execute(
        "UPDATE calibration SET noise = 1 WHERE timestamp >= ?1 AND timestamp <= ?2",
        rusqlite::params![&from, &to],
    )?;
    Ok(())
}

async fn get_entries(&self, window: &TimeWindow, exclude_noise: bool) -> Result<Vec<CalibrationEntry>, CoreError> {
    let conn = self.conn.lock().unwrap();
    let (from, to) = window.to_sql_pair();
    // ... existing query body with `&from, &to` substituted via params! macro
}

async fn list_segment_time_ranges(&self, window: &TimeWindow) -> Result<Vec<TimeWindow>, CoreError> {
    let conn = self.conn.lock().unwrap();
    let (from, to) = window.to_sql_pair();
    // ... existing query, then map row tuples → TimeWindow::new(start, end).expect(...)
    // (rows from DB are trusted; expect() OK)
}
```

- [ ] **Step 4C.2: Migrate frames.rs (1-2 methods)**

```bash
grep -n "in_range\|fn.*from.*to\|fn.*&str.*&str" crates/oneshim-storage/src/sqlite/frames.rs
```

For each match, apply `&TimeWindow` + `let (from, to) = window.to_sql_pair();` pattern. Use `rusqlite::params![&from, &to]` macro per Phase 1 iter-1 N4.

- [ ] **Step 4C.3: Migrate events.rs (1 method)**

Same pattern for `count_events_in_range`.

- [ ] **Step 4C.4: Migrate maintenance.rs (2 methods — Phase 2 iter-1 C7)**

Open `crates/oneshim-storage/src/sqlite/maintenance.rs`. Find `list_frame_file_paths_in_range` (~line 253) and `delete_data_in_range` (~line 286).

For `delete_data_in_range`: signature has 7+ params. Replace **only** `from: &str, to: &str` with `window: &TimeWindow`. Inside the body where `from`/`to` are bound to SQL params, use `let (from, to) = window.to_sql_pair();` then continue using local `from`/`to` String vars unchanged in subsequent SQL execution code.

Example shape:
```rust
pub fn delete_data_in_range(
    &self,
    window: &TimeWindow,
    delete_events: bool,
    delete_frames: bool,
    delete_metrics: bool,
    /* other existing params unchanged */
) -> Result<DeleteSummary, CoreError> {
    let (from, to) = window.to_sql_pair();
    let mut summary = DeleteSummary::default();
    let conn = self.conn.lock().unwrap();
    if delete_events {
        let n = conn.execute(
            "DELETE FROM events WHERE timestamp >= ?1 AND timestamp <= ?2",
            rusqlite::params![&from, &to],
        )?;
        summary.events = n;
    }
    if delete_frames {
        // similar pattern
    }
    // ...
    Ok(summary)
}
```

- [ ] **Step 4C.5: Migrate web_storage_impl.rs thin wrappers**

Open `crates/oneshim-storage/src/sqlite/web_storage_impl.rs`. This is a delegation-only file — every wrapper method must match the new port-trait sig. Verify:

```bash
grep -n "fn .*_in_range\|fn .*from.*&str.*to.*&str\|fn get_daily_active_secs" crates/oneshim-storage/src/sqlite/web_storage_impl.rs
```

For EACH wrapper found:
```rust
// Before:
fn count_events_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError> {
    SqliteStorage::count_events_in_range(self, from, to).map_err(Into::into)
}
// After:
fn count_events_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError> {
    SqliteStorage::count_events_in_range(self, window).map_err(Into::into)
}
```

Apply this same shape change to ALL wrappers — there are at least 5 (per Phase 2 iter-1 C7 enumeration), likely more.

- [ ] **Step 4C.6: Verify compile (lockstep check)**

```bash
cargo check -p oneshim-core 2>&1 | tail -10
cargo check -p oneshim-storage 2>&1 | tail -20
```
Both expected: clean. If `oneshim-storage` errors with "method signature mismatch", a port-trait sig diverged from impl — fix lockstep before proceeding.

### Sub-task 4D: Update src-tauri caller sites (Phase 2 iter-1 C6)

- [ ] **Step 4D.1: Update regime.rs callers (3 sites)**

Open `src-tauri/src/scheduler/analysis_pipeline/regime.rs`. Find calls (lines ~44, ~174, ~184):

```rust
// Before:
let entries = calibration.get_entries(from_dt, to_dt, exclude_noise).await?;
// After:
let window = TimeWindow::new(from_dt, to_dt)?; // ? converts via From<TimeWindowError> for CoreError
let entries = calibration.get_entries(&window, exclude_noise).await?;
```

```rust
// Before:
let segments = calibration.list_segment_time_ranges(from_dt, to_dt).await?;
// After:
let window = TimeWindow::new(from_dt, to_dt)?;
let segments = calibration.list_segment_time_ranges(&window).await?;
```

```rust
// Before:
calibration.flag_noise_range(noise_from, noise_to).await?;
// After:
let noise_window = TimeWindow::new(noise_from, noise_to)?;
calibration.flag_noise_range(&noise_window).await?;
```

Add at top of file:
```rust
use oneshim_core::types::TimeWindow;
```

- [ ] **Step 4D.2: Update tests.rs MockCalibration (1 mock)**

Open `src-tauri/src/scheduler/analysis_pipeline/tests.rs`. Find `MockCalibration` impl. Update its trait method signatures to match new port-trait sigs:

```rust
use oneshim_core::types::TimeWindow;

#[async_trait]
impl CalibrationReader for MockCalibration {
    async fn get_entries(&self, _window: &TimeWindow, _exclude_noise: bool) -> Result<Vec<CalibrationEntry>, CoreError> {
        Ok(vec![])
    }
    async fn list_segment_time_ranges(&self, _window: &TimeWindow) -> Result<Vec<TimeWindow>, CoreError> {
        Ok(vec![])
    }
}

#[async_trait]
impl CalibrationWriter for MockCalibration {
    async fn flag_noise_range(&self, _window: &TimeWindow) -> Result<(), CoreError> {
        Ok(())
    }
}
```

- [ ] **Step 4D.3: Verify full compile**

```bash
cargo check --workspace 2>&1 | tail -20
```
Expected: clean across ALL crates including src-tauri. If errors remain, additional caller sites exist beyond the 3 enumerated — grep them out and migrate.

### Sub-task 4E: Commit

- [ ] **Step 4E.1: Commit**

```bash
git add crates/oneshim-core/src/ports/{web_storage,calibration_store}.rs \
        crates/oneshim-storage/src/sqlite/ \
        src-tauri/src/scheduler/analysis_pipeline/{regime,tests}.rs
git commit -m "$(cat <<'EOF'
refactor(storage): migrate 8 SQL range helpers + 4 caller sites to &TimeWindow

Per Phase 2 iter-1 C6/C7 scope expansion. Touches:
- 3 CalibrationReader/Writer port-trait sigs (calibration_store.rs)
- 5 WebStorage sub-trait port-trait sigs (web_storage.rs)
- 5 SQLite impl files (events, frames, maintenance, calibration_store_impl, web_storage_impl)
- 3 src-tauri/scheduler/analysis_pipeline/regime.rs caller sites
- 1 MockCalibration in src-tauri/scheduler/analysis_pipeline/tests.rs
- delete_data_in_range preserves 5 boolean flag params; only from/to → &TimeWindow

All changes done in lockstep — port trait + impl + wrappers + callers + mocks.
EOF
)"
```

---

## Task 5: Storage Regression Tests

**Estimate:** 1.5h | **Spec ref:** §8.3 | **Files:** Existing `#[cfg(test)] mod tests` in `crates/oneshim-storage/src/sqlite/{frames,events,calibration_store_impl,web_storage_impl,maintenance}.rs`

- [ ] **Step 5.1: Update existing tests for new signatures**

For each migrated SQL helper, find existing tests in the same file's `#[cfg(test)] mod tests` block. Update calls:

```rust
// Before:
let count = storage.count_frames_in_range("2026-04-01T00:00:00Z", "2026-04-25T00:00:00Z").unwrap();

// After:
use oneshim_core::types::TimeWindow;
let window = TimeWindow::from_rfc3339_pair(
    "2026-04-01T00:00:00Z",
    "2026-04-25T00:00:00Z",
).unwrap();
let count = storage.count_frames_in_range(&window).unwrap();
```

- [ ] **Step 5.2: Add boundary regression tests (3 helpers)**

Add closed-closed boundary regression test for each of: `count_frames_in_range`, `count_events_in_range`, `delete_data_in_range`. Pattern:

```rust
#[test]
fn count_frames_in_range_includes_both_boundaries() {
    let storage = test_storage();
    let t1 = "2026-04-01T00:00:00Z";
    let t2 = "2026-04-25T00:00:00Z";
    storage.insert_frame_at(t1).unwrap();  // exactly at start
    storage.insert_frame_at("2026-04-15T00:00:00Z").unwrap();  // middle
    storage.insert_frame_at(t2).unwrap();  // exactly at end
    let window = TimeWindow::from_rfc3339_pair(t1, t2).unwrap();
    assert_eq!(storage.count_frames_in_range(&window).unwrap(), 3);
}
```

For `delete_data_in_range`, additionally verify boolean flag preservation:
```rust
#[test]
fn delete_data_in_range_respects_delete_flags() {
    let storage = test_storage();
    let window = TimeWindow::from_rfc3339_pair("2026-04-01T00:00:00Z", "2026-04-25T00:00:00Z").unwrap();
    seed_one_each(&storage);  // 1 event + 1 frame + 1 metric in window
    let summary = storage.delete_data_in_range(&window, true, false, false /* + others */).unwrap();
    assert_eq!(summary.events, 1);
    assert_eq!(summary.frames, 0); // flag was false
    assert_eq!(summary.metrics, 0); // flag was false
}
```

(Adapt to actual `DeleteSummary` struct field names.)

- [ ] **Step 5.3: Run all storage tests**

```bash
cargo test -p oneshim-storage 2>&1 | tail -15
```
Expected: all pre-existing tests pass + new boundary tests pass.

- [ ] **Step 5.4: Commit**

```bash
git add crates/oneshim-storage/src/sqlite/
git commit -m "test(storage): boundary regression tests for migrated SQL helpers (closed-closed + delete flag preservation)"
```

---

## Task 6: REST Handler Migration (frames/events/metrics/focus/idle/processes)

**Estimate:** 3h | **Spec ref:** §5.5 + Phase 2 iter-1 I3 | **Files:** `crates/oneshim-web/src/handlers/{frames,events,metrics,focus,idle,processes}.rs`

> **Phase 2 iter-1 I3**: spec §4.1 listed 8 handlers (`frames`/`events`/`metrics`/`focus`/`sessions`/`interruptions`/`data`/`reports`). Reality from `grep -rn "TimeRangeQuery" crates/oneshim-web/src/handlers/`: only 6 handlers use `TimeRangeQuery` directly (`frames`, `events`, `metrics`, `focus`, `idle`, `processes`). `sessions.rs` and `interruptions.rs` use typed query structs, not `TimeRangeQuery`. `data.rs` + `reports.rs` are covered in Task 7. Plan limited to actual usage.

- [ ] **Step 6.0: Verify the actual handler list**

```bash
grep -rln "TimeRangeQuery" crates/oneshim-web/src/handlers/
```
Expected: 6 files. **If the grep finds additional files** (e.g., new handlers added since plan write): expand scope inline and migrate each before commit.

- [ ] **Step 6.1: Migrate frames.rs handler**

Open `crates/oneshim-web/src/handlers/frames.rs`. Find handler using `TimeRangeQuery::with_defaults`:

```rust
pub async fn list_frames(
    Query(q): Query<TimeRangeQuery>,
    State(ctx): State<...>,
) -> Result<Json<Vec<FrameDto>>, ApiError> {
    let q = q.with_defaults(7);
    let from = q.from.unwrap();
    let to = q.to.unwrap();
    let frames = ctx.storage.get_frames(from.parse()?, to.parse()?, 100)?;
    Ok(Json(frames))
}
```

Change to:
```rust
use chrono::Duration;

pub async fn list_frames(
    Query(q): Query<TimeRangeQuery>,
    State(ctx): State<...>,
) -> Result<Json<Vec<FrameDto>>, ApiError> {
    let window = q.to_time_window(Duration::days(7))?;
    let frames = ctx.storage.get_frames(&window, q.limit.unwrap_or(100))?;
    Ok(Json(frames))
}
```

The `?` operator works through this chain (all wired in Task 1):
- `to_time_window` returns `Result<TimeWindow, TimeWindowError>`
- `From<TimeWindowError> for CoreError` (manual impl in Task 1.8)
- `From<CoreError> for ApiError` with explicit `TimeWindow → BadRequest` arm (Task 1.9)

Result: invalid timestamps → HTTP 400 with parse error message. Inverted bounds → HTTP 400 with "start ... must be <= end ..." message.

- [ ] **Step 6.2: Migrate events.rs**

Same pattern. Use `Duration::days(7)`.

- [ ] **Step 6.3: Migrate metrics.rs**

Same pattern. Use `Duration::days(7)`.

- [ ] **Step 6.4: Migrate focus.rs**

Use `Duration::days(30)` (focus_metrics is daily aggregate, longer default lookback).

- [ ] **Step 6.5: Migrate idle.rs (handler only — IdlePeriod model NOT migrated per NG7)**

Same pattern for handler. The model `IdlePeriod` retains its current `start_time + Option<end_time>` shape (per NG7 — open-ended ongoing idle period incompatible with TimeWindow's required `end`).

- [ ] **Step 6.6: Migrate processes.rs**

Same pattern. Use `Duration::days(7)`.

- [ ] **Step 6.7: Verify compile**

```bash
cargo check -p oneshim-web 2>&1 | tail -10
```

- [ ] **Step 6.8: Run handler tests**

```bash
cargo test -p oneshim-web --lib handlers 2>&1 | tail -15
```

- [ ] **Step 6.9: Commit**

```bash
git add crates/oneshim-web/src/handlers/{frames,events,metrics,focus,idle,processes}.rs
git commit -m "refactor(web-handlers): migrate 6 REST handlers to TimeRangeQuery::to_time_window adapter"
```

---

## Task 7: Migrate data.rs (GDPR) + reports.rs — Accessor Pattern

**Estimate:** 1.5h | **Spec ref:** §5.6 + Q-3 + Q-10 + Phase 2 iter-1 C9/I11 | **Files:** `crates/oneshim-api-contracts/src/data.rs`, `crates/oneshim-api-contracts/src/reports.rs`, `crates/oneshim-web/src/handlers/data.rs`, `crates/oneshim-web/src/handlers/reports.rs`

> **Phase 2 iter-1 C9 + I11**: plan v1's `#[serde(flatten, with = "...")]` is invalid — flatten and `with` don't compose. Use **Option C accessor pattern** for DeleteRangeRequest (keeps `from: String, to: String` fields untouched, adds `period() -> Result<TimeWindow, ...>` accessor). For ReportQuery, use `#[serde(flatten)] time_range: TimeRangeQuery` (flatten DOES work for struct-typed fields).

- [ ] **Step 7.1: DeleteRangeRequest — add `period()` accessor (Option C, Phase 2 iter-1 C9)**

Open `crates/oneshim-api-contracts/src/data.rs`. **DO NOT change struct fields.** Existing struct stays as-is:
```rust
#[derive(Debug, Deserialize)]
pub struct DeleteRangeRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub data_types: Vec<String>,
}
```

Add accessor:
```rust
use oneshim_core::types::{TimeWindow, TimeWindowError};

impl DeleteRangeRequest {
    /// Construct a TimeWindow from the request's from/to string fields.
    /// Per Phase 1 iter-1 Q-10 option (b) + Phase 2 iter-1 C9 Option C:
    /// keeps external JSON shape (`from`, `to` keys) AND internal struct shape
    /// trivially. Frontend DataSection.tsx requires NO changes.
    pub fn period(&self) -> Result<TimeWindow, TimeWindowError> {
        TimeWindow::from_rfc3339_pair(&self.from, &self.to)
    }
}
```

- [ ] **Step 7.2: Add roundtrip test**

Append to `data.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_range_request_external_shape_preserved() {
        // Frontend sends from/to keys — no change required
        let json = r#"{"from":"2026-04-01T00:00:00Z","to":"2026-04-25T00:00:00Z","data_types":["frames"]}"#;
        let req: DeleteRangeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.from, "2026-04-01T00:00:00Z");
        assert_eq!(req.to, "2026-04-25T00:00:00Z");
        assert_eq!(req.data_types, vec!["frames"]);
    }

    #[test]
    fn delete_range_request_period_accessor_returns_window() {
        let req = DeleteRangeRequest {
            from: "2026-04-01T00:00:00Z".to_string(),
            to: "2026-04-25T00:00:00Z".to_string(),
            data_types: vec!["frames".to_string()],
        };
        let window = req.period().unwrap();
        assert_eq!(window.start.timestamp(), TimeWindow::from_rfc3339_pair(&req.from, &req.to).unwrap().start.timestamp());
    }

    #[test]
    fn delete_range_request_period_rejects_inverted_bounds() {
        let req = DeleteRangeRequest {
            from: "2026-04-25T00:00:00Z".to_string(),
            to: "2026-04-01T00:00:00Z".to_string(),
            data_types: vec![],
        };
        assert!(matches!(req.period(), Err(TimeWindowError::InvertedBounds { .. })));
    }
}
```

- [ ] **Step 7.3: Migrate ReportQuery via flatten (Phase 2 iter-1 I11)**

Open `crates/oneshim-api-contracts/src/reports.rs`. Current:
```rust
#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    #[serde(default)]
    pub period: ReportPeriod,
    pub from: Option<String>,
    pub to: Option<String>,
}
```

Change to use `#[serde(flatten)]` (which DOES work for struct-typed fields, unlike C9's invalid `flatten + with` combo):
```rust
use oneshim_api_contracts::common::TimeRangeQuery;

#[derive(Debug, Deserialize, Default)]
pub struct ReportQuery {
    #[serde(default)]
    pub period: ReportPeriod,
    #[serde(flatten)]
    pub time_range: TimeRangeQuery,
}

impl ReportQuery {
    /// Convenience accessor — only meaningful when period == ReportPeriod::Custom.
    /// For Week/Month, callers should compute the window from period semantics
    /// rather than from time_range fields.
    pub fn to_time_window(&self, default_lookback: chrono::Duration) -> Result<oneshim_core::types::TimeWindow, oneshim_core::types::TimeWindowError> {
        self.time_range.to_time_window(default_lookback)
    }
}
```

This preserves the existing query string contract (`?period=custom&from=...&to=...`) — frontend unaffected.

Add roundtrip test:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_query_query_string_roundtrip() {
        // Simulating axum's serde-urlencoded (similar shape)
        let raw = "period=custom&from=2026-04-01T00:00:00Z&to=2026-04-25T00:00:00Z";
        let q: ReportQuery = serde_urlencoded::from_str(raw).unwrap();
        assert_eq!(q.period, ReportPeriod::Custom);
        assert_eq!(q.time_range.from, Some("2026-04-01T00:00:00Z".to_string()));
        assert_eq!(q.time_range.to, Some("2026-04-25T00:00:00Z".to_string()));
    }
}
```

(Add `serde_urlencoded` to dev-dependencies if not already present.)

- [ ] **Step 7.4: Update data.rs handler**

Open `crates/oneshim-web/src/handlers/data.rs`. Find handler using `req.from`/`req.to` for SQL deletion. Replace with:

```rust
let window = req.period()?;  // Result<TimeWindow, TimeWindowError> → ApiError::BadRequest via chain
ctx.storage.delete_data_in_range(
    &window,
    req.data_types.contains(&"events".to_string()),
    req.data_types.contains(&"frames".to_string()),
    req.data_types.contains(&"metrics".to_string()),
    /* preserve other params */
)?;
```

(Adapt to actual `DeleteRangeRequest` field semantics for the boolean flags.)

- [ ] **Step 7.5: Update reports.rs handler**

Open `crates/oneshim-web/src/handlers/reports.rs`. Find handler. Update to use `req.time_range.to_time_window(Duration::days(30))?` for Custom period; period-derived computation for Week/Month:

```rust
use chrono::Duration;
use oneshim_core::types::TimeWindow;

let window = match req.period {
    ReportPeriod::Custom => req.time_range.to_time_window(Duration::days(30))?,
    ReportPeriod::Week => {
        let now = Utc::now();
        TimeWindow::new(now - Duration::days(7), now).expect("valid 7-day window")
    },
    ReportPeriod::Month => {
        let now = Utc::now();
        TimeWindow::new(now - Duration::days(30), now).expect("valid 30-day window")
    },
};
```

- [ ] **Step 7.6: Verify compile + tests**

```bash
cargo check -p oneshim-web -p oneshim-api-contracts 2>&1 | tail -10
cargo test -p oneshim-api-contracts --lib data::tests reports::tests 2>&1 | tail -15
```

- [ ] **Step 7.7: Commit**

```bash
git add crates/oneshim-api-contracts/src/{data,reports}.rs crates/oneshim-web/src/handlers/{data,reports}.rs
git commit -m "$(cat <<'EOF'
refactor(api-contracts): DeleteRangeRequest period() accessor + ReportQuery flatten TimeRangeQuery

- DeleteRangeRequest: keeps from/to String fields (Phase 2 iter-1 C9 Option C —
  preserves frontend JSON shape trivially, NO custom serde required).
  Adds period() -> Result<TimeWindow, TimeWindowError> accessor.
- ReportQuery: uses #[serde(flatten)] time_range: TimeRangeQuery (Phase 2 iter-1
  I11 — flatten works for struct-typed fields). Preserves existing
  ?period=custom&from=X&to=Y query string contract.
- Both data.rs + reports.rs handlers updated.
- Roundtrip tests verify external JSON / query string shape.
EOF
)"
```

---

## Task 8: Domain Model Migration (FocusMetrics + SessionMetrics)

**Estimate:** 2h | **Spec ref:** §5.4 + NG7 + NG8 + Phase 2 iter-1 I1/I2 | **Files:** `crates/oneshim-core/src/models/work_session.rs`, `crates/oneshim-core/src/models/telemetry.rs`, plus 4 FocusMetrics call sites

> **Phase 2 iter-1 I2**: 4 `FocusMetrics { period_start, period_end }` call sites identified by grep. All must be migrated. The constructor `FocusMetrics::new(period_start, period_end) -> Self` must change to `FocusMetrics::new(start, end) -> Result<Self, TimeWindowError>` (since `TimeWindow::new` can fail). Internal callers use trusted construction (cron-aligned `date_to_period_range`) so `.expect("date_to_period_range produces valid window")` is acceptable.

> **Phase 2 iter-1 I1**: `oneshim_core::models::telemetry::SessionMetrics` has zero workspace callers (verified). Migrating for consistency. Follow-up cleanup PR may delete entirely.

- [ ] **Step 8.1: Enumerate all FocusMetrics call sites first**

```bash
grep -rn "FocusMetrics {" crates/
grep -rn "FocusMetrics::new" crates/
grep -rn "period_start\|period_end" crates/oneshim-storage/src/sqlite/edge_intelligence/
```

Expected sites (per Phase 2 iter-1 I2 enumeration):
- `crates/oneshim-storage/src/sqlite/edge_intelligence/focus_metrics.rs` (~lines 43-57, 213, 218 — splits `(start, end)` from `date_to_period_range`)
- `crates/oneshim-storage/src/sqlite/edge_intelligence/tests.rs` (~line 76 — `FocusMetrics::new(updated.period_start, updated.period_end)`)
- `crates/oneshim-web/tests/grpc_dashboard_integration.rs` (~line 461 — test fixture)
- Possibly `crates/oneshim-core/src/models/work_session.rs:317` (uses `(self.period_end - self.period_start).num_seconds()`)

If grep finds different/more sites: update plan inline.

- [ ] **Step 8.2: Migrate FocusMetrics struct + constructor (Option Z, NG8)**

Open `crates/oneshim-core/src/models/work_session.rs`. Find `FocusMetrics`:
```rust
pub struct FocusMetrics {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub deep_work_secs: u64,
    // ... other fields unchanged
}

impl FocusMetrics {
    pub fn new(period_start: DateTime<Utc>, period_end: DateTime<Utc>) -> Self {
        Self { period_start, period_end, /* ... */ }
    }
}
```

Change to:
```rust
use crate::types::{TimeWindow, TimeWindowError};

pub struct FocusMetrics {
    pub period: TimeWindow,
    pub deep_work_secs: u64,
    // ... other fields unchanged
}

impl FocusMetrics {
    /// Constructor returns Result because TimeWindow::new validates start <= end.
    /// Internal callers using cron-aligned date_to_period_range may use
    /// `.expect("date_to_period_range produces valid window")`.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, TimeWindowError> {
        let period = TimeWindow::new(start, end)?;
        Ok(Self { period, /* defaults for other fields */ })
    }
}
```

Also find any internal `(self.period_end - self.period_start).num_seconds()` (~work_session.rs:317):
```rust
// Before:
let elapsed = (self.period_end - self.period_start).num_seconds();
// After:
let elapsed = self.period.duration().num_seconds();
```

- [ ] **Step 8.3: Migrate FocusMetrics call sites (4 sites enumerated in Step 8.1)**

For `focus_metrics.rs` callers using `date_to_period_range` (trusted construction):
```rust
// Before:
let (period_start, period_end) = date_to_period_range(date)?;
let metrics = FocusMetrics { period_start, period_end, /* ... */ };
// After:
let (period_start, period_end) = date_to_period_range(date)?;
let metrics = FocusMetrics::new(period_start, period_end)
    .expect("date_to_period_range produces valid start <= end");
```

For test fixtures (e.g., `tests.rs:76`, `grpc_dashboard_integration.rs:461`):
```rust
// Before:
FocusMetrics::new(updated.period_start, updated.period_end)
// After:
FocusMetrics::new(updated.period.start, updated.period.end).unwrap()
// or if updated is itself FocusMetrics:
FocusMetrics::new(updated.period.start, updated.period.end).unwrap()
```

Also update any direct field access like `metrics.period_start` → `metrics.period.start`. Search:
```bash
grep -rn "\.period_start\|\.period_end" crates/ src-tauri/
```

For each match, update to `.period.start` / `.period.end`.

- [ ] **Step 8.4: Update FocusMetrics → FocusMetricsDto mapper (NG8 verification)**

Open `crates/oneshim-web/src/services/focus_assembler.rs` (or similar mapper file). Verify the mapper reads `metrics.period.start` / `metrics.period.end` and writes them into `FocusMetricsDto.date` (or whichever DTO field exists). Per NG8: `FocusMetricsDto` (frontend-facing) does NOT contain `period_start`/`period_end` — only `date: String` + scalars. Verify zero frontend impact:

```bash
grep -rn "period_start\|period_end" crates/oneshim-web/frontend/
```
Expected: no matches.

- [ ] **Step 8.5: Migrate SessionMetrics (Phase 2 iter-1 I1 — preemptive consistency)**

Open `crates/oneshim-core/src/models/telemetry.rs`. Find `SessionMetrics`:
```rust
pub struct SessionMetrics {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    // ...
}
```

Change to:
```rust
use crate::types::TimeWindow;

pub struct SessionMetrics {
    pub period: TimeWindow,
    // ...
}
```

If `SessionMetrics::new` constructor exists: same Result-returning pattern as Step 8.2.

**Note**: per Phase 2 iter-1 I1, this struct may be dead code — `grep -rn "SessionMetrics" crates/oneshim-core/src/` should be the only reference. If callers exist, update them. If truly dead, leave a TODO for follow-up cleanup PR.

- [ ] **Step 8.6: Verify compile**

```bash
cargo check --workspace 2>&1 | tail -20
```

- [ ] **Step 8.7: Commit**

```bash
FILES=$(grep -rln "FocusMetrics {\|FocusMetrics::new\|\.period_start\|\.period_end" crates/ src-tauri/ | grep -v "/.git/")
git add $FILES \
        crates/oneshim-core/src/models/{work_session,telemetry}.rs
git commit -m "$(cat <<'EOF'
refactor(core): FocusMetrics + SessionMetrics use TimeWindow primitive (NG8 internal-only)

- FocusMetrics::new returns Result<Self, TimeWindowError> (TimeWindow validates bounds)
- 4 internal call sites updated (focus_metrics.rs, tests.rs, grpc_dashboard_integration.rs)
- Internal field access .period_start/.period_end → .period.start/.period.end
- FocusMetricsDto mapper unchanged (NG8 — frontend uses date: String, not period bounds)
- SessionMetrics migrated for consistency (Phase 2 iter-1 I1 — may be dead code)
EOF
)"
```

---

## Task 9: Workspace Sweep + Final Cleanup

**Estimate:** 1h | **Spec ref:** §5 cleanup + spec-to-impl coverage check | **Files:** any remaining api-contracts/storage/handler files

- [ ] **Step 9.1: Sweep for remaining range-pair patterns**

```bash
grep -rn "from: Option<String>\|to: Option<String>\|period_start.*DateTime\|period_end.*DateTime" crates/oneshim-api-contracts/src/ crates/oneshim-core/src/models/ | grep -v test
```

For each remaining occurrence, evaluate:
- If absolute timestamp window → migrate to TimeWindow
- If wall-clock recurrence (TrackingWindow, coaching TimeRange) → leave alone per NG2/NG-WALL/scope
- If IdlePeriod's open-ended Option<end_time> → leave alone per NG7

- [ ] **Step 9.2: Verify all tests still pass**

```bash
cargo test --workspace 2>&1 | tail -15
```

- [ ] **Step 9.3: Commit (only if changes made)**

```bash
git add crates/oneshim-api-contracts/src/ crates/oneshim-core/src/models/
git commit -m "refactor(workspace): sweep remaining absolute-timestamp range pairs to TimeWindow"
```

---

## Task 10: End-to-End Integration Tests

**Estimate:** 2h | **Spec ref:** §8.3 + Phase 2 iter-1 C3 | **Files:** new `crates/oneshim-web/tests/timewindow_integration.rs`

> **Phase 2 iter-1 C3**: `ApiError::IntoResponse` only emits `{ error, status }` — no `code` field. E2E tests must NOT assert `body["code"]`. Asserting status code + error message substring instead.

- [ ] **Step 10.1: Add E2E test for REST → handler → storage flow**

Create `crates/oneshim-web/tests/timewindow_integration.rs`:

```rust
//! E2E test verifying TimeWindow flows correctly through REST → handler → storage layer.
//! Per Phase 2 iter-1 C3: assertions limited to status code + error message substring
//! (response body has no `code` field — ErrorResponse schema is { error, status }).

use axum::body::Body;
use http::Request;
use serde_json::Value;
use tower::ServiceExt;

// Adapt to actual test_app() / seed_frames() helper conventions.

#[tokio::test]
async fn frames_endpoint_with_explicit_window_returns_correct_count() {
    let app = test_app().await;
    seed_frames(&[
        "2026-04-01T00:00:00Z",
        "2026-04-15T00:00:00Z",
        "2026-04-25T00:00:00Z",
        "2026-04-30T00:00:00Z",
    ]).await;

    let response = app
        .oneshot(
            Request::get("/api/frames?from=2026-04-01T00:00:00Z&to=2026-04-25T00:00:00Z")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let frames = body.as_array().unwrap();
    assert_eq!(frames.len(), 3, "closed-closed should include both boundaries");
}

#[tokio::test]
async fn delete_range_request_preserves_external_from_to_shape() {
    let app = test_app().await;
    let body = r#"{"from":"2026-04-01T00:00:00Z","to":"2026-04-25T00:00:00Z","data_types":["frames"]}"#;

    let response = app
        .oneshot(
            Request::post("/api/data/delete-range")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Frontend sends from/to keys (no period nesting); endpoint accepts.
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn invalid_time_window_returns_400() {
    let app = test_app().await;
    let response = app
        .oneshot(
            Request::get("/api/frames?from=2026-04-25T00:00:00Z&to=2026-04-01T00:00:00Z")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 400, "inverted bounds → 400 BadRequest");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    // ErrorResponse schema = { error, status } — NO code field per Phase 2 iter-1 C3
    let err_msg = body["error"].as_str().unwrap();
    assert!(
        err_msg.contains("must be <=") || err_msg.contains("start") || err_msg.to_lowercase().contains("inverted"),
        "error message should mention bound inversion; got: {err_msg}"
    );
    assert_eq!(body["status"], 400);
}

#[tokio::test]
async fn invalid_iso8601_timestamp_returns_400() {
    let app = test_app().await;
    let response = app
        .oneshot(
            Request::get("/api/frames?from=not-a-date&to=2026-04-25T00:00:00Z")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 400, "parse failure → 400 BadRequest");
}
```

- [ ] **Step 10.2: Run E2E**

```bash
cargo test -p oneshim-web --test timewindow_integration 2>&1 | tail -10
```
Expected: 4 tests pass.

- [ ] **Step 10.3: Commit**

```bash
git add crates/oneshim-web/tests/timewindow_integration.rs
git commit -m "test(integration): TimeWindow E2E — closed-closed boundary + 400 error mapping (no code body field per ApiError schema)"
```

---

## Task 11: Documentation + STATUS.md + PHASE-HISTORY

**Estimate:** 1h | **Spec ref:** §9.1 | **Files:** `docs/STATUS.md`, `docs/PHASE-HISTORY.md`

- [ ] **Step 11.1: Run full test suite (collect counts)**

```bash
cargo test --workspace 2>&1 | tail -5
```
Capture pass count delta vs `docs/STATUS.md` current value.

- [ ] **Step 11.2: Update STATUS.md**

Update version + Rust test count. Add note about TimeWindow refactor.

- [ ] **Step 11.3: Update PHASE-HISTORY.md**

Add new section after the latest Phase 9 entries:

```markdown
## TimeWindow Primitive Refactor (v0.4.42-rc.1, DATE_OF_MERGE)

- **Consolidated 5 main + 4 supporting divergent absolute-timestamp time-range types** into single canonical `TimeWindow { start: DateTime<Utc>, end: DateTime<Utc> }` primitive at `oneshim-core::types`
- **Closed-closed `[start, end]` semantic** (matches existing SQL BETWEEN, Stripe-style business API pattern; per spec U4)
- **Wall-clock recurrence types unmigrated**: TrackingWindow (PR-A), coaching TimeRange — different domain (recurrence vs absolute window)
- **IdlePeriod unmigrated** (NG7): ongoing idle requires `Option<end_time>` which TimeWindow can't represent without semantic drift
- **Migration scope**: TimeRangeQuery::to_time_window adapter + 6 REST handlers + 8 SQL port-trait methods + 4 caller sites + FocusMetrics + SessionMetrics + DeleteRangeRequest (period() accessor — Option C) + ReportQuery (flatten TimeRangeQuery)
- **2 new wire codes**: time_window.inverted_bounds + time_window.parse_failed (ADR-019 define_code_enum! macro)
- **Tests**: +13 TimeWindow unit + 3 TimeWindowCode + 8 TimeRangeQuery adapter + 3 SQL boundary regression + 4 E2E + 2 ApiError mapping (~33 new tests total)
- **External API contract preserved**: REST query strings unchanged; DeleteRangeRequest JSON shape preserved via accessor pattern (no custom serde required)
- Spec + plan: `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md` (v3) + `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (v2)
```

- [ ] **Step 11.4: Commit**

```bash
git add docs/STATUS.md docs/PHASE-HISTORY.md
git commit -m "docs(time-window): STATUS.md + PHASE-HISTORY entry for TimeWindow refactor"
```

---

## Post-Completion Checklist

> **Phase 2 iter-1 I6**: per Memory `feedback_lefthook_clippy_cost.md`, clippy on cold cache takes ~16min. **Run clippy ONCE at PC1, NOT per-task** — subagent-driven implementation should use `cargo check -p <crate>` for fast per-task feedback and reserve full clippy for PC1.

- [ ] **PC1: Full test suite + lint (single run, end of all tasks)**

```bash
cargo test --workspace 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -20
cargo fmt --check
cd crates/oneshim-web/frontend && pnpm lint 2>&1 | tail -5
```
All expected GREEN. If clippy flags `pub` fields on `TimeWindow` (per spec U2 deliberate choice for destructure-friendliness): consider `#[allow(...)]` on the struct only, NOT on the crate.

- [ ] **PC2: Wire snapshot + i18n CI**

```bash
cargo test -p oneshim-core --test wire_contract_snapshot
bash scripts/check-wire-error-i18n-coverage.sh
```

- [ ] **PC3: Open PR**

```bash
git push -u origin refactor/timewindow-primitive
gh pr create --title "refactor(core): consolidate divergent time-range types into TimeWindow primitive" \
  --body-file .github/TIMEWINDOW-PR-description.md
```

PR description should summarize:
- Why (9+ divergent absolute-timestamp types, observability + correctness wins)
- What (TimeWindow primitive + 8 port-trait migrations + 6 handler migrations + accessor patterns)
- How (closed-closed semantic, big-bang scope, NG7/NG8 carve-outs)
- Testing matrix (unit + adapter + boundary + E2E + ApiError mapping)
- Non-goals (TrackingWindow, IdlePeriod, frontend types)
- Pre-merge: PF3 baseline computed actual = N → post-merge = N+2

---

## Plan Self-Review (v2 update)

### 1. Spec coverage
- §5.1 TimeWindow + TimeWindowError → Task 1
- §5.2 to_time_window adapter → Task 3
- §5.3 SQL helpers (8 port methods) → Task 4
- §5.4 FocusMetrics (NG8 Option Z) → Task 8
- §5.5 REST handlers (6 actual users) → Task 6
- §5.6 DeleteRangeRequest accessor pattern → Task 7
- §6 wire codes → Task 1 + Task 2
- §7 error handling → Task 1 (CoreError + ApiError integration in single commit)
- §8 testing → Tasks 1, 3, 5, 10
- §9 commits 1-12 → Tasks 1-11 (Tasks 1+2 of v1 merged into Task 1 of v2)
- §10 migration backward compat → Task 7 accessor pattern
- §11 open questions → all RESOLVED in spec v3 (Q-8 deferred to PF3)
- §12 risks → addressed via test coverage + ABORT GUARD

### 2. Placeholder scan
- ✅ No "TBD" / "fill in details"
- ✅ All Rust code blocks have full implementation
- ⚠ Task 4 sub-tasks use `grep` to find additional helpers/callers — implementer must enumerate based on actual content

### 3. Type consistency
- `TimeWindow` field names (`start`, `end`) consistent across all tasks
- `TimeWindowError::InvertedBounds` + `ParseFailed` consistent
- `TimeWindowCode::InvertedBounds` + `ParseFailed` matches error variants
- `to_time_window` signature `(&self, default_lookback: Duration) -> Result<TimeWindow, TimeWindowError>` consistent across handlers
- `CoreError::TimeWindow { code, message }` struct-variant consistent with ADR-019 §4.6 majority pattern (per Phase 2 iter-1 C1)

### 4. Known gaps + risks
- PR-B1 dependency: hard ABORT GUARD in PF1 (Phase 3 cannot start until #508 merges)
- Q-8 baseline count: PF3 captures actual count at impl time (NOT trusted from spec)
- delete_data_in_range 7+ params: only `from`/`to` migrated; preserve all bool flags (Phase 2 iter-1 C7)
- Subagent-driven implementation may need to grep for additional callers if `cargo check` fails after Task 4D — expand inline

### 5. Phase 2 iter-1 findings disposition

| Severity | ID | Disposition |
|----------|-----|-------------|
| Critical | C1 — CoreError struct-variant | ✅ Task 1.8 uses `TimeWindow { code, message }` + manual `From` impl |
| Critical | C2 — ApiError arm missing | ✅ Task 1.9 adds explicit `BadRequest` arm + 2 regression tests |
| Critical | C3 — body["code"] assertion broken | ✅ Task 10.1 asserts `error` substring + `status` only |
| Critical | C4 — TimeRangeQuery::Default | ✅ Task 3.1 adds `Default` to derive list |
| Critical | C5 — hand-computed timestamps | ✅ Task 3.3 uses `Utc.with_ymd_and_hms` helper |
| Critical | C6 — port trait scope | ✅ Task 4 expands to 8 methods + 4 callers |
| Critical | C7 — maintenance.rs delete_data_in_range | ✅ Task 4B.4 + 4C.4 preserve 5 bool flags |
| Critical | C8 — i18n dual `42` assertions | ✅ Task 2.3 grep-finds + updates BOTH lines |
| Critical | C9 — DeleteRangeRequest serde | ✅ Task 7 uses Option C accessor pattern |
| Important | I1 — SessionMetrics dead code | ✅ Task 8.5 notes; migrate for consistency |
| Important | I2 — FocusMetrics 4 call sites | ✅ Task 8 enumerates all 4 + Result-returning constructor |
| Important | I3 — Task 7 6 vs 8 handlers | ✅ Task 6.0 verifies actual list (6) |
| Important | I4 — oneshim-core dep verification | ✅ PF5 grep verification |
| Important | I5 — bridge Tasks 2 & 7 | ✅ Tasks merged: ApiError mapping in Task 1.9 (same commit as core integration) |
| Important | I6 — clippy cost | ✅ Post-Completion guidance: single PC1 run |
| Important | I7 — wire snapshot alphabetical | ✅ PF3 + Task 1.10 show alphabetical block sample |
| Important | I8 — Copy threshold note | (nit, no plan change) |
| Important | I9 — Tasks 1+2 circular dep | ✅ Merged into single Task 1 |
| Important | I10 — commit message scope | ✅ Updated to `feat(core)`, `feat(api-contracts)`, `refactor(web-handlers)`, etc. |
| Important | I11 — ReportQuery flatten | ✅ Task 7.3 uses `#[serde(flatten)] time_range: TimeRangeQuery` |

---

## Execution Handoff

**Plan v2 complete and saved to** `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task + 2-stage review.

**2. Inline Execution** — executing-plans batch with checkpoints.

(For ralph-loop continuation: Phase 2 plan creation v2 complete addressing 9 Critical + 11 Important. Next iteration: fresh subagent verification of plan v2. Phase 3 implementation BLOCKED on PR-B1 #508 merge.)
