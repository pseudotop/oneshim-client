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

**Total estimate:** ~30h across 11 tasks (~4 working days). Revised again at v9 (was ~28h) to absorb iter-9 NEW-C1 service-layer migration (+2h for 7 service files + tests).

**⚠ ABORT GUARD**: PR-B1 (#508) MUST merge before Task 1 begins. PR-B1 modifies `oneshim-core/config/sections/` and `oneshim-core/src/error_codes/` — overlapping crate areas. Implementing TimeWindow before #508 merges will cause significant rebase conflicts.

**Plan version:** v10 (Phase 2 iter-10 cleanup — addresses 2 NEW Critical + 2 NEW Important from iter-9 verification:
- NEW-C1: default-window-size regression (existing helpers default 24h, plan v9 prescribed 7d/30d which is 7×/30× widening). v10 changes default to `Duration::hours(24)` everywhere to preserve existing behavior; future PR can deliberately change.
- NEW-C2: Service-layer migration must decompose `&window` for non-migrated storage methods (get_frames/get_events/get_metrics/etc. still take `DateTime<Utc>`, NOT `&TimeWindow`). Documented decomposition pattern.
- NEW-I1: Step 6.2-6.7 add explicit "continues Task 4D.3" framing per Step 7.4 model.
- NEW-I2: Step 11.3 PHASE-HISTORY adds two behavior-change bullets.). v9 NEW Critical fix — plan v1-v8 missed the SERVICE LAYER migration. Reality: handlers thin-delegate via `FramesQueryService::get_frames(&params)` etc.; services internally call `params.from_datetime()` / `params.to_datetime()` (silently swallow parse errors with hardcoded 24h fallback). Plan Task 6 example showed handler going direct to storage — architecturally wrong. v9 updates Task 6 to migrate at SERVICE layer (services internally convert via `to_time_window()?` for proper error propagation). 7 service files + tests added to scope. Old `from_datetime()` / `to_datetime()` helpers documented as legacy / non-validating). v8 cleanup — addresses iter-7 verifier's flagged "minor weakness" in Step 4C.5: missing locals-binding prescription for work_sessions.rs `get_daily_active_secs`. v8 adds PRESERVE-BODY diff PLUS surfaces a meaningful semantic finding: this query uses half-open `started_at < ?2` (NOT closed-closed like other helpers). Per NG6 the `<` operator must be preserved verbatim. v8 documents the half-open boundary so implementer doesn't accidentally "fix" it to `<=`). v7 — addresses iter-6 verification: 1 NEW Critical NEW-C1 — Step 4C.1 calibration_store_impl.rs had same class of synthetic-drift errors as Step 4C.4. v7 rewrites Step 4C.1 with PRESERVE-BODY pattern: actual table is `calibration_log` (not `calibration`), column is `is_noise` (not `noise`), uses fallible-lock + `CoreError::Storage { code, message }` mapping, get_entries + list_segment_time_ranges use async `with_conn` pattern with `from_str`/`to_str` String shadowing, list_segment has table_exists guard for V9 migration). v6 — addresses iter-5 verification: 2 NEW Important — stale `MockCalibration` references in Step 4D.0 inventory + Step 4E.1 commit body; Step 4C.4 SQL placeholder snippet had wrong table names (`metrics` → `system_metrics`), wrong idle column (`timestamp` → `start_time`), missing `system_metrics_hourly` companion DELETE. v6 rewrites Step 4C.4 as preserve-body-replace-parameter prescription rather than synthetic inline code.). v5 — addresses 6 pre-existing Important issues from iter-4 verification: mock names `NoopCalibrationReader`+`NoopCalibrationWriter` (was `MockCalibration`); `DeletedRangeCounts` field names `events_deleted`/`frames_deleted`/etc. (was `events`/`frames`); maintenance test callers use `.expect()` not `?`; stale Files-to-be-modified table corrected; non-functional grep helper replaced; variable name `all` not `dirty`). v4 — addresses Phase 2 iter-3 verification: 2 NEW Critical + 1 NEW Important regressions from v3 corrections; FailingStorage uses delegation pattern not unconditional Err; regime.rs callers in `()`-returning functions use `.expect()` not `?`; calibration_store_impl test callers added). v3 — addresses Phase 2 iter-2 verification findings: 6 NEW Critical + 5 NEW Important factual mismatches with actual source code, on top of v2's 9C+11I disposition). Key v3 changes: corrected actual port-trait return types (`flag_noise_range` is sync + `Result<u64>`, `list_segment_time_ranges` returns 3-tuple `(String, DateTime, DateTime)` with segment_id, `get_daily_active_secs` returns `Vec<(String, i64)>`); fixed regime.rs caller enumeration (lines 44+174+184 are get_entries+list_segment_time_ranges+get_entries, NOT flag_noise_range); enumerated all 10 FocusMetrics call sites including 3 in src-tauri/focus_analyzer; enumerated 14+ SQL helper caller sites in services/, tests/support/, internal sqlite/* tests; clarified inherent `pub fn` signature change in lockstep with port traits; removed duplicate Step 1.11 lib.rs registration; fixed `crate::common` same-crate import; added `serde_urlencoded` dev-dep step; corrected `TimeRangeQuery::limit/offset` to `Option<usize>`.

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

### Files to be modified — `src-tauri` caller sites (Phase 2 iter-1 C6 + iter-3 NEW-C2 + iter-5 cleanup)

| File | Caller site | Change |
|------|-------------|--------|
| `src-tauri/src/scheduler/analysis_pipeline/regime.rs:44` | `reader.get_entries(lookback, now, true)` | Build `TimeWindow::new(lookback, now).expect("lookback < now")` then pass `&window`. Enclosing `run_periodic_regime_detection` returns `()` — use `.expect()`, not `?`. |
| `src-tauri/src/scheduler/analysis_pipeline/regime.rs:174` | `calibration_reader.list_segment_time_ranges(lookback, now)` | Same `.expect()` pattern. Enclosing `run_constrained_clustering` returns `()`. |
| `src-tauri/src/scheduler/analysis_pipeline/regime.rs:184` | `calibration_reader.get_entries(lookback, now, true)` (re-fetch — NOT `flag_noise_range`) | Reuse `window` from line 174 (same scope). |
| `src-tauri/src/scheduler/analysis_pipeline/regime.rs:194` | Destructure `(seg_id, seg_start, seg_end)` | Adapt to new `Vec<(String, TimeWindow)>` return: destructure `(seg_id, seg_window)` + use `seg_window.contains(e.timestamp)`. |
| `src-tauri/src/scheduler/analysis_pipeline/tests.rs:12-22 + 24-31` | `NoopCalibrationWriter` (sync) + `NoopCalibrationReader` (async) | Update separate impls per Step 4D.2 — NOT a single `MockCalibration`. |

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

- [ ] **Step 1.11: Verify compile** *(was Step 1.12 — Step 1.11 removed per Phase 2 iter-2 N-I1: duplicated Step 1.3 lib.rs registration)*

```bash
cargo check -p oneshim-core 2>&1 | tail -20
cargo check -p oneshim-web 2>&1 | tail -10
```
Both expected: clean.

- [ ] **Step 1.12: Run all new tests**

```bash
cargo test -p oneshim-core --lib types::time_window::tests 2>&1 | tail -20
cargo test -p oneshim-core --lib error_codes::time_window::tests 2>&1 | tail -10
cargo test -p oneshim-core --test wire_contract_snapshot 2>&1 | tail -10
cargo test -p oneshim-web --lib error::tests 2>&1 | tail -10
```
All expected GREEN — 13 TimeWindow tests + 3 TimeWindowCode tests + wire snapshot pass + 2 ApiError mapping tests.

- [ ] **Step 1.13: Commit**

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

- [ ] **Step 3.1: Add `Default` derive to TimeRangeQuery (Phase 2 iter-1 C4 + iter-2 N-I4)**

Open `crates/oneshim-api-contracts/src/common.rs`. Find the existing `TimeRangeQuery` struct definition (verified at `common.rs:4-10`):

```rust
#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<usize>,        // ← usize, NOT u32
    pub offset: Option<usize>,       // ← usize, NOT u32
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

> **Phase 2 iter-2 N-C5 + N-C4 corrections**: `flag_noise_range` is **synchronous** + returns `Result<u64, CoreError>` (rows updated count). `list_segment_time_ranges` returns **3-tuple** `Vec<(String, DateTime<Utc>, DateTime<Utc>)>` where String is segment_id — caller (regime.rs:194) destructures as `(seg_id, seg_start, seg_end)` and uses `seg_id` for HashMap keys. Cannot drop segment_id.

- [ ] **Step 4A.1: Add TimeWindow import to port file**

Open `crates/oneshim-core/src/ports/calibration_store.rs`. At top:
```rust
use crate::types::TimeWindow;
```

- [ ] **Step 4A.2: Update CalibrationWriter::flag_noise_range trait sig (sync, Result<u64>)**

```rust
// Actual current sig (verified at calibration_store.rs:24):
fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<u64, CoreError>;
// After:
fn flag_noise_range(&self, window: &TimeWindow) -> Result<u64, CoreError>;
```

**Note**: `CalibrationWriter` is **synchronous** (NOT `#[async_trait]`). Do NOT add `async`. Return is `Result<u64, ...>` (rows-updated count), NOT `Result<()>`.

- [ ] **Step 4A.3: Update CalibrationReader::get_entries trait sig**

```rust
// Actual current sig (verified at calibration_store.rs:35-40):
async fn get_entries(&self, from: DateTime<Utc>, to: DateTime<Utc>, exclude_noise: bool) -> Result<Vec<CalibrationEntry>, CoreError>;
// After:
async fn get_entries(&self, window: &TimeWindow, exclude_noise: bool) -> Result<Vec<CalibrationEntry>, CoreError>;
```

`CalibrationReader` IS `#[async_trait]` — keep `async`.

