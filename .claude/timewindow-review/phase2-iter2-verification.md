# TimeWindow Phase 2 — Iteration 2 Plan v2 Verification

**Date**: 2026-04-25
**Plan v2**: commit `314115b6`, file `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (2074 lines)
**Spec v3**: commit `f495dfbd`
**Findings doc reviewed**: `.claude/timewindow-review/phase2-iter1-findings.md` (9 Critical + 11 Important)
**Reviewer**: independent verifier (NOT original Phase 2 iter-1 reviewer)
**Outcome**: **NEEDS PHASE 2 iter-3** — 6 NEW Critical + 5 NEW Important issues found alongside generally-correct application of the original 9C+11I fixes.

---

## VERIFIED FIXED

- **C1**: ✓ Task 1 Step 1.8 uses `CoreError::TimeWindow { code: TimeWindowCode, message: String }` struct-variant + manual `From<TimeWindowError>` impl. Display template `"Time window error [{code}]: {message}"` matches the existing `Storage`/`Network` pattern at `crates/oneshim-core/src/error.rs:14, 16, 49, 56, 62, 120` (verified). Pattern is consistent with ADR-019 §4.6 majority struct-variant convention.
- **C2**: ✓ Task 1 Step 1.9 adds explicit `CoreError::TimeWindow { message, .. } => ApiError::BadRequest(message)` arm BEFORE the wildcard `other => ApiError::Internal` (verified arm exists at `crates/oneshim-web/src/error.rs:92`). Includes 2 regression tests: `time_window_inverted_bounds_maps_to_bad_request` + `time_window_parse_failed_maps_to_bad_request`.
- **C3**: ✓ Task 10.1 E2E asserts `body["error"]` substring (NOT `body["code"]`) and `body["status"] == 400`. Schema check matches actual `oneshim_api_contracts::error::ErrorResponse` shape `{ error: String, status: u16 }` (verified at `crates/oneshim-api-contracts/src/error.rs:4-6`).
- **C4**: ✓ Task 3.1 adds `Default` to `TimeRangeQuery` derive list. All field types (`from`, `to`, `limit`, `offset`, `min_importance`) are `Option<T>` so Default derive is safe (verified — current struct at `crates/oneshim-api-contracts/src/common.rs:4-10`).
- **C5**: ✓ Task 3.3 uses `Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()` helper via `dt()` function (NOT hand-computed Unix timestamps). All `assert_eq!` use `dt(...)` consistently.
- **C6**: ✓ Task 4 Sub-task 4A enumerates 3 calibration_store methods (`flag_noise_range`, `get_entries`, `list_segment_time_ranges`). Sub-task 4B enumerates 5 web_storage methods (`count_frames_in_range`, `list_frame_file_paths_in_range`, `count_events_in_range`, `delete_data_in_range`, `get_daily_active_secs`). Sub-task 4D updates 3 regime.rs callers + 1 MockCalibration. **PARTIAL CREDIT** — see N-C2 (regime.rs callers misidentified) and N-C3 (additional callers not enumerated).
- **C7**: ✓ Task 4B.4 + 4C.4 explicitly preserve the 5 boolean flag params on `delete_data_in_range` (`delete_events`, `delete_frames`, `delete_metrics`, `delete_processes`, `delete_idle`). Only `from`/`to` → `&TimeWindow` (verified actual signature at `crates/oneshim-storage/src/sqlite/maintenance.rs:286-294`).
- **C8**: ✓ Task 2.3 instructs grep for `toHaveLength` and updates BOTH assertions (lines 30 + 122). Verified both `expect(registry).toHaveLength(42)` (line 30) and `expect(translatedCodes('en')).toHaveLength(42)` (line 122) exist with matching D7 comments.
- **C9**: ✓ Task 7.1 keeps `from: String, to: String` fields untouched on `DeleteRangeRequest` and adds `period() -> Result<TimeWindow, TimeWindowError>` accessor (Option C). NO custom serde module. Roundtrip test `delete_range_request_external_shape_preserved` verifies field preservation.
- **I1**: ✓ Task 8.5 notes SessionMetrics may be dead code; migrating for consistency. Verified that `oneshim_core::models::telemetry::SessionMetrics` has no production callers (only `oneshim_analysis::SessionMetrics` is used in workspace).
- **I2**: ✓ Task 8 Step 8.1 enumerates 4 FocusMetrics call sites and prescribes Result-returning constructor. **PARTIAL CREDIT** — see N-C1 (additional call sites not enumerated).
- **I3**: ✓ Task 6.0 confirms 6 actual handlers via `grep -rn "TimeRangeQuery" crates/oneshim-web/src/handlers/`. Verified 6 handler files: events.rs, focus.rs, frames.rs, idle.rs, metrics.rs, processes.rs.
- **I4**: ✓ PF5 includes grep verification `grep -E "^oneshim-core\s*=" crates/oneshim-api-contracts/Cargo.toml`. Verified — dep exists.
- **I5**: ✓ Tasks merged: ApiError mapping in Task 1.9 (same commit as core integration), bridging Tasks 2 and 7.
- **I6**: ✓ Post-Completion guidance specifies single PC1 clippy run (NOT per-task), referencing memory `feedback_lefthook_clippy_cost.md`.
- **I7**: ✓ PF3 + Task 1.10 show alphabetical block sample with `time_window` < `tracking_schedule` ordering note.
- **I8**: ✓ (nit, no plan change required — Copy threshold acknowledgment).
- **I9**: ✓ Tasks 1+2 of v1 MERGED into single Task 1 of v2. Verified: there is no separate "Task 2" doing what old Task 2 did. Plan v2 has Task 2 = i18n translations (different responsibility).
- **I10**: ✓ Commit messages updated: `feat(core)`, `feat(api-contracts)`, `refactor(web-handlers)`, `refactor(storage)`, `refactor(core)`. Verified consistent with recent main commits (`feat(audit)`, `refactor(external-grpc-tests)`, `feat(tracking-schedule)`, etc.) at `git log --oneline -50 origin/main`.
- **I11**: ✓ ReportQuery uses `#[serde(flatten)] time_range: TimeRangeQuery`, NOT `Option<TimeWindow>` field. Plan Step 7.3 explicitly justifies why flatten works for struct-typed fields (unlike C9's invalid `flatten + with` combo).

