# P2 Tech-Debt: Nursery Lint Hardening Plan

**Date**: 2026-04-21
**Scope**: Select 3-5 nursery lints to harden workspace-wide via `#![deny(...)]`. Output is a decision + prioritization document — individual lint fixes and `deny` attribute rollout happen in separate PRs.
**Spec ref**: [`docs/reviews/2026-04-16-p2-tech-debt-brief.md`](2026-04-16-p2-tech-debt-brief.md) §Item 1
**Policy ref**: `clippy::nursery` is experimental by design — we do NOT enable the whole group.

## Current audit (2026-04-21, workspace-wide)

| Lint | Count | Risk | Value |
|------|-------|------|-------|
| `clippy::missing_const_for_fn` | **423** | Low (mechanical + const-viral) | Medium (compile-time eval) |
| `clippy::redundant_pub_crate` | **309** | Low (visibility only) | Low (cosmetic) |
| `clippy::significant_drop_tightening` | **277** | Medium (lock-scope semantics) | **High (catches real deadlock/contention risks)** |
| `clippy::use_self` | **188** | Low (stylistic) | Low (stylistic) |
| `clippy::option_if_let_else` | **82** | Low-Med (readability) | Medium (conciseness) |
| `clippy::derive_partial_eq_without_eq` | **34** | Low (mechanical) | Low (derive contract) |
| `clippy::redundant_clone` | ~10 | Low | Low-Med (perf) |
| `clippy::or_fun_call` | ~5 | Low | Low (perf) |
| `clippy::suboptimal_flops` | ~3 | Low | Low (perf) |
| `clippy::too_long_first_doc_paragraph` | ~3 | None (cosmetic) | None |

*Note: earlier memory (`reference_tech_debt_audit.md`, 2026-04-07) recorded "46 nursery warnings". Codebase has grown significantly — counts are much higher now. Brief's "~16" figures per category are out of date.*

## Selection criteria

1. **Value > cost**: each hardened lint must catch a meaningful class of bug or friction.
2. **Mechanical fixability**: violations must have a clear, low-risk fix pattern — not "refactor to be more idiomatic".
3. **Drift protection**: once `#![deny]`, regressions fail CI. We want lints where the benefit of locking the pattern outweighs the false-positive maintenance cost.
4. **No whole-group `-D clippy::nursery`**: nursery is experimental; lints churn between versions. Per-lint `deny` is stable.

## Recommended rollout (3 lints, 3 PRs)

### PR-A: `significant_drop_tightening` — **HIGH priority**

**Hits:** 277
**Why:** Catches lock held longer than necessary → potential contention/deadlock. Real bug class, not style.
**Fix pattern:** `let guard = mutex.lock(); ...long work...` → scope the guard tightly: `let data = { let g = mutex.lock(); g.clone() }; ...long work using data...`.
**Risk:** Medium — requires understanding each callsite's invariants. Some are intentional (guarding an atomic read-modify-write).
**Effort:** ~2 days at 50-80 fixes/day.
**Rollout:** crate-by-crate. Fix + `#![deny(clippy::significant_drop_tightening)]` at crate root when clean. Start with `oneshim-core` (fewest hits), end with `src-tauri` (most).

### PR-B: `derive_partial_eq_without_eq` — **LOW effort, quick win**

**Hits:** 34
**Why:** Trivial but locks a derive-macro contract (`impl PartialEq → also impl Eq` when possible). Prevents regression when types without float fields accidentally get `#[derive(PartialEq)]` only.
**Fix pattern:** `#[derive(PartialEq)]` → `#[derive(PartialEq, Eq)]` IF no float fields. Otherwise `#[allow(clippy::derive_partial_eq_without_eq)]` at the struct with a one-line reason.
**Risk:** Low — mechanical. `Eq`-unsafe types are rare (f32/f64 leaves only).
**Effort:** ~0.5 day.
**Rollout:** single-PR workspace fix + `#![deny]` in each touched crate.

### PR-C: `missing_const_for_fn` — **MEDIUM, largest lift**

**Hits:** 423
**Why:** Enables compile-time constant evaluation. Free perf win on hot paths. Common idiom in modern Rust.
**Fix pattern:** `fn foo() -> u32 { 42 }` → `const fn foo() -> u32 { 42 }`. Viral when the body contains non-const ops (heap alloc, trait methods) — those are `#[allow]` candidates.
**Risk:** Low individually, BUT const-viral: adding `const` on A forces B (caller) to be `const fn`-compatible, which may cascade. Some callsites will expose non-const ops that require refactor.
**Effort:** ~3-5 days at 100-150 fixes/day.
**Rollout:** crate-by-crate, leaves-to-root in the dep graph: `oneshim-core` → `oneshim-*` → `src-tauri`. Fix what fixes without cascade; `#[allow]` the residuals with reason. Hard-`deny` only when a crate is fully clean.

## Explicit non-goals

- **`redundant_pub_crate`** (309): pure cosmetic. No behavior change, no API surface implication. Skipped per "value > cost".
- **`use_self`** (188): pure style. Readability is subjective. Skipped.
- **`option_if_let_else`** (82): readability-only. Deferred — revisit after the 3 high-value lints land.
- **`redundant_clone`** (~10): too few to justify a PR. If we touch them, fix opportunistically.
- **`too_long_first_doc_paragraph`**: cosmetic doc formatting. Skipped.

## Drift protection plan

After each PR merges:
1. Commit `#![deny(clippy::<lint_name>)]` at each hardened crate's `lib.rs` / `main.rs` root.
2. CI `cargo clippy -- -D warnings` already runs on every PR — the `deny` attribute is enforced automatically.
3. New code that introduces a violation fails CI immediately; no drift accumulates.

Existing `#[allow(clippy::<lint_name>)]` site-level allows remain in place as escape hatches with documented rationale.

## Rollback plan

Each PR is independently revertable. If PR-A (significant_drop) causes an unexpected regression (e.g., changes lock scoping in a way that breaks a timing-dependent test), revert only that PR; PR-B and PR-C's work is unaffected.

## Validation commands

Per-lint count (use to track progress):

```bash
cargo clippy --workspace -- -W clippy::significant_drop_tightening 2>&1 | grep -c "warning:"
cargo clippy --workspace -- -W clippy::derive_partial_eq_without_eq 2>&1 | grep -c "warning:"
cargo clippy --workspace -- -W clippy::missing_const_for_fn 2>&1 | grep -c "warning:"
```

Per-crate progress (after switching the lint to `deny` at crate root):

```bash
cargo clippy -p oneshim-core -- -D warnings
cargo clippy -p oneshim-network -- -D warnings
# ...
cargo clippy --workspace -- -D warnings  # full workspace gate
```

## Follow-up triggers

- **IF** a new nursery lint catches a bug in review → consider adding to the hardened set
- **IF** Rust stable bumps and a hardened lint changes semantics → evaluate whether to keep the `deny` or roll back
- **IF** `option_if_let_else` count drops below 20 → harden it (low-hanging fruit)

## Related

- [`docs/reviews/2026-04-16-p2-tech-debt-brief.md`](2026-04-16-p2-tech-debt-brief.md) §Item 1 — brief
- [`docs/reviews/2026-04-16-p2-tech-debt-plan.md`](2026-04-16-p2-tech-debt-plan.md) — parent plan
- [`reference_tech_debt_audit.md`](.claude/projects/.../memory/reference_tech_debt_audit.md) — older audit snapshot (2026-04-07), now superseded
- `reference_clippy_195_patterns.md` (auto-memory) — Rust 1.95 new clippy lints, separate track
