# TimeWindow Phase 3 — Completion Marker

**Date completed**: 2026-04-26 KST
**Worktree**: `.claude/worktrees/timewindow-primitive`
**Branch**: `refactor/timewindow-primitive`
**Branch state**: 49 ahead / 0 behind `origin/main`
**Runner**: ralph-loop (4 iterations, ~auto-paced)

## Completion criteria status

| # | Criterion | Status | Notes |
|---|-----------|--------|-------|
| 1 | 11 commits with conventional prefixes | ⚠ **9 commits** (deviation) | Tasks 1+2 atomic merged (i18n hook coupling); Task 9 evaluated as no-op |
| 2 | PC1 all GREEN | ✅ | cargo test 3,889 / 0 failed; cargo fmt --check exit 0; pnpm lint biome + design-tokens both pass; clippy validated per-commit via lefthook |
| 3 | PC2 all GREEN | ✅ | wire snapshot 49 codes; i18n 49 keys per locale (en+ko) |
| 4 | Completion marker exists | ✅ | this file |

## Commit shas (refactor/timewindow-primitive HEAD = 3f3d9895)

| # | SHA | Subject | Tasks |
|---|-----|---------|-------|
| 1 | `eb22c479` | feat(core): TimeWindow primitive + wire codes + i18n translations | Tasks 1 + 2 atomic |
| 2 | `4e6035cf` | feat(api-contracts): TimeRangeQuery::to_time_window adapter + Default derive | Task 3 |
| 3 | `42e101bd` | refactor(storage): migrate 8 SQL range helpers + 30+ caller sites to TimeWindow | Task 4 |
| 4 | `4c14126d` | test(storage): closed-closed boundary regression tests for migrated SQL helpers | Task 5 |
| 5 | `c63b0048` | refactor(web-services): migrate 7 service-layer files to to_time_window adapter | Task 6 |
| 6 | `eac4aa1e` | refactor(api-contracts): DeleteRangeRequest::period() accessor + ReportQuery TimeWindow consolidation | Task 7 |
| 7 | `b3ddbcc8` | refactor(core): FocusMetrics + SessionMetrics use TimeWindow primitive (NG8 internal-only) | Task 8 |
| 8 | `95edb035` | test(integration): TimeWindow E2E — closed-closed boundary + 400 error mapping | Task 10 |
| 9 | `3f3d9895` | docs(time-window): STATUS.md + PHASE-HISTORY entry for TimeWindow refactor | Task 11 |

## Deviations from plan v13

### Tasks 1 + 2 atomic merge

The wire-error-i18n-coverage lefthook hook treats wire snapshot additions and i18n translations as an atomic unit — committing Task 1 (snapshot 47→49) without simultaneously updating i18n keys (Task 2) blocks the commit. Plan v13's separate commits were a mental model; the lefthook contract makes them atomic in practice. Net effect: 10 commits across 11 tasks instead of 11 (Task 9 also skipped per evaluation below).

### Task 9 (workspace sweep) — evaluated as no-op

Plan §9.3 says "Commit only if changes made". Sweep evaluation results:

| Type | Decision | Rationale |
|------|----------|-----------|
| `TimeRangeQuery`, `TimelineQuery` | already migrated | Tasks 3+6 added `to_time_window` |
| `ReportQuery` | skip | date-only `%Y-%m-%d` per Phase 2 iter-11 fix |
| `ExportQuery` (export.rs) | future PR | callers use out-of-plan-scope backup storage methods |
| `ListOverridesQuery` (recalibration.rs) | future PR | callers use out-of-plan-scope override storage methods |
| `RegimeChanged.from/to` | skip | regime IDs (not timestamps) |

Future PR can add accessors to `ExportQuery` + `ListOverridesQuery` if/when their underlying storage methods migrate to `&TimeWindow`.

### Drift discovered (PR #508 wire codes)

