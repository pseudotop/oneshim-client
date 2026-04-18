# Phase 4 Updater Hardening Design

**Date:** 2026-04-18
**Scope:** D9 multi-key signature trust enhancement + D10 staged rollout defensive handling + D11 post-install self-healthy probe with auto-rollback.
**Bundling:** Single PR.
**Target version:** **v0.4.40-rc.1** (non-breaking feature addition; D11 is new, D9/D10 enhance already-enforced behaviors).
**Predecessor:** v0.4.39-rc.1 (currently in CI). Assumes v0.4.39 stable promotion before this work lands.

---

## 1. Goals & Scope

### 1.1 Goals

Close three paths through which a bad release can reach users unintentionally or permanently:

1. **Trapped key trust** — the updater currently trusts exactly one hardcoded Ed25519 public key. A compromised or lost signing key cannot be rotated without stranding clients. **D9** introduces a multi-key trust array.
2. **Silent rollout loophole** — the staged rollout gate (`<!-- rollout:N -->`) is fully implemented but treats missing `installation_id` as always-eligible, which means a config regression admits users into the first-receive cohort. **D10** inverts this to defensive-exclude + documents the authoring convention.
3. **Irrecoverable crash loop** — once a bad release is installed, nothing brings the user back to a working version automatically. **D11** adds a self-healthy marker + 2-failed-boot auto-rollback using the existing `.rollback.{ts}` backup.

**Release metadata enrichment (A-4)** across UI and release notes (non-breaking, cosmetic):
- `CHANGELOG.md` — already includes ISO date `## [VERSION] - YYYY-MM-DD` (retained).
- Release body — new: `**Release Date:** Month Day, Year UTC` + `**Since v{prev}:** N commits · M PRs · K files changed` via cliff.toml template.
- GitHub Release name — new: `ONESHIM Client v{VERSION} — Released Month Day, Year`.
- Client UI — new: **render existing `published_at` field** (already populated in `update_coordinator.rs:446`, never displayed) as "v0.4.40-rc.1 (2026-04-18 배포)".

### 1.2 Scope boundary & non-goals

**In scope:**
- `src-tauri/src/updater/{mod,install,github}.rs` — key array, probe integration, rollback execution.
- `src-tauri/src/app_runtime_launch.rs` — startup probe call order guarantee.
- `src-tauri/src/update_coordinator.rs` — `RolledBack` phase broadcast + telemetry event.
- `crates/oneshim-api-contracts/src/update.rs` — `UpdatePhase::RolledBack` variant, `RollbackInfo`, `RollbackReason`.
- `crates/oneshim-core/src/config/sections/storage.rs` — reuse existing `signature_public_key` field (single-key shape preserved for serde compat); **add** `TRUSTED_PUBLIC_KEYS` constant in a new Rust source file that the verify path consults.
- `crates/oneshim-web/frontend/src/...` — render rollback state + `published_at` date.
- `.github/workflows/release.yml` — release_notes.md header expansion.

**Files created:**
- `src-tauri/src/updater/health_probe.rs`
- `src-tauri/src/updater/trusted_keys.rs`
- `cliff.toml`
- `docs/guides/updater-rollout.md`
- `docs/guides/updater-key-rotation.md`

**Non-goals:**
- **Not a breaking change / not v0.5.0** — D9 default `require_signature_verification: true` is already in effect (verified at `storage.rs:349-351` + enforcement at `storage.rs:240-244`). No config flag flip; no config migration.
- **Not adding a new `released_at` field** — reuse existing `published_at` in `PendingUpdateInfo` (already populated per `update_coordinator.rs:446`).
- **macOS notarization pipeline fix** — separate infrastructure PR (pre-existing `head_branch` condition bug on `notarize-macos-release-assets.yml`).
- **Nightly channel official activation** — product decision; `UpdateChannel::Nightly` remains internal-only enum variant with no user-facing surface.
- **Apple codesign / Tauri bundling changes** — working as-is.
- **Server-side release targeting** — client-side FNV-1a hash only.
- **Delta-patch flow changes** — `updater/delta.rs` unchanged.
- **Downgrade UX or manual-rollback control surface** — automatic-only.
- **Telemetry for rollout cohort observability** — deferred to post-telemetry-stable follow-up.
- **AWS Bedrock / SigV4** — C5, separate Phase 4 candidate.
- **pre-release-check.sh:241 Dependabot-disabled JSON-blob bug** — separate 5-minute PR (documented in §8.5).