- [ ] **Step 4A.4: Update CalibrationReader::list_segment_time_ranges trait sig (preserve segment_id)**

```rust
// Actual current sig (verified at calibration_store.rs:50-55):
async fn list_segment_time_ranges(
    &self,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<(String, DateTime<Utc>, DateTime<Utc>)>, CoreError> {
    let _ = (from, to);
    Ok(vec![])  // default impl
}

// After:
async fn list_segment_time_ranges(
    &self,
    window: &TimeWindow,
) -> Result<Vec<(String, TimeWindow)>, CoreError> {
    let _ = window;
    Ok(vec![])
}
```

The 3-tuple `(String, DateTime, DateTime)` becomes 2-tuple `(String, TimeWindow)` — the String is the segment_id and MUST be preserved. The 2 datetimes consolidate into `TimeWindow`. Caller at `regime.rs:194` destructures as `(seg_id, segment_window)` and accesses `segment_window.start`/`segment_window.end` for `e.timestamp` comparison.

### Sub-task 4B: Update web_storage.rs port trait (5 methods)

> **Phase 2 iter-2 N-C6 + N-C7 corrections**: `delete_data_in_range` has **5 boolean flags** (`delete_events`, `delete_frames`, `delete_metrics`, `delete_processes`, `delete_idle`) + `#[allow(clippy::too_many_arguments)]` annotation. Returns `DeletedRangeCounts`, not `DeleteSummary`. `get_daily_active_secs` returns `Vec<(String, i64)>` (date → active seconds tuples), NOT `u64`.

- [ ] **Step 4B.1: Add TimeWindow import to port file**

Open `crates/oneshim-core/src/ports/web_storage.rs`. At top:
```rust
use crate::types::TimeWindow;
```

- [ ] **Step 4B.2: Update FrameQueryStorage trait (2 methods)**

Verified actual sigs at `web_storage.rs:66, 74`:
```rust
// Before:
fn count_frames_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError>;
fn list_frame_file_paths_in_range(
    &self,
    from: &str,
    to: &str,
) -> Result<Vec<String>, CoreError>;

// After:
fn count_frames_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError>;
fn list_frame_file_paths_in_range(
    &self,
    window: &TimeWindow,
) -> Result<Vec<String>, CoreError>;
```

- [ ] **Step 4B.3: Update EventQueryStorage::count_events_in_range trait sig**

Verified actual sig at `web_storage.rs:97`:
```rust
// Before:
fn count_events_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError>;
// After:
fn count_events_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError>;
```

- [ ] **Step 4B.4: Update StorageMaintenanceStorage::delete_data_in_range trait sig (Phase 2 iter-1 C7 + iter-2 N-C7)**

Verified actual sig at `web_storage.rs:116-126`:
```rust
// Before:
#[allow(clippy::too_many_arguments)]
fn delete_data_in_range(
    &self,
    from: &str,
    to: &str,
    delete_events: bool,
    delete_frames: bool,
    delete_metrics: bool,
    delete_processes: bool,
    delete_idle: bool,
) -> Result<DeletedRangeCounts, CoreError>;

// After:
#[allow(clippy::too_many_arguments)]
fn delete_data_in_range(
    &self,
    window: &TimeWindow,
    delete_events: bool,
    delete_frames: bool,
    delete_metrics: bool,
    delete_processes: bool,
    delete_idle: bool,
) -> Result<DeletedRangeCounts, CoreError>;
```

Preserve `#[allow(clippy::too_many_arguments)]` annotation + ALL 5 boolean flags. Return type is `DeletedRangeCounts` (NOT `DeleteSummary`).

- [ ] **Step 4B.5: Update ActivityStatsStorage::get_daily_active_secs trait sig (returns Vec<(String, i64)>)**

Verified actual sig at `web_storage.rs:141`:
```rust
// Before:
fn get_daily_active_secs(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>, CoreError>;
// After:
fn get_daily_active_secs(&self, window: &TimeWindow) -> Result<Vec<(String, i64)>, CoreError>;
```

Return is `Vec<(String, i64)>` (date string → active seconds tuples for daily aggregation), NOT `u64`. Caller `crates/oneshim-web/src/services/stats_query_support.rs:112` consumes the Vec for daily breakdowns.

### Sub-task 4C: Update SQLite impls + inherent `pub fn` + thin wrappers

> **Phase 2 iter-2 N-C3 — Inherent fn lockstep decision**: `SqliteStorage` exposes inherent `pub fn` methods (verified at events.rs:14, frames.rs:10, maintenance.rs:253+286, work_sessions.rs:216) that are duplicated in `WebStorage` trait wrappers (web_storage_impl.rs delegates `SqliteStorage::method(self, from, to)`). **Decision: change inherent `pub fn` signatures TOO** so wrappers don't need impedance conversion. Internal test sites that call inherent methods (events.rs:406+426+452+471, frames.rs:175+192, maintenance.rs:931+1019+1052+1067+1083+1164+1183+1308+1358) must be updated in lockstep.

- [ ] **Step 4C.1: Migrate calibration_store_impl.rs (3 methods — Phase 2 iter-6 NEW-C1 PRESERVE-BODY rewrite)**

Open `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs`. **PRESERVE-BODY pattern** (mirror Step 4C.4): do NOT rewrite synthetic snippets. Each method has substantial production logic — fallible-lock CoreError mapping, async `with_conn` wrappers, table-existence guards, per-row parse error wrapping. Keep all of it bit-identical and change only the parameter signatures + add a single locals-binding line.

**Method 1: `flag_noise_range` (sync, lines 120-139)**

Actual current body (verified from source):
```rust
fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<u64, CoreError> {
    let conn = self.conn.lock().map_err(|e| CoreError::Storage {
        code: oneshim_core::error_codes::StorageCode::Failed,
        message: format!("SQLite lock poisoned: {e}"),
    })?;

    let updated = conn
        .execute(
            "UPDATE calibration_log SET is_noise = 1
             WHERE timestamp >= ?1 AND timestamp <= ?2",
            params![from.to_rfc3339(), to.to_rfc3339()],
        )
        .map_err(|e| CoreError::Storage {
            code: oneshim_core::error_codes::StorageCode::Failed,
            message: format!("flag noise range: {e}"),
        })?;

    debug!("flagged {} calibration entries as noise", updated);
    Ok(updated as u64)
}
```

Diff:
```rust
- fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<u64, CoreError> {
+ fn flag_noise_range(&self, window: &TimeWindow) -> Result<u64, CoreError> {
+     let from = window.start;
+     let to = window.end;
      // ... entire body unchanged: lock + execute on calibration_log SET is_noise = 1 + debug! + Ok(updated as u64)
  }
```

The `from.to_rfc3339()` calls inside `params!` continue to work because `from`/`to` are now `DateTime<Utc>` from the window's destructured fields (same type as before).

**Method 2: `get_entries` (async, lines 148-194)**

Actual body uses `self.with_conn(move |conn| { ... }).await` async closure pattern with separate `from_str`/`to_str` String locals. SQL queries `calibration_log` (not `calibration`) with conditional `is_noise = 0` filter. Uses `map_calibration_row` helper.

Diff:
```rust
- async fn get_entries(
-     &self,
-     from: DateTime<Utc>,
-     to: DateTime<Utc>,
-     exclude_noise: bool,
- ) -> Result<Vec<CalibrationEntry>, CoreError> {
-     let from_str = from.to_rfc3339();
-     let to_str = to.to_rfc3339();
+ async fn get_entries(
+     &self,
+     window: &TimeWindow,
+     exclude_noise: bool,
+ ) -> Result<Vec<CalibrationEntry>, CoreError> {
+     let from_str = window.start.to_rfc3339();
+     let to_str = window.end.to_rfc3339();
      // ... rest of body unchanged: self.with_conn(move |conn| { let sql = if exclude_noise { ... } else { ... }; let mut stmt = conn.prepare(sql)...; let rows = stmt.query_map(params![from_str, to_str], map_calibration_row)...; let mut entries = Vec::new(); for row_result in rows { ... entries.push(entry); } Ok(entries) }).await.map_err(Into::into)
  }
```

**Method 3: `list_segment_time_ranges` (async, lines 237-292) — return type change too**

Actual body has `table_exists` early-return guard (V9 migration check), per-row `parse_from_rfc3339` with separate error wrapping per field. Returns `Vec<(String, DateTime<Utc>, DateTime<Utc>)>` 3-tuple. Per Phase 2 iter-2 N-C4: change to `Vec<(String, TimeWindow)>` to consolidate the two datetimes — preserves segment_id String.

**⚠ Containment semantic (preserve)**: The query at line 262 uses `WHERE start_time >= ?1 AND end_time <= ?2` — DIFFERENT columns on each side. This is a "fully contained" semantic: returns segments whose entire `[start_time, end_time]` falls within the requested `[from, to]` window. Migration must preserve this — do NOT change to `start_time >= ?1 AND start_time <= ?2` (which would be "starts within window" semantic). The `from_str`/`to_str` String shadowing keeps `params![from_str, to_str]` working unchanged.

