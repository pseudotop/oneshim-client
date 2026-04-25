# TimeWindow Refactor — Phase 3 Execution Handoff Card

**Goal**: Replace 9+ divergent absolute-timestamp time-range types with single `TimeWindow { start: DateTime<Utc>, end: DateTime<Utc> }` primitive.

**Plan**: `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (v13, 2885 lines)
**Spec**: `docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md` (v3, 666 lines)
**Estimated**: ~30h across 11 tasks (~4 working days)
**Method**: `superpowers:subagent-driven-development` (fresh subagent per task + 2-stage review)

---

## ABORT GUARD (PF1)

```bash
gh pr view 508 --json state | jq -r .state
```
**Required: `MERGED`**. If `OPEN`/`CLOSED`: HALT.

## Pre-Flight Checks (PF2-PF5)

| # | Check | Status |
|---|-------|--------|
| PF1 | PR #508 = MERGED | ❌ BLOCKED — currently OPEN, BEHIND main, clippy FAILURE |
| PF2 | Rebase onto post-PR-B1 main | ⏳ Pending PF1 |
| PF3 | Wire snapshot baseline + i18n test count | ✅ Pre-captured: 42 codes, dual `toHaveLength(42)` lines 30+122 (will be 47 post-PR-B1) |
| PF4 | Baseline tests GREEN | ✅ Wire snapshot test PASS confirmed |
| PF5 | Required dep verification | ✅ 5/5 PASS (oneshim-core dep, CoreError struct-variant, From<CoreError> for ApiError, define_code_enum! macro, ErrorResponse no `code` field) |

## Task Sequence (sequential — each ends with cargo check + commit)

| # | Task | Estimate | Critical Notes |
|---|------|----------|----------------|
| 1 | TimeWindow Foundation (primitive + CoreError struct-variant + TimeWindowCode + ApiError mapping) | 4.5h | **Single commit** — circular dep avoided |
| 2 | Wire-error i18n translations (en + ko) | 0.5h | Update BOTH `toHaveLength()` assertions (lines 30 + 122) |
| 3 | TimeRangeQuery::to_time_window adapter | 1.5h | Add `Default` derive; adapter is `&self` (non-consuming) |
| 4 | SQL port + impl + 30 caller sites (lockstep) | 5h | 8 port methods. INHERENT pub fn changes too. Half-open `< ?2` preserved in work_sessions per NG6. Containment `start_time >= ?1 AND end_time <= ?2` preserved per NG-CONTAIN. |
| 5 | Storage boundary regression tests (3 helpers) | 1.5h | Use actual `DeletedRangeCounts` field names (`events_deleted` not `events`) |
| 6 | REST handler + service-layer migration (7 services) | 5h | **Migrate at SERVICE layer, NOT handler layer**. Default lookback `Duration::hours(24)` (preserve existing behavior — NOT 7d/30d). Decompose `&window` for non-migrated storage methods |
| 7 | data.rs + reports.rs service-layer migration | 1.5h | `request.period()` accessor (Option C). ReportQuery is **date-only `%Y-%m-%d`** — update `resolve_report_window` in reports_query_support.rs |
| 8 | FocusMetrics + SessionMetrics — 10 sites | 2h | **Pattern A vs B distinction critical**: 7 sites use struct literal with custom seeded fields → use renamed struct literal `FocusMetrics { period: TimeWindow::new(...).unwrap(), other_field: ..., ... }`. NEVER use constructor for sites that seed custom values. |
| 9 | Workspace sweep + final cleanup | 1h | grep for any remaining `from: Option<String>\|to: Option<String>\|period_start.*DateTime` |
| 10 | E2E integration tests | 2h | Assert `body["error"]` substring + `status: 400` (NO `body["code"]` field — ApiError schema is `{ error, status }`) |
| 11 | docs(STATUS + PHASE-HISTORY) | 1h | PHASE-HISTORY: 5 bullets including 1 behavior change (invalid timestamp → 400) + 3 behaviors preserved + helpers retained |

## Critical Patterns (Avoid These Mistakes)

1. **NEVER use constructor for custom-field test fixtures** — `FocusMetrics::new(...)` zeros the other 10 fields. Use renamed struct literal pattern instead.
2. **PRESERVE-BODY for SQL helpers** — don't rewrite synthetic snippets. Only swap parameter sig + add `let (from, to) = window.to_sql_pair();` line. Existing SQL strings + table names + column names + lock-error wrapping stay bit-identical.
3. **Service-layer NOT handler-layer** — handlers thin-delegate. Migration happens in `*_service.rs` files.
4. **Default lookback `Duration::hours(24)`** — preserves existing `from_datetime()` behavior. NOT 7d/30d (would 7×/30× widen payloads).
5. **ReportQuery is date-only `%Y-%m-%d`** — NOT RFC3339. `to_time_window` would fail-parse it. Update `resolve_report_window` instead.
6. **CoreError struct-variant** — `TimeWindow { code, message }`, NOT `#[from] tuple`. Add manual `From<TimeWindowError>` impl.
7. **`mod tests.rs` mock is NoopCalibration{Reader,Writer}** — NOT `MockCalibration`. Two separate impls (Reader async + Writer sync).
8. **DeletedRangeCounts fields end with `_deleted`**: `events_deleted, frames_deleted, metrics_deleted, process_snapshots_deleted, idle_periods_deleted`.
9. **regime.rs:184 is `get_entries`** (re-fetch), NOT `flag_noise_range`.
10. **list_segment_time_ranges return** — `Vec<(String, TimeWindow)>` (preserves segment_id String). Caller destructures `(seg_id, seg_window)` + uses `seg_window.contains(e.timestamp)`.

