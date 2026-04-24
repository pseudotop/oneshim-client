# Phase 9 Spec Review 3 — Cross-platform + Test + Risk

**Reviewer scope**: Linux/macOS/Windows specifics, test harness, CI, binary/perf, failure modes, rollout risk, observability, sandbox, scheduler integration, docs
**Date**: 2026-04-23
**Spec reviewed**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` @ 1247 lines / 5618558c
**Worktree tip**: 5618558c
**Out of scope** (owned by R1/R2): ADR drift, Hexagonal boundary violations, DDD placement, product naming, GDPR grounding, regulatory framing.

## Summary

- Critical: 2
- Important: 7
- Minor: 9

## Critical findings

### C1. Test strategy relies on mock-clock infrastructure that does not exist

**Where**: spec §6.1 "Feature 1 — Tracking Schedule — Integration: use a mock clock" (lines 971-973) and the implicit mock-clock assumption in the "post-window drain" assertion (line 972).

**Problem**: grep of the entire workspace finds **zero** mock-clock, fake-clock, or time-injection harness. Every time source in the scheduler (`src-tauri/src/scheduler/mod.rs:554`, `src-tauri/src/scheduler/loops/system.rs:298,342`) calls `chrono::Local::now()` directly. The spec's proposed helper `tracking_schedule_active(&AppConfig)` (lines 260-269) also internally calls `resolve_now(&schedule.timezone)` with no injection point. There is no way today to test "entering a window causes no new events for the duration and window exit resumes them" without either:

1. Adding a workspace-wide clock-injection primitive (non-trivial; out of scope for "quick wins"), or
2. Running tests in real wall-clock time (slow, flaky, and dependent on the test runner's local timezone — not acceptable on CI).

The spec claims "integration tests in `src-tauri/tests/`" will cover the transitions but does not explain how. Without a plan, the integration-test coverage promise is empty. This is distinct from the unit-test path for `TrackingWindow::window_is_active(now: DateTime<Local>)` which is pure and testable (that part is fine).

**Why Critical**: the "entering a window silently fails to suppress capture" failure mode is exactly the one the feature exists to prevent. Shipping without integration tests of the transition means Day-1 regressions are possible and undetectable in CI.

**Required action for plan**: the spec or plan must pick one of:
- (a) `resolve_now()` accepts an injected `Fn() -> DateTime<Local>` with a default of `chrono::Local::now`; test harness passes a `MockClock`.
- (b) `tracking_schedule_active(config, now)` takes `now: DateTime<Local>` as a second parameter; scheduler callsite passes `chrono::Local::now()`; tests pass fixed values. This is the simpler shape.
- (c) Document that integration tests rely on actual wall-clock time and use a test-only `TrackingScheduleConfig` with a narrow window around test runtime — acknowledge flakiness risk and add retry to CI.

Option (b) is cheapest and aligns with the existing `should_run_now(config: &AppConfig)` shape. Recommend adopting it and updating §6.1 to reflect pure-function testability.

### C2. REST contract "roundtrip test on each platform" in CI will fail on ubuntu-latest for Linux autostart

**Where**: spec §6.1 "Feature 2 — Autostart — REST contract: tests/ — GET then PUT then GET cycle with `{enabled: true}` and `{enabled: false}` on each platform" (lines 981-982).

**Problem**: `.github/workflows/ci.yml` lines 314-381 runs the `test` job on `ubuntu-latest` with `cargo test --workspace` (and `--features server` + `--features grpc`). `ubuntu-latest` GitHub-hosted runners have `systemctl` installed but **do not have a user D-Bus session**, so `systemctl --user daemon-reload` and `systemctl --user enable oneshim.service` both fail with `Failed to connect to bus: No medium found` or `Failed to start transient service`. The autostart module currently at `src-tauri/src/autostart.rs:389-401` calls these commands unconditionally when `has_systemctl() == true`.

A naive GET→PUT(`{enabled:true}`)→GET roundtrip test on CI will:
1. See `has_systemctl() == true` (the binary is installed on ubuntu-latest).
2. Write `~/.config/systemd/user/oneshim.service` (fine).
3. Fail on `systemctl --user enable`.
4. Either return 500 (test fails) or silently succeed with a logged warning (current code at line 398-401 only `warn!`s on non-zero exit, doesn't return error) — but the file is written, so the second GET returns `{enabled: true, mechanism: "systemd"}` with no actual systemd registration.

Second concern: the CI test job does NOT have a Linux-specific matrix variant that also tests frontend e2e with the Rust backend. The Linux test pass is a code-compile-and-run pass on Linux, not a platform-integration pass. There is no Linux-native equivalent of `macos-windowserver-gui-smoke.yml`.

**Why Critical**: the spec's test strategy claims per-platform REST-roundtrip coverage, but the CI environment cannot provide it for Linux. Shipping with a broken Linux path that appears green in CI is the worst outcome — users get enabled-but-not-enabled autostart. Windows may also fail depending on whether the Windows runner has HKCU registry writes (likely does, since hot-build tests run there), but on Linux the evidence is clear.

**Required action for plan**:
1. The plan must explicitly carve out Linux autostart integration tests to either (a) skip on CI with `#[ignore]`-gated smoke-tested paths only, (b) stub `has_systemctl()` and the Command spawn in tests with an env-var escape hatch, or (c) add a dedicated Linux CI matrix with `systemd-run --user` session start as a setup step.
2. Add a test that specifically asserts behavior when `systemctl --user enable` fails — verify the UI does NOT lie to the user. Today `enable()` on Linux at `autostart.rs:398-401` swallows a non-zero status with `warn!`, then returns `Ok(())`. That is a latent bug the Phase 9 IPC wiring will expose to users. The spec should call it out and the plan should fix it (return Err when systemctl non-zero-exits, at least behind a feature flag for CI).