---

## 2. D9 — Multi-Key Signature Trust Array

### 2.1 Current state (already live)

- Release workflow signs every artifact with Ed25519 via PyNaCl + `UPDATE_SIGNING_PRIVATE_KEY_B64` secret (`release.yml:1113-1149`).
- Client verifies `.sig` files at download time (`install.rs:76-79`, `install.rs:217-260`).
- Single hardcoded public key in production config default: `"GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E="` (`storage.rs:354`).
- `validate_integrity_policy` at `storage.rs:235-244` rejects `require_signature_verification == false` when updates are enabled (no bypass possible).
- Two unit tests at `mod.rs:831-858`.

### 2.2 Gap

The single public key is a rotation trap — if the private key must be rotated (scheduled or emergency), existing clients with only the old public key reject signatures from the new private key. Moving to a multi-key trust array enables smooth rotation.

### 2.3 Change

New file `src-tauri/src/updater/trusted_keys.rs`:

```rust
//! Trusted Ed25519 verification keys for update artifacts.
//!
//! Add new keys to the TOP of this array when rotating. Remove
//! deprecated keys only as part of a compromise response (see
//! docs/guides/updater-key-rotation.md). Normal rotation retains
//! old keys across 1-2 release cycles.
pub(crate) const TRUSTED_PUBLIC_KEYS: &[&str] = &[
    // v1 — introduced 2026-04-18, production key since v0.4.x
    // (identical to storage.rs:354 default; migration path: next rotation
    // adds a second key above this one.)
    "GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E=",
];
```

`install.rs::verify_signature` (re-implement, preserving signature):
```rust
pub(super) fn verify_signature(
    &self,
    payload: &[u8],
    signature_bytes: &[u8],
) -> Result<(), UpdateError> {
    // 1. If config.signature_public_key is non-empty (legacy path),
    //    try that first to preserve current user-config override behavior.
    let configured_key = self.config.signature_public_key
        .split_whitespace().next().filter(|k| !k.trim().is_empty());
    if let Some(k) = configured_key {
        if try_verify_with_key_b64(k, payload, signature_bytes).is_ok() {
            return Ok(());
        }
    }
    // 2. Walk built-in TRUSTED_PUBLIC_KEYS array.
    for (idx, key_b64) in trusted_keys::TRUSTED_PUBLIC_KEYS.iter().enumerate() {
        if try_verify_with_key_b64(key_b64, payload, signature_bytes).is_ok() {
            if idx > 0 {
                tracing::info!("signature validated by trusted key #{idx} (rotation in progress)");
            }
            return Ok(());
        }
    }
    Err(UpdateError::Integrity("no trusted key validated the signature".into()))
}
```

Rationale for preserving the config-supplied key path: existing users who have set a custom public key via their config file keep that override working; the array is an additive trust anchor.

### 2.4 Tests

**New unit tests (3)**:
- `verify_signature_accepts_builtin_key` — payload signed with the seed corresponding to `TRUSTED_PUBLIC_KEYS[0]` validates.
- `verify_signature_accepts_second_trusted_key_when_first_inactive` — array with two keys; payload signed with second key validates and emits rotation log.
- `verify_signature_rejects_payload_when_no_key_matches` — payload signed with unknown key → `Integrity` error.

Existing tests at `mod.rs:831-858` stay; rename if they collide with the new ones.

---

## 3. D10 — Staged Rollout Defensive Handling & Convention

### 3.1 Current state (already live)

- `is_eligible_for_rollout()` FNV-1a hash at `mod.rs:312-322`.
- `parse_rollout_percent()` parses `<!-- rollout:N -->` from release body at `mod.rs:327-338`.
- `installation_id` auto-UUID on first launch at `app_runtime_launch.rs:66-74`.
- Update check applies rollout gate at `mod.rs:184-197`.
- 5 existing unit tests at `mod.rs:1366-1391`.

### 3.2 Gap

