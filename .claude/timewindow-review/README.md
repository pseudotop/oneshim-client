# TimeWindow Refactor — Deep Review Findings Index

**Worktree**: `.claude/worktrees/timewindow-primitive` on `refactor/timewindow-primitive`
**Status**: Phase 1 + Phase 2 CLOSED. Phase 3 BLOCKED on PR #508 merge.
**Spec final**: v15 (`docs/superpowers/specs/2026-04-25-timewindow-primitive-design.md`)
**Plan final**: v13 (`docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md`)

---

## Quick-Start for Future Implementer

1. **Read first**: [`phase3-handoff-card.md`](phase3-handoff-card.md) — one-page execution checklist with 10 critical patterns
2. **Read second**: [`phase3-readiness-state.md`](phase3-readiness-state.md) — current state + PF baselines + drift audit + dep verification
3. **For deep context**: `docs/superpowers/specs/...design.md` (v15) + `docs/superpowers/plans/...plan.md` (v13)
4. **For historical findings** (only if reviewing an unclear plan section): see iteration findings below

---

## Phase 1 — Spec Deep Review (CLOSED, 3 iterations)

| Iteration | Doc | Findings | Outcome |
|-----------|-----|----------|---------|
| iter-1 | [`phase1-iter1-findings.md`](phase1-iter1-findings.md) | 4 Critical + 5 Important + 4 Nice-to-have | spec v1 → v2 |
| iter-2 | (no separate doc — verifier confirmed v2 GREEN) | 1 Important + 1 Suggestion | spec v2 → v3 |
| iter-3 | (no separate doc — verifier confirmed v3 GREEN) | 0 NEW | Phase 1 EXIT APPROVED |

## Phase 2 — Plan Deep Review (CLOSED, 13 iterations)

| Iteration | Doc | Findings | Outcome |
|-----------|-----|----------|---------|
| iter-1 | [`phase2-iter1-findings.md`](phase2-iter1-findings.md) | 9 Critical + 11 Important + 6 Nice-to-have | plan v1 → v2 |
| iter-2 | [`phase2-iter2-verification.md`](phase2-iter2-verification.md) | 6 NEW Critical + 5 NEW Important | plan v2 → v3 |
| iter-3 | [`phase2-iter3-verification.md`](phase2-iter3-verification.md) | 2 NEW Critical + 1 NEW Important | plan v3 → v4 |
| iter-4 | [`phase2-iter4-verification.md`](phase2-iter4-verification.md) | 4 Important + 2 Suggestion (pre-existing cleanup) | plan v4 → v5 |
| iter-5 | [`phase2-iter5-verification.md`](phase2-iter5-verification.md) | 2 NEW Important | plan v5 → v6 |
| iter-6 | [`phase2-iter6-verification.md`](phase2-iter6-verification.md) | 1 NEW Critical (Step 4C.1 PRESERVE-BODY) | plan v6 → v7 |
| iter-7 | [`phase2-iter7-verification.md`](phase2-iter7-verification.md) | 0 NEW + same-class audit | Phase 2 EXIT APPROVED (would have been v7) |
| iter-8 | (no doc — Suggestion-level cleanup: work_sessions half-open + containment semantic) | 0 Critical/Important | plan v7 → v8 |
| iter-9 | [`phase2-iter9-verification.md`](phase2-iter9-verification.md) | 1 NEW Critical (NEW-C1: service-layer migration) | plan v8/9 → v10 |
| iter-10 | (no doc — addressed in plan v10 disposition table 5g) | 2 NEW Critical + 2 NEW Important | plan v10 → v11 |
| iter-11 | (no doc — addressed in plan v11 disposition table 5h) | 1 NEW Critical (ReportQuery date-only) | plan v11 → v12 |
| iter-12 | (no doc — addressed in plan v12 disposition table 5i) | 1 NEW Critical (FocusMetrics struct-literal) | plan v12 → v13 |
| iter-13 | (no doc — Pattern A/B definitive verification) | 0 NEW | Phase 2 final EXIT APPROVED |