---

## NEW CRITICAL ISSUES

### N-C1 — FocusMetrics call site enumeration is incomplete; plan severely undercounts call sites

Plan Task 8 Step 8.1 enumerates 4 FocusMetrics call sites but the actual count is **6+ Rust call sites**:

**Missing from plan:**
- `src-tauri/src/focus_analyzer/mod.rs:384-386` (test fixture using struct literal `FocusMetrics { period_start: now, period_end: now + Duration::hours(8), ... }`)
- `src-tauri/src/focus_analyzer/mod.rs:420-422` (same pattern)
- `src-tauri/src/focus_analyzer/mod.rs:442-444` (same pattern)
- `crates/oneshim-core/src/models/work_session.rs:446` (test using `FocusMetrics::new(now, now + chrono::Duration::hours(1))`)
- `crates/oneshim-core/src/models/work_session.rs:317` (`(self.period_end - self.period_start).num_seconds()` — needs `self.period.duration().num_seconds()`)

If the implementer relies on the plan's enumeration without re-grep, they will miss these 5 sites and the workspace will fail to compile.

**Fix recommendation**: Update Step 8.1 enumeration to add a more thorough grep:
```bash
grep -rn "FocusMetrics {\|FocusMetrics::new\|\.period_start\|\.period_end" crates/ src-tauri/ | grep -v "/.git/" | grep -v frontend
```

This would have surfaced all 6+ sites including the 3 in src-tauri/src/focus_analyzer/mod.rs.

### N-C2 — regime.rs caller enumeration is wrong (lines and methods misidentified)

