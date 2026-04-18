# Phase 4 Updater Hardening Design

**Date:** 2026-04-18
**Scope:** D9 (signature verification enablement) + D10 (staged rollout verification) + D11 (post-install self-healthy probe with auto-rollback), bundled as a single PR.
**Target version:** v0.5.0 (breaking change: `require_signature_verification: true` by default).
**Dependencies:** Builds on v0.4.39-rc.1 (current RC). Assumes v0.4.39 stable is promoted first.

---

## 1. Goals & Scope

### 1.1 Goals

Close three paths through which a bad release can reach all users persistently:

1. **Tampered binary acceptance** — MITM or hash collision swaps a malicious binary past SHA256 check (addressed by **D9** enforced signature verification).
2. **Immediate 100% distribution** — a single bad release reaches all opt-in users instantly (addressed by **D10** staged rollout verification + authoring convention).
3. **Irrecoverable crash loop** — installed bad release fails to start, user must manually intervene (addressed by **D11** self-healthy probe + automatic rollback).

Enrich release metadata with **release date information** across four display layers (A-4):
- `CHANGELOG.md` header (already present, retained)
- `cliff.toml`-generated release body (new — "Release Date:" + "Since vX.Y.Z:" stats)
- GitHub Release name (new — "ONESHIM Client v0.5.0-rc.1 — Released April 18, 2026")
- Client updater UI (new — `PendingUpdateInfo.released_at` field, "v0.5.0-rc.1 (2026-04-18 배포)")

### 1.2 Non-goals

- **macOS notarization pipeline fix** — out of scope; pre-existing pipeline bug (`notarize-macos-release-assets.yml` head_branch condition always false). Separate infrastructure PR.
- **Nightly channel activation** — `UpdateChannel::Nightly` variant remains in code as internal-only; no user-facing surface, no release automation.
- **Apple codesign / Tauri code-signing changes** — already wired and working.
- **AWS Bedrock / SigV4** — C5, deferred to separate Phase 4 candidate.
- **Telemetry for rollout cohort observability** — relies on `telemetry` feature; separate follow-up.
- **Server-side release targeting** — client-side FNV-1a hash only.
- **Delta update complications** — `updater/delta.rs` unchanged.
- **Downgrade-by-user-request UI** — automatic rollback only; no manual downgrade control surface.

### 1.3 Scope boundary

Files modified:
- `src-tauri/src/updater/*.rs`
- `src-tauri/src/app_runtime_launch.rs`
- `src-tauri/src/update_coordinator.rs`
- `src-tauri/src/scheduler/loops/health.rs` (rollback trigger integration)
- `crates/oneshim-api-contracts/src/update.rs` (`UpdatePhase::RolledBack` variant, `PendingUpdateInfo.released_at`, `RollbackInfo`, `RollbackReason`)
- `crates/oneshim-core/src/config/sections/storage.rs` (`UpdateConfig` defaults)
- `crates/oneshim-core/src/config_manager.rs` (migration for legacy configs)
- `crates/oneshim-web/frontend/src/...` (UI rendering for rollback state + released_at)
- `.github/workflows/release.yml` (release_notes.md header)

Files created:
- `src-tauri/src/updater/health_probe.rs` (self-healthy module)
- `src-tauri/src/updater/public_key.rs` (trusted public keys array)
- `cliff.toml` (git-cliff template for release body)
- `docs/guides/updater-rollout.md` (authoring convention for `<!-- rollout:N -->`)
- `docs/guides/updater-key-rotation.md` (operations manual)

---

## 2. D9 — Ed25519 Signature Verification Enablement

### 2.1 Already implemented

- Ed25519 signature verification code at `install.rs:217-260` using `ed25519_dalek`.
- Release artifact signing pipeline at `release.yml:1113-1149` using PyNaCl `SigningKey` driven by `SIGNING_SEED` GitHub Secret; produces `.sig` files next to every artifact except `.sha256` / `.sig`.
- Two unit tests at `mod.rs:831-858`.
- `rehearse-key-rotation.sh` script exists for operational practice.

### 2.2 Gap closure

#### 2.2.1 Multi-key trust array (supersedes single hardcoded key)

New file `src-tauri/src/updater/public_key.rs`:

