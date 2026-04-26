# Phase 1 — Iteration 1 Spec Review Findings (PR-B2)

**Date**: 2026-04-25
**Spec under review**: spec v1 (commit `431d6668`)
**Reviewers**: 1 superpowers:code-reviewer subagent (sonnet) + 1 Explore subagent (feasibility verification)
**Outcome**: 3 Critical + 6 Important + 5 Nice-to-have

---

## CRITICAL (block Phase 2)

### C1 — sha2 version conflict + redundant dep add

Spec §5.2 line 228 says "Add `sha2 = "0.10"` to `src-tauri/Cargo.toml`".

Reality (verified):
- Workspace `Cargo.toml` line 119: `sha2 = "0.11"`
- `src-tauri/Cargo.toml` line 54: `sha2 = { workspace = true }` already present

Adding direct `sha2 = "0.10"` would create version conflict + duplicate dep. Q-B2-6 answered: sha2 0.11 already available, no Cargo.toml change needed.

**Fix**: Remove all references to "add sha2 dep" in spec (§5.2 line 228, §10.1 commit 1).

### C2 — AutostartCode enum baseline assumption wrong

Spec §6 says "PR-B2 adds 4 NEW variants to `AutostartCode` enum (currently has 5 from PR-B1)" → "47 (PR-B1 baseline) + 4 = 51".

Reality: PR-B2 worktree base is `0827e071` (origin/main, BEFORE PR-B1 merge). Therefore:
- `AutostartCode` enum does NOT exist in `crates/oneshim-core/src/error_codes/`
- `wire_contract_snapshot.expected.txt` has 42 lines, none `autostart.*`
- The "5 PR-B1 codes" baseline is hypothetical until PR-B1 merges

**Fix**: Spec must explicitly state two scenarios:
- Scenario A (PR-B1 merges first, then PR-B2 rebases onto post-PR-B1 main): 47 + 4 = 51 (PR-B2 only adds 4 variants)
- Scenario B (PR-B2 implementation happens before PR-B1 merge — NOT possible per implementation gate): N/A

Since PR-B1 must merge first per §10.2, Scenario A applies. Update wording in §6 to clarify this assumption explicitly.

### C3 — Existing test in autostart.rs:508 will fail after template change

`src-tauri/src/autostart.rs:508-509`:
```rust
fn service_file_contains_required_keys() {
    assert!(service.contains("Type=simple"));
}
```

PR-B2 changes template to `Type=notify` → this test fails.

Spec §9 testing strategy + §10.1 commit 6 don't mention updating this existing test.

**Fix**: Add explicit instruction in §10.1 commit 6: "Update existing inline test `service_file_contains_required_keys` (autostart.rs:508-509) to assert `Type=notify` instead of `Type=simple`."

---

## IMPORTANT (must address in spec or plan)

### I1 — libsystemd-dev missing in CI apt install

Spec §5.5 `linux-autostart-unit` CI job step: `cargo test ... --features systemd-notify ...`

`sd-notify` crate links against native `libsystemd` library on Linux. Without `libsystemd-dev` apt package, build fails.

**Fix**: Add to CI step:
```yaml
- run: sudo apt-get update && sudo apt-get install -y libsystemd-dev
```
Before the cargo test step. Also add to `linux-systemd-integration.yml` workflow.

### I2 — Cross-platform feature flag handling clarification

Spec §11.3 line 803: "Cross-platform builds: feature flag silently ignored on macOS/Windows (cfg-gated)".

Reality: Cargo `--features systemd-notify` triggers compilation of `sd-notify` crate regardless of target OS. The crate IS pure Rust (compiles on macOS/Windows), but architectural intent is unclear.

Also: spec says "Default build: `systemd-notify` feature ENABLED on Linux" — Cargo doesn't support per-platform default features without `build.rs` workaround.

**Fix**: Spec must clarify:
- Option A: Feature is non-default. CI explicit `--features systemd-notify` for Linux jobs only. Document in spec.
- Option B: Use cfg-gated import in code only; Cargo dep is unconditional but only compiled if feature enabled. Same as A in practice.

Recommended: Option A. Update §5.1 + §11.3 accordingly.

### I3 — Init hook placement: spec contradicts itself

Spec §5.1 line 176: "Call `notify_ready()` in main.rs after init complete"
Spec §5.2 says "wire ... in setup.rs after the existing D-Bus presence check"

Verified `setup.rs::init()` (line 12-52) ALREADY encompasses scheduler loops + DB migrations + window shown via `BootstrapRuntimeBuilder` (line 31) + `DesktopStartupCoordinator` (line 37). Returns `Ok(())` at line 51. main.rs only calls `setup::init` at line 240.

**Fix**: Pick ONE placement. Recommend: setup.rs::init() last line before `Ok(())`. Update §5.1 + §10.1 commit 9 accordingly.

