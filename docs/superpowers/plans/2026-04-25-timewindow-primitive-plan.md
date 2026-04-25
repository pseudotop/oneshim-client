# TimeWindow Primitive Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate 5 main + 4 supporting divergent absolute-timestamp time-range types across the workspace into a single canonical `TimeWindow { start: DateTime<Utc>, end: DateTime<Utc> }` primitive in `oneshim-core::types`.

**Architecture:** Closed-closed `[start, end]` semantic (matches existing SQL `BETWEEN`, Stripe-style business API pattern). Big-bang single PR migration covering REST handlers + SQL storage + domain models + GDPR API + custom serde for backward-compat. Wall-clock recurrence types (TrackingWindow, coaching TimeRange) intentionally unmigrated. IdlePeriod intentionally unmigrated (per NG7).

**Tech Stack:**
- Rust + chrono 0.4.44 (`DateTime<Utc>`, `Duration`)
- sha2 0.11 (already in workspace — no Cargo.toml changes)
- thiserror (existing convention for error types)
- serde + serde_with for custom serde rename (DeleteRangeRequest)
- ADR-019 wire codes via `define_code_enum!` macro
- rusqlite `params!` macro (existing pattern, prefer over slice)

**Source spec:** `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md` (v3, commit `f495dfbd`)

**Worktree:** `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/timewindow-primitive` on branch `refactor/timewindow-primitive`

**Total estimate:** ~21h across 12 tasks (~3-4 working days).

