# Phase 9 PR-B1 — Autostart Foundation

Wires the existing platform autostart implementations (macOS LaunchAgent + Windows Registry + Linux systemd/XDG) to a Tauri IPC + Settings UI surface, adds cross-platform single-instance enforcement via `tauri-plugin-single-instance` v2, and presents an opt-in onboarding prompt after the user's first 25-min productive focus session.

**Spec**: `docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md` (v3)
**Plan**: `docs/superpowers/plans/2026-04-25-phase9-pr-b1-autostart-foundation.md` (v2.6)
**Phase 1+2 review iterations**: 7 (3 spec + 4 plan), all Critical+Important resolved
**Phase 3 implementation commits**: 13 + 1 fix + 1 i18n test fix

## Architecture

- **OS state is sole source of truth** for autostart enabled/disabled (NOT cached in AppConfig per Phase 1 review I4)
- **AppConfig.autostart** stores ONLY onboarding state (prompt machine + counter + idempotency UUID)
- **Productive-session detection** runs in scheduler `monitor.rs` Idle↔Active transitions (no frontend round-trip — racy when window closed)
- **PR-B1 capabilities IPC** returns `{supported: true}` skeleton; PR-B2 adds real Linux env detection (Snap/Flatpak/headless)

## Changes

### Rust
- `crates/oneshim-core/src/config/sections/autostart.rs` — NEW. `AutostartConfig` + `AutostartPromptState` enum + `should_prompt()` + 10 unit tests
- `crates/oneshim-core/src/error_codes/autostart.rs` — NEW. `AutostartCode` enum (5 variants) via `define_code_enum!` macro per ADR-019
- `src-tauri/src/commands/autostart.rs` — NEW. 6 IPC commands: `enable_autostart`, `disable_autostart`, `is_autostart_enabled`, `autostart_capabilities`, `mark_autostart_prompt_state`, `get_autostart_config`
- `src-tauri/src/scheduler/loops/autostart_helper.rs` — NEW. Generic Runtime + closure-based testable inner fn + idempotent counter via session UUID
- `src-tauri/src/autostart.rs` — added `AutostartCapabilities` skeleton + `detect_capabilities()` (PR-B2 will populate)
- `src-tauri/src/main.rs` — registered `tauri-plugin-single-instance` plugin at top of builder chain
- `src-tauri/src/setup.rs` — D-Bus presence check warn log on Linux (per Phase 1 I7)
- `src-tauri/src/scheduler/loops/monitor.rs` — `FocusBlockState` integrated with Idle↔Active transitions
- `src-tauri/src/scheduler/loops/sync.rs` — passes `app_handle.clone()` to spawn_monitor_loop

