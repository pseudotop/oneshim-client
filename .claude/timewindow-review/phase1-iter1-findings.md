# TimeWindow Phase 1 — Iteration 1 Spec Review Findings

**Date**: 2026-04-25
**Spec**: v1 (commit `1c38cabd`)
**Reviewers**: code-reviewer (sonnet) + Explore (feasibility)
**Outcome**: 4 Critical + 5 Important + 4 Nice-to-have

---

## CRITICAL (block Phase 2)

### C1 — Wire code count baseline wrong (42 not 51)

Spec §7.2/§8.4 assumes "current 51 (post-PR-B2)" but worktree base `2ba38cf5` is PRE-PR-B1. Actual snapshot has **42 codes**.

**Fix**: Spec must use 42 baseline. Total post-PR = 42 + 2 = **44**, not 53. Update §7.2 + §8.4.

### C2 — `TimeWindowError → ApiError` conversion missing

Spec §6.1 uses `q.to_time_window(...)?` in Axum handlers returning `Result<_, ApiError>`. Requires `From<TimeWindowError> for ApiError`. Spec doesn't specify this.

**Fix**: Make `TimeWindowError` a variant of `CoreError` (cleanest — integrates with existing `From<CoreError> for ApiError` + ADR-019 `code()` method). Update §5.1.

### C3 — `TimeWindowCode` not registered in `all_codes()` aggregator

Spec §7.2 creates new `error_codes/time_window.rs` but doesn't mention 3 required wire-up steps in `error_codes/mod.rs`:
1. `pub mod time_window;`
2. `pub use time_window::TimeWindowCode;`
3. Add `for c in TimeWindowCode::all() { codes.push(c.as_str()); }` to `all_codes()`

**Fix**: Explicit instruction in §9.1 commit 2.

### C4 — `to_time_window(self, ...)` consumes — breaks 6 service sites

Spec §5.2 defines consuming `to_time_window(self, ...)`. Reality: 6 service methods take `&TimeRangeQuery` and call methods on the reference. Consuming method requires every caller to clone.

**Fix**: Change to `to_time_window(&self, default_lookback: Duration) -> Result<TimeWindow, TimeWindowError>`. Need to clone `Option<String>` fields internally — cheap. Update §5.2.

---

## IMPORTANT (must address)

### I1 — Q-1 (FocusMetrics serde) is moot — Option Z safe

`FocusMetrics` is domain model in oneshim-core. REST serializes `FocusMetricsDto` (in `oneshim-api-contracts/src/focus.rs`) which has `date: String` + scalars — NO `period_start`/`period_end` fields. Assembler discards those. **Frontend has zero references to `period_start`/`period_end`**.

**Fix**: Resolve Q-1 with **Option Z** (break internal model JSON shape — frontend unaffected). Saves 3h of Option Y custom serde work in §9.1 commit 9. Update §5.4 + Q-1.

### I2 — Q-3 (ReportQuery shape) explicit answer

`ReportQuery { period: ReportPeriod (enum: Week/Month/Custom), from: Option<String>, to: Option<String> }`. `period` is primary; `from/to` only meaningful when `period == Custom`.

**Fix**: Resolve Q-3 with: `ReportQuery { period: ReportPeriod, window: Option<TimeWindow> }`. Update §4.1 + §5 + Q-3.

### I3 — `DeleteRangeRequest` frontend impact contradicts NG4

NG4 says "Frontend type changes — TypeScript types unchanged". §10.1 admits "Frontend update needed" for `DeleteRangeRequest`. Verified: `crates/oneshim-web/frontend/src/pages/privacy-page/DataSection.tsx` references `delete-range`.

**Fix**: Either:
- (a) Add DataSection.tsx update to scope, remove NG4 contradiction
- (b) Preserve external JSON shape via custom serde (rename start→from, end→to)
- (c) Exclude DeleteRangeRequest from migration

Recommend **(b)** — minimal frontend churn + preserves API contract. Or **(a)** — ~30min DataSection update is small.

**Update**: Resolve in §5.6 + §10.1 + NG4. Choose (a) or (b).

### I4 — Q-2 (IdlePeriod) option (b) creates bug

`IdlePeriod` has `end_time: Option<DateTime<Utc>>` for ongoing idle. Spec proposes (b) `end = now()` — but value drifts every poll, breaks equality + stable serialization.

**Fix**: Resolve Q-2 with **(c) DON'T migrate IdlePeriod**. Add NG-7. Update Q-2 + §2.2 + §4.1 (remove IdlePeriod from modified files).

### I5 — `pub mod types;` registration in `lib.rs` missing from spec

Spec doesn't mention adding `pub mod types;` to `crates/oneshim-core/src/lib.rs`. Without it, `oneshim_core::types::TimeWindow` import fails.

**Fix**: Add explicit step in §9.1 commit 1 + §4.1 file list.

---

## NICE-TO-HAVE

### N1 — Drop `Hash` derive

`TimeWindow` derives `Hash` but no use case. Confusing for readers (suggests dedup). **Fix**: Remove `Hash` from §5.1 derive list unless use case identified.

### N2 — Missing tests

- `from_rfc3339_pair` with `Z` suffix variant (RFC3339 alternative to `+00:00`)
- `TimeWindowError::InvertedBounds.code()` returning correct wire code per ADR-019

**Fix**: Add to §8.1.

### N3 — `flag_noise_range` port trait change

Port at `crates/oneshim-core/src/ports/calibration_store.rs` defines `flag_noise_range(from: DateTime<Utc>, to: DateTime<Utc>)`. Migration changes both port trait + impl. **Spec only mentions impl file**.

**Fix**: Add port file to §4.1 modified list. Note in §5.3 that port trait sig changes too.

### N4 — `rusqlite::params!` macro vs slice consistency

§5.3 example uses `[&from, &to]` slice. Codebase uses `params![]` macro. **Fix**: Standardize on `params![]` for consistency in §5.3 + all 7+ migrated helpers.

---

## Discrepancies (count corrections)

- "8+ handlers using TimeRangeQuery" — actual 6-7 (frames, focus, metrics, idle, events, processes, possibly mod.rs)
- "10+ SQL helpers" — actual 7-9 (events, frames, calibration, web_storage_impl, maintenance)
- These don't change scope materially but spec should be precise.

---

## Phase 1 iter-2 Plan

After v2 spec:
1. Fresh subagent re-reviews
2. Verify all Critical + Important fixes
3. If clean → Phase 2 (writing-plans)
