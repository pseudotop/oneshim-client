# Phase 4 Updater Hardening — Implementation Plan

**Spec**: `docs/reviews/2026-04-18-phase4-updater-hardening-design.md` (Loop 1 EXIT at `1065cb56`)
**Target version**: v0.4.40-rc.1
**Bundling**: single PR
**Branch base**: branch from `main` AFTER v0.4.39 stable is promoted. Work continues on `feat/phase4-updater-hardening-spec` (which is a misnomer — keep this branch name for now; rename opportunity is end of Task 13 before PR creation).
**Total effort**: ~1,030 LOC across 14 tasks, ~3-4 days + 1 Windows spike day.

---

## Spec amendments approved during Loop 2 (non-substantive)

The following amendments to the spec were approved during plan writing and do not alter design intent. They are recorded here + applied to the spec in the task where they become relevant:

- **Amendment 1 (applied in Task 1)**: `HealthProbe::spawn_healthy_writer` signature changes from `self` (by value) to `&self`. Rationale: `check_startup_state` already uses `&self`, and the scheduler needs to call `spawn_healthy_writer` after boot while the launch path may still reference the probe. This simplifies ownership — a single probe instance is created in launch, passed by `Arc` to the scheduler for spawning, and dropped at app shutdown. Spec §4.4 signature line updated accordingly in the Task 1 commit.

---

## Plan discipline (applies to all tasks)

### Commit + push cadence

Commit + push per task. Motivation: mid-PR machine crash or review preemption should not lose progress. Each task ends with `git push origin feat/phase4-updater-hardening-spec` (current branch name — to be renamed before PR creation if desired).

### Bug-discovery policy (Phase 5-D8 precedent)

If implementing a task surfaces a pre-existing bug:
- **≤20 LOC fix**: land in-PR with a separate commit and note in CHANGELOG.
- **>20 LOC fix OR scope-creep**: file a follow-up issue; add a TODO comment at the site with reference; continue.

### SKIP markers

Task 0 audit may reveal redundant steps (e.g., if later commits add a helper that this plan proposes to add). Mark such steps `[SKIP — ALREADY PRESENT per audit: <reference>]` during Task 0, do not run them.

### Zero-gap escape hatch

If a task's audit shows all its concerns are already covered, record the audit outcome and skip the implementation; the task remains a "done" entry in commit history with an audit-only commit.

### Verification per task

At task close:
```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p <relevant-crate> --lib
```

Full workspace test runs only at Task-group boundaries (after D9, D10, D11 complete).

---

## Task 0 — Audit baseline + dependency verification

**Goal**: establish baseline test count, verify all referenced APIs + files exist in their spec'd shapes, dry-run cliff.toml, confirm Phase 2 telemetry surface.

### Steps

1. **Test baseline**: `cargo test --workspace 2>&1 | awk '/^test result:/ {p+=$4; i+=$8} END {print "passed=", p, "ignored=", i}'`. Record the numbers as "pre-Phase-4 baseline" in progress tracker.

2. **Verify spec-referenced code locations**:
   - `install.rs:378-392` — `backup_path_for` formatter exists and produces `{binary_name}.rollback.{ts}`.
   - `install.rs:217-260` — `verify_signature` current shape.
   - `install.rs:407-408` — `replace_binary` call site in `install_and_restart_with_ops`.
   - `crates/oneshim-core/src/config/sections/storage.rs:349-355` — `default_update_require_signature` returns `true`; default public key is `"GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E="`. (Same file for all subsequent `storage.rs:...` references below.)
   - `crates/oneshim-core/src/config/sections/storage.rs:235-280` — `validate_integrity_policy` current requirements.
   - `update.rs:25` — `published_at: Option<String>` field present.
   - `update_coordinator.rs:446` — `published_at` propagation.
   - `app_runtime_launch.rs:66-74` — installation_id auto-generation.
   - `mod.rs:831-858` — existing verify_signature tests.
   - `mod.rs:1366-1391` — existing rollout tests.

3. **Phase 2 telemetry surface probe**: search for `telemetry::` and `otel::` usage in `src-tauri/src/**/*.rs`. Record whether a counter API is public, or whether only span events are available. Decision:
   - If public counter API: use it for `updater.installation_id_missing_at_scheduler_start`.
   - If only span events: emit a span event; note in plan Task 3 that counter line stays commented out.
   - If neither: keep the `tracing::error!` only; counter line remains `// TODO` per spec §3.3.2.

4. **cliff.toml dry-run**: generate a CHANGELOG for a **fixed tag range** `v0.4.38..v0.4.39-rc.1` using the current `cliff.toml`. Save output to `/tmp/cliff-baseline.md`. Command: `git cliff v0.4.38..v0.4.39-rc.1 > /tmp/cliff-baseline.md`. **Do NOT use `--unreleased`** here — that would track the moving working tree and make the Task 11 diff unfocused. The fixed range is the diff anchor for Task 11.

