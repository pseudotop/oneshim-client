# Phase 1 — Iteration 1 Spec Review Findings

**Date**: 2026-04-25
**Spec under review**: `docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md` v1 (commit `fd8f64cf`)
**Reviewers**: 3 parallel subagents (correctness reviewer + cross-consumer auditor + feasibility verifier) + direct code verification
**Outcome**: 5 Critical, 8 Important, 5 Nice-to-have, 4 cross-consumer conflicts

---

## CRITICAL Issues (block plan phase)

### C1 — ConfigManager API completely fictitious in spec

**Spec assumed**: `Arc<ConfigManagerHandle>`, `update(|c| ...).await`, `update_returning(|c| ...).await`, `app.state::<Arc<ConfigManagerHandle>>()`

**Reality** (verified in `crates/oneshim-core/src/config_manager.rs:139-157`):
- Type: `ConfigManager` (no `Handle`)
- API: `update(&self, new_config: AppConfig) -> Result<(), CoreError>` (sync, whole-config)
- API: `update_with<F>(&self, updater: F) -> Result<AppConfig, CoreError>` where `F: FnOnce(&mut AppConfig) -> Result<(), String>` (sync, returns full AppConfig snapshot)
- State access pattern (`commands/settings.rs:92-95`): `state: tauri::State<'_, ConfigRuntimeState>` then `state.config_manager().update(...)` or `state.config_manager().get()`

**Impact**: Every IPC command code block in spec §5.1 will fail to compile. Spec must rewrite all 5 commands.

**Fix**: Rewrite §5.1 using real API. Sync (no async). Use `update_with` closure. State as Tauri command parameter, not `app.state::<>()`. For value-returning need (counter), use the returned `AppConfig` snapshot to extract counter.

---

### C2 — Tauri identifier separation is intentional, but spec ambiguous + plugin behavior unverified

**Reality**: `tauri.conf.json:4` = `"identifier": "com.oneshim.client"`; `autostart.rs:6` = `APP_LABEL = "com.oneshim.agent"`. Both intentional:
- `com.oneshim.client` = main app bundle ID (used by macOS bundle, Tauri internal)
- `com.oneshim.agent` = LaunchAgent service name (used by `launchctl`, plist filename)

This is a standard pattern (e.g., Slack: `com.slack.Slack` app, `com.slack.launcher` agent).

**Impact**: 
- `tauri-plugin-single-instance` Linux D-Bus name will be derived from Tauri identifier = `com.oneshim.client` (NOT autostart APP_LABEL)
- Document the separation explicitly in code + spec
- Verify plugin's actual D-Bus name source (currently inferred — must check plugin docs)

**Fix**: Update §5.2 to:
1. Document the two-identifier rationale (link to ADR if exists, otherwise inline)
2. Confirm `com.oneshim.client` is the D-Bus name plugin will register
3. Add note: macOS LaunchAgent + Linux systemd service files use `com.oneshim.agent` for backwards compat (existing user paths)

---

### C3 — Two-phase commit cannot recover from crash mid-revert; disable revert wrong default

**Spec issue** (§5.1 enable_autostart):
- OS enable success → config save fail → call `disable_autostart()` to revert
- If revert itself fails: silently swallowed (`let _ =`)
- If crash between OS enable and config save: durable inconsistency
- For `disable_autostart`: revert path tries to RE-ENABLE on config-save fail. Wrong: user clicked DISABLE, defaulting to leave-disabled is correct.

**Fix**: Replace two-phase commit with:
1. **Single-phase + reconciler**: write config first (intent), then enforce OS state, then mark settled. On startup, reconcile OS state vs config intent → if mismatch, log warn (no auto-correct, user can re-toggle).
2. Revert paths: best-effort with explicit warn log (not silent `_`). For `disable`, do NOT re-enable.
3. Add §11.4 explicit reconciler logic.

---

### C4 — Type=simple → Type=notify migration breaks running services + clobbers user customization

**Spec issue** (§6.1, §11.2):
- Migration: detect `Type=simple` in service file → overwrite with new template → `daemon-reload`
- Failure mode 1: app is currently running under `Type=simple` systemd unit. We overwrite file + daemon-reload. systemd now expects READY notification on already-running service. After `TimeoutStartSec=30`, marks unit `failed` → kill + restart loop.
- Failure mode 2: user customized service file (e.g., added `Environment=FOO=bar`). Blind overwrite destroys their changes.

**Fix**:
1. **Defer migration to NEXT login** (not during current session): write new file, log info, don't `daemon-reload`. systemd loads new file on next user session start.
2. **Hash check before overwrite**: compute hash of file, compare against known prior-version hashes. If matches: safe to overwrite. If doesn't match: user customized → log warn + skip + add note to runbook for manual migration.
3. Update §11.2 with explicit migration matrix.

---

### C5 — Productive session counter: no source event exists, frontend-initiated is racy, focus_metrics is daily-aggregate

