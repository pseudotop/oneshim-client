# TimeWindow Refactor — Phase 3 Readiness State

**Date**: 2026-04-25
**Worktree**: `.claude/worktrees/timewindow-primitive`
**Branch**: `refactor/timewindow-primitive`
**Base commit**: `2ba38cf5` (pre-PR-B1)
**Status**: Phase 1 + Phase 2 CLOSED; Phase 3 BLOCKED on PR #508 merge

---

## Phase Tracker

| Phase | Status | Iterations | Final Commit |
|-------|--------|------------|--------------|
| 0 — Spec extraction | ✅ COMPLETE | 1 | `1c38cabd` (v1) |
| 1 — Spec Deep Review | ✅ CLOSED | 3 | `f495dfbd` (v3) |
| 2 — Plan Deep Review | ✅ CLOSED | 8 | `8dcad4c0` (v7) |
| 2 — Plan EXIT recorded | ✅ APPROVED | 1 | `40701ebd` |
| 3 — Implementation | 🔒 **BLOCKED on PR #508 merge** | 0 | — |

---

## Cumulative Issues Addressed

Across 11 iterations (3 spec + 8 plan):
- **Critical**: 18 (4 Phase 1 + 9+6+2+1 Phase 2 across iter-1/2/3/6)
- **Important**: 23 (5 Phase 1 + 5+5+1+4+2 Phase 2 across iter-1/2/3/4/5)
- **Suggestion / Nice-to-have**: 6 (4 Phase 1 + 2 Phase 2 iter-4)

Plan grew from 1392 lines (v1) to 2687 lines (v7). All findings docs stored in `.claude/timewindow-review/`.

---

## PF3 Baselines Captured (2026-04-25)

### Wire-Code Snapshot

**Current count**: **42 codes** (verified via `wc -l crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`)

**Alphabetical neighborhood for `time_window.*` insertion** (verified via `grep -n "^st\|^ti\|^tr\|^ui\|^update\|^validation"`):
```
39: storage.failed
[INSERT HERE if PR-B1 NOT merged → time_window.inverted_bounds + time_window.parse_failed]
40: ui.element_missing
41: validation.invalid_arguments
42: validation.invalid_field
```