### Frontend
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPrompt.tsx` — NEW. Modal with Enable/NotNow/DontAsk handlers; Escape + outside-click → NotNow
- `crates/oneshim-web/frontend/src/components/AutostartOnboardingPromptHost.tsx` — NEW. ShowPromptCoordinator with module-level `hasShownThisSession` singleton
- `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx` — added `StartupSection` between Language + Web Dashboard cards
- `crates/oneshim-web/frontend/src/pages/dashboard/DashboardLayout.tsx` — renders `<AutostartOnboardingPromptHost />` ABOVE `<Outlet>`
- `crates/oneshim-web/frontend/src/i18n/locales/{en,ko}.json` — `settings.autostart.*` (10 keys) + `onboarding.autostart.*` (5 keys)
- `crates/oneshim-web/frontend/src/i18n/wire-errors.{en,ko}.json` — 5 new autostart wire-error translations

### Wire codes registered (per ADR-019)
- `autostart.enable_failed`
- `autostart.disable_failed`
- `autostart.query_failed`
- `autostart.counter_increment_failed`
- `autostart.event_emit_failed`

### Documentation
- `docs/STATUS.md` — version bump v0.4.39-rc.1 → v0.4.40-rc.1, Rust test count 3,651 → 3,771
- `docs/PHASE-HISTORY.md` — Phase 9 PR-B1 entry
- `docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md` — design spec v3
- `docs/superpowers/plans/2026-04-25-phase9-pr-b1-autostart-foundation.md` — implementation plan v2.6

## Test results

- **Rust workspace**: 3,771 passed / 0 failed / 21 ignored
- **Frontend Vitest**: 272 passed / 0 failed across 42 test files (after wire-code count fix)
- **Wire-contract snapshot**: GREEN (47 codes match enum)
- **i18n CI**: GREEN (all 47 wire codes have en + ko translations)
- **cargo check/test/clippy/fmt**: all GREEN

## Manual smoke test matrix

> **REQUIRED before merge** — record results below for each platform.

### macOS (Apple Silicon, latest OS)

- [ ] Settings → Startup section visible (between Language + Web Dashboard)
- [ ] Toggle reflects current OS state on mount (correctly off for fresh install)
- [ ] Toggle ON: app starts at next login (verify by logout/login)
- [ ] Toggle OFF: app does NOT start at next login
- [ ] Single-instance: launch 2nd instance via dock → existing window comes to foreground (no 2nd process spawned)
- [ ] Onboarding prompt: complete a 25+ min focus session → modal appears within ~1s after session end
- [ ] Prompt "Enable" button: enables autostart + dismisses
- [ ] Prompt "Not now" button: schedules re-prompt after 5 more sessions
- [ ] Prompt "Don't ask again" button: never re-prompts
- [ ] Escape key + outside click: same as "Not now"
- [ ] Modal does NOT re-fire on Dashboard re-mount (single-fire)

### Windows 11

- [ ] Settings → Startup section visible
- [ ] Toggle ON: app starts at next login (HKCU\Software\Microsoft\Windows\CurrentVersion\Run entry created)
- [ ] Toggle OFF: app does NOT start at next login (Registry entry removed)
- [ ] Single-instance: launch 2nd instance via Start menu → existing window foregrounds
- [ ] Onboarding flow same as macOS

### Linux Ubuntu 24.04 (X11 GNOME)

- [ ] Settings → Startup section visible
- [ ] Toggle ON: systemd unit file created at `~/.config/systemd/user/oneshim.service`
- [ ] Toggle OFF: systemd unit file removed
- [ ] Single-instance: launch 2nd instance → existing window foregrounds (D-Bus path: `/com/oneshim/client/SingleInstance`)
- [ ] D-Bus warn log if `DBUS_SESSION_BUS_ADDRESS` absent (verify via SSH session test)
- [ ] Onboarding flow same as macOS

### Linux Wayland (Fedora 40 GNOME) — Wayland kept-hidden case (per spec §13)

- [ ] Single-instance focus-grab on kept-hidden window: autostart launches app to tray (no main window) → click dock icon → window appears
- [ ] If broken: documented as known limitation per spec §13 (PR-B2 will document user runbook)

## Cross-consumer rebase status

Branch `feature/phase9-autostart-foundation` based off main `5618558c`. Latest main is `1ecaffd6` (Quick Wins #503/#504 merged 2026-04-25). **Rebase required before opening PR.**

Other in-flight branches that touch overlapping files (per spec §17):
- `feature/external-grpc-audit-liveconfig` (features2)
- `fix/phase9-pr-a-followup-cleanup`
- `feature/d13-v2b-pr-b2-subscribe-metrics`

Coordination needed if any are queued for merge before this PR.

## Follow-ups (out of scope)

- **PR-B2**: Linux deep — sd-notify Type=notify integration, Snap/Flatpak/headless detection, capability-aware UI gating, Linux integration tests. Branch `feature/phase9-autostart-linux-deep` after PR-B1 merges. Estimate ~15h.
- **Wayland kept-hidden window fallback**: deferred per spec §13 risk register acceptance. If smoke matrix above reveals issues, follow-up PR with `window.create()` fallback.
- **Reconciler (spec §11.4)**: informational startup-time consistency check. Low priority.

## Commits in this PR (25)

```
fd8f64cf ─ docs(spec): Phase 9 PR-B autostart IPC + single-instance + Linux deep design
5f1add95 ─ docs(spec): v2 rev-1 incorporates 5 Critical + 8 Important Phase 1 review fixes
1777a387 ─ docs(spec): v3 rev-2 incorporates Phase 1 iter-2 review fixes
48ffbfb5 ─ docs(spec): Phase 1 closure — cosmetic v2→v3 markers cleanup
f187d03b ─ docs(plan): Phase 2 iter-1 — initial implementation plan for PR-B1
05cb8051 ─ docs(plan): v2 corrections — Phase 2 iter-2 review found 8 Critical issues
9517f731 ─ docs(plan): v2.5 — Phase 2 iter-3 supersession banners + PF4 + Step 4.6 enum rewrite
c3d7ef33 ─ docs(plan): Phase 2 CLOSURE — iter-4 final fixes + Phase 3 setup
f4a4e30a ─ chore(autostart): add tauri-plugin-single-instance v2 dependency        (Task 1)
0c9ac38d ─ feat(autostart): AutostartConfig + AutostartPromptState in core           (Task 2)
eecc5e00 ─ test(autostart): AutostartConfig serde + should_prompt + idempotency      (Task 3)
c3e8685a ─ feat(autostart): IPC commands (6 commands) + AutostartCode wire codes    (Task 4)
af013732 ─ docs(state): sync Task 4 completion + session-recovery note
1f1acb7f ─ test(autostart): IPC command unit tests + integration smoke               (Task 5)
3cb2bd3e ─ feat(autostart): single-instance plugin + focus-grab + D-Bus check        (Task 6)
072ffa97 ─ test(autostart): single-instance integration smoke test                   (Task 7)
288d307e ─ feat(autostart): GeneralTab Startup section + capabilities-aware UI       (Task 8)
c56f23e1 ─ test(autostart): GeneralTab Vitest coverage for Startup section           (Task 9)
69c5c805 ─ feat(autostart): productive-session detection + Rust-side counter         (Task 10)
bf9113d3 ─ fix(autostart): register CounterIncrementFailed + EventEmitFailed         (Task 10 fix)
0dc613ab ─ test(autostart): add dismissed_state_skips_event_emission                 (Task 11)
18f6e381 ─ feat(autostart): AutostartOnboardingPrompt + Host + DashboardLayout       (Task 12)
55c90949 ─ test(autostart): AutostartOnboardingPrompt Vitest coverage                (Task 13)
310627fd ─ docs(autostart): STATUS.md + PHASE-HISTORY.md entry for Phase 9 PR-B1     (Task 14)
62ec3aa5 ─ fix(i18n): wire-code count expectations 42 → 47                           (Task 14 followup)
```

🤖 Generated with [Claude Code](https://claude.com/claude-code)