Diff:
```rust
- async fn list_segment_time_ranges(
-     &self,
-     from: DateTime<Utc>,
-     to: DateTime<Utc>,
- ) -> Result<Vec<(String, DateTime<Utc>, DateTime<Utc>)>, CoreError> {
-     let from_str = from.to_rfc3339();
-     let to_str = to.to_rfc3339();
+ async fn list_segment_time_ranges(
+     &self,
+     window: &TimeWindow,
+ ) -> Result<Vec<(String, TimeWindow)>, CoreError> {
+     let from_str = window.start.to_rfc3339();
+     let to_str = window.end.to_rfc3339();

      self.with_conn(move |conn| {
          // Check table existence (may not have run V9 migration yet) — UNCHANGED
          let table_exists: bool = conn.query_row(/* ... */, [], |row| row.get(0)).unwrap_or(false);
          if !table_exists { return Ok(vec![]); }

          let mut stmt = conn.prepare(/* SELECT id, start_time, end_time FROM activity_segments ... */)?;
          let rows = stmt.query_map(params![from_str, to_str], |row| { /* (id, start_str, end_str) */ })?;

          let mut result = Vec::new();
          for row_result in rows {
              let (id, start_str, end_str) = row_result.map_err(...)?;
              let start = DateTime::parse_from_rfc3339(&start_str).map(|dt| dt.with_timezone(&Utc)).map_err(...)?;
              let end   = DateTime::parse_from_rfc3339(&end_str).map(|dt| dt.with_timezone(&Utc)).map_err(...)?;
-             result.push((id, start, end));
+             // Phase 2 iter-2 N-C4: consolidate two DateTime<Utc> into TimeWindow.
+             // Trusted construction (DB-stored values satisfy start <= end by invariant).
+             let segment_window = TimeWindow::new(start, end)
+                 .expect("DB-stored segment ranges are trusted (start <= end invariant)");
+             result.push((id, segment_window));
          }
          Ok(result)
      }).await.map_err(Into::into)
  }
```

The `table_exists` guard, parse-error mapping, and async `with_conn` pattern all stay bit-identical. Only the return-type tuple is consolidated.

Add at top of file:
```rust
use oneshim_core::types::TimeWindow;
```

- [ ] **Step 4C.1.5: Migrate calibration_store_impl.rs internal test sites (Phase 2 iter-3 NEW-I1 — 5 sites)**

In the same file `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs`, find the `#[cfg(test)] mod tests` block. Update 5 caller sites:

```rust
use oneshim_core::types::TimeWindow;

// Line 400: storage.get_entries(from, to, false).await
// Before:
let loaded = storage.get_entries(from, to, false).await.unwrap();
// After:
let window = TimeWindow::new(from, to).expect("trusted test bounds");
let loaded = storage.get_entries(&window, false).await.unwrap();

// Line 414: storage.flag_noise_range(from, to)  (sync, returns u64)
// Before:
let flagged = storage.flag_noise_range(from, to).unwrap();
// After:
let window = TimeWindow::new(from, to).expect("trusted test bounds");
let flagged = storage.flag_noise_range(&window).unwrap();

// Line 420: storage.get_entries(wide_from, wide_to, true).await
// Before:
let clean = storage.get_entries(wide_from, wide_to, true).await.unwrap();
// After:
let wide_window = TimeWindow::new(wide_from, wide_to).expect("trusted test bounds");
let clean = storage.get_entries(&wide_window, true).await.unwrap();

// Line 425: .get_entries(wide_from, wide_to, false) — actual variable name is `all`, NOT `dirty`
// Before (from line 424-426 chain):
let all = storage
    .get_entries(wide_from, wide_to, false)
    .await
    .unwrap();
// After (reuse wide_window from above):
let all = storage
    .get_entries(&wide_window, false)
    .await
    .unwrap();

// Line 443: storage.get_entries(from, to, false).await
// Before:
let remaining = storage.get_entries(from, to, false).await.unwrap();
// After:
let window = TimeWindow::new(from, to).expect("trusted test bounds");
let remaining = storage.get_entries(&window, false).await.unwrap();
```

- [ ] **Step 4C.2: Migrate events.rs inherent fn + 4 internal test sites**

Open `crates/oneshim-storage/src/sqlite/events.rs`. Update inherent fn signature:

```rust
// Before (line 14):
pub fn count_events_in_range(&self, from: &str, to: &str) -> Result<u64, StorageError> {
    // ... query body ...
}
// After:
pub fn count_events_in_range(&self, window: &TimeWindow) -> Result<u64, StorageError> {
    let (from, to) = window.to_sql_pair();
    // ... query body unchanged (still uses &from, &to as locals) ...
}
```

Also update 4 internal test sites at lines 406, 426, 452, 471 — each currently calls `.count_events_in_range(&from, &to)` where `from`/`to` are RFC3339 String. Pattern:
```rust
// Before:
let count = storage.count_events_in_range(&from, &to).expect("count_events_in_range failed");
// After:
let window = TimeWindow::from_rfc3339_pair(&from, &to).expect("test ts trusted");
let count = storage.count_events_in_range(&window).expect("count_events_in_range failed");
```

- [ ] **Step 4C.3: Migrate frames.rs inherent fn + 2 internal test sites**

Same pattern as 4C.2 for `count_frames_in_range` (inherent at line 10, callers at 175 + 192).

- [ ] **Step 4C.4: Migrate maintenance.rs inherent fns + ~9 internal test sites (Phase 2 iter-1 C7 + iter-2 N-C7)**

Open `crates/oneshim-storage/src/sqlite/maintenance.rs`. Two inherent fns:

```rust
// Before (line 253):
pub fn list_frame_file_paths_in_range(&self, from: &str, to: &str) -> Result<Vec<String>, StorageError>
// After:
pub fn list_frame_file_paths_in_range(&self, window: &TimeWindow) -> Result<Vec<String>, StorageError>

// Before (line 286 — preserve all 5 bool flags + #[allow]):
#[allow(clippy::too_many_arguments)]
pub fn delete_data_in_range(
    &self,
    from: &str,
    to: &str,
    delete_events: bool,
    delete_frames: bool,
    delete_metrics: bool,
    delete_processes: bool,
    delete_idle: bool,
) -> Result<DeletedRangeCounts, StorageError>;

// After (PRESERVE-BODY pattern — Phase 2 iter-6 NEW-I2 fix):
//
// Do NOT rewrite the function body from scratch. The existing body (lines 285-360
// at plan-write time) handles 5 different tables (events, frames, system_metrics
// + companion system_metrics_hourly, process_snapshots, idle_periods) with their
// own column names ("timestamp" for most, "start_time" for idle_periods, "hour"
// for system_metrics_hourly), and uses fallible lock + .map_err(...)? error
// wrapping that produces specific error messages.
//
// Minimal change: replace ONLY the parameter declaration `from: &str, to: &str`
// with `window: &TimeWindow`. Inside the function, add a single line at the top:
//     let (from, to) = window.to_sql_pair();
// This binds `from` and `to` as local `String` vars with the same SHADOW as the
// pre-refactor parameter names — every existing `rusqlite::params![from, to]`
// invocation continues to work unchanged.
//
// Diff (conceptual):
//
//     #[allow(clippy::too_many_arguments)]
//     pub fn delete_data_in_range(
//         &self,
// -       from: &str,
// -       to: &str,
// +       window: &TimeWindow,
//         delete_events: bool,
//         delete_frames: bool,
//         delete_metrics: bool,
//         delete_processes: bool,
//         delete_idle: bool,
//     ) -> Result<DeletedRangeCounts, StorageError> {
// +       let (from, to) = window.to_sql_pair();
//         let conn = self.conn.lock()
//             .map_err(|e| StorageError::Internal(format!("Failed to acquire lock: {e}")))?;
//         let mut counts = DeletedRangeCounts::default();
//         if delete_events { /* unchanged body — uses params![from, to] */ }
//         if delete_frames { /* unchanged */ }
//         if delete_metrics { /* unchanged — TWO executes: system_metrics + system_metrics_hourly */ }
//         if delete_processes { /* unchanged */ }
//         if delete_idle { /* unchanged — uses start_time column, NOT timestamp */ }
//         Ok(counts)
//     }
//
// The internal SQL strings, column names, lock-error wrapping, and per-execute
// .map_err formatting all remain bit-identical to the existing implementation.
```

Note: actual `DeletedRangeCounts` field names per `crates/oneshim-core/src/models/storage_records.rs:88-94`:
- `events_deleted: u64`
- `frames_deleted: u64`
- `metrics_deleted: u64`
- `process_snapshots_deleted: u64`
- `idle_periods_deleted: u64`

Return type is `DeletedRangeCounts` (NOT `DeleteSummary`).

Internal test sites in maintenance.rs (~9 sites): lines 931, 1019, 1052, 1067, 1083, 1164, 1183, 1308, 1358. **All these are `#[test] fn name()` returning `()`** — `?` operator does NOT compile. Use `.expect("trusted test bounds")` pattern (mirror Step 4C.2 events.rs example):

```rust
// Pattern (apply at each maintenance.rs internal test caller):
// Before:
let n = storage.delete_data_in_range("2025-01-01T00:00:00Z", "2025-12-31T23:59:59Z", ...).unwrap();
// After:
let window = TimeWindow::from_rfc3339_pair("2025-01-01T00:00:00Z", "2025-12-31T23:59:59Z")
    .expect("trusted test bounds");
let n = storage.delete_data_in_range(&window, ...).unwrap();
```

Same pattern for `count_events_in_range` callers in maintenance test sites (1067, 1164, 1183, 1308) and `list_frame_file_paths_in_range` (1358).

- [ ] **Step 4C.5: Migrate work_sessions.rs inherent get_daily_active_secs (PRESERVE-BODY pattern)**

Open `crates/oneshim-storage/src/sqlite/edge_intelligence/work_sessions.rs`. Inherent fn at line 216.

**⚠ Half-open boundary preserved per NG6**: This query uses `started_at >= ?1 AND started_at < ?2` (half-open `[from, to)` upper bound), NOT closed-closed like the other range helpers. This is intentional — work_sessions started_at represents an instant, and excluding the end-of-day-T+1 boundary prevents double-counting at day rollovers. Migration must preserve `<`, NOT change to `<=`. Spec NG6 explicitly says "SQL BETWEEN preserved" — same principle applies to this query's existing operators.

**PRESERVE-BODY diff** — only swap signature + add locals binding:

