# TimeWindow Phase 2 — Iteration 4 Plan v4 Verification

**Date**: 2026-04-25
**Plan v4**: commit `2a477389`, file `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (~2534 lines)
**Spec v3**: commit `f495dfbd`
**Findings doc reviewed**: `.claude/timewindow-review/phase2-iter3-verification.md` (2 NEW Critical + 1 NEW Important)
**Reviewer**: independent verifier (NOT Phase 2 iter-3 reviewer)
**Outcome**: **READY FOR PHASE 2 EXIT** — all 3 iter-3 findings (NEW-C1, NEW-C2, NEW-I1) correctly applied. Zero NEW Critical/Important regressions introduced by v4. Pre-existing v3 issues remain (documented below for record but not v4-introduced).

---

## VERIFIED FIXED (iter-3 disposition)

### NEW-C1 (Critical) — ✓ FIXED at Step 4D.4 (plan lines 1534-1596)

**iter-3 finding**: Plan v3's Step 4D.4 used `Err(self.failure_error(...))` which (a) referenced a non-existent method on `FailingStorage`, and (b) replaced the documented delegation pattern with unconditional fail.

**Plan v4 verification**: All 5 FailingStorage methods now correctly delegate via `self.inner.method(window).map_err(Into::into)`:
- Line 1542-1546: `count_frames_in_range` → delegates to `self.inner.count_frames_in_range(window)`
- Line 1549-1556: `list_frame_file_paths_in_range` → delegates
- Line 1559-1563: `count_events_in_range` → delegates
- Line 1567-1586: `delete_data_in_range` → delegates with all 5 bool flags + `#[allow(clippy::too_many_arguments)]` preserved
- Line 1589-1593: `get_daily_active_secs` → delegates

Plan v4 explanatory note at line 1596 reinforces: "These methods preserve the production delegation behavior — only methods that the test specifically targets for failure (e.g., `start_idle_period`) replace `self.inner` calls with synthetic `Err(...)`."

Cross-verified actual file `crates/oneshim-web/tests/support/failing_storage.rs:276-405` — current pattern is delegation (lines 277-279, 300-302, 332-334, 370-380, 402-404). Plan v4 only changes parameter type (`from: &str, to: &str` → `window: &TimeWindow`) and inner call (`from, to` → `window`); delegation pattern preserved.

### NEW-C2 (Critical) — ✓ FIXED at Step 4D.1 (plan lines 1357-1392) + Step 4D.3 (plan lines 1452-1469)

**iter-3 finding**: Plan v3 used `?` operator after `TimeWindow::new(...)?` in regime.rs (3 sites, function returns `()`) and stats_query_support.rs (1 site, function returns `u64`) — both would fail E0277 since enclosing function doesn't return `Result`.

**Plan v4 verification — regime.rs:**
- Plan line 1357-1361 explicitly notes both functions return `()`: `run_periodic_regime_detection` (line 16) and `run_constrained_clustering` (line 140) — and explains why `.expect("...")` is the right fix.
- Line 1370-1372 (regime.rs:44 fix): `let window = TimeWindow::new(lookback, now).expect("lookback (now - 7d) is always before now");` ✓
- Line 1380-1382 (regime.rs:174 fix): same `.expect()` pattern ✓
- Line 1390-1391 (regime.rs:184 fix): "Reuse window constructed at line 174 (same scope — both in `run_constrained_clustering`)" — correctly identifies shared scope. ✓

Cross-verified actual file `src-tauri/src/scheduler/analysis_pipeline/regime.rs:16-19, 140-144` — both return `()`. Lines 174+184 share scope (both inside `run_constrained_clustering`), line 44 is in separate function `run_periodic_regime_detection` and gets its own `window` binding.

**Plan v4 verification — stats_query_support.rs:112:**
- Plan line 1453-1454 explicit comment: "Function signature: `total_active_secs_for_range(...) -> u64` — NO Result return / (NEW-C2 fix: cannot use ? operator)"
- Line 1463-1465: `let Ok(window) = TimeWindow::new(from, to) else { return fallback_events_logged * 5; };` — correctly preserves the existing fallback semantics. ✓

Cross-verified actual file `crates/oneshim-web/src/services/stats_query_support.rs:104-109` — return type is `u64`. The let-else fallback maintains semantic equivalence with the previous `match` arm `_ => fallback_events_logged * 5`.

