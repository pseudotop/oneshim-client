# Phase 2 iter-7 Verification — Plan v7 Convergence Audit

**Reviewer:** independent verifier (fresh subagent)
**Date:** 2026-04-25
**Plan version:** v7 (commit `8dcad4c0`)
**Plan length:** 2687 lines
**Branch:** `refactor/timewindow-primitive`
**Worktree:** `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/timewindow-primitive`
**User mandate:** "Critical+Important 이슈가 없을 때까지" — converge to zero.

---

## VERIFIED FIXED (iter-6 NEW-C1 disposition)

### NEW-C1: Step 4C.1 calibration_store_impl.rs PRESERVE-BODY rewrite — VERIFIED FIXED

**Required (per iter-6 verification):** Step 4C.1 must use PRESERVE-BODY pattern matching Step 4C.4
style; reference actual table `calibration_log`/`is_noise`; use fallible `lock().map_err(...)?` not
`lock().unwrap()`; preserve async `with_conn(move |conn| {...}).await` pattern; preserve
`table_exists` V9 migration guard for `list_segment_time_ranges`; preserve per-row
`parse_from_rfc3339` error wrapping; show diff blocks limited to parameter-sig swap + locals
binding line + return-type consolidation.

**Verification (cross-checked against actual source `calibration_store_impl.rs:120-292`):**

| Requirement | Plan v7 | Actual Source | Status |
|-------------|---------|---------------|--------|
| Table name `calibration_log` (NOT `calibration`) | line 1097, 1117 | line 128, 162-172 | ✓ |
| Column `is_noise` (NOT `noise`) | line 1097, 1117 | line 128, 162-172 | ✓ |
| Fallible `lock().map_err(\|e\| CoreError::Storage { code, message })?` | line 1090-1093 | line 121-124 | ✓ |
| `flag_noise_range` is sync, returns `Result<u64, CoreError>` | line 1089 / line 1114 | line 120 (sync `fn`) | ✓ |
| `get_entries` async, uses `self.with_conn(move \|conn\| {...}).await.map_err(Into::into)` | line 1144 narrative | line 157, 192-194 | ✓ |
| `from_str` / `to_str` String shadowing preserved (closure captures by `move`) | line 1142-1143 | line 154-155, 181 | ✓ |
| `list_segment_time_ranges` `table_exists` V9 migration guard preserved | line 1170-1171 | line 246-257 | ✓ |
| `list_segment_time_ranges` per-row `parse_from_rfc3339` error wrapping preserved | line 1179-1180 | line 280-285 | ✓ |
| Return-type consolidation `Vec<(String, DateTime<Utc>, DateTime<Utc>)>` → `Vec<(String, TimeWindow)>` (per iter-2 N-C4) | line 1158/1164 | line 241 (current shape) | ✓ |
| `expect("DB-stored segment ranges are trusted (start <= end invariant)")` rationale | line 1184-1185 | (will be added by impl) | ✓ |
| Diff blocks show ONLY parameter sig swap + locals binding + return-type consolidation | lines 1112-1119 (method 1), 1128-1146 (method 2), 1152-1191 (method 3) | n/a | ✓ |

### NEW-C1 Test-Site Migration (Step 4C.1.5) — VERIFIED FIXED

| Requirement | Plan v7 | Actual Source | Status |
|-------------|---------|---------------|--------|
| Test sites at lines 400, 414, 420, 425, 443 | line 1207, 1214, 1221, 1228, 1240 | source line 400, 414, 420, 425, 443 | ✓ |
| Variable name `all` (NOT `dirty`) at line 425 | line 1228, 1230, 1235 | line 424 (`let all = storage`) | ✓ |
| Reuse `wide_window` between line 420 and 425 | line 1234 | source: same `wide_from`/`wide_to` chain | ✓ |
| `flag_noise_range` test caller is sync (no `.await`) | line 1219 | line 414 (no `.await`) | ✓ |

**Conclusion (Part A): NEW-C1 from iter-6 is fully and correctly applied in v7. PRESERVE-BODY
discipline is consistent with Step 4C.4 (both methods use the same minimal-diff philosophy, with
appropriate stylistic differences in the locals binding to match the actual body's typing
requirements — `DateTime<Utc>` for flag_noise_range, `String` for delete_data_in_range).**

---

## NEW CRITICAL ISSUES (introduced by v7)

(empty list)

No new Critical issues identified in v7. The Step 4C.1 PRESERVE-BODY rewrite:
- Does not introduce any wrong type (DateTime<Utc> vs String distinction is preserved per body needs)
- Does not introduce broken diff (each diff block clearly shows pre/post with `-`/`+`)
- Does not introduce wrong line numbers (verified all line refs against actual source)
- Does not regress on any previously-fixed issue (table name, column name, lock pattern, async
  pattern, table_exists guard, per-row parse all preserved)

---

## NEW IMPORTANT ISSUES (introduced by v7)