`mod.rs:190-197` treats `installation_id == None` as always-eligible (bypasses rollout gate entirely). A config regression that clears the ID places a user in the first-receive cohort unconditionally. Also, no authoring document for release authors on the `<!-- rollout:N -->` convention.

### 3.3 Change

#### 3.3.1 Defensive None handling

```rust
// mod.rs check_for_updates_from — after `if latest > current` block
let Some(ref installation_id) = self.config.installation_id else {
    tracing::warn!("installation_id missing — treating as rollout-excluded");
    return Ok(UpdateCheckResult::UpToDate { current });
};
let rollout_percent = parse_rollout_percent(&release.body);
if !is_eligible_for_rollout(installation_id, &latest_str, rollout_percent) {
    tracing::debug!("device not in rollout bucket ({rollout_percent}%)");
    return Ok(UpdateCheckResult::UpToDate { current });
}
```

#### 3.3.2 Spawn-order guarantee (addresses Loop 1 iter 1 finding I-4)

Document and verify that `app_runtime_launch.rs` **writes `installation_id` to disk before spawning the update-check scheduler loop**. This prevents a first-launch race where the update loop fires before the UUID is persisted.

Concretely at `app_runtime_launch.rs:66-74`, the current flow writes the ID synchronously via `ConfigManager::update`. The update-check scheduler is spawned later in the launch sequence (`scheduler/mod.rs`). Add a panic-on-unset assertion in the update-check loop initialization:

```rust
// scheduler/mod.rs, at update-check spawn site
debug_assert!(
    config.update.installation_id.is_some(),
    "installation_id must be set before update-check scheduler starts"
);
```

#### 3.3.3 Authoring convention document

New file `docs/guides/updater-rollout.md` (~100 lines):
- Exact syntax: `<!-- rollout:N -->` (HTML comment in release body, single-line).
- Recommended progression: 5% → 25% → 50% → 100%, observing 24-48h / 3-5d / 3-5d gaps.
- Behavior on missing comment: defaults to 100% (backward-compat).
- Editing after publish: clients pick up changes on next 24h check cycle.
- Determinism contract: same `(installation_id, version)` → same bucket across checks.

### 3.4 Tests

**New unit tests (2)**:
- `update_check_respects_rollout_exclusion` — mock release body `<!-- rollout:1 -->` + installation_id with hash bucket > 1 → `UpToDate` result.
- `update_check_without_installation_id_is_excluded` — confirms defensive None handling.

(5 existing tests at `mod.rs:1366-1391` retained.)

---

## 4. D11 — Post-Install Self-Healthy Probe & Auto-Rollback

Largest new implementation (~500 LOC).

### 4.1 Decision summary

| Decision | Value |
|---|---|
| Failure signal | Self-healthy marker pattern (D) |
| Healthy threshold | 30 seconds of continuous wall-clock uptime after scheduler-boot completion (C); injectable via `HealthProbe::with_threshold(Duration)` for testing |
| Failed boot tolerance | 2 consecutive failures without an intervening success marker (B); success marker resets counter to 0 |
| Probe I/O error behavior | Non-fatal: increment boot counter and proceed normally; do not block startup on probe filesystem errors |
| Rollback binary selection | Read `backup_path` field recorded in `.install_pending_{VERSION}` at install time (deterministic, not `mtime`-based) |
| Backup cleanup | After a successful `.self_healthy_{VERSION}` write, delete all `.rollback.{ts}` files older than the most recent one kept as emergency fallback |
| Platform rollback mechanism | Unix (macOS/Linux): in-process rename + restart. Windows: deferred — see §4.8 spike |
| Notification | `UpdatePhase::RolledBack` (UI) + toast + telemetry event |

### 4.2 State machine