**Spec issues** (§5.5, §7.2, §10.1):
- `focus_metrics` table is DAILY aggregate (verified `crates/oneshim-storage/src/migration/v01_v08.rs::migrate_v6`) — no per-session granularity. Cannot derive sessions from existing data.
- NO existing event for "productive session completed" — must add to scheduler loop.
- Spec proposes scheduler emits Tauri event → frontend listener → frontend invokes IPC. **Race**: if main window is closed (tray-only), event lost.
- No idempotency: rapid event burst could double-count.

**Fix**:
1. **Move increment to Rust-side directly**: scheduler loop calls `ConfigManager::update_with` to increment counter when productive session detected. No Tauri event round-trip for counter.
2. **Frontend gets notified via separate concern**: scheduler emits `productive-session-completed` Tauri event AFTER counter incremented, for UI to know to re-evaluate prompt eligibility.
3. **Idempotency key**: increment scoped to a session_id (UUID generated at session start). Counter increments only on first observation of session_id.
4. **Where to emit**: per feasibility audit, `monitor.rs` (focus state) or new helper. Add to spec §5.5 + commit list §10.1.
5. **Scheduler restart handling**: if app restarts mid-session, the in-memory session state is lost — counter doesn't increment. Acceptable: missed increments are not harmful (worst case: prompt fires later than expected).

---

## IMPORTANT Issues (must fix in plan phase)

### I1 — Wayland kept-hidden window single-instance behavior unspecified
**Issue**: `window.show()` on never-mapped surface may not surface usable window on Wayland.
**Fix**: Add explicit smoke test in §9.5: "autostart launches to tray; click dock icon → window appears (test on GNOME Wayland + KDE Wayland + sway)". Document fallback if it doesn't work.

### I2 — Onboarding 500ms delay race conditions
**Issue**: timer reset on event mid-delay? Modal queue? Re-trigger on Dashboard re-mount?
**Fix**: Concrete state machine in §5.5:
- `ShowPromptCoordinator` runtime singleton (per app session)
- Single-fire per session: once shown, never re-shown until app restart
- Mount triggers eligibility check + 500ms timer → on fire, check still eligible + not already shown → show
- Eligibility change events during timer: don't reset timer, just update next-tick eligibility

### I3 — i18n template literal anti-pattern
**Issue**: `t('settings.general.autostart.unsupported.${caps.unsupported_reason?.kind}')` defeats CI parity check + produces literal `"undefined"` for None.
**Fix**: Use i18next `context`: `t('settings.general.autostart.unsupported', { context: caps.unsupported_reason?.kind ?? 'unknown' })` and explicitly enumerate keys (`unsupported_snap_sandbox`, `unsupported_flatpak_sandbox`, etc.) in en.json/ko.json.

### I4 — `AutostartConfig.enabled` field has no consumer
**Issue**: spec writes `enabled` but only readers are: (a) the same write path (no-op), (b) hypothetical future code. Drift source per §13.
**Fix**: Two options:
- **(a) Remove `enabled` field**: simpler, OS state is sole source. Update §5.3 to drop the field. Adjust serde/migration tests.
- **(b) Specify reader**: e.g., startup reconciler logs warn if `enabled=true` but OS=false.
**Recommendation**: (a) — YAGNI. The field adds 0 value if no reader.

### I5 — `--privileged` container CI is security regression
**Issue**: `--privileged` grants all capabilities, broader than needed for systemd integration tests.
**Fix**: Replace with rootless systemd alternative:
- Use `systemd-run --user --scope` patterns where possible (no container needed)
- Or split into 2 jobs: (i) service file generation tests (no systemd, fast, all PRs) + (ii) actual systemd interaction tests (manual trigger, branch-protected, defer to PR-B2 nice-to-have)
- Verify if any other workflow uses `--privileged` for precedent

### I6 — `update_with` returns full AppConfig — counter case wasteful
**Issue**: To return a single u32 counter from `increment_productive_session`, must clone whole AppConfig.
**Fix**: Two options:
- **(a) Accept clone overhead**: AppConfig is small (~10KB), called rarely (1× per 25 min). Acceptable.
- **(b) Add `update_with_returning<F, T>` API in a separate commit**: cleaner but more scope.
**Recommendation**: (a) for PR-B1, defer (b) to a future cleanup if profiling shows hot path.

### I7 — Plugin failure on D-Bus absence: "fail-open" pattern doesn't exist
**Issue**: Spec says "wrap plugin init in `match`" but `tauri::Builder::plugin()` returns the builder, not Result. Plugin init failure surfaces at runtime.
**Fix**: Update §5.2 to:
1. Plugin always added unconditionally (no match wrap)
2. If D-Bus absent: 2nd instance attempt fails to send IPC → 2nd instance behaves as standalone (no focus-grab) → user sees 2 windows. Worse UX, but app still launches.
3. Document this as known limitation in §13 risk register
4. Add log statement: at startup, check `DBUS_SESSION_BUS_ADDRESS` env var; if absent, log warn "single-instance enforcement degraded — focus-grab may not work"

