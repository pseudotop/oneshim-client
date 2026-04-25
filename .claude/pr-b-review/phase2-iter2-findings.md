# Phase 2 — Iteration 2 Plan Review Findings

**Date**: 2026-04-25
**Plan under review**: plan v1 (commit `f187d03b`)
**Reviewer**: 1 superpowers:code-reviewer subagent + direct codebase verification
**Outcome**: 8 Critical + 8 Important + 4 Nice-to-have. Plan v2 corrections applied via inline Edits + Addendum section.

---

## Critical Issues (all fixed in plan v2)

| ID | Issue | Fix |
|----|-------|-----|
| C1 | Wrong i18n file paths — actually `i18n/locales/{en,ko}.json` (subdir for 5 locales) | Inline Edit replaced 6 path references |
| C2 | Wrong Vitest i18n imports — `'../../i18n'` resolves index.ts | Inline Edit replaced 2 imports |
| C3 | Dashboard.tsx doesn't exist — actual structure is `pages/dashboard/DashboardLayout.tsx` + sections | Addendum A5 — render PromptHost in DashboardLayout above Outlet |
| C4 | `oneshim_app::autostart` won't resolve — src-tauri has no lib.rs, only `[[bin]]` | Addendum A3 — move integration test inline to `commands/autostart.rs` `#[cfg(test)]` |
| C5 | Wrong release binary name — `oneshim-app` → actual `oneshim` | Inline Edit replaced binary name in Tasks 7 + 15 |
| C6 | `tauri::test::mock_app` not available + AppHandle generic mismatch | Addendum A4 — refactor helper to take closure, eliminate `tauri::test` dependency |
| C7 | monitor.rs has no focus-block detection + AppHandle not plumbed | Addendum A4 — concrete hook in `handle_idle_tick` flow + plumb AppHandle through `spawn_monitor_loop` signature + `scheduler/mod.rs::run_scheduler_loops` caller |
| C8 | `get_app_config` IPC doesn't exist | Addendum A5 — make `get_autostart_config` (NOT full AppConfig) IPC creation REQUIRED, add to Task 4 alongside other autostart commands |

**Critical addition**: Wire codes architecture was completely wrong in plan v1. Codes are auto-generated from `error_codes::all_codes()` enum aggregator (per ADR-019), NOT direct text append to expected.txt. Addendum A1 + A2 prescribe the correct procedure: create `error_codes/autostart.rs` via `define_code_enum!` macro, register in mod.rs, then update expected.txt to match enum-generated output.

## Important Issues (most fixed)

| ID | Issue | Status |
|----|-------|--------|
| I1 | Static `import { invoke }` mismatch with existing dynamic `await import(...)` pattern | Addendum A6 — use `invokeDesktop` helper or replicate dynamic import inline |
| I2 | Wire-error message format inconsistency (existing uses `{message}`) | Documented; implementer chooses based on existing patterns |
| I3 | i18n keys at wrong nesting (replace whole file vs merge) | Addendum A7 — explicit JSON merge guidance using jq for verification |
| I4 | `AutostartCapabilities` struct order in autostart.rs vs commands/autostart.rs | Acceptable as-is (4.5 → 4.6 ordering correct) |
| I5 | `tauri::Emitter` import — already in plan, OK | No fix needed |
| I6 | `AutostartConfig` Default impl manual vs derive | Cosmetic; clippy may warn but acceptable |
| I7 | §11.4 reconciler explicitly deferred | Acceptable per spec |
| I8 | Pre-flight PF2 cross-consumer merge order is soft check | Acceptable |

## Nice-to-have

- **N1**: Wire-errors alphabetical insertion order — addendum A1 step 3 specifies precise position
- **N2**: Wire snapshot test architecture — addendum A1 explains
- **N3**: Estimate sum 23h vs claimed 22h — addendum A8 reconciles to ~22.5h
- **N4**: PHASE-HISTORY.md format — implementer references latest entry style

---

## Phase 2 iter-3 Plan

**Goals**:
1. Fresh subagent verifies plan v2 (with addendum) is correct + complete
2. Confirm zero new Critical/Important issues
3. Verify the addendum's corrections actually compile/work against actual project state
4. If clean: advance to Phase 3 (subagent-driven-development)
5. If issues remain: iter-4

**Special focus for iter-3**:
- The addendum A1 (wire codes via macro) is a structural change — verify the `define_code_enum!` macro signature matches the example in audio.rs
- The addendum A4 (closure-based testing) is a refactor — verify the proposed signature compiles
- The addendum A5 (DashboardLayout host) — verify `<Outlet>` placement doesn't conflict with existing layout structure
- The addendum approach of "this addendum SUPERSEDES the body" — verify implementer can navigate this correctly (or recommend full task body rewrite for cleanliness)

**Risk**: addendum-style corrections may be confusing for subagent-driven implementation. If iter-3 reviewer flags this, plan v3 will inline-rewrite the affected task bodies instead.