```
[new install or update complete]
    │ install.rs records .install_pending_{VERSION} with
    │   { installed_at, previous_version, backup_path }
    ↓
[app boot N=1]
    ├── HealthProbe::check_startup_state(): no marker yet
    │     → increment .boot_count_{VERSION} (atomic write)
    │     → return Normal (proceed with startup)
    ↓ scheduler completes boot
    ↓ spawn_healthy_writer: wait 30s wall-clock, no crash
    ├── on success → write .self_healthy_{VERSION}
    │     → delete .install_pending_{VERSION} + .boot_count_{VERSION}
    │     → cleanup .rollback.{ts} files except the one in backup_path
    └── on crash/exit before 30s → counter not reset; next boot sees boot_count=1

[app boot N=2]  (only if boot N=1 failed to write marker)
    ├── check_startup_state: boot_count=1, below threshold(2)
    │     → increment to 2
    │     → return Normal (give one retry)
    ↓ if success here → normal marker write
    ↓ if crash again → next boot sees boot_count=2

[app boot N=3] pre-flight
    ├── check_startup_state: boot_count=2, no marker, threshold reached
    │     → read .install_pending_{VERSION}.backup_path
    │     → return RollbackRequired { from_version, to_version=previous_version, backup_path, reason }
    ↓
[rollback executes] replace running binary with backup_path → restart
```

### 4.3 State files

Stored in the **install directory** (same directory as the running executable). Rationale: config.json is in app-data and can be mutated by app logic during a failing startup; the install directory is owned by the installer with atomic write guarantees.

| File | Format | Content | Lifecycle |
|---|---|---|---|
| `.install_pending_{VERSION}` | JSON | `{ "installed_at": "<ISO-8601>", "previous_version": "<semver>", "backup_path": "<absolute path>" }` | Created at install completion; deleted when `.self_healthy_{VERSION}` is written |
| `.boot_count_{VERSION}` | text | `<u32>` (rename-on-write atomic) | Incremented per startup; deleted with `.install_pending_` |
| `.self_healthy_{VERSION}` | text | `<ISO-8601>` | Written once 30s uptime reached; persists until next version install |

**Prefix consistency**: all three files use dot-prefix and `{VERSION}` suffix (e.g., `.self_healthy_0.4.40-rc.1`). No `v` prefix in filenames to avoid ambiguity with git tags.

**macOS .app bundle note**: state files live beside the current executable (`current_exe().parent()` resolves to `.../ONESHIM.app/Contents/MacOS/` on macOS). Because a macOS update replaces the entire `.app` bundle, prior-version state files inside the old bundle are discarded by the bundle replacement. This is correct behavior — the new version's install.rs writes fresh `.install_pending_{NEW_VERSION}` after its install, and rollback files from the old bundle are irrelevant.

### 4.4 New module `src-tauri/src/updater/health_probe.rs`

```rust
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

pub struct HealthProbe {
    install_dir: PathBuf,
    current_version: String,
    healthy_threshold: Duration,
    failed_boot_threshold: u8,
}

pub enum StartupAction {
    Normal,
    RollbackRequired {
        from_version: String,
        to_version: String,
        backup_path: PathBuf,
        reason: RollbackReason,
    },
}

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("install_pending file malformed: {0}")]
    InstallPendingParse(String),
    // ... other variants
}

impl HealthProbe {
    pub fn new(install_dir: PathBuf, current_version: String) -> Self {
        Self {
            install_dir,
            current_version,
            healthy_threshold: Duration::from_secs(30),
            failed_boot_threshold: 2,
        }
    }

    pub fn with_threshold(mut self, threshold: Duration) -> Self {
        self.healthy_threshold = threshold;
        self
    }

    /// Called synchronously at earliest startup (before scheduler spawn).
    /// Contract: any filesystem error is treated as Normal with a warning log
    /// — probe I/O failures do not block user startup.
    pub fn check_startup_state(&self) -> StartupAction {
        match self.check_startup_state_inner() {
            Ok(action) => action,
            Err(err) => {
                tracing::warn!("health probe filesystem error — proceeding normally: {err}");
                StartupAction::Normal
            }
        }
    }

    fn check_startup_state_inner(&self) -> Result<StartupAction, ProbeError> {
        // 1. .self_healthy_{VERSION} present? → Normal (nothing to do)
        // 2. .install_pending_{VERSION} absent? → Normal (fresh install, first boot writes pending via install.rs path)
        // 3. Read boot_count; increment and write atomically.
        // 4. If new_count >= failed_boot_threshold → RollbackRequired from install_pending metadata.
        // 5. Otherwise → Normal (retry window).
    }

    /// Spawn a tokio background task: wait `healthy_threshold`, then
    /// write `.self_healthy_{VERSION}` and clean state files.
    /// Uses `tokio::time::sleep` — in tests, caller injects shorter threshold.
    pub fn spawn_healthy_writer(self) -> tokio::task::JoinHandle<()> { /* ... */ }
}
```