### I8 — PR-B2 schema break risk
**Issue**: PR-B2 adds capabilities IPC + UI gating. If users on PR-B1 stable for weeks before PR-B2, frontend code path differs.
**Fix**: PR-B1 ships capabilities IPC skeleton (returns `{supported: true}` always for non-Linux + `{supported: detect_only_basic}` for Linux). Frontend code path identical between B1 and B2.
- Adds ~1h to PR-B1 estimate (~22h → ~23h)
- Adds 1 commit to PR-B1 (was 14 → 15)

---

## NICE-TO-HAVE

### N1 — Industry convention citations missing
**Fix**: Add citations to §3 (User-Locked Decision U3 rationale):
- App Store Review Guideline 5.4.1 (Login Items): https://developer.apple.com/app-store/review/guidelines/#login-items
- Microsoft Store Policy 10.2.4 (Background tasks)
- Both require explicit user opt-in and prohibit silent enrollment.

### N2 — `should_prompt` helper duplicate
**Fix**: Rust helper has no caller (frontend reimplements). Either remove from oneshim-core OR add `is_prompt_eligible` IPC for Rust to be source. Recommendation: keep helper in oneshim-core for unit testability + add IPC.

### N3 — i18n key parity CI lint not named
**Fix**: Verify `i18next-parser` or similar exists. If not, add to N4 list.

### N4 — Risk register undersells C3/C4
**Fix**: Re-rate "Two-phase commit revert fails" Low/Medium → Medium/Medium. Add user-recovery runbook in `docs/guides/autostart.ko.md`.

### N5 — Commit 13 mentions `handlers` extraneously
**Fix**: Remove the "if new endpoint added" qualifier from §10.1 commit 13 — autostart is Tauri IPC only, no REST endpoint.

---

## Cross-Consumer Conflicts

### CC1 — `feature/external-grpc-audit-liveconfig` (features2) — CRITICAL
**Files in conflict**: `crates/oneshim-core/src/config/mod.rs` (AppConfig field additions), `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`, `src-tauri/src/commands/settings.rs` (ALLOWED_KEYS)
**Risk**: PR-B1 + features2 both touch AppConfig fields. features2 (65 commits) is large.
**Mitigation**:
- features2 should merge first (it's ahead in queue per memory)
- PR-B1 rebase onto features2-merged main
- Resolution is mostly additive (different fields)

### CC2 — `feature/grpc-stress-test-suite` — IMPORTANT
**Files**: AppConfig + commands registration in main.rs
**Mitigation**: Currently parallel; whichever merges second rebases. Conflict resolution mechanical.

### CC3 — `fix/phase9-pr-a-followup-cleanup` — CRITICAL
**Files**: `tracking_schedule` related files, AppConfig
**Mitigation**: Should merge BEFORE PR-B1 (continuation of PR-A which we depend on conceptually). Add explicit dependency in §16.

### CC4 — `feature/d13-v2b-pr-b2-subscribe-metrics` — CRITICAL SEMANTIC
**Files**: `crates/oneshim-core/src/config/mod.rs` (REMOVES `external_grpc` field)
**Risk**: B2 branch removes field that features2 adds. If both branches merged in different orders, conflicts arise.
**Mitigation**: Coordinate with v2b owner. Likely merges before PR-B1.

---

## Summary of Spec v2 Changes Required

1. **§5.1** — Rewrite all 5 IPC commands using real `ConfigManager` API + `tauri::State<'_, ConfigRuntimeState>` pattern
2. **§5.2** — Document identifier separation, fix plugin failure description (no `match` wrap)
3. **§5.3** — Remove `enabled` field (per I4), update default + migration accordingly
4. **§5.5** — Move counter increment to Rust-side, add ShowPromptCoordinator state machine, fix race conditions
5. **§5.6** — Update i18n examples to use `context` instead of template literals
6. **§6.1** — Rewrite Type=notify migration: defer reload, hash check, document customization handling
7. **§6.2** — Capabilities IPC also needed in PR-B1 (skeleton returning supported=true)
8. **§6.3** — Replace `--privileged` CI with rootless systemd or split jobs
9. **§9** — Add Wayland kept-hidden test, single-instance D-Bus absence test
10. **§10.1** — Add 1 commit for capabilities skeleton (PR-B1: 14 → 15 commits, ~22h → ~23h)
11. **§11** — Add §11.4 reconciler logic + re-rate risks
12. **§12.1** — Resolve all 6 questions with concrete answers
13. **§13** — Re-rate C3/C4 related risks, add D-Bus absence + Wayland kept-hidden risks
14. **§17 (NEW)** — Cross-consumer dependencies + merge order

---

## Iteration 2 Goals

After spec v2 commit:
1. Fresh subagent re-review of spec v2 to verify fixes are correct
2. Verify no NEW issues introduced
3. Test compile-feasibility of revised IPC command code blocks against actual API
4. If clean: advance Phase 1 → Phase 2 (writing-plans skill invocation)
5. If issues remain: iteration 3
