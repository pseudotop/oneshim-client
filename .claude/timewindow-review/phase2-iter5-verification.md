# Phase 2 iter-5 Verification — Plan v5 (commit d8692f5c)

Independent verifier (not iter-4 reviewer). Two-part scope:
- Part A: confirm 6 pre-existing iter-4 disposition fixes correctly applied
- Part B: hunt for NEW Critical/Important regressions introduced by v5

---

## VERIFIED FIXED (iter-4 pre-existing disposition)

- **Pre-existing #1 (NoopCalibration{Reader,Writer} mocks)**: Step 4D.2 body (plan
  lines 1448-1483) is correctly rewritten — `impl CalibrationWriter for
  NoopCalibrationWriter` is sync (no `#[async_trait]`); `impl CalibrationReader
  for NoopCalibrationReader` is async (with `#[async_trait::async_trait]`);
  comment at line 1479-1481 documents that `list_segment_time_ranges` uses the
  trait default impl (not explicitly overridden). Independent verification
  against actual `src-tauri/src/scheduler/analysis_pipeline/tests.rs:13-31`
  confirms two separate structs with matching sync/async layouts. ✓
  *Caveat: see NEW Important #1 below — Step 4D.0 inventory + Step 4E.1 commit
  message still hold stale "MockCalibration" labels.*

- **Pre-existing #2 (DeletedRangeCounts field names)**: Step 4C.4 inherent fn body
  (plan lines 1241-1257) consistently uses
  `events_deleted`/`frames_deleted`/`metrics_deleted`/`process_snapshots_deleted`/`idle_periods_deleted`.
  Step 5.2 boundary regression test assertions (plan lines 1750-1754) match the
  same names. Independent verification against
  `crates/oneshim-core/src/models/storage_records.rs:88-94` confirms struct uses
  these exact field names. Step 7.4 (data.rs handler) does not reference field
  names directly — it propagates the result through the handler signature, so
  no downstream stale reference. ✓

- **Pre-existing #3 (maintenance test `?` → `.expect`)**: Step 4C.4 explicit
  pattern at plan lines 1272-1280 says **"All these are `#[test] fn name()`
  returning `()`** — `?` operator does NOT compile. Use `.expect("trusted test
  bounds")` pattern (mirror Step 4C.2 events.rs example)" with a literal code
  example. All 9 sites listed (lines 931, 1019, 1052, 1067, 1083, 1164, 1183,
  1308, 1358). Independent spot-check at maintenance.rs:931 confirms the test
  callsite exists and uses `.unwrap()` chain (returns `()`). ✓

- **Pre-existing #4 (stale Files-to-be-modified table at lines 188-195)**:
  Verified — regime.rs:184 row says `get_entries (re-fetch — NOT
  flag_noise_range)`, recommends `.expect()` pattern, tests.rs row mentions
  `NoopCalibrationWriter (sync) + NoopCalibrationReader (async)` and explicitly
  notes "NOT a single `MockCalibration`". An additional row for
  regime.rs:194 (destructure `(seg_id, seg_window)`) was added per disposition
  table 5c. Independent verification against actual
  `regime.rs:43-46`, `regime.rs:171-194` confirms call sites match plan claims
  (line 44 = get_entries, line 174 = list_segment_time_ranges, line 184 =
  get_entries re-fetch, line 194 = filter_map destructure). ✓

- **Pre-existing #5 (broken grep helper)**: Step 4D.3 reports_query_support
  region (plan line 1571) replaced with `awk 'NR<=86 && /^pub.*fn |^pub\(crate\)
  fn |^fn /' .../reports_query_support.rs | tail -1`. Plan line 1574
  pre-resolves the result: "Verified at plan-write time: enclosing fn is
  `pub(crate) fn build_daily_stats(input: DailyStatsInput<'_>) -> Vec<DailyStat>`
  (returns plain `Vec<_>`, NOT `Result`). **Use Pattern B** for this caller."
  Independent verification by re-running `awk` on actual source (lines 60-90)
  confirms the enclosing fn signature is exactly as stated, plain `Vec<DailyStat>`
  return type, so Pattern B selection is correct. ✓