Plan Task 4D Step 4D.1 enumerates 3 regime.rs callers:
- regime.rs:44 → `get_entries` ✓ (verified)
- regime.rs:174 → `list_segment_time_ranges` ✓ (verified)
- regime.rs:184 → `flag_noise_range` ✗ **WRONG** — line 184 is actually a SECOND `get_entries` call (verified by `sed -n '180,195p' src-tauri/src/scheduler/analysis_pipeline/regime.rs`)

There are **0 regime.rs callsites for `flag_noise_range`** — it's only used in mocks (`crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:414` test). And there are **2 callsites for `get_entries` in regime.rs** (lines 44 + 184), not 1.

**Fix recommendation**: Update Step 4D.1 to:
```
- src-tauri/src/scheduler/analysis_pipeline/regime.rs:44 → get_entries (build TimeWindow)
- src-tauri/src/scheduler/analysis_pipeline/regime.rs:174 → list_segment_time_ranges
- src-tauri/src/scheduler/analysis_pipeline/regime.rs:184 → get_entries (re-fetch for index mapping)
```

Remove the spurious `flag_noise_range` regime.rs claim; instead note that `flag_noise_range` is only used in calibration_store_impl test fixtures.

### N-C3 — Service layer + tests/support/ + inherent fn callers not enumerated; broken lockstep migration

Plan Task 4 enumerates port-trait + impl + 4 caller sites in src-tauri. **It misses 9+ additional caller sites** that will break when the port trait signatures change:

**Service layer callers (4 sites):**
- `crates/oneshim-web/src/services/stats_query_support.rs:112` → `get_daily_active_secs(&from_rfc, &to_rfc)`
- `crates/oneshim-web/src/services/data_web_service.rs:36` → `list_frame_file_paths_in_range(&request.from, &request.to)`
- `crates/oneshim-web/src/services/data_web_service.rs:51` → `delete_data_in_range(...)`
- `crates/oneshim-web/src/services/events_service.rs:35` → `count_events_in_range(&from.to_rfc3339(), &to.to_rfc3339())`
- `crates/oneshim-web/src/services/reports_query_support.rs:86` → `get_daily_active_secs(&from_rfc, &to_rfc)`

**Test mock support (5 sites):**
- `crates/oneshim-web/tests/support/failing_storage.rs:278` → `count_frames_in_range(from, to)`
- `crates/oneshim-web/tests/support/failing_storage.rs:301` → `list_frame_file_paths_in_range(from, to)`
- `crates/oneshim-web/tests/support/failing_storage.rs:333` → `count_events_in_range(from, to)`
- `crates/oneshim-web/tests/support/failing_storage.rs:371` → `delete_data_in_range(...)`
- `crates/oneshim-web/tests/support/failing_storage.rs:403` → `get_daily_active_secs(from, to)`

**Inherent fn callers (4 internal storage tests):**
- `crates/oneshim-storage/src/sqlite/events.rs:406, 426, 452, 471` → `count_events_in_range(&from, &to)`
- `crates/oneshim-storage/src/sqlite/frames.rs:175, 192` → `count_frames_in_range(&from, &to)`
- `crates/oneshim-storage/src/sqlite/maintenance.rs:931, 1019, 1052, 1067, 1083` → `delete_data_in_range(...)` and `count_events_in_range`

If the implementer follows the plan literally, `cargo check --workspace` after Task 4D.3 will fail with 9+ unresolved sig mismatches in service + test layers. The plan's note "If errors remain, additional caller sites exist beyond the 3 enumerated — grep them out and migrate" is true but understated — it's not "additional", it's **the majority of callers**.

**Fix recommendation**: Add an explicit Sub-task 4D.0:
```bash
grep -rn "count_events_in_range\|count_frames_in_range\|list_frame_file_paths_in_range\|delete_data_in_range\|get_daily_active_secs" crates/ src-tauri/ | grep -v "fn "
```

Enumerate ALL hits (currently 30+ as of `grep -c`) and add to plan: services/, tests/support/, sqlite/{events,frames,maintenance}/test sections.