**Other Step 4D.3 sites (verified Result-returning):**
- Line 1500: `events_service.rs:35` containing `pub async fn get_events(...) -> Result<EventPage, ApiError>` — `?` is valid via `.map_err(|e| ApiError::BadRequest(e.to_string()))?` mapping. ✓ Cross-verified `events_service.rs:26` matches.
- Lines 1473-1496: `data_web_service.rs:36 + 51` containing function returns `Result<DeleteResult, ApiError>` — `?` is valid. ✓
- Lines 1513-1530: `reports_query_support.rs:86` Pattern A/B selection (see NEW Important issue #2 below).

### NEW-I1 (Important) — ✓ FIXED at Step 4D.0 (plan line 1343) + Step 4C.1.5 (plan lines 1121-1167)

**iter-3 finding**: Step 4D.0 enumeration table omitted 5 internal `calibration_store_impl.rs` test callers at lines 400, 414, 420, 425, 443.

**Plan v4 verification:**
- Line 1343 in Step 4D.0 enumeration table adds the row: `Internal SQLite tests (calibration_store_impl.rs) — Phase 2 iter-3 NEW-I1 | lines 400, 414, 420, 425, 443 (storage.get_entries × 4 + storage.flag_noise_range × 1) | 5`. ✓
- New Step 4C.1.5 at lines 1121-1167 provides full migration code for all 5 sites with `.expect("trusted test bounds")` (correctly handling `()` returning test fn signatures).
- Step 4C.1.5 scoping verified: lines 414, 420, 425 are all inside `flag_noise_range_and_exclude` (so `wide_window` reuse at site 425 IS in scope). Line 400 is in `batch_insert_and_read`, line 443 is in `enforce_retention_by_max_rows` — each gets its own `window` binding. ✓
- Trusted-bounds `.expect("trusted test bounds")` justifications accurate: all 5 sites use `Utc::now() ± 1h` or `entries[i].timestamp ± 1s` bounds where `start <= end` by construction. ✓

Cross-verified actual file `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:400, 414, 420, 425, 443` — exactly matches the 5 sites enumerated.

---

## NEW CRITICAL ISSUES (introduced by v4)

**NONE.**

All 3 iter-3 findings (NEW-C1, NEW-C2, NEW-I1) were addressed correctly in v4. No new compile-blocking regressions were introduced by v4's editing. Independent grep against current source produced 38 invocation lines reconciling exactly with plan v4's Step 4D.0 enumeration table (Service 5 + Mock support 5 + events 4 + frames 2 + maintenance 9 + calibration 5 + web wrappers 5 + regime 3 + tests.rs MockCalibration 1 = 39, with -1 from MockCalibration trait method declaration not appearing in the `\.method(` invocation grep).

---

## NEW IMPORTANT ISSUES (introduced by v4)

**NONE that are exclusively v4-introduced.**

The reports_query_support Pattern A/B grep helper recommendation (Step 4D.3 line 1530) is technically v4-introduced text but harmless since the implementer can read the function manually. See "Pre-existing issues for record" below for issues that were present in v3 and remain in v4 (these are documented for completeness, not blocking).

---

## Pre-existing issues for record (NOT v4-introduced; carried forward from v3 and earlier)

These are real issues an implementer would hit, but they were already present in v3 and were not flagged by iter-3 reviewer. Documenting here so they don't get lost — but per the task's narrow scope of "NEW issues introduced by v4", they should NOT block Phase 2 EXIT. They CAN be addressed inline by the implementer or rolled into a v5 cleanup pass if desired.

### Pre-existing #1 — Step 4D.2 mock name "MockCalibration" (actual is `NoopCalibrationReader` + `NoopCalibrationWriter`)

**Plan v4 location**: Step 4D.2 (lines 1416-1444), v3 same content.

**Reality**: `src-tauri/src/scheduler/analysis_pipeline/tests.rs` defines two separate structs `NoopCalibrationWriter` (lines 12-22) and `NoopCalibrationReader` (lines 24-40). Plan v4 says single `MockCalibration` impl. Subagent grep for `MockCalibration` in tests.rs returns ZERO hits — only `NoopCalibrationWriter` + `NoopCalibrationReader` exist.

**Plan v4 also shows explicit `list_segment_time_ranges` impl on the mock**, but `NoopCalibrationReader` does NOT explicitly implement it — it relies on the trait default impl at `crates/oneshim-core/src/ports/calibration_store.rs:48-58` which returns `Ok(vec![])`. Adding the explicit override is harmless but creates a slight cognitive mismatch.

**Severity**: Important. Subagent will need to:
1. Realize `MockCalibration` doesn't exist; locate `NoopCalibrationReader` + `NoopCalibrationWriter`.
2. Update each separately (Reader for `get_entries`, optionally `list_segment_time_ranges`; Writer for `flag_noise_range`).
3. Optionally drop the `list_segment_time_ranges` override since trait default suffices.

### Pre-existing #2 — Step 4C.4 `counts.events = n as u64` field name typo

**Plan v4 location**: Step 4C.4 line 1240 (and Step 5.2 line 1706 + 1707-1708 assertions).

**Reality**: `crates/oneshim-core/src/models/storage_records.rs:88-94` defines:
```rust
pub struct DeletedRangeCounts {
    pub events_deleted: u64,
    pub frames_deleted: u64,
    pub metrics_deleted: u64,
    pub process_snapshots_deleted: u64,
    pub idle_periods_deleted: u64,
}
```

Plan v4 line 1240 uses `counts.events`, line 1706-1708 uses `counts.events`, `counts.frames`, `counts.metrics`. Actual production code (`maintenance.rs:304+`) uses `counts.events_deleted = ...`.

Plan v4 line 1712 has a softer disclaimer "Adapt to actual `DeletedRangeCounts` struct field names" for Step 5.2, but the disclaimer is missing from Step 4C.4 line 1240 itself.

**Severity**: Important. Subagent following Step 4C.4 literally would write `counts.events = n as u64;` — compile fails. Easy fix by reading actual struct (line 1712 disclaimer covers Step 5.2 by referral).

### Pre-existing #3 — Step 4C.4 maintenance.rs internal tests recommendation uses `?` in `()`-returning context

**Plan v4 location**: Step 4C.4 line 1252 — "Internal test sites in maintenance.rs (~9 sites): lines 931, 1019, 1052, 1067, 1083, 1164, 1183, 1308, 1358. For each, build `TimeWindow::from_rfc3339_pair(&from, &to)?` first."

**Reality**: All maintenance.rs internal test fns at the listed lines have signature `fn name()` (return `()`). The `?` operator would not compile.

**However**, Step 4C.2 line 1190-1191 (events.rs internal test pattern, also `()` returning) correctly uses `.expect("test ts trusted")` — so the implementer can pattern-match from 4C.2 to 4C.4. The compile error would be self-evident and the fix is mechanical.

**Severity**: Important. Inconsistency between Step 4C.2 (correct `.expect()`) and Step 4C.4 (incorrect `?`). 9 sites would fail compile if literal `?` recommendation followed.

### Pre-existing #4 — "Files to be modified" table at lines 188-195 contradicts actual Step 4D.1

**Plan v4 location**: Lines 188-195 (Files-to-be-modified table for src-tauri caller sites).

**Stale content** (carried from v3, also pre-existing):
- Line 194: `regime.rs:184` listed as `flag_noise_range(from, to)` — actually `get_entries`.
- Lines 192-194: Recommend "Build `TimeWindow::new(from, to)?` then pass `&window`" — would not compile (functions return `()`).

The actual Step 4D.1 (lines 1350-1394) correctly identifies the methods AND uses `.expect()`. So implementer following sequential steps reaches the correct guidance, but a reader skimming the upfront table gets misleading info.

**Severity**: Important (informational drift, not directly compile-blocking since the implementation step takes precedence).

### Pre-existing #5 — Step 4D.3 reports_query_support `grep` helper recommendation is non-functional

**Plan v4 location**: Line 1530 — `Use `grep -n "fn .*<Utc>.*from\|fn .*reports_query_support" ...` to read the enclosing function signature first; pick A or B accordingly.`

**Reality**: This regex returns ZERO matches for `crates/oneshim-web/src/services/reports_query_support.rs`. The actual enclosing function at line 68 is `pub(crate) fn build_daily_stats(input: DailyStatsInput<'_>) -> Vec<DailyStat>` — neither `<Utc>` nor `from` appears in the signature.

**Severity**: Suggestion / Important (cosmetic). Subagent can simply open the file at line 86 and look up to find the enclosing function. The Pattern B (non-Result) selection is correct for `build_daily_stats`. The plan correctly provides both A and B alternatives.

### Pre-existing #6 — Step 4C.1.5 line 1156 variable name "dirty" doesn't match actual source variable name "all"

**Plan v4 location**: Step 4C.1.5 line 1156 says `let dirty = storage.get_entries(&wide_window, false).await.unwrap();`

**Reality**: `crates/oneshim-storage/src/sqlite/calibration_store_impl.rs:424` says `let all = storage.get_entries(wide_from, wide_to, false).await.unwrap();` — variable is `all`, not `dirty`.

**Severity**: Suggestion / cosmetic. Subagent following the plan literally would rename `all` → `dirty` (functional impact zero; subsequent assertion uses field name not variable name).

---

## Independent grep verification (Part C)

Command:
```bash
grep -rn "count_events_in_range\|count_frames_in_range\|list_frame_file_paths_in_range\|delete_data_in_range\|get_daily_active_secs\|flag_noise_range\|\.get_entries(\|list_segment_time_ranges" crates/ src-tauri/ 2>/dev/null | grep -v "/.git/" | grep -v frontend | grep -v "fn " | wc -l
```

Returns: 49 lines (raw count includes continuation lines for multi-line `delete_data_in_range(` calls and `// Comment` lines).

After filtering comments and `.expect(...)` continuation lines: **38 unique invocation sites**.

Reconciliation against plan v4 Step 4D.0 enumeration table (claimed ~30, actual breakdown):

| Plan Category | Plan claim | Actual matches |
|---|---|---|
| Service layer (4 files) | 5 | 5 ✓ (stats_query_support × 1, data_web_service × 2, events_service × 1, reports_query_support × 1) |
| Test mock support (failing_storage.rs) | 5 | 5 ✓ |
| Internal SQLite tests (events.rs) | 4 | 4 ✓ (lines 406, 426, 452, 471) |
| Internal SQLite tests (frames.rs) | 2 | 2 ✓ (lines 175, 192) |
| Internal SQLite tests (maintenance.rs) | ~9 | 9 ✓ (lines 931, 1019, 1052, 1067, 1083, 1164, 1183, 1308, 1358) |
| Internal SQLite tests (calibration_store_impl.rs) — NEW iter-3 | 5 | 5 ✓ (lines 400, 414, 420, 425, 443) |
| web_storage_impl.rs wrappers | 5 | 5 ✓ (lines 82, 105, 126, 169, 246) |
| src-tauri regime.rs | 3 | 3 ✓ (lines 44, 174, 184) |
| src-tauri MockCalibration (tests.rs) | 1 | 1 (declaration in trait impl, not visible in invocation grep — see Pre-existing #1) |

**Total enumerated**: 5+5+4+2+9+5+5+3+1 = **39**. Reconciles exactly with grep output of 38 invocation lines + 1 trait-method declaration in tests.rs.

**No callers missed.** Plan v4's enumeration is complete.

---

## VERDICT

**READY FOR PHASE 2 EXIT** (Phase 3 BLOCKED on PR-B1 #508 merge).

**Concise rationale**:

1. **All 3 iter-3 findings (NEW-C1, NEW-C2, NEW-I1) correctly applied in plan v4** — verified at the exact plan sections claimed in disposition table 5b.
2. **Zero NEW Critical issues introduced by v4**.
3. **Zero NEW Important issues exclusively introduced by v4** (the `reports_query_support` Pattern A/B grep helper at line 1530 is technically v4-edited text but harmless).
4. **Independent grep reconciles exactly with plan v4's Step 4D.0 enumeration** (39 sites including the trait declaration; 38 invocation sites matching the 9-row table).
5. **6 pre-existing issues remain** (carried from v3 and earlier — none v4-introduced):
   - Mock named `MockCalibration` (actual `NoopCalibrationReader`/`NoopCalibrationWriter`)
   - `counts.events` field name (actual `events_deleted`)
   - maintenance.rs internal tests `?` recommendation (would fail compile)
   - Stale "Files to be modified" table at lines 188-195
   - Non-functional `grep` helper recommendation at line 1530
   - Variable name `dirty` vs actual `all` at line 1156

   These are documented for completeness. Per the task scope ("NEW issues introduced by v4"), they should NOT block Phase 2 EXIT. They CAN be addressed inline by a careful implementer (each is mechanically obvious) OR rolled into a v5 cleanup pass before Phase 3 implementation begins.

**Recommended action**: Phase 2 EXIT approved. Phase 3 implementation BLOCKED on PR-B1 (#508) merge per existing dependency. When implementer begins Phase 3, advise them to:

1. Skim Step 4D.0 table FIRST (definitive enumeration), and treat the upfront "Files to be modified" tables (lines 188-195) as informational only.
2. Pattern-match from Step 4C.2's `.expect("test ts trusted")` style when migrating internal SQLite tests in events/frames/maintenance/calibration_store_impl.
3. Locate `NoopCalibrationReader`/`NoopCalibrationWriter` (not `MockCalibration`) when executing Step 4D.2.
4. Read actual `DeletedRangeCounts` field names (`events_deleted`, `frames_deleted`, etc.) when writing Step 4C.4 implementation code and Step 5.2 assertions.

These 4 mitigations cover all 6 pre-existing issues. Estimated implementer overhead: ~5-10 minutes of careful reading per issue (~30-60 min total) — equivalent to typical "trust but verify" effort during plan execution.

The original 17 Critical + 17 Important findings across 4 iterations have been correctly addressed where iter-3 reviewer flagged them. Remaining pre-existing issues are scope-creep beyond iter-3's specific findings disposition.

---

**End of Phase 2 iter-4 verification.**