- **Pre-existing #6 (variable name `all` not `dirty`)**: Step 4C.1.5 line 1150-1152
  uses `let all = ...` with explicit comment "actual variable name is `all`,
  NOT `dirty`". Independent verification against actual
  `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:424-428` confirms
  `let all = storage.get_entries(wide_from, wide_to, false).await.unwrap();`.
  ✓

---

## NEW CRITICAL ISSUES (introduced by v5)

(none)

---

## NEW IMPORTANT ISSUES (introduced by v5)

### NEW-I1: Stale `MockCalibration` labels NOT propagated to inventory + commit message

The Step 4D.2 fix correctly renames the body to `NoopCalibrationWriter` /
`NoopCalibrationReader`, but TWO downstream propagation sites still hold the
old label:

1. **Step 4D.0 inventory table — plan line 1378**:
   ```
   | src-tauri tests.rs MockCalibration | line ~19 | 1 |
   ```
   Should be split into two rows or relabeled (e.g.,
   "src-tauri tests.rs NoopCalibrationWriter (sync) + NoopCalibrationReader (async)
   | lines 12-31 | 2"). An implementer who reads this row literally will
   `grep MockCalibration` and find no matches in actual source.

2. **Step 4E.1 commit message body — plan line 1676**:
   ```
   - 1 MockCalibration in src-tauri/scheduler/analysis_pipeline/tests.rs (sync flag_noise_range)
   ```
   The eventual commit message will be misleading; the actual change
   touches both `NoopCalibrationWriter` (sync, log_batch + flag_noise_range) and
   `NoopCalibrationReader` (async, get_entries + enforce_retention).

**Severity rationale**: the body of Step 4D.2 is correct and the implementer
following it will produce correct code. But the inventory row breaks the
"if grep finds MORE than these, expand scope inline" check at line 1380
(grep for MockCalibration finds zero, implementer might think the site is
already migrated and skip it), and the commit message ships a factually
wrong artifact. Important not Critical because the Step 4D.2 body itself
unambiguously instructs the right thing.

### NEW-I2: Step 4C.4 inherent fn snippet drifts from actual source on table/column names

Step 4C.4 (plan lines 1232-1260) presents a "Before / After" body for
`MaintenanceStorage::delete_data_in_range`. The "After" block has FIVE
abbreviated SQL placeholders:

| Plan | Actual maintenance.rs:303-356 |
|------|-------------------------------|
| `"DELETE FROM events WHERE timestamp >= ?1 AND timestamp <= ?2"` (line 1238 — fully spelled) | matches line 306 |
| `/* DELETE FROM frames WHERE ... */` (line 1244) | actual: `"DELETE FROM frames WHERE timestamp >= ?1 AND timestamp <= ?2"` (line 316) — OK |
| `/* DELETE FROM metrics WHERE ... */` (line 1248) | actual: `"DELETE FROM **system_metrics** WHERE timestamp >= ?1 AND timestamp <= ?2"` (line 326) — table is `system_metrics`, NOT `metrics` |
| (no second SQL inside `delete_metrics`) | actual ALSO executes `"DELETE FROM system_metrics_hourly WHERE **hour** >= ?1 AND **hour** <= ?2"` (line 333) — uses `hour` column, NOT `timestamp` |
| `/* DELETE FROM process_snapshots WHERE ... */` (line 1252) | matches line 341 |
| `/* DELETE FROM idle_periods WHERE ... */` (line 1256) | actual: `"DELETE FROM idle_periods WHERE **start_time** >= ?1 AND **start_time** <= ?2"` (line 352) — uses `start_time` column, NOT `timestamp` |

**Three concrete drifts** that an implementer following the snippet literally
will get wrong:

1. Plan abbreviates `DELETE FROM metrics` but actual table is `system_metrics`
2. Plan completely omits the `system_metrics_hourly` companion DELETE inside
   the `delete_metrics` block — this is a real second SQL execution against a
   second table with a different column name (`hour`)
3. Plan abbreviates `DELETE FROM idle_periods WHERE timestamp >= ?1` but
   actual column is `start_time`

Additionally, the Step 4C.4 snippet uses two stylistic patterns that diverge
from actual source and observably change error variants:

- Plan line 1235: `let conn = self.conn.lock().unwrap();`
- Actual line 296-299: `let conn = self.conn.lock().map_err(|e| StorageError::Internal(format!("Failed to acquire lock: {e}")))?;`

- Plan line 1240 ff.: `rusqlite::params![&from, &to])?;` then `counts.events_deleted = n as u64;`
- Actual line 305-310 ff.: `.map_err(|e| StorageError::Internal(format!("event delete failure: {e}")))? as u64;`

The plan's `?` does compile (because
`crates/oneshim-storage/src/error.rs:11` declares
`Sqlite(#[from] rusqlite::Error)`), but the resulting error variant is
`StorageError::Sqlite(rusqlite::Error)`, NOT
`StorageError::Internal("event delete failure: ...")` — observable change in
error message and variant for any downstream code matching on the variant or
parsing the message.