Also clarify the **inherent fn vs port-trait** decision: does the inherent `pub fn count_events_in_range(&self, from: &str, to: &str)` on `SqliteStorage` (events.rs:14, frames.rs:10, maintenance.rs:253, work_sessions.rs:216) keep its `&str, &str` signature with the trait wrapper doing conversion, OR change too? Plan is silent. If trait sig changes but inherent stays `&str, &str`, the wrapper has impedance mismatch (must call `window.to_sql_pair()` then re-pass strings). This is workable but the plan must specify it.

### N-C4 — Plan misstates `list_segment_time_ranges` return type; proposed `Vec<TimeWindow>` would break caller

Plan Task 4A.4 says:
```rust
// Before:
async fn list_segment_time_ranges(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>, CoreError>;
```

**Reality** (verified at `crates/oneshim-core/src/ports/calibration_store.rs:50-55`):
```rust
async fn list_segment_time_ranges(
    &self,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<(String, DateTime<Utc>, DateTime<Utc>)>, CoreError>
```

The actual return is a **3-tuple `(String, DateTime<Utc>, DateTime<Utc>)`** where the String is the segment_id — NOT a 2-tuple as the plan claims.

The plan's proposed change `Result<Vec<TimeWindow>, CoreError>` would **drop the segment_id**. The caller at `src-tauri/src/scheduler/analysis_pipeline/regime.rs:194` destructures `(seg_id, seg_start, seg_end)` and uses `seg_id` for `feature_indices: HashMap<String, usize>` keys. If segment_id is dropped, the entire feature mapping pipeline breaks.

**Fix recommendation**: Change Step 4A.4 to:
```rust
// Before (actual):
async fn list_segment_time_ranges(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<(String, DateTime<Utc>, DateTime<Utc>)>, CoreError>;
// After:
async fn list_segment_time_ranges(&self, window: &TimeWindow) -> Result<Vec<(String, TimeWindow)>, CoreError>;
```

Then the regime.rs:194 destructure becomes `(seg_id, segment_window)` and `seg_start`/`seg_end` are accessed via `segment_window.start`/`segment_window.end`. Caller's HashMap key (segment_id) is preserved.

The plan's note "(Optional refinement — return `Vec<TimeWindow>` instead of tuple list for consistency. Adjust if call-sites expect raw tuples.)" hints at uncertainty but doesn't resolve it. Must be resolved before implementation, not deferred to "adjust if".

### N-C5 — Plan misstates `flag_noise_range` return type

Plan Task 4A.2 says:
```rust
// Before:
async fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<(), CoreError>;
```

**Reality** (verified at `crates/oneshim-core/src/ports/calibration_store.rs:24`):
```rust
fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<u64, CoreError>;
```

Two errors:
1. Return is `Result<u64, ...>` (number of rows updated), NOT `Result<(), ...>`.
2. Method is **synchronous** (no `async`). It's on `CalibrationWriter` which doesn't use `#[async_trait]`.

If the implementer follows the plan, they will:
- Write the trait sig as `async fn ... -> Result<(), CoreError>` — wrong return type
- Add `#[async_trait]` to `CalibrationWriter` (currently sync) — would force async on `log_batch` too, cascading API churn

**Fix recommendation**: Update Step 4A.2 to reflect reality:
```rust
// Before (actual):
fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<u64, CoreError>;
// After:
fn flag_noise_range(&self, window: &TimeWindow) -> Result<u64, CoreError>;
```

Note CalibrationWriter is sync, NOT async.

### N-C6 — Plan misstates `get_daily_active_secs` return type

Plan Task 4B.5 says:
```rust
// Before:
fn get_daily_active_secs(&self, from: &str, to: &str) -> Result<u64, CoreError>;
```

**Reality** (verified at `crates/oneshim-core/src/ports/web_storage.rs:142` `ActivityStatsStorage`):
```rust
fn get_daily_active_secs(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>, CoreError>;
```

Return is `Vec<(String, i64)>` (date string + active seconds tuples for daily aggregation), NOT `u64`.

If the implementer follows the plan, they'd write `fn get_daily_active_secs(&self, window: &TimeWindow) -> Result<u64, CoreError>` and the impl would not compile against the existing inherent `pub fn` at `crates/oneshim-storage/src/sqlite/edge_intelligence/work_sessions.rs:216` which returns `Vec<(String, i64)>`.