### 4.5 Integration points

| Location | Change |
|---|---|
| `src-tauri/src/app_runtime_launch.rs` | After existing config + installation_id setup, before scheduler spawn: instantiate `HealthProbe` with `current_exe().parent()?` as install_dir; call `check_startup_state()`; on `RollbackRequired`, invoke `install::execute_rollback(backup_path, from, to, reason)` and exit. |
| `src-tauri/src/updater/install.rs` | On successful install completion (new helper `write_install_pending`): write `.install_pending_{VERSION}` JSON with `backup_path` = the `.rollback.{ts}` path that was created at `install.rs:391`. |
| `src-tauri/src/scheduler/mod.rs` | After all loops spawn, invoke `probe.spawn_healthy_writer()`. The healthy timer starts only after the scheduler is fully up (design intent: "30s uptime" = 30s after useful app state is reachable, not 30s after process start). |
| `src-tauri/src/update_coordinator.rs` | Translate a rollback completion into `UpdatePhase::RolledBack` broadcast + toast + telemetry. |

### 4.6 Rollback execution (`install::execute_rollback`)

Extends existing `install_and_restart_with_ops` flow:

```rust
pub fn execute_rollback(
    &self,
    backup_path: &Path,
    from_version: &str,
    to_version: &str,
    reason: RollbackReason,
) -> Result<(), UpdateError> {
    // 1. Verify backup_path exists + has executable permissions.
    // 2. Unix (macOS/Linux): atomic rename of backup_path → current_exe_path.
    // 3. Windows: see §4.8 spike deliverable.
    // 4. Broadcast UpdatePhase::RolledBack event before process exit.
    // 5. Self-restart via the platform mechanism already used elsewhere.
}
```

### 4.7 Tests

**Unit (5)**:
- `check_startup_no_pending_install_is_normal`
- `check_startup_with_healthy_marker_is_normal`
- `check_startup_below_failed_boot_threshold_is_normal` (clarifies threshold dimension)
- `check_startup_at_failed_boot_threshold_triggers_rollback` (clarifies threshold dimension)
- `spawn_healthy_writer_sets_marker_after_injected_short_delay` — uses `HealthProbe::with_threshold(Duration::from_millis(50))` on real filesystem (no tokio paused time — `std::fs` is outside tokio time control)

**Non-fatal contract test (1)**:
- `probe_io_error_is_non_fatal` — point `install_dir` at a read-only directory, confirm `check_startup_state()` returns `Normal` with warn-log.

**Integration (1)**:
- `rollback_e2e_restores_previous_binary` — in `src-tauri/tests/`: create temp install dir with fake current + backup binaries + install_pending JSON, invoke `check_startup_state` + `execute_rollback`, assert binary swap occurred and exit code is rollback-specific.

**Total D11 tests: 7 new**. Reconciliation with §6.1: 3 (D9) + 2 (D10) + 7 (D11) = **12 new tests**.

### 4.8 Windows rollback spike (pre-implementation)

Windows cannot replace a running executable. The spike day (allocated before implementation) validates:

1. **Install location assumption** — Tauri v2 defaults install to `%LOCALAPPDATA%\oneshim-app\` (user-scope, not Program Files). Verify against current installer manifest. If Program Files is used, UAC blocks rename and this design becomes unsafe.
2. **Helper mechanism choice** — compare:
   - `cmd.exe /c "timeout /t 3 /nobreak >nul && move /Y {backup} {current} && start {current}"` (simple, PATH-dependent).
   - Small `.exe` helper bundled at install time (robust, adds to installer).
   - `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)` (UX poor; user must reboot before rollback activates).
3. **Antivirus interaction** — test with Windows Defender active; confirm no quarantine on `.exe` rename.

Spike output: decision memo in `docs/guides/updater-key-rotation.md` (windows sub-section) + updated §4.6 Windows row with exact implementation.

**If spike reveals blockers**: fall back to `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)` and notify user on next launch.

---

## 5. UI & UpdateStatus Extension

### 5.1 `oneshim-api-contracts::update` additions

```rust
// crates/oneshim-api-contracts/src/update.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpdatePhase {
    Idle, Checking, PendingApproval, Downloading,
    ReadyToInstall, Installing, Updated, Deferred, Error,
    RolledBack,  // new
}

