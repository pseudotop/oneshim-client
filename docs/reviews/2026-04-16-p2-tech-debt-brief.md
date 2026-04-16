# P2 Tech-Debt — Spec + Plan Brief

## Context
Working dir: `client-rust/.claude/worktrees/features` (worktree of `pseudotop/oneshim-client`).
Branch: `fix/pr422-followups` (post-#423 merge will trigger cleanup separately — out of scope here).

The memory reference `reference_tech_debt_audit.md` is 8 days old and lists 3 P2 items with counts that are now out of date:
- Memory: "46 nursery lints" — **actual (2026-04-16): hundreds of lint hits across ~20 categories.**
- Memory: "windows-sys 5 versions (transitive)" — **actual: still 5 versions (0.45.0, 0.52.0, 0.59.0, 0.60.2, 0.61.2).**
- Memory: "8 large frontend components >500 LOC" — **actual: 11 frontend files >500 LOC; also 102 Rust non-test files >500 LOC.**

## Scope of this session (Ralph Loop: 2 loops only)

**Loop 1 — SPEC**: Define the P2 work — for each item: goal, boundaries, acceptance criteria, risks, deferred subitems. Deep-review gated on zero minor-or-higher issues.

**Loop 2 — PLAN**: Step-by-step action items with effort estimates per track, ordering, validation commands, rollback notes. Deep-review gated on zero minor-or-higher issues.

**No implementation.** Code changes are deferred — this is planning only so the user can decide whether to pursue.

## Item 1 — Nursery lints cleanup (high effort)

Current nursery run hits hundreds of lints; top 10 categories:
- `significant_drop_tightening` (~16)
- `missing_const_for_fn` (~16)
- `use_self` (~13)
- `option_if_let_else` (~12)
- `redundant_clone` (~10)
- `redundant_pub_crate` (~7)
- `suboptimal_flops` (~7)
- `derive_partial_eq_without_eq` (~6)
- (and ~20 other long-tail categories)

**Goal**: Not to enable `-D clippy::nursery` workspace-wide (nursery is by design experimental and noisy). Instead: pick the 3–5 most impactful & stable categories, harden them individually via `#![deny(clippy::specific_lint)]` at crate level, fix violations, keep the rest as documented noise.

**Non-goals**: Total nursery adoption; cosmetic-only lints (e.g., `redundant_pub_crate`).

## Item 2 — windows-sys version consolidation

Cargo.lock contains 5 versions of `windows-sys`. All are transitive (no direct workspace pin). Primarily driven by:
- Tauri v2 ecosystem (0.59 / 0.60)
- sysinfo / image / reqwest transitive crates (0.45 / 0.52 / 0.61)

**Goal**: Investigate whether any can be eliminated via version unification hints in the workspace, or explicit overrides. Quantify the download/compile-time cost. Decide keep-as-is vs intervene.

**Non-goals**: Forking upstream crates; blocking on upstream updates.

## Item 3 — Large files audit & prioritized split

Current files >500 LOC:
- **Frontend TypeScript (11 files)**: `api/contracts.ts` (1724), `api/client.ts` (1235), `api/standalone.ts` (1219), `hooks/useSettingsForm.ts` (984), `pages/setting-tabs/ai-automation/index.tsx` (758), `pages/Onboarding.tsx` (607), `pages/timeline/AllFrames.tsx` (606), `pages/chat/index.tsx` (589), `stories/mock-data.ts` (566), `components/BugReportWizard.tsx` (541), `pages/setting-tabs/GeneralTab.tsx` (508).
- **Rust non-test (102 files)**: top 10 include `src-tauri/src/updater/mod.rs` (1404), `sqlite/maintenance.rs` (1401), `vision/privacy.rs` (1276), `storage/frame_storage.rs` (1167), `analysis/adaptive_search.rs` (1166), `analysis/coaching_engine/mod.rs` (1151), `network/local_llm_session.rs` (1002), `vision/accessibility/windows.rs` (997), `sqlite/web_storage_impl.rs` (993), `core/models/intent.rs` (974).

**Remember the 500-line split policy** (`feedback_file_split_policy.md`): 500-line split is over-engineering unless there's a SOLID violation. **Triage goal**: identify files where SOLID is violated (single file doing multiple responsibilities), NOT just line count.

**Goal**: Produce a triaged list — for each >500 LOC file, classify as `keep/maybe-split/must-split` with rationale. Output is a decision document, not a split plan.

**Non-goals**: Generating actual refactor plans per file (that's a future engagement).

## Deliverables

- `.claude/plans/p2-tech-debt-spec.md` — finalized spec (Loop 1)
- `.claude/plans/p2-tech-debt-plan.md` — finalized plan (Loop 2)

Both must have zero minor-or-higher review issues on their final revision.

## Deep-review issue severities (for gating)
- **Critical** — factual error, would lead to wrong code/implementation
- **Important** — missing scope, ambiguous boundary, architectural concern
- **Minor** — clarity gap, typo in code example, citation drift
- **Nit** — purely stylistic (allowed to remain)

**Gate**: Any Minor+ must be fixed before advancing. Nits are ignored.

## Completion Promise
Output literal string `RALPH_P2_SPEC_PLAN_DONE` inside a `<promise>` tag when both loops reach zero minor+ issues on their final revisions. Do NOT emit before both are truly done.

## Constraints
- Do NOT commit.
- Do NOT run implementation (no `cargo fix`, no file edits beyond the two plan docs).
- English content primarily; Korean-aware citations where relevant (e.g., feedback memory).
- Be realistic about effort — if a subitem is "needs 2 weeks", say so explicitly.
