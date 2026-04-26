# TimeWindow Phase 2 — Iteration 1 Plan Review Findings

**Date**: 2026-04-25
**Plan**: v1 (commit `76f7dc17`)
**Spec**: v3 (commit `f495dfbd`)
**Reviewer**: code-reviewer (deep-review subagent)
**Outcome**: 9 Critical + 11 Important + 6 Nice-to-have + 18 verified-correct items

The plan is well-structured and covers the spec at high level, but contains **multiple show-stoppers that would make a subagent stall or build wrong code**. Most originate from incorrect assumptions about the existing `CoreError` shape (struct-variant + typed `code` field per ADR-019, NOT a `#[from]` chain). Several spec requirements are also under-covered (port trait migration scope, frontend i18n test count update, ApiError code() routing).

---

## CRITICAL (block Phase 2 exit)

### C1 — `CoreError::TimeWindow(#[from] TimeWindowError)` does not match existing CoreError shape

Plan Step 2.3 instructs:

```rust
#[error("time_window: {0}")]
TimeWindow(#[from] crate::types::TimeWindowError),
```

**Reality** (`crates/oneshim-core/src/error.rs`, ADR-019 §4.6): every domain CoreError variant is a **struct variant** carrying a typed `code: XxxCode` field plus contextual fields (`message`, etc.) and there is **only ONE precedent for `#[from]`** — `Serialization(#[from] serde_json::Error)` and `Io(#[from] std::io::Error)`, both with the wire code **hardcoded inside the `#[error]` template** (e.g., `"[internal.io]: {0}"`). Those `#[from]` arms also have a regression test (`from_variants_display_includes_wire_code`) that asserts the bracketed wire code appears in `Display` output.

If implementer follows the plan literally:
1. The `#[error("time_window: {0}")]` template **omits the bracketed wire code** required by the ADR-019 regression invariant — drift with `from_variants_display_includes_wire_code` test pattern.
2. Mixing struct-variant style (every other domain variant) with `#[from]`-tuple style is inconsistent. Spec §4.6 explicitly justifies hardcoded wire codes for `#[from]` variants because they don't carry a typed `code:` field.

**Fix recommendations**:

Option A (preferred — match the existing ADR-019 §4.6 `#[from]` pattern):

```rust
// In CoreError enum:
#[error("Invalid time window [{}]: {0}", InvertedBounds.as_str())]
TimeWindow(#[from] crate::types::TimeWindowError),
```

But this is awkward because `TimeWindowError` has 2 variants (`InvertedBounds`, `ParseFailed`) so the static string would be wrong for the other. So really:

Option B (preferred — match the **majority** struct-variant pattern):

```rust
// In CoreError enum:
#[error("Time window error [{code}]: {message}")]
TimeWindow {
    code: crate::error_codes::TimeWindowCode,
    message: String,
},
```

Then in `CoreError::code()`:
```rust
Self::TimeWindow { code, .. } => code.as_str(),
```

Then add a manual `From<TimeWindowError> for CoreError` impl that maps both error variants to the correct code:
```rust
impl From<crate::types::TimeWindowError> for CoreError {
    fn from(err: crate::types::TimeWindowError) -> Self {
        Self::TimeWindow {
            code: err.code(),
            message: err.to_string(),
        }
    }
}
```

This matches the existing pattern (`Storage { code, message }`, `Network { code, message }`, etc.) and lets `CoreError::code()` return the right wire code per error variant.

The plan must be rewritten to use Option B (or explicitly justify Option A with a custom `Display` template that handles both variants — but the call-site complexity makes Option B clearly better).

### C2 — `From<CoreError> for ApiError` in `oneshim-web` has no arm for `CoreError::TimeWindow` (would silently fall to `ApiError::Internal` 500)

Plan Step 7.1 says "`?` operator works because `From<TimeWindowError> for ApiError` chain via `CoreError::TimeWindow` per Task 2." But the actual `From<CoreError> for ApiError` impl (`crates/oneshim-web/src/error.rs:56`) is a **closed match** with `other => ApiError::Internal(other.to_string())` as the wildcard arm. Without an explicit arm for the new `CoreError::TimeWindow` variant, **invalid time windows will return HTTP 500, not 400**.

This contradicts spec §7.1: "start > end (manually constructed) ... Caller propagates as 400 Bad Request via existing IpcError/ApiError chain" and breaks Plan Step 11.1 E2E test `invalid_time_window_returns_400`.

**Fix**: Add to plan Step 2 a sub-step that updates `crates/oneshim-web/src/error.rs`:

```rust
CoreError::TimeWindow { message, .. } => ApiError::BadRequest(message),
```

Place it near the existing `InvalidArguments`/`Validation` arms (semantic siblings — bound-validation failure is a 400, not 500).