**Severity rationale**: this is a genuine plan-vs-source drift that pre-existed
v5 (v4 had even less detail — `if delete_frames { /* similar */ }`), but v5's
expansion **introduced more specificity without verifying it against actual
source**. v5 now states wrong table/column names through abbreviated comments,
which is materially worse than v4's noncommittal `/* similar */`. An
implementer who treats Step 4C.4 as a guide-by-example template without
re-reading actual maintenance.rs:285-360 will:
(a) miss the second `system_metrics_hourly` DELETE entirely (silent data
retention bug — hourly aggregates accumulate forever),
(b) write `DELETE FROM idle_periods WHERE timestamp >= ?1` and hit a SQL
error at runtime ("no such column: timestamp"),
(c) write `DELETE FROM metrics` and hit a SQL error ("no such table: metrics"),
(d) lose precise error messages.

Recommended v6 fix: expand the snippet to include the literal SQL for ALL 5
flag blocks AND the second `system_metrics_hourly` execution, OR replace the
snippet with a "preserve-existing-body / replace only `from`+`to` parameter
binding with `let (from, to) = window.to_sql_pair();`" prescription. The
latter is safer because it preserves all current SQL exactly.

---

## VERDICT

**NEEDS PHASE 2 iter-7**

Pre-existing 6 disposition: all 6 ✓.
NEW Critical: 0 ✓.
NEW Important: 2 (NEW-I1, NEW-I2).

User mandate is "Critical+Important 이슈가 없을 때까지" — converging to zero,
not "good enough". v5's correct fix to Step 4D.2 body left two stale
MockCalibration labels in inventory + commit message (NEW-I1), and v5's
expansion of Step 4C.4 abbreviated SQL placeholders introduced specificity
that drifts from actual source (NEW-I2, with three concrete column/table
mismatches and a missing second SQL execution).

Recommended scope for iter-7 (single revision, ~30min):
- Update plan line 1378 — split MockCalibration row into two Noop entries (or
  relabel)
- Update plan line 1676 — fix commit message body
- Update plan lines 1232-1260 — either fully spell out the 4 abbreviated SQL
  statements (with correct table + column names + add the
  `system_metrics_hourly` companion) OR rewrite Step 4C.4 to use a
  preserve-body / parameter-binding-replacement prescription instead of a
  full-body "Before / After" pair

Phase 3 implementation BLOCKED on (a) iter-7 plan revision AND (b) PR-B1 (#508)
merge.