```rust
- pub fn get_daily_active_secs(
-     &self,
-     from: &str,
-     to: &str,
- ) -> Result<Vec<(String, i64)>, StorageError> {
+ pub fn get_daily_active_secs(
+     &self,
+     window: &TimeWindow,
+ ) -> Result<Vec<(String, i64)>, StorageError> {
+     let (from, to) = window.to_sql_pair();
      let conn = self.conn.lock().map_err(|e| StorageError::Internal(format!("Failed to acquire lock: {e}")))?;
      let mut stmt = conn.prepare(
          "SELECT DATE(started_at) as day, SUM(duration_secs) as total_secs
           FROM work_sessions
           WHERE state = 'completed'
             AND started_at >= ?1 AND started_at < ?2     -- ← HALF-OPEN preserved per NG6
           GROUP BY day
           ORDER BY day",
      ).map_err(|e| StorageError::Internal(format!("Failed to prepare SQL: {e}")))?;
      let rows = stmt.query_map(rusqlite::params![from, to], |row| {
          Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
      }).map_err(|e| StorageError::Internal(format!("Query failed: {e}")))?;
      let result: Vec<_> = rows.flatten().collect();
      Ok(result)
  }
```

The `let (from, to) = window.to_sql_pair();` shadows the previous parameter names — the `params![from, to]` binding continues to work unchanged. SQL string preserved verbatim including the `< ?2` half-open operator. Lock-error wrapping + query_map closure unchanged.

- [ ] **Step 4C.6: Migrate web_storage_impl.rs thin wrappers (5 wrappers)**

Open `crates/oneshim-storage/src/sqlite/web_storage_impl.rs`. Update 5 wrappers (verified locations: lines 82, 105, 126, 169, 246):

```rust
use oneshim_core::types::TimeWindow;

// Line 82:
fn count_events_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError> {
    SqliteStorage::count_events_in_range(self, window).map_err(Into::into)
}

// Line 105:
fn count_frames_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError> {
    SqliteStorage::count_frames_in_range(self, window).map_err(Into::into)
}

// Line 126:
fn list_frame_file_paths_in_range(&self, window: &TimeWindow) -> Result<Vec<String>, CoreError> {
    SqliteStorage::list_frame_file_paths_in_range(self, window).map_err(Into::into)
}

// Line 169 — delete_data_in_range with 5 bool flags preserved:
#[allow(clippy::too_many_arguments)]
fn delete_data_in_range(
    &self,
    window: &TimeWindow,
    delete_events: bool,
    delete_frames: bool,
    delete_metrics: bool,
    delete_processes: bool,
    delete_idle: bool,
) -> Result<DeletedRangeCounts, CoreError> {
    SqliteStorage::delete_data_in_range(
        self,
        window,
        delete_events,
        delete_frames,
        delete_metrics,
        delete_processes,
        delete_idle,
    ).map_err(Into::into)
}

// Line 246:
fn get_daily_active_secs(&self, window: &TimeWindow) -> Result<Vec<(String, i64)>, CoreError> {
    SqliteStorage::get_daily_active_secs(self, window).map_err(Into::into)
}
```

- [ ] **Step 4C.7: Verify compile (lockstep check across oneshim-core + oneshim-storage)**

```bash
cargo check -p oneshim-core 2>&1 | tail -10
cargo check -p oneshim-storage 2>&1 | tail -20
```
Both expected: clean. If `oneshim-storage` errors with "method signature mismatch", a port-trait sig diverged from impl — fix lockstep before proceeding.

### Sub-task 4D: Update all caller sites (Phase 2 iter-1 C6 + iter-2 N-C1/N-C2/N-C3)

> **Phase 2 iter-2 N-C2 correction**: regime.rs:184 is a SECOND `get_entries` call (re-fetch for index mapping), NOT `flag_noise_range`. There are **0 `flag_noise_range` callers in regime.rs** — only test fixtures in `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:414` use it.

> **Phase 2 iter-2 N-C3 correction**: 9+ additional caller sites missed in v2 (services/, tests/support/failing_storage.rs). Step 4D.0 below enumerates them all.

- [ ] **Step 4D.0: Comprehensive caller enumeration (run before any caller migration)**

```bash
grep -rn "count_events_in_range\|count_frames_in_range\|list_frame_file_paths_in_range\|delete_data_in_range\|get_daily_active_secs\|flag_noise_range\|\.get_entries(\|list_segment_time_ranges" crates/ src-tauri/ 2>/dev/null | grep -v "/.git/" | grep -v frontend | grep -v "fn "
```

Expected ~30 hits across these categories (verified at plan-write time):

| Category | Files | Count |
|----------|-------|-------|
| Service layer | `crates/oneshim-web/src/services/{stats_query_support,data_web_service,events_service,reports_query_support}.rs` | 5 |
| Test mock support | `crates/oneshim-web/tests/support/failing_storage.rs` | 5 |
| Internal SQLite tests (events.rs) | lines 406, 426, 452, 471 | 4 |
| Internal SQLite tests (frames.rs) | lines 175, 192 | 2 |
| Internal SQLite tests (maintenance.rs) | lines 931, 1019, 1052, 1067, 1083, 1164, 1183, 1308, 1358 | ~9 |
| Internal SQLite tests (calibration_store_impl.rs) — Phase 2 iter-3 NEW-I1 | lines 400, 414, 420, 425, 443 (storage.get_entries × 4 + storage.flag_noise_range × 1) | 5 |
| web_storage_impl.rs wrappers | lines 82, 105, 126, 169, 246 | 5 (covered by Step 4C.6) |
| src-tauri regime.rs | lines 44, 174, 184 | 3 |
| src-tauri tests.rs Noop mocks (Phase 2 iter-5/6 corrected) | `NoopCalibrationWriter` lines 12-22 + `NoopCalibrationReader` lines 24-31 | 2 (no `MockCalibration` — separate sync/async impls) |

If grep finds MORE than these, expand scope inline.

- [ ] **Step 4D.1: Update regime.rs callers (3 sites — N-C2 corrected, NEW-C2 `()` return fix)**

Open `src-tauri/src/scheduler/analysis_pipeline/regime.rs`. Add at top:
```rust
use oneshim_core::types::TimeWindow;
```

**NEW-C2 (Phase 2 iter-3) fix**: Both enclosing functions return `()` (no Result):
- `run_periodic_regime_detection(ts, now)` — `()` return at line 16
- `run_constrained_clustering(ts, features, now)` — `()` return at line 140

The `?` operator on `TimeWindow::new(...)?` won't compile. Use `.expect("...")` since `lookback`/`now` are trusted (lookback = `now - 7d` always satisfies `start <= end`).

Update each caller:

```rust
// Line 44: get_entries call (first usage, in run_periodic_regime_detection — () return)
// Before:
match reader.get_entries(lookback, now, true).await {
// After:
let window = TimeWindow::new(lookback, now)
    .expect("lookback (now - 7d) is always before now");
match reader.get_entries(&window, true).await {
```

```rust
// Line 174: list_segment_time_ranges (in run_constrained_clustering — () return)
// Before:
let segment_ranges = match ts.calibration_reader.list_segment_time_ranges(lookback, now).await {
// After:
let window = TimeWindow::new(lookback, now)
    .expect("lookback (now - 7d) is always before now");
let segment_ranges = match ts.calibration_reader.list_segment_time_ranges(&window).await {
```

```rust
// Line 184: SECOND get_entries call (re-fetch for index mapping; same function as line 174)
// Before:
let entries_with_ts = match ts.calibration_reader.get_entries(lookback, now, true).await {
// After:
// Reuse window constructed at line 174 (same scope — both in run_constrained_clustering)
let entries_with_ts = match ts.calibration_reader.get_entries(&window, true).await {
```

Verify line 44 (`run_periodic_regime_detection`) and lines 174+184 (`run_constrained_clustering`) are in DIFFERENT functions — lines 174+184 share `window` scope, line 44 needs its own `window` binding.

Also update the destructuring at line ~194 to match the new `Vec<(String, TimeWindow)>` return:
```rust
// Before:
.filter_map(|(seg_id, seg_start, seg_end)| {
    entries_with_ts
        .iter()
        .position(|e| e.timestamp >= *seg_start && e.timestamp <= *seg_end)
        .map(|idx| (seg_id.clone(), idx))
})
// After:
.filter_map(|(seg_id, seg_window)| {
    entries_with_ts
        .iter()
        .position(|e| seg_window.contains(e.timestamp))
        .map(|idx| (seg_id.clone(), idx))
})
```

Note: `TimeWindow::contains(instant)` is the closed-closed boundary check — replaces explicit `>=` + `<=` per spec §5.1.