5. **tempfile dependency verification**: `grep tempfile src-tauri/Cargo.toml` — confirm presence. Note: `tempfile` is currently in `[dependencies]` (not `[dev-dependencies]`); this means the crate is shipped in release builds. Not a Phase 4 blocker but worth noting. If a follow-up wants to move it to `[dev-dependencies]`, file a housekeeping ticket (out of this PR's scope).

6. **Commit**: `docs(phase4): Task 0 — audit baseline` with progress update in `.claude/phase4-progress.md`.

### Deliverables

- `.claude/phase4-progress.md` Task 0 section filled in with baseline test count, telemetry-API decision, cliff dry-run location.
- A saved `/tmp/cliff-baseline.md` for Task 11 diff reference.

### Commit message

```
docs(phase4): Task 0 — audit baseline + telemetry surface verification

Pre-Phase-4 baseline: N tests passed, M ignored.
Telemetry API decision: <counter | span event | tracing only>.
Cliff baseline saved to /tmp/cliff-baseline.md for Task 11 diff.

All spec-referenced code locations verified:
- install.rs backup_path_for at L378-392 ok
- install.rs verify_signature at L217-260 ok
- storage.rs defaults at L349-355 ok
- [etc.]
```

---

## Task 1 — `updater/` module layout + stub files + spec amendment

**Goal**: add `trusted_keys.rs` + `health_probe.rs` as stub modules wired into `updater/mod.rs`; apply the Loop-2-approved spec Amendment 1 (signature change `self` → `&self` on `spawn_healthy_writer`).

### Steps

1. Check current `src-tauri/src/updater/` structure (`mod.rs`, `install.rs`, `github.rs`, `delta.rs`, `state.rs` per spec §2.1).

2. Add `src-tauri/src/updater/trusted_keys.rs`:
   ```rust
   //! Trusted Ed25519 verification keys for update artifacts.
   //!
   //! Add new keys to the TOP of this array when rotating. Remove
   //! deprecated keys only as part of a compromise response (see
   //! docs/guides/updater-key-rotation.md). Normal rotation retains
   //! old keys across 1-2 release cycles.
   pub(crate) const TRUSTED_PUBLIC_KEYS: &[&str] = &[
       // v1 — introduced 2026-04-18, production key since v0.4.x
       // (identical to the default at storage.rs:354)
       "GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E=",
   ];
   ```

3. Add `src-tauri/src/updater/health_probe.rs` with module skeleton only (public types + `new` / `with_threshold` / public method signatures; body `todo!()` (clippy-allowed in stub phase, replaced in Task 5)):
   - `pub struct HealthProbe { ... }`
   - `pub enum StartupAction { Normal, RollbackRequired { ... } }`
   - `pub enum RollbackReason { RepeatedStartupFailure }`
   - `pub enum ProbeError { ... }` (thiserror)
   - `pub fn new`, `pub fn with_threshold`, `pub fn check_startup_state(&self) -> StartupAction`, **`pub fn spawn_healthy_writer(&self) -> tokio::task::JoinHandle<()>`** (signature amendment — receives `&self`, not `self`; Task 1 applies Amendment 1 per front-matter note).
   - Add `#[allow(clippy::todo)]` on the stub fn bodies; remove in Task 5.

4. **Apply spec Amendment 1** to `docs/reviews/2026-04-18-phase4-updater-hardening-design.md` §4.4: change `pub fn spawn_healthy_writer(self) -> ...` to `pub fn spawn_healthy_writer(&self) -> ...`. Add a one-line annotation in §4.4 referencing "Amendment 1 applied in Task 1 plan commit".

5. Add `mod trusted_keys;` + `mod health_probe;` declarations in `updater/mod.rs`.

6. Re-export `health_probe::{HealthProbe, StartupAction, RollbackReason, ProbeError}` from `updater/mod.rs`.

7. Run `cargo check -p oneshim-app` + `cargo clippy -p oneshim-app --all-targets -- -D warnings -A clippy::todo` (temporary clippy allow for stubs). Compile must succeed. No new tests yet.

### Commit message

```
feat(updater): add trusted_keys + health_probe module skeletons

Scaffolding for Phase 4 D9 + D11 implementation. No behavior — tests
come in Task 2 (D9) and Task 5 (D11).

- trusted_keys.rs: TRUSTED_PUBLIC_KEYS array with the single v1
  production key (identical to storage.rs:354 default).
- health_probe.rs: HealthProbe struct + StartupAction / RollbackReason
  / ProbeError enums + unimplemented!() bodies.

cargo check passes. No new tests.
```

---

## Task 2 — D9 multi-key `verify_signature` refactor + validate_integrity_policy relaxation

**Goal**: replace `verify_signature` body per spec §2.3; relax `validate_integrity_policy` per spec §2.3.1.

### Steps

1. In `install.rs::verify_signature`:
   - Extract existing body into a private helper `try_verify_with_key_b64(key_b64: &str, payload: &[u8], sig: &[u8]) -> Result<(), UpdateError>`.
   - Rewrite `verify_signature` per spec §2.3: loop over `TRUSTED_PUBLIC_KEYS` first; fallback to `config.signature_public_key` only when non-empty AND not in the array.

2. In `storage.rs::validate_integrity_policy`:
   - Remove the early `if self.signature_public_key.is_empty() { return Err(...) }` check (the builtin array now carries trust).
   - Preserve the base64 + 32-byte validation **only when the field is non-empty** (opt-in override validation).

3. Add tests in `mod.rs` at the existing test location (around line 831):
   - `verify_signature_accepts_builtin_key` — derive a payload signed with the seed matching `TRUSTED_PUBLIC_KEYS[0]` (use the `UPDATE_SIGNING_PRIVATE_KEY_B64` test fixture pattern from Phase 2 telemetry tests if available; otherwise generate fresh keypair at test time and temporarily override `TRUSTED_PUBLIC_KEYS` via a test-only indirection helper — see §Test helpers below).
   - `verify_signature_accepts_second_trusted_key_when_first_inactive` — two keys in array, payload signed with second.
   - `verify_signature_fallback_to_configured_key_when_not_in_array` — configured key is genuinely different from builtin; validates via fallback branch.
   - `verify_signature_rejects_payload_when_no_key_matches` — random key; expect `Integrity` error.
   - `validate_integrity_policy_allows_empty_public_key` — config with empty `signature_public_key` no longer errors when updates are enabled.

4. **Test helper note**: if `TRUSTED_PUBLIC_KEYS` is truly a `const`, tests can't swap it. Options:
   - (a) Promote to `pub(crate) static TRUSTED_PUBLIC_KEYS: &[&str]` with a `#[cfg(test)]` alternate array — minor API asymmetry.
   - (b) Let tests construct a private `verify_signature_with_keys(&[&str], ...)` inner fn and test that directly; public `verify_signature` passes the const.
   - **Choose (b)** — cleaner separation, no production-code `#[cfg(test)]` gates.

5. Run `cargo test -p oneshim-app --lib updater`, `cargo fmt`, `cargo clippy -p oneshim-app --all-targets -- -D warnings`.

### Commit message

```
feat(updater): D9 multi-key signature trust array

Invert verify_signature precedence: built-in TRUSTED_PUBLIC_KEYS array
is walked first; configured signature_public_key is consulted only as a
fallback when non-empty AND different from any built-in key. This makes
day-1 key rotation effective.

Relax validate_integrity_policy: signature_public_key is no longer
required to be non-empty (the built-in array is authoritative); when
provided, base64 + 32-byte validation still applies.

5 new unit tests (§6.1 enumeration). Existing tests at mod.rs:831-858
kept. cargo test --lib green.
```

---

## Task 3 — D10 defensive None handling + spawn-order guard + tests

**Goal**: spec §3.3.1 + §3.3.2 implemented with tests.

### Steps

1. In `updater/mod.rs::check_for_updates_from` around line 184:
   - Replace the current rollout gate (`if let Some(ref installation_id) = ...`) with the `let Some(ref installation_id) = ... else { ... return UpToDate }` pattern per spec.
   - Ensure the `tracing::warn!` log is emitted on the None branch.

2. In `src-tauri/src/scheduler/mod.rs` at the update-check spawn site:
   - Add the block from spec §3.3.2 with `tracing::error!`, the commented-out telemetry line (with `// TODO(plan Task 0 resolved: <decision>)` referring to Task 0 result), and `debug_assert!(false, ...)` with the dual-build comment.

3. Add tests in `updater/mod.rs` tests module:
   - `update_check_respects_rollout_exclusion` — mock GitHub API release with body `<!-- rollout:1 -->`. Choose an `installation_id` whose FNV-1a hash modulo 100 is > 1 (hardcode a UUID that hashes accordingly; discover via a one-off helper). Expect `UpdateCheckResult::UpToDate`.
   - `update_check_without_installation_id_is_excluded` — config with `installation_id = None`. Expect `UpdateCheckResult::UpToDate` + warn log captured.

4. Run `cargo test -p oneshim-app --lib updater`, fmt, clippy.

### Commit message

```
feat(updater): D10 defensive rollout None handling + spawn-order guard

check_for_updates_from now treats installation_id: None as
rollout-excluded (was: always-eligible). This makes a config regression
visible via tracing::warn! instead of silently admitting the device to
the first-receive cohort.

scheduler/mod.rs update-check spawn site emits tracing::error! +
debug_assert!(false) when installation_id is None — the invariant is
guaranteed by app_runtime_launch.rs:66-74, but production regressions
are now observable. Telemetry path: <per Task 0 decision> (symmetric
with Task 9's update_coordinator handler choice).

2 new unit tests. cargo test --lib green.
```

---

## Task 4 — D10 `docs/guides/updater-rollout.md` authoring convention

**Goal**: ~100-line documentation file for release authors.

### Steps

1. Create `docs/guides/updater-rollout.md` with sections:
   - Exact `<!-- rollout:N -->` syntax
   - Recommended progression 5 → 25 → 50 → 100 with observation gates
   - Missing-comment fallback behavior
   - Edit-after-publish behavior
   - Determinism contract
   - Emergency stop (`rollout:0`)

2. Also add or update a line in `CONTRIBUTING.md` (if present) or `docs/README.md` linking to the new guide.

3. No code changes; only doc.

### Commit message

```
docs(updater): add staged rollout authoring convention guide

Documents the <!-- rollout:N --> HTML comment convention that the
client FNV-1a rollout gate (mod.rs:327-338) parses from GitHub Release
bodies.

Covers:
- Syntax + placement
- Recommended progression 5 → 25 → 50 → 100
- Default-100 behavior when the comment is absent
- Edit-after-publish pickup (24h check cycle)
- Determinism (same installation_id + version → same bucket)
- Emergency stop via rollout:0
```

---

## Task 5 — D11 health_probe.rs core logic + 7 unit tests

**Goal**: replace Task 1 stubs with real implementation of `check_startup_state_inner` + `spawn_healthy_writer`; add all 6 spec §4.7 unit tests + the non-fatal contract test.

### Steps

1. Implement `check_startup_state_inner` per spec §4.4 step list (order 0 → 1 → 2 → 3 → 4 → 5). State-file format per §4.3.

2. Implement `spawn_healthy_writer`: tokio task waits `healthy_threshold`, then writes `.self_healthy_{VERSION}` + deletes `.install_pending_{VERSION}` + `.boot_count_{VERSION}` + cleans `{binary_name}.rollback.{ts}` except the one listed in `backup_path`.

3. Wrap the public `check_startup_state` per §4.4 non-fatal contract (catches all `ProbeError` → `tracing::warn!` + return `Normal`).

4. Add tests (use `tempfile::tempdir()` for real filesystem + `HealthProbe::with_threshold(Duration::from_millis(50))` for the healthy_writer timing test):
   - `check_startup_no_pending_install_is_normal`
   - `check_startup_with_healthy_marker_is_normal`
   - `check_startup_below_failed_boot_threshold_is_normal`
   - `check_startup_at_failed_boot_threshold_triggers_rollback`
   - `stale_install_pending_older_than_24h_returns_normal_without_rollback`
   - `spawn_healthy_writer_sets_marker_after_injected_short_delay`
   - `probe_io_error_is_non_fatal`

5. `cargo test -p oneshim-app --lib updater::health_probe`, fmt, clippy.

### Bug-discovery expectation

`tempfile` is already in `src-tauri/Cargo.toml [dependencies]` per Task 0 audit step 5 — no addition needed. If Task 0 surfaced the "tempfile in [dependencies] vs [dev-dependencies]" concern and filed a housekeeping ticket, proceed without moving it (out of Phase 4 scope).

### Commit message

```
feat(updater): D11 health probe module + 7 tests

Implement HealthProbe::check_startup_state_inner per spec §4.4 step
list: (0) staleness check → (1-2) fresh-install / already-healthy
short-circuit → (3) read boot_count → (4) threshold check → (5)
increment after threshold check. Staleness rule (§4.3) is step 0 to
prevent phantom rollback after same-version manual reinstall.

spawn_healthy_writer waits healthy_threshold (default 30s; injectable
via with_threshold for tests) then writes self_healthy marker, deletes
install_pending + boot_count, cleans rollback backups except backup_path.

6 unit tests + 1 non-fatal contract test covering all state-machine
branches. Uses tempfile + injected short thresholds (std::fs operates
outside tokio virtual time).
```

---

## Task 6 — D11 install.rs `write_install_pending` + orphan cleanup + tests

**Goal**: install.rs records `.install_pending_{VERSION}` at the correct call site with the correct content; orphan backups cleaned on earlier-step failures.

### Steps

1. Add `fn write_install_pending(&self, version: &str, previous_version: &str, backup_path: &Path) -> Result<(), UpdateError>` to `impl Updater`. Content: JSON `{ installed_at, previous_version, backup_path }`.

2. In `install_and_restart_with_ops`:
   - After `replace_binary` success (around `install.rs:407-408`) and BEFORE `restart_app`: call `write_install_pending`. On failure, attempt restoration (Unix rename; Windows: defer to spike — stub with `#[cfg(windows)] unimplemented!("§4.8 spike")` until Task 12).
   - Wrap earlier steps (download, signature verify, replace_binary) in a pattern that cleans `backup_path` on error.

3. Tests:
   - `install_pending_written_after_successful_replace`: mock install flow + assert file exists + content parses.
   - `orphan_backup_removed_on_signature_verify_failure`: simulate verify failure → assert `.rollback.{ts}` file is gone.

4. `cargo test -p oneshim-app --lib updater::install`, fmt, clippy.

### Commit message

```
feat(updater): D11 write_install_pending + orphan-backup cleanup

install.rs records .install_pending_{VERSION} with
{ installed_at, previous_version, backup_path } immediately after
replace_binary succeeds and before restart_app. This gives
HealthProbe::check_startup_state a deterministic backup selection
mechanism (per spec §4.3 + §4.5).

If write_install_pending itself fails, attempts platform-specific
restoration (Unix rename; Windows stubbed pending §4.8 spike — Task 12).

Earlier-step failures (download / signature verify / replace_binary)
now explicitly std::fs::remove_file(backup_path) to prevent orphan
.rollback.{ts} accumulation.

2 unit tests added.
```

---

## Task 7 — D11 api-contracts types preamble + `execute_rollback` + ROLLBACK_EXIT_CODE + integration test

**Goal**: spec §4.6 implementation + the file-ops integration test from spec §4.7. Because `execute_rollback` references `UpdatePhase::RolledBack` (not yet in the contract) and `RollbackInfo`, this task **first** lands the api-contracts types needed for compilation; **Task 9 later** lands the `update_coordinator` handler that actually routes rollback events to `UpdateControl`.

### Steps

1. **api-contracts types preamble** (extracted from old Task 9 step 1; resolves Loop 2 iter 1 finding I-1 — ordering). In `crates/oneshim-api-contracts/src/update.rs`:
   - Add `RolledBack` variant to `UpdatePhase`.
   - Add `pub struct RollbackInfo { from_version: String, from_published_at: Option<String>, to_version: String, to_published_at: Option<String>, reason: RollbackReason, rolled_back_at: String }` with `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]`. Doc-comment each `..._at` field as RFC3339 UTC.
   - Add `pub enum RollbackReason { RepeatedStartupFailure }` with `#[serde(rename_all = "snake_case")]`.
   - Add `pub rollback: Option<RollbackInfo>` field to `UpdateStatus` (with `#[serde(skip_serializing_if = "Option::is_none")]`).
   - `cargo test -p oneshim-api-contracts --lib` + fmt + clippy on the contracts crate.

2. Add `pub const ROLLBACK_EXIT_CODE: i32 = 75;` at the top of `updater/install.rs`.

3. Implement `pub fn execute_rollback(...) -> Result<Infallible, UpdateError>`:
   - Verify `backup_path` exists + executable.
   - Broadcast `UpdatePhase::RolledBack` event (Task 9 wires `update_coordinator` to receive this).
   - Flush any async logs (tracing flush if needed).
   - Unix: `std::fs::rename(backup_path, current_exe_path)` → replace current process image via the Rust `std::os::unix::process::CommandExt` trait method that performs the image-replacement syscall.
   - Windows: delegate to Task 12 spike-deliverable helper. Until spike lands, `#[cfg(windows)] return Err(UpdateError::Install("§4.8 spike pending".into()))`.
   - Success path never returns (process terminated).

4. **Test-mode helper signature** (resolves Loop 2 iter 1 finding M-3). In `install.rs` alongside `execute_rollback`, add:
   ```rust
   /// Internal test helper: perform rollback file-ops without replacing the process image.
   /// Split from execute_rollback so tests can observe the swap + event emission without
   /// the image-replacement syscall terminating the test harness.
   #[cfg(test)]
   pub(crate) fn execute_rollback_swap_only(
       &self,
       backup_path: &Path,
       current_exe_path: &Path,
       sender: &broadcast::Sender<UpdateStatus>,
       from_version: &str,
       to_version: &str,
       reason: RollbackReason,
   ) -> Result<(), UpdateError> {
       // Steps 1-3 of execute_rollback (verify + broadcast + rename).
       // No image-replacement syscall.
   }
   ```
   `execute_rollback` internally calls the same underlying steps then performs the image-replacement syscall. Production and test paths share steps 1-3.

5. Integration test in `src-tauri/tests/rollback_swaps_binary_and_emits_event.rs`:
   - Create temp install dir, fake current + backup binaries (simple content markers), install_pending JSON pointing to backup.
   - Invoke `execute_rollback_swap_only` (the `#[cfg(test)]` helper from step 4).
   - Assert: swapped binary bytes match pre-rollback backup; install_pending was read before swap; RolledBack event broadcast via the captured sender.

6. Tests, fmt, clippy.

### Commit message

```
feat(updater): D11 execute_rollback + ROLLBACK_EXIT_CODE + swap test

execute_rollback returns Result<Infallible, UpdateError> — success path
terminates the process (Unix image replacement; Windows deferred to
§4.8 spike stub). ROLLBACK_EXIT_CODE = 75 (EX_TEMPFAIL) for explicit
process exit from error paths.

Integration test exercises the file-swap + event-broadcast portion in
#[cfg(test)] mode without actually replacing the test harness's process
image. Full end-to-end coverage lives in release-reliability-smoke.sh
per spec §6.1.
```

---

## Task 8 — D11 integration into `app_runtime_launch.rs` + scheduler probe spawn

**Goal**: wire `HealthProbe::check_startup_state` into app boot; wire `spawn_healthy_writer` into scheduler post-boot.

### Steps

1. In `app_runtime_launch.rs` after existing config + installation_id setup, BEFORE scheduler spawn:
   - Resolve `current_exe().parent()?` as `install_dir`.
   - Instantiate `HealthProbe::new(install_dir, CURRENT_VERSION.to_string())`.
   - Call `probe.check_startup_state()`.
   - Match: `Normal` → continue; `RollbackRequired { ... }` → call `install.execute_rollback(...)` and match its `Err` arm with `tracing::error! + std::process::exit(1)`.

2. In `scheduler/mod.rs` after all loops spawn:
   - Invoke `probe.spawn_healthy_writer()` (probe must be passed or re-created; choose the approach that keeps `HealthProbe` owned by the launch path and passed into scheduler as `Arc`).

3. Hand-test on local macOS: install `cargo run`, confirm marker file appears after 30s, confirm normal shutdown deletes it.

4. Tests: hard to unit-test process-level integration; the integration test from Task 7 covers the file-ops chain. Document manual-smoke step in CHANGELOG draft (Task 13).

### Commit message

```
feat(updater): D11 probe wiring — app_runtime_launch + scheduler

app_runtime_launch.rs now calls HealthProbe::check_startup_state()
before scheduler spawn. On RollbackRequired, invokes
install::execute_rollback which never returns on success; Err arm
exits with code 1 (leaving user on the failing binary for the next
boot's retry).

scheduler/mod.rs post-boot invokes probe.spawn_healthy_writer() — the
30s healthy timer starts only after the scheduler is fully up ("useful
app state reachable" intent per spec §4.5).

Manual smoke tested on macOS: marker written after 30s; deleted on
normal shutdown. Full CI coverage via Task 7 integration test +
release-reliability-smoke.sh step (Task 13 adds it).
```

---

## Task 9 — update_coordinator rollback handler wiring

**Goal**: the api-contracts types were landed in Task 7; Task 9 wires `update_coordinator` to receive rollback events broadcast by `execute_rollback` and translate them into `UpdateControl::set_status` calls. This task closes the loop from rollback execution to UI observation.

### Steps

1. In `src-tauri/src/update_coordinator.rs`:
   - Add a handler that receives rollback events (from Task 7's `execute_rollback` broadcast) and calls `UpdateControl::set_status` with phase = `RolledBack` + `rollback: Some(RollbackInfo { ... })`.
   - Populate `from_published_at` / `to_published_at` from the GitHub Release metadata if cached in the coordinator's release-info cache; `None` otherwise (spec §5 fallback).
   - Emit telemetry event per Task 0 decision. **Record the Task 0 telemetry-API decision explicitly in this commit message** (resolves Loop 2 iter 1 M-5) — e.g., "Telemetry path: `<counter API | span event | tracing only>` per Task 0 audit".

2. `cargo test -p oneshim-api-contracts --lib` + `cargo check -p oneshim-app`.

3. Frontend consumption is Task 10.

### Commit message

```
feat(updater): update_coordinator rollback handler + UpdateControl wiring

update_coordinator.rs:
- Receives rollback events from install::execute_rollback broadcast
  (Task 7 landed the broadcast channel + api-contracts types).
- Translates to UpdatePhase::RolledBack + RollbackInfo via
  UpdateControl::set_status. Populates from_published_at /
  to_published_at from the coordinator's release-info cache when
  available; None otherwise (graceful UI fallback per spec §5).
- Telemetry path: <counter API | span event | tracing only>
  (chosen per Task 0 audit decision).

Frontend consumption in Task 10.
```

---

## Task 10 — Frontend UpdateStatusPanel rollback render + published_at surface + i18n

**Goal**: spec §5.2 frontend work.

### Steps

1. In `crates/oneshim-web/frontend/src/...`:
   - Extend `UpdateStatusPanel.tsx` to handle `phase === "rolled_back"`: render from/to versions + dates (via shared formatter) + reason via i18n.
   - Update the "pending update" rendering to surface `published_at` when present: `v{latest_version} (YYYY-MM-DD 배포)`. Fallback to version alone when `None`.

2. Shared date formatter (add if absent): `formatReleaseDate(iso: string | null | undefined, locale: "ko" | "en"): string | null`. Relative time for <24h; absolute ISO YYYY-MM-DD for older.

3. i18n keys (ko/en):
   - `update.rolledBack.title`
   - `update.rolledBack.reason.repeatedStartupFailure`
   - `update.rolledBack.toast.bothDates` (interpolated: `{fromVersion}`, `{fromDate}`, `{toVersion}`, `{toDate}`)
   - `update.rolledBack.toast.partialDates` (shorter template without dates)
   - `update.releaseDate` (suffix wrapper "{date} 배포")
   - `update.releaseDateUnknown` (tooltip shown when date is None)
   - `update.releasedAgo.minutes`, `.hours`, `.days` (interpolated count)

4. Run `pnpm -C crates/oneshim-web/frontend lint` + `pnpm build`.

5. Visual smoke: run dashboard locally; mock backend to emit `RolledBack` state; confirm panel renders.

### Commit message

```
feat(frontend): UpdateStatusPanel rollback state + published_at render

Adds RolledBack case to UpdateStatusPanel: shows from/to version +
published_at date via shared formatReleaseDate (relative for <24h,
absolute ISO for older). Reason rendered via i18n. Date-absent fallback
renders version alone + tooltip with update.releaseDateUnknown.

Pending-update rendering now surfaces the (previously unused)
PendingUpdateInfo.published_at field — "v0.4.40-rc.1 (2026-04-18 배포)".

13 new i18n keys across ko + en:
- update.rolledBack.title / reason.repeatedStartupFailure / toast.bothDates / toast.partialDates
- update.releaseDate / releaseDateUnknown / releasedAgo.(minutes|hours|days)

pnpm lint + build green.
```

---

## Task 11 — cliff.toml body amendment + release.yml header step

**Goal**: spec §6.3 diff applied; release_notes.md header prepend landed.

### Steps

1. Apply the exact diff from spec §6.3 to `cliff.toml` body template. Preserve trailing `\` continuations. **Note**: existing line 10 (`## [...]\`) ends with `\` (no newline). The diff must explicitly add a `\n` boundary between the header and the new `**Release Date:**` line, or output will collide on a single line. The dry-run diff (step 3) will surface this if missed.

2. Dry-run against the **same fixed tag range** used in Task 0 (`v0.4.38..v0.4.39-rc.1`, NOT `--unreleased`). Command: `git cliff v0.4.38..v0.4.39-rc.1 > /tmp/cliff-amended.md`.

3. Diff `/tmp/cliff-baseline.md` (from Task 0) vs `/tmp/cliff-amended.md`. Expect: only the `**Release Date:** ...` + `**Since v... :** ...` lines appear in the amended output (plus the intentional blank line). Any other diff lines are regressions in template whitespace handling.

4. Add the prepend step in `.github/workflows/release.yml` per spec §6.3:
   ```yaml
   - name: Prepend date header to release notes
     run: |
       DATE=$(date -u +"%B %d, %Y")
       TAG="${RELEASE_TAG}"
       printf "## ONESHIM Client ${TAG} — Released ${DATE}\n\n" | cat - release_notes.md > _rn && mv _rn release_notes.md
   ```
   Insert between existing "Generate release notes" step and any upload step.

5. Run a local dry-run of the workflow step by invoking the `printf | cat` pipe on a sample `release_notes.md`.

### Commit message

```
docs(release): cliff.toml body amendment + release.yml header prepend

cliff.toml body: insert **Release Date:** + **Since vX.Y.Z:** lines
after the ## [VERSION] header, preserving trailing \ whitespace control
per spec §6.3. Dry-run diff against the baseline (Task 0) confirms only
the two new substantive lines + intentional blank line differ.

release.yml: prepend "## ONESHIM Client {TAG} — Released {DATE}" to
release_notes.md, so the GitHub Release name and body both carry the
human-readable release date.

Contributor-count variable guarded with {% if previous and previous.version
and contributors %} for single-tag + old-git-cliff fallback.
```

---

## Task 12 — Windows rollback spike day + `docs/guides/updater-rollback-windows.md`

**Goal**: resolve spec §4.8 open question; produce the deliverable doc; implement the Windows side of `execute_rollback`.

### Steps

1. **Verify install location assumption**: inspect `src-tauri/tauri.conf.json` for Windows install scope (per-user vs system). If system-scope, this spike must address UAC. Note findings.

2. **Test shell-helper mechanism** on a local Windows VM (or CI runner):
   - Write fake current + backup binaries in a temp dir.
   - Spawn `cmd.exe /c "timeout /t 3 /nobreak >nul && move /Y {backup} {current} && start {current}"`.
   - Current process exits; verify swap + relaunch occurred.
   - Test with Windows Defender active.

3. **Fallback test**: `MoveFileEx(MOVEFILE_DELAY_UNTIL_REBOOT)` via `windows-sys` crate. Verify the `PendingFileRenameOperations` registry entry is written. Acknowledge UX cost (requires reboot).

4. Based on results, implement the Windows branch of `execute_rollback`:
   - Preferred: shell-helper.
   - Fallback: MoveFileEx.
   - Emit `tracing::error!` + `std::process::exit(ROLLBACK_EXIT_CODE)` after helper spawn or scheduled rename.

5. Write `docs/guides/updater-rollback-windows.md` documenting the choice + the fallback logic + CI-row caveat (spec §8.4).

6. `cargo test -p oneshim-app --target x86_64-pc-windows-msvc --lib` (on Windows CI runner).

### Commit message

```
feat(updater): D11 Windows rollback — <shell-helper | MoveFileEx> implementation

Spike outcome: <chosen mechanism>. Documented in
docs/guides/updater-rollback-windows.md. Unblocks §4.8 stub in Task 7's
execute_rollback.

<Brief description of mechanism behavior>

Windows CI row asserts <per spec §8.4 caveat>. Full end-to-end Windows
rollback verification requires a real reboot — outside CI scope;
release-reliability-smoke.ps1 asserts only the observable precondition.
```

---

## Task 13 — Key rotation runbook + CHANGELOG draft + final verification

**Goal**: land the two rotation docs + CHANGELOG.md 0.4.40-rc.1 entry + run the full verification suite.

### Steps

1. Create `docs/guides/updater-key-rotation.md`:
   - Scheduled rotation procedure per spec §7.3.1.
   - Compromise response per spec §7.3.2 with platform-specific trust-anchor sub-sections.
   - Reference `rehearse-key-rotation.sh` usage.

2. Add an entry to `CHANGELOG.md` under `[Unreleased]`:
   ```markdown
   ## [Unreleased]

   ### Added
   - Phase 4 Updater Hardening (D9 + D10 + D11): multi-key Ed25519 trust array
     for day-1 key rotation support; defensive `installation_id: None` handling
     in the staged rollout gate; post-install self-healthy probe with 2-failed-
     boot automatic rollback. See
     [docs/reviews/2026-04-18-phase4-updater-hardening-design.md](docs/reviews/2026-04-18-phase4-updater-hardening-design.md)
     for the full design.
   - Release body enrichment: **Release Date** + **Since vX.Y.Z** metadata
     lines in CHANGELOG + GitHub Release notes via cliff.toml amendment.
   - `published_at` (ISO-8601 UTC) now rendered in the updater's pending-update
     panel as "v{version} ({date} 배포)".

   ### Changed
   - `validate_integrity_policy` no longer requires `signature_public_key` to be
     non-empty; the built-in `TRUSTED_PUBLIC_KEYS` array is the authoritative
     trust source.
   ```

3. Run full verification:
   - `cargo test --workspace` (measure new baseline).
   - `cargo clippy --workspace --all-targets -- -D warnings`.
   - `cargo fmt --check`.
   - `pnpm -C crates/oneshim-web/frontend lint + build`.

4. Record new test-count baseline in `.claude/phase4-progress.md` Task 13 entry.

5. Prepare PR body draft using the spec summary (§1.1 goals + §6.1 tests + §8 acceptance).

### Commit message

```
docs(phase4): key rotation runbook + CHANGELOG 0.4.40-rc.1 entry

docs/guides/updater-key-rotation.md covers:
- Scheduled rotation (1-2 release overlap window, §7.3.1)
- Compromise response (immediate removal, §7.3.2) with platform-specific
  trust-anchor guidance (macOS codesign / Windows SHA-256 / Linux attest)
- rehearse-key-rotation.sh usage

CHANGELOG [Unreleased] entry added: D9 + D10 + D11 summary + A-4 release
metadata + validate_integrity_policy relaxation.

Full verification suite passes:
- cargo test --workspace: passed M (+N from Task 0 baseline), ignored K
- cargo clippy --all-targets -D warnings: zero warnings
- cargo fmt --check: clean
- pnpm lint + build: green
```

---

## Final PR checklist

Before submitting:

- [ ] All 13 tasks committed + pushed.
- [ ] Task 0 baseline matches post-Task-13 **approximately +14 tests** (exact count depends on Task 0 zero-gap escape hatches; Task 13 CHANGELOG records the actual delta).
- [ ] Spec `docs/reviews/2026-04-18-phase4-updater-hardening-design.md` committed in the PR branch.
- [ ] All 5 new files present: `trusted_keys.rs`, `health_probe.rs`, `updater-rollout.md`, `updater-key-rotation.md`, `updater-rollback-windows.md`.
- [ ] `cliff.toml` amended, dry-run diff confirms only the two new substantive lines.
- [ ] `CHANGELOG.md [Unreleased]` has the 0.4.40-rc.1-worthy entry.
- [ ] PR body describes D9 + D10 + D11 at high level + links to spec.
- [ ] Manual macOS smoke: forcibly crash app twice within 30s → third launch rolls back.
- [ ] Windows CI row green per spike outcome.

---

## Acceptance criteria (from spec §8.4, restated for PR review)

- All 14 new tests pass; workspace test count is pre-Phase-4 baseline + 14.
- `cargo clippy --workspace --all-targets -- -D warnings`: zero warnings.
- `cargo fmt --check`: clean.
- Manual rollback smoke on macOS passes.
- Release body (when v0.4.40-rc.1 is cut) includes **Release Date** + **Since v0.4.39** headers.
- Frontend renders `published_at` in pending-update + rolled-back panels, with `None` fallback behavior confirmed.
- Windows CI row green per §4.8 spike outcome (caveat for MoveFileEx path: asserts deferred-rename scheduled, not rollback completed).
- `docs/guides/updater-key-rotation.md` is reviewed; `rehearse-key-rotation.sh` dry-run documented.

---

## Deferred follow-ups (out of this PR, restated from spec §8.5)

- `pre-release-check.sh:241` Dependabot-disabled JSON-blob guard.
- Notarization workflow `head_branch` condition fix.
- Telemetry cohort observability wiring.
- Nightly channel official activation.
- Additional `RollbackReason` variants.
- CalVer-in-tag (A-2 option).

---

*End of plan.*