// PendingUpdateInfo already has published_at: Option<String> at line 25.
// No new field added; frontend reads the existing field.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackInfo {
    pub from_version: String,
    /// RFC3339 UTC timestamp from GitHub Releases published_at.
    pub from_published_at: Option<String>,
    pub to_version: String,
    /// RFC3339 UTC timestamp from GitHub Releases published_at.
    pub to_published_at: Option<String>,
    pub reason: RollbackReason,
    /// RFC3339 UTC timestamp at which the rollback completed.
    pub rolled_back_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RollbackReason {
    RepeatedStartupFailure,  // initial reason; enum is additive for future
}

// UpdateStatus gains: pub rollback: Option<RollbackInfo>   (populated when phase == RolledBack)
```

### 5.2 Frontend rendering

| Touchpoint | Change |
|---|---|
| `UpdateStatusPanel.tsx` | Add `RolledBack` case; render from/to versions + `from_published_at` / `to_published_at` dates + reason string |
| Existing shared date formatter (or new one if absent) | Relative time for <24h ("3시간 전 배포"), absolute ISO YYYY-MM-DD for older |
| `PendingUpdateInfo` render | **Surface existing `published_at`** — currently data is in the contract but frontend doesn't display it |
| i18n keys (ko/en) | ~10 new: `update.rolledBack.title`, `update.rolledBack.reason.repeatedStartupFailure`, `update.releaseDate`, `update.releasedAgo` (interpolated with unit) |

### 5.3 Desktop notification

Reuse existing `DesktopNotifierImpl` (oneshim-vision). One toast per rollback event, deduplicated by `RollbackInfo.rolled_back_at`.

Toast copy (ko):
> "ONESHIM 업데이트 안내 — v{from} ({from_date} 배포) 설치 문제로 v{to} ({to_date} 배포)로 복구되었습니다."

---

## 6. Testing Matrix & External Impact

### 6.1 Test totals

| Level | Scope | Count (new) |
|---|---|---|
| Unit | D9 (3) + D10 (2) + D11 probe (5) + probe-io-nonfatal (1) | **11** |
| Integration (`src-tauri/tests/`) | D11 rollback E2E (1) | 1 |
| Platform CI matrix | macOS/Linux inline swap job + Windows helper job (gated on §4.8 spike result) | 2 rows |
| E2E smoke (`release-reliability-smoke.sh`) | Post-install probe trigger validation (1 step) | 1 step |
| **Total** | | **12 tests + 2 CI rows + 1 smoke step** |

### 6.2 External dependencies

| System | Impact | Action |
|---|---|---|
| `self_update` crate | None (wrapped only) | — |
| `ed25519_dalek` | Already used | — |
| GitHub Releases API `published_at` | Already fetched at `update_coordinator.rs:446`, never rendered | Propagate to UI |
| `app_runtime_launch.rs` installation_id | Spawn-order tightened (no API change) | Debug assert added at scheduler start |
| CI release.yml | cliff.toml + release_notes.md header | ~50 LOC (new cliff.toml + ~10 LOC workflow edit) |
| Cargo deps | No new | — |
| `CHANGELOG.md` | Manual 0.4.40-rc.1 entry (git-cliff generates body, user adds context) | Author responsibility at release time |

### 6.3 `cliff.toml` template concrete stub

```toml
[changelog]
header = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n"
body = """
{% if version %}
## [{{ version | trim_start_matches(pat="v") }}] — {{ timestamp | date(format="%Y-%m-%d") }}

**Release Date:** {{ timestamp | date(format="%B %d, %Y UTC") }}
**Since {{ previous.version }}:** {{ commits | length }} commits · {{ contributors | length }} contributors

{% else %}## [Unreleased]
{% endif %}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}- {{ commit.message | upper_first }}{% endfor %}
{% endfor %}
"""
```

`release.yml` `release_notes.md` generation adds exactly one new line before `## What's Changed`:
```yaml
- name: Prepend date header to release notes
  run: |
    DATE=$(date -u +"%B %d, %Y")
    TAG="${RELEASE_TAG}"
    printf "## ONESHIM Client ${TAG} — Released ${DATE}\n\n" | cat - release_notes.md > _rn && mv _rn release_notes.md
```

