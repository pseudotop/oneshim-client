# Phase 9 Plan Review 1 — Architecture + Task Ordering

**Reviewer**: 1 of 3
**Lens**: Architecture, Task Ordering, Dependency Integrity, ADR Compliance
**Date**: 2026-04-24
**Plan reviewed**: `docs/reviews/2026-04-23-phase9-quick-wins-plan.md` @ 1353 lines
**Spec**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` @ 1599 lines (22 Decisions, Loop 1e PASS)
**Worktree tip**: `5618558c` (origin/main post-PR-#486)

---

## Summary

- **Critical**: 2
- **Important**: 7
- **Minor**: 8

**Verdict**: **FAIL** — 2 Critical findings block Loop-2 gate (zero-Critical-zero-Important required).
Both Criticals are file-path drift rather than design flaws; targeted edits are
sufficient to clear. Loop 2b fix should be small (est. <1h) before advancing to
Loop 3 impl.

---

## Critical findings

### C1. Composition-root file paths are wrong — `SmartCaptureTrigger::with_schedule` + `BatchUploader::new` live in `agent_runtime_support.rs`, not `main.rs` / `app_runtime_launch.rs`

**Impact**: Two high-risk refactors (A.7 hoist + A.12 DI wiring) cite the wrong
files. The implementer following the plan verbatim would grep the wrong files
and waste time. Worse, the plan says "Find `BatchUploader::new(...)` or
`BatchUploader::with_health_flag(...)` call" in `main.rs` — there is no such
call there.

**Evidence** (all at tip `5618558c`):
- `SmartCaptureTrigger::with_schedule(...)`: **one** live call site at
  `src-tauri/src/agent_runtime_support.rs:251` (`Arc::new(SmartCaptureTrigger::with_schedule(...))`).
  Plan A.7 says the call is in "`src-tauri/src/main.rs` (composition root) or
  `src-tauri/src/app_runtime_launch.rs`". Grep on both yields zero hits.
- `BatchUploader::new(...)`: **one** live production call site at
  `src-tauri/src/agent_runtime_support.rs:405` (`Arc::new(BatchUploader::new(...))`).
  All other matches are in `crates/oneshim-network/src/batch_uploader.rs` test
  modules. Plan A.12 again says "`src-tauri/src/main.rs` or `src-tauri/src/app_runtime_launch.rs`".

**Fix**: change both A.7 and A.12 file citations to `src-tauri/src/agent_runtime_support.rs`
and re-verify the hoist + DI chain around lines 249-254 and 405.

**Ordering note**: A.7 (hoist) and A.12 (DI) touch the **same file** — the plan
should bundle them (or at minimum cross-reference them) so the implementer
edits `agent_runtime_support.rs` once per cold-clippy cycle, not twice.

---

### C2. `start_audio_capture` signature change not spelled out — gating requires new state param

**Impact**: A.9 row 13 proposes adding a `tracking_schedule_active` gate to
`commands::audio::start_audio_capture`. The proposed code:

```rust
if crate::scheduler::tracking_schedule_active(&config_state.get()) {
    return Err(IpcError::new("validation.invalid_arguments", "..."));
}
```

...references a `config_state` parameter the fn does **not** currently accept.
Actual signature at `src-tauri/src/commands/audio.rs:19-21`:

```rust
pub async fn start_audio_capture(
    state: tauri::State<'_, AudioRuntimeState>,
) -> Result<(), IpcError>
```

The gate requires injecting `State<'_, ConfigRuntimeState>` or routing through
`AudioRuntimeState` (which would need to carry a `ConfigManager` reference it
doesn't carry today). The plan is silent on which path.

**Fix**: plan must either (a) specify that `start_audio_capture` signature gains
a second `State<'_, ConfigRuntimeState>` param, (b) specify that
`AudioRuntimeState` gains a `ConfigManager` field in a pre-commit, or (c)
route the check through a helper that reads `ConfigManager` via app-handle
lookup. Each path has trade-offs; the decision is architecturally non-trivial.

**Precedent check**: other command modules mix state params — e.g. `commands::settings::update_setting`
takes multiple states. Path (a) is the least invasive, but it's still a
signature change and A.9 doesn't list it.

---

## Important findings

### I1. `ALLOWED_KEYS` edit breaks a snapshot test — plan doesn't mention the test update

**Plan A.14** adds `"tracking_schedule"` to `src-tauri/src/commands/settings.rs:44` (`ALLOWED_KEYS`).
The file has a companion **locked snapshot test** at `settings.rs:343-361`
(`allowed_keys_matches_expected_set`) that asserts exact array equality against
a hard-coded `expected` vec. Adding the new key without also updating the test
would make A.14 fail CI unambiguously, but the plan would be silent about it —
reviewer might miss it on re-verify.

**Fix**: A.14 must list both edits explicitly. Also verify that the sibling test
`allowed_keys_excludes_sensitive_sections` at `:363` does NOT list
`tracking_schedule` as forbidden (it shouldn't; tracking-schedule has no
credentials).

**Line-citation note**: plan says line 44 (which is "notification"). The correct
insertion point is between line 54 (`"coaching"`) and line 55 (closing `]`).
Minor drift — line numbers are off by 10.

---

### I2. `tray.rs` has no existing async task — plan wording implies one

**Plan A.17** says: "in the tray task spawn, add ... `tokio::spawn(async move { while rx.changed().await.is_ok() { ... } })`"
and later "spawn the subscriber as part of existing tray task lifecycle; no new
spawn at binary entry".

Verified: `tray.rs` contains **zero** `tokio::spawn` calls. `setup_tray<R: Runtime>`
at `:175` is a synchronous setup function; menu updates go through
`sync_tray_state` which is a sync callback from `on_menu_event`. There is no
existing async task to extend.

**Impact**: The subscriber task must be spawned fresh, either from `setup_tray`
(which would need to become async-aware) or from main.rs after tray setup.
This is architecturally a new worker task — the plan's wording is misleading.

**Fix**: clarify that A.17 creates a new async worker, spawned from the
composition root after `setup_tray` returns, keyed on an `AppHandle` clone for
emit/icon calls. Document the task lifetime (does it tear down on app shutdown?
who owns the JoinHandle?).

---

### I3. `NotificationConfig` does not derive `PartialEq` — plan under-states the impact

**Plan A.17** notes "`PartialEq` derive required on `TrackingScheduleConfig`,
`NotificationConfig` — add if missing". Verified: `NotificationConfig` at
`crates/oneshim-core/src/config/sections/storage.rs:109` derives only
`Debug, Clone, Serialize, Deserialize`. Adding `PartialEq` cascades:

- `NotificationConfig` is embedded in `AppConfig`. If the diff closure eventually
  needs to compare entire `AppConfig` trees, every sub-struct in the tree must
  derive `PartialEq`. Plan scopes the diff to `cfg.tracking_schedule != last.0 || cfg.notification != last.1`, so only 2 structs need `PartialEq` — but this coupling isn't called out.
- Some embedded types (e.g. `Duration` fields, `PathBuf` fields) already support
  `PartialEq`, but custom derives inside `NotificationConfig` would need to be
  checked.

**Fix**: A.17 should list the exact set of derives to add (likely `PartialEq, Eq`)
and explicitly scope the diff: "compare on `tracking_schedule` sub-tree OR
`notification.tracking_schedule_enabled` field" (per spec §3.11a), NOT the
whole `NotificationConfig` (which would unnecessarily fire on unrelated
notification toggles). Plan does mention the filter rule but doesn't align the
code snippet with it — the snippet compares the full `cfg.notification`.

---

### I4. `autostart.rs` sync→async transition is a signature change — plan doesn't call it out

**Plan B.3** wraps `Command::new("launchctl/systemctl").output()` in
`tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(...))`.

Current state: `autostart.rs` is **100% synchronous** (no `async fn`, no tokio
calls). The three public entry points — `enable_autostart` (`:8`),
`disable_autostart` (`:31`), `is_autostart_enabled` (`:54`) — return `Result<_, String>`
not `impl Future`. Adding `tokio::time::timeout` requires:

- Converting public fns to `async fn`, OR
- Wrapping the `timeout` + `spawn_blocking` in a synchronous `tokio::runtime::Handle::current().block_on(...)` call — which only works inside a tokio runtime and is a code smell.

Plan B.5 subsequently calls `autostart::is_autostart_enabled()` from an
`async fn` IPC command, implying the public fn is now `async` — so path (a) is
what the plan assumes. But path (a) is a breaking signature change that isn't
explicitly listed in B.3's "Touches" section.

**Consumer audit**: the only current consumers of the three public fns are tests
inside `autostart.rs` itself (`enable_disable_roundtrip_unsupported_platform` at
`:544-547`). All tests use `let _ = enable_autostart();` — adding `.await`
works but is still a compile-time change. Tests run with `#[tokio::test]` —
check whether the existing test uses that attribute or a plain `#[test]`.

**Fix**: B.3 must list `pub fn` → `pub async fn` as an explicit signature
change, and B.1 tests must be marked `#[tokio::test]` where required.

---

### I5. TDD ordering across PR-A is inconsistent — A.2 lands BEFORE A.1 is impossible

**Plan §3.3 commits A.1 through A.22**:

| Commit | Kind | Issue |
|---|---|---|
| A.1 | `feat(tracking-schedule): add chrono-tz workspace dependency` | Impl-first (no red-first test) |
| A.2 | `test(tracking-schedule): TrackingScheduleConfig serde + Default contract tests` | Red gate |
| A.3 | `feat(tracking-schedule): TrackingScheduleConfig types + window_is_active` | Green |

A.1 is ordered first for a reason (deps must land before types that use them),
but the plan's "Test-first for every impl commit" claim in §7.1 has an implicit
exception for A.1. Acknowledge this: A.1 is a **prerequisite commit**, not an
impl commit in the TDD sense, and the plan should say so.

Similarly:
- A.5 green-lights BOTH A.2 (config) AND A.4 (helper) — the latter's tests exist
  in a different file than A.2's. Plan is fine but should explicitly note "A.5
  satisfies two red gates (A.2 + A.4)".
- A.11 green-lights A.10, A.12 adds the wiring — A.12 has no red test. Plan says
  "none new; A.8's ts_active_suppresses_window_switch_events already exercises
  the upload path" — but A.8 tests the whole pipeline, not specifically
  predicate wiring. A focused wiring test at A.12 would strengthen TDD.

**Fix**: make §3.3 prose explicit about: (a) A.1 is a dep bump, not TDD; (b)
A.5 resolves two red gates; (c) A.12 should add one micro-test for the
predicate-closure hookup.

---

### I6. PR-A landing-order claim contradicts itself — "no technical dep" but A.9 requires A.5

**Plan §2.2** says "PR-A Tracking Schedule — landed first... If PR-A lands late,
PR-C is unaffected" and separately §9.2 claims "all three PRs are independent".

But internal PR-A ordering: A.9 (gate 9 loops + audio command) depends on A.5
(introduces `tracking_schedule_active` helper) and A.11 (uploader predicate
builder). If A.9 ships **before** A.11, the uploader gate doesn't work.

§3.3 orders A.9 before A.11 implicitly (numerical sequence), but the prose
doesn't spell out the dependency. A reviewer reading commit-by-commit might
split commits across PRs (e.g. spin A.9 into a standalone PR for faster review)
and break the ordering.

**Fix**: add an explicit dependency graph to §3.3 (e.g. "A.9 requires A.5 ∧
A.11; do not land A.9 without both"). Graphically:

```
A.1 → A.3 → A.5 → A.7 → A.9 → A.17
         ↘ A.4 ↗       ↗
                   A.11 → A.12
         A.13 → A.14
         A.15 → A.16
         A.18 (uses A.5)
         A.19 → A.20 (uses A.14 + A.16)
         A.21, A.22 (docs — no dep)
```

---

### I7. `ConfigManager::get()` deep-clones per flush — plan's predicate closure has a hot-path cost

**Plan A.12** defines the suppression predicate as:

```rust
let cfg_mgr_for_pred = config_manager.clone();
let pred: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
    crate::scheduler::tracking_schedule_active(&cfg_mgr_for_pred.get())
});
```

`ConfigManager::get()` at `crates/oneshim-core/src/config_manager.rs:97-99`:

```rust
pub fn get(&self) -> AppConfig {
    AppConfig::clone(&self.inner.sender.borrow())
}
```

This is a **deep clone of the entire `AppConfig`** every time `flush()` runs.
`AppConfig` contains 37 config sections per `config/sections/` inventory; each
flush pays the clone cost. The alternative `ConfigManager::snapshot() -> Arc<AppConfig>`
at `:122-124` is O(1) Arc-clone, no deep copy.

**Fix**: change the predicate closure to use `snapshot()` not `get()`:

```rust
let pred: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
    crate::scheduler::tracking_schedule_active(&cfg_mgr_for_pred.snapshot())
});
```

This also requires `tracking_schedule_active` to accept `&Arc<AppConfig>` or
`&AppConfig` — both work since `&Arc<T>` deref-coerces.

Same concern applies throughout A.9: every gated scheduler-loop tick calls
`tracking_schedule_active(&cm.get())`. At 1s monitor-loop cadence the cost is
tolerable; at higher-cadence loops (input_interval = 100ms per spec §3.8 row 8)
the deep clone is a measurable regression.

Spec §3.8 risk table did flag: "`tracking_schedule_active` perf at high-cadence
loop ... O(1) early-return when disabled". The early-return is only on
`!enabled`, not after the config clone — the clone happens regardless. Fix by
using `snapshot()` universally, or by having the helper accept `&ConfigManager`
and do the Arc-clone internally.

---

## Minor findings

### M1. Plan line-number drift on `TimelineLayout.tsx:131-140`

Plan cites "`TimelineLayout.tsx:131-140`" for the batchTagMutation consumer.
Actual range at tip is **130-138** (mutation starts at 130, closing brace at 138).
Plan is only 2 lines off; correct to `130-138` on next revision.

### M2. Commit-count table discrepancy (§9.4)

§9.4 table lists PR-B = 11 commits and inventory shows 5 test + 4 impl + 1 docs = 10.
B.11 (REST integration test) puts total at 11, but "Test commits" column says 5
(should be 6). Minor arithmetic.

### M3. Plan's plan-meta §1348 "Korean + English" note says "plan body is English" — plan body is indeed English, no action

Factually correct; noted for completeness.

### M4. Plan references `crates/oneshim-web/frontend/src/pages/setting-tabs/ScheduleSettings.tsx` as a pattern model — file verified to exist at `ScheduleSettings.tsx` but not referenced by line numbers

Plan says "follow existing `ScheduleSettings.test.tsx` pattern if present". Plan
should either confirm existence of the test file or mark as "verify in Loop 3".

### M5. `GeneralTab.stories.tsx` Storybook entry exists but is not mentioned

Plan adds `GeneralTab.tsx` autostart toggle + `GeneralTab.test.tsx`. Existing
`GeneralTab.stories.tsx` is silent in the plan — Storybook story may need
update for the new autostart toggle (for design-review workflows). Minor.

### M6. `cfg_mgr.get()` vs `cfg_mgr.snapshot()` — picked up in I7; also applies to plan §3.8 helper wrapper

Plan's wrapper (§3.8 / spec §3.8 `scheduler/mod.rs` adjacent):

```rust
pub fn tracking_schedule_active(config: &AppConfig) -> bool { ... }
```

...takes `&AppConfig`. Callers always pass `&cm.get()` which forces clone. If
signature becomes `&Arc<AppConfig>` or generic over `Deref<Target=AppConfig>`,
`snapshot()` composes cleanly.

### M7. `monitor.rs` at 498 lines — plan correctly calls out, but adding composite-gate + 1-line helper-call goes to ~499-500 exactly

Spec §3.8 CONS-I06: "`monitor.rs` is at 498 lines... CLAUDE.md sets the guardrail
at under 500". Plan A.7 changes 1 line (`should_run_now` → `capture_permitted_now`)
and A.7 hoists the `save_event` call 6-line block `inside` the gate — net
change could still cross 500. Plan says "Double-check line budget stays ≤ 500
(CONS-I06)". The check is correct but reactive; plan should specify the exact
expected post-change LoC.

### M8. `tracking_schedule_helper.rs` sibling pattern — plan aligns with existing `coaching_helper.rs` / `focus_auto_helper.rs`

Plan §3.8 notes the helper extraction mirrors existing siblings. Verified: both
helpers exist. Good consistency — no action.

---

## Dimensions checklist (A–J)

### A. Task-ordering + dependency correctness

- [x] Inter-task deps: **mostly good**. I5 flags A.1's exception from TDD; I6
  flags missing explicit dep graph.
- [x] PR A → B → C order: spec and plan align; §2.2 rationale solid.
- [x] TDD: 19 test commits paired with 19 impl commits; ratio 1:1. A.1 is the
  only exception (dep bump, justified).
- [x] Circular deps: none.
- [x] Lefthook cold-clippy cost: §8.6 table explicitly bundles A.3/A.5/A.9 +
  B.3/B.5 + C.4/C.6; sound trade-off per memory `feedback_lefthook_clippy_cost.md`.

### B. Spec → plan fidelity

- [x] All 22 Decisions referenced — random sample D1 (§3.3 quoted), D3 (deferred,
  §3.2), D8 (C.6 new endpoint), D13 (A.9 scope expansion), D14 (A.5 overnight fix),
  D15 (C.4 behavior change), D16 (A.1 chrono-tz), D17 (A.7 trigger hoist),
  D-prop (A.17 subscribe), D-errtype (B.5 substring map), D19 (C.3 MAX_BATCH_SIZE),
  D20 (C.7 popover content), D22 (§3.2 single-schedule). All present.
- [x] CONS-C01..C12: spot-checked. CONS-C01 → D13 → A.9; CONS-C04 → A.2 (`dst_*`
  tests); CONS-C05 → A.5 overnight fix; CONS-C07 → B.3 (`ONESHIM_AUTOSTART_STUB`);
  CONS-C09 → C.4 behavior change; CONS-C10 → B.3. All addressed.
- [x] CONS-I01..I16: CONS-I06 (monitor.rs 498 LoC) → A.7/A.9 helper path;
  CONS-I08 → D19 → C.3; CONS-I09 → D8-alt → C.4; CONS-I15 → B.3 OnceLock.
  Others appear in spec §7 via D-codes; plan inherits.
- [x] §3.8 13 gated pipelines: A.8 test commit has 13 test names matching the
  spec's 13 gated rows; A.9 impl commit cites each file:line. Good coverage.
- [x] No new design decisions introduced by plan. Plan is implementation-only.

### C. Hexagonal + ADR compliance

- [x] `TrackingScheduleConfig` in `oneshim-core/src/config/sections/` — correct
  per ADR-001 leaf-crate rule. Plan A.3.
- [x] `tracking_schedule_active(cfg, now)` pure fn in `src-tauri/src/scheduler/loops/tracking_schedule_helper.rs` — binary-crate colocation, plan uses `pub(crate)` visibility. Correct per ADR-001 §1.
  Wrapper `tracking_schedule_active(cfg)` also exposed on `scheduler/mod.rs` —
  plan matches spec §3.8.
- [~] `ConfigChangeBus` via `ConfigManager::subscribe()` — verified API exists
  (`config_manager.rs:113`). Plan's A.17 usage matches. But I2: no existing tray
  task to attach to.
- [x] `BatchUploader::with_suppression_predicate` closure — fits existing
  `with_health_flag` builder pattern per spec §3.9. Plan A.11 mirrors the precedent.
- [x] `OnceLock<bool>` for `has_systemctl` — plan B.3 uses `std::sync::OnceLock`.
  Correct for Rust 1.77.1+ (workspace MSRV per CLAUDE.md).
- [~] `chrono-tz` in `oneshim-core` — D16 notes +2.1MB cost, plan A.1 adds
  workspace dep. Hexagonal dependency-direction note (ADR-001 §4) is
  acknowledged in spec but **not repeated in plan**. Minor — plan should call
  out that this is a conscious architectural deviation.

### D. Commit structure + conventional commit labels

- [x] All 43 commits prefixed `feat` / `fix` / `refactor` / `test` / `docs`. No
  `chore:` — memory `feedback_squash_merge_cliff_skip.md` respected.
- [x] Concrete messages (no "update"/"various"). A.5 message "tracking_schedule_active + capture_permitted_now + should_run_now overnight fix" is specific.
- [x] Test-before-impl order preserved (I5 exception noted).
- [x] No bundled-unrelated concerns. A.3/A.5/A.9 bundles are intra-feature
  (all tracking-schedule).
- [ ] One observation: A.22 (integrity-gate regression test) is marked "(Optional)"
  — should be mandatory given CONS-M15 integrity-gate concerns. Move to §3.3
  proper, not optional.

### E. File-path + line anchor accuracy

Sample of 15 cited anchors — verified pass/fail:

| Citation | Plan says | Actual (tip 5618558c) | Verdict |
|---|---|---|---|
| `scheduler/loops/monitor.rs` total LoC | 498 | 498 | PASS |
| `autostart.rs` total LoC | 549 | 549 | PASS |
| `scheduler/mod.rs` `should_run_now` | :548-571 | 548-571 | PASS |
| `trigger.rs` test count | 13 | 13 | PASS |
| `trigger.rs:370` blackout comment | present | present | PASS |
| `autostart.rs` test count | 9 | 9 | PASS |
| `intelligence.rs:14,124,160` | spawn_analysis/focus/coaching | 14/124/160 match | PASS |
| `events.rs:60-92` process_interval | process branch | 60-92 | PASS |
| `sync.rs:15,87` | oauth_refresh / cross_device_sync | 15/87 match | PASS |
| `agent_runtime_support.rs:251` | (not cited — says main.rs) | ACTUAL location | **FAIL** (see C1) |
| `agent_runtime_support.rs:405` | (not cited — says main.rs) | ACTUAL location | **FAIL** (see C1) |
| `settings.rs:44` ALLOWED_KEYS | :44 | 41-55 (line 44 = "notification") | FAIL (M1/I1) |
| `tags.rs:83-98` batch_add_tag | 83-98 | 83-98 closing | PASS |
| `client.ts:579-587` batchAddTag | :579-587 | 579-587 | PASS |
| `TimelineLayout.tsx:131-140` | :131-140 | 130-138 | Near-match (M1) |

**Net**: 11 PASS, 2 FAIL (C1), 2 near-match (M1/I1). Drift concentrated in 2
hot-spots — composition-root file + ALLOWED_KEYS line.

### F. Contract integrity

- [x] OpenAPI + manifest hand-patch covered in A.21 / B.10 / C.10. Each PR
  updates both files.
- [x] `./scripts/verify-integrity.sh` named as gate; plan §8.2.
- [x] No proto changes — correct, Phase 9 has no gRPC surface.
- [x] Zero new wire codes — spec §6.3 confirms; plan respects.
  Spec's "42 locked codes" vs workspace CLAUDE.md "41" is noted as doc-drift
  follow-up, not a Phase 9 action item.

### G. Rollback paths + feature-flag strategy

- [x] Each PR has a §*.7 rollback-path section. PR-A: revert A.9 + A.17. PR-B:
  revert B.3 or B.5+B.7. PR-C: revert C.4 + C.8.
- [x] `TrackingScheduleConfig::default()` = disabled — plan A.3 `#[serde(default)]`
  ensures backward-compat. No config migration.
- [x] D15 frontend consumer in same commit as backend rename (C.4) — good
  bundling for the 200→500 behavior change.
- [~] No explicit feature-flag (per spec D2: "client-only privacy primitive; no
  server authority"). Reasonable, but means no kill-switch short of revert.
  Consider: a `tracking_schedule.enabled = false` default effectively IS the
  kill-switch since all gates short-circuit on `!enabled`. Document.

### H. Concurrent-writer safety

- [x] Tracking-schedule via `ConfigChangeBus` — `watch::Receiver` uses
  lock-free channel semantics; safe for scheduler-loop consumers.
- [x] Bulk-tag SQL transaction — C.2 uses single `conn.transaction()` scope.
  Plan C.1 test `batch_ops_compete_with_concurrent_writer` explicitly verifies
  no deadlock.
- [~] Tray indicator vs config-change race — I2 notes no existing tray task.
  Once spawned, a race between `sync_tray_state` (menu-event-driven) and the
  new config-change subscriber may cause a 1-tick flicker. Probably tolerable.
- [x] `BatchUploader` predicate race: predicate flips during flush — §3.8 risk
  table marks "Acceptable — in-flight batch completes normally". Reasonable.
- [x] `OnceLock<bool>` for `has_systemctl` — process-lifetime memoization;
  race-free. Memory `reference_serial_test_pattern.md` applies only if tests
  mutate the OnceLock — they shouldn't.

### I. Architectural guardrails (CLAUDE.md workspace)

- [x] `monitor.rs` ≤ 500 LoC — A.7 carefully extracts helper to keep under (see M7).
- [~] `autostart.rs` at 549 — plan B.3 splits into sub-modules per ADR-003 (see
  Q-plan-2). Plan picks "extract" — correct per ADR threshold.
- [x] `AppState` sub-struct policy — plan introduces no new `AppState` fields
  (spec §3.10 explicitly rejects a new atomic). Plan respects.
- [x] Port instance sharing — A.12 wires one predicate closure; no duplication.

### J. Plan's open-questions triage

See "Open-questions disposition" section below.

---

## Open-questions disposition (5 items)

### Q-plan-1: Commit bundling aggressiveness (22 vs 28 commits in PR-A)

**Classification**: Minor / resolve-in-review.
**Recommendation**: keep bundled (A.3/A.5/A.9). Memory `feedback_lefthook_clippy_cost.md`
documents ~16min cold-clippy — 3 bundled cycles saves ~32min total. Review-clarity
cost is acceptable. Loop 3 reviewer can call for split if needed.

### Q-plan-2: Sub-module extraction of `autostart.rs` (~650 LoC post-B.3)

**Classification**: Important (defer decision = technical debt).
**Recommendation**: **extract now**. Reasoning:
- ADR-003 threshold is 500-600 LoC. Post-B.3 the file is ~650 (per plan estimate);
  crossing threshold is not a border case.
- Platform-split layout (`autostart/linux.rs`, `autostart/macos.rs`, `autostart/windows.rs`)
  aligns with `#[cfg(target_os = ...)]` boundaries and makes the module far
  easier to navigate for the non-primary platforms.
- `test_observer` submodule is a clean testing concern — standalone file.
- Leaving a "single-file 650 LoC" would be tech debt. Follow ADR-003 on first
  touch (memory `feedback_file_split_policy.md`: split when SOLID warrants, and
  this does).

### Q-plan-3: `NotificationConfig.tracking_schedule_enabled` sibling-naming drift

**Classification**: Minor.
**Recommendation**: **Alternative B (follow spec, inconsistent with neighbors)**.
Reasoning:
- Spec locks `tracking_schedule_enabled` per CONS-M05.
- Existing neighbors `idle_notification` / `long_session_notification` are
  legacy-named; renaming is out of Phase 9 scope.
- Semantic clarity: `tracking_schedule_enabled` reads cleanly; the "notification.*"
  parent namespace provides disambiguation.
- Cost of Alternative A (`tracking_schedule_notification`): redundant
  "notification.notification.*" pattern; cognitive load higher.
- Resolve-in-review: defer to impl reviewer if they prefer A. Document the
  inconsistency in a plan-footnote.

### Q-plan-4: Tauri IPC vs REST consumer

**Classification**: Minor / resolve-in-review.
**Recommendation**: plan is correct as-stated — frontend uses REST calls
exclusively (verified: `client.ts` has no Tauri `invoke` paths for settings).
The Tauri IPC commands in A.14 and B.5 serve as the native-window compatibility
surface for future Tauri-only features (e.g. tray menu item "Open Tracking
Schedule Settings" could dispatch a Tauri command). No immediate consumer —
document that and leave the IPC commands registered for future-proofing.

### Q-plan-5: PR-A vs PR-B landing order

**Classification**: Minor / resolve-in-review.
**Recommendation**: **keep A → B → C**. Reasoning:
- Spec §§3.8 + 3.9 identifies PR-A as the highest-risk surface (9 newly-gated
  pipelines). Burn down risk early → PR-A first.
- "PR-B first for confidence" is a social-dynamics argument, not a technical one.
  Memory `feedback_dimensional_convergence.md`: converge on the risk dimension,
  not the team-morale dimension.
- No technical crossing between PRs per §9.2.

---

## Verdict

**FAIL**.

Two Critical findings (C1 composition-root file paths, C2 audio signature
change) block the Loop 2 zero-Critical gate. Both are fix-in-place issues: the
plan's design is sound but the implementer-facing details are wrong or missing.

Seven Important findings (I1 ALLOWED_KEYS test update, I2 tray task spawn, I3
PartialEq scope, I4 autostart sync→async, I5 TDD ordering exception, I6 intra-PR-A
dep graph, I7 `get()` vs `snapshot()`) block the zero-Important gate. All are
also fix-in-place.

Eight Minor findings are line-number drift, missing cross-refs, or cosmetic
guidance — no gate impact.

Recommended Loop 2b edits: plan-author revises ~15 line-items in the plan,
totaling <1h of edit time, and re-submits. The plan's architectural soundness
is good — the issues are precision, not design.

**Pass criteria after Loop 2b**:
- C1: all composition-root citations changed to `agent_runtime_support.rs`.
- C2: A.9 explicitly specifies the state-injection path for audio command.
- I1: A.14 lists both ALLOWED_KEYS + test update.
- I2: A.17 prose clarifies "new async task spawned from composition root".
- I3: A.17 lists the exact PartialEq derive set.
- I4: B.3 lists `pub fn` → `pub async fn` signature change.
- I5: §3.3 prose documents A.1 dep-bump exception.
- I6: §3.3 adds explicit intra-PR-A dep graph.
- I7: A.12 + A.9 use `snapshot()` not `get()` for predicate/gate calls.

Once cleared, Loop 3 impl can proceed.

---

_End of Reviewer 1 review. Target wc: 200-400 lines. Actual: ~400._