**Fix recommendation**: Update Step 4B.5 to reflect reality:
```rust
// Before (actual):
fn get_daily_active_secs(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>, CoreError>;
// After:
fn get_daily_active_secs(&self, window: &TimeWindow) -> Result<Vec<(String, i64)>, CoreError>;
```

---

## NEW IMPORTANT ISSUES

### N-I1 — Plan duplicates `pub mod types;` registration in lib.rs (Steps 1.3 + 1.11)

Plan Task 1 Step 1.3 (line 325) instructs:
> Open `crates/oneshim-core/src/lib.rs`. Find existing `pub mod` declarations (alphabetical or grouped). Add: `pub mod types;`

And Plan Task 1 Step 1.11 (line 621) instructs:
> Open `crates/oneshim-core/src/lib.rs`. Find the existing `pub mod` declarations block and add (alphabetical position): `pub mod types;`

Both steps add the EXACT same line. This is redundant — Step 1.11 should be removed. Subagent reading the plan literally would either:
- Try to add the same line twice (rustc error: "duplicate definition of types")
- Get confused about why Step 1.11 exists, potentially skip Step 1.3

**Fix recommendation**: Remove Step 1.11 entirely. Step 1.3 already accomplishes the registration. Update Step 1.12 step number references accordingly.

### N-I2 — Plan Step 7.3 same-crate import path is wrong

Plan Step 7.3 instructs adding `use oneshim_api_contracts::common::TimeRangeQuery;` to `crates/oneshim-api-contracts/src/reports.rs`. This is a **same-crate import** — it would fail to compile because crates can't import themselves via their crate name.

The correct path is `use crate::common::TimeRangeQuery;` (or equivalently `use super::common::TimeRangeQuery;` if the file is at the same level as `common.rs`).

**Fix recommendation**: Change line 1499 from:
```rust
use oneshim_api_contracts::common::TimeRangeQuery;
```
To:
```rust
use crate::common::TimeRangeQuery;
```

### N-I3 — Plan Step 7.3 test uses `serde_urlencoded` but doesn't add it as dev-dependency

Plan Step 7.3 includes a test that calls `serde_urlencoded::from_str(raw)`. The plan parenthetically says "(Add `serde_urlencoded` to dev-dependencies if not already present.)" but doesn't include the actual `Cargo.toml` modification step.

Verified: `serde_urlencoded` is **NOT** in `crates/oneshim-api-contracts/Cargo.toml` dev-dependencies. Without it, the test won't compile.

**Fix recommendation**: Add Step 7.3.5:
```bash
# Verify serde_urlencoded dev-dep
grep -E "^serde_urlencoded" crates/oneshim-api-contracts/Cargo.toml
# If empty, add to [dev-dependencies]:
echo 'serde_urlencoded = "0.7"' >> crates/oneshim-api-contracts/Cargo.toml
```

Or use `serde_json::from_value` with a parsed dict instead of `serde_urlencoded`.

### N-I4 — `TimeRangeQuery::limit`/`offset` types misstated in plan

Plan Step 3.1 shows the existing struct as:
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

**Reality** (verified at `crates/oneshim-api-contracts/src/common.rs:5-11`):
```rust
pub struct TimeRangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<usize>,    // ← usize, not u32
    pub offset: Option<usize>,   // ← usize, not u32
    pub min_importance: Option<f64>,
}
```

Plan Step 3.3 test `to_time_window_takes_ref_so_caller_keeps_other_fields` uses `limit: Some(50)` which works for both `Option<u32>` and `Option<usize>` (50 fits both literal-inference cases). So this discrepancy doesn't break the tests, but it indicates the plan author misread the source. Could lead to confusion if Future tests use values like `i32::MAX as u32`.

**Fix recommendation**: Update Step 3.1 to show actual `Option<usize>` types. No test changes needed.

### N-I5 — Test count summary misalignment

