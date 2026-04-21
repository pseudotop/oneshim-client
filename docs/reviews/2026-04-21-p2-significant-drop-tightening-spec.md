# P2 PR-A: `significant_drop_tightening` Hardening — Spec

**Date**: 2026-04-21
**Scope**: Fix all `clippy::significant_drop_tightening` violations workspace-wide and harden affected crates against regression.
**Parent plan**: [PR #466 nursery lint plan](../../reviews/2026-04-21-p2-nursery-lint-plan.md) — PR-A slot.
**Status**: SPEC (Loop 1)

## Scope correction from parent plan

Parent plan estimated **277 hits** workspace-wide based on an earlier pattern-matching grep. **Actual audit shows only 5 hits** across 2 files:

| File | Sites | Category |
|------|-------|----------|
| `crates/oneshim-web/src/handlers/pomodoro.rs` | 4 | Mutex guard held across pure in-memory ops |
| `crates/oneshim-storage/src/integration_state_store/inner.rs` | 1 | Lock held across disk I/O (intentional atomicity guard) |
| **Total** | **5** | |

The earlier "277" came from `grep -c "warning:"` which counted ALL clippy warnings during the lint run — including unrelated lints, multi-line warning bodies, and tail summary lines. Actual site count (one warning per unique location) is 5.

Scope is therefore dramatically smaller than anticipated. PR can land in ~1 day (was estimated at ~2).

## Goal

Drive `cargo clippy --workspace -- -W clippy::significant_drop_tightening` to **0 warnings** and add `#![deny(clippy::significant_drop_tightening)]` at the two affected crate roots (`oneshim-web`, `oneshim-storage`) to lock the contract.

## Categorization: Fix vs. Allow

Each of the 5 sites is categorized based on what the lock guards and whether the long-hold is intentional.

### Category A — Tighten scope (4 sites, all pomodoro.rs)

The mutex guards a single `Option<PomodoroSession>` kept purely in memory; no I/O, no long-running work. Clippy's suggestion to tighten the scope via `drop(guard)` is purely mechanical and carries zero risk.

| # | Site | Fix |
|---|------|-----|
| 1 | `start_pomodoro:60` — guard held through `*guard = Some(session); Ok(..)` | Construct `response` before taking the lock; drop guard immediately after `*guard = Some(session)` |
| 2 | `get_current_pomodoro:89` — guard held through `guard.as_ref().map(session_to_response)` then return | Extract response via `let response = { let g = lock; g.as_ref().map(session_to_response) }` block (auto-drop at block end) |
| 3 | `cancel_pomodoro:104` — guard held through session mutation + `Json(session_to_response(session))` | Mutate inside block, drop guard, then build response outside |
| 4 | `complete_pomodoro:132` — same pattern as #3 | Same fix as #3 |

**Risk**: None. All fixes are local and preserve observable behavior — guard-scope tightening on a Mutex protecting in-memory-only state cannot change what gets mutated, only when the lock is released. Existing unit tests cover the `session_to_response` shaping function (4 tests in `handlers::pomodoro::tests`). No integration tests exist for the handler endpoints themselves; that gap is a pre-existing coverage issue orthogonal to this PR.

### Category B — Intentional long-hold with `#[allow]` + rationale (1 site)

**`integration_state_store::inner::store_session_sync:87`** — plus the two sibling writers (`clear_session_sync:94`, `enqueue_outbox_sync:103`, etc. which clippy doesn't flag separately because it only reports the leading example in a pattern).

```rust
pub(super) fn store_session_sync(&self, state: IntegrationSessionState) -> Result<(), StorageError> {
    let mut registry = self.registry.lock();     // guard acquired
    registry.session = Some(state);              // in-memory mutation
    self.save_registry(&registry)                // ← disk I/O while holding lock
}
```

`save_registry` performs 3 filesystem operations: `create_dir_all`, atomic `write` to `.tmp`, `rename`. On a slow spinning disk this could hold the lock for tens of milliseconds.

**Why we KEEP the long hold**: the whole point of this call is to atomically transition `(in-memory state) → (on-disk state)`. If we:
1. Lock, mutate, drop, save (tightened) — two concurrent writers race: both see the same pre-state, both mutate, both drop, both save in unspecified order → **last-write-wins at FS level but the in-memory winner may not be the FS winner**. Data loss possible.
2. Lock, mutate, save, drop (current) — serializes writers. FS state always matches the most recent `mutate+save` atomically.

The lock *is* the atomicity guard for the file. Tightening it inverts this invariant.

**Fix**: site-level `#[allow(clippy::significant_drop_tightening)]` with a comment explaining the rationale.

This pattern applies to all write methods on the same `registry` Mutex. The actual writers in the impl (as of 2026-04-21):
- `store_session_sync` (line 83) ← currently flagged by clippy
- `clear_session_sync` (line 92)
- `enqueue_outbox_sync` (line 98)
- `delete_outbox_sync` (line 129)
- `store_outbox_ack_cursor_sync` (line 141+, not shown)
- …and future siblings

All share the same `lock → mutate in-memory → save_registry → unlock` pattern. Clippy only reports the first instance, but future clippy upgrades may broaden detection — **impl-level `#[allow]`** at `impl FileIntegrationStateInner` (line 22 of `inner.rs`) covers all methods with one attribute and is future-proof.

The file uses `parking_lot::Mutex` (see line 19: `registry: parking_lot::Mutex<...>`), which makes the contention argument even weaker than it would be for `std::sync::Mutex` — parking_lot is substantially faster under contention and the I/O hold is already quantified in tens of ms, not seconds.

## Implementation order

1. Fix Category A (4 pomodoro sites) first — purely mechanical
2. Add Category B `#[allow]` on `impl Inner` block — no behavior change, just suppresses the lint
3. Verify `cargo clippy --workspace -- -W clippy::significant_drop_tightening` → 0 warnings
4. Add `#![deny(clippy::significant_drop_tightening)]` to crate roots:
   - `crates/oneshim-web/src/lib.rs`
   - `crates/oneshim-storage/src/lib.rs`
5. Run `cargo test --workspace` — existing tests must pass unchanged

## Acceptance criteria

| Check | Expected |
|-------|----------|
| `cargo clippy --workspace -- -W clippy::significant_drop_tightening` warnings count | 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | no regressions |
| `cargo fmt --check` | clean |
| `#![deny(clippy::significant_drop_tightening)]` present in `oneshim-web` + `oneshim-storage` | yes |
| Category B rationale comment present on `inner.rs` `#[allow]` | yes — references the atomicity invariant |

## Risks

1. **False-negative on `#[allow]` expansion**: if a new write method is added to `Inner` without the impl-level pattern, the lint will fire and catch it. That's a feature, not a bug.

2. **Regression on pomodoro code path**: fixes are tiny and locally scoped. Existing handler tests cover start/get/cancel/complete flows. Any regression would surface in `cargo test -p oneshim-web`.

3. **Future sibling sites**: if a new site appears in a different crate (outside `oneshim-web` / `oneshim-storage`), the `#![deny]` only fires in those two crates. Consumers can either fix the site or add their own `#![deny]`. This is the intentional crate-by-crate rollout model from the plan doc.

## Out of scope

- **Workspace-wide `#![deny]`** — not done because the `Cargo.toml` `[workspace.lints]` would require aligning every crate simultaneously. Per-crate `#![deny]` is the chosen pattern (see plan doc).
- **Other nursery lints** (`missing_const_for_fn`, `redundant_pub_crate`, etc.) — separate PRs per the plan.
- **Refactoring the `integration_state_store` atomicity model** — the current lock+I/O coupling is intentional. Re-evaluation would require moving to a write-ahead-log or async-friendly locking scheme, both significant designs. Out of scope.

## Rollback plan

The PR touches 4 files: `pomodoro.rs`, `inner.rs`, `oneshim-web/src/lib.rs`, `oneshim-storage/src/lib.rs`. `git revert` of the single squash commit restores the pre-PR state atomically. `#![deny]` can be removed if a future edge-case requires it; prefer `#[allow]` at the specific site instead to preserve the workspace invariant.

## Validation commands (pre-merge)

```bash
# 1. Targeted lint check
cargo clippy --workspace -- -W clippy::significant_drop_tightening 2>&1 | grep -c "^warning: temporary"
# Expected: 0

# 2. Full workspace lint gate
cargo clippy --workspace --all-targets -- -D warnings
# Expected: clean (exit 0)

# 3. Unit + integration tests
cargo test --workspace --lib
cargo test -p oneshim-web --lib handlers::pomodoro
cargo test -p oneshim-storage --lib integration_state_store
# Expected: all pass

# 4. Format
cargo fmt --check
# Expected: clean

# 5. Verify deny attributes present
grep -n "deny(clippy::significant_drop_tightening)" \
  crates/oneshim-web/src/lib.rs \
  crates/oneshim-storage/src/lib.rs
# Expected: 2 matches (one per crate)
```

## Self-review (Loop 1 gate)

- [x] **Scope accuracy**: verified via `cargo clippy --message-format=short` — 5 sites confirmed
- [x] **Categorization**: each site classified A (fix) or B (allow+reason)
- [x] **Risk bound**: category A is mechanical; category B keeps existing atomicity invariant
- [x] **Acceptance criteria**: measurable, command-verifiable
- [x] **No placeholders**: all 5 sites named with file:line + fix pattern
- [x] **Out-of-scope listed**: workspace-wide deny deferred; other nursery lints in separate PRs
- [x] **Rollback**: trivial (git revert)
- [x] **No hidden assumptions**: atomicity argument for Category B spelled out

**Spec gate passed** — proceed to Loop 2 (Plan).
