# Phase 2 iter-6 Verification — Plan v6 Convergence Check

**Date:** 2026-04-25
**Plan:** `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md`
**Plan version:** v6 (commit `5eda2622`)
**Plan size:** 2603 lines
**Branch:** `refactor/timewindow-primitive`
**Worktree:** `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/timewindow-primitive/`

**Verifier:** Independent (no implementation involvement; strict, narrow scope)

---

## VERIFIED FIXED (iter-5 disposition)

### NEW-I1 (stale MockCalibration labels) — ✓ VERIFIED FIXED

`grep -n "MockCalibration"` finds 6 matches, all of which are ACCEPTABLE:

| Line | Context | Verdict |
|------|---------|---------|
| 24 | Plan version metadata describing what was changed | OK — historical record |
| 196 | Files-to-be-modified table — explanatory: "Update separate impls per Step 4D.2 — NOT a single `MockCalibration`." | OK — anti-instruction |
| 1383 | Step 4D.0 inventory row — "no `MockCalibration` — separate sync/async impls" | OK — explicit correction |
| 1455 | Step 4D.2 instructions — "Actual mock types are **two separate structs**... — NOT a single `MockCalibration`." | OK — anti-instruction |
| 2528 | Disposition table — historical reference | OK — disposition table |
| 2535 | Disposition table — historical reference | OK — disposition table |

**Step 4D.0 (line 1383) — verified:**
```
| src-tauri tests.rs Noop mocks (Phase 2 iter-5/6 corrected) | `NoopCalibrationWriter` lines 12-22 + `NoopCalibrationReader` lines 24-31 | 2 (no `MockCalibration` — separate sync/async impls) |
```

**Step 4E.1 commit body (line 1681) — verified:**
```
- 2 mocks in src-tauri/scheduler/analysis_pipeline/tests.rs (NoopCalibrationWriter sync + NoopCalibrationReader async — list_segment_time_ranges relies on trait default impl)
```

Both BOTH Noop impls are explicitly named. Verdict: ✓ CORRECTLY APPLIED.

### NEW-I2 (Step 4C.4 SQL preserve-body) — ✓ VERIFIED FIXED

Step 4C.4 (plan lines 1199-1289) now uses a proper PRESERVE-BODY pattern:

1. Lines 1222-1238 explicitly declare: "Do NOT rewrite the function body from scratch."
2. Lines 1231-1236 prescribe minimal change: replace ONLY the parameter signature + add ONE shadowing line `let (from, to) = window.to_sql_pair();`
3. Conceptual diff (lines 1238-1262) uses `/* unchanged body */` markers and explicitly notes:
   - Line 1258: "TWO executes: system_metrics + system_metrics_hourly"
   - Line 1260: "uses start_time column, NOT timestamp"
4. NO synthetic `/* DELETE FROM metrics ... */` placeholders remain that would drift from actual code.

Cross-check against actual source (`crates/oneshim-storage/src/sqlite/maintenance.rs:286-365`) confirms:
- `system_metrics` + companion `system_metrics_hourly` with `hour` column — preserved by plan
- `idle_periods.start_time` (NOT `timestamp`) — preserved by plan
- Per-execute `.map_err` with distinct error message strings — preserved by plan

The shadowing trick (`let (from, to) = window.to_sql_pair();` produces local `String` vars matching existing `params![from, to]` calls) is technically sound — `to_sql_pair()` returns `(String, String)` per spec §6.

Verdict: ✓ CORRECTLY APPLIED.

---

## NEW CRITICAL ISSUES (introduced by v6)

### NEW-C1 (v6) — Step 4C.1 calibration_store_impl.rs over-specification persists with wrong code body

**Severity:** Critical
**Location:** Plan lines 1081-1120 (Step 4C.1)
**Pattern:** Same class of error as iter-5 NEW-I2 — synthetic code skeleton drifts from actual source.