## Important findings

### I1. `is_enabled()` file-existence check is a cross-platform lie detector (CI-hidden)

**Where**: spec §4.7 "Stale unit files fail at boot with `status=203/EXEC`" (line 571) and §4.3 Tauri command returning `AutostartStatus.enabled` (lines 459-462).

**Problem**: spec acknowledges Linux `is_enabled()` returns true based on file presence (line 573-574) but treats this as acceptable. The same pattern applies on **all three platforms**:
- macOS `is_enabled()` = `plist_path().exists()` (`autostart.rs:161`) — does NOT verify `launchctl` has actually loaded the plist. A user can remove a plist via `launchctl unload` without deleting the file (rare) or, more commonly, the plist can exist but be failing to load due to a moved binary (a `launchctl list | grep com.oneshim.agent` would reveal `status: 78`).
- Linux (spec §4.7 already acknowledges).
- Windows `is_enabled()` = `RegQueryValueExW` existence check (`autostart.rs:262-294`) — does not verify the target path is valid or that the executable exists.

This is not purely a Phase 9 bug (pre-existing), but Phase 9 **surfaces it to the user via IPC**. A user whose autostart silently broke (moved `.AppImage`, revoked launchctl permissions, corrupted registry) will see `enabled: true` in Settings and a broken boot experience.

**Why Important**: the spec's §4.7 decision to document-not-fix is reasonable for Phase 9 scope, but the spec should surface the same quirk for macOS and Windows. The "Repair Autostart" button (§4.7 final paragraph) should apply to all three platforms, not just Linux.

**Recommended action**: update §4.3 and §4.7 to note this is a cross-platform quirk and the documentation-only disposition is per-platform.

### I2. `has_systemctl()` caching concern (Q5) is downplayed — should be per-process, not per-call

**Where**: spec §Q5 (line 1161) and §4.3 "this is a private function today; expose it as `pub(crate)`".