Also add a regression test mirroring the existing pattern (`permission_denied_maps_to_forbidden`):

```rust
#[test]
fn time_window_inverted_bounds_maps_to_bad_request() {
    let core = oneshim_core::error::CoreError::TimeWindow {
        code: oneshim_core::error_codes::TimeWindowCode::InvertedBounds,
        message: "start > end".to_string(),
    };
    let api: ApiError = core.into();
    assert!(matches!(api, ApiError::BadRequest(_)));
}
```

### C3 — Plan Step 11.1 E2E asserts `body["code"] == "time_window.inverted_bounds"` but `ApiError::IntoResponse` does NOT serialize the wire code

Inspecting `crates/oneshim-web/src/error.rs:34-50` and `oneshim-api-contracts::error::ErrorResponse`:

```rust
pub struct ErrorResponse {
    pub error: String,
    pub status: u16,
}
```

`ApiError::IntoResponse` only emits `{ "error": "...", "status": 400 }` — there is **no `code` field**. The E2E test's assertion `body["code"] == "time_window.inverted_bounds"` will always fail.

**Fix**: Either
- (a) Drop the `code` assertion from Step 11.1 and assert only `response.status() == 400` plus that the error body string mentions invertedness, OR
- (b) Add `code: String` to `ErrorResponse` and propagate `core_err.code()` into `ApiError::IntoResponse`. This is a bigger change touching `oneshim-api-contracts` + every translateError consumer in the frontend — should be explicitly out-of-scope unless the spec is amended.

Recommend **(a)** for this PR. The wire-code surfacing would be a follow-up.

### C4 — `TimeRangeQuery` does NOT derive `Default` — every adapter test in Plan Step 4.2 fails to compile

Plan Step 4.2 has 6 tests using:
```rust
let q = TimeRangeQuery {
    from: Some("...".to_string()),
    to: None,
    ..Default::default()
};
```

Reality (`crates/oneshim-api-contracts/src/common.rs:4-11`):
```rust
#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    pub from: Option<String>,
    ...
}
```

No `Default` derive. `..Default::default()` won't compile. The same struct also has `pub min_importance: Option<f64>` which the plan tests don't fill in.

**Fix**: Either
- (a) Add `Default` to the derive list in Step 4.1 (zero risk — all fields are `Option<T>`):
  ```rust
  #[derive(Debug, Default, Deserialize)]
  pub struct TimeRangeQuery { ... }
  ```
- (b) Construct the test fixtures fully (all 5 `Option` fields) without `..Default::default()`.

Pick (a). Add a one-line Step 4.0 to the plan that bumps the derive line. This also unblocks Step 1.5's serde fixture style if needed.

### C5 — `from_rfc3339_pair` test `from_rfc3339_pair_handles_timezone_offset` has wrong expected end (asserts `dt(2026,4,25)` but parses `2026-04-25T09:00:00+09:00` which is `2026-04-25T00:00:00Z` ✓) — but `dt(2026, 4, 1)` for start should be `2026-04-01T00:00:00Z`. Need to verify ALL chrono assertion timestamps.

This one IS actually consistent (09:00 KST = 00:00 UTC). False alarm — but the fragility of hand-computed timestamps in the integer assertions of Step 4.2 is real:

```rust
assert_eq!(w.start.timestamp(), 1775433600); // 2026-04-01 UTC
assert_eq!(w.end.timestamp(), 1777507200);   // 2026-04-25 UTC
```

These integers are not validated. `2026-04-01T00:00:00Z` is `1774915200`, not `1775433600`. `2026-04-25T00:00:00Z` is `1776988800`, not `1777507200`. The plan's hand-computed seconds are off by **518400s = 6 days** — implying the plan author may have computed for `2026-04-07` and `2026-05-01` accidentally.

**Fix**: Replace integer assertions with chrono helper:
```rust
use chrono::TimeZone;
assert_eq!(w.start, Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap());
assert_eq!(w.end, Utc.with_ymd_and_hms(2026, 4, 25, 0, 0, 0).unwrap());
```

This avoids hand-computed timestamps entirely (also matches the helper `dt()` already used in Task 1 tests).

### C6 — Port trait migration scope vastly under-covered. Plan only updates `flag_noise_range`; misses ALL `WebStorage` sub-traits + `CalibrationReader` async methods

Spec §1.2 row 9 says "10+ methods" and Phase 1 N3 mentions only `flag_noise_range`. **In reality, the following port traits all need updates** (verified via `grep` on `crates/oneshim-core/src/ports/`):