**After PR-B1 (#508) merge** (expected): +5 tracking_schedule.* codes → **47 codes**, alphabetical block becomes:
```
storage.failed
time_window.inverted_bounds        ← TimeWindow refactor inserts here (post-PR-B1)
time_window.parse_failed           ← TimeWindow refactor inserts here
tracking_schedule.invalid_window   ← from PR-B1
tracking_schedule.overlap_detected ← from PR-B1
... (3 more PR-B1 codes if 5 total)
ui.element_missing
```

**After TimeWindow merge** (target): **49 codes** (47 + 2 new from this refactor).

### i18n Test Count Assertions

**File**: `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts`

**Locations** (verified via `grep -n "toHaveLength"`):
- **Line 30**: `expect(registry).toHaveLength(42)` — D7 addition comment
- **Line 122**: `expect(translatedCodes('en')).toHaveLength(42)` — D7 addition comment

Both must be updated to `BASELINE_AT_IMPL_TIME + 2` per Plan Step 2.3.

### Baseline GREEN Verification (PF4 partial)

```bash
$ cargo test -p oneshim-core --test wire_contract_snapshot
running 1 test
test wire_codes_match_expected_snapshot ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

✅ Wire snapshot baseline GREEN.

Full `cargo check --workspace` + `cargo test --workspace` deferred to PF4 (Phase 3 start) — heavy operation, not needed pre-merge.

---

## Drift Audit vs origin/main (re-checked 2026-04-25 ~16:30)

Worktree is 12 ahead / 2 behind `origin/main`. The 2 new commits since `2ba38cf5`:
- `89ab7910` refactor(external-grpc): extract serve_external_inner shared core (#506)
- `54c894d5` ci(lefthook): expand clippy 1.95 scan with field_reassign_with_default (#509)

**Drift impact on TimeWindow refactor**: ZERO.
- Wire-code baseline on origin/main: still **42** (no new codes added)
- i18n test count assertions on origin/main: still both `toHaveLength(42)` at lines 30 + 122
- File diff in TimeWindow-touched paths: only `crates/oneshim-web/src/grpc/external/{mod,test_support}.rs` changed (PR #506). These files are NOT in TimeWindow scope — no path conflict with plan v7.

PF2 rebase will be trivial when Phase 3 starts. After PR #508 merges, baseline jumps to ~47 (PR-B1 adds tracking_schedule.* codes); TimeWindow then adds 2 more → 49.

---

## PF5 Dep Verification (2026-04-25, post plan v13)

All 5 plan-assumed dependencies verified to exist in current worktree source:

| # | Assumption | Verification | Status |
|---|------------|--------------|--------|
| 1 | `oneshim-core` workspace dep on `oneshim-api-contracts` | `grep -E "^oneshim-core" crates/oneshim-api-contracts/Cargo.toml` → `oneshim-core = { workspace = true }` | ✅ |
| 2 | CoreError struct-variant pattern (Phase 2 iter-1 C1) | `Storage { code: StorageCode, message: String }` + `Network { code: NetworkCode, message: String }` at error.rs:16, 120 | ✅ |
| 3 | `From<CoreError> for ApiError` impl exists with addable arm position (Phase 2 iter-1 C2) | error.rs:56-67 — uses `match` with arms like `CoreError::Validation { field, message, .. } => ApiError::BadRequest(...)`. Plan Step 1.9 can add `CoreError::TimeWindow { message, .. } => ApiError::BadRequest(message)` in same shape | ✅ |
| 4 | `define_code_enum!` macro available (Plan Step 1.6) | `macro_rules! define_code_enum` defined at `crates/oneshim-core/src/error_codes/macros.rs`. AudioCode example confirms usage pattern matches plan. | ✅ |
| 5 | ErrorResponse schema has NO `code` field (Phase 2 iter-1 C3) | `pub struct ErrorResponse { pub error: String, pub status: u16, }` at api-contracts/src/error.rs:4-7 | ✅ |

Plan v13 has zero blocking dependency mismatches. Implementer can execute Tasks 1-11 immediately upon PR #508 merge + PF2 rebase.

---

## Cross-Layer Scope Audit (2026-04-25 ~17:00, post plan v9)

After plan v9 added service-layer migration scope (NEW-C1), an independent grep against ALL adapter layers confirms **plan scope is COMPLETE — no additional unmigrated callers exist**:

```bash
grep -rln "TimeRangeQuery\|count_events_in_range\|count_frames_in_range\|delete_data_in_range\|get_daily_active_secs\|list_frame_file_paths_in_range\|flag_noise_range\|\.from_datetime()\|\.to_datetime()" \
  src-tauri/src/commands/ \
  crates/oneshim-web/src/grpc/ \
  crates/oneshim-network/src/
```

Result: **zero matches**. No Tauri IPC commands, no gRPC handlers, no network-layer code consumes any of the migrated APIs.

Migration scope bounded to:
- ✅ Service layer (`crates/oneshim-web/src/services/` — 9 files in plan v9 Task 6 + 7)
- ✅ REST handlers (`crates/oneshim-web/src/handlers/` — thin pass-through, no changes needed)
- ✅ Storage SQLite impls + internal tests (`crates/oneshim-storage/src/sqlite/` — Task 4)
- ✅ src-tauri scheduler (`src-tauri/src/scheduler/analysis_pipeline/` — Task 4D.1+4D.2)
- ✅ FocusMetrics call sites (10 sites enumerated — Task 8)

---

## External Blocker Detail (PR #508)

```
$ gh pr view 508 --json state,mergeable,mergeStateStatus,statusCheckRollup
{
  "state": "OPEN",
  "mergeable": "MERGEABLE",
  "mergeStateStatus": "BEHIND",
  "ci": [{"name": "Check (fmt + clippy)", "conclusion": "FAILURE"}]
}
```

**Two blockers** for #508 merge:
1. **CI Failure**: Check (fmt + clippy) failed. Lives in worktree `.claude/worktrees/phase9-autostart-foundation` — out of scope for THIS workstream.
2. **BEHIND main**: needs `gh pr merge 508 --auto --update-branch` or manual rebase.

**Phase 3 cannot proceed until #508 reaches MERGED state** per plan ABORT GUARD at PF1.

---

## Phase 3 Resume Procedure

When PR #508 merges:

1. `cd <worktree> && git fetch origin && git rebase origin/main` (PF2)
2. Re-verify baselines (PF3): wire count likely 47, dual i18n `toHaveLength(47)` (verify via grep)
3. `cargo check --workspace && cargo test --workspace` GREEN (PF4)
4. PF5 dep verification (oneshim-core dep on api-contracts, etc.)
5. Execute Tasks 1-11 via `superpowers:subagent-driven-development` against plan v7 (commit `8dcad4c0`)
6. PC1-PC3 post-completion checklist
7. Open PR → release `0.4.42-rc.1`

---

## Files Created (cumulative across 11 iterations)

- `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md` (v1 → v2 → v3)
- `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (v1 → ... → v7)
- `.claude/timewindow-review/phase1-iter1-findings.md` (4C+5I+4N)
- `.claude/timewindow-review/phase2-iter1-findings.md` (9C+11I+6N)
- `.claude/timewindow-review/phase2-iter2-verification.md` (6 NEW C + 5 NEW I)
- `.claude/timewindow-review/phase2-iter3-verification.md` (2 NEW C + 1 NEW I)
- `.claude/timewindow-review/phase2-iter4-verification.md` (4 Important + 2 Suggestion)
- `.claude/timewindow-review/phase2-iter5-verification.md` (2 NEW Important)
- `.claude/timewindow-review/phase2-iter6-verification.md` (1 NEW Critical)
- `.claude/timewindow-review/phase2-iter7-verification.md` (PHASE 2 EXIT APPROVED)
- `.claude/timewindow-review/phase3-readiness-state.md` (this file)

---

**End of Phase 3 readiness state. Ralph loop should pause until PR #508 merges.**
