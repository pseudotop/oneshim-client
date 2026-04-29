# PR-B2: Phase 9 Autostart Linux Deep — systemd Type=notify + hash-based migration + env detection

Upgrades Linux autostart from the PR-B1 `Type=simple` skeleton to a production-quality `Type=notify` service, adds hash-based automatic migration for existing installs (both ONESHIM-era and Maekon-era service files), and replaces the PR-B1 `detect_capabilities()` stub with real Linux environment detection (Snap/Flatpak/headless).

**Spec**: `docs/superpowers/specs/2026-04-25-phase9-pr-b2-autostart-linux-deep-design.md` (v2.5)
**Plan**: `docs/superpowers/plans/2026-04-25-phase9-pr-b2-autostart-linux-deep-plan.md` (v2)
**Phase 1+2 review iterations**: 12 (5 spec + 7 plan), all Critical+Important resolved
**Phase 3 implementation commits**: 14 (Tasks 1-15; Tasks 2+3 combined per `wire-error-i18n-coverage` hook coupling)

## Summary

- **systemd Type=notify integration**: Linux service template upgraded from `Type=simple` to `Type=notify` + `NotifyAccess=main` + `TimeoutStartSec=30`. New `lifecycle::sd_notify::notify_ready()` wrapper sends `READY=1` signal at end of `setup::init()`. Gated behind non-default `systemd-notify` Cargo feature flag — Linux release builds must opt in via `--features systemd-notify`.
- **Hash-based deferred migration**: existing PR-B1 service files auto-migrated to the PR-B2 template on app startup. Both ONESHIM-brand (v0.4.40-rc.1/rc.2) and Maekon-brand (v0.4.40-rc.3/v0.4.40 stable) hashes registered in `KNOWN_PRIOR_HASHES`. `daemon-reload` deferred to next user login (no service interruption). Customized service files are skipped with a warn log.
- **Real `detect_capabilities()` Linux env detection**: replaces PR-B1 stub. Detects Snap/Flatpak container sandboxes, headless sessions (no `DISPLAY`/`WAYLAND_DISPLAY`), and selects systemd vs XDG autostart fallback. Settings UI toggle disabled with environment-specific tooltip for unsupported platforms.
- **4 new wire codes** (ADR-019): `autostart.sd_notify_skipped`, `autostart.service_migrated`, `autostart.service_migration_failed`, `autostart.service_migration_skipped`. Wire snapshot: 49 → 53.
- **Linux integration tests + Korean ops guide**: rootless `linux-autostart-unit` always-on CI job; `linux-systemd-integration.yml` manual `workflow_dispatch` for T8-T10 smoke runs. Korean operations guide at `docs/guides/autostart.ko.md` covers platform behavior matrix, migration steps for both brand eras, and troubleshooting.

## Architecture notes

- **OS state is sole source of truth** (inherited from PR-B1 I4): no caching in `AppConfig`
- **Migration is fire-and-forget on startup**: if migration fails it logs `autostart.service_migration_failed` and leaves the old file; the app still starts normally
- **sd_notify is lenient-when-absent**: if `NOTIFY_SOCKET` env var is absent (non-systemd launch, default builds without the feature flag) the wrapper is a no-op and logs `autostart.sd_notify_skipped`

## Changes

### Rust

- `src-tauri/src/autostart.rs` — real `detect_capabilities()` replacing PR-B1 stub; Snap/Flatpak/headless detection
- `src-tauri/src/lifecycle/sd_notify.rs` — NEW. `notify_ready()` wrapper; no-op on missing `NOTIFY_SOCKET` / non-Linux / default builds; `autostart.sd_notify_skipped` wire code
- `src-tauri/src/lifecycle/migration_hashes.rs` — NEW. `KNOWN_PRIOR_HASHES` const slice + `migrate_service_file_if_needed()` + deferred-reload semantics; wire codes `service_migrated` / `service_migration_failed` / `service_migration_skipped`
- `src-tauri/src/lifecycle/mod.rs` — exports `sd_notify` + `migration_hashes` modules
- `src-tauri/src/setup.rs` — calls `migration_hashes::migrate_service_file_if_needed()` + `sd_notify::notify_ready()` at end of `init()`
- `crates/oneshim-core/src/error_codes/autostart.rs` — 4 new variants added to `AutostartCode` enum; snapshot 49 → 53
- `src-tauri/src/autostart/linux_service_template.rs` — `Type=notify` + `NotifyAccess=main` + `TimeoutStartSec=30` template

### Tests

- `src-tauri/src/lifecycle/migration_hashes.rs` — 10 unit tests (happy-path migration, skip-when-customized, skip-when-path-missing, both brand-era hash fixtures)
- `src-tauri/src/lifecycle/sd_notify.rs` — 2 unit tests (notify_ready no-op when NOTIFY_SOCKET absent, smoke)
- `.github/workflows/linux-autostart-unit.yml` — always-on `linux-autostart-unit` CI job (rootless Ubuntu)
- `.github/workflows/linux-systemd-integration.yml` — manual `workflow_dispatch` T8-T10 smoke

### Docs

- `docs/guides/autostart.ko.md` — NEW. Korean ops guide: platform behavior matrix, migration guide (ONESHIM + Maekon era), troubleshooting
- `docs/STATUS.md` — test count + wire snapshot bumped
- `docs/PHASE-HISTORY.md` — PR-B2 entry added

### i18n

- `src-tauri/src/web/frontend/src/i18n/en.json` — 3 new tooltip keys (Snap / Flatpak / headless disabled states)
- `src-tauri/src/web/frontend/src/i18n/ko.json` — matching Korean translations

