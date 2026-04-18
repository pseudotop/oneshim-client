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
3. **Irrecoverable crash loop** — once a bad release is installed, nothing brings the user back to a working version automatically. **D11** adds a self-healthy marker + 2-failed-boot auto-rollback using the existing `{binary_name}.rollback.{ts}` backup.

**Release metadata enrichment (A-4)** across UI and release notes (non-breaking, cosmetic):
- `CHANGELOG.md` — already includes ISO date `## [VERSION] - YYYY-MM-DD` (retained).
- Release body — new: `**Release Date:** Month Day, Year UTC` + `**Since v{prev}:** N commits · M contributors` via cliff.toml template amendment (not PR or file counts — those aren't available in git-cliff without a shell wrapper; see §6.3 for scope detail).
- GitHub Release name — new: `ONESHIM Client v{VERSION} — Released Month Day, Year`.
- Client UI — new: **render existing `published_at` field** (already populated in `update_coordinator.rs:446`, never displayed) as "v0.4.40-rc.1 (2026-04-18 배포)". **Fallback**: when `published_at == None`, render version alone without date suffix.

### 1.2 Scope boundary & non-goals

**Files modified (in scope):**
- `src-tauri/src/updater/{mod,install,github}.rs` — key array wiring, probe integration, rollback execution, `write_install_pending` helper.
- `src-tauri/src/app_runtime_launch.rs` — startup probe call-order guarantee.
- `src-tauri/src/scheduler/mod.rs` — production-visible error log + telemetry when `installation_id` missing at update-check spawn.
- `src-tauri/src/update_coordinator.rs` — `RolledBack` phase broadcast + telemetry event.
- `crates/oneshim-api-contracts/src/update.rs` — `UpdatePhase::RolledBack` variant, `RollbackInfo`, `RollbackReason`.
- `crates/oneshim-core/src/config/sections/storage.rs` — relax `validate_integrity_policy` to no longer require `signature_public_key` non-empty (verify path now consults the built-in array first; see §2.3).
- `crates/oneshim-web/frontend/src/...` — render rollback state + `published_at` date (with fallback for `None`).
- `.github/workflows/release.yml` — release_notes.md header expansion.
- **`cliff.toml` (existing, ~30 lines)** — amend body template to inject `**Release Date:** …` + `**Since {{ previous.version }}:** …` lines. See §6.3 for the diff.

**Files created:**
- `src-tauri/src/updater/health_probe.rs`
- `src-tauri/src/updater/trusted_keys.rs`
- `docs/guides/updater-rollout.md`
- `docs/guides/updater-key-rotation.md`
- `docs/guides/updater-rollback-windows.md` (deliverable of §4.8 spike)

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

`install.rs::verify_signature` (re-implement with **array-first precedence** so rotation takes effect):

```rust
pub(super) fn verify_signature(
    &self,
    payload: &[u8],
    signature_bytes: &[u8],
) -> Result<(), UpdateError> {
    // 1. Walk built-in TRUSTED_PUBLIC_KEYS array (primary trust source).
    //    Rotation: add new key at [0], old key stays at [1] during transition.
    for (idx, key_b64) in trusted_keys::TRUSTED_PUBLIC_KEYS.iter().enumerate() {
        if try_verify_with_key_b64(key_b64, payload, signature_bytes).is_ok() {
            if idx > 0 {
                tracing::info!(
                    "signature validated by trusted key #{idx} (rotation in progress)"
                );
            }
            return Ok(());
        }
    }
    // 2. Fallback: if user has overridden signature_public_key in their
    //    config to a non-default value (e.g., self-signing in dev), try it.
    let configured_key = self.config.signature_public_key
        .split_whitespace().next().filter(|k| !k.trim().is_empty());
    if let Some(k) = configured_key {
        if !trusted_keys::TRUSTED_PUBLIC_KEYS.iter().any(|&t| t == k) {
            if try_verify_with_key_b64(k, payload, signature_bytes).is_ok() {
                tracing::warn!("signature validated via user-configured key (override)");
                return Ok(());
            }
        }
    }
    Err(UpdateError::Integrity("no trusted key validated the signature".into()))
}
```

Rationale:
- Array is consulted **first** so rotation is effective. The legacy behavior (configured key always wins) would silently shadow the array whenever `storage.rs:354` default is in effect (i.e., for every user who hasn't manually edited their config), defeating the purpose of D9.
- Configured key is consulted as a fallback only when it's **different** from any built-in key (genuine user override path, e.g., dev self-signing). If it matches a built-in key, the array already validated.

### 2.3.1 Related change: `validate_integrity_policy` relaxation

`crates/oneshim-core/src/config/sections/storage.rs:247-256` currently requires `signature_public_key` to be non-empty when updates are enabled. Since the built-in array is now the primary trust source, this requirement is obsolete and prevents future ergonomic improvements (e.g., empty default that forces array-only trust).

Change: remove the "empty key" error path from `validate_integrity_policy`. Retain the base64 + 32-byte validation but only when the field is non-empty (opt-in override).

### 2.4 Tests

**New unit tests (4)**:
- `verify_signature_accepts_builtin_key` — payload signed with the seed corresponding to `TRUSTED_PUBLIC_KEYS[0]` validates.
- `verify_signature_accepts_second_trusted_key_when_first_inactive` — array with two keys; payload signed with second key validates and emits rotation log.
- `verify_signature_fallback_to_configured_key_when_not_in_array` — configured `signature_public_key` that is NOT a built-in key is used as fallback and validates with warn log.
- `verify_signature_rejects_payload_when_no_key_matches` — payload signed with unknown key → `Integrity` error.

Existing tests live in `src-tauri/src/updater/mod.rs:831-858` (both `verify_signature_accepts_valid_ed25519_signature` and its negative counterpart). They stay; rename if collision with the new array-first variants.

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

Concretely at `app_runtime_launch.rs:66-74`, the current flow writes the ID synchronously via `ConfigManager::update`. The update-check scheduler is spawned later in the launch sequence (`scheduler/mod.rs`). Add **production-visible** observability at the update-check loop initialization:

```rust
// scheduler/mod.rs, at update-check spawn site
if config.update.installation_id.is_none() {
    // Regression guard: this should be unreachable because
    // app_runtime_launch.rs:66-74 writes the UUID synchronously
    // before any scheduler loop spawns. Surface loudly if the
    // invariant breaks so the regression is observable in production.
    tracing::error!(
        "update-check scheduler started with installation_id = None; \
         rollout gate will exclude this device"
    );
    telemetry::increment_counter("updater.installation_id_missing_at_scheduler_start");
    debug_assert!(false, "installation_id must be set before update-check scheduler starts");
}
```

Release builds get `tracing::error!` + telemetry counter (observable). Debug builds additionally panic via `debug_assert!` (immediate dev feedback). This makes a first-launch race regression visible in both environments.

**Telemetry API surface (to verify at implementation time)**: the Phase 2 telemetry foundation (X2) exposes counter emission via the `tracing` ecosystem (OTel span events) rather than a direct `telemetry::increment_counter` function. If the exact API is not yet public at the caller site, the plan-writer should **stub with `tracing::error!` only** and defer the counter wiring to a follow-up. Do not invent a new API; use whatever Phase 2 already landed (verify path during Task 0 audit of the plan).

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
| Backup cleanup | After a successful `.self_healthy_{VERSION}` write, delete all `{binary_name}.rollback.{ts}` files older than the most recent one kept as emergency fallback |
| Platform rollback mechanism | Unix (macOS/Linux): in-process rename + restart. Windows: deferred — see §4.8 spike |
| Notification | `UpdatePhase::RolledBack` (UI) + toast + telemetry event |

### 4.2 State machine

```
[new install or update complete]
    │ install.rs records .install_pending_{VERSION} with
    │   { installed_at, previous_version, backup_path }
    ↓
[app boot N=1]
    ├── HealthProbe::check_startup_state():
    │     step 0: staleness check (installed_at > 24h ago + no marker?)
    │             → skip on first boot (installed_at is recent)
    │     step 1-2: no marker, pending exists → continue
    │     step 3: read .boot_count_{VERSION} (absent or 0)
    │     step 4: current count (0) < threshold(2) → not rollback
    │     step 5: atomically increment to 1, return Normal
    ↓ scheduler completes boot
    ↓ spawn_healthy_writer: wait 30s wall-clock, no crash
    ├── on success → write .self_healthy_{VERSION}
    │     → delete .install_pending_{VERSION} + .boot_count_{VERSION}
    │     → cleanup {binary_name}.rollback.{ts} files except the one in backup_path
    └── on crash/exit before 30s → counter stays at 1; next boot sees boot_count=1

[app boot N=2]  (only if boot N=1 failed to write marker)
    ├── check_startup_state: step 3 reads count=1; step 4 count < threshold(2)
    │     → step 5: increment to 2, return Normal (retry window)
    ↓ if success here → normal marker write
    ↓ if crash again → counter stays at 2; next boot sees boot_count=2

[app boot N=3] pre-flight
    ├── check_startup_state: step 3 reads count=2; step 4 count ≥ threshold(2)
    │     → read .install_pending_{VERSION}.backup_path
    │     → return RollbackRequired { from_version, to_version=previous_version, backup_path, reason }
    ↓
[rollback executes] replace running binary with backup_path → terminate + restart

[Staleness branch — same-version manual reinstall or long abandoned pending]
    ├── check_startup_state step 0: installed_at > 24h ago AND no .self_healthy_ marker
    │     → delete .install_pending_{VERSION} + .boot_count_{VERSION}
    │     → return Normal (abandoned pending; DO NOT roll back)
```

### 4.3 State files

Stored in the **install directory** (same directory as the running executable). Rationale: config.json is in app-data and can be mutated by app logic during a failing startup; the install directory is owned by the installer with atomic write guarantees.

| File | Format | Content | Lifecycle |
|---|---|---|---|
| `.install_pending_{VERSION}` | JSON | `{ "installed_at": "<ISO-8601>", "previous_version": "<semver>", "backup_path": "<absolute path>" }` | Created at install completion; deleted when `.self_healthy_{VERSION}` is written |
| `.boot_count_{VERSION}` | text | `<u32>` (rename-on-write atomic) | Incremented per startup; deleted with `.install_pending_` |
| `.self_healthy_{VERSION}` | text | `<ISO-8601>` | Written once 30s uptime reached; persists until next version install |

**Prefix consistency**: all three files use dot-prefix and `{VERSION}` suffix (e.g., `.self_healthy_0.4.40-rc.1`). No `v` prefix in filenames to avoid ambiguity with git tags.

**Backup filename format**: the existing `install.rs:378-392::backup_path_for()` returns `{parent}/{binary_name}.rollback.{nano_ts}` (e.g., `oneshim-app.rollback.1736123456789000000`). `backup_path` stored in `.install_pending_{VERSION}` is the **full absolute path** from this formatter. Cleanup glob derives `{binary_name}` from `current_exe().file_name()`; never hardcode `.rollback.*` as a pattern.

**Self-reinstall idempotency**: the `.install_pending_{VERSION}` file includes an `installed_at` ISO-8601 timestamp. Probe rule: if `installed_at` is older than 24 hours **and** `.self_healthy_{VERSION}` is absent, treat as stale (e.g., a user manually reinstalled the same version with a broken config that swallows the healthy writer's filesystem access). Delete stale `.install_pending_{VERSION}` + `.boot_count_{VERSION}` and return `Normal` without triggering rollback. This prevents a phantom rollback after manual same-version reinstallation.

**macOS .app bundle note**: state files live beside the current executable (`current_exe().parent()` resolves to `.../ONESHIM.app/Contents/MacOS/` on macOS). For cross-version updates that replace the bundle, the install.rs of the new version writes a fresh `.install_pending_{NEW_VERSION}` post-install — state is version-scoped, not carried across. For same-version manual reinstalls on macOS (drag-drop install of the same version), the 24h staleness rule above covers the phantom-rollback scenario.

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
        // 0. Staleness check (self-reinstall idempotency, §4.3):
        //    If .install_pending_{VERSION} exists AND .self_healthy_{VERSION}
        //    absent AND install_pending.installed_at > 24h ago →
        //      delete .install_pending_{VERSION} + .boot_count_{VERSION}
        //      return Normal (abandoned pending; DO NOT increment boot_count
        //      and DO NOT trigger rollback)
        // 1. .self_healthy_{VERSION} present? → Normal (nothing to do)
        // 2. .install_pending_{VERSION} absent? → Normal (fresh install,
        //    first boot writes pending via install.rs path)
        // 3. Read current .boot_count_{VERSION} (absent/unreadable → 0).
        // 4. If current_count >= failed_boot_threshold (i.e., >= 2):
        //      → read .install_pending_{VERSION}.backup_path
        //      → return RollbackRequired { ... } (DO NOT increment further)
        // 5. Otherwise (current_count < threshold):
        //      → atomically write current_count + 1 to .boot_count_{VERSION}
        //      → return Normal
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
| `src-tauri/src/updater/install.rs` | New helper `write_install_pending(version, previous_version, backup_path)`. **Call site**: immediately after `replace_binary` succeeds in `install_and_restart_with_ops` (`install.rs:407-408`) and **before** `restart_app`. **On `write_install_pending` failure** (e.g., install dir read-only, disk full): attempt restoration using the same platform mechanism as `execute_rollback` (Unix rename, Windows spike-deliverable helper). If restoration itself also fails, emit `tracing::error!` and return `UpdateError::Install` — the user is left on the new binary without a pending marker; the next scheduled probe will not trigger rollback (no `.install_pending_` file), but subsequent health anomalies can still be caught by manual downgrade. Windows constraint applies symmetrically: the restoration path faces the same running-executable constraint addressed by the §4.8 spike. **On earlier step failure** (download, signature verify, replace_binary): explicitly `std::fs::remove_file(backup_path)` to clean the orphan `{binary_name}.rollback.{ts}` file before returning the original error. |
| `src-tauri/src/scheduler/mod.rs` | After all loops spawn, invoke `probe.spawn_healthy_writer()`. The healthy timer starts only after the scheduler is fully up (design intent: "30s uptime" = 30s after useful app state is reachable, not 30s after process start). |
| `src-tauri/src/update_coordinator.rs` | Translate a rollback completion into `UpdatePhase::RolledBack` broadcast + toast + telemetry. |

### 4.6 Rollback execution (`install::execute_rollback`)

Extends existing `install_and_restart_with_ops` flow:

```text
// Function signature:
pub fn execute_rollback(
    &self,
    backup_path: &Path,
    from_version: &str,
    to_version: &str,
    reason: RollbackReason,
) -> Result<std::convert::Infallible, UpdateError>;

// Contract: on success, this function DOES NOT RETURN — current process is
// replaced (Unix process-image replacement) or terminated after spawning a
// helper (Windows). On error, returns UpdateError so caller can log/exit.

// Implementation sketch:
// 1. Verify backup_path exists + has executable permissions.
// 2. Broadcast UpdatePhase::RolledBack on the async runtime + flush.
// 3. Unix: std::fs::rename(backup_path, current_exe_path), then replace
//    the current process image using std::os::unix::process::CommandExt
//    (the trait method that replaces the running image with a new binary).
// 4. Windows: §4.8 spike deliverable chooses between shell-helper spawn
//    (current process exits via std::process::exit; helper does swap +
//    restart) or MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT) (current process
//    exits; swap happens at next boot).
// 5. Happy path: Infallible never materializes; process is terminated.

// Constant:
pub const ROLLBACK_EXIT_CODE: i32 = 75;  // EX_TEMPFAIL — rolled back
```

**Caller in `app_runtime_launch.rs`** (§4.5) on `StartupAction::RollbackRequired`:
```text
match install.execute_rollback(&backup_path, &from, &to, reason) {
    Ok(_infallible) => unreachable!(), // Infallible — success path terminates
    Err(e) => {
        tracing::error!("rollback failed: {e}");
        // Leave user on the current (failing) binary; next boot retries.
        std::process::exit(1);
    }
}
```

### 4.7 Tests

**Unit (6)**:
- `check_startup_no_pending_install_is_normal`
- `check_startup_with_healthy_marker_is_normal`
- `check_startup_below_failed_boot_threshold_is_normal` (clarifies threshold dimension)
- `check_startup_at_failed_boot_threshold_triggers_rollback` (clarifies threshold dimension)
- `stale_install_pending_older_than_24h_returns_normal_without_rollback` (self-reinstall idempotency per §4.3)
- `spawn_healthy_writer_sets_marker_after_injected_short_delay` — uses `HealthProbe::with_threshold(Duration::from_millis(50))` on real filesystem (no tokio paused time — `std::fs` is outside tokio time control)

**Non-fatal contract test (1)**:
- `probe_io_error_is_non_fatal` — point `install_dir` at a read-only directory, confirm `check_startup_state()` returns `Normal` with warn-log.

**Integration (1)**:
- `rollback_e2e_restores_previous_binary` — in `src-tauri/tests/`: create temp install dir with fake current + backup binaries + install_pending JSON, invoke `check_startup_state` + a mock replacement of `execute_rollback` that performs only the binary-swap step (skipping process replacement since the test can't actually be replaced). Assert: (a) `current_exe_path` content now matches the pre-rollback backup bytes, (b) `.install_pending_{VERSION}` was read before swap, (c) `UpdatePhase::RolledBack` event was broadcast. No assertion on process exit code because the test harness cannot observe cross-process replacement.

**D11 total: 7 unit + 1 integration = 8 new tests.**

**Phase 4 total reconciliation**: D9 (4 unit) + D10 (2 unit) + D11 (7 unit) + D11 integration (1) = **13 unit + 1 integration = 14 new tests**. (Matches §6.1.)

### 4.8 Windows rollback spike (pre-implementation)

Windows cannot replace a running executable. The spike day (allocated before implementation) validates:

1. **Install location assumption** — Tauri v2 defaults install to `%LOCALAPPDATA%\oneshim-app\` (user-scope, not Program Files). Verify against current installer manifest. If Program Files is used, UAC blocks rename and this design becomes unsafe.
2. **Helper mechanism choice** — compare:
   - `cmd.exe /c "timeout /t 3 /nobreak >nul && move /Y {backup} {current} && start {current}"` (simple, PATH-dependent).
   - Small `.exe` helper bundled at install time (robust, adds to installer).
   - `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)` (UX poor; user must reboot before rollback activates).
3. **Antivirus interaction** — test with Windows Defender active; confirm no quarantine on `.exe` rename.

Spike output: decision memo in new file `docs/guides/updater-rollback-windows.md` + updated §4.6 Windows row with exact implementation.

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
| `UpdateStatusPanel.tsx` | Add `RolledBack` case; render from/to versions + `from_published_at` / `to_published_at` dates + reason string. **Fallback when a date is `None`**: render version alone without the " (YYYY-MM-DD 배포)" suffix and show `update.releaseDateUnknown` as tooltip on hover. |
| Existing shared date formatter (or new one if absent) | Relative time for <24h ("3시간 전 배포"), absolute ISO YYYY-MM-DD for older. Return `null` when input is `None`. |
| `PendingUpdateInfo` render | **Surface existing `published_at`** — currently data is in the contract but frontend doesn't display it. Render conditionally: present → "v0.4.40-rc.1 (2026-04-18 배포)", absent → "v0.4.40-rc.1". |
| i18n keys (ko/en) | ~13 new: `update.rolledBack.title`, `update.rolledBack.reason.repeatedStartupFailure`, `update.rolledBack.toast.bothDates` (interpolated with from/to versions + dates), `update.rolledBack.toast.partialDates` (when at least one date is None), `update.releaseDate`, `update.releaseDateUnknown`, `update.releasedAgo` (interpolated with unit). |

### 5.3 Desktop notification

Reuse existing `DesktopNotifierImpl` (oneshim-vision). One toast per rollback event, deduplicated by `RollbackInfo.rolled_back_at`.

Toast copy (ko):
- **When both dates present**: "ONESHIM 업데이트 안내 — v{from} ({from_date} 배포) 설치 문제로 v{to} ({to_date} 배포)로 복구되었습니다."
- **When either date absent (`None`)**: "ONESHIM 업데이트 안내 — v{from} 설치 문제로 v{to}로 복구되었습니다." (drop the date parenthetical for the missing side, or both if both missing)

Format decision made at the toast composition site based on `RollbackInfo.from_published_at.is_some() && .to_published_at.is_some()`.

---

## 6. Testing Matrix & External Impact

### 6.1 Test totals

| Level | Scope | Count (new) |
|---|---|---|
| Unit — D9 | `verify_signature_accepts_builtin_key` / `..._second_trusted_key_when_first_inactive` / `..._fallback_to_configured_key_when_not_in_array` / `..._rejects_payload_when_no_key_matches` | 4 |
| Unit — D10 | `update_check_respects_rollout_exclusion` / `update_check_without_installation_id_is_excluded` | 2 |
| Unit — D11 probe | 6 tests enumerated in §4.7 Unit block | 6 |
| Unit — D11 non-fatal | `probe_io_error_is_non_fatal` | 1 |
| Integration — D11 (`src-tauri/tests/`) | `rollback_e2e_restores_previous_binary` | 1 |
| Platform CI matrix | macOS/Linux inline swap + Windows helper (gated on §4.8 spike result) | 2 rows |
| E2E smoke (`release-reliability-smoke.sh`) | Post-install probe trigger validation | 1 step |
| **Total** | | **13 unit + 1 integration = 14 tests + 2 CI rows + 1 smoke step** |

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

### 6.3 `cliff.toml` modification (diff against existing)

The repo ships an existing `cliff.toml` (~30 lines). This work **amends** the `[changelog]` `body` template. It does **not** overwrite the existing `[git]` section or postprocessors. Current body (simplified):

```text
{% if version %}
## [{{ version | trim_start_matches(pat="v") }}] - {{ timestamp | date(format="%Y-%m-%d") }}
{% else %}
## [Unreleased]
{% endif %}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group }}
{% for commit in commits %}- {{ commit.message | upper_first }}{% if commit.breaking %} [**BREAKING**]{% endif %}
  {%- if commit.body %}{{ commit.body | trim | indent(first=true, prefix="  ") }}{% endif %}
{% endfor %}
{% endfor %}
```

**Amended body** (adds two lines of metadata after the `## [...]` header when `version` is present):

```text
{% if version %}
## [{{ version | trim_start_matches(pat="v") }}] - {{ timestamp | date(format="%Y-%m-%d") }}

**Release Date:** {{ timestamp | date(format="%B %d, %Y UTC") }}
{% if previous and previous.version %}**Since {{ previous.version }}:** {{ commits | length }} commits · {{ contributors | length }} contributors{% endif %}

{% else %}
## [Unreleased]
{% endif %}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group }}
{% for commit in commits %}- {{ commit.message | upper_first }}{% if commit.breaking %} [**BREAKING**]{% endif %}
  {%- if commit.body %}{{ commit.body | trim | indent(first=true, prefix="  ") }}{% endif %}
{% endfor %}
{% endfor %}
```

**Git-cliff variable availability** (verify at implementation time via `git cliff --version` + a local dry run on a sample tag range):
- `previous.version` — available when a prior tag exists; guarded with `{% if previous and previous.version %}` for initial release edge case.
- `contributors` — available in git-cliff ≥ 1.4 as a list of unique commit authors. **Fallback**: if the CI's git-cliff version is older, omit the "Since v…" line entirely (guard via `{% if previous and previous.version and contributors %}` or check at release.yml step before invoking git-cliff).
- **Not available natively in git-cliff**: "PRs" count, "files changed" count. The original proposal phrased "N commits · M PRs · K files changed" is not directly expressible in git-cliff templates. Scope reduced to **commits · contributors**.

**§1.1 acceptance amendment**: "Since v{prev}: N commits · M contributors" (not "M PRs · K files changed").

`release.yml` `release_notes.md` header is prepended with one additional line before `## What's Changed`:
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
5. **Users on v0.4.N-1 or earlier**: their client has only the old (now-removed) key and will reject the hotfix. They must re-install manually from a signed installer download. Out-of-band trust anchors per platform:
   - **macOS**: Apple codesign + (when fixed) notarization — Gatekeeper validates the DMG/PKG signature.
   - **Windows**: GitHub Release SHA-256 published on the release page; users verify via PowerShell `Get-FileHash`. No Authenticode codesign currently.
   - **Linux**: GitHub Release SHA-256; the signed `.sig` file is produced by the *new* private key so it won't validate against the now-removed old key — users must **manually** trust the SHA-256 + provenance attestation (`actions/attest-build-provenance`) from `release.yml:1152-1155`.
   Notify via release notes + external channels (GitHub Discussions, Discord if present, email if on file).
6. Revoke the compromised signing key at the CI secret level (rotate GitHub token access).

Runbook detail: `docs/guides/updater-key-rotation.md`.

### 7.4 Release-body rollout gate usage

GitHub Release body editing updates the `<!-- rollout:N -->` comment. Clients pick up the change at their next 24h check cycle. Emergency stop: set `<!-- rollout:0 -->` to block all new downloads; combined with D11 auto-rollback in already-distributed clients, this gives a two-layer kill switch.

---

## 8. Summary

### 8.1 Files touched

- **Modified (10)**: `src-tauri/src/updater/{mod.rs,install.rs}`, `app_runtime_launch.rs`, `scheduler/mod.rs`, `update_coordinator.rs`, `oneshim-api-contracts/src/update.rs`, `oneshim-core/src/config/sections/storage.rs` (validate_integrity_policy relaxation), frontend `UpdateStatusPanel.tsx` + related, `.github/workflows/release.yml`, `cliff.toml` (body template amendment per §6.3), `CHANGELOG.md` (manual release-note entry at publish time).
- **Created (5)**: `src-tauri/src/updater/health_probe.rs`, `src-tauri/src/updater/trusted_keys.rs`, `docs/guides/updater-rollout.md`, `docs/guides/updater-key-rotation.md`, `docs/guides/updater-rollback-windows.md` (deliverable of §4.8 spike).

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

- All tests pass: new **14 tests** listed in §6.1 + existing suite unchanged.
- `cargo clippy --workspace --all-targets -- -D warnings` zero warnings.
- `cargo fmt --check` clean.
- Manual smoke: install v0.4.40-rc.1 locally; kill the process twice within 30 seconds of startup; third launch triggers rollback; UI shows `RolledBack` state; toast notification appears; `execute_rollback` restores the backup binary recorded in `.install_pending_{VERSION}.backup_path`.
- Release body at `https://github.com/pseudotop/oneshim-client/releases/tag/v0.4.40-rc.1` includes the `**Release Date:** …` + `**Since v0.4.39:** …` headers from cliff.toml template.
- `PendingUpdateInfo.published_at`, **when present**, is rendered in the frontend's update panel in the format "v{VERSION} (YYYY-MM-DD 배포)". When `None`, renders "v{VERSION}" alone with the `update.releaseDateUnknown` tooltip key.
- Windows platform CI row (`release-reliability-smoke.ps1` or equivalent) exercises the rollback path per §4.8 spike result. **Caveat**: if the spike falls back to `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)`, the Windows CI row asserts only that the deferred rename was scheduled (e.g., registry key `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\PendingFileRenameOperations` contains the expected entry), not that rollback visibly completes — it can't, because that requires a real OS reboot outside the CI runner's scope.
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