- [ ] **Step 4D.2: Update tests.rs NoopCalibration mocks (Phase 2 iter-4 Pre-existing #1 fix)**

Open `src-tauri/src/scheduler/analysis_pipeline/tests.rs`. Actual mock types are **two separate structs**: `NoopCalibrationWriter` (sync, lines 12-22) and `NoopCalibrationReader` (async, lines 24-31) — NOT a single `MockCalibration`. The Reader does NOT explicitly implement `list_segment_time_ranges` (uses trait default impl).

Update method signatures:

```rust
use oneshim_core::types::TimeWindow;

// CalibrationWriter is SYNC (no #[async_trait])
impl CalibrationWriter for NoopCalibrationWriter {
    fn log_batch(&self, _entries: &[CalibrationEntry]) -> Result<(), CoreError> {
        Ok(())
    }
    fn flag_noise_range(&self, _window: &TimeWindow) -> Result<u64, CoreError> {
        Ok(0)
    }
}

#[async_trait::async_trait]
impl CalibrationReader for NoopCalibrationReader {
    async fn get_entries(
        &self,
        _window: &TimeWindow,
        _exclude_noise: bool,
    ) -> Result<Vec<CalibrationEntry>, CoreError> {
        Ok(vec![])
    }
    async fn enforce_retention(&self, _max_days: u32, _max_rows: u64) -> Result<u64, CoreError> {
        Ok(0)
    }
    // Note: list_segment_time_ranges NOT explicitly impl'd here — uses trait default
    // at calibration_store.rs:48-58 which now returns Ok(vec![]) with the new
    // Vec<(String, TimeWindow)> signature. No change needed in this file for that method.
}
```

- [ ] **Step 4D.3: Update service layer callers (5 sites — Phase 2 iter-2 N-C3)**

Update each:

```rust
// crates/oneshim-web/src/services/stats_query_support.rs:112
// Function signature: total_active_secs_for_range(...) -> u64 — NO Result return
// (NEW-C2 fix: cannot use ? operator)
// Before:
let from_rfc = from.to_rfc3339();
let to_rfc = to.to_rfc3339();
match ctx.storage.get_daily_active_secs(&from_rfc, &to_rfc) {
    Ok(daily) if !daily.is_empty() => daily.iter().map(|(_, seconds)| *seconds as u64).sum(),
    _ => fallback_events_logged * 5,
}
// After:
let Ok(window) = TimeWindow::new(from, to) else {
    return fallback_events_logged * 5;
};
match ctx.storage.get_daily_active_secs(&window) {
    Ok(daily) if !daily.is_empty() => daily.iter().map(|(_, seconds)| *seconds as u64).sum(),
    _ => fallback_events_logged * 5,
}
```

```rust
// crates/oneshim-web/src/services/data_web_service.rs:36
// Before:
.list_frame_file_paths_in_range(&request.from, &request.to)
// After:
let window = TimeWindow::from_rfc3339_pair(&request.from, &request.to)?;
.list_frame_file_paths_in_range(&window)
```

```rust
// crates/oneshim-web/src/services/data_web_service.rs:51 — preserve 5 bool flags
// Before:
.delete_data_in_range(
    &request.from, &request.to,
    request.delete_events, request.delete_frames, request.delete_metrics,
    request.delete_processes, request.delete_idle,
)
// After:
let window = TimeWindow::from_rfc3339_pair(&request.from, &request.to)?;
.delete_data_in_range(
    &window,
    request.delete_events, request.delete_frames, request.delete_metrics,
    request.delete_processes, request.delete_idle,
)
```

```rust
// crates/oneshim-web/src/services/events_service.rs:35
// Function: get_events(&self, params: &TimeRangeQuery) -> Result<EventPage, ApiError> — Result return ✓
// Before:
let total = self.ctx.storage
    .count_events_in_range(&from.to_rfc3339(), &to.to_rfc3339())
    .map_err(|error| ApiError::Internal(error.to_string()))?;
// After:
let window = TimeWindow::new(from, to)
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
let total = self.ctx.storage
    .count_events_in_range(&window)
    .map_err(|error| ApiError::Internal(error.to_string()))?;
```

```rust
// crates/oneshim-web/src/services/reports_query_support.rs:86
// (Verify enclosing function return type before patching — adapt to either Result or non-Result form below)
// Pattern A — if function returns Result<_, ApiError>:
let window = TimeWindow::from_rfc3339_pair(&from_rfc, &to_rfc)
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
if let Ok(daily_active) = input.ctx.storage.get_daily_active_secs(&window) {
    // ... existing body
}
// Pattern B — if function returns plain value (no Result):
if let Ok(window) = TimeWindow::from_rfc3339_pair(&from_rfc, &to_rfc) {
    if let Ok(daily_active) = input.ctx.storage.get_daily_active_secs(&window) {
        // ... existing body
    }
}
```

To find the enclosing function signature, scan upward from the caller site:

```bash
awk 'NR<=86 && /^pub.*fn |^pub\(crate\) fn |^fn /' crates/oneshim-web/src/services/reports_query_support.rs | tail -1
```

Verified at plan-write time: enclosing fn is `pub(crate) fn build_daily_stats(input: DailyStatsInput<'_>) -> Vec<DailyStat>` (returns plain `Vec<_>`, NOT `Result`). **Use Pattern B** for this caller.

Add `use oneshim_core::types::TimeWindow;` at top of each service file.

- [ ] **Step 4D.4: Update tests/support/failing_storage.rs delegation impls (5 sites — Phase 2 iter-3 NEW-C1 fix)**

Open `crates/oneshim-web/tests/support/failing_storage.rs`. **`FailingStorage` is a delegation harness** — every method delegates to `self.inner` (a real `SqliteStorage`) except for the specific failure scenarios under test. Preserve the delegation pattern. Update 5 sites at lines 278, 301, 333, 371, 403:

```rust
use oneshim_core::types::TimeWindow;

// Line 278 — delegate to inner
fn count_frames_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError> {
    self.inner
        .count_frames_in_range(window)
        .map_err(Into::into)
}

// Line 301 — delegate to inner
fn list_frame_file_paths_in_range(
    &self,
    window: &TimeWindow,
) -> Result<Vec<String>, CoreError> {
    self.inner
        .list_frame_file_paths_in_range(window)
        .map_err(Into::into)
}

// Line 333 — delegate to inner
fn count_events_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError> {
    self.inner
        .count_events_in_range(window)
        .map_err(Into::into)
}

// Line 371 — delegate to inner; preserve all 5 bool flags + #[allow]
#[allow(clippy::too_many_arguments)]
fn delete_data_in_range(
    &self,
    window: &TimeWindow,
    delete_events: bool,
    delete_frames: bool,
    delete_metrics: bool,
    delete_processes: bool,
    delete_idle: bool,
) -> Result<DeletedRangeCounts, CoreError> {
    self.inner
        .delete_data_in_range(
            window,
            delete_events,
            delete_frames,
            delete_metrics,
            delete_processes,
            delete_idle,
        )
        .map_err(Into::into)
}

// Line 403 — delegate to inner
fn get_daily_active_secs(&self, window: &TimeWindow) -> Result<Vec<(String, i64)>, CoreError> {
    self.inner
        .get_daily_active_secs(window)
        .map_err(Into::into)
}
```

These methods preserve the production delegation behavior — only methods that the test specifically targets for failure (e.g., `start_idle_period`) replace `self.inner` calls with synthetic `Err(...)`. The 5 range-helper methods just need their signatures updated to take `&TimeWindow` while still delegating.

- [ ] **Step 4D.5: Verify full compile**

```bash
cargo check --workspace 2>&1 | tail -20
```
Expected: clean across ALL crates including src-tauri. If errors remain, additional caller sites exist beyond the 30 enumerated in 4D.0 — grep them out and migrate.

### Sub-task 4E: Commit

- [ ] **Step 4E.1: Commit**

```bash
git add crates/oneshim-core/src/ports/{web_storage,calibration_store}.rs \
        crates/oneshim-storage/src/sqlite/ \
        crates/oneshim-web/src/services/{stats_query_support,data_web_service,events_service,reports_query_support}.rs \
        crates/oneshim-web/tests/support/failing_storage.rs \
        src-tauri/src/scheduler/analysis_pipeline/{regime,tests}.rs
git commit -m "$(cat <<'EOF'
refactor(storage): migrate 8 SQL range helpers + 14+ caller sites to &TimeWindow

Per Phase 2 iter-1 C6/C7 + iter-2 N-C1/N-C2/N-C3 scope expansion. Touches:
- 3 CalibrationReader/Writer port-trait sigs (calibration_store.rs)
  - flag_noise_range stays SYNC with Result<u64> (rows updated count)
  - list_segment_time_ranges keeps segment_id String → Vec<(String, TimeWindow)>
- 5 WebStorage sub-trait port-trait sigs (web_storage.rs)
  - delete_data_in_range preserves 5 bool flags + #[allow(too_many_arguments)]
  - get_daily_active_secs keeps Vec<(String, i64)> return
- 5 SQLite impl files (events, frames, maintenance, calibration_store_impl, work_sessions)
- 5 web_storage_impl.rs thin wrappers
- 5 service layer callers (stats_query_support, data_web_service x2, events_service, reports_query_support)
- 5 FailingStorage MockStorage impls (tests/support/failing_storage.rs)
- 3 src-tauri/scheduler/analysis_pipeline/regime.rs caller sites
  - lines 44, 184 = get_entries (re-fetch); line 174 = list_segment_time_ranges
  - regime.rs:194 destructure adapted to (seg_id, seg_window) using TimeWindow::contains
- 2 mocks in src-tauri/scheduler/analysis_pipeline/tests.rs (NoopCalibrationWriter sync + NoopCalibrationReader async — list_segment_time_ranges relies on trait default impl)
- ~15 internal SQLite test sites in events.rs, frames.rs, maintenance.rs

All changes done in lockstep — port trait + impl + inherent fn + wrappers + callers + mocks.
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

- [ ] **Step 5.2: Add 3 boundary regression tests (Phase 2 iter-2 N-I5 — explicit code blocks)**

Add closed-closed boundary regression test for each of: `count_frames_in_range`, `count_events_in_range`, `delete_data_in_range`.

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

#[test]
fn count_events_in_range_includes_both_boundaries() {
    let storage = test_storage();
    let t1 = "2026-04-01T00:00:00Z";
    let t2 = "2026-04-25T00:00:00Z";
    storage.insert_event_at(t1).unwrap();
    storage.insert_event_at("2026-04-15T00:00:00Z").unwrap();
    storage.insert_event_at(t2).unwrap();
    let window = TimeWindow::from_rfc3339_pair(t1, t2).unwrap();
    assert_eq!(storage.count_events_in_range(&window).unwrap(), 3);
}

#[test]
fn delete_data_in_range_respects_delete_flags() {
    let storage = test_storage();
    let window = TimeWindow::from_rfc3339_pair("2026-04-01T00:00:00Z", "2026-04-25T00:00:00Z").unwrap();
    seed_one_each(&storage);  // 1 event + 1 frame + 1 metric in window
    // delete only events; preserve frames + metrics + processes + idle
    let counts = storage.delete_data_in_range(
        &window,
        true,   // delete_events
        false,  // delete_frames
        false,  // delete_metrics
        false,  // delete_processes
        false,  // delete_idle
    ).unwrap();
    assert_eq!(counts.events_deleted, 1);
    assert_eq!(counts.frames_deleted, 0);
    assert_eq!(counts.metrics_deleted, 0);
    assert_eq!(counts.process_snapshots_deleted, 0);
    assert_eq!(counts.idle_periods_deleted, 0);
}
```

(Adapt to actual `test_storage()` / `seed_one_each()` test fixture conventions. `DeletedRangeCounts` field names verified at `crates/oneshim-core/src/models/storage_records.rs:88-94`.)

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

## Task 6: REST Handler + Service Layer Migration (Phase 2 iter-9 SCOPE EXPANSION)

**Estimate:** 5h (was 3h pre-iter-9 — +2h for service-layer migration) | **Spec ref:** §5.5 + Phase 2 iter-1 I3 + iter-9 NEW-C1 | **Files:** `crates/oneshim-web/src/handlers/{frames,events,metrics,focus,idle,processes}.rs` + `crates/oneshim-web/src/services/{frames,events,metrics,focus,idle,processes,timeline}_service.rs` + their `#[cfg(test)] mod tests` blocks

> **Phase 2 iter-1 I3**: 6 handlers use `TimeRangeQuery` directly.
>
> **Phase 2 iter-9 NEW-C1 — ARCHITECTURAL CORRECTION**: Reality is that **handlers thin-delegate to service layer** (`FramesQueryService::get_frames(&params)?` etc.). v8 example showing handler going direct to storage was WRONG. Migration must happen at the **service layer**, not handler layer.
>
> **Existing helpers behavior** (verified `crates/oneshim-api-contracts/src/common.rs:28-50`):
> - `from_datetime()`: silently parses `self.from` or returns `Utc::now() - Duration::hours(24)` (HARDCODED 24h fallback)
> - `to_datetime()`: silently parses `self.to` or returns `Utc::now()`
> - **Both swallow parse errors** — invalid timestamps fall back to defaults, return 200 OK with default-window data
>
> **New behavior after migration** via `to_time_window(default_lookback)`:
> - Returns `Result<TimeWindow, TimeWindowError>` — propagates parse errors as 400 BadRequest
> - **DEFAULT LOOKBACK PRESERVED at `Duration::hours(24)` everywhere** (Phase 2 iter-9 NEW-C1 fix). Plan v9 originally prescribed 7d/30d defaults — that would be a 7×/30× widening of payloads when frontend sends no bounds. v10 reverts to 24h to preserve current behavior; any deliberate widening should be a separate PR with frontend coordination.
> - **ONE BEHAVIOR CHANGE**: invalid timestamps now → 400 (was 200 with default window). User-facing API contract becomes stricter / more correct.
> - **NO behavior change for missing-bounds case**: `to_time_window(Duration::hours(24))` matches existing `from_datetime()` / `to_datetime()` semantics exactly.

- [ ] **Step 6.0: Verify ACTUAL caller scope (handlers + services)**

```bash
# Handler files using TimeRangeQuery
grep -rln "TimeRangeQuery" crates/oneshim-web/src/handlers/

# Service files using from_datetime/to_datetime helpers
grep -rln "\.from_datetime()\|\.to_datetime()" crates/oneshim-web/src/services/
```

Expected (verified at plan-write time):
- 6 handler files: `frames`, `events`, `metrics`, `focus`, `idle`, `processes` (sessions/interruptions don't use TimeRangeQuery directly)
- 7 service files: `frames_service`, `focus_service`, `idle_service`, `metrics_service`, `events_service`, `processes_service`, `timeline_service`

The handler layer is THIN — most just call `ServiceName::method(&params)?`. Migration happens in services.

- [ ] **Step 6.1: Update FramesQueryService to use to_time_window adapter (ARCHITECTURE-CORRECT pattern)**

Open `crates/oneshim-web/src/services/frames_service.rs`. Find method using `params.from_datetime()` + `params.to_datetime()`:

```rust
pub fn get_frames(&self, params: &TimeRangeQuery) -> Result<PaginatedResponse<FrameResponse>, ApiError> {
    let from = params.from_datetime();
    let to = params.to_datetime();
    let limit = params.limit_or_default();
    let offset = params.offset_or_default();
    // ... uses from, to, limit, offset
}
```

Change to (default lookback preserved at 24h per NEW-C1):
```rust
use chrono::Duration;
use oneshim_core::types::TimeWindow;

pub fn get_frames(&self, params: &TimeRangeQuery) -> Result<PaginatedResponse<FrameResponse>, ApiError> {
    let window = params.to_time_window(Duration::hours(24))   // ← matches existing 24h fallback
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let limit = params.limit_or_default();
    let offset = params.offset_or_default();
    // ... use window — see decomposition pattern below
}
```

**Storage method decomposition (Phase 2 iter-9 NEW-C2 fix)**: Task 4 only migrated 8 specific methods to `&TimeWindow`. The 7 services use OTHER storage methods that still take `DateTime<Utc>` or `&str`. For those, decompose `&window`:

| Storage method | Migrated by Task 4? | Decomposition pattern in service |
|----------------|---------------------|----------------------------------|
| `count_events_in_range(&TimeWindow)` | ✅ | `.count_events_in_range(&window)` |
| `count_frames_in_range(&TimeWindow)` | ✅ | `.count_frames_in_range(&window)` |
| `get_daily_active_secs(&TimeWindow)` | ✅ | `.get_daily_active_secs(&window)` |
| `delete_data_in_range(&TimeWindow, ...)` | ✅ | `.delete_data_in_range(&window, ...)` |
| `list_frame_file_paths_in_range(&TimeWindow)` | ✅ | `.list_frame_file_paths_in_range(&window)` |
| `flag_noise_range(&TimeWindow)` | ✅ | `.flag_noise_range(&window)` |
| `get_entries(&TimeWindow, bool)` | ✅ | `.get_entries(&window, exclude_noise)` |
| `list_segment_time_ranges(&TimeWindow)` | ✅ | `.list_segment_time_ranges(&window)` |
| `get_frames(DateTime<Utc>, DateTime<Utc>, usize)` | ❌ | `.get_frames(window.start, window.end, limit)` |
| `get_events(DateTime<Utc>, DateTime<Utc>, usize)` | ❌ (out of plan scope) | `.get_events(window.start, window.end, limit)` |
| `get_metrics`, `get_process_snapshots`, `get_idle_periods`, `list_work_sessions`, `list_interruptions`, `list_hourly_metrics_since` | ❌ (out of plan scope) | Pass `window.start, window.end` (decompose) |

**Out-of-scope methods stay on `DateTime<Utc>` signatures** — migrating them is future work (separate PR). For this PR, services destructure `&window` to `(window.start, window.end)` when calling them. Use the destructure pattern: `let TimeWindow { start, end } = window;` if Copy-derived (it is).

**No handler change needed** — handler still calls `FramesQueryService::new(context).get_frames(&params)?`. The handler-level `?` propagates `ApiError::BadRequest` → HTTP 400 automatically.

- [ ] **Step 6.2: Update EventsQueryService (CONTINUES Task 4D.3 events_service.rs:35 migration)**

Default lookback `Duration::hours(24)` (preserve existing behavior per NEW-C1). Task 4D.3 already migrated `events_service.rs:35` to use `TimeWindow::new(from, to)` inline — Step 6.2 refactors that inline conversion to `params.to_time_window(Duration::hours(24))?` for consistency with the rest of Task 6. Net change vs Task 4D.3: same TimeWindow construction, but error mapping uses `BadRequest` instead of `From<TimeWindowError> for CoreError` chain.

- [ ] **Step 6.3: Update MetricsService**

Same pattern. `Duration::hours(24)`.

- [ ] **Step 6.4: Update FocusQueryService (4 helper-call sites — verify ALL migrated)**

Open `crates/oneshim-web/src/services/focus_service.rs`. **Has 4 `from_datetime()` / `to_datetime()` calls** (lines 53-54, 68-69 per audit). Each method:
```bash
grep -n "from_datetime\|to_datetime\|to_time_window\|fn " crates/oneshim-web/src/services/focus_service.rs
```

Migrate each method's usage. Use `Duration::hours(24)` (preserve existing behavior — was 30d in v9 but flagged as default-window-size widening per NEW-C1).

- [ ] **Step 6.5: Update IdleService (handler-only migration, NG7 — IdlePeriod model NOT migrated)**

Service layer migrates the query window construction. The `IdlePeriod` model fields stay untouched (per NG7 — ongoing idle requires `Option<end_time>`).

- [ ] **Step 6.6: Update ProcessesService**

Same pattern. `Duration::hours(24)`.

- [ ] **Step 6.7: Update TimelineService**

Open `crates/oneshim-web/src/services/timeline_service.rs`. Apply same pattern. `Duration::hours(24)`.

- [ ] **Step 6.8: Decision — what to do with from_datetime/to_datetime helpers?**

Two options:

**(a) DEPRECATE in this PR**: Add `#[deprecated(note = "Use to_time_window for validating conversion")]` to `from_datetime` + `to_datetime`. Allows transitional period — existing test callers + handlers/mod.rs:80-style callers continue to work but emit warnings. Removed in follow-up cleanup PR.

**(b) KEEP as-is**: Helpers remain non-deprecated; they're useful for non-validating contexts (test fixtures, demos). New code uses `to_time_window`; old code can migrate gradually.

**Recommend (b)** — surgical scope. Don't expand this PR with deprecation churn. Document in PHASE-HISTORY that `to_time_window` is the new preferred path for validating conversion; old helpers retained for non-validating use.

- [ ] **Step 6.9: Verify compile + service tests**

```bash
cargo check -p oneshim-web 2>&1 | tail -10
cargo test -p oneshim-web --lib services 2>&1 | tail -20
```

- [ ] **Step 6.10: Commit**

```bash
git add crates/oneshim-web/src/services/{frames,events,metrics,focus,idle,processes,timeline}_service.rs
git commit -m "$(cat <<'EOF'
refactor(web-services): migrate 7 service-layer files to to_time_window adapter

Per Phase 2 iter-9 NEW-C1 (architectural correction). Plan v1-v8 example showed
handler going direct to storage — wrong. Reality: handlers thin-delegate to
service layer. Migration happens at service layer.

Affected services (7 files, ~16 helper-call sites):
- frames_service::get_frames (2 sites)
- focus_service get_work_sessions + get_interruptions (4 sites)
- idle_service (2 sites)
- metrics_service (2 sites)
- events_service (2 sites)
- processes_service (2 sites)
- timeline_service (2 sites)

Each service replaces params.from_datetime() + params.to_datetime() with
let window = params.to_time_window(default_lookback).map_err(BadRequest)?
then passes &window to storage methods (Task 4 already updated their sigs).

Behavior change: invalid timestamps now propagate as HTTP 400 BadRequest
(previously fell through to hardcoded 24h fallback in from_datetime() and
returned 200 OK with default-window data). This is a strict API contract
improvement — documented in PHASE-HISTORY.

from_datetime() / to_datetime() helpers retained (non-deprecated) for
non-validating uses (test fixtures, demos, internal tooling). New code uses
to_time_window for validating conversion.
EOF
)"
```

- [ ] **Step 6.11: Handler-layer note (no changes needed)**

Handlers `crates/oneshim-web/src/handlers/{frames,events,metrics,focus,idle,processes}.rs` are thin pass-through to services. No handler changes required for this migration — error propagation flows through service `?` chain.

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

- [ ] **Step 7.3: Migrate ReportQuery via flatten (Phase 2 iter-1 I11 + iter-2 N-I2)**

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

Change to use `#[serde(flatten)]` (which DOES work for struct-typed fields, unlike C9's invalid `flatten + with` combo). **Use `crate::common`, NOT `oneshim_api_contracts::common`** (Phase 2 iter-2 N-I2 — same-crate import):

```rust
use crate::common::TimeRangeQuery;

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

- [ ] **Step 7.3.5: Add `serde_urlencoded` dev-dependency (Phase 2 iter-2 N-I3)**

```bash
grep -E "^serde_urlencoded\s*=" crates/oneshim-api-contracts/Cargo.toml
```

If empty, add to `[dev-dependencies]` in `crates/oneshim-api-contracts/Cargo.toml`:
```toml
[dev-dependencies]
serde_urlencoded = "0.7"
```

(Used by Step 7.3 query-string roundtrip test.)

- [ ] **Step 7.3.6: Add roundtrip test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_query_query_string_roundtrip() {
        // Simulating axum's serde-urlencoded query parsing
        let raw = "period=custom&from=2026-04-01T00:00:00Z&to=2026-04-25T00:00:00Z";
        let q: ReportQuery = serde_urlencoded::from_str(raw).unwrap();
        assert_eq!(q.period, ReportPeriod::Custom);
        assert_eq!(q.time_range.from, Some("2026-04-01T00:00:00Z".to_string()));
        assert_eq!(q.time_range.to, Some("2026-04-25T00:00:00Z".to_string()));
    }
}
```

- [ ] **Step 7.4: Update DataCommandService (NOT handler — Phase 2 iter-9 NEW-C1 architectural correction)**

Reality (verified at `crates/oneshim-web/src/handlers/data.rs:9-16`): handler is thin pass-through to `DataCommandService::delete_data_range(&request)?`. The SQL deletion happens inside the service.

Since Task 4D.3 already migrated `data_web_service.rs:36+51` to build TimeWindow inline via `TimeWindow::from_rfc3339_pair(&request.from, &request.to)?`, this step **simplifies the call sites** to use the new `period()` accessor (defined in Task 7.1):

Open `crates/oneshim-web/src/services/data_web_service.rs`. Refactor the Task 4D.3 inline conversions:

```rust
// At data_web_service.rs:36 (list_frame_file_paths_in_range caller) — replace Task 4D.3 inline:
// Task 4D.3 wrote:
let window = TimeWindow::from_rfc3339_pair(&request.from, &request.to)?;
.list_frame_file_paths_in_range(&window)
// Task 7.4 simplifies to:
let window = request.period().map_err(|e| ApiError::BadRequest(e.to_string()))?;
.list_frame_file_paths_in_range(&window)

// Same pattern at data_web_service.rs:51 (delete_data_in_range caller) — reuse the window from above
```

Hoist the `let window = request.period()?;` to the top of `delete_data_range` method scope so both `list_frame_file_paths_in_range` (line 36) and `delete_data_in_range` (line 51) share the single TimeWindow. The handler stays unchanged (still thin delegate). HTTP 400 propagates via the service `?` chain through `DataCommandService::delete_data_range` → handler.

**No changes to `crates/oneshim-web/src/handlers/data.rs`** — already correct as thin delegate.

- [ ] **Step 7.5: Update ReportQueryService (NOT handler — Phase 2 iter-9 NEW-C1 architectural correction)**

Reality (verified at `crates/oneshim-web/src/handlers/reports.rs:11-19`): handler is thin pass-through to `ReportQueryService::generate_report(&params).await?`. The dispatch logic happens inside the service.

Open `crates/oneshim-web/src/services/reports_service.rs`. Inside `generate_report`, find the period dispatch logic and update:

```rust
use chrono::Duration;
use oneshim_core::types::TimeWindow;

pub async fn generate_report(&self, params: &ReportQuery) -> Result<ReportResponse, ApiError> {
    let window = match params.period {
        ReportPeriod::Custom => params.time_range.to_time_window(Duration::days(30))
            .map_err(|e| ApiError::BadRequest(e.to_string()))?,
        ReportPeriod::Week => {
            let now = Utc::now();
            TimeWindow::new(now - Duration::days(7), now).expect("valid 7-day window")
        },
        ReportPeriod::Month => {
            let now = Utc::now();
            TimeWindow::new(now - Duration::days(30), now).expect("valid 30-day window")
        },
    };
    // ... rest of report generation uses &window
}
```

**No changes to `crates/oneshim-web/src/handlers/reports.rs`** — already correct as thin delegate.

- [ ] **Step 7.6: Verify compile + tests**

```bash
cargo check -p oneshim-web -p oneshim-api-contracts 2>&1 | tail -10
cargo test -p oneshim-api-contracts --lib data::tests reports::tests 2>&1 | tail -15
```

- [ ] **Step 7.7: Commit**

```bash
git add crates/oneshim-api-contracts/src/{data,reports}.rs crates/oneshim-web/src/services/{data_web_service,reports_service}.rs
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

- [ ] **Step 8.1: Enumerate ALL FocusMetrics call sites (Phase 2 iter-2 N-C1)**

```bash
grep -rn "FocusMetrics {\|FocusMetrics::new\|\.period_start\|\.period_end" crates/ src-tauri/ 2>/dev/null | grep -v "/.git/" | grep -v frontend
```

**Expected 10 sites** (verified at plan-write time — Phase 2 iter-2 N-C1 enumeration):

| # | File:Line | Pattern | Notes |
|---|-----------|---------|-------|
| 1 | `crates/oneshim-core/src/models/work_session.rs:317` | `(self.period_end - self.period_start).num_seconds()` | Internal duration calc — replace with `self.period.duration().num_seconds()` |
| 2 | `crates/oneshim-core/src/models/work_session.rs:446` | `FocusMetrics::new(now, now + chrono::Duration::hours(1))` | Test fixture — must `.unwrap()` post-refactor |
| 3 | `crates/oneshim-storage/src/sqlite/edge_intelligence/focus_metrics.rs:55` | `Ok(FocusMetrics { ... })` | Struct literal in DB row mapping |
| 4 | `crates/oneshim-storage/src/sqlite/edge_intelligence/focus_metrics.rs:76` | `Ok(FocusMetrics::new(period_start, period_end))` | Trusted construction (cron-aligned) |
| 5 | `crates/oneshim-storage/src/sqlite/edge_intelligence/focus_metrics.rs:217` | `FocusMetrics { ... }` | Struct literal |
| 6 | `crates/oneshim-storage/src/sqlite/edge_intelligence/tests.rs:76` | `FocusMetrics::new(updated.period_start, updated.period_end)` | Test — must `.unwrap()` |
| 7 | `crates/oneshim-web/tests/grpc_dashboard_integration.rs:461` | `let metrics = FocusMetrics { ... }` | Test fixture |
| 8 | `src-tauri/src/focus_analyzer/mod.rs:384` | `let metrics = FocusMetrics { ... }` | Test fixture (struct literal) |
| 9 | `src-tauri/src/focus_analyzer/mod.rs:420` | `let metrics = FocusMetrics { ... }` | Test fixture |
| 10 | `src-tauri/src/focus_analyzer/mod.rs:442` | `let metrics = FocusMetrics { ... }` | Test fixture |

**Note**: `crates/oneshim-monitor/src/input_activity.rs:230` matches `period_start` but it's `self.period_start` on a DIFFERENT struct (not FocusMetrics) — false positive, skip.

If grep finds MORE than 10: update plan inline.

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
- **Tests**: +13 TimeWindow unit + 3 TimeWindowCode + 8 TimeRangeQuery adapter + 3 SQL boundary regression + 4 E2E + 2 ApiError mapping + 4 api-contracts roundtrip (DeleteRangeRequest×3 + ReportQuery×1) = **37 new tests total**
- **External API contract — query string shape preserved**: REST query strings unchanged (`?from=...&to=...&limit=...`); DeleteRangeRequest JSON shape preserved via accessor pattern (no custom serde required)
- **Behavior change #1 — invalid timestamp handling**: requests with malformed `from` / `to` query strings now return HTTP 400 BadRequest with parse error message. Previously: silently fell back to defaults (`from = Utc::now() - 24h`, `to = Utc::now()`) and returned 200 OK with default-window data. Strict API contract improvement.
- **Behavior preserved — default-window size**: `to_time_window(Duration::hours(24))` matches existing `from_datetime()` 24h fallback exactly. NO change for missing-bounds requests. (Plan v9 originally prescribed 7d/30d defaults — corrected in v10 per Phase 2 iter-9 NEW-C1.)
- **Behavior preserved — half-open vs closed-closed boundaries**: NG6 honored. SQL `BETWEEN` queries unchanged (closed-closed); work_sessions `started_at < ?2` half-open preserved (per NG6); calibration `start_time >= ?1 AND end_time <= ?2` containment semantic preserved.
- **Helpers retained**: `TimeRangeQuery::from_datetime()` / `to_datetime()` / `limit_or_default()` / `offset_or_default()` kept (non-deprecated) for non-validating use cases (test fixtures, demos, internal tooling). New code uses `to_time_window` for validating conversion.
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

### 5g. Phase 2 iter-10 findings disposition (v10 cleanup)

| Severity | ID | Disposition |
|----------|-----|-------------|
| Critical | NEW-C1 — default-window-size 24h→7d/30d widening | ✅ All 7 services use `Duration::hours(24)` (matches existing `from_datetime()` 24h fallback). Behavior preserved exactly. Phase 11.3 PHASE-HISTORY documents "Behavior preserved — default-window size" as explicit non-change. |
| Critical | NEW-C2 — non-migrated storage methods need decomposition | ✅ Step 6.1 adds explicit decomposition table: 8 storage methods migrated to `&TimeWindow` (Task 4); 6+ others (`get_frames`, `get_events`, `get_metrics`, etc.) stay on `DateTime<Utc>` and services destructure `&window` to `(window.start, window.end)` when calling them. Out-of-scope methods explicitly listed. |
| Important | NEW-I1 — Step 6.2 missing continuation framing | ✅ Step 6.2 explicitly says "CONTINUES Task 4D.3 events_service.rs:35 migration" + clarifies net change vs Task 4D.3 (same TimeWindow construction, different error mapping path). |
| Important | NEW-I2 — PHASE-HISTORY behavior change incomplete | ✅ Step 11.3 PHASE-HISTORY adds 5 new bullets: behavior change (invalid timestamp → 400), behavior preserved (default-window-size), behavior preserved (boundary semantics), helpers retained. Discoverable for frontend / integrators. |

### 5f. Phase 2 iter-9 findings disposition (v9 architectural correction)

| Severity | ID | Disposition |
|----------|-----|-------------|
| Critical | NEW-C1 — Service layer migration missing | ✅ Task 6 RENAMED to "REST Handler + Service Layer Migration" + completely rewritten with Steps 6.1-6.11. Audits 7 service files (frames/focus/idle/metrics/events/processes/timeline_service) using `from_datetime()`/`to_datetime()` helpers (silent error swallowing + hardcoded 24h fallback). Migration moves to service layer (handlers stay thin pass-through). Documents BEHAVIOR CHANGE: invalid timestamps now → HTTP 400 (was 200 with default-window data). Decision documented: keep `from_datetime()`/`to_datetime()` non-deprecated for non-validating uses. Estimate +2h (28h → 30h). |

### 5e. Phase 2 iter-6 findings disposition (v7 cleanup)

| Severity | ID | Disposition |
|----------|-----|-------------|
| Critical | NEW-C1 — Step 4C.1 calibration_store_impl synthetic-drift | ✅ Step 4C.1 rewritten with PRESERVE-BODY pattern (mirrors Step 4C.4 fix). Documents actual source at calibration_log table + is_noise column + fallible-lock CoreError::Storage mapping + async with_conn pattern + table_exists V9 migration guard + per-row parse error wrapping. Diff blocks show only parameter sig swap + locals binding + return-type consolidation for list_segment_time_ranges. All existing SQL/error-handling/control-flow preserved bit-identical. |

### 5d. Phase 2 iter-5 findings disposition (v6 cleanup)

| Severity | ID | Disposition |
|----------|-----|-------------|
| Important | NEW-I1 — stale MockCalibration labels | ✅ Step 4D.0 inventory row updated to "NoopCalibrationWriter + NoopCalibrationReader" (2 mocks); Step 4E.1 commit body updated to mention BOTH Noop impls |
| Important | NEW-I2 — Step 4C.4 wrong table/column names + missing system_metrics_hourly DELETE | ✅ Step 4C.4 rewritten as PRESERVE-BODY pattern: only swap parameter `from: &str, to: &str` → `window: &TimeWindow` + add `let (from, to) = window.to_sql_pair();` line. All existing SQL strings (system_metrics + system_metrics_hourly companion + idle_periods.start_time + lock error wrapping) preserved bit-identical |

### 5c. Phase 2 iter-4 pre-existing issues disposition (v5 cleanup)

| Severity | ID | Disposition |
|----------|-----|-------------|
| Important | Pre-existing #1 — MockCalibration vs NoopCalibration{Reader,Writer} | ✅ Step 4D.2 rewritten — uses two separate impls (`NoopCalibrationWriter` sync + `NoopCalibrationReader` async); notes default `list_segment_time_ranges` impl from trait |
| Important | Pre-existing #2 — DeletedRangeCounts field names | ✅ Step 4C.4 uses `events_deleted`/`frames_deleted`/`metrics_deleted`/`process_snapshots_deleted`/`idle_periods_deleted` per actual struct at storage_records.rs:88-94; Step 5.2 boundary test assertions also corrected |
| Important | Pre-existing #3 — maintenance.rs internal tests `?` | ✅ Step 4C.4 explicit pattern with `.expect("trusted test bounds")` for all 9 sites (mirrors Step 4C.2) |
| Important | Pre-existing #4 — stale Files-to-be-modified table | ✅ Lines 188-195 corrected: regime.rs:184 = get_entries (re-fetch); use `.expect()` not `?`; tests.rs has TWO Noop impls; added regime.rs:194 destructure adaptation row |
| Suggestion | Pre-existing #5 — non-functional grep helper | ✅ Step 4D.3 reports_query_support replaced grep with `awk` and pre-resolves to "use Pattern B for build_daily_stats" |
| Suggestion | Pre-existing #6 — variable name `dirty` vs `all` | ✅ Step 4C.1.5 line 425 example uses `let all = ...` matching actual source |

### 5b. Phase 2 iter-3 findings disposition (v4 corrections)

| Severity | ID | Disposition |
|----------|-----|-------------|
| Critical | NEW-C1 — FailingStorage delegation pattern | ✅ Step 4D.4 rewritten — all 5 methods delegate to `self.inner.method(window).map_err(Into::into)` (was wrongly using `self.failure_error(...)` which doesn't exist) |
| Critical | NEW-C2 — `?` operator in `()`/u64-returning callers | ✅ Step 4D.1 uses `.expect("lookback (now-7d) is always before now")` for regime.rs (both functions return `()`); Step 4D.3 stats_query_support uses `let Ok(window) = ... else { return fallback }` for `u64` return; events_service uses `.map_err(|e| ApiError::BadRequest(...))?` for Result return; reports_query_support documents Pattern A vs B selection |
| Important | NEW-I1 — calibration_store_impl test callers missing | ✅ Step 4D.0 enumeration table adds row for 5 sites (lines 400, 414, 420, 425, 443); New Step 4C.1.5 migrates them with full code blocks |

### 5a. Phase 2 iter-2 findings disposition (v3 corrections)

| Severity | ID | Disposition |
|----------|-----|-------------|
| Critical | N-C1 — FocusMetrics 5 missing call sites | ✅ Task 8.1 enumerates ALL 10 sites with file:line table |
| Critical | N-C2 — regime.rs:184 misidentified | ✅ Task 4D.1 corrected: lines 44+184 are `get_entries`, line 174 is `list_segment_time_ranges`, NO `flag_noise_range` callers in regime.rs |
| Critical | N-C3 — 9+ caller sites missing | ✅ Task 4D.0 enumerates 30 callers + 4D.3/4D.4 migrate services + failing_storage; 4C decision: change inherent `pub fn` in lockstep with traits |
| Critical | N-C4 — list_segment_time_ranges 3-tuple | ✅ Task 4A.4 returns `Vec<(String, TimeWindow)>` (preserves segment_id) + Task 4D.1 destructures `(seg_id, seg_window)` |
| Critical | N-C5 — flag_noise_range sync + Result<u64> | ✅ Task 4A.2 specifies sync (NO async) + Result<u64, CoreError>; Task 4D.2 mock matches |
| Critical | N-C6 — get_daily_active_secs Vec<(String,i64)> | ✅ Task 4B.5 corrected return type |
| Important | N-I1 — duplicate Step 1.11 | ✅ Step 1.11 removed (Step 1.3 already covers it) — re-numbered Steps 1.12 → 1.11 + 1.13 → 1.12 + 1.14 → 1.13 |
| Important | N-I2 — wrong same-crate import | ✅ Task 7.3 uses `use crate::common::TimeRangeQuery;` |
| Important | N-I3 — serde_urlencoded dev-dep missing | ✅ New Step 7.3.5 adds dev-dep |
| Important | N-I4 — Option<usize> not Option<u32> | ✅ Task 3.1 corrected |
| Important | N-I5 — test count drift | ✅ Step 5.2 has explicit 3 boundary test code blocks + Step 11.3 PHASE-HISTORY updated to 37 tests |

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

**Plan v7 complete and saved to** `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task + 2-stage review.

**2. Inline Execution** — executing-plans batch with checkpoints.

(For ralph-loop continuation: Phase 2 iter-7 plan v7 complete addressing iter-1 (9C+11I) + iter-2 (6 NEW C + 5 NEW I) + iter-3 (2 NEW C + 1 NEW I) + iter-4 (4 Important + 2 Suggestion pre-existing cleanup) + iter-5 (2 NEW Important from v5 over-specification) + iter-6 (1 NEW Critical from v6 missed PRESERVE-BODY pattern in Step 4C.1) = 18 Critical + 23 Important + 2 Suggestion total across 7 iterations. Next iteration: fresh subagent verification of plan v7. If clean → Phase 2 EXIT. Phase 3 implementation BLOCKED on PR-B1 #508 merge.)