(empty list)

No new Important issues identified in v7. Disposition table 5e is correctly added with the single
NEW-C1 entry; v7 commit message correctly references iter-6's missed PRESERVE-BODY pattern with
explicit enumeration of the 6 sub-errors v6 made.

---

## SAME-CLASS AUDIT (Steps 4C.2/4C.3/4C.5/4C.6 needing PRESERVE-BODY?)

Examined each remaining Step 4C step against the actual source to determine whether they require
the same PRESERVE-BODY discipline as Step 4C.1/4C.4, or whether the synthetic snippets are
safe-synthetic given the body's simplicity.

### Step 4C.2 (events.rs `count_events_in_range` line 14 + 4 callers at 406, 426, 452, 471)

- Body inspection (events.rs:14-29): single `conn.query_row("SELECT COUNT(*) FROM events ... ?1 ... ?2", params![from, to], ...)` with `lock().map_err(...)?` and `query_row.map_err(...)?` — total 16 lines. No table-name pitfalls (table is plain `events`), no async, no companion table, no per-row error wrapping.
- Plan v7 snippet (line 1252-1262): `let (from, to) = window.to_sql_pair();` + `// query body unchanged` comment.
- Caller line numbers: source at 406, 426, 452, 471 — all four match plan v7's claim.

**Verdict: SAFE-SYNTHETIC.** The body is simple enough that the `// query body unchanged` shorthand
is unambiguous. No drift risk. PRESERVE-BODY not needed.

### Step 4C.3 (frames.rs `count_frames_in_range` line 10 + 2 callers at 175, 192)

- Body inspection (frames.rs:10-25): identical structural pattern to events.rs (single query_row, plain `frames` table). 16 lines.
- Plan v7 snippet (line 1273-1275): one-line "Same pattern as 4C.2" reference.
- Caller line numbers: source at 175, 192 — both match plan v7's claim.

**Verdict: SAFE-SYNTHETIC.** Body trivially simple, callers verified. No drift risk.

### Step 4C.5 (work_sessions.rs `get_daily_active_secs` line 216)

- Body inspection (work_sessions.rs:216-246): 30 lines — uses `lock().map_err(...)?`, `prepare(...)`, `query_map(params![from, to], ...)`, `flatten().collect()`. SQL queries `work_sessions` table with `started_at >= ?1 AND started_at < ?2` (column `started_at`, not `timestamp`).
- Plan v7 snippet (line 1369-1377): only signature swap (4 lines). Does NOT prescribe body changes.
- Inherent fn signature change is purely sig-swap; body uses `params![from, to]` which works whether `from`/`to` are `&str` (current) or `String` (post-`to_sql_pair()`) — both auto-borrow to `&str`.

**Verdict: SAFE-SYNTHETIC.** The plan only changes the signature. The reader will infer the
`let (from, to) = window.to_sql_pair();` line is needed (mirroring Step 4C.4 pattern). However,
this IS a minor weakness — the plan does not explicitly prescribe the locals-binding line for
this method. Given that this is a single-method file, the implementer can easily figure it out by
mirroring Step 4C.4. NOT escalated to Important because it is a 30-line trivial body and the
implementer reading this plan will already have processed Steps 4C.1+4C.4.

### Step 4C.6 (web_storage_impl.rs 5 wrapper sites)

- Body inspection (web_storage_impl.rs:80-247): each wrapper is a single-line forward call: `SqliteStorage::method(self, from, to, ...).map_err(Into::into)`. No internal logic — just delegation.
- Plan v7 snippet (line 1383-1426): full code blocks for all 5 wrappers showing the post-state with `window: &TimeWindow` parameter and `SqliteStorage::method(self, window, ...).map_err(Into::into)`.
- Line numbers: source at 82 (count_events body), 105 (count_frames body), 126 (list_frame body), 169 (delete_data body), 246 (get_daily body) — labels reference the body call line, not the fn signature start. This labeling convention is internally consistent with the rest of plan v7 (verified by checking 4C.6 narrative wording "5 wrappers (verified locations: lines 82, 105, 126, 169, 246)" — these are the body call lines visible to a reader who runs `grep -n "SqliteStorage::method_name(self," web_storage_impl.rs`).

**Verdict: SAFE-SYNTHETIC.** Each wrapper is a single-line delegation; full code blocks shown;
no body logic to lose. No drift risk.

---

## FINAL CONVERGENCE AUDIT (Part C grep results)

### 1. Wrong-table SQL placeholders (`DELETE FROM metrics`, `UPDATE calibration SET`, `noise = 1`)

```
1097:            "UPDATE calibration_log SET is_noise = 1
1117:      // ... entire body unchanged: lock + execute on calibration_log SET is_noise = 1 + debug! + Ok(updated as u64)
```