---

## 7. Release Timeline & Rollout

### 7.1 Version path

```
[now]  v0.4.39-rc.1 (CI in progress)
            ↓ RC validation 2-4 weeks
v0.4.39 stable (promote-stable.sh 0.4.39-rc.1)
            ↓ Phase 4 PR merges
v0.4.40-rc.1 ← D9 + D10 + D11 (non-breaking feature release)
            ↓ PreRelease channel opt-in, 2-4 weeks
            ↓ Observe rollout/rollback in telemetry
v0.4.40 stable (promote-stable.sh)
            ↓ <!-- rollout:5 -->
            ↓ <!-- rollout:25 --> (24-48h)
            ↓ <!-- rollout:50 --> (3-5d)
            ↓ <!-- rollout:100 --> full public
```

**v0.5.0 deferred**: that version number is reserved for a genuinely breaking change later; this PR does not justify the minor bump.

### 7.2 Defence-in-depth layers

| Layer | Role | Active from |
|---|---|---|
| PreRelease channel opt-in | Early adopter pool | RC tag publish |
| `<!-- rollout:N -->` gate | Post-stable phased public | Stable tag |
| D9 signature verification (multi-key) | MITM + tamper block with rotation capability | Every update (already on) |
| D11 self-healthy probe + rollback | Recovery from bad builds | Post-install, 2 boot cycles |

### 7.3 Emergency procedures (split: scheduled vs compromise)

#### 7.3.1 Scheduled rotation (planned, low urgency)

1. Derive new keypair using `rehearse-key-rotation.sh` as reference.
2. Add **new** public key at position `[0]` of `TRUSTED_PUBLIC_KEYS` in `trusted_keys.rs`, keeping the old key at `[1]`.
3. Ship `v0.4.N+1-rc.1` containing both keys (clients now trust both).
4. After this release is in distribution for ≥ 1 release cycle (every active installation has the new key), switch the CI signing secret `UPDATE_SIGNING_PRIVATE_KEY_B64` to the new private key. Releases from this point sign with the new key.
5. After ≥ 1 additional release cycle with the new key in use, remove the old key from the array in `v0.4.N+2`.

Total window: two releases before the old key is gone from trust.

#### 7.3.2 Compromise response (urgent)

Private key has been exposed. Old-key-signed updates are now untrustworthy, even if the old release assets are authentic.

1. Derive new keypair immediately.
2. In a hotfix branch, **remove the compromised key** (do NOT retain it) and insert only the new key in `TRUSTED_PUBLIC_KEYS`.
3. Switch `UPDATE_SIGNING_PRIVATE_KEY_B64` secret to the new private key.
4. Ship `v0.4.N-hotfix` signed with the new key.
5. **Users on v0.4.N-1 or earlier**: their client has only the old (now-removed) key and will reject the hotfix. They must re-install manually from a signed installer download (Apple codesign + GitHub Attestation provide the out-of-band trust anchor for the initial installer). Notify via release notes + external channels.
6. Revoke the compromised signing key at the CI secret level (rotate GitHub token access).

Runbook detail: `docs/guides/updater-key-rotation.md`.

### 7.4 Release-body rollout gate usage

GitHub Release body editing updates the `<!-- rollout:N -->` comment. Clients pick up the change at their next 24h check cycle. Emergency stop: set `<!-- rollout:0 -->` to block all new downloads; combined with D11 auto-rollback in already-distributed clients, this gives a two-layer kill switch.

---

## 8. Summary

### 8.1 Files touched

