# TimeWindow Phase 2 — Iteration 3 Plan v3 Verification

**Date**: 2026-04-25
**Plan v3**: commit `4896e054`, file `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (2420 lines)
**Spec v3**: commit `f495dfbd`
**Findings doc reviewed**: `.claude/timewindow-review/phase2-iter2-verification.md` (6 NEW Critical + 5 NEW Important)
**Reviewer**: independent verifier (NOT Phase 2 iter-2 reviewer)
**Outcome**: **NEEDS PHASE 2 iter-4** — 2 NEW Critical + 1 NEW Important regressions introduced by v3 corrections, despite all 6C+5I from iter-2 being correctly addressed.

---

## VERIFIED FIXED (iter-2 disposition)

### Critical findings (all 6 disposed correctly)

- **N-C1** ✓ — Plan Task 8.1 enumerates exactly 10 FocusMetrics call sites in a numbered table at plan lines 1944-1955. Independent grep `grep -rn "FocusMetrics {\|FocusMetrics::new\|\.period_start\|\.period_end" crates/ src-tauri/ | grep -v "/.git/" | grep -v frontend | grep -v "fn " | grep -v "self\.period_start.*lock"` returns:
  - 1 internal duration use (work_session.rs:317)
  - 1 test fixture (work_session.rs:446)
  - 3 SQL impl sites (focus_metrics.rs:55, 76, 217)
  - 1 internal test (edge_intelligence/tests.rs:76)
  - 1 grpc integration test (grpc_dashboard_integration.rs:461)
  - 3 src-tauri test fixtures (focus_analyzer/mod.rs:384, 420, 442)

  Total = 10. Matches plan's table exactly. False-positive at `input_activity.rs:230` correctly excluded as Note in plan.

- **N-C2** ✓ — Plan Task 4D.1 corrected: regime.rs:44 = `get_entries`, regime.rs:174 = `list_segment_time_ranges`, regime.rs:184 = `get_entries` (re-fetch), with explicit "(NOT flag_noise_range as v2 wrongly stated)" note at plan line 1329. Verified by reading actual `src-tauri/src/scheduler/analysis_pipeline/regime.rs` lines 44, 174, 184.

- **N-C3** ✓ — Plan Task 4D.0 (lines 1280-1297) provides 9-row caller enumeration table covering Service layer (5), Test mock support (5), Internal SQLite tests (15+), web_storage_impl wrappers (5), src-tauri regime.rs (3), src-tauri MockCalibration (1). Sub-task 4C heading explicitly resolves inherent-fn-vs-trait decision: "Decision: change inherent `pub fn` signatures TOO" (plan line 1078). Internal test sites enumerated for events.rs/frames.rs/maintenance.rs.

- **N-C4** ✓ — Plan Task 4A.4 specifies `Result<Vec<(String, TimeWindow)>, CoreError>` (preserves segment_id String), and Task 4D.1 destructures `(seg_id, seg_window)` with `seg_window.contains(e.timestamp)` for closed-closed boundary check. HashMap<String, usize> caller in regime.rs:194 preserved.

- **N-C5** ✓ — Plan Task 4A.2 specifies sync `fn flag_noise_range(&self, window: &TimeWindow) -> Result<u64, CoreError>` (no `async`, returns rows-updated u64). Verified actual port at `crates/oneshim-core/src/ports/calibration_store.rs:24` is `fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<u64, CoreError>` (sync, Result<u64>). Task 4D.2 mock at plan line 1382 also `fn` (not `async fn`).

- **N-C6** ✓ — Plan Task 4B.5 specifies `fn get_daily_active_secs(&self, window: &TimeWindow) -> Result<Vec<(String, i64)>, CoreError>`. Verified port at `web_storage.rs:142` and inherent at `work_sessions.rs:216` both return `Vec<(String, i64)>`. Caller use case (date → active seconds tuples) preserved.

### Important findings (all 5 disposed correctly)

- **N-I1** ✓ — Plan Step 1.11 (the duplicated lib.rs registration) removed. Numbering goes 1.10 → 1.11 (verify compile, marked "*was Step 1.12 — Step 1.11 removed per Phase 2 iter-2 N-I1*") → 1.12 (run tests) → 1.13 (commit). Verified via grep showing no duplicate "pub mod types;" registration step.

- **N-I2** ✓ — Plan line 1807 reads `use crate::common::TimeRangeQuery;` (NOT `use oneshim_api_contracts::common::TimeRangeQuery;`). Same-crate import correctly resolved. No remaining instances of the broken `oneshim_api_contracts::common::TimeRangeQuery` import in the plan.

- **N-I3** ✓ — Plan Step 7.3.5 (lines 1829-1841) explicitly adds `serde_urlencoded = "0.7"` to `[dev-dependencies]` in `crates/oneshim-api-contracts/Cargo.toml`. Step 7.3.6 (line 1843) adds the test that consumes it.

- **N-I4** ✓ — Plan Step 3.1 lines 743-744 show actual struct with `pub limit: Option<usize>` and `pub offset: Option<usize>` (with explicit comment "← usize, NOT u32"). Matches actual common.rs:7-8.

- **N-I5** ✓ — Plan Step 5.2 has 3 explicit boundary test code blocks (lines 1561-1604: `count_frames_in_range_includes_both_boundaries`, `count_events_in_range_includes_both_boundaries`, `delete_data_in_range_respects_delete_flags`). Step 11.3 PHASE-HISTORY draft at line 2279 says "37 new tests total" with explicit breakdown: "13 TimeWindow unit + 3 TimeWindowCode + 8 TimeRangeQuery adapter + 3 SQL boundary regression + 4 E2E + 2 ApiError mapping + 4 api-contracts roundtrip (DeleteRangeRequest×3 + ReportQuery×1)". Math: 13+3+8+3+4+2+4 = 37 ✓. Task 7 tests verified at Steps 7.2 (3 tests) + 7.3.6 (1 test) = 4 ✓.

---

## NEW CRITICAL ISSUES (introduced by v3)

### NEW-C1 — Step 4D.4 FailingStorage snippets call non-existent method `failure_error` AND replace delegation pattern

**Location**: Plan lines 1450-1490 (Step 4D.4 "Update tests/support/failing_storage.rs MockStorage trait impls")

**Plan claim** (line 1458):
```rust
fn count_frames_in_range(&self, _window: &TimeWindow) -> Result<u64, CoreError> {
    Err(self.failure_error("count_frames_in_range"))
}
```

**Reality** (verified at `crates/oneshim-web/tests/support/failing_storage.rs:276-279`):
```rust
fn count_frames_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError> {
    self.inner
        .count_frames_in_range(from, to)
        .map_err(Into::into)
}
```

Two compile-blocking errors:
1. **`self.failure_error(...)` does not exist** on `FailingStorage`. `grep -nE "fn failure_error|failure_error\(" crates/oneshim-web/tests/support/failing_storage.rs` returns zero hits. The struct only has `pub fn new(inner)` and `pub fn with_fail_start_idle(mut self)` (verified at lines 56-67 of that file). Also `_window` (underscore-prefixed unused param) would emit `unused_variables` warning if any caller depended on the window value.
2. **Delegation pattern broken**: `FailingStorage` is documented as "Wraps `SqliteStorage` and injects configurable faults on specific methods. **All other methods delegate to the inner `SqliteStorage`.**" (file header lines 1-3 + struct doc line 47-50). The plan's snippet replaces the delegation with unconditional `Err(...)` — this would break any test calling `count_frames_in_range` against `FailingStorage` because it now fails 100% of the time instead of delegating to real SQLite. There is no `fail_count_frames` flag analogous to `fail_start_idle`; the only injection point is `start_idle_period`.

**Impact**: Subagent following plan literally will:
- Fail compile on `failure_error` (undefined method)
- After fix-up to delete the unconditional Err, would still need to KEEP the delegation pattern, only changing the parameter type (`from: &str, to: &str` → `window: &TimeWindow`) and the inner call (e.g., `self.inner.count_frames_in_range(window)`).

**Fix recommendation**: Replace all 5 Step 4D.4 snippets with the delegation pattern matching existing code:
```rust
fn count_frames_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError> {
    self.inner.count_frames_in_range(window).map_err(Into::into)
}
fn list_frame_file_paths_in_range(&self, window: &TimeWindow) -> Result<Vec<String>, CoreError> {
    self.inner.list_frame_file_paths_in_range(window).map_err(Into::into)
}
fn count_events_in_range(&self, window: &TimeWindow) -> Result<u64, CoreError> {
    self.inner.count_events_in_range(window).map_err(Into::into)
}
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
    self.inner.delete_data_in_range(
        window, delete_events, delete_frames, delete_metrics, delete_processes, delete_idle,
    ).map_err(Into::into)
}
fn get_daily_active_secs(&self, window: &TimeWindow) -> Result<Vec<(String, i64)>, CoreError> {
    self.inner.get_daily_active_secs(window).map_err(Into::into)
}
```

Note: Since the inner type is `Arc<SqliteStorage>` and `SqliteStorage` inherent fns now also accept `&TimeWindow` (per Sub-task 4C), the delegation passes through cleanly without conversion.

### NEW-C2 — Step 4D.1 + 4D.3 use `?` operator in functions returning non-Result types

**Location**: Plan lines 1310-1335 (Step 4D.1 regime.rs), 1392-1399 (Step 4D.3 stats_query_support.rs)

**Plan claim** (line 1315 — regime.rs:44):
```rust
let window = TimeWindow::new(lookback, now)?;
match reader.get_entries(&window, true).await {
```

And (line 1324 — regime.rs:174):
```rust
let window = TimeWindow::new(lookback, now)?;
let segment_ranges = match ts.calibration_reader.list_segment_time_ranges(&window).await {
```

And (line 1397 — stats_query_support.rs:112):
```rust
let window = TimeWindow::from_rfc3339_pair(&from_rfc, &to_rfc)?;
match ctx.storage.get_daily_active_secs(&window) {
```

**Reality**: The containing functions return `()` or `u64`, NOT `Result<_, _>`:
- `pub(in crate::scheduler) async fn run_periodic_regime_detection(...)` (regime.rs:16-19) returns `()`. Contains line 44.
- `async fn run_constrained_clustering(...)` (regime.rs:140-144) returns `()`. Contains lines 174 and 184.
- `pub(crate) fn total_active_secs_for_range(...) -> u64` (stats_query_support.rs:104-109) returns `u64`. Contains line 112.

The `?` operator requires the enclosing function to return a type implementing `FromResidual<Result<Infallible, E>>` — typically `Result<_, _>`. None of these three functions do.

**Impact**: `cargo check --workspace` after Step 4D.5 will fail with errors like:
```
error[E0277]: the `?` operator can only be used in a function that returns `Result` or `Option`
  --> src-tauri/src/scheduler/analysis_pipeline/regime.rs:44
```

Three call sites would block compile. Subagent must fix these inline.

**Fix recommendation**: Replace `?` with `.expect(...)` in trusted-construction call sites where `lookback < now` is invariant by construction:

For regime.rs:44:
```rust
let window = TimeWindow::new(lookback, now)
    .expect("lookback = now - 7 days is < now by construction");
```

For regime.rs:174 (and reuse at 184):
```rust
let window = TimeWindow::new(lookback, now)
    .expect("lookback = now - 7 days is < now by construction");
```

For stats_query_support.rs:112 (where `from_rfc`/`to_rfc` come from caller-validated DateTime<Utc>):
```rust
let Ok(window) = TimeWindow::from_rfc3339_pair(&from_rfc, &to_rfc) else {
    return fallback_events_logged * 5;  // existing fallback semantics
};
match ctx.storage.get_daily_active_secs(&window) {
```

(Or alternatively: change fn signature to return `Result<u64, _>` — but that ripples to callers, more invasive.)

Also, note that **events_service.rs:35** (Step 4D.3 line 1432) is OK because the containing `pub async fn get_events(...) -> Result<EventPage, ApiError>` returns Result, and the `?` chain works through `From<TimeWindowError> -> CoreError -> ApiError`.

And **data_web_service.rs:36 + 51** (Step 4D.3 lines 1406, 1419) are OK because the containing function returns `Result<DeleteResult, ApiError>`.

And **reports_query_support.rs:86** (Step 4D.3 line 1441) uses `if let Ok(window)` — safe.

So the bug is specific to the 3 sites listed above (regime.rs ×2, stats_query_support.rs ×1).

---

## NEW IMPORTANT ISSUES (introduced by v3)

### NEW-I1 — Step 4D.0 enumeration table omits 5 internal `calibration_store_impl.rs` test callers

**Location**: Plan Step 4D.0 enumeration table (lines 1288-1298)

The enumeration table lists callers per category but does NOT include the 5 internal `calibration_store_impl.rs` test sites that call `storage.get_entries(...)` and `storage.flag_noise_range(...)`:
- `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:400` — `storage.get_entries(from, to, false)`
- `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:414` — `storage.flag_noise_range(from, to)`
- `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:420` — `storage.get_entries(wide_from, wide_to, true)`
- `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:425` — `storage.get_entries(wide_from, wide_to, false)`
- `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:443` — `storage.get_entries(from, to, false)`

When the trait sigs change in Sub-task 4A and impl changes in 4C.1, these test callers must also be updated. Plan implicitly covers them via Step 4C.1 (which migrates the impl + says "existing query body unchanged"), but doesn't explicitly enumerate the test sites.

**Impact**: Subagent reading Step 4D.0 table literally would not realize calibration_store_impl.rs has 5 internal test sites. After Sub-task 4A + 4C.1, `cargo test -p oneshim-storage --lib calibration_store_impl` would fail with 5 sig mismatches.

The plan's Step 4D.5 closing note "If errors remain, additional caller sites exist beyond the 30 enumerated in 4D.0 — grep them out and migrate" is the catch-all, but this specific case is high-frequency enough to enumerate explicitly.

**Fix recommendation**: Add a row to Step 4D.0 enumeration table:
```
| Internal calibration_store_impl tests | lines 400, 414, 420, 425, 443 | 5 |
```

And add a `Step 4C.1.1` immediately after Step 4C.1 (line 1119) that says:
```
Update 5 internal test sites in `calibration_store_impl.rs` (lines 400, 414, 420, 425, 443):
- Replace `storage.get_entries(from, to, ...)` → build `let window = TimeWindow::new(from, to).unwrap();` then `storage.get_entries(&window, ...)`.
- Replace `storage.flag_noise_range(from, to)` → `storage.flag_noise_range(&window)`.
```

---

## VERDICT

**NEEDS PHASE 2 iter-4**

**Concise rationale**: All 6 NEW Critical + 5 NEW Important findings from Phase 2 iter-2 were correctly addressed in plan v3. Each disposition entry maps to a verified plan section. However, v3 introduced **2 NEW Critical compile-blocking issues**:

1. **NEW-C1**: Step 4D.4 FailingStorage snippets reference non-existent `self.failure_error(...)` method AND replace the documented delegation pattern with unconditional `Err(...)` returns. 5 call sites would not compile, AND the test semantics would be broken (FailingStorage would fail-100% on these 5 methods instead of delegating).

2. **NEW-C2**: Steps 4D.1 (3 sites in regime.rs) and 4D.3 (1 site in stats_query_support.rs) use `?` operator in functions whose return types are `()` or `u64` (not Result). `cargo check` would fail at these 4 specific sites with E0277.

Both issues are factually wrong about the surrounding source code (calling methods that don't exist; using `?` in non-Result-returning functions) and would block the implementer at Sub-task 4D. Fix is mechanical (~15 min plan editing): replace 5 FailingStorage snippets with the delegation pattern, and replace 4 `?` uses with `.expect("...")` or `if let Ok(window)` patterns.

1 NEW Important issue (NEW-I1) — Step 4D.0 enumeration omits 5 calibration_store_impl test callers. Catchable by the plan's own "If errors remain..." closing note in Step 4D.5, but explicit enumeration would prevent confusion.

**Recommended Phase 2 iter-4 actions:**

1. **Fix NEW-C1**: Rewrite all 5 FailingStorage snippets in Step 4D.4 to use delegation: `self.inner.<method>(window).map_err(Into::into)`. Note the plan text says "Update tests/support/failing_storage.rs MockStorage trait impls (5 sites)" — this is a delegating wrapper (B3-7 selective fault injection), NOT a pure mock. Inner type is `Arc<SqliteStorage>`. Since inherent fns also accept `&TimeWindow` after Sub-task 4C, delegation passes through cleanly without conversion.

2. **Fix NEW-C2**: Replace `?` with `.expect(...)` (trusted construction) in:
   - Step 4D.1 line 1315 (regime.rs:44 `let window = TimeWindow::new(lookback, now)?` → `.expect("lookback < now by construction")`)
   - Step 4D.1 line 1324 (regime.rs:174 same fix)
   - Step 4D.3 line 1397 (stats_query_support.rs:112 — use `let Ok(window) = ... else { return fallback_events_logged * 5; }` to preserve the existing fallback semantics)

3. **Fix NEW-I1**: Add row to Step 4D.0 enumeration table for `calibration_store_impl.rs:400, 414, 420, 425, 443` (5 internal test sites). Optionally add Step 4C.1.1 for explicit migration guidance.

4. **Re-verify the resulting plan v4 has zero new Critical/Important issues.**

5. **Then declare Phase 2 EXIT** and Phase 3 BLOCKED on PR-B1 #508.

The original 6C+5I disposition is sound. The implementation churn from these new findings is approximately +15-20 minutes of plan editing (no architectural changes, just snippet corrections). After iter-4 fixes, plan v4 should be the final v3-quality plan ready for hand-off.

**Risk if implemented as-is**:
- Subagent stalls at Step 4D.4 (FailingStorage `failure_error` undefined → 5 compile errors → confusion about delegation pattern)
- Subagent stalls at Step 4D.5 `cargo check --workspace` (4 `?` operator errors in non-Result functions → manual fixes needed at 4 specific sites)
- Total wall time loss: ~1-2 hours of debugging + fixup, similar order to iter-2 stall risk.

---

**End of Phase 2 iter-3 verification.**