## Test Count Targets

After successful implementation, ~37 NEW tests:
- 13 TimeWindow unit + 3 TimeWindowCode + 8 TimeRangeQuery adapter + 3 SQL boundary + 4 E2E + 2 ApiError mapping + 4 api-contracts roundtrip

## Commit Message Conventions (per repo style)

- `feat(core)` — new TimeWindow primitive (Task 1)
- `test(i18n)` — wire-error translations (Task 2)
- `feat(api-contracts)` — adapter (Task 3)
- `refactor(storage)` — SQL port + impl (Task 4)
- `test(storage)` — boundary regressions (Task 5)
- `refactor(web-services)` — service-layer migration (Task 6)
- `refactor(api-contracts)` — DeleteRangeRequest + ReportQuery (Task 7)
- `refactor(core)` — FocusMetrics + SessionMetrics (Task 8)
- `refactor(workspace)` — sweep (Task 9)
- `test(integration)` — E2E (Task 10)
- `docs(time-window)` — STATUS + PHASE-HISTORY (Task 11)

## Post-Completion Checklist (PC1-PC3)

- PC1: `cargo test --workspace` GREEN + `cargo clippy --workspace --all-targets -- -D warnings` GREEN (single run at end, NOT per-task) + `cargo fmt --check` + `pnpm lint` (frontend)
- PC2: `cargo test -p oneshim-core --test wire_contract_snapshot` + `bash scripts/check-wire-error-i18n-coverage.sh`
- PC3: `git push -u origin refactor/timewindow-primitive` + `gh pr create` (target release: `0.4.42-rc.1`)

## Subagent-Driven Development Protocol

Per `superpowers:subagent-driven-development`:
1. Dispatch fresh subagent per task with task-specific prompt
2. Subagent executes task + reports back with diff + cargo check/test results
3. Stage 1 review (spec compliance) — fresh code-reviewer subagent
4. Stage 2 review (code quality) — fresh code-reviewer subagent
5. If both reviews PASS → next task; otherwise iterate fix → re-review

---

**End of handoff card. Plan v13 is genuinely implementation-ready. Phase 3 unblocks when PR #508 merges.**