**Problem**: `has_systemctl()` at `autostart.rs:365-371` spawns `systemctl --version` every call. On container/WSL environments this is slower (spec acknowledges), but on any Linux, spawning a process for a yes/no check is ~ms even when cached. Spec §Q5 recommends "Consider caching" without deciding.

Additionally: the spec proposes calling `has_systemctl()` from `get_autostart_status` IPC command (§4.3). The Settings page likely polls this on mount + after every toggle. If the frontend also uses a ReactQuery-style stale-while-revalidate pattern, this could trigger `systemctl --version` on every Settings page visit.

**Why Important**: spawn-per-request is a latent DoS vector for the IPC layer. Not critical (local IPC is trust-boundary-inside), but it's free to fix by caching once on first access via `std::sync::OnceLock<bool>`.

**Recommended action**: the plan should specify `static HAS_SYSTEMCTL: OnceLock<bool> = OnceLock::new();` with one-time init. Drop the ambiguity in Q5.

### I3. Suspend/resume and clock-skew failure modes missing for Tracking Schedule

**Where**: spec §3.7 "DST" (lines 232-234) covers DST but no other clock irregularities.

**Problem**: Laptop users regularly suspend/resume (close lid). The scheduler loop in `src-tauri/src/scheduler/loops/monitor.rs` is a `tokio::time::sleep_until(...)` pattern; on suspend-resume, tokio may or may not compensate (depends on whether the runtime uses monotonic vs wall clock for its timer). If the machine was suspended **during** a tracking-schedule window:
- Wake-up after window end: next tick evaluates `tracking_schedule_active` with the new time — correctly inactive. No bug.
- Wake-up before window end: the window was effectively "still active" the whole time — correct.
- But: the notification "Tracking resumed" (spec §3.11, line 362) fires on transition, observed by the loop. If the loop was asleep during the entire window (suspend spans 11:55–13:05), the notifications are missed entirely. The user thinks tracking never paused.

Clock-skew (user changes system clock backward mid-window): the wall-clock jumps backward, re-entering the window. Not a correctness bug (gate re-evaluates), but the notifications would fire twice ("Tracking paused" toast again).

**Why Important**: these are real failure modes and the user-visible indicator is the GDPR transparency lever (spec §3.11). Silent de-sync is worse than no indicator.

**Recommended action**: the spec should document expected behavior:
- Suspend crossing a window boundary: accept missed notifications; the gate was correct throughout.
- Backward clock-jump re-entering a window: accept duplicate notifications, debounce via a `last_notification_at: Instant` cooldown in the scheduler loop.
- Forward clock-jump skipping a window end: accept "stuck in suppression" until the clock is sane again — this is user-self-inflicted.

### I4. Scheduler-loop integration vague — which of the 16 loops evaluates `tracking_schedule_active`?

**Where**: spec §3.10 "each consumer evaluates `tracking_schedule_active(&cfg)` at decision time" (line 349) and §3.8 (line 248) which names 4 consumers (capture, monitor-loop gate, upload flush, analysis loop).

**Problem**: CLAUDE.md lists 16 scheduler loops. The spec enumerates 4 integration points but does not say anything about the other 12. Consumers that may also need to check the gate:
- `events.rs` loop — writes events to storage. Even if capture is suppressed upstream, some events (idle/resume, window-change from cached state) still flow.
- `sync.rs` loop — cross-device sync. Per GDPR purpose-limitation, syncing a device-to-device copy of pre-window data **during a window** may or may not be acceptable; needs a decision.
- `heartbeat` — telemetry ping. Not user-data but contains `user_id`/`device_id`; probably fine to continue (server needs to know the client is alive).
- `oauth_refresh` — continuing token refresh is fine (infrastructure-level).
- `coaching` — should coaching suggestions fire during a tracking window? The spec §Q6 declares this a non-issue but the answer is not obvious — "your productivity dipped during lunch, consider..." is exactly the kind of coaching that feels invasive during an opt-out window.

**Why Important**: without explicit per-loop disposition, the implementation will be ad-hoc and regression-prone. The "AND" composition in §3.8 line 92 is only complete if **every** data-producing loop checks it.