The v6 plan explicitly committed to fixing Step 4C.4 with the PRESERVE-BODY pattern but failed to apply the same standard to Step 4C.1, which has THREE separate drift instances:

#### Drift 1.1 — flag_noise_range SQL is wrong (table + column names)

Plan v6 line 1093:
```rust
"UPDATE calibration SET noise = 1 WHERE timestamp >= ?1 AND timestamp <= ?2",
```

Actual source `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:128-129`:
```rust
"UPDATE calibration_log SET is_noise = 1
 WHERE timestamp >= ?1 AND timestamp <= ?2",
```

- Plan says table `calibration` → actual is `calibration_log`
- Plan says column `noise` → actual is `is_noise`

This is the EXACT same class of mistake (synthetic SQL drift) that iter-5 NEW-I2 caught for Step 4C.4. The implementer following plan literally would silently break the SQL.

#### Drift 1.2 — get_entries skeleton uses wrong concurrency pattern

Plan v6 lines 1099-1103:
```rust
async fn get_entries(&self, window: &TimeWindow, exclude_noise: bool) -> Result<Vec<CalibrationEntry>, CoreError> {
    let conn = self.conn.lock().unwrap();
    let (from, to) = window.to_sql_pair();
    // ... existing query body with `&from, &to` substituted via params! macro
}
```

Actual source `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:148-194`:
- Uses `self.with_conn(move |conn| { ... }).await.map_err(Into::into)` async pattern
- Computes `from_str = from.to_rfc3339()` separately, NOT via `to_sql_pair()` shadowing
- 47-line body with conditional SQL (exclude_noise branch), `prepare`, `query_map`, row-by-row error handling

The plan's `let conn = self.conn.lock().unwrap()` would BREAK the async with_conn pattern. An implementer literally following this would replace working async code with broken synchronous lock.

The lock-error wrapping pattern in actual `flag_noise_range` (lines 121-124) uses typed `CoreError::Storage { code: StorageCode::Failed, message: ... }` — NOT `.unwrap()` as plan shows.

#### Drift 1.3 — list_segment_time_ranges skeleton omits table-exists check + per-row error handling

Plan v6 lines 1106-1119:
```rust
async fn list_segment_time_ranges(&self, window: &TimeWindow) -> Result<Vec<(String, TimeWindow)>, CoreError> {
    let conn = self.conn.lock().unwrap();
    let (from, to) = window.to_sql_pair();
    // existing query returns rows of (segment_id, start_ts, end_ts); map:
    let rows: Vec<(String, DateTime<Utc>, DateTime<Utc>)> = /* existing query body */;
    let result = rows.into_iter()
        .map(|(seg_id, start, end)| { ... })
        .collect();
    Ok(result)
}
```

Actual source `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:237-292`:
- Uses `self.with_conn(move |conn| { ... }).await.map_err(Into::into)` async pattern
- Has `table_exists` early-return guard (lines 247-257) for missing V9 migration
- Per-row `parse_from_rfc3339` with separate error wrapping per field
- 56-line body — non-trivial logic

The plan's collect-into-Vec-then-map pattern would lose the table-exists guard, change error semantics for missing-migration case, and replace async `with_conn` with broken sync lock.

**Required fix:** Rewrite Step 4C.1 using the same PRESERVE-BODY pattern as Step 4C.4:
1. Tell implementer to swap parameter signature ONLY
2. Tell implementer to add `let (from, to) = window.to_sql_pair();` line ONLY where needed (or `from.to_rfc3339()` shadowing for current `from_str` variable name)
3. Explicitly preserve: `with_conn` async pattern, table_exists guard, per-row parse error wrapping, typed CoreError::Storage construction
4. Delete the synthetic SQL strings (`UPDATE calibration SET noise = 1 ...`)
5. For `list_segment_time_ranges`: clearly document the change is from `Vec<(String, DateTime, DateTime)>` → `Vec<(String, TimeWindow)>` and where in the existing body to insert the TimeWindow::new mapping