### I4 — Migration binary_path replacement is fragile

Spec §5.2 `canonicalize()`:
```rust
content.replace(binary_path, "{BINARY_PATH}")
```

Edge cases:
- `binary_path` = `/home/user/oneshim`; file contains `ExecStart=/home/user/oneshim-old` → false positive replacement
- `current_exe()` resolves symlinks via `/proc/self/exe` on Linux; user wrote service file with symlink path → no match

**Fix**: Use word-boundary-aware replacement OR replace exact `ExecStart={path}\n` line:
```rust
let exec_line = format!("ExecStart={}\n", binary_path);
content.replace(&exec_line, "ExecStart={BINARY_PATH}\n")
```

Add Q-B2-10 for symlink edge case to spec §12. Also document in risk register §13.

### I5 — T9 won't actually verify systemd notification on ubuntu-latest

Spec §5.5 T9 (`sd_notify_succeeds_under_systemd_run`) requires `systemd-run --user --scope`. ubuntu-latest GitHub Actions runners don't have real systemd PID 1 — `systemctl --user` operations are limited.

Without `NOTIFY_SOCKET`, `sd_notify::notify` returns Err (logged at debug, no panic). T9 silently passes without actually verifying notify reaches systemd.

**Fix**: Spec must clarify what T9 actually asserts. Either:
- (a) Re-scope T9 to "notify_ready() doesn't panic when invoked outside systemd" (achievable in CI)
- (b) Move T9 to manual smoke matrix + remove from CI
- (c) Use a self-hosted runner with real systemd (defer)

Recommended: (a) for CI + add new manual T8b test in §9.4 for real verification.

### I6 — info! err.code field misuse

Spec §8.2 line 700:
```rust
info!(err.code = AutostartCode::ServiceMigrated.as_str(), ...)
```

ADR-019 convention: `err.code` field is for ERROR codes (warn/error level). Using on info-level success log will pollute Loki/Grafana error-rate dashboards.

**Fix**: Either:
- (a) Use `event.code` for success-path info logs
- (b) Drop the code field on info!, only use code on warn/error

Apply to all 4 examples in §8.2:
- `SdNotifySkipped` (debug): borderline acceptable, keep as `err.code` since it CAN indicate misconfig
- `ServiceMigrated` (info): change to `event.code`
- `ServiceMigrationSkipped` (warn): keep `err.code`
- `ServiceMigrationFailed` (warn): keep `err.code`

---

## NICE-TO-HAVE

### N1 — Q-B2-9 line-ending normalization promotion

Spec §13 risk register references Q-B2-9 mitigation, but Q-B2-9 in §12 says "Add to spec §5.2 if confirmed needed" — circular.

**Fix**: Promote Q-B2-9 to spec decision: add `content.replace("\r\n", "\n")` as first operation in `canonicalize()`. Resolve Q-B2-9 in §12.

### N2 — Empty/zero-byte service file test

T6/T7 don't cover the case `path.exists() = true, content = ""`. Empty file hashes to deterministic SHA, no match → treated as customized (correct, log warn). Worth explicit T7b test.

**Fix**: Add T7b to §5.5 test matrix.

### N3 — lifecycle.rs flat file vs directory module

Verified `src-tauri/src/lifecycle.rs` is currently a flat file. Spec proposes `lifecycle/sd_notify.rs` + `lifecycle/migration_hashes.rs` + `lifecycle/autostart_migration.rs` — implies directory module per ADR-003.

**Fix**: Spec §5.1/§5.2 must specify: convert `lifecycle.rs` → `lifecycle/mod.rs` + new submodules per ADR-003. Add as a step in commit 4 (sd_notify wrapper module creation).

### N4 — Korean docs Flatpak path incorrect

Spec line 547: `~/.var/app/com.oneshim.client/...` is Flatpak app DATA dir, not autostart config.

**Fix**: Replace with: "Flatpak background portal API" (https://docs.flatpak.org/en/latest/portal-api-reference.html#gdbus-org.freedesktop.portal.Background) or remove the specific path.

### N5 — systemd minimum version not specified

Type=notify introduced in systemd 219 (2015). Modern distros all qualify but Korean ops doc should mention.

**Fix**: Add line to autostart.ko.md "Linux 환경별 지원" section: "systemd 219 이상 필요 (Ubuntu 20.04+, Fedora 33+, Debian 10+ 기본 충족)".

---

## Phase 1 iter-2 Plan

After v2 spec applies all fixes above:
1. Fresh subagent re-reviews v2
2. Verify fixes don't introduce new issues
3. Verify Q-B2-1 through Q-B2-10 status
4. If clean → Phase 2 (writing-plans)
5. If issues → iter-3

**Estimated v2 deltas**: ~10-15 spec edits, +1 new section explaining Scenario A (PR-B1 merge order), +1 risk register entry for symlink edge case, +1 Q-B2-10.
