# PR-B Ralph-Loop State Tracking

**Created**: 2026-04-25
**Spec**: `docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md` (v3, commit `48ffbfb5`)
**Plan**: `docs/superpowers/plans/2026-04-25-phase9-pr-b1-autostart-foundation.md` (v2.6 = v2.5 + iter-4 final fixes, pending commit)
**Worktree**: `.claude/worktrees/phase9-autostart-foundation`
**Branch**: `feature/phase9-autostart-foundation`

## Phase Tracker

| Phase | Status | Iterations |
|-------|--------|------------|
| 1 — Spec Deep Review | ✅ COMPLETE | 3 |
| 2 — Plan Creation + Review | ✅ **COMPLETE** | 4 |
| 3 — Implementation + Review | **STARTING** (next iter — Task 1) | 0 |

## Phase 2 — Plan Deep Review (CLOSED)

### Summary

| Iter | Issues Found | Fix Outcome |
|------|--------------|-------------|
| iter-1 | Plan v1 created (15 tasks, 1997 lines) | committed `f187d03b` |
| iter-2 | 8 Critical + 8 Important — paths, APIs, file existence, runtime mismatch | inline edits + Addendum (commit `05cb8051`) |
| iter-3 | 2 NEW Critical (subagent-driven readiness) — body steps still execute wrong procedure | 7 SUPERSEDED banners + PF4 + step 4.6 enum rewrite (commit `9517f731`) |
| iter-4 | 1 Important (Step 12.7 git add stale path) + 1 Suggestion (header estimate) | inline fixes (this commit) |

### Phase 2 Exit Criteria — ALL MET

- ✅ All Critical fixed (10 total: 8 iter-2 + 2 iter-3)
- ✅ All Important fixed (12 total: 8 iter-2 + 3 iter-3 + 1 iter-4)
- ✅ Plan v2.6 committed
- ✅ SUPERSEDED banners in place — subagent cannot miss addendum
- ✅ PF4 ensures addendum is read first
- ✅ All stale references cleaned (Dashboard.tsx → DashboardLayout.tsx in git add path)

## Phase 3 — Implementation (STARTING)

**Trigger**: Phase 2 exit (this iteration).

**Skill**: `superpowers:subagent-driven-development` — fresh subagent per task + 2-stage review.

**Plan**: 15 tasks per `docs/superpowers/plans/2026-04-25-phase9-pr-b1-autostart-foundation.md`.

**Plan total estimate**: ~22.5h.

**Per-iter strategy** (in ralph-loop):
- Each iter: implement 1 task (or 1 sub-step group) via fresh subagent
- After implementation: 2-stage review (impl-self-review + spec-compliance review)
- Commit per task
- Move to next task in subsequent iter

**Phase 3 expected iter count**: 15 tasks × 1-3 iters each ≈ 20-40 iterations total.

**Quality gates per task**:
- TDD-style: test first, fail, impl, pass, commit
- `cargo check/test/clippy/fmt --workspace` GREEN before commit
- Frontend `pnpm lint`/typecheck GREEN before commit
- Subagent self-review for ADR alignment
- Subagent code-review for issues

## Files Created (cumulative)

- `docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md`
- `docs/superpowers/plans/2026-04-25-phase9-pr-b1-autostart-foundation.md` (v1 → v2 → v2.5 → v2.6)
- `.claude/pr-b-review/phase1-iter1-findings.md`
- `.claude/pr-b-review/phase1-iter2-findings.md`
- `.claude/pr-b-review/phase2-iter2-findings.md`
- `.claude/pr-b-review/phase2-iter3-findings.md`
- `.claude/pr-b-loop-state.md`

## Commits

- `fd8f64cf` docs(spec): v1
- `5f1add95` docs(spec): v2
- `1777a387` docs(spec): v3
- `48ffbfb5` docs(spec): Phase 1 closure
- `f187d03b` docs(plan): Phase 2 iter-1 plan v1
- `05cb8051` docs(plan): Phase 2 iter-2 v2 corrections
- `9517f731` docs(plan): Phase 2 iter-3 supersession banners + PF4
- (this iter) docs(plan): Phase 2 iter-4 final fixes (Step 12.7 path + header estimate) → Phase 2 CLOSED, Phase 3 STARTING

## Phase 3 Task Tracker (15 tasks)

| Task | Status | Notes |
|------|--------|-------|
| 1: tauri-plugin-single-instance dep | pending | 0.5h, ~30 min subagent |
| 2: AutostartConfig + AutostartPromptState in core | pending | 1.5h |
| 3: AutostartConfig unit tests | pending | 1.5h |
| 4: IPC commands + wire codes (per A1+A2+A5) | pending | 3h, large task — may split |
| 5: IPC command tests (per A3 inline) | pending | 1h |
| 6: Single-instance plugin + D-Bus check | pending | 2h |
| 7: Single-instance integration smoke test | pending | 1.5h |
| 8: GeneralTab Startup section + i18n | pending | 2.5h |
| 9: GeneralTab Vitest coverage | pending | 1h |
| 10: Productive-session detection in monitor.rs (per A4) | pending | 3h, large — may split |
| 11: Autostart helper unit tests (per A4 closure pattern) | pending | 1h |
| 12: AutostartOnboardingPrompt + Host (per A5+A6) | pending | 3h, large — may split |
| 13: AutostartOnboardingPrompt Vitest | pending | 1.5h |
| 14: STATUS.md + PHASE-HISTORY entry | pending | 0.5h |
| 15: Manual smoke test matrix (PR description) | pending | 1h |