**Recommended action**: the spec should add a table enumerating each of the 16 loops and the tracking-schedule disposition (gated / ungated / domain-specific decision). The plan can then tick each off.

### I5. `BatchUploader::with_suppression_predicate` pattern is claimed to "mirror existing `with_capture_paused`" — false precedent

**Where**: spec §3.9 line 334 "This mirrors the existing `with_capture_paused(Arc<AtomicBool>)` pattern at `src-tauri/src/scheduler/mod.rs:429-430`".

**Problem**: grep `with_capture_paused` in `crates/oneshim-network/src/batch_uploader.rs` returns zero hits. The `with_capture_paused` at `src-tauri/src/scheduler/mod.rs:429-430` is on the `Scheduler` builder, not on `BatchUploader`. `BatchUploader` today has no suppression-predicate builder at all — the spec proposes one that has no precedent.

This is not a fatal issue: introducing a new builder method is fine. But the spec misrepresents it as "consistent with existing pattern" when it is actually a new pattern. Plan reviewers should not be misled.

**Why Important**: the spec cites precedent incorrectly in a decision the reviewer may rubber-stamp. If the precedent were accurate, the spec would deserve less scrutiny; because it isn't, the suppression-predicate design deserves explicit review (lifetime of the `Fn`, `Arc<dyn Fn() -> bool + Send + Sync>` closure-capturing `config_manager` clone, call-cost per flush tick).

**Recommended action**: the spec should reword §3.9 to acknowledge the predicate-injection is a new pattern, explain why a closure is preferred over a trait object `Arc<dyn SuppressionPredicate>`, and decide whether the call per flush tick (`batch_uploader.rs:199`) is acceptable (it is — flush ticks are seconds-scale, not microseconds).

### I6. Autostart `launchctl load`/`systemctl enable` error path: spec silently accepts partial success

**Where**: spec §4.10 table (lines 626-632) and actual code at `autostart.rs:389-401`.

**Problem**: as noted in C2, current `linux::enable()` swallows `systemctl --user enable` non-zero exit with a `warn!` and returns `Ok(())`. `macos::enable()` at `autostart.rs:137-141` returns `Err` from the `launchctl load` only on spawn failure — if `launchctl load` returns non-zero (e.g., duplicate-load, already-loaded), this is silently ignored (the `.output()` result is discarded via `?` only for spawn error).

Spec §4.10 says "`launchctl` / `systemctl` spawn fails → `internal.io` (500)" — but spawn is only one of three failure modes:
1. Binary not installed (spawn error — correctly mapped).
2. Binary installed, non-zero exit (silently ignored today).
3. Hang / exceeded timeout (no timeout set today; the `.output()` blocks indefinitely).

**Why Important**: the IPC wiring surfaces `is_enabled()` state to the UI. If `enable()` silently failed to register with systemd, the file exists, `is_enabled()` returns true, and the UI shows "autostart: on" while the actual boot behavior is broken. This is exactly the trust-eroding failure mode the Phase 9 autostart wiring is supposed to fix.

**Recommended action**: the spec (or plan) should commit to:
- Returning `Err` on non-zero-exit from `launchctl load` / `systemctl --user enable`.
- Adding a timeout to the Command invocation (e.g., `5s` via `Command::output` in a `tokio::time::timeout` wrapper).
- Documenting the behavioral change in the migration section (not user-observable before, observable after).

### I7. Test coverage for Feature 3 under-specifies concurrent-writer and FK behavior

**Where**: spec §6.1 "Feature 3 — Bulk Tag — Integration: handler-level — POST + DELETE roundtrip" (lines 985-988).