```rust
/// Trusted Ed25519 verification keys for signed update artifacts.
///
/// Add new keys at the TOP of the array during rotation windows; remove
/// deprecated keys only after a grace-period release gap (at minimum one
/// release cycle after the new key is distributed).
pub(crate) const TRUSTED_PUBLIC_KEYS: &[&str] = &[
    // v1 — introduced 2026-04-18, initial production key
    // Derived from SIGNING_SEED via: python3 -c "from nacl.signing import SigningKey; import base64; print(base64.b64encode(SigningKey(bytes.fromhex('<SEED>')).verify_key.encode()).decode())"
    "PLACEHOLDER_BASE64_KEY_REPLACED_DURING_IMPLEMENTATION==",
];
```

`verify_signature` in `install.rs` iterates this array, accepting the signature if any listed key validates. Fails with a single `Integrity` error if no key matches.

Rationale: Day-1 rotation support avoids a trapped-trust situation if the private key is compromised or regular rotation is needed.

#### 2.2.2 Config defaults flip

`crates/oneshim-core/src/config/sections/storage.rs`:
```rust
// Change Default impl:
require_signature_verification: true,   // was false
signature_public_key: String::new(),    // resolved at runtime via public_key.rs; legacy field retained for deserialization compat
```

Runtime behavior: `install.rs:verify_signature` prefers the `TRUSTED_PUBLIC_KEYS` array; `signature_public_key` config field is ignored going forward (stays in serialization surface only to not break on existing configs).

#### 2.2.3 Config migration

`crates/oneshim-core/src/config_manager.rs` adds `migrate_update_config`:
- Load existing config JSON.
- If `update.require_signature_verification == false`, force-overwrite to `true`.
- Log at `tracing::info` level.
- Called on every config load (idempotent — if already true, no-op).

This is a **breaking change**, justified under semver 0.x.y rules by the 0.4 → 0.5 minor bump.

CHANGELOG headline for v0.5.0:
> 🔒 **Breaking:** Signed update verification is now required by default. Automatic migration sets `require_signature_verification: true` on existing configurations.

### 2.3 Tests

- Existing: `verify_signature_accepts_valid_ed25519_signature`, `verify_signature_rejects_wrong_signature` (mod.rs:831-858).
- **Added (2):**
  - `verify_signature_accepts_second_trusted_key` — array has old + new key, payload signed with new key validates.
  - `config_migration_forces_signature_verification_true` — raw JSON with `require_signature_verification: false` → after migration, `true`.

---

## 3. D10 — Staged Rollout Verification

### 3.1 Already implemented

- `is_eligible_for_rollout()` at `mod.rs:312-322` (FNV-1a hash of `installation_id + version`).
- `parse_rollout_percent()` at `mod.rs:327-338` (parses `<!-- rollout:N -->` from release body, defaults to 100).
- `installation_id` auto-UUID generation on first launch at `app_runtime_launch.rs:66-74`.
- Update check applies rollout gate at `mod.rs:184-197`.
- Five existing unit tests.

### 3.2 Gap closure

#### 3.2.1 Defensive behavior for missing `installation_id`

Current behavior at `mod.rs:190-197` treats `installation_id: None` as always-eligible (bypasses rollout gate). This is unsafe — a config regression that clears the ID would place a user in the first-receive cohort unconditionally.

New behavior: **treat `None` as ineligible** (conservative default). The next launch regenerates the UUID via `app_runtime_launch.rs:66-74` and normal eligibility resumes.

```rust
let Some(ref installation_id) = self.config.installation_id else {
    tracing::warn!("installation_id missing — treating as rollout-excluded");
    return Ok(UpdateCheckResult::UpToDate { current });
};
if !is_eligible_for_rollout(installation_id, &latest_str, rollout_percent) {
    tracing::debug!("Device not in rollout bucket ({rollout_percent}%)");
    return Ok(UpdateCheckResult::UpToDate { current });
}
```

#### 3.2.2 Authoring convention document

New file `docs/guides/updater-rollout.md` (~100 lines):
- Exact syntax: `<!-- rollout:N -->` as an HTML comment in release body.
- Recommended progression: 5 → 25 → 50 → 100 over a week.
- Edit-after-publish behavior: clients pick up changes at next 24h check cycle.
- Absence of comment → 100% (backward compat).
- Determinism: same `(installation_id, version)` → same bucket across checks.

### 3.3 Tests

- Existing: `rollout_100_always_eligible`, `rollout_0_never_eligible`, `rollout_deterministic`, `parse_rollout_present`, `parse_rollout_absent`.
- **Added (2):**
  - `update_check_respects_rollout_exclusion` — mock release body with `<!-- rollout:5 -->` + installation_id in excluded bucket → `UpToDate` result.
  - `update_check_without_installation_id_is_excluded` — confirms defensive None handling.

---

## 4. D11 — Post-Install Self-Healthy Probe & Auto-Rollback