- **Modified (9)**: `src-tauri/src/updater/{mod.rs,install.rs}`, `app_runtime_launch.rs`, `scheduler/mod.rs`, `update_coordinator.rs`, `oneshim-api-contracts/src/update.rs`, frontend `UpdateStatusPanel.tsx` + related, `.github/workflows/release.yml`, `CHANGELOG.md` (manual release-note entry at publish time).
- **Created (5)**: `src-tauri/src/updater/health_probe.rs`, `src-tauri/src/updater/trusted_keys.rs`, `cliff.toml`, `docs/guides/updater-rollout.md`, `docs/guides/updater-key-rotation.md`.

### 8.2 LOC estimate

| Section | LOC |
|---|---|
| D9 multi-key array + verify_signature refactor + tests | ~80 |
| D10 defensive None + spawn-order assert + tests + rollout guide | ~120 |
| D11 health_probe + install_pending writer + execute_rollback + tests | ~500 |
| UI / UpdateStatus (types + Rust coordinator + frontend render) | ~80 |
| cliff.toml + release.yml header | ~50 |
| Docs (rollout + key-rotation with scheduled + compromise branches) | ~200 |
| **Total** | **~1,030** |

### 8.3 Effort

3-4 calendar days of focused work + 1 Windows spike day. Multi-session execution expected.

### 8.4 Acceptance criteria

- All tests pass: new **12 tests** listed in §6.1 + existing suite unchanged.
- `cargo clippy --workspace --all-targets -- -D warnings` zero warnings.
- `cargo fmt --check` clean.
- Manual smoke: install v0.4.40-rc.1 locally; kill the process twice within 30 seconds of startup; third launch triggers rollback; UI shows `RolledBack` state; toast notification appears; `execute_rollback` restores the backup binary recorded in `.install_pending_{VERSION}.backup_path`.
- Release body at `https://github.com/pseudotop/oneshim-client/releases/tag/v0.4.40-rc.1` includes the `**Release Date:** …` + `**Since v0.4.39:** …` headers from cliff.toml template.
- `PendingUpdateInfo.published_at` is rendered in the frontend's update panel in the format "v{VERSION} (YYYY-MM-DD 배포)".
- Windows platform CI row (`release-reliability-smoke.ps1` or equivalent) exercises the rollback path successfully using the mechanism chosen in §4.8 spike.
- Scheduled rotation runbook executes end-to-end in a dry-run (use `rehearse-key-rotation.sh`).

### 8.5 Deferred follow-ups

- **pre-release-check.sh:241** Dependabot-disabled JSON-blob guard — separate 5-minute PR.
- **Notarize workflow `head_branch` condition** — separate infrastructure PR to fix skipped notarization runs.
- **Telemetry for rollout cohort observability** — after telemetry feature stabilizes (Phase 2 foundation already in).
- **Nightly channel official activation** — product decision; enum variant remains hidden.
- **Additional `RollbackReason` variants** — additive; out of v0.4.40 scope.
- **CalVer-in-tag adoption (original A-2 option)** — deferred; A-4 surfaces date information through `published_at` without altering tag scheme.

---

## 9. Open questions resolved during revision

| ID | Question | Resolution |
|---|---|---|
| Q1 | Current `require_signature_verification` default? | `true` (storage.rs:349) — no flip needed |
| Q2 | Existing `published_at` field? | Present (update.rs:25) + propagated (update_coordinator.rs:446) — reuse, do not duplicate |
| Q3 | Rollback binary selection? | `backup_path` recorded in `.install_pending_{VERSION}` at install time |
| Q4 | Probe I/O error behavior? | Non-fatal Normal return with warn log |
| Q5 | `healthy_threshold` testability? | `HealthProbe::with_threshold(Duration)` builder method |
| Q6 | installation_id race? | Spawn-order guarantee at scheduler-start with debug_assert |
| Q7 | cliff.toml template shape? | Stub provided in §6.3 |
| Q8 | Key rotation vs compromise? | Two distinct procedures, §7.3.1 and §7.3.2 |
| Q9 | macOS app-bundle state files? | Inside bundle; bundle replacement discards them (correct) |
| Q10 | `install_dir` resolution? | `std::env::current_exe()?.parent()?` (matches where backup is written) |

### Deferred to implementation

- Exact base64 public key value is the one already in `storage.rs:354`; no derivation needed.
- Windows rollback mechanism (§4.8 spike day) must precede implementation of Windows code path.

---

*End of design document.*