**Problem**: the spec's §5.4 adds `add_tag_to_frames` / `remove_tag_from_frames` as transactional operations. Missing test cases:
- **FK violation**: `frame_ids` contains an id that doesn't exist in `frames`. Today per-row `add_tag_to_frame` would fail at insert-time (there is a FK from `frame_tags.frame_id → frames.id` per schema at `v01_v08.rs:186-212`). With all-or-nothing rollback, the whole batch fails. The spec §5.6 claims "selecting a tag that isn't attached to some frames is a no-op for those frames" (line 924) — but this is confusing add vs remove, and for nonexistent `frame_id` it's not a no-op, it's a rollback.
- **Concurrent writer**: `SqliteStorage` uses `Arc<Mutex<Connection>>` per existing pattern. The new `conn.lock()` acquires the mutex. If another writer holds it (e.g., the event-writer loop inserting frames), the batch call blocks. At high event rates (tests using seed workload) this could time out.
- **SQLite statement cache behavior across transactions**: spec uses `prepare_cached` inside a scoped `tx` (line 708). SQLite cached statements are per-connection, so this is fine, but the test should verify the second batch call (reusing the cached statement) also rolls back cleanly on error.
- **Empty-input early-return**: spec line 694 returns `Ok(0)` without acquiring the lock — good, but there's no test for this optimization.

**Why Important**: "bulk op either succeeds fully or fails clean" is the whole point of Decision D9 (§5.7). Under-testing rollback paths undermines the main value of the change.

**Recommended action**: the plan should enumerate these cases in its test plan. Recommend adding: `add_tag_to_frames_rolls_back_on_fk_violation`, `remove_tag_from_frames_handles_missing_pairs_transactionally`, `batch_ops_compete_with_concurrent_writer` (use a second thread in the test), and `empty_input_is_lock_free`.

## Minor findings

### M1. chrono-tz binary-size claim is approximately correct

**Where**: spec §3.7 line 236 "+2.1MB binary size".

Verified: `chrono-tz` not in `Cargo.lock` (grep returns 0 matches). `chrono = { version = "0.4", features = ["serde"] }` is in the workspace root. Published `chrono-tz` with `default-tz` feature is ~2MB of embedded IANA data. Claim is accurate within 10%. No action; the number is a valid input to the Q1 decision.

Note: spec Q1 asks "acceptable to add" — answer from a test-strategy lens is yes, because restricting to `"Local"` only forces all tests and user configs to honor ambient local timezone, which is a flakiness generator for CI and regression tests.

### M2. Playwright E2E claim is unverified — frontend/tests/ dir absent

**Where**: spec §6.1 "Playwright E2E for 'select 3 frames → add tag → remove tag → verify tag count returns to zero'" (lines 988-989).

`crates/oneshim-web/frontend/tests/` does not exist. Existing Playwright tests live in `crates/oneshim-web/frontend/e2e/` and `e2e-tauri/`. Spec says "tests/" (ambiguous). The plan should specify the correct path — probably `e2e/timeline-actions.spec.ts` (exists) extended with bulk-tag cases.

### M3. "Repair Autostart" UI in §4.7 is functionally orphaned

**Where**: spec §4.7 line 574 "Surface this as a 'Repair Autostart' button ... Deferred to a follow-up if review prefers minimal surface."

The spec defers the decision but the Settings UI design (§4.9) does not mention it. If deferred, the user-journey for "my autostart is broken" has no recovery path other than "toggle off and on" — which the spec should note. If shipped, the UI design needs it. Recommend resolving in the plan phase.

### M4. Tray icon asset question (Q7) is test-surface-adjacent

**Where**: spec §Q7 line 1165.

If the plan picks "new icon asset", platform-specific icon files (`.icns` for macOS, `.ico` for Windows, `.png` variants for Linux) need to be committed. The `src-tauri/tauri.conf.json` bundle.icon array at lines 44-50 lists the current icons. Adding a new one is a tauri.conf.json + asset-folder change. This is out of spec's code-change enumeration. Recommend the plan either rejects the icon (reuse tooltip per §3.11 recommendation) or budgets the icon asset work.

### M5. Alpine / musl / OpenRC coverage absent