---

## NEW IMPORTANT ISSUES (introduced by v6)

(empty list)

---

## FINAL CONVERGENCE AUDIT (Part C)

### Audit 1: Broken patterns from prior iterations

```
$ grep -n "MockCalibration\|self\.failure_error\|counts\.events =\|\"DELETE FROM metrics\"" plan
196:  | ... — NOT a single `MockCalibration`. |    [explanatory]
1383: | ... 2 (no `MockCalibration` — separate sync/async impls) |   [explanatory]
1455: | ... — NOT a single `MockCalibration`. ...                   [explanatory]
2528: | Important | NEW-I1 — stale MockCalibration labels | ✅ ...   [disposition]
2535: | Important | Pre-existing #1 — MockCalibration vs ... | ✅ ... [disposition]
2546: | ... was wrongly using `self.failure_error(...)` ... | ✅ ...  [disposition]
```

**Verdict:** All matches are in explanatory text or disposition tables. Zero ACTIVE broken patterns. CLEAN.

### Audit 2a: TimeWindow::new with `?` operator

```
$ grep -B 1 "TimeWindow::new(.*)\?" plan | head -30
| Build `TimeWindow::new(lookback, now).expect("lookback < now")` ...   [explanatory text — no ?]
let w = TimeWindow::new(dt(2026, 4, 1), dt(2026, 4, 25)).unwrap();    [test code — .unwrap is correct]
... [more test code with .unwrap() — correct]
```

**Verdict:** Zero `TimeWindow::new(...)?` usages in caller migration patterns. Test code correctly uses `.unwrap()`. Caller text correctly prescribes `.expect()`. CLEAN.

### Audit 2b: TimeWindow::from_rfc3339_pair with `?` operator

```
$ grep -B 1 "TimeWindow::from_rfc3339_pair(.*)\?" plan | head -30
... let restored = TimeWindow::from_rfc3339_pair(&from, &to).unwrap();   [test — correct]
... [more test code .unwrap() — correct]
let window = TimeWindow::from_rfc3339_pair(&from, &to).expect("test ts trusted");   [test caller — correct]
let window = TimeWindow::from_rfc3339_pair("2025-01-01T00:00:00Z", "2025-12-31T23:59:59Z")  [test — correct]
let window = TimeWindow::from_rfc3339_pair(&request.from, &request.to)?;   [Result-returning fn — CORRECT use of ?]
let window = TimeWindow::from_rfc3339_pair(&request.from, &request.to)?;   [Result-returning fn — CORRECT use of ?]
let window = TimeWindow::from_rfc3339_pair(&from_rfc, &to_rfc)             [Pattern A: enclosing returns Result<_, ApiError>]
```

**Verdict:** All `?` uses are in callers with Result-returning enclosing functions (Pattern A). All non-Result callers correctly use `.expect()` or `.unwrap()`. CLEAN.

### Audit 3: Same-crate import

```
$ grep -n "use oneshim_api_contracts::common::TimeRangeQuery" plan
(zero matches)
```

**Verdict:** Zero same-crate self-import bug. CLEAN.

### Audit 4 (additional): wrong-table SQL placeholders

```
$ grep -n "UPDATE calibration SET noise" plan
1093:    "UPDATE calibration SET noise = 1 WHERE timestamp >= ?1 AND timestamp <= ?2",
```

**Verdict:** ONE remaining bad SQL placeholder at Step 4C.1 line 1093 — already flagged as NEW-C1 above.

### Audit 5 (additional): synthetic concurrency pattern in async fns

```
$ grep -n "let conn = self.conn.lock().unwrap()" plan
1090:    let conn = self.conn.lock().unwrap();    [flag_noise_range — sync, may be OK conceptually]
1100:    let conn = self.conn.lock().unwrap();    [get_entries — WRONG, actual uses with_conn]
1107:    let conn = self.conn.lock().unwrap();    [list_segment_time_ranges — WRONG, actual uses with_conn]
```

