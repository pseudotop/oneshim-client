# P2 Remaining Nursery Lints — Decision

**Date**: 2026-04-21
**Parent**: [`2026-04-21-p2-nursery-lint-plan.md`](2026-04-21-p2-nursery-lint-plan.md) (PR #466)

Addresses the 3 nursery lints the parent plan explicitly **skipped** as "value < cost":

- `clippy::use_self` (188 sites, stylistic)
- `clippy::option_if_let_else` (82 sites, readability-only)
- `clippy::redundant_pub_crate` (302 sites, cosmetic)

**Decision: accept all 3 crate-wide via `#![allow]`.** Matches the precedent from PR #470 (`significant_drop_tightening` for large crates) and PR #473 (`missing_const_for_fn`).

## Audit (2026-04-21)

Total across 11 crates: **572 sites** (188 + 82 + 302).

## Why accept (same reasoning as PR #473)

1. **None of these are correctness lints**. All three are stylistic/cosmetic suggestions.
2. **Nursery false-positive rate observed in prior P2 PRs**. The lint authors themselves flag the nursery group as experimental.
3. **Workspace-wide application adds no value**. These patterns are fine case-by-case; authors of new code can decide per-site.
4. **Each lint would add noise without catching bugs**:
   - `use_self`: Prefers `Self` over full type name. Pure style — readers can grep for either.
   - `option_if_let_else`: Prefers `option.map_or_else(...)` over `if let Some(x) = option { ... } else { ... }`. Terse vs. explicit — no functional difference.
   - `redundant_pub_crate`: Flags `pub(crate) fn f()` inside private modules. Visibility is already effectively `pub(crate)` via module scope; removing the keyword is noise.

## Policy

All 11 crates get a single `#![allow(...)]` block at crate root, grouping the 3 lints with a pointer to this doc.

No `#![deny]`. Drift protection is not applicable for cosmetic lints.

## Implementation

Applied via crate-level attributes in each `src/lib.rs`. See companion PR.

## Parent plan update

Parent plan [PR #466](2026-04-21-p2-nursery-lint-plan.md) categorized these as "Explicitly skipped (value < cost)". That decision is now formalized as crate-level `#![allow]`s so new contributors don't need to re-discover the reasoning — the attribute is the code-level record.

**P2 nursery-lint track complete after this PR.**

## Related

- [PR #466](https://github.com/pseudotop/oneshim-client/pull/466): parent P2 plan
- [PR #470](https://github.com/pseudotop/oneshim-client/pull/470): precedent — accept pattern for large-crate noisy lints
- [PR #473](https://github.com/pseudotop/oneshim-client/pull/473): precedent — `missing_const_for_fn` accept-crate-wide