**Interpretation:** Both matches are CORRECT actual-SQL substrings (`calibration_log` table,
`is_noise = 1` column). The grep regex `noise = 1` is a substring of the correct token
`is_noise = 1`. A tighter regex `\bnoise = 1\b` returns ZERO matches, confirming no naked `noise`
column references remain. ✓ CLEAN.

### 2. MockCalibration

7 matches, all in legitimate corrective context:
- line 24 (changelog narrative explaining v5 fixed `MockCalibration` → `NoopCalibrationReader`+`NoopCalibrationWriter`)
- line 196 (Files-to-be-modified table noting "Update separate impls per Step 4D.2 — NOT a single `MockCalibration`")
- line 1461 (Caller-site enumeration noting "no `MockCalibration` — separate sync/async impls")
- line 1533 (Step 4D.2 narrative noting "Actual mock types are **two separate structs**: `NoopCalibrationWriter` ... — NOT a single `MockCalibration`")
- line 2612, 2619 (disposition tables 5d/5c documenting the v5 cleanup)

All references are corrective documentation — none prescribe MockCalibration as the active code
target. ✓ CLEAN.

### 3. `self.conn.lock().unwrap()`

Zero matches. ✓ CLEAN. (All synthetic `lock()` patterns in the plan use the actual fallible
`map_err(...)?` form.)

### 4. Same-crate import errors (`use oneshim_api_contracts::common::TimeRangeQuery`)

Zero matches. ✓ CLEAN. (Plan correctly uses `crate::common::TimeRangeQuery` for in-crate imports per iter-2 N-I2 fix.)

### Additional probes (deeper audit)

- `\bnoise = 1\b` (without `is_`): zero matches ✓
- `FROM calibration\b` / `UPDATE calibration\b` / `INSERT INTO calibration\b` / `DELETE FROM calibration\b`: zero matches ✓ (only `calibration_log` form exists, which is correct)
- `unwrap_or_else.*lock` / `expect.*lock`: zero matches ✓ (no synthetic-divergent lock-error fallbacks)

---

## V7 COMMIT MESSAGE VERIFICATION

```
docs(plan): TimeWindow refactor — Phase 2 iter-7 plan v7 (1 NEW Critical fix)

Addresses Phase 2 iter-6 verification (.claude/timewindow-review/phase2-iter6-verification.md).
v6 fixed Step 4C.4 with PRESERVE-BODY pattern but missed applying same discipline to Step 4C.1.

Critical fix (1):
- NEW-C1: Step 4C.1 calibration_store_impl.rs rewritten with PRESERVE-BODY pattern.
  v6 had multiple synthetic-drift errors:
  - Wrong table: "calibration" → actual is "calibration_log"
  - Wrong column: "noise = 1" → actual is "is_noise = 1"
  - Wrong lock pattern: lock().unwrap() → actual is fallible map_err to CoreError::Storage
  - Wrong concurrency model: lock().unwrap() → actual async with_conn(move |conn| {...}).await pattern
  - Lost table_exists guard for V9 migration in list_segment_time_ranges
  - Lost per-row parse_from_rfc3339 error wrapping per field
  ...
```

Commit message correctly references iter-6 verification artifact, explicitly enumerates v6's
6 missed sub-errors, prescribes the v7 fix discipline, and forecasts the next iteration. ✓ CORRECT.

---

## VERDICT

**PHASE 2 EXIT APPROVED — plan v7 has zero outstanding Critical/Important issues.**

Rationale:
1. iter-6 NEW-C1 is fully applied in v7 (Step 4C.1 PRESERVE-BODY rewrite verified against actual source on every dimension: table name, column name, lock pattern, async pattern, table_exists guard, per-row parse error wrapping, return-type consolidation, line numbers).
2. No new Critical or Important issues introduced by v7.
3. Same-class audit of Steps 4C.2/4C.3/4C.5/4C.6 confirms they are all safe-synthetic — bodies are simple enough that PRESERVE-BODY is not strictly required, and line-number/caller-site claims verified against actual source.
4. Final convergence grep audit returns zero genuine drift patterns. The two non-zero matches in (1) and seven matches in (2) are all legitimate (correct SQL substrings or corrective documentation).
5. v7 commit message correctly references iter-6 verification and enumerates the 6 sub-errors of v6's missed pattern.
6. Plan v7 spans 2687 lines (~7-iteration cumulative addressed: 18 Critical + 23 Important + 2 Suggestion). The disposition table 5e closes the iter-6 gap.

User mandate "converge to zero" is satisfied for Phase 2.

**Next action:** Phase 3 implementation can proceed (BLOCKED on PR-B1 #508 merge per plan PF1).
No iter-9 needed.

---

## STATISTICS

- Total iterations to convergence: 7 (Phase 2 iter-1 through iter-7)
- Cumulative findings addressed: 18 Critical + 23 Important + 2 Suggestion = 43 issues
- Plan growth: ~600 lines → 2687 lines (4.5x)
- Final disposition tables: 5 (5a/5b/5c/5d/5e)
- All disposition entries verified ✅ in v7