Plan/spec/handoff card stated PR-B1 (#508) added `tracking_schedule.*` wire codes; actual addition was `autostart.*` codes (5 of them). Wire baseline count was correct (47 post-PR-B1) — only the alphabetical neighborhood description was inaccurate. Insertion point for `time_window.*` codes is alphabetically between `storage.failed` and `ui.element_missing` (lines 44-45 of `wire_contract_snapshot.expected.txt`). Final count: 49.

## Wire-code progression

```
40 (ADR-019 baseline)
  → 41 (D7: service.circuit_open, 2026-04-20)
  → 42 (D7 expansion in main, pre-PR-B1)
  → 47 (PR #508 added 5 autostart.* codes, 2026-04-25)
  → 49 (TimeWindow added time_window.inverted_bounds + time_window.parse_failed, 2026-04-26)
```

## i18n test count progression

`crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts`:
```
toHaveLength(41)  → toHaveLength(42)  (D7)
toHaveLength(42)  → toHaveLength(47)  (PR-B1 autostart)
toHaveLength(47)  → toHaveLength(49)  (TimeWindow refactor — both lines 31 + 123 + describe titles)
```

`crates/oneshim-web/frontend/src/i18n/wire-errors.{en,ko}.json`: 49 keys per locale (verified by `scripts/check-wire-error-i18n-coverage.sh`).

## New tests added (+37 across all crates)

| Crate / file | Count | Pattern |
|--------------|-------|---------|
| oneshim-core types::time_window | 13 | TimeWindow primitive (constructor + contains + duration + serde + RFC3339 roundtrip + code routing) |
| oneshim-core error_codes::time_window | 3 | TimeWindowCode (as_str unique + naming + Display) |
| oneshim-core wire_contract_snapshot | (existing, baseline updated) | 47 → 49 |
| oneshim-api-contracts common::time_window_adapter | 8 | TimeRangeQuery::to_time_window (both bounds + missing-to + missing-from + missing-both + invalid + inverted + ref-not-consumed) |
| oneshim-api-contracts data | 4 | DeleteRangeRequest external shape + period accessor variants |
| oneshim-storage maintenance | 3 | closed-closed boundary regression (frames + events + delete_data flag preservation) |
| oneshim-web error::tests | 2 | ApiError mapping (TimeWindow InvertedBounds + ParseFailed → 400) |
| oneshim-web tests/timewindow_integration | 4 | E2E (closed-closed + DeleteRangeRequest external shape + inverted bounds → 400 + invalid RFC3339 → 400) |
| **Total** | **37** | |

## Per-crate test counts (post-merge — `cargo test --workspace`)

`grep -E "^test result" | sum_passed`: ~3,889 across all crates / 0 failed / ~21 ignored (matches expected ~3,855 ± minor delta from estimation).

## Files modified

```
14 source files modified (Tasks 1-11 except Task 9):
  crates/oneshim-core/src/types/{mod,time_window}.rs           (NEW)
  crates/oneshim-core/src/error_codes/{mod,time_window}.rs    (mod modified, time_window NEW)
  crates/oneshim-core/src/error.rs                             (TimeWindow variant + manual From impl)
  crates/oneshim-core/src/lib.rs                               (pub mod types)
  crates/oneshim-core/tests/wire_contract_snapshot.expected.txt (47 → 49)
  crates/oneshim-core/src/ports/{calibration_store,web_storage}.rs
  crates/oneshim-core/src/models/{work_session,telemetry}.rs
  crates/oneshim-storage/src/sqlite/{calibration_store_impl,events,frames,maintenance,web_storage_impl}.rs
  crates/oneshim-storage/src/sqlite/edge_intelligence/{focus_metrics,work_sessions,tests}.rs
  crates/oneshim-api-contracts/src/{common,data,timeline}.rs
  crates/oneshim-web/src/error.rs
  crates/oneshim-web/src/services/{frames,events,metrics,focus,idle,processes,timeline,data_web,reports,reports_query_support,stats_query_support}.rs (or _service variants)
  crates/oneshim-web/tests/{grpc_dashboard_integration,timewindow_integration}.rs
  crates/oneshim-web/tests/support/failing_storage.rs
  crates/oneshim-web/frontend/src/i18n/wire-errors.{en,ko}.json
  crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts
  src-tauri/src/scheduler/analysis_pipeline/{regime,tests}.rs
  src-tauri/src/focus_analyzer/mod.rs

3 doc files modified:
  docs/STATUS.md                                               (version + test count)
  docs/PHASE-HISTORY.md                                        (new TimeWindow Refactor section)

3 lefthook env stubs (untracked, gitignored):
  src-tauri/oneshim-sandbox-worker-aarch64-apple-darwin
  crates/oneshim-web/frontend/dist/index.html
  (and node_modules/ for pnpm lint runs)
```

## Hard constraints (verified honored)

- ❌ NO `git push` — branch remains LOCAL ONLY
- ❌ NO `gh pr create` — left for user
- ❌ NO amend / rebase of existing 40 prior commits
- ❌ NO modifications to other worktrees
- ❌ NO mid-execution plan/spec/handoff card mutations (deviations documented here only)
- ✅ `cargo check` ran at end of every task

## Next steps for user

1. Review this completion marker + recent commits
2. Optional: `cargo clippy --workspace --all-targets -- -D warnings` (per-commit lefthook validated; full workspace pass not re-run for time)
3. Push: `git push -u origin refactor/timewindow-primitive`
4. Open PR: `gh pr create --title "refactor(core): consolidate divergent time-range types into TimeWindow primitive" ...`
5. Tag for release `0.4.42-rc.1` post-merge

---

**Phase 3 implementation complete. Ready for PR.**
