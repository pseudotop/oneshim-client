# P2 PR-C: `missing_const_for_fn` — Decision

**Date**: 2026-04-21
**Parent**: [`2026-04-21-p2-nursery-lint-plan.md`](2026-04-21-p2-nursery-lint-plan.md) (PR #466)
**Revision to parent plan**: original estimate 3-5 days for 423 sites; actual audit shows 353 sites, and the decision shifts from "mechanical fix + `#![deny]`" to **"accept crate-wide via `#![allow]`"** for the same reason PR #470 accepted `significant_drop_tightening`: nursery lints on large codebases have a false-positive / noise rate that outweighs the diagnostic value.

## Audit (2026-04-21, workspace-wide)

| Crate | Sites |
|-------|------:|
| oneshim-core | 156 |
| oneshim-web | 70 |
| oneshim-analysis | 42 |
| oneshim-vision | 18 |
| oneshim-network | 17 |
| oneshim-storage | 12 |
| oneshim-api-contracts | 12 |
| oneshim-monitor | 9 |
| oneshim-automation | 9 |
| oneshim-suggestion | 6 |
| oneshim-audio | 2 |
| **Total** | **353** |

Revised from parent plan's "~423" estimate.

## Why "accept" rather than "fix"

1. **Not a correctness lint**. `missing_const_for_fn` is a stylistic/perf hint — a `const fn` enables compile-time evaluation at call sites. Missing `const` is never a bug.

2. **Const-viral cascade**. Adding `const` to `A` forces `B` (if `B` wants to be `const fn` and calls `A`) to also be `const fn`, cascading across the workspace. Some callers inevitably invoke non-const ops (heap alloc, trait methods, non-const FFI) and must either stay non-const or refactor extensively. **Net result: many fixes trigger follow-up fixes without clear stopping point**.

3. **Clippy false positives observed in prior P2 PRs**. Both `derive_partial_eq_without_eq` (PR #467) and `significant_drop_tightening` (PR #468-470) had suggestions that produced invalid Rust or broke observable behavior. `missing_const_for_fn` shares the nursery-lint authoring philosophy — experimental, noisy, occasionally wrong.

4. **The real value — compile-time eval — is realized case-by-case**. When authoring a new `fn`, the author knows whether compile-time eval is wanted. Workspace-wide `const`-ification adds no value over that per-site judgment.

5. **Matches established workspace policy**. PR #470 set precedent: for large-crate noisy nursery lints, crate-level `#![allow]` with rationale is the honest answer.

## Policy

**All 11 crates with flagged sites get crate-level `#![allow(clippy::missing_const_for_fn)]`** with a comment pointing at this decision doc.

**No `#![deny]`**. Drift protection is deferred — new non-const `fn`s will not fail CI. Authors of new code should still use `const` when appropriate; the lint would remind them, but so does a moment of thought.

## Revision to parent plan

The parent plan [PR #466](2026-04-21-p2-nursery-lint-plan.md) estimated PR-C at 3-5 days effort. **Actual effort with the accept-crate-wide approach: ~30 minutes** (11 files touched, all `#![allow]` additions).

Parent plan's PR-C row in the recommendation table should be updated from:

> PR-C | `missing_const_for_fn` | 423 | ~3-5 days | MEDIUM | Free compile-time perf. Const-viral — some cascading refactor expected.

To:

> ~~PR-C~~ | `missing_const_for_fn` | 353 | ~30 min | — | **Accepted crate-wide per [PR-C decision doc](2026-04-21-p2-missing-const-for-fn-decision.md)** — nursery lint's noise rate outweighs value; case-by-case `const` adoption at authoring time remains the right pattern.

## Implementation

See companion PR adding `#![allow(clippy::missing_const_for_fn)]` to 11 crate roots with pointer to this doc.

## Future

If a specific workflow benefits from workspace-wide `const fn` (e.g., heavy use of `const`-eval in a new subsystem), revisit this decision. For now, the lint stays off.

## Related

- [PR #466](https://github.com/pseudotop/oneshim-client/pull/466): parent P2 nursery-lint prioritization plan
- [PR #467](https://github.com/pseudotop/oneshim-client/pull/467): PR-B `derive_partial_eq_without_eq` (mechanical fix path)
- [PR #468](https://github.com/pseudotop/oneshim-client/pull/468): PR-A oneshim-web (per-site fix + #![deny])
- [PR #470](https://github.com/pseudotop/oneshim-client/pull/470): PR-A large-crate accept precedent
