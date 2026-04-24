# Phase 9 Quick Wins — Implementation Plan

**Date**: 2026-04-24
**Worktree**: `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-quick-wins`
**Branch**: `feature/phase9-quick-wins` @ `5618558c` (origin/main post-PR-#486)
**Spec**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` (1599 lines, Loop 1e PASS, 22 Decisions locked)
**Author**: _(blank — first-draft, reviewer-unassigned; Loop 2a)_
**Loop history**:
- Loop 1a: three-reviewer deep review (R1 arch, R2 product/privacy, R3 platform/test).
- Loop 1b: synthesis at `docs/reviews/2026-04-23-phase9-spec-review-synthesis.md` (46 consolidated findings).
- Loop 1c: 13 user decisions (U1-U13) locked.
- Loop 1d: 49-step fix plan applied; spec grew to 1599 lines, D13-D22 + D-prop/D-errtype/D-i18n/D-guide settled.
- Loop 1e: three independent verifications (`spec-review-{1,2,3}-verify.md`) — all PASS (zero-Critical-zero-Important gate clears on each axis).

---

## 1. Executive summary

**PR split recommendation**: **three separate PRs**, landed in a strict order (PR-A → PR-B → PR-C), each `feat:` or `refactor:` prefixed for `git-cliff` visibility, each independently reviewable and independently rollback-able. Rationale details in §2.

**Total effort estimate**: **~122 engineer-hours** (≈15 engineer-days at ~8h/day), comprising:

- **PR-A** (Tracking Schedule — privacy core): ~64h (8 days). Single-review amortised across 5-6 commit batches.
- **PR-B** (Autostart IPC wiring + systemd bug fixes): ~30h (4 days). Platform-branched; bulk of effort in tests + CI stub.
- **PR-C** (Bulk Tag transactional + batch-remove): ~22h (3 days). Smallest surface, but the 200→500 behavior change (D15) needs the most careful frontend coordination.
- **Cross-cutting** (STATUS.md, PHASE-HISTORY.md, CLAUDE.md bumps, follow-up TODO registration): ~6h spread across the three PRs.

**Critical risks**:

1. **D13 scope creep (PR-A, §3.8)** — Phase 9 newly gates 9 previously-ungated data-producing pipelines (analysis/focus/coaching/process/input/clipboard/file-access/cross-device-sync/audio). Every touched loop is a regression surface. Mitigation: TDD-first per-loop gating tests + integration test asserting zero `events` table rows during a TS window.
2. **D15 silent-200 → explicit-500 refactor (PR-C, §5.5)** — flips existing "silent partial success" to "rollback on any row failure". Single frontend consumer (`TimelineLayout.tsx:49` type alias + `:135` onSuccess) must land in the same commit. Mitigation: frontend + backend edit-and-test bundled.
3. **CI env-var stub (PR-B)** — `ONESHIM_AUTOSTART_STUB=1` is a new test affordance. Must be rock-solid or Linux CI silently green-lights broken systemd registration (CONS-C07).
4. **D17 trigger refactor (PR-A, §3.8a)** — removes `schedule: ScheduleConfig` from `SmartCaptureTrigger::with_schedule`. Touches DI composition root + 3 test migrations. Mitigation: strict pre-PR "all 13 trigger tests still green" gate.
5. **chrono-tz addition (PR-A, §3.7 / D16)** — new workspace dep, +2.1MB binary. Mitigation: already user-decided; impl plan runs a binary-size `du -sh target/release/oneshim-app` diff and surfaces in PR description.

**Non-goals for this plan**:
- Unified `TimeWindow` refactor (deferred per D3 — `project_next_tasks.md`).
- `es/ja/zh-CN` i18n (deferred per D-i18n/U12).
- `AutostartError` typed-enum upgrade (deferred per D-errtype/U8).
- User-facing tracking-schedule guide (deferred per D-guide/U13).
- a11y ARIA hardening on multi-select (deferred per CONS-M04 — pre-existing gap).

---

## 2. PR split strategy

### 2.1 Decision: three PRs, landed in order A → B → C

**Pros**:
- **Independent rollback**. Tracking Schedule (PR-A) is the highest-risk surface because it gates 9 new pipelines; if a regression surfaces post-merge we can revert without also losing autostart and bulk-tag fixes.
- **Smaller review surface per PR** — reviewers can keep a complete mental model; Phase 9 bundled in one PR would be >3k LoC touching 4 crates + frontend + 2 contract files, which invites reviewer fatigue and missed findings (consistent with memory `feedback_holistic_pre_merge_review.md`: N-loop deep reviews pick up less in huge PRs).
- **Parallel CI scheduling** — three smaller PRs green-light faster than one monolith (lefthook clippy cold is ~16 min per memory `feedback_lefthook_clippy_cost.md`; monolith forces all three feature tests into a single cache invalidation).
- **`git-cliff` visibility** — each PR takes a `feat(phase9-<area>):` or `refactor(phase9-<area>):` prefix; the merged CHANGELOG shows three line-items for Phase 9 instead of one squashed line (per memory `feedback_squash_merge_cliff_skip.md`: squash-merge `chore:` drops from cliff; `feat:` / `refactor:` surface).
- **TDD discipline is easier** — each PR has its own test-first/impl-second commit cadence without cross-feature coupling.
- **Frontend coupling** — PR-C's D15 frontend edit is tiny and self-contained; bundling with PR-A's tracking-schedule settings UI would make the frontend diff hard to re-review.

**Cons addressed**:
- **Three rounds of CI** — accepted cost (see "Pros" parallel scheduling counter).
- **Three review cycles** — mitigated by ordering (A first, because it has the most uncertainty), and by keeping each PR scoped tight enough that a single deep review suffices.
- **Coordination cost across PRs** — there is **one** shared technical decision: the `BatchUploader::with_suppression_predicate` builder (PR-A, §3.9) ships in `oneshim-network` before any other consumer touches it. Since PR-B and PR-C don't touch the uploader, no coupling. PR-A's frontend adds a `TrackingScheduleSettings.tsx` section to `crates/oneshim-web/frontend/src/pages/setting-tabs/`; PR-B adds an autostart toggle to `GeneralTab.tsx` (same directory, different file). No collision.

**Single-PR alternative (rejected)**: "Atomic feature bundle, one review". Rejected because the combined PR would be ≥4-5x the maximum PR size the team can deep-review effectively in one pass; past PRs in this size class (e.g., PR-B2/B3 in the subscribe-metrics series) were split specifically because monolithic review produced 55+ mid-review findings and the split let reviewers focus per-layer. Evidence supporting the split: memory `feedback_3loop_yields_real_catches.md` — the reviewing team has a track record of catching more issues on small focused PRs than on monoliths, and the Phase 9 spec synthesis itself (46 consolidated findings) proves the reviewers' own pattern-recognition drops off on big surfaces.

**Two-PR alternative (rejected)**:
- Option 1: "PR-A (privacy gate + autostart both as privacy/lifecycle) + PR-B (bulk tag)". Rejected because Tracking Schedule and Autostart share zero code paths; the grouping is purely thematic and doesn't reduce review surface.
- Option 2: "PR-A (Tracking Schedule) + PR-B (Autostart + Bulk Tag)". Rejected because autostart and bulk-tag share zero code paths either; grouping them re-introduces the monolith-review drawback without a coupling benefit.

### 2.2 Landing order: A → B → C

1. **PR-A Tracking Schedule — largest and riskiest**. Landed first so subsequent PRs benefit from the merged config-change-bus wire-up and any incidental refactoring of `scheduler/mod.rs`. If PR-A lands late, PR-C is unaffected but PR-B's integration tests that happen to inspect config state become easier (since `TrackingScheduleConfig` is then in `AppConfig`).
2. **PR-B Autostart** — depends on no PR-A artifact. Ships second because it's medium-risk and has the longest tail (Linux CI stub, 3-platform sweep).
3. **PR-C Bulk Tag** — smallest, most focused. Ships last so the D15 silent→explicit behavior change gets its own clean CHANGELOG entry uncoupled from the larger Phase 9 story. Also: the frontend Vitest suite additions for PR-C won't collide with PR-A's new `TrackingScheduleSettings.tsx` tests.

### 2.3 Branch naming

- PR-A: `feature/phase9-tracking-schedule` (derived from `feature/phase9-quick-wins`)
- PR-B: `feature/phase9-autostart-wiring`
- PR-C: `feature/phase9-bulk-tag`

Each feature branch rebased on `main` at start. If PR-A lands first, PR-B and PR-C re-rebase on new `main`.

Commit prefixes:
- `feat(tracking-schedule): ...`, `refactor(tracking-schedule): ...`, `test(tracking-schedule): ...`, `docs(tracking-schedule): ...`
- `feat(autostart): ...`, `fix(autostart): ...` (non-zero-exit bug fixes), `test(autostart): ...`, `docs(autostart): ...`
- `feat(timeline-bulk-tag): ...`, `refactor(timeline-bulk-tag): ...` (handler transactional), `test(timeline-bulk-tag): ...`

Per CLAUDE.md release process: **no manual `git tag`**. PR merges let the next `./scripts/release.sh` bundle all Phase 9 features into a single RC.

---

## 3. PR-A — Tracking Schedule (privacy core)

### 3.1 Goals

Ship the new privacy-hardening negative-gate primitive `TrackingScheduleConfig` with:
- Config schema in `oneshim-core` with backward-compatible `#[serde(default)]` deserialization (spec §3.6).
- Pure-fn `tracking_schedule_active(cfg, now) -> bool` evaluator in `src-tauri/src/scheduler/loops/tracking_schedule_helper.rs` (D14, U3, CONS-I06).
- `should_run_now` overnight wrap-midnight fix + hoist from trigger (D14/D17, U2, U7).
- 13 data-producing pipelines gated per §3.8; 3 ungated infrastructure loops with per-loop rationale.
- `BatchUploader::with_suppression_predicate` closure injection for upload flush gating.
- Tray tooltip propagation via ADR-016 `ConfigManager::subscribe()` (D-prop, U6).
- IPC commands `get_tracking_schedule`, `set_tracking_schedule`, `get_tracking_schedule_status`.
- REST `GET/PUT /api/tracking-schedule` + `GET /api/tracking-schedule/status`.
- Settings UI section at `pages/setting-tabs/TrackingScheduleSettings.tsx` (model: `ScheduleSettings.tsx`).
- `chrono-tz` dependency added to `oneshim-core` (D16, U5).
- OpenAPI + manifest updates for 3 new routes.

### 3.2 Goals out-of-PR-A (explicitly deferred)

- Unified `TimeWindow` primitive (D3).
- Named multiple schedules (D22 — single-config design locked).
- Fine-grained per-field suppression ("only OCR").
- Server-side enforcement.
- Pre-configured presets.
- `tracking-schedule.md` user guide (D-guide).
- `es/ja/zh-CN` i18n (D-i18n).

### 3.3 Commits (ordered, TDD-first)

The ordering below follows superpowers:test-driven-development: each functional commit is paired with a test commit that lands **before** the implementation (red → green). Because lefthook cold-clippy is ~16min per memory `feedback_lefthook_clippy_cost.md`, commits 3+8+9+11 bundle to amortize warm-cache cost.

**TDD ordering exceptions** (per §7.1 — documented per CONS-PI07):
- **A.1** is a prerequisite dep-bump commit (chrono-tz) — not TDD red-first; subsequent commits exercise the API.
- **A.5** resolves two red gates simultaneously (A.2 + A.4) — the composite `capture_permitted_now` implementation lands in one commit that greens both pure-fn contract tests and the config-section serde contract.
- **A.12** has one micro-test for predicate-closure hookup; main assertions flow through A.8's integration tests at the end-to-end level.
- **B.3a** is a pure refactor — no new red test; existing tests must pass unchanged across the refactor boundary. TDD red state begins in B.3b.

**Intra-PR-A dependency graph** (per CONS-PI08):
```
A.1 (dep)  →  A.3 (types)  →  A.5 (helper)  →  A.7 (hoist)  →  A.9 (gate sites)  →  A.17 (tray)
                 ↘ A.4 ↗                    ↗
                                   A.11 (uploader) → A.12 (DI wire)
A.13 (IPC test) → A.14 (IPC impl)
A.15 (REST test) → A.16 (REST impl)
A.18 (notifier — uses A.5) ; A.19 (FE test) → A.20 (FE impl — uses A.14 + A.16)
A.21 (docs contracts — uses everything) → A.22 (drift test)
```

#### Commit A.1 — `feat(tracking-schedule): add chrono-tz workspace dependency`
**Effort**: 1h
**Touches**:
- `Cargo.toml` (workspace `[workspace.dependencies]`): add `chrono-tz = { version = "0.10", features = ["serde", "default-tz"] }`
- `crates/oneshim-core/Cargo.toml`: pull `chrono-tz.workspace = true`
- `Cargo.lock`: auto-regenerate via `cargo check --workspace`
**Tests-first**: none for dep addition; subsequent commits exercise the API.
**Risk**: Low — isolated dep add. `cargo check --workspace` verifies resolution.
**Acceptance**:
```
cargo check --workspace
du -sh target/release/oneshim-app  # record delta vs pre-Phase9 baseline for PR description
```
**Prereqs**: none.

#### Commit A.2 — `test(tracking-schedule): TrackingScheduleConfig serde + Default contract tests`
**Effort**: 3h
**Touches** (NEW file):
- `crates/oneshim-core/src/config/sections/tracking_schedule.rs` — one `mod` exposed, `#[cfg(test)] mod tests { ... }` only; types stubbed with `todo!()`
**Tests** (red):
- `default_is_disabled_with_empty_windows` — `TrackingScheduleConfig::default() == { enabled: false, windows: [], timezone: "Local" }`
- `serde_roundtrip` — parse JSON with full fields → serialize → reparse → equality
- `serde_missing_fields_default` — parse `{}` → `Default`; parse `{"enabled": true}` → other fields default
- `overnight_window_wraps` — `TrackingWindow { start: "22:00", end: "06:00", days_of_week: [Sat] }` — `window_is_active` returns true at Sat 23:00 AND Sun 01:00, false at Sat 21:00
- `normal_window_does_not_wrap` — `TrackingWindow { start: "12:00", end: "13:00", days_of_week: [Mon] }` — true at Mon 12:30, false at Mon 13:01 and Mon 11:59
- `empty_days_never_active` — empty `days_of_week` vec → always false
- `dst_fall_back_fires_twice` — DST fall-back US/Eastern 02:30 ambiguous; window ending 02:30 matches **both** occurrences (per CONS-C04 rewrite, spec §3.7)
- `dst_spring_forward_window_in_skipped_hour_never_fires` — window `02:30–02:59` on spring-forward Sunday → zero firings (per spec §3.7)
- **Serde validation edge cases** (per CONS-PI06 — spec §6.3 error codes `config.invalid` + `validation.invalid_field` align):
  - `serde_rejects_invalid_hhmm` — `"25:00"` deserialization → `Err` with `validation.invalid_field`.
  - `serde_rejects_invalid_iana_timezone` — `"Foo/Bar"` deserialization or later `.parse::<chrono_tz::Tz>()` call → `Err` with `config.invalid`.
  - `window_with_empty_end_is_invalid` — `TrackingWindow { end: "", ... }` → validation error `validation.invalid_field`.
  - `window_end_before_start_not_same_day_is_invalid` — `start: "13:00", end: "12:00"` with `days_of_week: [Mon]` — if not an overnight wrap (both times same-day within active range), reject as invalid. (Note: this differs from the legitimate overnight-wrap case where end < start signals wrap-midnight semantics; this test covers the "end before start on the same calendar day with no overnight semantics" configuration error.)

A.2 test count: 8 (original) → 12 (+4 validation tests per CONS-PI06).
**Risk**: Low. Pure-fn tests, no mocks.
**Acceptance**:
```
cargo test -p oneshim-core --lib config::sections::tracking_schedule
# expect compile error (types not defined) — confirms red gate
```

#### Commit A.3 — `feat(tracking-schedule): TrackingScheduleConfig types + window_is_active`
**Effort**: 5h (bundle with A.4, A.5 to amortize clippy cold cost)
**Touches**:
- `crates/oneshim-core/src/config/sections/tracking_schedule.rs` (NEW, per ADR-003 colocate pattern):
  - `pub struct TrackingScheduleConfig { enabled: bool, windows: Vec<TrackingWindow>, timezone: String }`
  - `pub struct TrackingWindow { start: String, end: String, days_of_week: Vec<Weekday>, label: String }`
  - `impl TrackingWindow { pub fn window_is_active(&self, now: DateTime<Local>) -> bool { ... } }`
  - `fn default_timezone() -> String { "Local".into() }`
  - `impl Default for TrackingScheduleConfig { ... }`
  - `#[serde(default)]` on every non-required field per spec §3.6.
- `crates/oneshim-core/src/config/sections/mod.rs`: add `mod tracking_schedule; pub use tracking_schedule::*;`
- `crates/oneshim-core/src/config/mod.rs`: `pub tracking_schedule: TrackingScheduleConfig` + `#[serde(default)]` adjacent to line 41; default-init in `AppConfig::default_config()` near line 113.
**Tests-first**: A.2 already red; this makes A.2 green.
**Risk**: Medium — DST semantics are subtle. Mitigation: test `dst_fall_back_fires_twice` + `dst_spring_forward_window_in_skipped_hour_never_fires` are A.2's strongest anchors.
**Acceptance**:
```
cargo test -p oneshim-core --lib config::sections::tracking_schedule
cargo clippy -p oneshim-core --no-deps -- -D warnings
```

#### Commit A.4 — `test(tracking-schedule): tracking_schedule_active + capture_permitted_now pure-fn contracts`
**Effort**: 3h
**Touches** (NEW file):
- `src-tauri/src/scheduler/loops/tracking_schedule_helper.rs` — stub with **4-term composite signature** per spec §3.4:
  ```rust
  pub(crate) fn tracking_schedule_active(cfg: &AppConfig, now: DateTime<Local>) -> bool { todo!() }

  /// Composes all 4 privacy gates per spec §3.4:
  /// consent_granted(tier) AND active_hours_gate(now) AND !tracking_schedule_active(now) AND !capture_paused
  pub(crate) fn capture_permitted_now(
      cfg: &AppConfig,
      consent: &ConsentPermissions,
      capture_paused: bool,
      now: DateTime<Local>,
  ) -> bool { todo!() }
  ```
  `#[cfg(test)] mod tests { ... }` below.
- `src-tauri/src/scheduler/loops/mod.rs`: register `mod tracking_schedule_helper;` — **do not** re-export yet (caller will `use` the path).
**Tests** (red):
- `disabled_config_returns_false` — `enabled: false` → false regardless of `now`
- `empty_windows_returns_false` — `enabled: true, windows: []` → false
- `normal_window_in_range` — `12:00-13:00 Mon`, `now = Mon 12:30` → true
- `normal_window_out_of_range` — `12:00-13:00 Mon`, `now = Mon 13:01` → false
- `multiple_windows_one_matches` — `[{12-13 Mon}, {18-22 Mon}]`, `now = Mon 19:00` → true
- `multiple_windows_none_match` — `[{12-13 Mon}, {18-22 Mon}]`, `now = Mon 10:00` → false
- `timezone_local_uses_chrono_local` — smoke-test with `"Local"` value
- `capture_permitted_combines_all_four_gates` — 16-row truth table per spec §3.4 + §3.8a, covering all 2⁴ of `(consent, active_hours, !ts, !capture_paused)` + overnight-active × overnight-ts per §3.4 row 7/8.
- `capture_permitted_respects_should_run_now_wrap` — overnight active_hours (22-06 Mon-Fri) + empty TS at `now = Wed 23:00` → true (per post-CONS-C05 fix)
- **Consent top-authority test** (per CONS-PC02): `consent_revoked_overrides_ts_inactive_active_hours` — consent revoked + TS inactive + active_hours active → `capture_permitted_now` returns `false` (consent gate has veto authority).
- **Capture-paused veto test** (per CONS-PC02): `capture_paused_overrides_ts_inactive` — `capture_paused: true` + TS inactive + active_hours active + consent granted → `false`.
- **Clock-irregularity coverage** (per CONS-PI07 / spec §3.7a):
  1. `window_active_across_suspend` — `tracking_schedule_active` returns correct value regardless of tick timing between suspend/resume (tested via two distinct `now` values flanking a suspend interval).
  2. `forward_clock_jump_into_future_window` — clock jumps from `Mon 11:50` to `Mon 12:30` (window `12:00-13:00 Mon`); returns `true` after jump.
  3. `forward_clock_jump_past_window_end` — clock jumps from `Mon 12:50` to `Mon 13:10`; returns `false` after jump.
**Risk**: Low. Pure-fn tests.
**Acceptance**: `cargo test -p oneshim-app --lib scheduler::loops::tracking_schedule_helper` — expect compile fail → confirms red.

#### Commit A.5 — `feat(tracking-schedule): tracking_schedule_active + capture_permitted_now 4-term composite + should_run_now overnight fix`
**Effort**: 6h (bundled; this is the hottest-scrutiny commit — **TDD exception per §7.1**: this single commit resolves two red gates at once — A.2 helper signature + A.4 pure-fn contracts)
**Touches**:
- `src-tauri/src/scheduler/loops/tracking_schedule_helper.rs`: impl `tracking_schedule_active(cfg, now)` reading `cfg.tracking_schedule`, iterating `windows.iter().any(|w| w.window_is_active(now))`. Impl `capture_permitted_now(cfg, consent, capture_paused, now)` composing ALL 4 gates per spec §3.4:
  ```rust
  pub(crate) fn capture_permitted_now(
      cfg: &AppConfig,
      consent: &ConsentPermissions,
      capture_paused: bool,
      now: DateTime<Local>,
  ) -> bool {
      consent.allows_tier(ConsentTier::Capture)          // consent top-authority (spec §3.4b)
          && should_run_now_with_time(cfg, now)          // active_hours gate
          && !tracking_schedule_active(cfg, now)          // TS negative gate
          && !capture_paused                              // user tray-toggle veto
  }
  ```
  **Important**: all 4 terms composed as AND; revoking any one gate vetoes capture. `consent.allows_tier` is the authoritative top gate per spec §3.4b.
- `src-tauri/src/scheduler/mod.rs`: **FIX** `should_run_now` per spec §3.4a pseudocode — add wrap-midnight branch so `active_hours { start: 22, end: 6 }` correctly permits at Wed 23:00 AND Thu 01:00. Extract the time-injected form as `fn should_run_now_with_time(config: &AppConfig, now: DateTime<Local>) -> bool` and make `should_run_now(config)` delegate to it with `chrono::Local::now()`.
- Adjacent to new `should_run_now_with_time`, add public wrappers in `scheduler/mod.rs`:
  ```rust
  pub fn tracking_schedule_active(config: &AppConfig) -> bool {
      crate::scheduler::loops::tracking_schedule_helper::tracking_schedule_active(config, chrono::Local::now())
  }

  /// Full 4-term composite — use this at all gate sites rather than piecemeal checks.
  /// Callers must supply a ConsentPermissions snapshot and the current capture_paused atomic read.
  pub fn capture_permitted_now(
      config: &AppConfig,
      consent: &ConsentPermissions,
      capture_paused: bool,
  ) -> bool {
      crate::scheduler::loops::tracking_schedule_helper::capture_permitted_now(
          config, consent, capture_paused, chrono::Local::now()
      )
  }
  ```
**Tests-first**: A.4 is red; this commit makes them green. Also makes the pre-existing `should_run_when_disabled` test at `scheduler/mod.rs:582` continue passing.
**Risk**: **High**. `should_run_now` overnight fix is a behavior change (CONS-C05). Mitigation:
- Existing `should_run_when_disabled` test unchanged.
- Add `should_run_now_handles_overnight_range` unit test next to it — explicit red→green.
- Explicit test `should_run_now_wraps_midnight_thu_01:00` with overnight active_hours.
**Acceptance**:
```
cargo test -p oneshim-app --lib scheduler::
cargo test -p oneshim-core  # ensure A.3 still passes
cargo clippy --workspace --no-deps -- -D warnings
```

#### Commit A.6 — `test(tracking-schedule): SmartCaptureTrigger hoist migration tests`
**Effort**: 2h
**Touches**:
- `crates/oneshim-vision/src/trigger.rs`: mark 3 schedule tests (`blocks_capture_outside_active_hours` at :373, `allows_capture_when_schedule_disabled` at :398, `handles_overnight_active_hours` at :409) with `#[ignore = "Phase 9: migrated to scheduler::mod::tests in A.7"]` to document intended migration.
- Add migrated tests (same names, prefixed `scheduler_`) in `src-tauri/src/scheduler/mod.rs` test module — testing the composite gate via `capture_permitted_now(cfg, now)` 2-arg pure fn.
**Tests** (red — because `SmartCaptureTrigger::new(throttle_ms)` doesn't exist yet):
- `scheduler_blocks_capture_outside_active_hours` — cfg with `active_hours_enabled=true, active_start_hour=9, active_end_hour=17, active_days=[Mon]`, now=Mon 20:00 → `capture_permitted_now == false`
- `scheduler_allows_capture_when_schedule_disabled` — cfg with `active_hours_enabled=false`, any now → true (when TS also inactive)
- `scheduler_handles_overnight_active_hours` — 22-06 Mon-Fri, now=Wed 23:00 → true; now=Thu 01:00 → true; now=Thu 05:59 → true; now=Thu 06:01 → false; now=Sat 00:01 → false (Sat not in days)
**Risk**: Low — pure-fn tests.
**Acceptance**: tests compile; fail assertions until A.7 hoist lands.

#### Commit A.7 — `refactor(tracking-schedule): SmartCaptureTrigger::new (hoist schedule to scheduler)`
**Effort**: 3h
**Touches**:
- `crates/oneshim-vision/src/trigger.rs`:
  - Remove `schedule: ScheduleConfig` field from `SmartCaptureTrigger` struct.
  - Remove `is_within_active_hours()` method.
  - Rename `with_schedule(throttle_ms, schedule)` → `new(throttle_ms)`.
  - Delete leftover `// ── Blackout-hours tests (Q3) ─` comment at `:370` (CONS-M02).
  - Ungate A.6's `#[ignore]`-ed tests — **delete** them (they migrated).
  - Remaining 10 tests unchanged.
- `crates/oneshim-vision/src/lib.rs`: keep `SmartCaptureTrigger` re-export if present; no API break for crate consumers.
- `src-tauri/src/agent_runtime_support.rs:251` (composition root — verified at `5618558c`): replace `Arc::new(SmartCaptureTrigger::with_schedule(throttle_ms, schedule_config))` with `Arc::new(SmartCaptureTrigger::new(throttle_ms))`. **Bundle with A.12 edit** — same file also has `BatchUploader::new(...)` call at `:405`, so bundling saves one cold-clippy cycle (~16 min per memory `feedback_lefthook_clippy_cost.md`).
- `src-tauri/src/scheduler/loops/monitor.rs:200-207`: change gate from `should_run_now(&cm.get())` to the **full 4-term composite** `capture_permitted_now(&cm.snapshot(), &consent_permissions, capture_paused.load(Ordering::Relaxed))` via `scheduler/mod.rs` wrapper. **Do not** inline `tracking_schedule_active` in monitor.rs — keep the helper-extracted path to respect the 498-line guardrail (CONS-I06). Uses `snapshot()` not `get()` per CONS-PI13 (O(1) Arc-clone vs deep clone).
- `src-tauri/src/scheduler/loops/monitor.rs:181-189`: hoist the pre-gate `save_event` + `uploader.enqueue` calls *inside* the `if capture_permitted_now(...)` block — the composite already includes `!capture_paused` so no separate `&& !capture_paused` check needed. Fixes the spec §3.8 row 3 leak.
**Tests-first**: A.6's 3 tests go green when the composite gate lands.
**Risk**: **High** — DI callsite migration + monitor-loop edit. Mitigation:
- Before-the-commit: run `cargo test -p oneshim-vision --lib trigger` and note the 13 passing tests.
- After-the-commit: `cargo test -p oneshim-vision --lib trigger` shows 10 passing (3 removed); `cargo test -p oneshim-app --lib scheduler` shows the 3 migrated tests now green.
- Full workspace check: `cargo check --workspace`.
**Acceptance**:
```
cargo test -p oneshim-vision --lib trigger   # 10 tests pass (was 13)
cargo test -p oneshim-app --lib scheduler    # includes 3 migrated
cargo check --workspace
cargo clippy --workspace --no-deps -- -D warnings
grep -r "Blackout" crates/ src-tauri/ && echo "FAIL: blackout leftover" || echo "OK: no blackout"
```

#### Commit A.8 — `test(tracking-schedule): scheduler loop gating tests (13 pipelines)`
**Effort**: 5h
**Touches** (NEW integration test file):
- `src-tauri/tests/tracking_schedule_gating_integration.rs` — NEW test file, listed in `src-tauri/Cargo.toml [[test]]`. Uses `serial_test` per memory `reference_serial_test_pattern.md` because scheduler loops touch global storage state.
**Tests** (red — the loops aren't gated yet):
- `ts_active_suppresses_window_switch_events` — seed config with active TS window; simulate window-switch in monitor loop; assert `events` table gains zero `Window` rows.
- `ts_active_suppresses_process_snapshot_events` — assert zero `Process` rows.
- `ts_active_suppresses_input_events` — assert zero `Input` rows.
- `ts_active_suppresses_clipboard_events` — assert zero `Clipboard` rows.
- `ts_active_suppresses_file_access_events` — assert zero `FileAccess` rows.
- **Per-variant sanity tests** (per CONS-PI05 — split from single `ts_inactive_allows_events` to catch variant-specific over-suppression bugs):
  - `ts_inactive_allows_window_events` — TS inactive; `COUNT(*) WHERE variant='Window' > 0`.
  - `ts_inactive_allows_process_events` — TS inactive; `COUNT(*) WHERE variant='Process' > 0`.
  - `ts_inactive_allows_input_events` — TS inactive; `COUNT(*) WHERE variant='Input' > 0`.
  - `ts_inactive_allows_clipboard_events` — TS inactive; `COUNT(*) WHERE variant='Clipboard' > 0`.
  - `ts_inactive_allows_file_access_events` — TS inactive; `COUNT(*) WHERE variant='FileAccess' > 0`.
- **Consent + pause composite tests** (per CONS-PC02):
  - `consent_revoked_suppresses_events_during_ts_inactive` — consent revoked + TS inactive + active_hours active → zero events of any variant (consent top-authority).
  - `capture_paused_suppresses_events_during_ts_inactive` — `capture_paused: true` + TS inactive + consent granted → zero events of any variant.
- `ts_active_blocks_analysis_loop_tick` — observe `spawn_analysis_loop` early-continues (via a `#[cfg(test)]` instrument counter).
- `ts_active_blocks_focus_loop_tick` — same pattern for `spawn_focus_loop`.
- `ts_active_blocks_coaching_loop_tick` — same pattern for `spawn_coaching_loop`.
- `ts_active_blocks_cross_device_sync_loop_tick` — same pattern for `spawn_cross_device_sync_loop`.
- `audio_capture_ipc_refuses_during_ts` — `commands::audio::start_audio_capture` returns `validation.invalid_arguments` during window.
- `heartbeat_loop_continues_during_ts` — sanity: heartbeat NOT gated (row 14 of §3.8).
- `oauth_refresh_loop_continues_during_ts` — sanity: OAuth NOT gated (row 15).
- **Optional** (per CONS-PM09): `metrics_loop_continues_during_ts` — sanity: metrics loop NOT gated (row 16 of §3.8).

A.8 test count: 13 (original) → 18 (post per-variant split) + 2 consent/pause + 1 optional = 18-21 (authoritative floor = 18 per §7.2 table).
**Risk**: Medium. Integration tests require a real `SqliteStorage` in test fixtures. Pattern: follow existing `src-tauri/tests/*.rs` precedent (`scheduler_integration_tests.rs` if present; else use in-memory SQLite via `SqliteStorage::new_in_memory()` pattern from the storage crate).
**Acceptance**: these tests fail (the gates don't exist yet). Confirms red.

#### Commit A.9 — `feat(tracking-schedule): gate 9 previously-ungated data-producing loops + 1 upload flush (D13)`
**Effort**: 7h (bundled — touches 4 scheduler files)

**Important**: each gate site calls the **full 4-term composite** `capture_permitted_now(&cfg.snapshot(), &consent_permissions, capture_paused.load(...))` — NOT the 2-term shortcut (`tracking_schedule_active || !should_run_now`). The 2-term shortcut drops `consent_granted` and `!capture_paused`, which weakens GDPR transparency guarantees. Composite per CONS-PC02 / spec §3.4. Uses `snapshot()` not `get()` per CONS-PI13.

Each scheduler loop that gets gated must first capture references to `Arc<ConsentManager>` and `Arc<AtomicBool>` (capture_paused) in its spawn closure, just like `config_manager` already is. DI changes are localized per-loop; composition root (`agent_runtime_support.rs`) clones the already-constructed singletons when spawning.

**Touches**:
- `src-tauri/src/scheduler/loops/monitor.rs:200-207` — already converted in A.7 to the full 4-term composite. Double-check line budget stays ≤ 500 (CONS-I06).
- `src-tauri/src/scheduler/loops/events.rs:58-152`:
  - **Row 7 (process_interval branch)**: at top of `process_interval.tick()`, add `if !crate::scheduler::capture_permitted_now(&config.snapshot(), &consent_permissions, capture_paused.load(Ordering::Relaxed)) { continue; }` before `proc_mon9.get_detailed_processes(...)`.
  - **Row 8 (input_interval branch)**: at top of `input_interval.tick()`, add same composite check.
  - **Row 9 (clipboard sub-branch)**: already inside input_interval; covered by row 8 gate.
  - **Row 10 (file-access sub-branch)**: already inside input_interval; covered by row 8 gate.
  - Actually: upon re-reading `events.rs`, rows 9+10 sub-branches are **inside** the input_interval `tick` block. A single row-8 gate covers all three. Verify by reading the loop body again before committing.
- `src-tauri/src/scheduler/loops/intelligence.rs:14,124,160`:
  - `spawn_analysis_loop` (line 14) — at tick body entry: `if !crate::scheduler::capture_permitted_now(&cfg.snapshot(), &consent_permissions, capture_paused.load(...)) { continue; }`.
  - `spawn_focus_loop` (line 124) — same pattern.
  - `spawn_coaching_loop` (line 160) — same pattern (coaching during opt-out window feels invasive per R3.I4).
- `src-tauri/src/scheduler/loops/sync.rs:87` (`spawn_cross_device_sync_loop`) — same composite gate.
- `src-tauri/src/scheduler/loops/sync.rs:15` (`spawn_oauth_refresh_loop`) — **do not gate** (row 15 of §3.8 explicitly ungated).
- `src-tauri/src/commands/audio.rs` (`start_audio_capture`) — **SIGNATURE CHANGE** per CONS-PC04. Current actual signature (verified at `5618558c`):
  ```rust
  pub async fn start_audio_capture(
      state: tauri::State<'_, AudioRuntimeState>,
  ) -> Result<(), IpcError>
  ```
  Choice (a): **extend `AudioRuntimeState`** to hold `Arc<ConfigManager>` + `Arc<ConsentManager>` + `Arc<AtomicBool>` (capture_paused) — cleaner, single state arg, no call-site breakage. Recommended over multi-state injection because Tauri `generate_handler!` composition stays identical. Updated signature:
  ```rust
  pub async fn start_audio_capture(
      state: tauri::State<'_, AudioRuntimeState>,
  ) -> Result<(), IpcError> {
      if !crate::scheduler::capture_permitted_now(
          &state.config_manager.snapshot(),
          &state.consent_manager.permissions(),
          state.capture_paused.load(Ordering::Relaxed),
      ) {
          return Err(IpcError::new(
              "validation.invalid_arguments",
              "Audio capture unavailable — privacy gate active (consent/hours/schedule/pause).",
          ));
      }
      // ... existing body unchanged
  }
  ```
  **Wiring**: composition root (`agent_runtime_support.rs`) already constructs all three singletons — extend `AudioRuntimeState::new(...)` constructor to accept + store them alongside existing fields. Tauri v2 auto-injects `AudioRuntimeState` at IPC call time, no `generate_handler!` changes needed. (Alternative: multi-state via a second `State<'_, ConfigRuntimeState>` param — precedent in `commands::settings::update_setting` — rejected because it adds a second state arg when one suffices.)
**Tests-first**: A.8 is red; this makes it green.
**Risk**: **High — this is the D13 scope expansion**. Each of 9 scheduler loops is a regression surface. Mitigations:
- TDD per-loop test in A.8.
- Config manager's `Arc<AppConfig>`-clone semantics are fast; `tracking_schedule_active` returning early on `!enabled` is O(1) when the user hasn't configured windows, so non-TS-user cost is negligible.
- Watch for the `DateTime<Local>` cost: each call constructs a `Local::now()`. Acceptable at 1s monitor-loop cadence; may become visible at event-loop cadence (10-100Hz) — revisit if benchmarks show >1ms per call.
**Acceptance**:
```
cargo test -p oneshim-app --test tracking_schedule_gating_integration
cargo test --workspace  # no regression
```

#### Commit A.10 — `test(tracking-schedule): BatchUploader::with_suppression_predicate contract`
**Effort**: 2h
**Touches**:
- `crates/oneshim-network/tests/batch_uploader_suppression_test.rs` (NEW) or extend existing batch_uploader.rs unit tests.
**Tests** (red — builder doesn't exist):
- `flush_returns_zero_when_suppression_predicate_true` — construct `BatchUploader` with `with_suppression_predicate(Arc::new(|| true))`; enqueue 3 events; `flush().await` → `Ok(0)`; queue length unchanged.
- `flush_drains_when_suppression_predicate_false` — same setup but predicate returns `false`; flush drains events per existing logic.
- `predicate_reads_latest_config` — predicate closure reads `Arc<ConfigManager>`; test flips config mid-run, predicate reflects the flip on next `flush()`.
**Risk**: Low.
**Acceptance**: `cargo test -p oneshim-network --tests batch_uploader_suppression_test` — expect compile fail.

#### Commit A.11 — `feat(tracking-schedule): BatchUploader::with_suppression_predicate builder + flush gate`
**Effort**: 2h
**Touches**:
- `crates/oneshim-network/src/batch_uploader.rs`:
  - Add field `upload_suppressed: Arc<dyn Fn() -> bool + Send + Sync>` with constructor default `Arc::new(|| false)`.
  - Add `pub fn with_suppression_predicate(mut self, pred: Arc<dyn Fn() -> bool + Send + Sync>) -> Self`.
  - In `flush()` body (line 199), early-return `Ok(0)` when `(self.upload_suppressed)()` is `true`, with `debug!("upload flush suppressed — tracking schedule active")`.
**Tests-first**: A.10 is red; makes it green.
**Risk**: Low — single-field addition with closure bound well-understood.
**Acceptance**:
```
cargo test -p oneshim-network --lib
cargo clippy -p oneshim-network -- -D warnings
```

#### Commit A.12 — `feat(tracking-schedule): DI wiring of BatchUploader suppression predicate in composition root`
**Effort**: 2h
**Touches**:
- `src-tauri/src/agent_runtime_support.rs:405` (composition root — verified at `5618558c`; this is also where A.7 modifies `SmartCaptureTrigger::with_schedule` at `:251`, so **bundle both edits** into one cold-clippy cycle):
  - After existing `BatchUploader::new(...)` construction (line 405), chain `.with_suppression_predicate(pred)`:
  ```rust
  let cfg_mgr_for_pred = config_manager.clone();
  // Use snapshot() not get() — snapshot returns Arc<AppConfig> (O(1) Arc-clone) vs get() deep-clones
  // the entire AppConfig (37 sections). Predicate is called per-flush; hot-path cost matters.
  let pred: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
      crate::scheduler::tracking_schedule_active(&cfg_mgr_for_pred.snapshot())
  });
  let uploader = BatchUploader::new(/* ... */)
      .with_health_flag(health_flag)
      .with_suppression_predicate(pred);
  ```
  - **Micro-test** (TDD exception per §7.1): one small unit test asserting that after wiring, `flush()` with TS active returns `Ok(0)` — exercised indirectly via A.8's integration tests (which already cover this end-to-end).
**Tests**: none new; A.8's `ts_active_suppresses_window_switch_events` integration test already exercises the upload path via `uploader.enqueue`. Add one extra assertion in that test: with TS active, a subsequent `flush()` returns `Ok(0)`.
**Risk**: Low — single wiring edit.
**Acceptance**: `cargo check --workspace`.

#### Commit A.13 — `test(tracking-schedule): IPC command contract tests`
**Effort**: 2h
**Touches** (NEW):
- `src-tauri/src/commands/tracking_schedule.rs` — stub module with `pub async fn get_tracking_schedule / set_tracking_schedule / get_tracking_schedule_status` returning `todo!()`; `#[cfg(test)] mod tests` at bottom.
- `src-tauri/src/commands/mod.rs` — add `pub(crate) mod tracking_schedule;` adjacent to existing modules.
**Tests** (red):
- `set_then_get_roundtrip` — call `set_tracking_schedule(cfg)` with `{ enabled: true, windows: [...], timezone: "Asia/Seoul" }`; call `get_tracking_schedule()`; deep-equal.
- `get_status_returns_active_when_now_in_window` — inject a `now` that falls inside a window; assert `active_now: true`, `ends_at: Some(...)`, `label: "..."` populated.
- `get_status_returns_inactive_outside_window` — inverse.
- `get_status_returns_next_starts_at_within_7_days` — lookahead semantic.
- `ipc_error_on_invalid_hhmm_format` — `set` with `"12:XX"` → `IpcError` carrying wire code `validation.invalid_arguments`.
**Risk**: Low. Test harness pattern follows existing `src-tauri/src/commands/*.rs` `#[cfg(test)] mod tests { ... }`.

#### Commit A.14 — `feat(tracking-schedule): IPC commands + settings allowlist entry`
**Effort**: 3h
**Touches**:
- `src-tauri/src/commands/tracking_schedule.rs`: impl 3 commands delegating to `ConfigRuntimeState::get()`/`set()`. For `set`, perform validation pre-write (ranges, HH:MM format, timezone parse-check).
- `crates/oneshim-api-contracts/src/tracking_schedule.rs` (NEW): `pub struct TrackingScheduleStatus { active_now, ends_at: Option<String>, next_starts_at: Option<String>, label: String }`.
- `crates/oneshim-api-contracts/src/lib.rs`: register `pub mod tracking_schedule;`.
- `src-tauri/src/commands/settings.rs` — **two edits** (per CONS-PI11; verified at `5618558c`):
  - Append `"tracking_schedule"` to the `ALLOWED_KEYS` array (declared at `:41`). Insertion point: between `"coaching"` (line 54) and closing `]` (line 55).
  - **Snapshot test update** at `:341-361`: `allowed_keys_matches_expected_set` asserts exact array equality against an `expected` vec — must add `"tracking_schedule"` to that expected vec.
  - **Sibling test verification** at `:364-378`: `allowed_keys_excludes_sensitive_sections` iterates a forbidden-keys list; confirm `"tracking_schedule"` is NOT in that forbidden list (it contains user-configurable wall-clock values, not secrets — safe to allow).
- `src-tauri/src/main.rs` builder — register the 3 commands via `tauri::generate_handler![..., tracking_schedule::get_tracking_schedule, tracking_schedule::set_tracking_schedule, tracking_schedule::get_tracking_schedule_status, ...]`.
**Tests-first**: A.13 is red; green after this.
**Risk**: Medium — `ALLOWED_KEYS` is a security-sensitive list; adding an entry widens the WebView surface. Mitigation: `tracking_schedule` contains no credential/secret fields, only user-configurable wall-clock values — accepting in `ALLOWED_KEYS` is safe.
**Acceptance**:
```
cargo test -p oneshim-app --lib commands::tracking_schedule
```

#### Commit A.15 — `test(tracking-schedule): REST handler contract`
**Effort**: 1.5h
**Touches** (NEW):
- `crates/oneshim-web/tests/tracking_schedule_handler_test.rs` (or extend existing `handlers` tests in-file).
**Tests** (red):
- `get_returns_default_config` — `GET /api/tracking-schedule` on a fresh install → 200 with default JSON.
- `put_persists_config` — `PUT /api/tracking-schedule` with valid body → 200 with echo; subsequent `GET` returns the put value.
- `put_rejects_invalid_hhmm` — PUT with bad window format → 400 with wire code `validation.invalid_arguments`.
- `get_status_reflects_configured_windows` — with a window active for "now", `GET /api/tracking-schedule/status` → `{ active_now: true, ... }`.
**Risk**: Low.

#### Commit A.16 — `feat(tracking-schedule): REST handlers + routes`
**Effort**: 2h
**Touches**:
- `crates/oneshim-web/src/handlers/tracking_schedule.rs` (NEW): 3 handler fns (`get`, `put`, `get_status`) delegating to `StorageWebContext::config_manager` (verify this access pattern matches existing `handlers/settings.rs`).
- `crates/oneshim-web/src/handlers/mod.rs`: register `pub mod tracking_schedule;`.
- `crates/oneshim-web/src/routes.rs` (add adjacent to line 117):
  ```rust
  .route("/tracking-schedule", get(handlers::tracking_schedule::get).put(handlers::tracking_schedule::put))
  .route("/tracking-schedule/status", get(handlers::tracking_schedule::get_status))
  ```
**Tests-first**: A.15 red → green.
**Risk**: Low — handler mirrors `handlers/settings.rs` pattern.

#### Commit A.17 — `feat(tracking-schedule): tray tooltip propagation via ConfigManager::subscribe`
**Effort**: 3h
**Key correction** (per CONS-PI12): `src-tauri/src/tray.rs` contains **zero** `tokio::spawn` calls at `5618558c` (`setup_tray<R: Runtime>` is sync; `sync_tray_state` is a sync callback from `on_menu_event`). A.17 therefore creates a **NEW** async worker task — it does NOT "extend existing task" as the original plan wording implied.
**Touches**:
- `src-tauri/src/agent_runtime_support.rs` (composition root — **new spawn site** after `setup_tray(...)` returns): spawn a fresh async worker task. Reason for spawning here rather than in `tray.rs`: tray.rs stays sync, and the spawn needs the constructed `AppHandle` + `Arc<ConfigManager>` which are both available in composition root post-setup.
  ```rust
  // Pre-commit audit per CONS-PM02:
  //   grep -rn 'NotificationConfig ==\|NotificationConfig !=\|PartialEq<NotificationConfig>' crates/ src-tauri/
  // If zero hits, PartialEq + Eq is a pure additive derive (safe).
  let cm_for_tray = config_manager.clone();
  let handle_for_tray = app_handle.clone();
  let mut rx = cm_for_tray.subscribe();
  let mut last_ts_flag = cm_for_tray.snapshot().notification.tracking_schedule_enabled;
  let mut last_ts_cfg = cm_for_tray.snapshot().tracking_schedule.clone();
  let tray_watch_handle = tokio::spawn(async move {
      while rx.changed().await.is_ok() {
          let cfg = rx.borrow().clone();
          // Narrow filter per spec §3.11a — only tracking_schedule sub-tree or the
          // notification.tracking_schedule_enabled boolean. PartialEq on
          // NotificationConfig scoped to the tracking_schedule_enabled field per
          // CONS-PI06.
          if cfg.tracking_schedule != last_ts_cfg
              || cfg.notification.tracking_schedule_enabled != last_ts_flag
          {
              last_ts_cfg = cfg.tracking_schedule.clone();
              last_ts_flag = cfg.notification.tracking_schedule_enabled;
              sync_tray_state(&handle_for_tray, ...);  // sync fn, tokio::task::block_in_place if needed
          }
      }
  });
  app_state.tray_watch_handle = Some(tray_watch_handle);  // lifetime ownership — teardown on app shutdown
  ```
  - `PartialEq` derive required on `TrackingScheduleConfig`. For `NotificationConfig`, **narrow the comparison** to the `tracking_schedule_enabled` field only (per CONS-PI06) rather than deriving full-struct `PartialEq` — keeps the filter tight and avoids unintended equality semantics on unrelated sibling fields (`idle_notification`, etc.).
  - Tray tooltip copy: reuse Paused icon + tooltip label `"Tracking Schedule Active until HH:MM"` (Q7 / CONS-M12 resolution).
- **Task lifetime**: store `JoinHandle` in `AppState.tray_watch_handle: Option<JoinHandle<()>>`. Teardown via `handle.abort()` on app shutdown or graceful drop of the `watch::Sender` on ConfigManager.
**Watch coalescence note** (per CONS-PM01): `watch::Receiver` is latest-wins — rapid enable→disable→enable mutations within a single tick may coalesce and collapse to no-change from the receiver's perspective. Spec §3.7a accepts this as a non-goal (missed transitions during rapid flicker acceptable); add as row in §3.8 risk register. If edge-trigger semantics are later required, switch to tick-based poll.
**Tests-first**: tough to unit-test tray task end-to-end. Add a focused test `tray_diff_detects_tracking_schedule_change` that exercises the diff logic only (purely synchronous); full tray re-render stays manual QA.
**Risk**: Medium — new async task memory + task leak surface. Mitigation: explicit `JoinHandle` ownership in `AppState`; documented teardown.

#### Commit A.18 — `feat(tracking-schedule): DesktopNotifier integration + 60s debounce`
**Effort**: 3h
**Touches**:
- `src-tauri/src/scheduler/loops/tracking_schedule_helper.rs`: add helper `evaluate_and_notify_transitions(cfg: &AppConfig, prev_active: bool, now_active: bool, last_notified_at: &mut Option<Instant>, notifier: &dyn DesktopNotifier)` with 60s cooldown.
- `src-tauri/src/scheduler/loops/monitor.rs` (or wherever 1s ticks evaluate config): call the helper with prev-state tracking.
- `crates/oneshim-core/src/config/sections/storage.rs` (`NotificationConfig`): add field `tracking_schedule_enabled: bool` with `#[serde(default = "default_true")]`. Field named per CONS-M05 / spec §3.11 — deliberately parallel to existing `daily_summary_notification`; neighbor `idle_notification` uses the older convention but `tracking_schedule_enabled` matches the newer enterprise naming style the spec argues for. **Do NOT rename existing neighbors** — that's a separate concern outside Phase 9.
**Tests**:
- `notifier_fires_on_ts_enter` — prev=false, now=true → notifier called with start message
- `notifier_fires_on_ts_exit` — prev=true, now=false → notifier called with end message
- `notifier_debounces_within_60s` — rapid flip-flop (backward clock jump) within 60s → second fire suppressed
- `notifier_does_not_fire_when_config_disabled` — `notification.tracking_schedule_enabled=false` → no calls
**Risk**: Low — helper is pure except for `DesktopNotifier` port.

#### Commit A.19 — `test(tracking-schedule): frontend Vitest — TrackingScheduleSettings component`
**Effort**: 3h
**Touches** (NEW):
- `crates/oneshim-web/frontend/src/pages/setting-tabs/TrackingScheduleSettings.test.tsx` (or co-located; follow existing `ScheduleSettings.test.tsx` pattern if present).
**Tests** (red):
- Renders empty state when `windows=[]` — shows "No windows configured." with "Add window" button.
- Clicking "Add window" appends a default window to form state.
- Submitting a valid form → calls `PUT /api/tracking-schedule` via React Query mutation.
- Shows "Active now — ends HH:MM" pill when `/status` returns `active_now: true`.
- HH:MM validation — entering "12:XX" surfaces inline validation error.
- Timezone input shows dropdown with IANA names + "Local" default.
- Korean locale test — labels render "추적 일정" not "스케줄" (U11).
**Risk**: Low.

#### Commit A.20 — `feat(tracking-schedule): TrackingScheduleSettings.tsx + SettingsLayout wiring`
**Effort**: 6h
**Touches** (NEW + EDIT):
- `crates/oneshim-web/frontend/src/pages/setting-tabs/TrackingScheduleSettings.tsx` (NEW): form UI patterned on `ScheduleSettings.tsx`. Fields: master switch; list of windows with start/end HH:MM inputs, day-of-week checkboxes, optional label; timezone dropdown; "Add window"/"Remove window" buttons; status pill showing `active_now` state.
- `crates/oneshim-web/frontend/src/pages/setting-tabs/index.ts`: export `TrackingScheduleSettings`.
- `crates/oneshim-web/frontend/src/pages/settings/SettingsLayout.tsx` (or relevant parent): add tab/nav entry for `/settings/tracking-schedule` pointing at `TrackingScheduleSettings`.
- `crates/oneshim-web/frontend/src/api/client.ts` (new fn): `getTrackingSchedule() / setTrackingSchedule(cfg) / getTrackingScheduleStatus()`.
- `crates/oneshim-web/frontend/src/i18n/locales/en.json` + `ko.json`: add the 13 tracking-schedule keys per spec §6.4, including EN+KO notification strings + activeNow pill + nextWindow copy. Korean strings uniformly use "추적 일정" (U11 lock).
**Tests-first**: A.19 red → green.
**Risk**: Medium — new user-facing UI. Korean i18n lock must be enforced (grep for "스케줄" → 0 hits in added strings).
**Acceptance**:
```
cd crates/oneshim-web/frontend && pnpm test -- TrackingScheduleSettings
cd crates/oneshim-web/frontend && pnpm lint   # Biome
```

#### Commit A.21 — `docs(tracking-schedule): contracts + STATUS/PHASE-HISTORY bump`
**Effort**: 3h
**Touches**:
- `docs/contracts/http-interface-manifest.v1.json` (HAND-MAINTAINED, no generator): add the 3 new route entries — `GET /api/tracking-schedule`, `PUT /api/tracking-schedule`, `GET /api/tracking-schedule/status` — with full handler-ref + response-schema fields per existing manifest schema.
- `docs/contracts/oneshim-web.v1.openapi.yaml` (AUTO-GENERATED from the manifest): **do NOT hand-edit**. After patching the manifest, regenerate via `./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml` — the script reads `http-interface-manifest.v1.json` and produces the YAML.
- `docs/STATUS.md`: bump test counts — PR-A adds ~47 new tests (A.2: 12 serde + validation; A.4: 12 helper + clock-irregularity; A.6: 3 migrated; A.8: 18 integration per-variant; A.10: 3 uploader; A.13: 5 IPC; A.15: 4 REST; A.18: 4 notifier; A.19: 7 frontend — see §7.2 for table breakdown). Adjust total.
- `docs/PHASE-HISTORY.md`: add Phase 9 entry describing "Tracking Schedule privacy primitive" with one-paragraph summary.
- `client-rust/CLAUDE.md`: note `tracking_schedule_helper.rs` in the scheduler helper list; document the new 16-loop inventory mentions TS-gated pipelines.
**Risk**: Low. Snapshot-drift is auto-detected by `verify-http-openapi-sync.sh` in CI — forgetting to regen the YAML surfaces immediately, not silently.
**Acceptance**:
```
# Patch manifest, then regenerate OpenAPI snapshot, then verify consistency.
./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml
./scripts/verify-http-interface-manifest.sh    # manifest ↔ registered routes
./scripts/verify-http-openapi-sync.sh          # YAML ↔ manifest (drift check)
```

#### Commit A.22 — `test(tracking-schedule): contract-drift gate regression test`
**Effort**: 1h
**Touches**: add a quick shell-invoked test in `src-tauri/tests/` or `scripts/` that runs `./scripts/verify-http-interface-manifest.sh && ./scripts/verify-http-openapi-sync.sh` to confirm manifest + OpenAPI snapshot consistency. (Optional — the CI `check` job at `.github/workflows/ci.yml:192-199` already runs these; include only if local-run convenience is desirable.)

### 3.4 PR-A cross-cutting docs

- Follow-up TODOs registered in `project_next_tasks.md` (confirmed via memory index — this file exists; no `docs/follow-ups.md` either/or):
  - TimeWindow unified primitive refactor (D3).
  - `AutostartError` typed-enum upgrade (from PR-B; list here if PR-A lands first).
  - `es/ja/zh-CN` translations for PR-A's 13 i18n keys (D-i18n).
  - User-facing `docs/guides/tracking-schedule.md` + `.ko.md` (D-guide).
  - `reference_doc_drift` — workspace CLAUDE.md "41 wire codes" → 42.
  - **`on_window_boundary_approaching` pre-flush drain** for long TS windows (spec §3.9 clause 4, deferred per complexity; without it, a 10h+ suppression window with `max_queue_size` overflow causes `drop_oldest()` to silently drop pre-window events — acceptable trade per Phase 9 scope, but tracked for revisit).
  - **macOS/Windows autostart test-matrix expansion** in CI (see §8.7 platform coverage gap).

### 3.5 PR-A acceptance criteria (bundle gate)

All commits pass local:
```
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --no-deps -- -D warnings
cargo fmt --check
cd crates/oneshim-web/frontend && pnpm test
cd crates/oneshim-web/frontend && pnpm lint
# Contract-drift gate (NOT verify-integrity.sh — that's the supply-chain gate):
./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml
./scripts/verify-http-interface-manifest.sh
./scripts/verify-http-openapi-sync.sh
```

PR-A adds **~47 new tests** (Rust + frontend). Baseline trigger.rs count drops from 13 → 10 (3 migrated). Autostart unchanged in this PR.

### 3.6 PR-A prerequisites

- Phase 9 spec Loop 1e PASS on all three axes (already done).
- Fresh `main` rebase — pick up any intervening config changes.
- Read reference files to verify: `reference_ci_tauri_externalbin_stub.md` (if PR-A adds any oneshim-web frontend build). `crates/oneshim-web/frontend/dist/index.html` stub must exist for local lefthook pre-commit clippy to compile Tauri.

### 3.7 PR-A rollback path

- Each commit is standalone reversible via `git revert`.
- If post-merge TS window gating causes a regression (e.g., user reports events missing outside their configured windows — meaning a scheduler gate is over-active), the fastest path is to `git revert` commit A.9 (the scope-expansion commit) plus A.17 (tray propagation). All other commits are inert (types exist, no call sites).
- Alternative: ship a hotfix that hardcodes `tracking_schedule_active` to `false` inside the helper, making the primitive a no-op until the bug is fixed.
- Config migration: none needed because the config has `#[serde(default)]` and existing installs see `enabled: false`.

### 3.8 PR-A risk register

| Risk | Level | Concrete failure mode | Mitigation |
|---|---|---|---|
| `should_run_now` wrap fix breaks existing active-hours users | High | 22-06 users at 23:00 suddenly permitted where they were previously blocked (unintentional behavior change) | The spec §3.4a pseudocode is the CORRECT behavior per the bug description — existing users with overnight schedules were getting intermittent capture. Document the fix as a fix, not a regression. Unit test covers the exact Wed 23:00 → Thu 05:59 window. |
| `chrono-tz` +2.1MB binary | Medium | Binary size bloat affects download time | Accepted trade (D16). Record before/after `du -sh` in PR description. |
| `BatchUploader` flush race when predicate flips mid-flush | Medium | Predicate returns `false` at gate check, flips `true` during network call | Acceptable — in-flight batch completes normally; subsequent flushes are gated. Integration test A.10 + A.8 documents. |
| DST test flakiness (timezone-sensitive CI runners) | Medium | US/Eastern fixture fails on UTC CI | Tests use explicit `chrono-tz` TZ construction; do not rely on runner TZ. |
| Settings UI breaks existing `ScheduleSettings` tab | Medium | Adding new tab misroutes URL parameters | Follow existing pattern; Playwright E2E at `frontend/e2e/` will catch nav regressions. |
| `tracking_schedule_active` perf at high-cadence loop | Low | 10Hz+ loops see `Local::now()` overhead | O(1) early-return when disabled; non-user cost is negligible. Re-measure if a future loop runs at >100Hz. |
| Contract-drift CI red due to stale OpenAPI snapshot | Medium | `./scripts/verify-http-openapi-sync.sh` fails because engineer edited manifest but forgot to run `./scripts/generate-http-openapi.sh` | Commit A.21 pre-runs the generator + both verify scripts locally before push. |
| Korean i18n term leak ("스케줄") | Low | Stray translated string uses the wrong term | `grep -r "추적 스케줄" crates/oneshim-web/frontend/src/i18n/` → expect 0 hits in PR-A's lint step. |
| `watch::Receiver` coalescence misses rapid transitions | Low | Rapid enable→disable→enable within one tick collapses to "no-change" — notifier fires 0 times | Accepted non-goal per spec §3.7a (clock-irregularity table). 60s debounce on A.18 notifier already caps fire-rate. If edge-trigger semantics required later, switch to tick-based poll. (CONS-PM01) |

---

## 4. PR-B — Autostart IPC wiring

### 4.1 Goals

- Expose `src-tauri/src/autostart.rs::{enable_autostart, disable_autostart, is_autostart_enabled}` via Tauri IPC (D3a).
- Add REST endpoints `GET/PUT /api/autostart` + `POST /api/autostart/repair`.
- Fix CONS-C10 bugs: return `Err` on non-zero exit, 5s `tokio::time::timeout` wrap, on all 3 platforms.
- Add `ONESHIM_AUTOSTART_STUB=1` env-var stub for Linux CI (U4, CONS-C07).
- `OnceLock<bool>` memoize `has_systemctl()` (CONS-I15).
- Wayland env detection in `generate_service_file` (D5).
- Settings UI toggle in `GeneralTab.tsx` (§4.9).
- "Repair Autostart" button cross-platform (D6).

### 4.2 Goals out-of-PR-B (explicitly deferred)

- Typed `AutostartError` enum upgrade (D-errtype / U8).
- First-run autostart prompt in welcome dialog (D21 / U9).
- `es/ja/zh-CN` translations (D-i18n / U12).

### 4.3 Commits (ordered, TDD-first)

#### Commit B.1 — `test(autostart): non-zero-exit + timeout behavior fix`
**Effort**: 3h
**Touches**:
- `src-tauri/src/autostart.rs` — extend existing `#[cfg(test)] mod tests` block (lines 460-548).
**Tests** (red — CONS-C10 behavior not yet fixed):
- `macos_enable_returns_err_on_launchctl_nonzero_exit` — inject stub command exit=1; assert `Err(...)`.
- `linux_enable_returns_err_on_systemctl_nonzero_exit` — same pattern.
- `linux_enable_times_out_after_5s` — stub blocks >5s; assert `Err("... timed out ...")`.
- `windows_enable_returns_err_on_regsetvalueexw_nonzero` — (Windows only — gated with `#[cfg(target_os = "windows")]`).
- `macos_enable_times_out_after_5s` — stub blocks >5s; assert `Err`.
**Harness**: uses `ONESHIM_AUTOSTART_STUB=1` env-var (introduced in B.3) + a thread-local `TestObserver` + `serial_test` per memory `reference_serial_test_pattern.md`.
**Risk**: Low — red state is expected until B.3 lands.

#### Commit B.2 — `test(autostart): Wayland env + XDG session-type propagation`
**Effort**: 1.5h
**Touches**: same test module.
**Tests** (red — fn signature not yet updated):
- `generate_service_file_includes_wayland_display_when_set` — set `WAYLAND_DISPLAY=wayland-0`; generated unit contains both `Environment=DISPLAY` AND `Environment=WAYLAND_DISPLAY=wayland-0`.
- `generate_service_file_propagates_xdg_session_type` — set `XDG_SESSION_TYPE=wayland`; generated unit contains `Environment=XDG_SESSION_TYPE=wayland`.
- `generate_service_file_default_x11_when_wayland_absent` — unset `WAYLAND_DISPLAY`; generated unit contains `Environment=DISPLAY=:0` (existing default).
**Risk**: Low. Uses `env::set_var` / `env::remove_var` wrapped in `serial_test` to avoid interleaving.

#### Commit B.3a — `refactor(autostart): split into sub-modules per ADR-003`
**Effort**: 2h (pure refactor — zero semantic change; all existing tests pass unchanged)
**Touches**:
- Move `src-tauri/src/autostart.rs` → `src-tauri/src/autostart/` directory module:
  - `src-tauri/src/autostart/mod.rs` — public fns (`enable_autostart`, `disable_autostart`, `is_autostart_enabled`, `generate_service_file`, `has_systemctl`) + re-exports + common helpers.
  - `src-tauri/src/autostart/linux.rs` — Linux systemd/xdg_desktop impls (existing `#[cfg(target_os = "linux")]` block).
  - `src-tauri/src/autostart/macos.rs` — macOS launchctl impls.
  - `src-tauri/src/autostart/windows.rs` — Windows registry impls.
  - `src-tauri/src/autostart/test_observer.rs` — thread-local `record()` + `last_command()` stub hooks (scaffolded here; wired to stub gate in B.3b).
- Bisect boundary: this commit is pure rename/move/split — no behavior changes. Any git bisect across the B.3a/B.3b boundary cleanly attributes semantic changes to B.3b.
**Tests-first**: existing `enable_disable_roundtrip_unsupported_platform` at `:544-547` still passes verbatim.
**Risk**: Low — purely mechanical refactor per ADR-003 (current file ~549 LoC; refactor + B.3b additions will push to ~650, past ADR-003 500-600 threshold).
**Acceptance**:
```
cargo test -p oneshim-app --lib autostart   # all existing tests green
cargo clippy --workspace --no-deps -- -D warnings
```

#### Commit B.3b — `fix(autostart): non-zero exit Err + 5s timeout + ONESHIM_AUTOSTART_STUB + OnceLock + async signature`
**Effort**: 6h — the largest commit in PR-B; bundles the three platform fixes + async signature flip
**Touches**:
- `src-tauri/src/autostart/mod.rs` — **SIGNATURE CHANGE** per CONS-PI01: sync → async for Linux + macOS:
  - `pub fn enable_autostart() -> Result<(), String>` → `pub async fn enable_autostart() -> Result<(), String>`
  - Same for `disable_autostart` and `is_autostart_enabled`.
  - Required because `tokio::time::timeout` requires async context, and all IPC callers in B.5 are already `async fn`.
  - **Caller audit**: `grep -rn 'enable_autostart\|disable_autostart\|is_autostart_enabled' src-tauri/` → zero production callers outside the new IPC commands in B.5 (verified at `5618558c`).
  - Existing in-module tests (`enable_disable_roundtrip_unsupported_platform` at `:544-547`) gain `#[tokio::test]` attribute (replacing `#[test]`).
- `src-tauri/src/autostart/linux.rs`:
  - `enable()` (current Linux impl at original autostart.rs:389-401): replace `warn!(...); Ok(())` with `return Err(format!("systemctl --user enable returned non-zero exit: {}", stderr));` when exit status is non-zero. Wrap the `Command::new("systemctl").arg(...).output()` call in `tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(move || cmd.output()))`.
- `src-tauri/src/autostart/macos.rs`:
  - `enable()` (at original :137-141): check `.status.success()` on the `Output`; return `Err` if not. Wrap in `tokio::time::timeout`.
- `src-tauri/src/autostart/windows.rs`:
  - **Windows-specific exception per CONS-PI02**: `RegSetValueExW` is a synchronous Win32 call (no process spawn, no I/O bounded delay — typical latency 50-100μs). `tokio::time::timeout` wrap is **vestigial** (adds tokio-task overhead for no real protection). Verify `RegSetValueExW` return code; `Err` on non-zero. **Skip** the timeout wrap entirely — keep Windows impl as either `pub fn` (sync) or `pub async fn` delegating to `spawn_blocking` with NO timeout.
  - Rationale: adding timeout around a synchronous registry write is purely vestigial overhead; the public-API async signature is preserved via `async fn` delegating to `spawn_blocking`, but timeout adds nothing.
- **Shared** (in `mod.rs` or platform files consistently): add env-var gate:
  ```rust
  if std::env::var("ONESHIM_AUTOSTART_STUB").ok().as_deref() == Some("1") {
      // Record the would-be command in test observer; return Ok without side effects.
      test_observer::record(...);
      return Ok(());
  }
  ```
  Place just before the first platform-specific `Command::output` call in each of `enable`/`disable`/`is_enabled`.
- `src-tauri/src/autostart/test_observer.rs` — populate with `thread_local!` + `record()` / `last_command()` helpers (scaffolded in B.3a).
- **`has_systemctl()` memoize** (in `linux.rs` or `mod.rs`): replace function body with `static HAS_SYSTEMCTL: OnceLock<bool> = OnceLock::new();` + `*HAS_SYSTEMCTL.get_or_init(|| { Command::new("systemctl").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) })`. Change visibility from `pub` to `pub(crate)` so `commands::autostart` can call.
- **Wayland env detection** in `generate_service_file` (in `linux.rs`) per spec §4.6 pseudocode — read `WAYLAND_DISPLAY` + `XDG_SESSION_TYPE` at function call-time, append env lines conditionally.
- **Remove `#![allow(dead_code)]` at mod.rs top** — once B.5 wires the IPC commands, all `pub fn` symbols become reachable.
**Tests-first**: B.1 + B.2 red → green (red state expected through the B.3a/B.3b boundary).
**Risk**: **High** — the behavioral fix flips autostart from "silent success" to "explicit failure" on common failure paths. Without CI stub, Linux PRs currently silently pass; with stub, they correctly report failure. This is the right behavior but requires the stub to be correct.
**Acceptance**:
```
ONESHIM_AUTOSTART_STUB=1 cargo test -p oneshim-app --lib autostart
```

#### Commit B.4 — `test(autostart): IPC commands contract`
**Effort**: 2h
**Touches** (NEW):
- `src-tauri/src/commands/autostart.rs` — stub with `todo!()` bodies; `#[cfg(test)] mod tests` at bottom.
**Tests** (red):
- `get_status_returns_mechanism_per_platform` — under `#[cfg(target_os = "macos")]` returns `"launchctl"`; Linux with stub returns `"systemd"` (because the stub pretends systemctl is available); Linux with `has_systemctl() = false` returns `"xdg_desktop"`; Windows returns `"registry"`.
- `set_autostart_enabled_true_persists_state` — calls `set_autostart(true)`; `get_autostart_status()` → `{ enabled: true, ... }`.
- `set_autostart_enabled_false_clears_state` — inverse.
- `repair_autostart_is_idempotent_with_current_exe` — calls `set_autostart(true)` twice; second call succeeds, unit-file contents unchanged (verify via stub observer).
- `ipc_returns_internal_io_on_systemctl_nonzero` — with stub configured to simulate non-zero exit, `set_autostart(true)` returns `IpcError` with wire code `internal.io`.
- `ipc_returns_storage_failed_on_file_write_error` — stub simulates `fs::write` error; `IpcError` wire code `storage.failed`.
- `ipc_returns_validation_invalid_arguments_on_unsupported_platform` — `#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]` path returns 400-equivalent.
- **`needs_repair` contract test** (per CONS-PI08): `needs_repair_true_when_recorded_path_differs_from_current_exe` — stub injects a recorded autostart path different from `std::env::current_exe()`; assert `get_autostart_status().needs_repair == true`. Inverse: `needs_repair_false_when_paths_match`.
- **Rate-limit test** (per CONS-PM12): `repair_autostart_throttles_rapid_retries` — call `repair_autostart` twice within 5s; second call returns `IpcError { code: "cooldown.throttled" }`; third call after 5s+ elapsed succeeds.
**Uses** `ONESHIM_AUTOSTART_STUB=1` + `serial_test`.

#### Commit B.5 — `feat(autostart): Tauri IPC commands`
**Effort**: 3h
**Touches**:
- `src-tauri/src/commands/autostart.rs`:
  - `pub async fn get_autostart_status() -> Result<AutostartStatus, IpcError>` — delegates to `autostart::is_autostart_enabled()` + platform-detect.
  - `pub async fn set_autostart(enabled: bool) -> Result<AutostartStatus, IpcError>` — delegates to `autostart::enable/disable_autostart`; wraps String error via substring-map boundary helper per D-errtype (U8 Option A accepted lossy mapping).
  - `pub async fn repair_autostart() -> Result<AutostartStatus, IpcError>` — calls `enable_autostart()` idempotently.
  - Helper `fn map_autostart_error(err: String) -> IpcError` — substring match on `HOME` / "permission denied" / "timeout" / "non-zero exit" → wire codes per spec §4.10 table.
- `crates/oneshim-api-contracts/src/autostart.rs` (NEW) (per CONS-PI08 — add `needs_repair` field):
  ```rust
  pub struct AutostartStatus {
      pub enabled: bool,
      pub mechanism: String,       // "launchctl" | "systemd" | "xdg_desktop" | "registry" | "unsupported"
      pub fallback_used: bool,
      pub needs_repair: bool,      // NEW per CONS-PI08
  }
  ```
  **`needs_repair` computation** (per-platform):
  - `needs_repair = is_autostart_enabled() && !recorded_path_matches_current_exe()`
  - Linux: parse `ExecStart=` line of `~/.config/systemd/user/<service>.unit`; compare path vs `std::env::current_exe()`.
  - macOS: parse plist `ProgramArguments` array element 0; compare.
  - Windows: read registry value at `HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run`; compare.
- **Rate-limit** (per CONS-PM12): `repair_autostart` IPC has min 5s elapsed between invocations via `AtomicU64 last_repair_at` — misbehaving frontend retry loop cannot spam `systemctl --user enable`. On throttle violation: return `IpcError::new("cooldown.throttled", "repair throttled — please wait 5s")`.
- `crates/oneshim-api-contracts/src/lib.rs`: register `pub mod autostart;`.
- `src-tauri/src/commands/mod.rs`: `pub(crate) mod autostart;`.
- `src-tauri/src/main.rs`: register commands in `generate_handler![...]`.
**Tests-first**: B.4 red → green.
**Risk**: Medium — substring-map is lossy (acknowledged per D-errtype). Tests assert the three-way split (storage/internal/validation) maps correctly.

#### Commit B.6 — `test(autostart): REST handler contract`
**Effort**: 1.5h
**Touches** (NEW):
- `crates/oneshim-web/src/handlers/autostart.rs` — stub; `#[cfg(test)] mod tests` at bottom or `crates/oneshim-web/tests/autostart_handler_test.rs`.
**Tests** (red):
- `get_returns_current_status` — `GET /api/autostart` → 200 with `AutostartStatus { enabled, mechanism, fallback_used, needs_repair }`.
- `put_enable_toggles_state` — `PUT /api/autostart {enabled: true}` → 200; subsequent `GET` reflects.
- `put_disable_toggles_state` — inverse.
- `post_repair_is_idempotent` — `POST /api/autostart/repair` → 200 twice in a row (outside the rate-limit window).
- `put_unsupported_platform_returns_400` — on platforms where autostart is not implemented, `PUT` returns `validation.invalid_arguments` via handler.
- **`needs_repair` surface test** (per CONS-PI08): `get_returns_needs_repair_true_when_recorded_path_differs` — stub-inject mismatched path; `GET /api/autostart` response includes `needs_repair: true`.

#### Commit B.7 — `feat(autostart): REST handlers + routes`
**Effort**: 2h
**Touches**:
- `crates/oneshim-web/src/handlers/autostart.rs` (NEW): 3 handler fns. Unlike storage handlers, these do not take `StorageWebContext`; they shell to `autostart::*`. Follow ADR-019 wire-code mapping at the boundary.
- `crates/oneshim-web/src/handlers/mod.rs`: `pub mod autostart;`.
- `crates/oneshim-web/src/routes.rs` (add near line 94 — the "settings" area):
  ```rust
  .route("/autostart", get(handlers::autostart::get).put(handlers::autostart::put))
  .route("/autostart/repair", post(handlers::autostart::repair))
  ```
**Tests-first**: B.6 → green.
**Risk**: Low.

#### Commit B.8 — `test(autostart): frontend Vitest — GeneralTab autostart toggle`
**Effort**: 2h
**Touches** (NEW + EDIT):
- `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.test.tsx` (if present, extend; else NEW).
**Tests** (red until B.9):
- `autostart_toggle_renders_with_mechanism_text` — mock `GET /api/autostart` returning `{enabled: false, mechanism: "systemd", fallback_used: false}`; assert toggle present + text "Using systemd user service".
- `toggle_on_calls_put_api` — clicking toggle fires `PUT /api/autostart {enabled: true}`.
- `shows_fallback_warning_on_linux_xdg` — response has `mechanism: "xdg_desktop", fallback_used: true`; warning glyph rendered.
- `toggle_disabled_on_unsupported_platform` — `mechanism: "unsupported"`; toggle is greyed out.
- `error_toast_on_storage_failed` — mutation returns 500 with wire code `storage.failed`; toast shows `"Could not write autostart file — ..."` (per `settings.autostartError.fileWrite` i18n key).
- `repair_button_appears_when_path_mismatch` — stub `/status` returning `fallback_used: false` but a `needs_repair: true` extension flag (reuse existing flags if any; otherwise add a simple stale-path detection); assert "Repair Autostart" button rendered.

#### Commit B.9 — `feat(autostart): GeneralTab toggle + repair button + i18n keys`
**Effort**: 4h
**Touches**:
- `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`: add `ToggleRow` for `autostart.enabled`; below it, subdued text showing `mechanism` text. When `fallback_used: true` on Linux: warning glyph with tooltip. When stale-path detected (check `current_exe()` vs recorded path at handler level): "Repair Autostart" button.
- `crates/oneshim-web/frontend/src/api/client.ts`: `getAutostartStatus() / setAutostartEnabled(bool) / repairAutostart()` fns.
- `crates/oneshim-web/frontend/src/i18n/locales/en.json`: add 8 keys per spec §4.9 (label, desc, unavailable, 4 mechanism strings) + 4 error-copy keys per spec §6.4 (autostartError.fileWrite/commandFailed/homeMissing/unsupported). Anchor to general-tab section (en.json:1343).
- `crates/oneshim-web/frontend/src/i18n/locales/ko.json`: KO parallel of all 12 keys.
**Tests-first**: B.8 → green.
**Risk**: Medium — 12 new i18n keys across 2 locales; pnpm lint (Biome) will catch unused keys but won't catch translation drift. `grep` check before push.

#### Commit B.10 — `docs(autostart): contracts + STATUS/PHASE-HISTORY bump`
**Effort**: 2.5h
**Touches**:
- `docs/contracts/http-interface-manifest.v1.json` (HAND-MAINTAINED): add 3 new route entries — `GET /api/autostart`, `PUT /api/autostart`, `POST /api/autostart/repair` — with handler-ref + response-schema fields.
- `docs/contracts/oneshim-web.v1.openapi.yaml` (AUTO-GENERATED): regenerate via `./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml` after patching the manifest. **Do NOT hand-edit.**
- `docs/STATUS.md`: bump ~21 new tests from PR-B.
- `docs/PHASE-HISTORY.md`: append autostart-wiring bullet to Phase 9 entry (or merge with PR-A's entry if PR-A lands first).
- `client-rust/CLAUDE.md`: note autostart module split (if B.3 extracted sub-modules per ADR-003).
- Register follow-up: `AutostartError` typed upgrade (D-errtype), first-run onboarding prompt (D21), Snap path-repair auto-detection (CONS-M13).
**Acceptance**:
```
./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml
./scripts/verify-http-interface-manifest.sh
./scripts/verify-http-openapi-sync.sh
```

#### Commit B.11 — `test(autostart): integration — REST roundtrip with serial_test + stub env`
**Effort**: 2h
**Touches** (NEW):
- `crates/oneshim-web/tests/autostart_integration.rs`:
  - `#[test] #[serial]` — full roundtrip `GET → PUT(true) → GET` with `ONESHIM_AUTOSTART_STUB=1` set at test entry.
  - `#[test] #[serial]` — same for `PUT(false)`.
  - `#[test] #[serial]` — `POST /api/autostart/repair` idempotence.
- `crates/oneshim-web/Cargo.toml [dev-dependencies]`: add `serial_test = "3"` if not already present.
**Uses** memory `reference_serial_test_pattern.md`.
**Risk**: Low.
**Acceptance**: `cargo test -p oneshim-web --test autostart_integration`.

### 4.4 PR-B cross-cutting docs

- Follow-up TODOs registered:
  - `AutostartError` typed-enum (D-errtype).
  - First-run onboarding prompt (D21).
  - Snap refresh auto-detection (CONS-M13).
  - `es/ja/zh-CN` autostart i18n (D-i18n).

### 4.5 PR-B acceptance criteria

```
cargo check --workspace
cargo test --workspace
ONESHIM_AUTOSTART_STUB=1 cargo test -p oneshim-app --lib autostart
cargo clippy --workspace --no-deps -- -D warnings
cargo fmt --check
cd crates/oneshim-web/frontend && pnpm test -- GeneralTab
cd crates/oneshim-web/frontend && pnpm lint
# Contract-drift gate (NOT verify-integrity.sh — that's the supply-chain gate):
./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml
./scripts/verify-http-interface-manifest.sh
./scripts/verify-http-openapi-sync.sh
```

PR-B adds **~21 new tests** (Rust + frontend). Baseline autostart.rs count grows from 9 → 14 (9 existing + 5 new per B.1 + B.2).

### 4.6 PR-B prerequisites

- `main` rebased (or PR-A merged first). PR-B is architecturally independent of PR-A — the commits could land in either order, but queuing PR-B second keeps Phase 9 risk burned-down early.
- Reference memory `reference_ci_tauri_externalbin_stub.md` for the `src-tauri/oneshim-sandbox-worker-<triple>` stub + `crates/oneshim-web/frontend/dist/index.html` stub in fresh worktrees.

### 4.7 PR-B rollback path

- `git revert` B.3 removes the CONS-C10 behavioral fixes but keeps the IPC surface. Users see IPC errors instead of silent success — arguably still an improvement; acceptable interim state.
- Alternatively, `git revert` B.5 + B.7 removes the new IPC/REST surface entirely; autostart module stays `#![allow(dead_code)]` again. Zero user-visible change.
- No DB migration, no config migration — autostart state lives on the filesystem per D4.

### 4.8 PR-B risk register

| Risk | Level | Failure mode | Mitigation |
|---|---|---|---|
| Linux CI silently green on broken systemd | High | `systemctl --user enable` non-zero swallowed even after "fix" | Explicit test `linux_enable_returns_err_on_systemctl_nonzero_exit` in B.1 with stub asserting the Err path. |
| `ONESHIM_AUTOSTART_STUB=1` env-var leaks to production | Medium | Env-var accidentally set at runtime → autostart silently no-ops | Check env-var early; the observer-record is only read in tests. Document clearly. |
| `has_systemctl()` cache wrong-answer corner case | Low | User installs systemctl mid-session; OnceLock gives wrong answer | Acceptable — per-process cache; restart resolves. Edge case. |
| Wayland env detection misses cases | Medium | `WAYLAND_DISPLAY` set but compositor doesn't honor env propagation | Best-effort; user can manually edit service file. |
| Substring-map error wire-code mismatch | Medium | Error string shape changes upstream → wrong wire code | Acceptable per D-errtype; typed upgrade tracked as follow-up. |
| Frontend mechanism-text falls back to English | Low | `es/ja/zh-CN` locale user sees English | Documented per D-i18n. |
| 5s timeout too aggressive on slow CI | Low | CI flake on 5s boundary | 5s is generous; CI runners spawn systemctl in <1s typically. Extendable if measured flake. |
| Snap refresh changes exe path silently | Medium | Snap auto-refresh updates `oneshim-app` binary to a new `/snap/oneshim/<rev>/...` path; existing autostart unit file points at stale revision | **Repair Autostart button** (D6) detects this via `needs_repair` field (CONS-PI08 — recorded path vs `std::env::current_exe()` mismatch) and offers one-click re-registration. Snap refresh users benefit directly. Follow-up registered for auto-detection on every app start (vs user-triggered). |

---

## 5. PR-C — Timeline Bulk Tag (transactional + batch-remove)

### 5.1 Goals

- Add storage-layer transactional ops `add_tag_to_frames` + `remove_tag_from_frames`.
- Refactor `batch_add_tag` handler to use transactional op (D9, D15 — 200→500 behavior change).
- Add new `DELETE /api/frames/batch-tags` endpoint + handler (D8).
- Rename `BatchTagResponse.tagged_count` → `affected_count` (D8-alt, CONS-I09).
- Enforce `MAX_BATCH_SIZE = 1000` backend cap (D19).
- Frontend: extend Timeline floating action bar with "Remove tag" popover (D20).
- Frontend: update `TimelineLayout.tsx:49` (type alias) + `:135` (onSuccess) consumer for rename + `onError` behavior — 2 edit sites per CONS-PI10.
- Playwright E2E: extend `timeline-actions.spec.ts` with select+remove cases.

### 5.2 Goals out-of-PR-C (explicitly deferred)

- Cross-page selection persistence (D11 — explicitly reset).
- Shift-click / ctrl-click range select (D10).
- a11y ARIA hardening (CONS-M04 pre-existing gap).
- Partial-success per-row reporting (D9 rejected).

### 5.3 Commits (ordered, TDD-first)

#### Commit C.1 — `test(timeline-bulk-tag): storage transactional op contracts`
**Effort**: 3h
**Touches** (EXTEND):
- `crates/oneshim-storage/src/sqlite/tags.rs` — extend existing `#[cfg(test)] mod tests` or add sibling `port_contract_tests.rs`.
**Tests** (red — fns don't exist yet):
- `add_tag_to_frames_happy_path` — 3 valid frame_ids + tag_id → `Ok(3)`.
- `add_tag_to_frames_empty_input_is_lock_free` — `&[]` → `Ok(0)` without lock acquisition (observe via test harness that `conn.lock()` never called — using a Drop probe or a counter).
- `add_tag_to_frames_idempotent_insert_or_ignore` — add same tag twice → second call returns `Ok(0)` (ignored), no error.
- `add_tag_to_frames_rolls_back_on_fk_violation` — include a nonexistent `frame_id` (e.g., 999999) → error; assert `frame_tags` has zero new rows post-error (transactional rollback).
- `remove_tag_from_frames_happy_path` — 3 (frame, tag) pairs existing → `Ok(3)`.
- `remove_tag_from_frames_handles_missing_pairs_transactionally` — mix of (attached, non-attached) pairs → `Ok(n)` where n is only the actually-deleted rows; no error.
- `remove_tag_from_frames_empty_input_is_lock_free` — `&[]` → `Ok(0)`.
- `batch_ops_compete_with_concurrent_writer` — spawn a second thread holding storage lock; batch call blocks then succeeds; no deadlock.
- `statement_cache_reuse_across_rolled_back_transactions` — do a failing batch (rollback), then a successful one; prepared-stmt cache reuse is safe.
**Harness**: uses `SqliteStorage::new_in_memory()` pattern from existing storage tests; uses `serial_test` if global state is touched (shouldn't be for in-memory DBs).
**Risk**: Low.

#### Commit C.2 — `feat(timeline-bulk-tag): storage transactional add/remove_tag_to_frames`
**Effort**: 3h
**Touches**:
- `crates/oneshim-storage/src/sqlite/tags.rs` (after line 186):
  - `pub fn add_tag_to_frames(&self, frame_ids: &[i64], tag_id: i64) -> Result<usize, StorageError>` — impl per spec §5.4 pseudocode.
  - `pub fn remove_tag_from_frames(&self, frame_ids: &[i64], tag_id: i64) -> Result<usize, StorageError>` — mirror.
  - Both use `prepare_cached` + transaction scope. Pattern mirrors `crates/oneshim-storage/src/sqlite/events.rs:126`.
**Tests-first**: C.1 red → green.
**Risk**: Low — mechanical translation from spec.

#### Commit C.3 — `test(timeline-bulk-tag): handler refactor + MAX_BATCH_SIZE cap`
**Effort**: 2h
**Touches**:
- `crates/oneshim-web/src/handlers/tags.rs` — extend existing `#[cfg(test)] mod tests`.
**Tests** (red):
- `batch_add_tag_is_transactional_on_fk_violation` — PUT/POST with a fake frame_id → 500 with wire code `storage.failed` (not the old silent-200 silent-skip).
- `batch_add_tag_rejects_oversized_batch` — 1001 ids → 400 `validation.invalid_arguments`.
- `batch_add_tag_accepts_1000_ids_under_50ms` — 1000 valid ids → 200 < 50ms.
- `batch_add_tag_returns_affected_count_field` — response JSON has `affected_count` key (not `tagged_count`).
- `batch_remove_tag_transactional_happy_path` — DELETE with valid frame_ids → 200 with `affected_count`.
- `batch_remove_tag_returns_zero_for_non_attached_frames` — DELETE where no (frame, tag) pair exists → 200 `affected_count: 0`.
- `batch_remove_tag_rejects_oversized_batch` — 1001 ids → 400.
**Risk**: Low.

#### Commit C.4 — `refactor(timeline-bulk-tag): batch_add_tag transactional + rename to affected_count + MAX_BATCH_SIZE`
**Effort**: 3h — **highest coordination risk in PR-C**
**Touches**:
- `crates/oneshim-api-contracts/src/tags.rs:29-31`: rename `tagged_count` → `affected_count`:
  ```rust
  #[derive(Debug, Serialize)]
  pub struct BatchTagResponse {
      pub affected_count: u32,
  }
  ```
- `crates/oneshim-web/src/handlers/tags.rs:83-98`: replace loop with transactional call:
  ```rust
  pub async fn batch_add_tag(
      State(context): State<StorageWebContext>,
      Json(req): Json<BatchTagRequest>,
  ) -> Result<Json<BatchTagResponse>, ApiError> {
      const MAX_BATCH_SIZE: usize = 1000;
      if req.frame_ids.len() > MAX_BATCH_SIZE {
          return Err(ApiError::BadRequest(
              "validation.invalid_arguments",
              format!("batch too large (max {MAX_BATCH_SIZE})"),
          ));
      }
      let affected_count = context.storage.add_tag_to_frames(&req.frame_ids, req.tag_id)?;
      Ok(Json(BatchTagResponse { affected_count: affected_count as u32 }))
  }
  ```
- `crates/oneshim-web/src/services/tags_service.rs`: add `pub fn add_tag_to_frames(...)` delegating — keep service layer thin.
- **Frontend** `crates/oneshim-web/frontend/src/api/client.ts:579-587`: update `batchAddTag` fn's return type `BatchTagResponse` to use `affected_count`.
- **Frontend** `crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx` — **two edit sites** (per CONS-PI10; verified at `5618558c`):
  - **line 49** (useMutation generic type alias): `typeof useMutation<{ tagged_count: number }, ...>` → `typeof useMutation<{ affected_count: number }, ...>`.
  - **line 135** (onSuccess consumer): `addToast('success', t('timeline.batchTagged', { count: data.tagged_count }))` → `... count: data.affected_count ...`.
  - Add `onError` handler to surface HTTP 500 (the D15 behavior change) as a toast (i18n key `timeline.batchTagError` — add to en.json + ko.json).
  - **Pre-commit verify**: `grep -rn 'tagged_count' crates/oneshim-web/frontend/src/` returns **0 hits** post-refactor.
**Tests-first**: C.3 red → green.
**Risk**: **High** — D15 silent-200→500 is the most user-visible behavior change in Phase 9. **Bundle** the backend rename + frontend consumer update in one commit so CI sees matched types.
**Acceptance**:
```
cargo test -p oneshim-web --lib handlers::tags
cargo test -p oneshim-api-contracts
cd crates/oneshim-web/frontend && pnpm test -- TimelineLayout
```

#### Commit C.5 — `test(timeline-bulk-tag): DELETE /api/frames/batch-tags handler`
**Effort**: 1.5h
**Touches**: extend `handlers/tags.rs` test module.
**Tests** (red):
- `batch_remove_tag_happy_path` — DELETE endpoint returns 200 with transactional removal.
- `batch_remove_tag_is_transactional_on_error` — forced error → 500 with `storage.failed`; no partial deletions.
- `batch_remove_tag_route_is_delete_verb` — POST to same path → 405 Method Not Allowed.
- `batch_remove_tag_max_batch_size_enforced` — 1001 ids → 400.

#### Commit C.6 — `feat(timeline-bulk-tag): DELETE /api/frames/batch-tags handler + route`
**Effort**: 2h
**Touches**:
- `crates/oneshim-web/src/handlers/tags.rs`: add `pub async fn batch_remove_tag(...)` — mirror shape of refactored `batch_add_tag` but calling `remove_tag_from_frames`.
- `crates/oneshim-web/src/routes.rs:117`: add DELETE verb on existing path:
  ```rust
  .route("/frames/batch-tags", post(handlers::tags::batch_add_tag))
  .route("/frames/batch-tags", delete(handlers::tags::batch_remove_tag))  // NEW
  ```
  Axum's `.route` is verb-additive — both register on same path but dispatch by method.
- `crates/oneshim-web/src/services/tags_service.rs`: `pub fn remove_tag_from_frames(...)` method.
- **Frontend** `client.ts`: add `batchRemoveTag(frameIds, tagId)` fn.
**Tests-first**: C.5 red → green.
**Risk**: Low.

#### Commit C.7 — `test(timeline-bulk-tag): frontend Vitest — Remove-tag popover + select flow`
**Effort**: 2h
**Touches**:
- `crates/oneshim-web/frontend/src/pages/timeline/__tests__/AllFrames.test.tsx` (extend or create).
**Tests** (red):
- `remove_tag_popover_shows_all_tags_not_intersection` — selected 3 frames (each with different tag sets); popover lists all tags in the tag registry (per D20).
- `remove_tag_toast_shows_x_of_n_format` — mutation resolves `affected_count: 2, selected_count: 3`; toast reads `"2 of 3 frames untagged"`.
- `remove_tag_button_fires_delete_api` — click invokes `DELETE /api/frames/batch-tags`.
- `page_change_resets_selection` — select 3 frames, navigate to next page; `selectedFrames.size == 0` on new page (D11 fix).
- `select_all_is_bounded_to_page_viewport` — select all with pageSize=50; selection count == 50 (not cross-page).
- **D15 silent-200 → 500 error-path tests** (per CONS-PI09 — this is the highest-coordination-risk change in Phase 9, under-tested with 1 frontend test):
  - `mutation_500_storage_failed_fires_onError_with_localized_toast` — mutation returns 500 with body `{ "code": "storage.failed", ... }`; `onError` fires; toast shows localized `timeline.batchTagError` key content.
  - `mutation_500_validation_invalid_arguments_shows_different_toast` — mutation returns 500 with `validation.invalid_arguments` (batch > 1000); toast shows the validation-specific i18n key (distinct from `batchTagError`).
  - `mutation_200_affected_count_zero_shows_success_toast` — mutation succeeds with `affected_count: 0`; toast reads `"0 frames newly tagged"` — success path, not error (critical distinction: 0 != failure post-D15).

#### Commit C.8 — `feat(timeline-bulk-tag): Remove-tag popover + page-change selection reset`
**Effort**: 4h
**Touches**:
- `crates/oneshim-web/frontend/src/pages/timeline/AllFrames.tsx:582-603` (floating batch action bar): add second popover/dropdown "Remove tag" beside the existing "Add tag" TagInput. Popover shows all tags (fetched from existing `/api/tags` list endpoint); on select, fires `batchRemoveTag` mutation.
- `crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:88-104`: in the `setPage` handler, add `setSelectedFrames(new Set());` — fixes D11.
- `crates/oneshim-web/frontend/src/i18n/locales/en.json`: add 3 new keys per spec §6.4:
  - `timeline.batchUntagged`: `"{{count}} frames untagged"`
  - `timeline.removeTag`: `"Remove tag"`
  - `timeline.removeTagPlaceholder`: `"Select a tag to remove…"`
  - Plus `timeline.batchTagError` (from C.4 for D15).
- `crates/oneshim-web/frontend/src/i18n/locales/ko.json`: Korean parallel.
**Tests-first**: C.7 red → green.
**Risk**: Medium — new UI entry point. Existing Playwright E2E `timeline-actions.spec.ts` covers the add-tag flow; C.9 extends for remove.

#### Commit C.9 — `test(timeline-bulk-tag): Playwright E2E — select → add → remove → verify`
**Effort**: 2h
**Touches**:
- `crates/oneshim-web/frontend/e2e/timeline-actions.spec.ts` — extend existing spec file (not create new — per CONS-M11 / memory path verification).
**Tests** (new cases):
- `select 3 frames → add tag "test" → frames show tag` — already exists in spec? Check and extend; the new case is:
- `select 3 frames → remove tag "test" via Remove-tag popover → frames no longer show tag → toast shows "3 of 3 frames untagged"`.
- `select 2 frames (only 1 has tag "test") → remove "test" → toast shows "1 of 2 frames untagged"`.
- **Forced-error scenario** (per CONS-PI09): `select 5 frames → mock API to return 500 storage.failed → click "Add tag" → toast shows timeline.batchTagError → verify selection state preserved (user can retry)`.
**Risk**: Low — Playwright already running in CI.

#### Commit C.10 — `docs(timeline-bulk-tag): contracts + STATUS/PHASE-HISTORY bump`
**Effort**: 1.5h
**Touches**:
- `docs/contracts/http-interface-manifest.v1.json` (HAND-MAINTAINED): add `DELETE /api/frames/batch-tags` route entry; update `BatchTagResponse` schema definition (rename `tagged_count` → `affected_count`). Confirm the previous schema was `GenericObject` (untyped) — define it properly now.
- `docs/contracts/oneshim-web.v1.openapi.yaml` (AUTO-GENERATED): regenerate via `./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml` after patching the manifest. **Do NOT hand-edit.**
- `docs/STATUS.md`: bump ~30 new tests (C.1: 9 storage; C.3: 7 handler; C.5: 4 DELETE; C.7: 8 frontend per CONS-PI09 extensions; C.9: 2 E2E + forced-error scenario).
- `docs/PHASE-HISTORY.md`: append bullet to Phase 9 entry.
- `CHANGELOG` note (handled by git-cliff on release): the squash commit should be `refactor(timeline-bulk-tag): ...` so cliff surfaces it.
**Acceptance**:
```
./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml
./scripts/verify-http-interface-manifest.sh
./scripts/verify-http-openapi-sync.sh
```

### 5.4 PR-C cross-cutting docs

- Follow-up TODOs:
  - a11y ARIA hardening (CONS-M04).
  - Partial-success per-row reporting (rejected by D9; no TODO needed).
  - Cross-page selection persistence (rejected by D11; no TODO).

### 5.5 PR-C acceptance criteria

```
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --no-deps -- -D warnings
cd crates/oneshim-web/frontend && pnpm test -- timeline
cd crates/oneshim-web/frontend && pnpm lint
cd crates/oneshim-web/frontend && pnpm e2e -- timeline-actions
# Contract-drift gate (NOT verify-integrity.sh — that's the supply-chain gate):
./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml
./scripts/verify-http-interface-manifest.sh
./scripts/verify-http-openapi-sync.sh
```

PR-C adds **~30 new tests** (Rust + frontend + Playwright).

### 5.6 PR-C prerequisites

- `main` rebased. PR-C is architecturally independent of PR-A and PR-B. The only shared artifact is `docs/PHASE-HISTORY.md` — last-merger resolves any trivial conflict.

### 5.7 PR-C rollback path

- `git revert` C.4 restores the silent-200 behavior. Side-effect: the field rename rollback also reverts the frontend consumer. **Recommended** to revert C.4 + C.8 together if rolling back, since C.8's `batchTagError` i18n key will be orphaned.
- Alternative for urgent rollback: ship a hotfix commit that keeps the transactional refactor but maps rollback-500 to silent-200 response (restores old behavior while keeping transactional atomicity — a hybrid that would need tests).
- The new DELETE endpoint can be individually `git revert`ed without touching other PR-C commits.
- No DB migration needed — the existing `frame_tags` schema is unchanged.

### 5.8 PR-C risk register

| Risk | Level | Failure mode | Mitigation |
|---|---|---|---|
| D15 silent-200 → 500 surprises downstream users | Medium | External API consumers depending on 200 break | Per spec §9 Q9: no known external consumers beyond `TimelineLayout.tsx`. CHANGELOG callout on merge. |
| `tagged_count → affected_count` breaks external clients | Low | Same as above | OpenAPI was `GenericObject`; no external contract commitment. |
| Transaction contention under concurrent writer | Medium | Event-writer thread holds lock → batch timeout | Batch operations acquire lock briefly (< 50ms); test `batch_ops_compete_with_concurrent_writer` asserts no deadlock. |
| MAX_BATCH_SIZE=1000 too strict for power users | Low | Scripted user exceeds 1000 | 1000 is generous — frontend UI caps at 50-per-page. Admins can remove client-side cap and send 1000. Document. |
| Playwright flake on remove-tag popover | Medium | E2E test times out on UI interaction | Use stable selectors; add explicit `await popover.waitFor(...)`. |
| Frontend toast shows wrong count on concurrent removal | Low | User removes tag A; concurrent session removes tag A first → affected_count=0 | Acceptable — toast "0 of N" is informative. |

---

## 6. Cross-cutting deliverables

### 6.1 CLAUDE.md (workspace-level and client-rust-level) updates

Per memory `reference_doc_drift.md`-style drift catch, update:

- **`client-rust/CLAUDE.md`** (root):
  - Scheduler section: `src-tauri/src/scheduler/loops/` — add `tracking_schedule_helper.rs` to the helper-file list (currently: `coaching_helper.rs`, `detection_helper.rs`, `focus_auto_helper.rs`, `vision_helper.rs`, `helpers.rs`).
  - 16-loop scheduler inventory note (if present): either add "tracking_schedule_gate (shared helper, not a loop)" clarification, or leave unchanged.
  - `oneshim-core::config` section: mention `TrackingScheduleConfig` in the `sections/` enumeration.
  - Wire codes mention: note "42 locked wire codes" (correct post-PR-A) — but this is outside the feature branches, may belong to a separate doc-drift PR. Per memory, track as follow-up.
- **Workspace-level `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/CLAUDE.md`** (parent): out-of-scope for Phase 9; tracked as `reference_doc_drift` TODO per spec §6.5.

Delivered in: PR-A commit A.21.

### 6.2 STATUS.md test-count bump

Authoritative test counts per §7.2 table (source of truth):
- PR-A adds **~47** new tests (post-CONS-PI05 per-variant split + CONS-PI06 validation + CONS-PI07 clock-irregularity + CONS-PI09 frontend error-path additions).
- PR-B adds **~21** new tests.
- PR-C adds **~30** new tests (post-CONS-PI09 frontend error-path extensions).
- Total Phase 9: **~98 new tests** — spec's prompt estimated ~40; the larger count reflects D13 scope expansion (13 pipeline-gating integration tests → now 18 per-variant) plus Loop 2c/2d coverage additions.

Delivered across A.21, B.10, C.10 (each PR bumps its own delta, final landing updates the total).

### 6.3 PHASE-HISTORY.md Phase 9 entry

Structure (added during A.21, extended in B.10 + C.10):

```markdown
## Phase 9 — Quick Wins (2026-04-24 → …)

**Bundle**: three independent features addressing papercuts across privacy, platform, and timeline-UX surfaces.

### Feature 1: Tracking Schedule (privacy-hardening negative gate)
- `TrackingScheduleConfig` with multiple-window-per-day support, overnight/DST semantics, chrono-tz timezone resolution.
- 13 data-producing pipelines gated: capture, monitor guard, window-switch, analysis, focus, coaching, process/input/clipboard/file-access events, upload flush, cross-device sync, audio command.
- `SmartCaptureTrigger` hoisted to time-agnostic; scheduler owns schedule knowledge.
- `should_run_now` overnight wrap-midnight bug fixed (latent bug CONS-C05).
- Tray tooltip propagation via ADR-016 ConfigManager::subscribe.
- 47 new tests.

### Feature 2: Linux systemd Autostart IPC Wiring
- Exposed autostart.rs via Tauri IPC + REST `GET/PUT /api/autostart` + `POST /api/autostart/repair`.
- Behavioral fixes (CONS-C10): all 3 platforms return Err on non-zero exit, 5s timeout wrap.
- Wayland env detection in systemd unit template.
- `ONESHIM_AUTOSTART_STUB=1` env-var stub for Linux CI.
- `has_systemctl()` OnceLock memoization.
- 21 new tests.

### Feature 3: Timeline Bulk Tag (transactional + batch-remove)
- Storage-layer `add_tag_to_frames` + `remove_tag_from_frames` — single-transaction atomic ops.
- Handler refactor: silent-partial-200 → explicit-500-rollback (D15 behavior change).
- New `DELETE /api/frames/batch-tags` endpoint.
- Rename `tagged_count` → `affected_count` (D8-alt).
- `MAX_BATCH_SIZE=1000` cap + frontend "Remove tag" popover.
- 30 new tests.

### Follow-ups registered
- TimeWindow unified primitive refactor.
- `AutostartError` typed-enum upgrade.
- First-run onboarding autostart prompt.
- `es/ja/zh-CN` i18n for Phase 9 keys.
- User-facing tracking-schedule guide + KO companion.
- a11y ARIA hardening on multi-select.
- Snap refresh auto-repair.
- Workspace CLAUDE.md "41 → 42" wire-code drift fix.
```

### 6.4 Follow-up TODOs registration

All follow-ups tracked in an existing TODO doc (`project_next_tasks.md` per memory, if it exists; else `docs/follow-ups.md` — verify during A.21).

- **From PR-A**: TimeWindow unified primitive (D3); `tracking-schedule.md` user guide (D-guide); es/ja/zh-CN translations (D-i18n).
- **From PR-B**: `AutostartError` typed enum (D-errtype); first-run onboarding prompt (D21); Snap refresh auto-detection (CONS-M13); es/ja/zh-CN autostart translations.
- **From PR-C**: a11y ARIA multi-select (CONS-M04).
- **Cross-cutting**: workspace CLAUDE.md "41 wire codes → 42" drift fix (`reference_doc_drift`).

### 6.5 `http-interface-manifest.v1.json` + OpenAPI generator workflow

The manifest is HAND-MAINTAINED (no generator); the OpenAPI YAML snapshot is AUTO-GENERATED from the manifest via `./scripts/generate-http-openapi.sh`. Correct per-PR workflow:

1. **Hand-patch** `docs/contracts/http-interface-manifest.v1.json` with the new route entries + response schemas.
2. **Regenerate** `docs/contracts/oneshim-web.v1.openapi.yaml` via `./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml`.
3. **Verify** via `./scripts/verify-http-interface-manifest.sh` (manifest ↔ registered routes) and `./scripts/verify-http-openapi-sync.sh` (YAML ↔ regenerated-from-manifest — drift check).

**Do NOT hand-edit the YAML.** CI `check` job at `.github/workflows/ci.yml:192-199` regenerates the YAML to a temp file and diffs against the tracked copy — any drift fails the gate. **Do NOT run `./scripts/verify-integrity.sh`** for this purpose: that script is the supply-chain gate (cargo-audit / cargo-deny / cargo-vet / cargo-cyclonedx SBOM), not the contract-drift gate.

PR-A adds: `GET /api/tracking-schedule`, `PUT /api/tracking-schedule`, `GET /api/tracking-schedule/status`.
PR-B adds: `GET /api/autostart`, `PUT /api/autostart`, `POST /api/autostart/repair`.
PR-C adds: `DELETE /api/frames/batch-tags`; updates `BatchTagResponse` schema (rename `tagged_count` → `affected_count`).

### 6.6 Observability (new surfaces)

Per CONS-PI14 / CLAUDE.md logging convention (`warn!(err.code = %e.code(), "...")`), Phase 9's new surfaces must carry observability. Format bikeshed acceptable; the inventory:

**Tracing spans** (wrap the corresponding functions):
- `tracking_schedule_active(enter/exit)` — one span per gate evaluation at scheduler-loop cadence; fields: `cfg_enabled`, `window_count`, `active_result: bool`.
- `autostart_enable / autostart_disable / autostart_status` — one span per IPC invocation; fields: `mechanism`, `fallback_used`, `result: ok|err`, `err.code?` on error.
- `autostart_repair` — one span per invocation; fields: `throttled: bool`, `needs_repair_before: bool`.
- `bulk_tag_transaction(add|remove)` — one span per handler call; fields: `op: add|remove`, `batch_size`, `affected_count`, `duration_ms`.

**Counters** (Prometheus-compatible naming per industry convention per memory `feedback_industry_convention_check.md`):
- `oneshim_tracking_schedule_state{active}` — gauge: 1 if TS window active, 0 otherwise. Observed on every scheduler-tick evaluation.
- `oneshim_tracking_schedule_transition_total{direction}` — counter: increment per enter/exit edge.
- `oneshim_bulk_tag_operations_total{op, result}` — counter: `op ∈ {add, remove}` × `result ∈ {ok, err}`.
- `oneshim_autostart_attempt_total{result, mechanism}` — counter: `result ∈ {ok, err}` × `mechanism`.
- `oneshim_autostart_repair_total{result, throttled}` — counter.

**`err.code` on warn/error logs** (per CLAUDE.md convention): all new `warn!`/`error!` sites in PR-A/B/C include the structured `err.code` field so Loki/Grafana/OTel can group by wire code without regex-matching Display bodies. Example:
```rust
warn!(err.code = %e.code(), "autostart enable failed: {e}");
```

**Audit log entries** (fire via existing `AuditLogger` port):
- `TrackingScheduleTransition { prev: bool, now: bool, reason: "user" | "scheduled" }` — fired from A.18 on debounced enter/exit events.
- `AutostartStateChange { enabled: bool, mechanism: String, via: "ipc" | "rest" }` — fired from B.5 on successful enable/disable.
- `BulkTagMutation { op: "add" | "remove", affected_count: u32, selection_count: u32, had_error: bool }` — fired from C.4/C.6 on handler completion.

Implementation sites: `tracing::instrument` attribute on helper fns; `metrics::counter!` / `metrics::gauge!` calls threaded via existing `MetricsRegistry` Arc; audit fires via `AuditLogger::record()` port. No new crates or infra additions required — all observability infrastructure already exists.

---

## 7. Test strategy per task (TDD order)

### 7.1 Principles

1. **Red first**: for each feature commit, the test commit lands **before** the implementation commit (reversed ordering per superpowers:test-driven-development skill).
2. **Pure-fn tests preferred**: wherever a 2-arg `(cfg, now)` or similar signature is possible, use it (per U3 decision). Avoids mock-clock complexity.
3. **Integration tests use `serial_test`** when touching global state (FS, env vars, storage singletons) per memory `reference_serial_test_pattern.md`.
4. **Frontend uses Vitest + React Testing Library**; E2E uses Playwright at `frontend/e2e/*.spec.ts` (verified path per CONS-M11).
5. **Manual mocks only** (no mockall) per ADR-001 §5.

### 7.2 Test count breakdown

| PR | Unit (Rust) | Integration (Rust) | Unit (Frontend) | E2E (Playwright) | Total |
|---|---|---|---|---|---|
| PR-A | ~22 (A.2: 12 serde + validation; A.4: 12 helper + clock-irregularity; A.6: 3 migrated; A.10: 3 uploader; A.13: 5 IPC; A.15: 4 REST; A.18: 4 notifier — sum 43) | ~18 (A.8: per-variant suppression + sanity) | ~7 (A.19: TrackingScheduleSettings) | 0 new (reuses settings spec) | **~47** |
| PR-B | ~12 (autostart module + macOS/Linux/Windows + commands + handler) | ~3 (REST roundtrip with serial_test) | ~6 (GeneralTab autostart toggle) | 0 new | **~21** |
| PR-C | ~9 (storage tx ops) + ~7 (handler) = ~16 | ~4 (DELETE + roundtrip) | ~8 (AllFrames popover + TimelineLayout — including D15 error-path extensions per CONS-PI09) | ~2 (timeline-actions extension + forced-error E2E) | **~30** |
| **Total** | **~50** | **~25** | **~21** | **~2** | **~98** |

Note: this table is the SOURCE OF TRUTH for PR-level test counts. All mentions in §3.3 A.21 (PR-A), §3.5 (PR-A gate prose), §6.2 (STATUS bump), §7.2 (this table) must stay consistent. Loop 2c/2d baseline: PR-A = 47, PR-B = 21, PR-C = 30, total 98. Prompt's ~40 estimate predates D13 scope expansion (13 scheduler-loop integration tests — now 18 per-variant per CONS-PI05).

### 7.3 Fixture & mock patterns

- **Time injection**: 2-arg `(cfg, now: DateTime<Local>)` pure-fn per U3 Option B. No mock-clock primitive in workspace.
- **Clock-relative dates in tests** per memory `feedback_time_relative_test_dates.md`: where tests hardcode dates (e.g., DST fixtures), use explicit `chrono-tz` TZ constructors, never runner-TZ-dependent formats.
- **Autostart stub**: `ONESHIM_AUTOSTART_STUB=1` env-var + thread-local `test_observer::{record, last_command}` + `serial_test::serial` wrapper.
- **Storage**: `SqliteStorage::new_in_memory()` where available for speed; `serial_test` when the test touches the global config manager or real FS.
- **Config manager**: construct `ConfigManager::new_with_path(tempdir)` for tests that need write-through-disk semantics; otherwise construct `Arc::new(AppConfig::default())` directly.

### 7.4 Anti-flakiness patterns

- **Time-based**: never use real `SystemTime::now()` in unit tests; always inject.
- **DST fixtures**: use `chrono_tz::US::Eastern` with explicit `NaiveDate` + `and_hms_opt` to build ambiguous instants.
- **Integration tests w/ background tasks**: use `tokio::time::pause/advance` inside `#[tokio::test]` where scheduler-tick behavior must be tested deterministically.
- **Frontend**: Vitest with deterministic MSW (Mock Service Worker) responses — no network.

### 7.5 Benchmarking (non-blocking)

- **PR-C**: 1000-row batch perf benchmark (per R3 verification residual #1). Run once during C.2 impl; record in PR description. Acceptance target: `MAX_BATCH_SIZE=1000` batch completes < 50ms on a dev Mac.
- **PR-A**: `tracking_schedule_active` benchmark if called at >10Hz (not currently; deferred).

---

## 8. CI + lefthook + integrity-gates impact

### 8.1 New CI jobs / required checks

- **None new**. All Phase 9 tests run in existing jobs:
  - `ci.yml` Rust `test` job (at `.github/workflows/ci.yml:314`) → existing; **`runs-on: ubuntu-latest` only** — see §8.7 for platform coverage gap.
  - `ci.yml` frontend test step → existing.
  - `ci.yml` `check` job (at `.github/workflows/ci.yml:192-199`) → existing; enforces OpenAPI + manifest consistency via `verify-http-interface-manifest.sh` + `verify-http-openapi-sync.sh`.
  - `integrity-gates.yml` → existing supply-chain gate (`verify-integrity.sh`: cargo-audit, cargo-deny, cargo-vet, cargo-cyclonedx SBOM). **Not the contract-drift gate** — do not conflate.
- **New env-var required in Linux test job** (PR-B only): add `ONESHIM_AUTOSTART_STUB: "1"` to the `env:` block of the Rust test **STEP** (not job-level) in `.github/workflows/ci.yml` around line 314. Step-level scoping prevents leaking the env-var to unrelated build steps. Documented as part of B.3.

### 8.2 Contract-drift CI gate (`.github/workflows/ci.yml check` job)

- Contract-drift enforcement is a **check-job** step (not `integrity-gates.yml` — that's a separate supply-chain gate using `./scripts/verify-integrity.sh`). The `check` job at `.github/workflows/ci.yml:192-199` runs `./scripts/verify-http-interface-manifest.sh` and `./scripts/verify-http-openapi-sync.sh` on every push.
- No workflow file changes required — each PR patches the manifest (hand) + regenerates the YAML (`./scripts/generate-http-openapi.sh`).
- PR-A lines to patch:
  - `docs/contracts/http-interface-manifest.v1.json` — 3 new route entries, 2 schema definitions (hand).
  - `docs/contracts/oneshim-web.v1.openapi.yaml` — regenerated (no hand edit).
- PR-B lines to patch:
  - `docs/contracts/http-interface-manifest.v1.json` — 3 new route entries, 1 schema definition (hand).
  - `docs/contracts/oneshim-web.v1.openapi.yaml` — regenerated.
- PR-C lines to patch:
  - `docs/contracts/http-interface-manifest.v1.json` — 1 new route (DELETE) + 1 schema rename (`tagged_count` → `affected_count`) (hand).
  - `docs/contracts/oneshim-web.v1.openapi.yaml` — regenerated.

### 8.3 Lefthook / pre-commit impact

- No lefthook file changes needed.
- Cold clippy cost per memory `feedback_lefthook_clippy_cost.md`: ~16min. **Commit bundling** strategy per PR-A helps — A.3 + A.5 + A.9 are bundled multi-change commits specifically to share one cold-clippy run rather than three.
- Per memory `feedback_noninteractive_overhead.md`, non-interactive agents skip lefthook — only human pushes pay.
- Per memory `reference_ci_tauri_externalbin_stub.md`, fresh worktrees need:
  - `src-tauri/oneshim-sandbox-worker-<triple>` stub file (touch).
  - `crates/oneshim-web/frontend/dist/index.html` stub file (echo `<html></html>`).
- Per memory `feedback_lefthook_pre_push_stdin.md`, pre-push hooks use `git @{u}` (already fine).

### 8.4 Commit-message hygiene

Per memory `reference_commit_hygiene_false_positives.md`, `verify-commit-message-hygiene.sh` flags substrings like `secret_store`, `password`. PR-A/B/C commits don't use these words; low risk.

### 8.5 Release process

Per CLAUDE.md: **no manual `git tag`**. Post-merge, the next `./scripts/release.sh 0.4.2-rc.1` bundles Phase 9. `git-cliff` auto-generates CHANGELOG from `feat:` / `refactor:` prefixes (per memory `feedback_squash_merge_cliff_skip.md`).

### 8.6 Lefthook cold-cost budget

| PR | Commits | Est. cold-clippy runs | Warm-cache runs | Total lefthook cost |
|---|---|---|---|---|
| PR-A | ~22 | 3 (A.3/A.5/A.9 bundled) | ~19 | ~3×16min + 19×1min = ~67min |
| PR-B | ~11 | 2 (B.3/B.5 touch similar surfaces) | ~9 | ~41min |
| PR-C | ~10 | 1-2 (C.4/C.6 similar surfaces) | ~8 | ~32min |

Total lefthook time: ~2.5 hours engineer-clock, spread across weeks.

### 8.7 Platform coverage gap (PR-B)

PR-B introduces `#[cfg(target_os = "windows")]` tests in B.1 (`windows_enable_returns_err_on_regsetvalueexw_nonzero`) and `#[cfg(target_os = "macos")]` branches in B.4 (`get_status_returns_mechanism_per_platform`). Per `.github/workflows/ci.yml:314`, the `test` job runs on `ubuntu-latest` **only**; the `build` job at line 424 uses `${{ matrix.os }}` (4-platform matrix) but invokes `cargo build`, not `cargo test`.

**Consequence**: these platform-branched tests run only **locally**, never in merge gates. Per U-P2 decision A (locked), this is an accepted pre-existing CI limitation — scope discipline keeps CI matrix expansion out of Phase 9.

**Developer responsibility before merging PR-B**:
1. Run `cargo test -p oneshim-app --lib autostart` on a **macOS host** — all branch tests pass.
2. Run the same command on a **Windows host** — all branch tests pass.
3. Attach local-run evidence (command output + platform info) to the PR description.

**Follow-up registered** (`project_next_tasks.md`): expand the `ci.yml` `test` job to a 4-platform matrix (`ubuntu-latest`, `macos-latest`, `windows-latest`, `ubuntu-24.04-arm`). Adds ~30 min per-push CI cost but closes the coverage gap structurally. Tracked as its own infra PR — separate concern from Phase 9.

**Demotion rationale** (from original R3 Critical call): R3 initially ranked this as Critical. Synthesis demoted to Important because (1) the gap is pre-existing and structurally independent of Phase 9; (2) §4.8 already notes Linux as primary target and macOS/Windows fall-through is testable locally; (3) Option A (document + defer) is consistent with scope-discipline guidance.

---

## 9. Timeline + effort summary

### 9.1 Wall-clock estimate

Assuming single engineer at 6-7 effective hours/day (accounting for review iterations per memory `feedback_holistic_pre_merge_review.md`):

- **PR-A**: 64h → **~10 wall-clock days** (accounting for 2-3 review cycles @ 1 day each).
- **PR-B**: 30h → **~5 wall-clock days** (1-2 review cycles).
- **PR-C**: 22h → **~4 wall-clock days** (1-2 review cycles).

**Total headline**: ~19 wall-clock days serial OR ~12 days if PR-B and PR-C work parallel to PR-A's review cycles.

**Realistic ceiling** (per U-P3 locked Option A — commentary added for stakeholder expectation management, engineering plan unchanged): 26-30 wall-clock days serial accounting for 30-50% review-cycle tax on security-sensitive PRs (memory `feedback_3loop_yields_real_catches.md`). PR-A's pipeline-gating qualifies as security-sensitive (GDPR transparency guarantees weakened on regression). The 19-day figure is the optimistic floor assuming single deep-review passes; the ceiling accounts for expected Loop 2c/2d-style iteration cycles on the larger PR-A surface. Published estimate intentionally carries both numbers so stakeholders can plan around the ceiling while engineering targets the floor.

### 9.2 Dependency graph

```
PR-A (tracking-schedule)  ────────────────────→ merge
   │
   │  (no technical dep — but recommend land-first for risk burn-down)
   ▼
PR-B (autostart)  ───────────────────────→ merge
   │
   │  (independent)
   ▼
PR-C (bulk-tag)  ────────────────────────→ merge
```

Strictly, all three PRs are independent — the only shared artifact is `docs/PHASE-HISTORY.md`. Reviewer capacity determines actual ordering.

### 9.3 Parallelization opportunities

- While PR-A is in review, PR-B storage-layer fixes (`autostart.rs` behavioral fixes, B.1-B.3) can be prepared in a separate branch.
- While PR-B is in review, PR-C's storage transactional ops (C.1-C.2) can be prepared.

This assumes memory `feedback_subagent_driven_catches_stale_plans.md`: sequential PRs increase drift risk; pre-preparing makes them faster to kick off but each must still re-verify anchors pre-impl (memory `feedback_cross_worktree_line_drift.md`).

### 9.4 Commit-count summary

| PR | Test commits | Impl/refactor commits | Docs commits | Total commits |
|---|---|---|---|---|
| PR-A | 9 (A.2/A.4/A.6/A.8/A.10/A.13/A.15/A.18/A.19) | 11 (A.1/A.3/A.5/A.7/A.9/A.11/A.12/A.14/A.16/A.17/A.20) | 2 (A.21/A.22) | 22 |
| PR-B | 6 (B.1/B.2/B.4/B.6/B.8/B.11) | 5 (B.3a/B.3b/B.5/B.7/B.9) | 1 (B.10) | ~12 |
| PR-C | 4 (C.1/C.3/C.5/C.7/C.9) | 4 (C.2/C.4/C.6/C.8) | 1 (C.10) | ~10 |
| **Total** | **~20** | **~20** | **~4** | **~44** |

Squash-merge policy: each PR squashes to 1 commit per PR at merge (so the final main branch gets 3 Phase 9 squash commits). The squash message per PR follows `feat(phase9-<area>): <summary>\n\n<bullet list of sub-changes>` — `git-cliff` visibility preserved.

### 9.5 Milestone gates

- **M1 (end of PR-A)**: Tracking Schedule lands; tray + scheduler gating verified in production; follow-ups registered. ~10 wall-clock days.
- **M2 (end of PR-B)**: Autostart IPC lives in all 3 platforms; CI stub proven green-on-failure. ~14-16 wall-clock days.
- **M3 (end of PR-C)**: Bulk-tag transactional live; D15 behavior change absorbed by frontend. ~18-20 wall-clock days.

### 9.6 Contingency budget

- Reviewer finds post-first-draft issue requiring spec revision: +2 days per issue (absorbed in review-cycle estimate).
- Contract-drift regression surprise: +1 day per PR if `./scripts/verify-http-interface-manifest.sh` or `./scripts/verify-http-openapi-sync.sh` surfaces unexpected manifest or OpenAPI snapshot drift.
- DST test flake on CI (PR-A): +1-2 days if US/Eastern fixtures misbehave on CI-runner TZ.

---

## 10. Open questions for Loop 2c review

Only genuinely-open items — everything else was locked in Loop 1d. Target ≤5.

1. **Q-plan-1: Commit bundling aggressiveness** — the plan bundles A.3/A.5/A.9 to amortize lefthook cold-clippy cost. Reviewers may prefer smaller-commits-for-review-clarity over shorter-wall-clock. Alternative: expand to ~28 commits in PR-A instead of ~22. Recommend: keep bundled; reviewer can ask for split during review if needed.

2. **Q-plan-2: Sub-module extraction of `autostart.rs` (B.3)** — **RESOLVED by U-P1 Option B (locked)**: split B.3 into B.3a (pure refactor per ADR-003) + B.3b (behavioral fix + async signature + stub + OnceLock). Two cold-clippy cycles (~32 min extra) in exchange for ADR-003 compliance + clean bisect boundary. See B.3a/B.3b sections above.

3. **Q-plan-3: `NotificationConfig.tracking_schedule_enabled` sibling-naming drift** — spec §3.11 claims `idle_enabled` / `long_session_enabled` as sibling pattern, but the **actual** neighboring fields are `idle_notification` / `long_session_notification` (verified during plan drafting via `grep NotificationConfig` in `config/sections/storage.rs`). The spec's recommendation for `tracking_schedule_enabled` still makes sense (newer enterprise naming convention), but it's inconsistent with neighbors. Alternative A: use `tracking_schedule_notification` to match existing neighbors. Alternative B: follow spec and accept the inconsistency (neighbors stay legacy-named). Recommend: **Alternative B** per spec — inconsistency is acceptable because `NotificationConfig` section provides context and renaming legacy fields is outside Phase 9 scope. Flag explicitly for Loop 2c reviewer to confirm.

4. **Q-plan-4: Frontend Tauri/WebView environment detection for IPC commands** — PR-A's IPC commands (A.14) and PR-B's (B.5) are registered in `tauri::generate_handler!`. The web-dashboard frontend uses REST exclusively; Tauri IPC is only surfaced when the user is in the native Tauri window. The settings UI lives in `crates/oneshim-web/frontend/` which is served both ways. Per memory guard (`Overlay Frontend Patterns` in CLAUDE.md), frontend should prefer `fetch()` REST calls over Tauri IPC. Current plan has frontend using REST (client.ts calls) for all 3 features. Reviewer: confirm no Tauri-native UI separately consumes these IPC commands?

5. **Q-plan-5: PR-A vs PR-B landing order trade-off** — spec's risk analysis recommends PR-A first (largest surface, burn down risk). Alternative: land PR-B first because it's self-contained (less risk if it regresses) and gives the team a fast Phase 9 win to build confidence. Trade-off: PR-B-first delays the higher-value Tracking Schedule shipping. Recommend: keep A → B → C unless reviewer feedback prefers otherwise.

Deferred questions (not for Loop 2c; tracked for future phases):
- What does the "first-run onboarding prompt" look like architecturally? (deferred to onboarding PR per D21)
- When do we retire the `Result<_, String>` error type from `autostart.rs`? (next major refactor per D-errtype)
- Should `MAX_BATCH_SIZE=1000` apply to `fetchFrames` backend cap too? (deferred per spec §5.10 — separate concern)

---

## 11. References

### 11.1 Spec sections

- **Feature 1 (Tracking Schedule)**: spec §3.1-3.14
  - §3.4 Composition rule + consent top-authority
  - §3.4a `should_run_now` overnight fix (D14)
  - §3.7 DST semantics corrected (CONS-C04)
  - §3.7a Clock irregularities
  - §3.8 16-row pipeline gate table (D13)
  - §3.8a `SmartCaptureTrigger` hoist (D17)
  - §3.9 Upload-defer FIFO-exit (CONS-C03) + `with_suppression_predicate`
  - §3.11a Tray propagation via ConfigManager::subscribe (D-prop, U6)
  - §3.12 IPC/REST surface
- **Feature 2 (Autostart)**: spec §4.1-4.10b
  - §4.3 `has_systemctl` OnceLock + cross-platform `is_enabled()` caveat
  - §4.4 REST endpoint (D3a)
  - §4.6 Wayland env detection (D5)
  - §4.7 Binary-path stability + Repair Autostart button (D6)
  - §4.10 Error mapping + behavioral fix (CONS-C10)
  - §4.10a Defer typed error (D-errtype, U8)
  - §4.10b Defer first-run prompt (D21, U9)
- **Feature 3 (Bulk Tag)**: spec §5.1-5.10
  - §5.3 Current state table
  - §5.4 Storage transactional ops
  - §5.5 Handler refactor (D9)
  - §5.6 DELETE endpoint + rename (D8, D8-alt)
  - §5.8 Remove-tag popover (D20, U10)
  - §5.9 Selection reset + MAX_BATCH_SIZE (D11, D19)
- **Cross-cutting**: spec §6.1-6.5 (tests, observability, wire codes, i18n, CI)
- **Decisions**: spec §7 Decisions log (D1-D22 + D-prop/D-errtype/D-i18n/D-guide)

### 11.2 ADRs

- **ADR-001** (hexagonal + async trait + DI patterns) — `docs/architecture/ADR-001-rust-client-architecture-patterns.md`
- **ADR-003** (directory module pattern) — applicable to `commands/autostart/`, `handlers/autostart/` if grown
- **ADR-004** (Tauri v2 IPC conventions)
- **ADR-008** (network resilience patterns — relevant to `BatchUploader` surface)
- **ADR-016** (config-change-bus) — `docs/architecture/ADR-016-config-change-bus.md` — used in A.17 tray propagation
- **ADR-019** (error-code infrastructure) — wire-code catalog reference for §6.3 / §4.10

### 11.3 Spec review artifacts

- `docs/reviews/2026-04-23-phase9-quick-wins-spec-review-1-architecture.md`
- `docs/reviews/2026-04-23-phase9-quick-wins-spec-review-2-product-privacy.md`
- `docs/reviews/2026-04-23-phase9-quick-wins-spec-review-3-platform-test-risk.md`
- `docs/reviews/2026-04-23-phase9-spec-review-synthesis.md` (Loop 1b — 46 consolidated findings)
- `docs/reviews/2026-04-23-phase9-spec-review-1-verify.md` (Loop 1e PASS)
- `docs/reviews/2026-04-23-phase9-spec-review-2-verify.md` (Loop 1e PASS)
- `docs/reviews/2026-04-23-phase9-spec-review-3-verify.md` (Loop 1e PASS)

### 11.4 CONS-* findings cross-reference

Critical-gate items resolved before this plan:
- CONS-C01 → D13 (analysis-loop scope expansion).
- CONS-C02 → D13 + §3.8 16-row enumeration.
- CONS-C03 → §3.9 upstream-gated FIFO semantics.
- CONS-C04 → §3.7 DST corrected (fires twice on fall-back).
- CONS-C05 → D14 (should_run_now overnight fix, U2 Option C).
- CONS-C06 → U3 Option B (2-arg pure fn).
- CONS-C07 → U4 Option B (ONESHIM_AUTOSTART_STUB).
- CONS-C08 → §3.9 `with_health_flag` precedent.
- CONS-C09 → D15 (200→500 behavior change).
- CONS-C10 → §4.10 behavioral fix on 3 platforms.
- CONS-C11 → test counts corrected (9 autostart, 13 trigger).
- CONS-C12 → 42 wire codes (CLAUDE.md drift noted).

Important-gate items (all 16 resolved — see spec §7 + synthesis §2).

### 11.5 Memory references

Relevant memory docs steering this plan:
- `feedback_holistic_pre_merge_review.md` — N-loop deep reviews.
- `feedback_squash_merge_cliff_skip.md` — `feat:` / `refactor:` prefix choice.
- `feedback_subagent_driven_catches_stale_plans.md` — 2-stage review catches drift.
- `feedback_3loop_yields_real_catches.md` — 3-loop ratchet for security-sensitive PRs.
- `feedback_route_refactor_e2e_completeness.md` — in-PR E2E migration.
- `feedback_test_gap_audit_dual_file.md` — check both `tests.rs` + `port_contract_tests.rs` when auditing.
- `feedback_cross_consumer_audit.md` — grep ALL consumers before config semantics change.
- `feedback_cross_platform_cargo_check.md` — macOS local check skips `#[cfg(target_os = "linux|windows")]`; audit gated files.
- `feedback_cross_worktree_line_drift.md` — re-verify anchors in target worktree pre-impl.
- `feedback_time_relative_test_dates.md` — relative dates in recent-N-days tests.
- `feedback_lefthook_clippy_cost.md` — ~16 min cold; bundle commits.
- `feedback_noninteractive_overhead.md` — non-interactive agents skip lefthook.
- `reference_serial_test_pattern.md` — module-global-state tests use `serial_test`.
- `reference_ci_tauri_externalbin_stub.md` — fresh worktree stub files.
- `reference_parent_submodule_bump.md` — parent oneshim submodule pointer bump post-PR-merge.
- `reference_clippy_195_patterns.md` — Rust 1.95 clippy lints to avoid.

### 11.6 CI workflow files

- `.github/workflows/ci.yml` — Rust + frontend tests; `ubuntu-latest` runner (PR-B requires `ONESHIM_AUTOSTART_STUB=1` env).
- `.github/workflows/integrity-gates.yml` — supply-chain gate (cargo-audit/deny/vet/cyclonedx SBOM). Contract drift uses the 3-script pair in `.github/workflows/ci.yml` lines 192-199 (`verify-http-interface-manifest.sh` + `verify-http-openapi-sync.sh` + `generate-http-openapi.sh`).
- `.github/workflows/build-smoke.yml` — cross-platform smoke.

### 11.7 Release workflow

- `./scripts/release.sh` (RC) — per CLAUDE.md release process.
- `./scripts/promote-stable.sh` (stable).
- `git-cliff` auto-generates CHANGELOG from conventional commits.

---

## Plan meta — final checks

- **Every `file:line` citation**: verified against worktree tip `5618558c` during plan drafting. Where spec cites a line (e.g., `autostart.rs:389-401`), plan re-asserts the citation rather than restating. If a later commit edits the cited line, re-verify pre-commit.
- **Every commit task has a paired test task (TDD)**: yes — see §3.3, §4.3, §5.3 where each impl commit cites its preceding test commit. Exceptions documented per §3.3 / §7.1 (A.1 dep-bump, A.5 two-gates-in-one, A.12 micro-test, B.3a pure-refactor).
- **All four test-count mentions consistent with §7.2 table**: A.21 = 47; §3.5 = 47; §6.2 = 47/21/30; §7.2 table = 47/21/30/98. Pre-push verify: `grep -n "PR-A adds.*~.*new tests" docs/reviews/2026-04-23-phase9-quick-wins-plan.md` — all mentions cite 47.
- **Composition-root citations verified**: `SmartCaptureTrigger::with_schedule` at `src-tauri/src/agent_runtime_support.rs:251`; `BatchUploader::new` at `:405`. Both edits bundled to share one cold-clippy cycle.
- **Contract-drift workflow correct**: `verify-integrity.sh` is the supply-chain gate, NOT the contract-drift gate; contract drift uses `verify-http-interface-manifest.sh` + `verify-http-openapi-sync.sh` with YAML auto-regenerated via `generate-http-openapi.sh`.
- **Effort estimates realistic**: includes lefthook cold-clippy cost + review iteration absorption (~30% buffer per memory). Realistic ceiling of 26-30 days documented in §9.1 per U-P3 Option A.
- **No "TBD" placeholders**: if a reviewer asks, cite the spec section. All 22 Decisions landed.
- **Korean + English**: plan body is English; Korean user-facing strings (tracking-schedule, notifications, autostart) quoted in EN + KO side-by-side per spec §6.4 / §4.9.

---

_End of plan._