In `crates/oneshim-core/src/ports/web_storage.rs`:
- `FrameQueryStorage::count_frames_in_range(&self, from: &str, to: &str)` (line 66)
- `FrameQueryStorage::list_frame_file_paths_in_range(&self, from: &str, to: &str, ...)` (line 74)
- `EventQueryStorage::count_events_in_range(&self, from: &str, to: &str)` (line 97)
- `StorageMaintenanceStorage::delete_data_in_range(&self, from: &str, to: &str, ...)` (line 116)
- `ActivityStatsStorage::get_daily_active_secs(&self, from: &str, to: &str)` (line 141)

In `crates/oneshim-core/src/ports/calibration_store.rs`:
- `CalibrationWriter::flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>)` (line 24) ← only one plan covers
- `CalibrationReader::get_entries(&self, from: DateTime<Utc>, to: DateTime<Utc>, exclude_noise: bool)` (line 35)
- `CalibrationReader::list_segment_time_ranges(&self, from: DateTime<Utc>, to: DateTime<Utc>)` (line 50)

The 5 web_storage_impl.rs WebStorage trait methods are **plain inherent fn**s on `SqliteStorage` AND **port trait methods** — both must change in lockstep, otherwise the trait `impl` won't satisfy the port.

The 2 CalibrationReader methods have **3 caller sites** in `src-tauri/src/scheduler/analysis_pipeline/regime.rs` (lines 44, 174, 184) plus 1 mock impl in `src-tauri/src/scheduler/analysis_pipeline/tests.rs:19`.

**Fix**: Plan Task 5 must be rewritten to enumerate all 8 port methods (not 1). Also Task 5 must list:
- `crates/oneshim-core/src/ports/web_storage.rs` in its Files list
- `src-tauri/src/scheduler/analysis_pipeline/regime.rs` (3 callers)
- `src-tauri/src/scheduler/analysis_pipeline/tests.rs` (mock)

Estimated effort jumps from 3h to ~5h.

### C7 — Plan misses many `*_in_range` callers + `web_storage_impl.rs` thin-wrapper layer

`crates/oneshim-storage/src/sqlite/web_storage_impl.rs` is a 700+ LoC adapter file that contains ~15 thin wrappers like:
```rust
fn count_events_in_range(&self, from: &str, to: &str) -> Result<u64, CoreError> {
    SqliteStorage::count_events_in_range(self, from, to).map_err(Into::into)
}
```

Each wrapper must be migrated **plus** the inherent method on `SqliteStorage` (in `events.rs`, `frames.rs`, `maintenance.rs`, etc.). The plan only references "events.rs, frames.rs, calibration_store_impl.rs, web_storage_impl.rs, maintenance.rs" but doesn't note that `maintenance.rs` contains:
- `pub fn list_frame_file_paths_in_range` (line 253)
- `pub fn delete_data_in_range` (line 286, with `delete_events: bool, delete_frames: bool, delete_metrics: bool, ...` signature — NOT just from/to)

**Fix**: Plan Task 5 must enumerate per-file:

| File | Methods to migrate |
|------|--------|
| `events.rs` | `count_events_in_range` |
| `frames.rs` | `count_frames_in_range` (no `get_frames` — that's a separate signature on `FrameQueryStorage`) |
| `maintenance.rs` | `list_frame_file_paths_in_range`, `delete_data_in_range` |
| `calibration_store_impl.rs` | `flag_noise_range`, `get_entries`, `list_segment_time_ranges` |
| `web_storage_impl.rs` | thin wrappers for ALL 5 above (+ `get_daily_active_secs`) |

Add `get_daily_active_secs` to the touched-files list. Also add `list_hourly_metrics_since(&self, from: &str)` review — this is NOT a range query (one bound only) but might be in scope per spec §1.2 row 9.

Currently the plan would compile-pass on subagent step 5.7 because the `cargo check` command doesn't run all permutations, but the gap will surface during `cargo check --workspace` in Task 9 — too late, with a partial migration in flight.

### C8 — Wire snapshot baseline is NOW 42, will become 47 after PR #508 — but Plan Step 2.4 has no instruction to RECOMPUTE alphabetical position OR i18n test count

`crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts` has **two hard-coded `42` assertions** (lines 30 and 122):

```typescript
expect(registry).toHaveLength(42)
expect(translatedCodes('en')).toHaveLength(42)
```

Plan Step 3.3 says "Update count = current + 2 (per PF3 actual baseline)". This is correct in principle but the plan doesn't enumerate WHICH count assertion lines to update — there are TWO of them, **both with the comment `// 41 → 42 with D7 addition...`** that should be updated to indicate the new addition.

**Fix**: In Step 3.3:
1. Make the `grep` pattern explicit: `grep -n "toHaveLength(42)" ...` (or `toHaveLength(\d\+)`)
2. List both line numbers (30 and 122 at the time of writing).
3. Update both to current+2.
4. Update comments to reflect TimeWindow addition.

Also: the plan PF3 says "post-PR-B1 = 47, post-PR-B2 = 51" but there is no source-of-truth verification for these numbers. PR #508 might add ≠5 codes (e.g., 4 or 6). Plan must say "compute actual count after merge — DO NOT trust pre-merge estimates."

### C9 — Plan Step 8.1 `#[serde(flatten, with = "...")]` does NOT work as written (incompatible serde attributes)

```rust
#[serde(flatten, with = "delete_range_period_serde")]
pub period: TimeWindow,
```

Serde's `#[serde(flatten)]` and `#[serde(with = "...")]` do not compose. The `with = "..."` module is expected to define `serialize/deserialize` functions for the field type, but `flatten` requires the field to be a struct/map that's inlined. They conflict.

The intended outcome (preserve external `from`/`to` keys, store as `TimeWindow` internally) requires either:

Option A (custom Deserialize/Serialize on the whole struct):
```rust
pub struct DeleteRangeRequest {
    pub period: TimeWindow,
    pub data_types: Vec<String>,
}

// Manual impl Serialize + impl Deserialize that reads `from`/`to`/`data_types` keys
```

Option B (intermediate "External" struct, no flatten):
```rust
#[derive(Serialize, Deserialize)]
struct DeleteRangeRequestExt {
    from: String,
    to: String,
    #[serde(default)]
    data_types: Vec<String>,
}

impl From<DeleteRangeRequestExt> for Result<DeleteRangeRequest, ...> { ... }
```

with conversion at the handler boundary, OR

Option C (plain field-rename via two separate fields, parsed at handler):
```rust
pub struct DeleteRangeRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub data_types: Vec<String>,
}

impl DeleteRangeRequest {
    pub fn period(&self) -> Result<TimeWindow, TimeWindowError> {
        TimeWindow::from_rfc3339_pair(&self.from, &self.to)
    }
}
```

Option C is the **simplest and least risky**: it preserves the external JSON shape **and** the internal struct shape; only adds a `period()` accessor. The handler uses `req.period()?` instead of `req.period`. **Recommend C** — actually no migration of the struct fields is needed.

The plan must be rewritten — Step 8.1's serde module pattern would fail compile or produce wrong serialization (a flattened `TimeWindow` would emit `start`/`end` not `from`/`to`).

Also note: existing `DeleteRangeRequest` is `#[derive(Debug, Deserialize)]` — **no `Serialize`**. Plan Step 8.2's roundtrip test calls `serde_json::to_string(&req).unwrap()` which would fail compile because `Serialize` is missing. Plan must add `Serialize` to derives if Option A or B is chosen, OR drop the round-trip serialization test under Option C.

---

## IMPORTANT (must address)

### I1 — `oneshim_core::models::telemetry::SessionMetrics` has zero callers in the workspace

`grep -rn` finds no production code that constructs or reads `oneshim_core::models::telemetry::SessionMetrics`. The only consumers are:
- `crates/oneshim-analysis/src/assembler.rs:76` — defines a DIFFERENT `SessionMetrics` type
- `crates/oneshim-analysis/src/analyzer.rs:427,462` — uses analysis::SessionMetrics
- `src-tauri/tests/text_extraction_e2e.rs:114,125` — uses analysis::SessionMetrics
- `crates/oneshim-network/src/integration/runtime_telemetry.rs` — uses `oneshim_core::models::integration` (different)

**The plan's Task 9 Step 9.3 migrates `oneshim_core::models::telemetry::SessionMetrics` but it appears to be dead code.** Migrating it adds zero value but adds churn.

**Fix recommendations**:
- (a) Verify with `cargo +nightly udeps` or `dead_code` survey, AND if confirmed dead — remove the type entirely (separate cleanup commit, NOT this PR).
- (b) Migrate as-planned for consistency, accepting the churn.
- (c) Document in the plan that this is preemptive consolidation for future use, with a note "verify zero callers; if so, consider dropping this struct as a follow-up."

The decision changes the commit graph slightly. Recommend (b) for safety, with a TODO to revisit dead-code in a separate cleanup PR.

### I2 — Plan only migrates ONE FocusMetrics constructor; ignores existing 4 call sites

`grep -rn "FocusMetrics {" crates/` finds 4 sites:
1. `crates/oneshim-storage/src/sqlite/edge_intelligence/focus_metrics.rs:55, 217` (2 sites)
2. `crates/oneshim-web/tests/grpc_dashboard_integration.rs:461` (test fixture)
3. `crates/oneshim-storage/src/sqlite/edge_intelligence/tests.rs:76` — `FocusMetrics::new(updated.period_start, updated.period_end)` (calling the deprecated 2-arg constructor)

Plan Step 9.2 says "Find every `FocusMetrics { period_start: ..., period_end: ... }` literal in the codebase: `grep -rn "FocusMetrics {" crates/`" — but **doesn't enumerate the four found sites**. Subagent driven implementation will need to handle each call site, including:
- The constructor `FocusMetrics::new(period_start: DateTime<Utc>, period_end: DateTime<Utc>) -> Self` at `work_session.rs:287` — must change signature OR add overload `from_period(period: TimeWindow)`.
- The internal use of `period_start`/`period_end` at `work_session.rs:317` (`(self.period_end - self.period_start).num_seconds()`).
- `focus_metrics.rs:43-57` — splits a `(period_start, period_end)` tuple from `date_to_period_range`.
- `focus_metrics.rs:213,218` — same pattern.

**Fix**: Step 9.2 should enumerate the file:line list and explicitly say:

> Update `FocusMetrics::new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, TimeWindowError>` (returns Result because TimeWindow::new can fail). All 4 internal call sites use trusted construction (cron-aligned date_to_period_range computes start < end), so `.expect("date_to_period_range produces valid window")` is acceptable.

Also recommend renaming `FocusMetrics::new` to `FocusMetrics::from_window(period: TimeWindow, ...)` to match the new shape — clearer than passing 2 datetimes to construct one nested field.

### I3 — Plan Task 7 only lists 6 handlers; spec §4.1 lists 8

Spec §4.1 modified files include: `frames`, `events`, `metrics`, `focus`, `sessions`, `interruptions`, `data`, `reports`. Plan Task 7 lists `frames/events/metrics/focus/idle/processes` (substituting `idle` and `processes` for `sessions` and `interruptions`).

Reality from `grep -rn "TimeRangeQuery" crates/oneshim-web/src/handlers/`:
- `idle.rs` ✓
- `focus.rs` ✓
- `events.rs` ✓
- `metrics.rs` ✓
- `frames.rs` ✓
- `processes.rs` ✓

`sessions.rs` and `interruptions.rs` don't appear in the grep. Spec §4.1 may be wrong, or those handlers don't use `TimeRangeQuery` directly (they may take typed query structs).

**Fix**: Plan should explicitly state which spec §4.1 entries are excluded (sessions/interruptions don't use TimeRangeQuery in current code) and add a Task 7 sub-step that runs `grep -rn "TimeRangeQuery" crates/oneshim-web/src/handlers/` to confirm the actual list before migrating, with a note "if grep finds more files than the 6 listed, expand the migration."

### I4 — `oneshim-core::types::TimeWindow` import in `oneshim-api-contracts/src/common.rs` requires `oneshim-core` workspace dep

Plan Step 4.1 adds `use oneshim_core::types::{TimeWindow, TimeWindowError};` to `crates/oneshim-api-contracts/src/common.rs`. The plan PF5 step 6 mentions verifying `oneshim-core = { workspace = true }` exists in `crates/oneshim-api-contracts/Cargo.toml:16`. But it doesn't actually verify — the line "(per Phase 1 iter-1 N3)" is just a reminder.

**Fix**: PF5 should use a verification command:
```bash
grep -E "^oneshim-core\s*=" crates/oneshim-api-contracts/Cargo.toml
```
If empty: HALT and add the dep before proceeding.

In practice this dep almost certainly exists (verified during my research — `error::ErrorResponse` is in api-contracts already), but the verification command must be in the plan.

### I5 — Plan Step 7.1 ApiError mapping omits the BadRequest from Step 2

Plan Step 7.1 says:

> (`?` operator works because `From<TimeWindowError> for ApiError` chain via `CoreError::TimeWindow` per Task 2.)

But Task 2 (per current Plan Step 2.3) only adds `CoreError::TimeWindow(#[from] TimeWindowError)` and the `code()` arm — NOT the `From<CoreError> for ApiError` arm in `oneshim-web/src/error.rs`. See C2 above.

**Fix**: Bridge Tasks 2 and 7. Add a Task 2 sub-step "Step 2.3.5: Update `From<CoreError> for ApiError` in `oneshim-web/src/error.rs`" with the explicit arm. Or move the ApiError arm into Task 7 (handler migration is where it's needed) but document the dependency clearly.

### I6 — Plan provides NO acceptance criteria for `cargo clippy` quality gate

PC1 (Post-Completion checklist) runs clippy as a gate, but the implementation tasks don't reference clippy except at the end. Per Memory `feedback_lefthook_clippy_cost.md`, clippy on cold cache takes ~16min. If subagent runs `cargo clippy --workspace` after every task, that's ~3-4 hours of cumulative wait time.

**Fix**: Add a note at the top of "Pre-Flight Checks" or in the `superpowers:executing-plans` reference: "Run `cargo clippy --workspace --all-targets -- -D warnings` once after Task 11 (full integration) and once at PC1 — not per-task. Use `cargo check -p <crate>` for fast feedback during tasks."

Also: clippy will likely flag the `pub` fields on `TimeWindow` (`pub start, pub end`) per spec §5.1 with the `clippy::struct_excessive_bools` or similar; verify against current ALLOW list in `lib.rs`.

### I7 — Wire-snapshot insertion position uses regex `^t` but ignores the alphabetical ordering convention

Plan Step 2.4 / PF3:
```bash
grep -n "^t" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt
```

The actual file has zero `^t` codes currently (verified — last alphabetical line is `validation.invalid_field`, no `t*` prefix). After PR #508 lands with `tracking_schedule.*` codes (assumed), the position will be:
```
... storage.failed
... [PR-B1] tracking_schedule.invalid_window
... [PR-B1] tracking_schedule.overlap_detected
... [TimeWindow] time_window.inverted_bounds
... [TimeWindow] time_window.parse_failed
ui.element_missing
validation.invalid_arguments
...
```

Wait: `time_window.*` comes BEFORE `tracking_schedule.*` alphabetically? No: `time_window` < `tracking_schedule` (because `i` < `r`), so the order is:
```
service.unavailable
storage.failed
time_window.inverted_bounds          ← TimeWindow first
time_window.parse_failed
tracking_schedule.... [PR-B1]
ui.element_missing
```

Plan Step 2.4 should clarify the alphabetical neighbors AND confirm that `time_window` < `tracking_schedule` (so order is `time_window.*` THEN `tracking_schedule.*` THEN `ui.*`). The current grep `^t` is correct after rebase but the plan doesn't explain WHY two `t*` blocks may appear.

**Fix**: Step 2.4 should show an example of the expected alphabetical block:
```
storage.failed
time_window.inverted_bounds
time_window.parse_failed
[then any tracking_schedule.* lines from PR-B1]
ui.element_missing
```

### I8 — TimeWindow `#[derive(Copy)]` may bloat hot paths if struct grows (acceptable now, but plan should note)

Plan Step 1.2 derives `Clone, Copy`. This is currently fine because `TimeWindow` is two `DateTime<Utc>` (16 bytes each = 32 bytes total — well under the typical Copy threshold of 64 bytes). But if the spec ever evolves to add metadata (timezone hint, span tag, etc.), Copy becomes a footgun.

**Fix** (nit): Plan could note "If TimeWindow grows beyond ~32 bytes, drop the `Copy` derive and pass by reference everywhere. Currently safe."

Also: Spec Q-7 keeps `pub start, pub end` to allow direct destructuring. With `Copy`, Rust patterns on `&TimeWindow` (e.g., `if let TimeWindow { start, end } = *w`) work; without `Copy`, callers need explicit `let TimeWindow { start, end } = *w` or `(w.start, w.end)`. Worth a one-line note.

### I9 — Plan Step 1.6 has confusing fallback path that creates a circular dep on Task 2

Step 1.6 says:

> If `TimeWindowCode` not yet defined: tests fail compile; commit Task 1 with `code()` method and last 2 tests temporarily commented out, then re-enable in Task 2.

But Step 1.2's `time_window.rs` IMPORTS `use crate::error_codes::TimeWindowCode;` at the top. Without `TimeWindowCode` defined, the **entire file fails to compile**, not just the last 2 tests. The entire `oneshim-core::types` module would fail compile.

**Fix**: Reverse Task 1 / Task 2 order:
- Task 1 (new): Create `TimeWindowCode` enum + register in `error_codes/mod.rs` + update wire snapshot.
- Task 2 (new): Create `TimeWindow` struct + use the now-existing `TimeWindowCode`.

OR keep order but make Task 1 self-contained: don't import `TimeWindowCode`, return a `&'static str` from `code()` (hardcoded `"time_window.inverted_bounds"` etc.) until Task 2 wires the typed enum. This contradicts ADR-019 §7 but works as a temporary state.

OR (cleanest) **merge Tasks 1 + 2** into a single commit:
- Create `TimeWindow` + `TimeWindowError` + `TimeWindowCode` together (one logical unit).
- Wire registration + update snapshot.
- Single commit message: `feat(time): add TimeWindow primitive + TimeWindowCode wire codes + CoreError variant`.

I recommend the merge-1-and-2 approach. Each task should be a coherent compile-passing unit.

### I10 — Plan's commit messages don't all match conventional commit / git-cliff conventions

Per Memory `feedback_squash_merge_cliff_skip.md`, `chore:` squashes are skipped by git-cliff. Plan's Task 12 uses `docs(time-window): ...` which is OK. But Plan Step 5.8 uses `refactor(storage): migrate ...` and Task 9 uses `refactor(models): ...` which are fine.

Verify: Task 1's commit message "feat(time): add TimeWindow primitive + TimeWindowError + types module" — `feat(time)` is unusual (most existing commits use crate-name like `feat(core)` or feature-area like `feat(error-codes)`). Per Memory `feedback_subagent_worktree_pinning.md`, conventions matter for cliff parsing.

**Fix**: Survey existing recent commits with `git log --oneline -50 origin/main` and align scope tags. Suggested mapping:
- Task 1: `feat(core): add TimeWindow primitive + types module` (NOT `feat(time)`)
- Task 2: `feat(error-codes): TimeWindowCode + CoreError::TimeWindow integration`
- Task 5: `refactor(storage): migrate SQL range helpers to TimeWindow`
- Task 9: `refactor(core): FocusMetrics + SessionMetrics use TimeWindow primitive`

This is a small-but-real impact on git-cliff release-note generation.

### I11 — `#[serde(rename_all = "lowercase")]` already exists on `ReportPeriod` enum — plan must not break this

Plan Step 8.3 says:
```rust
pub struct ReportQuery {
    pub period: ReportPeriod,
    pub window: Option<TimeWindow>,
}
```

But the existing `ReportPeriod` is:
```rust
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReportPeriod {
    #[default]
    Week,
    Month,
    Custom,
}
```

When `period` isn't sent in the request, it defaults to `Week` (per `#[serde(default)]`). The frontend currently sends `?period=week&from=...&to=...` query strings. After refactor:
- frontend sends `?period=custom&from=...&to=...`
- query deserializes to `ReportQuery { period: Custom, window: None }`
- handler must then construct `TimeWindow` from `from`/`to` query params... but `ReportQuery` no longer carries those fields!

**Fix**: Either
- (a) Keep `from: Option<String>, to: Option<String>` in `ReportQuery` and add the `to_time_window()` adapter method (similar to Phase 1 I2 resolution).
- (b) Refactor `ReportQuery` to flatten `TimeRangeQuery` via `#[serde(flatten)]` (which DOES work for fields that are themselves Deserialize structs, unlike C9):
  ```rust
  pub struct ReportQuery {
      pub period: ReportPeriod,
      #[serde(flatten)]
      pub time_range: TimeRangeQuery,
  }
  ```
  Then `req.time_range.to_time_window(default_lookback)?`.

Plan must explicitly resolve this. Spec Q-3 says "ReportQuery { period: ReportPeriod, window: Option<TimeWindow> }" — but this design is broken because TimeWindow can't deserialize from `?from=X&to=Y` query string format (it has `start/end` keys via Serialize derive). **Spec Q-3 is wrong as stated; plan inherits the bug.**

Recommend (b) — flatten `TimeRangeQuery` — and update spec Q-3 to match.

---

## NICE-TO-HAVE

### N1 — Plan Tasks 1.5/1.6 has 12 tests but plan introduction promises "12 unit tests"; one is wrong

Counting Plan Step 1.5 tests:
1. `new_accepts_valid_bounds`
2. `new_accepts_zero_duration_window`
3. `new_rejects_inverted_bounds`
4. `contains_includes_both_bounds`
5. `contains_excludes_outside`
6. `duration_returns_difference`
7. `to_sql_pair_round_trips_via_from_rfc3339_pair`
8. `from_rfc3339_pair_accepts_z_suffix`
9. `from_rfc3339_pair_handles_timezone_offset`
10. `from_rfc3339_pair_rejects_invalid_strings`
11. `serde_roundtrip_json`
12. `time_window_error_code_inverted_bounds`
13. `time_window_error_code_parse_failed`

That's **13 tests**, not 12. Trivial mismatch with the introduction. Either accept 13 or drop one (e.g., merge `time_window_error_code_inverted_bounds` and `_parse_failed` into a single `code_routing` test using parametrized assertions).

### N2 — `from_rfc3339_pair_rejects_invalid_strings` test name doesn't match `("not-a-date", "2026-04-25T00:00:00Z")`

The test passes ONE invalid string. The name implies plural. Pluralize OR the test should also check all-invalid:
```rust
fn from_rfc3339_pair_rejects_invalid_strings() {
    assert!(matches!(
        TimeWindow::from_rfc3339_pair("not-a-date", "2026-04-25T00:00:00Z"),
        Err(TimeWindowError::ParseFailed(_))
    ));
    assert!(matches!(
        TimeWindow::from_rfc3339_pair("2026-04-25T00:00:00Z", "not-a-date"),
        Err(TimeWindowError::ParseFailed(_))
    ));
    assert!(matches!(
        TimeWindow::from_rfc3339_pair("nope", "also-not-a-date"),
        Err(TimeWindowError::ParseFailed(_))
    ));
}
```

### N3 — Plan does not specify Korean translation review

The proposed Korean translation `"시간 범위가 잘못되었습니다: 시작이 종료보다 빨라야 합니다"` (Time range is wrong: start must be earlier than end) is correct but verbose. Compare to existing patterns:
- `"network.timeout": "네트워크 요청이 시간 초과되었습니다 — 연결을 확인하고 재시도하세요"` (en: "The request timed out — check your connection and retry") — natural tone.
- `"validation.invalid_arguments": "잘못된 인수입니다: {message}"` — terse.

Suggest:
```
"time_window.inverted_bounds": "시간 범위가 올바르지 않습니다 — 시작 시각이 종료 시각보다 앞서야 합니다",
"time_window.parse_failed": "타임스탬프 형식이 올바르지 않습니다: {message}",
```

(Replace 잘못 with 올바르지 않 for consistency with sibling `config.invalid` translation.)

### N4 — `same_start_end_is_valid_zero_duration_window` — name suggests redundancy; actually tests Q-6 RESOLVED case

Plan renamed to `new_accepts_zero_duration_window`. But spec §8.1 still uses `same_start_end_is_valid_zero_duration_window`. Pick one consistently or note the rename in plan vs spec docs.

### N5 — `to_time_window` adapter test for `..Default::default()` would not compile (see C4 fix)

Already covered in C4. Listing here for completeness.

### N6 — Plan Step 12.3 PHASE-HISTORY entry says "+12 unit tests" but actual count per Plan = 13 (see N1)

Update the count in the PHASE-HISTORY draft (or fix tests to actually be 12).

---

## VERIFIED CORRECT (no action)

- spec §1.2 row catalog → plan's File-to-modify list ✓ (mostly — see C6, C7 for gaps)
- spec §2.1 G1-G5 goals → plan tasks address all 5 ✓
- spec §2.2 NG1 (TrackingWindow not migrated) → plan does not touch tracking_schedule.rs ✓
- spec §2.2 NG2 (TimeBucket deferred) → plan does not introduce TimeBucket ✓
- spec §2.2 NG3 (REST query string format) → plan keeps `?from`/`?to` ✓ (assuming I11 ReportQuery resolved)
- spec §2.2 NG4 (Frontend types unchanged) → plan does not touch TS types in frontend (assuming C9 resolved with Option C)
- spec §2.2 NG5 (UTC internal) → plan uses `DateTime<Utc>` consistently ✓
- spec §2.2 NG6 (SQL BETWEEN preserved) → plan keeps `WHERE timestamp >= ?1 AND timestamp <= ?2` ✓
- spec §2.2 NG7 (IdlePeriod not migrated) → plan Step 7.5 explicitly says "model NOT migrated" + activity.rs absent from touched files ✓
- spec §2.2 NG8 (FocusMetrics internal Option Z) → plan Step 9.1 explicitly says "Option Z, internal model only" + verifies focus.rs DTO has different fields ✓
- spec §3 U1-U5 user-locked decisions → plan respects all 5 ✓
- spec §5.1 TimeWindow signature → plan Step 1.2 signature matches ✓
- spec §5.2 `to_time_window(&self, ...)` non-consuming (C4 from Phase 1) → plan Step 4.1 uses `&self` ✓
- spec §5.4 FocusMetrics field rename → plan Step 9.1 ✓
- spec §6.2 default-lookback flow → plan tests cover both bounds-missing case ✓
- spec §6.3 GDPR delete (no default applied) → plan Step 8.1 + 8.2 verifies external shape ✓ (assuming C9 resolution)
- spec §10 backward compatibility → plan Task 8 + Task 9 align ✓ (assuming C9 resolution)
- spec §11 Q-1 through Q-9 resolutions → plan tasks honor each (Q-8 deferred to PF3 ✓)

---

## Phase 2 iter-2 Plan

After plan v2:
1. Fresh subagent re-reviews
2. Verify all Critical + Important fixes
3. Specifically re-verify:
   - C1 (CoreError struct-variant pattern with manual From impl)
   - C2 (ApiError arm)
   - C6 (port trait migration scope expanded to 8 methods)
   - C9 (DeleteRangeRequest serde — Option C accessor recommended)
   - I11 (ReportQuery flatten approach)
4. Run a dry compile of Step 1 + Step 2 in worktree to catch any remaining issues with macro/import ordering.
5. If clean → Phase 2 EXIT, Phase 3 BLOCKED on PR #508.

## Risk Adjustment

The plan's ~21h estimate is **likely too low** given the C6 + C7 expansion (port traits + maintenance.rs callers). Revised estimate: **~26-28h** (~3.5-4 working days). Subagent-driven development with 2-stage review per task should still complete in ~4-5 calendar days.

---

**End of Phase 2 iter-1 findings.**