## Phase 2.5 — Spec↔Plan Alignment Audit (post-Phase 2, 12 iterations)

After Phase 2 EXIT, surfaced spec drift via spec coverage audit. Resulted in 12 spec versions (v4 → v15) eliminating drifts in §1.2, §2.2 NG9-12, §3 U5, §4.1, §4.2, §5.1, §5.2, §5.3, §5.4, §5.5, §5.6, §6.1-6.3, §7.2, §8.4, §9.1, §10.1-10.3, §11 Q-3/Q-8/Q-10, §12 Risk Register, §13, §14, §15.

| Iteration | Spec version | Drift fixed |
|-----------|--------------|-------------|
| - | v4 | §5.1 CoreError struct-variant pattern |
| - | v5 | §5.5 service-layer migration + 24h default |
| - | v6 | §5.6 + §6.1-6.3 DeleteRangeRequest accessor + flow updates |
| - | v7 | §5.3 SAFE-SYNTHETIC vs PRESERVE-BODY + §7.2 dynamic baseline |
| - | v8 | §5.4 Pattern A/B + §8.4 dynamic test count |
| - | v9 | §9.1 commit table fully rewritten |
| - | v10 | §11 Q-3 corrected + §13 PR status + §14-15 stale refs |
| - | v11 | §10.2/10.3 + §12 Risk Register |
| - | v12 | §2.2 NG9-12 + §3 U5 default lookback |
| - | v13 | §1.2 catalog line numbers |
| - | v14 | §4.1 Component Layout major rewrite (10 corrections) |
| - | v15 | §4.2 + §5.2 inline doc-comment cleanup |

---

## Cumulative Issue Counts (final, by phase)

| Phase | Critical | Important | Suggestion |
|-------|---------:|----------:|-----------:|
| Phase 1 (3 iter) | 4 | 5 | 4 |
| Phase 2 iter-1 | 9 | 11 | 6 |
| Phase 2 iter-2 (NEW) | 6 | 5 | 0 |
| Phase 2 iter-3 (NEW) | 2 | 1 | 0 |
| Phase 2 iter-4 | 0 | 4 | 2 |
| Phase 2 iter-5 (NEW) | 0 | 2 | 0 |
| Phase 2 iter-6 (NEW) | 1 | 0 | 0 |
| Phase 2 iter-9 (NEW) | 1 | 0 | 0 |
| Phase 2 iter-10 (NEW) | 2 | 2 | 0 |
| Phase 2 iter-11 (NEW) | 1 | 0 | 0 |
| Phase 2 iter-12 (NEW) | 1 | 0 | 0 |
| Phase 2.5 spec alignment v4-v15 | ~10 | ~15 | 0 |
| **Total** | **~37** | **~45** | **~12** |

(Spec alignment counts are approximate — include drifts in §1.2 catalog, §4.1 Component Layout (10 corrections), §5.5 service-layer + 24h default, §5.6 + §6.1-6.3 DeleteRangeRequest, §7.2 wire baseline, §8.4 dynamic test count, §9.1 commit table rewrite, §11 Q-3/Q-8/Q-10 corrections, §12 Risk Register, §13 PR status, §14-15 stale references, §2.2 NG9-12, §3 U5.)

---

## Phase 3 Status

🔒 **BLOCKED on PR #508 merge** (per plan ABORT GUARD at PF1).

PR #508 (`feature/phase9-autostart-foundation`) is currently OPEN with:
- CI: Check (fmt + clippy) = FAILURE
- mergeStateStatus: BEHIND main

Both must be resolved before TimeWindow Phase 3 can begin.

When PR #508 merges:
1. PF2 rebase onto post-PR-B1 main (trivial — only 2 commits drift in disjoint paths)
2. Re-capture PF3 baselines (wire codes likely 47, dual `toHaveLength(47)`)
3. PF4 GREEN verification + PF5 dep re-verification
4. Execute Tasks 1-11 via `superpowers:subagent-driven-development` against plan v13
5. PC1-PC3 post-completion + open PR → release `0.4.42-rc.1`

---

**End of index. Total work product: ~6700 lines across spec + plan + 11 review docs.**