**⚠ ABORT GUARD**: PR-B1 (#508) MUST merge before Task 1 begins. PR-B1 modifies `oneshim-core/config/sections/` and `oneshim-core/src/error_codes/` — overlapping crate areas. Implementing TimeWindow before #508 merges will cause significant rebase conflicts.

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

- [ ] **PF3: Capture wire snapshot baseline (Q-8 + C1)**

```bash
wc -l crates/oneshim-core/tests/wire_contract_snapshot.expected.txt
```
Record the count. Spec assumes 42 (pre-PR-B1) but post-PR-B1 = 47, post-PR-B2 = 51. Use the actual count + 2 for spec §7.2 wire code total verification.

Also: identify alphabetical insertion position for `time_window.inverted_bounds` and `time_window.parse_failed` codes:
```bash
grep -n "^t" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt
```
The `time_window.*` codes go between `tag.*` (if any) / `tracking_schedule.*` (if PR-A merged) and `update.*`.

- [ ] **PF4: Verify baseline GREEN**

```bash
cargo check --workspace
cargo test -p oneshim-core --test wire_contract_snapshot
```
Both expected GREEN.

- [ ] **PF5: Required reading**

1. `crates/oneshim-core/src/lib.rs` — find module registration pattern
2. `crates/oneshim-core/src/error.rs` — find `CoreError` enum + `From` impls + `code()` method
3. `crates/oneshim-core/src/error_codes/mod.rs` — `all_codes()` aggregator pattern
4. `crates/oneshim-core/src/error_codes/audio.rs` — `define_code_enum!` macro example
5. `crates/oneshim-api-contracts/src/common.rs:5-11` — current `TimeRangeQuery` struct
6. `crates/oneshim-api-contracts/Cargo.toml:16` — confirm `oneshim-core = { workspace = true }` dep
7. `crates/oneshim-storage/src/sqlite/frames.rs:10` — current `count_frames_in_range` signature
8. `crates/oneshim-web/src/handlers/frames.rs` — current handler using `TimeRangeQuery::with_defaults`
9. `crates/oneshim-core/src/ports/calibration_store.rs` — `flag_noise_range` port trait sig
10. **Spec v3**: `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md`

---

## File Structure

### Files to be created

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/types/mod.rs` | Re-export `TimeWindow` and `TimeWindowError` |
| `crates/oneshim-core/src/types/time_window.rs` | `TimeWindow` struct + `TimeWindowError` enum + impl + tests |
| `crates/oneshim-core/src/error_codes/time_window.rs` | `TimeWindowCode` enum via `define_code_enum!` macro |

### Files to be modified

| File | What changes |
|------|--------------|
| `crates/oneshim-core/src/lib.rs` | Add `pub mod types;` |
| `crates/oneshim-core/src/error_codes/mod.rs` | `pub mod time_window;` + `pub use time_window::TimeWindowCode;` + `for c in TimeWindowCode::all() { codes.push(c.as_str()); }` in `all_codes()` |
| `crates/oneshim-core/src/error.rs` | Add `TimeWindow(TimeWindowError)` variant to `CoreError` enum + `From<TimeWindowError> for CoreError` impl + map to `TimeWindowError::code()` in `CoreError::code()` |
| `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` | Insert `time_window.inverted_bounds` + `time_window.parse_failed` in alphabetical position |
| `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json` | Add 2 new wire-error translations |
| `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json` | Add 2 Korean translations |
| `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts` | Update wire-code count expectations |
| `crates/oneshim-api-contracts/src/common.rs` | Add `to_time_window(&self, default_lookback) -> Result<TimeWindow, TimeWindowError>` impl on `TimeRangeQuery` |
| `crates/oneshim-api-contracts/src/data.rs` | Migrate `DeleteRangeRequest` to `period: TimeWindow` field with custom serde for shape preservation |
| `crates/oneshim-api-contracts/src/reports.rs` | Migrate `ReportQuery` to `{ period: ReportPeriod, window: Option<TimeWindow> }` |
| `crates/oneshim-storage/src/sqlite/events.rs` | `count_events_in_range(window: &TimeWindow)` |
| `crates/oneshim-storage/src/sqlite/frames.rs` | `count_frames_in_range(window: &TimeWindow)` + `get_frames(window: &TimeWindow, limit)` |
| `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs` | `flag_noise_range(window: &TimeWindow)` (impl change) |
| `crates/oneshim-core/src/ports/calibration_store.rs` | `flag_noise_range` port trait signature change to `&TimeWindow` |
| `crates/oneshim-storage/src/sqlite/web_storage_impl.rs` | `get_daily_active_secs(window: &TimeWindow)` + other range query helpers |
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | Any range-query helpers using `(from, to)` pair |
| `crates/oneshim-web/src/handlers/frames.rs` | Use `q.to_time_window(Duration::days(7))?` |
| `crates/oneshim-web/src/handlers/events.rs` | Use `q.to_time_window(Duration::days(7))?` |
| `crates/oneshim-web/src/handlers/metrics.rs` | Use `q.to_time_window(Duration::days(7))?` |
| `crates/oneshim-web/src/handlers/focus.rs` | Use `q.to_time_window(...)` |
| `crates/oneshim-web/src/handlers/idle.rs` | Use `q.to_time_window(...)` (handler only — IdlePeriod model NOT migrated per NG7) |
| `crates/oneshim-web/src/handlers/processes.rs` | Use `q.to_time_window(...)` |
| `crates/oneshim-web/src/handlers/data.rs` | GDPR delete using `req.period: TimeWindow` |
| `crates/oneshim-web/src/handlers/reports.rs` | ReportQuery with `period` enum + optional `window` |
| `crates/oneshim-core/src/models/work_session.rs:287-299` | `FocusMetrics`: `period_start/period_end` → `period: TimeWindow` (per NG8 — internal model only, REST DTO unchanged) |
| `crates/oneshim-core/src/models/telemetry.rs:16-17` | `SessionMetrics`: `period_*` → `period: TimeWindow` |
| `docs/STATUS.md` | Test count update + version note |
| `docs/PHASE-HISTORY.md` | TimeWindow refactor entry |

---

## Task 1: TimeWindow Primitive Type + types/ Module Registration

**Estimate:** 2.5h | **Spec ref:** §5.1 + §4.1 + Phase 1 iter-1 I5 | **Files:** Create `crates/oneshim-core/src/types/mod.rs`, `crates/oneshim-core/src/types/time_window.rs`, modify `crates/oneshim-core/src/lib.rs`

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

- [ ] **Step 1.6: Run tests**

```bash
cargo test -p oneshim-core --lib types::time_window::tests 2>&1 | tail -15
```
Expected: 12 tests pass (after Task 2 wires up `TimeWindowCode`). If `TimeWindowCode` not yet defined: tests fail compile; commit Task 1 with `code()` method and last 2 tests temporarily commented out, then re-enable in Task 2.

- [ ] **Step 1.7: Commit**

```bash
git add crates/oneshim-core/src/types/ crates/oneshim-core/src/lib.rs
git commit -m "feat(time): add TimeWindow primitive + TimeWindowError + types module"
```

---

## Task 2: TimeWindowCode Wire Code Enum + CoreError Integration

**Estimate:** 1.5h | **Spec ref:** §7.2 + Phase 1 iter-1 C2 + C3 | **Files:** Create `crates/oneshim-core/src/error_codes/time_window.rs`, modify `crates/oneshim-core/src/error_codes/mod.rs`, `crates/oneshim-core/src/error.rs`, `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`

- [ ] **Step 2.1: Create TimeWindowCode enum**

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

- [ ] **Step 2.2: Register in error_codes/mod.rs**

Open `crates/oneshim-core/src/error_codes/mod.rs`. Add:

1. After existing `pub mod` declarations, alphabetical position (after `tag` if present, before `update`):
```rust
pub mod time_window;
```

2. After existing `pub use` re-exports:
```rust
pub use time_window::TimeWindowCode;
```

3. In the `all_codes()` function, add to the iteration list (alphabetical):
```rust
for c in TimeWindowCode::all() {
    codes.push(c.as_str());
}
```

- [ ] **Step 2.3: Add CoreError variant + From impl**

Open `crates/oneshim-core/src/error.rs`. Find the `CoreError` enum definition. Add a new variant:

```rust
#[error("time_window: {0}")]
TimeWindow(#[from] crate::types::TimeWindowError),
```

(Or whatever the project's existing pattern is — verify by reading neighboring variants like `Storage(#[from] StorageError)`.)

Find the `CoreError::code() -> &str` method. Add a match arm:

```rust
CoreError::TimeWindow(e) => e.code().as_str(),
```

- [ ] **Step 2.4: Update wire snapshot expected.txt**

Open `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`. Insert in alphabetical position:

```
time_window.inverted_bounds
time_window.parse_failed
```

Use `grep -n "^time\|^tracking\|^update" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` to find correct insertion line.

- [ ] **Step 2.5: Verify wire snapshot test + new code tests**

```bash
cargo test -p oneshim-core --test wire_contract_snapshot 2>&1 | tail -10
cargo test -p oneshim-core --lib error_codes::time_window::tests 2>&1 | tail -10
cargo test -p oneshim-core --lib types::time_window::tests 2>&1 | tail -10
```
All expected GREEN.

- [ ] **Step 2.6: Commit**

```bash
git add crates/oneshim-core/src/error_codes/time_window.rs \
         crates/oneshim-core/src/error_codes/mod.rs \
         crates/oneshim-core/src/error.rs \
         crates/oneshim-core/tests/wire_contract_snapshot.expected.txt
git commit -m "feat(error-codes): TimeWindowCode wire codes + CoreError::TimeWindow integration"
```

---

## Task 3: Wire-Error i18n Translations

**Estimate:** 0.5h | **Spec ref:** §7.2 ADR-019 i18n CI gate | **Files:** `crates/oneshim-web/frontend/src/i18n/wire-errors.{en,ko}.json`, `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts`

- [ ] **Step 3.1: Add en translations**

Open `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json`. Add (alphabetical position):
```json
  "time_window.inverted_bounds": "Invalid time range: start must be before end",
  "time_window.parse_failed": "Invalid timestamp format: {message}",
```

- [ ] **Step 3.2: Add ko translations**

Open `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json`:
```json
  "time_window.inverted_bounds": "시간 범위가 잘못되었습니다: 시작이 종료보다 빨라야 합니다",
  "time_window.parse_failed": "타임스탬프 형식이 잘못되었습니다: {message}",
```

- [ ] **Step 3.3: Update Vitest count expectations**

Open `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts`. Find current count assertions:
```bash
grep -n "toHaveLength\|expected.*codes" crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts
```

Update count = current + 2 (per PF3 actual baseline).

- [ ] **Step 3.4: Run CI gate + Vitest**

```bash
bash scripts/check-wire-error-i18n-coverage.sh 2>&1 | tail -5
cd crates/oneshim-web/frontend && pnpm test src/i18n/__tests__/translateError.test.ts --run 2>&1 | tail -10
```
Both expected GREEN.

- [ ] **Step 3.5: Commit**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/timewindow-primitive
git add crates/oneshim-web/frontend/src/i18n/wire-errors.en.json \
         crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json \
         crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts
git commit -m "test(i18n): wire-error translations for TimeWindow codes (en+ko)"
```

---

## Task 4: TimeRangeQuery::to_time_window Adapter

**Estimate:** 1.5h | **Spec ref:** §5.2 + Phase 1 iter-1 C4 | **Files:** Modify `crates/oneshim-api-contracts/src/common.rs`

- [ ] **Step 4.1: Add adapter method (non-consuming &self per C4)**

Open `crates/oneshim-api-contracts/src/common.rs`. Find existing `impl TimeRangeQuery {}` block (or add one). Append:

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

- [ ] **Step 4.2: Add tests**

Append to the same file or `crates/oneshim-api-contracts/src/common.rs` `#[cfg(test)] mod tests`:

```rust
#[cfg(test)]
mod time_window_adapter_tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn to_time_window_with_both_bounds_provided() {
        let q = TimeRangeQuery {
            from: Some("2026-04-01T00:00:00Z".to_string()),
            to: Some("2026-04-25T00:00:00Z".to_string()),
            ..Default::default()
        };
        let w = q.to_time_window(Duration::days(7)).unwrap();
        assert_eq!(w.start.timestamp(), 1775433600); // 2026-04-01 UTC
        assert_eq!(w.end.timestamp(), 1777507200);   // 2026-04-25 UTC
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
        assert_eq!(w.end.timestamp() - w.start.timestamp(), 7 * 86400);
    }

    #[test]
    fn to_time_window_default_both_when_neither_provided() {
        let q = TimeRangeQuery {
            from: None,
            to: None,
            ..Default::default()
        };
        let w = q.to_time_window(Duration::days(7)).unwrap();
        assert_eq!(w.end.timestamp() - w.start.timestamp(), 7 * 86400);
    }

    #[test]
    fn to_time_window_rejects_invalid_iso8601() {
        let q = TimeRangeQuery {
            from: Some("not-a-date".to_string()),
            to: None,
            ..Default::default()
        };
        let result = q.to_time_window(Duration::days(7));
        assert!(matches!(result, Err(TimeWindowError::ParseFailed(_))));
    }

    #[test]
    fn to_time_window_takes_ref_so_caller_keeps_other_fields() {
        // C4 verification: &self adapter doesn't consume q
        let q = TimeRangeQuery {
            from: Some("2026-04-01T00:00:00Z".to_string()),
            to: Some("2026-04-25T00:00:00Z".to_string()),
            limit: Some(50),
            ..Default::default()
        };
        let _w = q.to_time_window(Duration::days(7)).unwrap();
        // q still usable
        assert_eq!(q.limit, Some(50));
    }
}
```

- [ ] **Step 4.3: Verify compile + tests**

```bash
cargo test -p oneshim-api-contracts --lib common::time_window_adapter_tests 2>&1 | tail -15
```
Expected: 6 tests pass.

- [ ] **Step 4.4: Commit**

```bash
git add crates/oneshim-api-contracts/src/common.rs
git commit -m "feat(api): TimeRangeQuery::to_time_window adapter (non-consuming &self per C4)"
```

---

## Task 5: SQL Storage Helper Migration + Calibration Port Trait

**Estimate:** 3h | **Spec ref:** §5.3 + Phase 1 iter-1 N3 | **Files:** Modify `crates/oneshim-storage/src/sqlite/{events,frames,calibration_store_impl,web_storage_impl,maintenance}.rs`, `crates/oneshim-core/src/ports/calibration_store.rs`

- [ ] **Step 5.1: Update calibration_store port trait**

Open `crates/oneshim-core/src/ports/calibration_store.rs`. Find `flag_noise_range` trait method:
```rust
fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<...>;
```

Change to:
```rust
fn flag_noise_range(&self, window: &TimeWindow) -> Result<...>;
```

Add `use crate::types::TimeWindow;` at top of file.

- [ ] **Step 5.2: Migrate calibration_store_impl**

Open `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs`. Find `flag_noise_range` impl:
```rust
fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    conn.execute("UPDATE ... WHERE timestamp >= ?1 AND timestamp <= ?2", [from, to])?;
    Ok(())
}
```

Change to:
```rust
use oneshim_core::types::TimeWindow;

fn flag_noise_range(&self, window: &TimeWindow) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    let (from, to) = window.to_sql_pair();
    conn.execute(
        "UPDATE ... WHERE timestamp >= ?1 AND timestamp <= ?2",
        rusqlite::params![&from, &to],
    )?;
    Ok(())
}
```

- [ ] **Step 5.3: Migrate frames.rs**

Open `crates/oneshim-storage/src/sqlite/frames.rs`. Find `count_frames_in_range`:
```rust
pub fn count_frames_in_range(&self, from: &str, to: &str) -> Result<u64, StorageError>
```

Change to:
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

Find `get_frames(from: DateTime<Utc>, to: DateTime<Utc>, limit: usize)`:
```rust
pub fn get_frames(&self, window: &TimeWindow, limit: usize) -> Result<Vec<FrameRow>, StorageError> {
    let conn = self.conn.lock().unwrap();
    let (from, to) = window.to_sql_pair();
    // ... rest of impl using from, to as RFC3339 strings
}
```

- [ ] **Step 5.4: Migrate events.rs**

Open `crates/oneshim-storage/src/sqlite/events.rs`. Find `count_events_in_range`. Apply same pattern as Step 5.3.

- [ ] **Step 5.5: Migrate web_storage_impl.rs**

Open `crates/oneshim-storage/src/sqlite/web_storage_impl.rs`. Find `get_daily_active_secs(from: &str, to: &str)`. Apply same pattern.

Search for OTHER `*_in_range` methods in this file:
```bash
grep -n "in_range\|fn .*from.*to" crates/oneshim-storage/src/sqlite/web_storage_impl.rs
```
Migrate each one.

- [ ] **Step 5.6: Migrate maintenance.rs**

Open `crates/oneshim-storage/src/sqlite/maintenance.rs`. Search for range-query helpers:
```bash
grep -n "from.*to\|in_range" crates/oneshim-storage/src/sqlite/maintenance.rs
```
Migrate any matching methods.

- [ ] **Step 5.7: Verify compile**

```bash
cargo check -p oneshim-storage 2>&1 | tail -10
cargo check -p oneshim-core 2>&1 | tail -10
```
Both expected: clean.

- [ ] **Step 5.8: Commit**

```bash
git add crates/oneshim-core/src/ports/calibration_store.rs crates/oneshim-storage/src/sqlite/
git commit -m "refactor(storage): migrate SQL range helpers to &TimeWindow + port trait change for flag_noise_range"
```

---

## Task 6: Storage Regression Tests

**Estimate:** 1h | **Spec ref:** §8.3 | **Files:** Existing `#[cfg(test)] mod tests` in `crates/oneshim-storage/src/sqlite/{frames,events,calibration_store_impl,web_storage_impl}.rs`

- [ ] **Step 6.1: Update existing tests for new signatures**

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

- [ ] **Step 6.2: Add boundary regression test**

For at least one helper (e.g., `count_frames_in_range`), add a new test verifying closed-closed semantic preserved:

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

(Adapt to actual test fixture API.)

- [ ] **Step 6.3: Run all storage tests**

```bash
cargo test -p oneshim-storage 2>&1 | tail -15
```
Expected: all pre-existing tests pass + new boundary test passes.

- [ ] **Step 6.4: Commit**

```bash
git add crates/oneshim-storage/src/sqlite/
git commit -m "test(storage): regression tests for migrated SQL helpers (boundary preservation verified)"
```

---

## Task 7: REST Handler Migration (frames/events/metrics/focus/idle/processes)

**Estimate:** 4h | **Spec ref:** §5.5 | **Files:** `crates/oneshim-web/src/handlers/{frames,events,metrics,focus,idle,processes}.rs`

- [ ] **Step 7.1: Migrate frames.rs handler**

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

(`?` operator works because `From<TimeWindowError> for ApiError` chain via `CoreError::TimeWindow` per Task 2.)

- [ ] **Step 7.2: Migrate events.rs**

Same pattern. Adapt default lookback (likely 7 days for events).

- [ ] **Step 7.3: Migrate metrics.rs**

Same pattern. Use `Duration::days(7)`.

- [ ] **Step 7.4: Migrate focus.rs**

Same pattern. Use appropriate default lookback (focus_metrics is daily aggregate, may use `Duration::days(30)`).

- [ ] **Step 7.5: Migrate idle.rs (handler only — IdlePeriod model NOT migrated per NG7)**

Same pattern for handler. The model `IdlePeriod` retains its current `start_time + Option<end_time>` shape.

- [ ] **Step 7.6: Migrate processes.rs**

Same pattern.

- [ ] **Step 7.7: Verify compile**

```bash
cargo check -p oneshim-web 2>&1 | tail -10
```

- [ ] **Step 7.8: Run handler tests**

```bash
cargo test -p oneshim-web --lib handlers 2>&1 | tail -15
```

- [ ] **Step 7.9: Commit**

```bash
git add crates/oneshim-web/src/handlers/{frames,events,metrics,focus,idle,processes}.rs
git commit -m "refactor(handlers): migrate 6 REST handlers to TimeRangeQuery::to_time_window adapter"
```

---

## Task 8: Migrate data.rs (GDPR) + reports.rs with Custom Serde

**Estimate:** 1.5h | **Spec ref:** §5.6 + Q-3 + Q-10 | **Files:** `crates/oneshim-api-contracts/src/data.rs`, `crates/oneshim-api-contracts/src/reports.rs`, `crates/oneshim-web/src/handlers/data.rs`, `crates/oneshim-web/src/handlers/reports.rs`

- [ ] **Step 8.1: Migrate DeleteRangeRequest with custom serde**

Open `crates/oneshim-api-contracts/src/data.rs`. Current:
```rust
pub struct DeleteRangeRequest {
    pub from: String,
    pub to: String,
    pub data_types: Vec<String>,
}
```

Change to:
```rust
use oneshim_core::types::TimeWindow;
use serde::{Deserialize, Serialize, Deserializer, Serializer};

pub struct DeleteRangeRequest {
    /// Internally a TimeWindow, externally serialized as flat from/to per
    /// Phase 1 iter-1 Q-10 option (b) — preserves frontend DataSection.tsx
    /// without requiring TypeScript type updates.
    #[serde(flatten, with = "delete_range_period_serde")]
    pub period: TimeWindow,
    pub data_types: Vec<String>,
}

mod delete_range_period_serde {
    use super::*;
    use chrono::DateTime;

    #[derive(Serialize, Deserialize)]
    struct External {
        from: String,
        to: String,
    }

    pub fn serialize<S: Serializer>(window: &TimeWindow, s: S) -> Result<S::Ok, S::Error> {
        External {
            from: window.start.to_rfc3339(),
            to: window.end.to_rfc3339(),
        }.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<TimeWindow, D::Error> {
        let ext = External::deserialize(d)?;
        TimeWindow::from_rfc3339_pair(&ext.from, &ext.to)
            .map_err(serde::de::Error::custom)
    }
}
```

- [ ] **Step 8.2: Add roundtrip test**

Append to `data.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_range_request_external_shape_preserved() {
        let json = r#"{"from":"2026-04-01T00:00:00Z","to":"2026-04-25T00:00:00Z","data_types":["frames"]}"#;
        let req: DeleteRangeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.data_types, vec!["frames"]);
        let serialized = serde_json::to_string(&req).unwrap();
        // Verify external shape uses from/to (not start/end)
        assert!(serialized.contains("\"from\":"));
        assert!(serialized.contains("\"to\":"));
        assert!(!serialized.contains("\"start\":"));
        assert!(!serialized.contains("\"end\":"));
    }
}
```

- [ ] **Step 8.3: Migrate ReportQuery in reports.rs (per Q-3)**

Open `crates/oneshim-api-contracts/src/reports.rs`. Current:
```rust
pub struct ReportQuery {
    pub period: ReportPeriod,
    pub from: Option<String>,
    pub to: Option<String>,
}
```

Change to:
```rust
use oneshim_core::types::TimeWindow;

pub struct ReportQuery {
    pub period: ReportPeriod,
    /// Only Some when period == ReportPeriod::Custom.
    pub window: Option<TimeWindow>,
}
```

- [ ] **Step 8.4: Update data.rs handler**

Open `crates/oneshim-web/src/handlers/data.rs`. Find handler using `req.from`/`req.to`. Change to use `req.period: TimeWindow` directly (already validated at deserialization).

- [ ] **Step 8.5: Update reports.rs handler**

Open `crates/oneshim-web/src/handlers/reports.rs`. Find handler. Update logic to use `req.period` enum + optional `req.window` based on dispatch.

- [ ] **Step 8.6: Verify compile + tests**

```bash
cargo check -p oneshim-web -p oneshim-api-contracts 2>&1 | tail -10
cargo test -p oneshim-api-contracts 2>&1 | tail -10
```

- [ ] **Step 8.7: Commit**

```bash
git add crates/oneshim-api-contracts/src/{data,reports}.rs crates/oneshim-web/src/handlers/{data,reports}.rs
git commit -m "refactor(api): migrate DeleteRangeRequest (custom serde) + ReportQuery to TimeWindow per Q-3+Q-10"
```

---

## Task 9: Domain Model Migration (FocusMetrics + SessionMetrics)

**Estimate:** 1.5h | **Spec ref:** §5.4 + NG7 + NG8 | **Files:** `crates/oneshim-core/src/models/work_session.rs:287-299`, `crates/oneshim-core/src/models/telemetry.rs:16-17`

- [ ] **Step 9.1: Migrate FocusMetrics (per NG8 — Option Z, internal model only)**

Open `crates/oneshim-core/src/models/work_session.rs`. Find `FocusMetrics`:
```rust
pub struct FocusMetrics {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub deep_work_secs: u64,
    // ...
}
```

Change to:
```rust
use crate::types::TimeWindow;

pub struct FocusMetrics {
    pub period: TimeWindow,
    pub deep_work_secs: u64,
    // ...
}
```

- [ ] **Step 9.2: Update FocusMetrics constructors / mappers**

Find every `FocusMetrics { period_start: ..., period_end: ... }` literal in the codebase:
```bash
grep -rn "FocusMetrics {" crates/
```

Update each to use `period: TimeWindow::new(start, end)?` or `TimeWindow::new(start, end).expect("...")` if previously trusted.

Also update `focus_assembler.rs` mapping from `FocusMetrics` to `FocusMetricsDto` — read `period_start`/`period_end` becomes `period.start`/`period.end`.

- [ ] **Step 9.3: Migrate SessionMetrics**

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

Update constructors similarly to Step 9.2.

- [ ] **Step 9.4: Verify compile**

```bash
cargo check --workspace 2>&1 | tail -10
```

- [ ] **Step 9.5: Commit**

```bash
git add crates/oneshim-core/src/models/{work_session,telemetry}.rs $(grep -rln "FocusMetrics {" crates/ | grep -v "/.git/")
git commit -m "refactor(models): FocusMetrics + SessionMetrics period_* → period: TimeWindow (NG8 internal-only)"
```

---

## Task 10: ReportQuery Cleanup + Final api-contracts Sweep

**Estimate:** 1h | **Spec ref:** §5 + Q-3 cleanup | **Files:** any remaining api-contracts files

- [ ] **Step 10.1: Sweep for remaining range-pair patterns**

```bash
grep -rn "from: Option<String>\|to: Option<String>\|period_start.*DateTime\|period_end.*DateTime" crates/oneshim-api-contracts/src/ | grep -v test
```

For each remaining occurrence, evaluate:
- If absolute timestamp window → migrate to TimeWindow
- If wall-clock recurrence (TrackingWindow, coaching TimeRange) → leave alone per NG-IDLE/scope

- [ ] **Step 10.2: Verify all tests still pass**

```bash
cargo test --workspace 2>&1 | tail -15
```

- [ ] **Step 10.3: Commit (only if changes made)**

```bash
git add crates/oneshim-api-contracts/src/
git commit -m "refactor(api): sweep remaining absolute-timestamp range pairs to TimeWindow"
```

---

## Task 11: End-to-End Integration Tests

**Estimate:** 2h | **Spec ref:** §8.3 | **Files:** new test file or existing `crates/oneshim-web/tests/`

- [ ] **Step 11.1: Add E2E test for REST → handler → storage flow**

Create or extend `crates/oneshim-web/tests/timewindow_integration.rs`:

```rust
//! E2E test verifying TimeWindow flows correctly through REST → handler → storage layer.

use axum::Router;
use serde_json::Value;

#[tokio::test]
async fn frames_endpoint_with_explicit_window_returns_correct_count() {
    let app = test_app();  // existing fixture
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
    let body: Value = serde_json::from_slice(&hyper::body::to_bytes(response.into_body()).await.unwrap()).unwrap();
    let frames = body.as_array().unwrap();
    assert_eq!(frames.len(), 3, "closed-closed should include both boundaries");
}

#[tokio::test]
async fn delete_range_request_preserves_external_from_to_shape() {
    let app = test_app();
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

    assert_eq!(response.status(), 200);
    // No new "period" key required from frontend
}

#[tokio::test]
async fn invalid_time_window_returns_400() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::get("/api/frames?from=2026-04-25T00:00:00Z&to=2026-04-01T00:00:00Z")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    let body: Value = serde_json::from_slice(&hyper::body::to_bytes(response.into_body()).await.unwrap()).unwrap();
    assert_eq!(body["code"], "time_window.inverted_bounds");
}
```

(Adapt to actual test fixture conventions. Use existing test_app() helper if available.)

- [ ] **Step 11.2: Run E2E**

```bash
cargo test -p oneshim-web --test timewindow_integration 2>&1 | tail -10
```

- [ ] **Step 11.3: Commit**

```bash
git add crates/oneshim-web/tests/timewindow_integration.rs
git commit -m "test(integration): TimeWindow E2E flow + closed-closed boundary + 400 error mapping"
```

---

## Task 12: Documentation + STATUS.md + PHASE-HISTORY

**Estimate:** 1h | **Spec ref:** §9.1 | **Files:** `docs/STATUS.md`, `docs/PHASE-HISTORY.md`

- [ ] **Step 12.1: Run full test suite**

```bash
cargo test --workspace 2>&1 | tail -5
```
Capture count.

- [ ] **Step 12.2: Update STATUS.md**

Update version + Rust test count. Add note about TimeWindow refactor.

- [ ] **Step 12.3: Update PHASE-HISTORY.md**

Add new section after the latest Phase 9 entries:

```markdown
## TimeWindow Primitive Refactor (v0.4.42-rc.1, DATE_OF_MERGE)

- **Consolidated 5 main + 4 supporting divergent absolute-timestamp time-range types** into single canonical `TimeWindow { start: DateTime<Utc>, end: DateTime<Utc> }` primitive at `oneshim-core::types`
- **Closed-closed `[start, end]` semantic** (matches existing SQL BETWEEN, Stripe-style business API pattern; per spec U4)
- **Wall-clock recurrence types unmigrated**: TrackingWindow (PR-A), coaching TimeRange — different domain (recurrence vs absolute window)
- **IdlePeriod unmigrated** (NG7): ongoing idle requires `Option<end_time>` which TimeWindow can't represent without semantic drift
- **Migration scope**: TimeRangeQuery::to_time_window adapter + 6 REST handlers + ~7 SQL storage helpers + FocusMetrics + SessionMetrics + DeleteRangeRequest (custom serde external shape preservation) + ReportQuery
- **2 new wire codes**: time_window.inverted_bounds + time_window.parse_failed (ADR-019 define_code_enum! macro)
- **Tests**: +12 unit tests (TimeWindow primitive) + 6 adapter tests + 3 SQL boundary regression tests + 3 E2E tests
- **External API contract preserved**: REST query strings unchanged; DeleteRangeRequest JSON shape preserved via custom serde
- Spec + plan: `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md` (v3) + `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md`
```

- [ ] **Step 12.4: Commit**

```bash
git add docs/STATUS.md docs/PHASE-HISTORY.md
git commit -m "docs(time-window): STATUS.md + PHASE-HISTORY entry for TimeWindow refactor"
```

---

## Post-Completion Checklist

- [ ] **PC1: Full test suite + lint**

```bash
cargo test --workspace 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt --check
cd crates/oneshim-web/frontend && pnpm lint 2>&1 | tail -5
```
All expected GREEN.

- [ ] **PC2: Wire snapshot + i18n CI**

```bash
cargo test -p oneshim-core --test wire_contract_snapshot
bash scripts/check-wire-error-i18n-coverage.sh
```

- [ ] **PC3: Open PR**

```bash
git push -u origin refactor/timewindow-primitive
gh pr create --title "refactor(time): consolidate divergent time-range types into TimeWindow primitive" \
  --body-file .github/TIMEWINDOW-PR-description.md
```

(Compose PR description from spec §1-§12 summary + commit list.)

---

## Plan Self-Review

### 1. Spec coverage
- §5.1 TimeWindow + TimeWindowError → Task 1
- §5.2 to_time_window adapter → Task 4
- §5.3 SQL helpers → Task 5
- §5.4 FocusMetrics (NG8 Option Z) → Task 9
- §5.5 REST handlers → Task 7
- §5.6 DeleteRangeRequest custom serde → Task 8
- §6 wire codes → Task 2 + Task 3
- §7 error handling → Task 2 (CoreError integration)
- §8 testing → Tasks 1, 4, 6, 11
- §9 commits 1-12 → Tasks 1-12
- §10 migration backward compat → Task 8 custom serde
- §11 open questions → all RESOLVED in spec v3 (Q-8 deferred to PF3)
- §12 risks → addressed via test coverage + ABORT GUARD

### 2. Placeholder scan
- ✅ No "TBD" / "fill in details"
- ✅ All Rust code blocks have full implementation
- ⚠ Task 5 step 5.5/5.6 use `grep` to find OTHER helpers — implementer must enumerate based on actual content

### 3. Type consistency
- `TimeWindow` field names (`start`, `end`) consistent across all tasks
- `TimeWindowError::InvertedBounds` + `ParseFailed` consistent
- `TimeWindowCode::InvertedBounds` + `ParseFailed` matches error variants
- `to_time_window` signature `(&self, default_lookback: Duration) -> Result<TimeWindow, TimeWindowError>` consistent across handlers

### 4. Known gaps
- PR-B1 dependency: hard ABORT GUARD in PF1
- Q-8 baseline count: PF3 captures actual count at impl time

---

## Execution Handoff

**Plan complete and saved to** `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — fresh subagent per task + 2-stage review.

**2. Inline Execution** — executing-plans batch with checkpoints.

(For ralph-loop continuation: Phase 2 plan creation done. Phase 2 deep review next iteration. Phase 3 implementation BLOCKED on PR-B1 #508 merge.)