**Verdict:** TWO async-pattern drifts at Step 4C.1 — already flagged as NEW-C1 sub-issues 1.2 + 1.3 above.

---

## VERDICT

**NEEDS PHASE 2 iter-7**

**Rationale:**

1. **Iter-5 NEW Important findings**: Both VERIFIED FIXED. NEW-I1 (stale MockCalibration labels) and NEW-I2 (Step 4C.4 PRESERVE-BODY pattern) are correctly applied in v6. ✓

2. **NEW Critical introduced by v6**: ONE — NEW-C1 (Step 4C.1 over-specification persists). The v6 plan correctly applied PRESERVE-BODY pattern to Step 4C.4 but FAILED to apply the same fix to Step 4C.1, which has IDENTICAL class of error in three sub-instances:
   - Drift 1.1: Wrong SQL table name (`calibration` → `calibration_log`) + wrong column (`noise` → `is_noise`)
   - Drift 1.2: Wrong async concurrency pattern (`self.conn.lock().unwrap()` instead of `self.with_conn(...).await`)
   - Drift 1.3: Lost table-exists guard + per-row error handling

3. **Audit grep**: Final integrity audit found ONE remaining synthetic SQL placeholder (already flagged as NEW-C1) and TWO synthetic async concurrency patterns (already flagged). All other patterns CLEAN.

4. **User mandate**: "Critical+Important 이슈가 없을 때까지" — converge to ZERO. v6 has 1 Critical outstanding (with 3 sub-instances). Cannot declare PHASE 2 EXIT APPROVED.

**Required iter-7 changes:**

Rewrite Step 4C.1 (plan lines 1081-1120) using the same PRESERVE-BODY discipline as Step 4C.4:
- For each of the 3 methods (`flag_noise_range`, `get_entries`, `list_segment_time_ranges`), instruct the implementer to swap ONLY the parameter signature
- Delete the synthetic SQL strings
- Delete the synthetic `let conn = self.conn.lock().unwrap()` lines
- Reference the actual line numbers (120-139, 148-194, 237-292) and prescribe surgical inserts (e.g., "before line 154, change `let from_str = from.to_rfc3339()` to `let from_str = window.start().to_rfc3339()`" or similar — let the plan-author choose the cleanest pattern)
- Note: `flag_noise_range` is sync + uses typed CoreError::Storage construction (not `.unwrap()`)
- Note: `get_entries` and `list_segment_time_ranges` are async + use `with_conn` + have separate `from_str`/`to_str` locals + table-exists guard (list_segment_time_ranges only)

For `list_segment_time_ranges` specifically, also prescribe how the return-type change `Vec<(String, DateTime, DateTime)>` → `Vec<(String, TimeWindow)>` interacts with the existing per-row parse loop: the loop currently builds `(id, start, end)` tuples line-by-line — the TimeWindow::new conversion can be inlined into the existing `result.push(...)` call without restructuring the loop.

---

## NOTE ON SCOPE

This verification was strictly scoped to:
- (A) Verify iter-5's 2 Important findings were applied
- (B) Look for NEW Critical/Important introduced by v6
- (C) Final integrity audit via 3 prescribed grep commands

I did NOT re-verify the entire plan v6 from scratch (would be iter-7's predecessor's job). However, the v5 over-specification pattern — which iter-5 caught only at Step 4C.4 — clearly persists at Step 4C.1 with the same class of error. Once v6 chose to fix one instance with PRESERVE-BODY, the corollary fix at Step 4C.1 became part of the same convergence requirement.

If iter-7 is invoked, it should:
1. Apply PRESERVE-BODY fix to Step 4C.1 (3 sub-instances)
2. Audit all other Step 4* code blocks for the same pattern (4C.2, 4C.3, 4C.5, 4C.6 should be quick — most have brief signature-only diffs already)