**Where**: spec §4.1 "enable `src-tauri/src/autostart.rs`" and §4.8 "keep XDG fallback" (lines 576-587).

Spec's §4.8 mentions "musl-based distros" as a reason to keep XDG fallback but does not enumerate which distros. Missing:
- Alpine Linux uses OpenRC, not systemd — `has_systemctl() == false`, falls through to XDG desktop file. The `.desktop` file depends on the desktop environment (GNOME, KDE, XFCE) — not all OpenRC setups run a DE. The assumption "XDG fallback always works on non-systemd" is false.
- Void Linux (runit) — same concern.
- Gentoo with OpenRC — same concern.

Not a Phase 9 blocker (these are < 1% of user base) but the spec's XDG-fallback framing over-promises. Recommend tightening §4.8 to "keep XDG fallback as best-effort; document that desktop-environment-less systemd-less distros may not honor it".

### M6. `serial_test` may be needed for autostart IPC tests

**Where**: spec §6.5 line 1096 "`serial_test` not needed for any Phase 9 change".

Disagree for autostart. The enable/disable/is_enabled functions all write to the same user-wide filesystem paths (`~/Library/LaunchAgents/*.plist`, `~/.config/systemd/user/*.service`, HKCU registry). Two test threads writing those paths simultaneously will race. Test memory `reference_serial_test_pattern.md` documents this pattern and specifically mentions module-global state.

Unit tests today only test file-content generation (pure functions) — they don't touch real filesystem state. But the spec proposes new integration tests at §6.1 that "GET then PUT then GET" on live filesystem state. Those **will** need `serial_test` guards if they run in parallel with each other or with anything else touching `~/.config/systemd/user/`.

**Recommended action**: update §6.5 to note that any new autostart integration test that touches real FS must be `#[serial]`-annotated.

### M7. Observability: `err.code` logging convention not propagated to autostart

**Where**: spec §6.2 line 1003 `warn!(err.code = "storage.failed", "autostart: write failed: {e}");`.

This line is correct per CLAUDE.md convention. But the **three** other error sites in autostart (macOS `launchctl load` failure, Linux `systemctl --user enable` failure, Windows `RegSetValueExW` non-zero) are not enumerated. The spec should enumerate all structured-log sites, not just one.

Additionally: the spec uses `info!(mechanism = ?mech, "autostart: enabled")` (line 1001). For audit-log consumers, `?mech` with Debug-derived formatting gives `"systemd"` vs `systemd` depending on the Debug impl — prefer `%mech` with `Display` for stable log parsing (the `AutostartStatus.mechanism: String` field from §4.3 is already a display-stable string).

### M8. Contract files (OpenAPI + manifest) update scope under-specified

**Where**: spec §6.5 lines 1089-1093.

Spec correctly identifies the contract files needing regeneration. Not explicitly noted: the contract update is an **integrity gate** (`.github/workflows/integrity-gates.yml`). If the OpenAPI spec and actual routes diverge, CI fails. Plan should note this is a gate, not optional.

Further: `http-interface-manifest.v1.json` is a JSON file currently hand-maintained (no script generator — I did not verify this claim exhaustively but the absence of `scripts/generate-manifest*.sh` suggests hand-maintenance). The plan should specify who writes these deltas and who verifies them.

### M9. CLAUDE.md / STATUS.md / PHASE-HISTORY.md doc updates not in spec scope

**Where**: spec §6.5 "Lefthook / CI implications" covers tests/lint; §10 "References" lists docs but doesn't commit to updates.

Per CLAUDE.md / docs/STATUS.md / docs/PHASE-HISTORY.md conventions (project memory index references these), any new feature-crate affecting test counts or workspace structure needs:
- `docs/STATUS.md` — test-count bump (new unit tests change the total).
- `docs/PHASE-HISTORY.md` — new Phase 9 entry.
- `docs/DOCUMENTATION_POLICY.md` companion — if a new user-facing guide is written (e.g., `docs/guides/tracking-schedule.md`), it needs a `.ko.md` companion per English-primary + Korean companion policy.