## Test Plan

### Cross-platform regression check

- macOS / Windows: PR-B1 autostart toggle behavior MUST be unchanged (this PR only modifies Linux paths)
- Workspace test count: 3,906 passed (macOS host) — PR-B2 contribution +12 (migration_hashes 10 + sd_notify 2)
- Vitest: 279 passed across 43 test files (wire-code assertion bumped 49 → 53; no new test cases)

### Linux smoke matrix (per environment)

| Environment | Toggle | Service file | Migration | Manual fallback |
|-------------|--------|--------------|-----------|-----------------|
| Ubuntu 24.04 systemd (X11 GNOME) | enabled | Type=notify | auto | n/a |
| Fedora 40 Wayland GNOME | enabled | Type=notify | auto | n/a |
| sway (Wayland tiling WM) | enabled | Type=notify | auto | n/a |
| Snap package | disabled with tooltip | n/a | n/a | use Snap's autostart |
| Flatpak package | disabled with tooltip | n/a | n/a | use Flatpak portal |
| Headless SSH session | disabled with tooltip | n/a | n/a | desktop session req'd |
| systemctl missing (legacy distro) | enabled | n/a (XDG fallback) | n/a | XDG .desktop |

### Smoke procedure (Ubuntu 24.04 reference)

- [ ] Settings → Startup section visible
- [ ] Toggle ON: writes `~/.config/systemd/user/oneshim.service` with `Type=notify` + `NotifyAccess=main` + `TimeoutStartSec=30`
- [ ] Toggle ON + logout + login: `systemctl --user is-active oneshim` returns `active` (clean Type=notify lifecycle, READY signal received)
- [ ] Toggle OFF: removes service file; `systemctl --user is-enabled oneshim` returns `disabled`
- [ ] Migration (ONESHIM era): install v0.4.40-rc.1 (or rc.2) → toggle ON → install PR-B2 build → restart app → log shows `err.code = autostart.service_migrated` (info) + service file Type=notify + currently-running service NOT restarted
- [ ] Migration (Maekon era): install v0.4.40 → toggle ON → install PR-B2 build → restart app → log shows `err.code = autostart.service_migrated` (info) + service file Type=notify + currently-running service NOT restarted
- [ ] Customized file (manual edits): install v0.4.40 → manually edit `oneshim.service` → restart app → log shows `autostart.service_migration_skipped` (warn) + file untouched

### Snap (if test packaging available)

- [ ] Settings → Startup toggle disabled with tooltip "Snap의 내장 자동 시작 설정을 사용하세요" (ko) / equivalent en string
- [ ] No service file written

### Flatpak (if test packaging available)

- [ ] Settings → Startup toggle disabled with tooltip "Flatpak의 내장 자동 시작 설정을 사용하세요" (ko) / equivalent en string

### Headless SSH session

- [ ] Settings → Startup toggle disabled with tooltip "자동 시작은 데스크톱 세션이 필요합니다" (ko) / equivalent en string

### macOS / Windows regression check

- [ ] PR-B1 autostart behavior unchanged (toggle ON writes plist/registry; toggle OFF removes; round-trip works)

## Migration Semantics

The migration strategy is **deferred-reload**: the service file is rewritten to the PR-B2 template but `daemon-reload` is intentionally NOT invoked. This was a Phase 1 review decision to avoid:

1. `Type=notify` requires a `READY=1` signal within `TimeoutStartSec=30`. If systemd reloads while the running service has not yet been restarted with the new sd_notify code, it would time out and mark the service as failed.
2. Invoking `daemon-reload` at runtime would terminate the currently-running instance.

**User-visible effect**: migration is silent with no service interruption. The new `Type=notify` semantics take effect on the next user login, when systemd naturally reloads user units on session start.

## Wire codes added (per ADR-019)

| Code | Level | Purpose |
|------|-------|---------|
| `autostart.sd_notify_skipped` | debug | `notify_ready()` no-op: `NOTIFY_SOCKET` absent, non-Linux platform, or default build without `systemd-notify` feature |
| `autostart.service_migrated` | info | Service file successfully rewritten to PR-B2 template (deferred daemon-reload) |
| `autostart.service_migration_failed` | warn | Write failure during migration; old file left intact; app continues normally |
| `autostart.service_migration_skipped` | warn / debug | Migration cannot proceed: path missing, file customized (hash mismatch vs known set), or `current_exe()` failure |

Wire snapshot: 49 → 53 (TimeWindow primitive had added 2 codes between plan-write and impl, correcting earlier plan estimate of 47 → 51).

## Implementation notes

- **Wire counts corrected at impl time**: plan v2 said 47 → 51; actual baseline was 49 (TimeWindow primitive PR landed between plan-write and impl). Final: 49 → 53. No logic change, just bookkeeping.
- **`Description=Maekon` in template**: plan v2 still referenced `Description=ONESHIM`; corrected at impl time to reflect Maekon rebrand (#520, landed between plan-write and impl).
- **Both brand-era hashes**: originally planned only for the Maekon era; Task 7 review added ONESHIM-era rc.1/rc.2 hashes to `KNOWN_PRIOR_HASHES` as well.
- **Tasks 2+3 combined**: separate "wire codes" + "i18n keys" commits collapsed into one atomic commit to satisfy the `wire-error-i18n-coverage` lefthook hook, which validates both in a single pass.
- 14 commits on branch `feature/phase9-autostart-linux-deep`, reset onto post-PR-B1 main `0613f976` (v0.4.40 stable). Spec/plan iteration history preserved on main via #524 audit recovery commit.