This is the largest new implementation chunk (~500 LOC, ~3 days).

### 4.1 Decision summary

| Decision | Value |
|---|---|
| Failure signal | Self-healthy marker pattern (D) |
| Healthy threshold | 30 seconds of continuous wall-clock uptime from scheduler-boot completion (C) |
| Failed boot tolerance | 2 consecutive failures without an intervening success marker (success resets counter to 0) (B) |
| Notification | `UpdatePhase::RolledBack` (UI) + toast + telemetry |

### 4.2 State machine

```
[new install or update complete]
    ↓
[app boot N=1]
    ↓ 30s uptime reached?
    ├── yes → write .self_healthy_v{VERSION} → normal boot, committed
    └── no (crash / exit) → increment .boot_count_v{VERSION} → next boot detects
              ↓
[app boot N=2]
    ↓ .self_healthy_v{VERSION} already present?
    ├── yes → normal (previous cycle succeeded)
    └── no → retry 30s uptime probe
              ↓ 30s uptime reached?
              ├── yes → write marker → committed
              └── no (crash) → boot_count = 2 → next boot triggers rollback
                        ↓
[app boot N=3 pre-flight]
    ↓ boot_count ≥ 2 AND no .self_healthy_v{VERSION}?
    └── yes → execute_rollback(backup_binary, previous_version) → restart
```

### 4.3 State files

Stored in the **install directory** (not app data), because the install directory is owned by the installer and has atomic write guarantees. Config.json in app data can be mutated by app logic during a failing startup.

| File | Content | Lifecycle |
|---|---|---|
| `.install_pending_{VERSION}` | JSON: `{ installed_at: ISO-8601, previous_version: string }` | Created at install completion; deleted when `.self_healthy_{VERSION}` is written |
| `.boot_count_{VERSION}` | uint32 as plain text (atomic rename-write) | Incremented on every startup; reset to 0 when marker is written |
| `.self_healthy_{VERSION}` | ISO-8601 timestamp | Written after 30s uptime; persists until next version's install |

### 4.4 New module `src-tauri/src/updater/health_probe.rs`

```rust
use std::path::PathBuf;
use std::time::Duration;

pub struct HealthProbe {
    install_dir: PathBuf,
    current_version: String,
    healthy_threshold: Duration,          // default 30s
    failed_boot_threshold: u8,            // default 2
}

pub enum StartupAction {
    Normal,
    RollbackRequired {
        from_version: String,
        to_version: String,
        reason: RollbackReason,
    },
}

#[derive(Debug)]
pub enum ProbeError { /* thiserror variants */ }

impl HealthProbe {
    pub fn new(install_dir: PathBuf, current_version: String) -> Self { ... }

    /// Called synchronously at startup BEFORE the main scheduler boots.
    /// Returns RollbackRequired if threshold exceeded.
    pub fn check_startup_state(&self) -> Result<StartupAction, ProbeError> {
        // 1. If no .install_pending_{VERSION} exists → fresh install or normal boot → Normal.
        // 2. If .self_healthy_{VERSION} exists → already validated → Normal.
        // 3. Increment .boot_count_{VERSION}.
        // 4. If boot_count >= failed_boot_threshold → read .install_pending_{VERSION} for from/to → RollbackRequired.
        // 5. Otherwise → Normal (try again).
    }

    /// Spawn a tokio background task that waits `healthy_threshold` then writes the marker.
    /// Also deletes .install_pending_{VERSION} and .boot_count_{VERSION}.
    pub fn spawn_healthy_writer(&self) -> tokio::task::JoinHandle<()> { ... }
}
```

### 4.5 Integration points

| Location | Change |
|---|---|
| `src-tauri/src/main.rs` or `app_runtime_launch.rs` | Call `HealthProbe::check_startup_state()` at earliest reasonable point in startup. On `RollbackRequired`, invoke `install::execute_rollback()` with `from/to` metadata, then exit. |
| `src-tauri/src/updater/install.rs` | On successful install completion, write `.install_pending_{VERSION}` with current timestamp + previous version string. |
| `src-tauri/src/scheduler/mod.rs` | After scheduler loops spawn, invoke `health_probe.spawn_healthy_writer()`. |
| `src-tauri/src/update_coordinator.rs` | Translate `RollbackRequired` into `UpdatePhase::RolledBack` broadcast + toast notification + telemetry event. |

### 4.6 Rollback execution (`install::execute_rollback`)

Reuses existing `.rollback.{ts}` backup binary pattern from `install.rs:391` + existing `install_and_restart_with_ops` flow.

Platform concerns:

| OS | Mechanism |
|---|---|
| macOS | In-process rename: `.rollback.{ts}` → `oneshim-app`; immediate restart via existing `restart_fn` pattern. Inode-based file handles preserve running process. |
| Linux | Same as macOS (inode-based). |
| Windows | Running executable cannot be replaced. Use shell helper pattern: spawn `cmd.exe /c "ping 127.0.0.1 -n 3 > nul & move /Y backup.exe oneshim-app.exe & start oneshim-app.exe"` — 3s delay for self-exit, then swap, then relaunch. |

### 4.7 Tests

**Unit (5):**
- `check_startup_no_pending_install_is_normal`
- `check_startup_with_healthy_marker_is_normal`
- `check_startup_below_threshold_is_normal`
- `check_startup_at_threshold_triggers_rollback`
- `spawn_healthy_writer_sets_marker_after_delay` (tokio paused time)

**Integration (1):**
- `rollback_e2e_restores_previous_binary` — temporary install dir + fake binaries + full cycle.

---

## 5. UI & UpdateStatus Extension

### 5.1 `oneshim-api-contracts::update` additions

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpdatePhase {
    Idle, Checking, PendingApproval, Downloading,
    ReadyToInstall, Installing, Updated, Deferred, Error,
    RolledBack,  // new
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PendingUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub released_at: Option<String>,  // new — ISO 8601 from GitHub Releases published_at
    // ... existing fields
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackInfo {
    pub from_version: String,
    pub from_released_at: Option<String>,
    pub to_version: String,
    pub to_released_at: Option<String>,
    pub reason: RollbackReason,
    pub rolled_back_at: String,  // ISO 8601
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RollbackReason {
    RepeatedStartupFailure,  // initial release only reason; additive enum for future
}

// UpdateStatus adds:
//   pub rollback: Option<RollbackInfo>,   // populated when phase == RolledBack
```

### 5.2 Frontend

| Touchpoint | Change |
|---|---|
| `UpdateStatusPanel.tsx` | Add `RolledBack` case showing from/to versions + dates + reason |
| Shared date formatter | Relative time for recent (<24h), absolute ISO for older |
| i18n (ko / en) | ~10 new keys: `update.rolledBack.title`, `update.rolledBack.reason.repeatedStartupFailure`, `update.releaseDate` |

### 5.3 Desktop Notification

Reuse existing `DesktopNotifierImpl` (oneshim-vision) port. Trigger once per rollback event (deduplicated by `rolled_back_at`).

Toast copy (ko):
> "ONESHIM 업데이트 안내 — {from_version} (2026-05-01 배포) 설치 문제로 {to_version} (2026-04-18 배포)로 복구되었습니다."

---

## 6. Config Defaults + Testing Matrix

### 6.1 Config migration

Covered in Section 2.2.3. One-directional force-overwrite. Idempotent.

### 6.2 Test matrix

| Level | Scope | Count (new) |
|---|---|---|
| Unit (per-section enumerated) | 2 (§2.3) + 2 (§3.3) + 5 (§4.7) = 9 | 9 |
| Integration (`src-tauri/tests/`) | 1 (§4.7 E2E rollback). Optional additions during implementation if gaps surface: multi-key signing, rollout exclusion | 1 baseline, up to 3 |
| Platform CI matrix rows | macOS/Linux inline swap, Windows shell helper swap | 2 rows |
| E2E smoke (`release-reliability-smoke.sh`) | Post-install probe trigger validation | 1 step |
| **Baseline total** | | **~10** tests + 2 CI rows + 1 smoke step |

### 6.3 External dependencies

| System | Impact | Action |
|---|---|---|
| `self_update` crate | None (wrapped only) | — |
| GitHub Releases API | `published_at` already fetched; new usage | Propagate to `PendingUpdateInfo` |
| `app_runtime_launch.rs` installation_id | None | — |
| CI release.yml | `cliff.toml` for body + release_notes.md header expansion | ~10 LOC |
| Cargo deps | None new (`ed25519_dalek`, `tokio` present) | — |

---

## 7. Release Timeline & Rollout Strategy

### 7.1 Version path

```
[now]  v0.4.39-rc.1 (CI in progress)
            ↓ RC validation (2-4 weeks)
v0.4.39 stable (promote-stable.sh 0.4.39-rc.1)
            ↓ Phase 4 Updater Hardening PR merges to main
v0.5.0-rc.1 ← Breaking change: require_signature_verification defaults true
            ↓ PreRelease channel opt-in users only, 2-4 weeks
            ↓ Collect rollback/signature-verify telemetry (target: 0 crashes, 0 false rollbacks in 4-week window)
v0.5.0 stable (promote-stable.sh 0.5.0-rc.1)
            ↓ <!-- rollout:5 --> initial canary (24-48h observation)
            ↓ <!-- rollout:25 --> (3-5d observation)
            ↓ <!-- rollout:50 --> → <!-- rollout:100 --> full public
```

### 7.2 Defence-in-depth layers

| Layer | Role | Active from |
|---|---|---|
| PreRelease channel opt-in | Early adopter pool, technical users | RC tag publish |
| `<!-- rollout:N -->` gating | Post-stable % phased public | Stable tag |
| D9 signature verification | MITM / tamper block | Every update, always |
| D11 self-healthy probe | Automatic recovery from bad builds | Post-install, 2 boot cycles |

### 7.3 Emergency procedures

**Critical regression detected:**
1. Edit GitHub Release body → `<!-- rollout:0 -->` → blocks all further distribution within 24h (client check interval).
2. Already-installed users: D11 probe auto-rollback after 2 failed boot cycles (no user action).
3. Ship hotfix RC → fast-track RC → stable.

**Suspected key compromise:**
1. Add new verification key at top of `TRUSTED_PUBLIC_KEYS` array (old key retained).
2. Ship emergency update signed with **new** key (old clients still validate via old key).
3. Following release, sign with new key only.
4. After 1-2 release gap, remove old key from array.

Runbook: `docs/guides/updater-key-rotation.md`.

### 7.4 Script audit findings

All existing release scripts (release.sh, promote-stable.sh, publish-rc-tag.sh, pre-release-check.sh, release-common.sh) are semver-agnostic via regex and support 0.5.x without modification.

**Independent bug found (separate small PR):**
- `pre-release-check.sh:241` — Dependabot-disabled 403 response is not trapped as non-integer; `[ "$ALERT_COUNT" -gt 0 ]` fails. Fix: guard with `[[ "$VAR" =~ ^[0-9]+$ ]]`.

---

## 8. Spec Summary & Estimates

### 8.1 Files touched

- Modified (10): 6 Rust files in src-tauri/crates + 1 workflow + 3 frontend
- Created (5): health_probe.rs, public_key.rs, cliff.toml, 2 guides

### 8.2 LOC estimate

| Section | LOC |
|---|---|
| D9 (multi-key + migration + tests) | ~110 |
| D10 (defensive None handling + tests + docs) | ~110 |
| D11 (probe + rollback + tests) | ~500 |
| UI / UpdateStatus | ~60 |
| Config defaults + tests | ~40 |
| cliff.toml + release.yml header | ~50 |
| Docs (rollout + key-rotation) | ~200 |
| **Total** | **~1,070** |

### 8.3 Effort

~4-5 calendar days of focused work. Multi-session execution expected.

### 8.4 Acceptance criteria

- All unit + integration tests pass.
- `cargo clippy --workspace --all-targets -- -D warnings` zero.
- `cargo fmt --check` clean.
- Manual CI release workflow produces signed `.sig` files per artifact (already working).
- Manual smoke: on v0.5.0-rc.1 install, forcibly crash the app twice within 30s of startup → third launch performs rollback → verify `UpdatePhase::RolledBack` emitted + toast shown.
- Config migration: legacy config with `require_signature_verification: false` → after load, value is `true`.
- Release body includes `**Release Date:** April DD, 2026` + `**Since v0.4.38:** N commits · M PRs` headers.
- Frontend `UpdateStatusPanel` renders rollback state with from/to + dates + reason.

### 8.5 Deferred follow-ups

- **pre-release-check.sh:241 bug fix** — separate 5-minute PR.
- **Notarization workflow trigger fix** — separate infrastructure PR.
- **Telemetry for rollout cohort observability** — after telemetry feature stabilizes.
- **Nightly channel official activation** — product decision; enum variant stays hidden.
- **Additional `RollbackReason` variants** (e.g., `HealthCheckTimeout`, `ExplicitUserRequest`) — additive; out of v0.5.0 scope.

---

## 9. Open Questions

None at design-time. All user decisions captured in Sections 1-7. Answer points to verify during implementation:

1. Exact public key base64 — derive from `SIGNING_SEED` secret during implementation; placeholder in spec.
2. Exact Windows shell helper command — verify `cmd.exe` quoting works across Windows 10/11 shells.
3. `install_dir` resolution — Tauri `AppHandle::path().app_data_dir()` vs `std::env::current_exe()` parent — use the latter to match where backup is already written.

---

*End of design document.*