The spec doesn't promise any user-facing guide. That's acceptable for three "quick wins" (scope argument), but the plan should explicitly state either "no user-facing guide needed" or "guide + companion will be added in this PR". Otherwise, the documentation policy silently goes unsatisfied.

## Dimensions checklist

- [x] A. Cross-platform (Linux/macOS/Windows)
- [x] B. Test strategy
- [x] C. CI implications
- [x] D. Binary size + performance
- [x] E. Failure modes
- [x] F. Observability
- [x] G. Migration + rollout
- [x] H. Sandbox profile
- [x] I. Scheduler integration
- [x] J. Docs updates

## Notes / observations (not findings)

1. **macOS is NOT sandboxed** (`src-tauri/assets/oneshim.entitlements` has no `com.apple.security.app-sandbox` key; hardened-runtime-only). So the autostart IPC writing `~/Library/LaunchAgents/*.plist` does not hit sandbox-container-path restrictions. This is good news for Feature 2 on macOS — no new entitlements needed. H (sandbox profile) is clean.

2. **tauri v2 security CSP** at `src-tauri/tauri.conf.json` lines 30-33 restricts `connect-src` and `img-src` to `127.0.0.1:10090..10099`. None of the Phase 9 features introduce new origins, so no CSP changes needed.

3. **Existing frontend select-mode pattern** at `pages/timeline/AllFrames.tsx:582-603` is already there (spec §5.3 correctly identifies this; I verified — it is truly in place). The spec's "reshapes the remaining work" clarification is accurate: Feature 3's frontend work is incremental, not greenfield.

4. **Existing `batch_add_tag` handler at `handlers/tags.rs:83-98`** silently swallows per-row errors with `tracing::warn!` (verified lines 88-97). The spec §2.3 frames this as "partial-success risk" — the reality is worse: **the endpoint returns HTTP 200 with a `tagged_count` that may be less than `frame_ids.len()`, and no signal to the client which rows failed**. The spec's transactional rewrite is strictly better; reviewers should not second-guess Decision D9.

5. **Scheduler loop count** cited as 16 in CLAUDE.md matches what I found: `src-tauri/src/scheduler/loops/` has 12 files + conditional (health_check, suggestion_sse, suggestion_maintenance) per the grep. Spec's scheduler integration reasoning is on solid ground — just needs per-loop enumeration per I4.

6. **No `mock_clock` / `FakeClock` / `MockClock` symbols anywhere in the workspace**. Spec §6.1's "use a mock clock" is a load-bearing hand-wave; see C1.

7. **`supervisor_respawns_on_injected_panic` pattern** cited in my prompt is NOT present in the current workspace (grep returned nothing). Phase 9 scheduler loops inherit whatever panic handling the existing loops have — no new panic-recovery primitive needed, but also no new promise can be made. Removing the mention from my framing; panic-recovery is not a Phase 9 spec gap.

8. **CI matrix for test job is `ubuntu-latest`** (line 314-316 of `.github/workflows/ci.yml`). Spec's per-platform REST roundtrip test for autostart (§6.1) runs on ubuntu-latest for the Linux path, but as C2 explains, `systemctl --user enable` does not work there. The spec's test plan has a correctness gap here.

9. **`has_systemctl()` visibility bump** (§4.3: `pub(crate)`): fine. The `linux` module is already `#[cfg(target_os = "linux")]`-gated, so the symbol is compile-time conditional; bumping visibility is local.

10. **Binary-path stability table §4.7** (lines 561-569) is accurate for common cases. One gap: snap packages (Ubuntu Snap Store) relocate binaries under `/snap/<pkg>/current/` and the binary path changes on every snap refresh. `AutoStart` registered with the initial path will break on refresh. Recommend the plan add "Snap — Changed each refresh — **YES — broken**" to the table.

_End of review._