Step 11.3 PHASE-HISTORY draft says "+13 TimeWindow unit + 3 TimeWindowCode + 8 TimeRangeQuery adapter + 3 SQL boundary regression + 4 E2E + 2 ApiError mapping (~33 new tests total)". Sum: 13+3+8+3+4+2 = 33 ✓.

But Step 5.2 explicitly enumerates only **2** boundary regression tests (`count_frames_in_range_includes_both_boundaries` + `delete_data_in_range_respects_delete_flags`) — not 3. The "3rd" test is implicitly the same pattern for `count_events_in_range`. Step 5.2 says "Add closed-closed boundary regression test for each of: `count_frames_in_range`, `count_events_in_range`, `delete_data_in_range`" — three methods listed, but only two test code blocks shown.

Also:
- Task 7 includes 4 tests (delete_range_request_external_shape_preserved + delete_range_request_period_accessor_returns_window + delete_range_request_period_rejects_inverted_bounds + report_query_query_string_roundtrip). These are NOT counted in the PHASE-HISTORY summary.

Updated count: 13+3+8+3+4+2 + 4 (Task 7) = **37 tests**, not 33.

**Fix recommendation**: Either (a) add explicit code blocks for all 3 boundary tests in Step 5.2, OR (b) update the count to 37 in Step 11.3 PHASE-HISTORY draft + clarify which tests count.

---

## VERDICT

**NEEDS PHASE 2 iter-3** — concise rationale: While **all 9 Critical and 11 Important findings from Phase 2 iter-1 were correctly addressed in plan v2** (each disposition entry maps to a real plan section), iter-2 introduced **6 new Critical issues** that would block compilation if implemented literally:

1. **N-C1**: 5 missing FocusMetrics call sites (3 in focus_analyzer/mod.rs, 1 in work_session.rs:317, 1 in work_session.rs:446)
2. **N-C2**: regime.rs:184 misidentified as `flag_noise_range` (it's `get_entries`)
3. **N-C3**: 9+ caller sites missing (services/, tests/support/failing_storage.rs, internal sqlite/* tests) — would cause cascading compile failures
4. **N-C4**: `list_segment_time_ranges` return type misstated as 2-tuple (actually 3-tuple with segment_id) — proposed `Vec<TimeWindow>` would drop segment_id and break regime.rs:194 caller
5. **N-C5**: `flag_noise_range` return type misstated as `Result<()>` (actually `Result<u64>`) + method is sync (NOT async)
6. **N-C6**: `get_daily_active_secs` return type misstated as `u64` (actually `Vec<(String, i64)>`)

These are all **factual errors about the actual source code** that would either fail to compile or produce wrong runtime behavior. They're easy to fix — just regenerate the affected sections after a more thorough source grep — but the plan as written cannot be safely handed to a subagent.

5 New Important issues cover plan polish (duplicate Step 1.11, wrong same-crate import in 7.3, missing dev-dep, type misstatement, test count drift).

**Recommended Phase 2 iter-3 actions:**

1. **Fix N-C1 through N-C6** by re-running comprehensive greps and updating plan claims to match actual source signatures + caller site counts.
2. **Resolve open question in N-C3**: does the inherent `pub fn` on `SqliteStorage` (events.rs:14, frames.rs:10, etc.) keep its `(&str, &str)` signature with the trait wrapper doing conversion, OR change too? Specify explicitly to avoid impedance mismatch.
3. **Fix N-I1 through N-I5** as plan polish (5-min fixes each).
4. **Re-verify the resulting plan v3 has zero new Critical/Important issues.**
5. **Then declare Phase 2 EXIT** and Phase 3 BLOCKED on PR-B1 #508.

The original 9C+11I disposition is sound. The implementation churn from these new findings is approximately +20-30 minutes of plan editing (no architectural changes, just enumeration corrections).

**Risk if implemented as-is**: subagent stalls at Task 4 (port-trait sig mismatches) and Task 8 (FocusMetrics caller failures). Total wall time loss: ~2-3 hours of debugging + re-grep + plan reversion.

---

**End of Phase 2 iter-2 verification.**
