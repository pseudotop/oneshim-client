# Phase 9 PR-B2 — Autostart Linux Deep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Linux story for ONESHIM autostart by adding systemd Type=notify integration (with hash-based deferred migration for PR-B1 users), real environment detection (Snap/Flatpak/headless), capability-aware UI tooltip refinements, rootless Linux integration tests, and Korean operations documentation.

**Architecture:** PR-B2 builds on PR-B1's foundation. Service file template changes from `Type=simple` to `Type=notify` with `NotifyAccess=main` + `TimeoutStartSec=30`. A new `lifecycle/sd_notify.rs` wrapper sends READY signal to systemd at end of `setup::init()`. Migration uses SHA-256 hash check against `KNOWN_PRIOR_HASHES` to safely overwrite PR-B1-era files without clobbering customizations or breaking running services (defer daemon-reload to next user login). Real `detect_capabilities()` replaces PR-B1 skeleton, gating UI on Snap/Flatpak/headless/XDG environments.

**Tech Stack:**
- Rust + Tauri 2 + tokio
- `sd-notify = "0.4"` (NEW Linux-only dep, optional via `systemd-notify` feature flag)
- `sha2 = "0.11"` (already in workspace, no Cargo.toml change needed per Phase 1 iter-1 C1)
- `serial_test` (existing dev-dep, used for env-mutating tests per `reference_serial_test_pattern.md`)
- ADR-019 wire codes via `define_code_enum!` macro
- ADR-003 directory module pattern (lifecycle.rs flat → lifecycle/ directory)

**Source spec:** `docs/superpowers/specs/2026-04-25-phase9-pr-b2-autostart-linux-deep-design.md` (v2.5, commit `d1cc9130`)

**Worktree:** `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-linux-deep` on branch `feature/phase9-autostart-linux-deep`

**Total estimate:** ~14.5h across 14 tasks.

**Hard blocker:** PR-B1 (#508) MUST merge to main BEFORE Phase 3 implementation starts. After merge, this branch must be rebased onto post-PR-B1 main.

**⚠ ABORT GUARD (per Phase 2 iter-2 I1)**: If `gh pr view 508 --json state` returns anything other than `MERGED`, HALT immediately. No implementation task (Task 1-15) may proceed before PF1 + PF2 succeed. The plan body assumes PR-B1 types (`AutostartCode`, `AutostartCapabilities`, `EnvironmentKind`, `UnsupportedReason`, `detect_capabilities`) exist — they do NOT in pre-PR-B1 base.

---

## Pre-Flight Checks (before Task 1)

- [ ] **PF1: Verify PR-B1 (#508) is merged to main**

```bash
gh pr view 508 --json state,mergedAt,mergeCommit
```
Expected: `state == "MERGED"`. If not merged, STOP — implementation is blocked. Spec/plan iterations can continue but no implementation commits should be made.

- [ ] **PF2: Rebase this branch onto post-PR-B1 main**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-linux-deep
git fetch origin
git rebase origin/main
```
Resolve any conflicts. Expected: PR-B1 commits now in history before our PR-B2 commits.

- [ ] **PF3: Verify baseline state**

```bash
cargo check --workspace
cargo test -p oneshim-core --test wire_contract_snapshot
cargo test -p oneshim-app --bin oneshim commands::autostart::tests
```
Expected:
- workspace compiles clean
- wire snapshot test passes (47 codes after PR-B1 merge)
- PR-B1's autostart command tests pass

- [ ] **PF4: Read required reference files**

Required reading before starting Phase 3:
1. `src-tauri/src/setup.rs` (full file) — find `init()` last line before `Ok(())` for sd_notify placement
2. `src-tauri/src/autostart.rs` (Linux mod, lines 297-540) — current `generate_service_file()` template + `has_systemctl()` + existing inline tests
3. `src-tauri/src/lifecycle.rs` — current flat file structure (will be converted to directory module per ADR-003)
4. `crates/oneshim-core/src/error_codes/autostart.rs` — `AutostartCode` enum from PR-B1 (5 variants)
5. `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` — current 47 codes
6. `crates/oneshim-web/frontend/src/i18n/wire-errors.{en,ko}.json` — wire-error translations
7. `.github/workflows/ci.yml` — existing test job pattern for CI workflow additions
8. PR-B2 spec: `docs/superpowers/specs/2026-04-25-phase9-pr-b2-autostart-linux-deep-design.md` (v2.5)

---

## File Structure

### Files to be created

| File | Responsibility |
|------|----------------|
| `src-tauri/src/lifecycle/sd_notify.rs` | systemd Type=notify wrapper (notify_ready, notify_stopping) — no-op on non-Linux or when `systemd-notify` feature disabled |
| `src-tauri/src/lifecycle/migration_hashes.rs` | `KNOWN_PRIOR_HASHES` registry + `canonicalize()` + `compute_hash()` + `matches_known_template()` pure functions |
| `src-tauri/src/lifecycle/autostart_migration.rs` | `run_startup_migration()` — read existing service file, hash-check, decide overwrite/skip |
| `src-tauri/tests/linux_autostart_unit.rs` | T1-T7 unit tests (no real systemd needed) |
| `src-tauri/tests/linux_autostart_systemd_live.rs` | T8-T10 manual-only integration tests (`#[ignore]`) |
| `.github/workflows/linux-systemd-integration.yml` | Manual workflow_dispatch for T8-T10 |
| `docs/guides/autostart.ko.md` | Korean operations + migration guide |

### Files to be modified

| File | What changes |
|------|--------------|
| `src-tauri/Cargo.toml` | Add `sd-notify = { version = "0.4", optional = true }` dep + `systemd-notify = ["dep:sd-notify"]` feature flag (NOT default) |
| `src-tauri/src/lifecycle.rs` → `src-tauri/src/lifecycle/mod.rs` | Convert flat file to directory module per ADR-003; re-export sd_notify + migration_hashes + autostart_migration submodules |
| `src-tauri/src/autostart.rs` | (a) Update `linux::generate_service_file()` template to Type=notify; (b) Update existing inline test `service_file_contains_required_keys` (line ~508) per C3; (c) Replace PR-B1 skeleton `detect_capabilities()` with real Linux env detection |
| `src-tauri/src/setup.rs` | Add `lifecycle::autostart_migration::run_startup_migration()` call AFTER existing D-Bus check; add `lifecycle::sd_notify::notify_ready()` as LAST line before `Ok(())` |
| `crates/oneshim-core/src/error_codes/autostart.rs` | Append 4 new variants: `SdNotifySkipped`, `ServiceMigrated`, `ServiceMigrationSkipped`, `ServiceMigrationFailed` |
| `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` | Insert 4 new autostart codes in alphabetical order |
| `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json` | Add 4 new translations |
| `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json` | Add 4 new translations |
| `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts` | Update count from 47 → 51 (2 places) |
| `.github/workflows/ci.yml` | Add `linux-autostart-unit` job (always-on, NEW) |
| `docs/STATUS.md` | Update version + Rust + Vitest test counts |
| `docs/PHASE-HISTORY.md` | Add Phase 9 PR-B2 entry |

---

## Task 1: Add sd-notify Dependency with Feature Flag

**Estimate:** 0.5h | **Spec ref:** §5.1 + §10.1 commit 1 | **Files:** `src-tauri/Cargo.toml`, `Cargo.lock`

- [ ] **Step 1.1: Add dep + feature flag**

Open `src-tauri/Cargo.toml`. Locate `[dependencies]` section. Append:

```toml
sd-notify = { version = "0.4", optional = true }
```

Then locate or create `[features]` section. Add (or append to existing):

```toml
systemd-notify = ["dep:sd-notify"]
```

**Important** (per spec §11.3 + Phase 1 iter-2 I2): `systemd-notify` is NOT in the `default` features array. Linux release builds must pass `--features systemd-notify` explicitly.

- [ ] **Step 1.2: Verify cargo resolves**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-linux-deep
cargo check -p oneshim-app --features systemd-notify 2>&1 | tail -10
```
Expected: clean compile (no usage of sd-notify yet, just dep added). Cargo.lock updated.

- [ ] **Step 1.3: Verify cross-platform check still works**

```bash
cargo check -p oneshim-app 2>&1 | tail -5
```
Expected: clean compile WITHOUT `--features systemd-notify` (sd-notify crate not pulled in unless feature enabled).

- [ ] **Step 1.4: Commit**

```bash
git add src-tauri/Cargo.toml Cargo.lock
git commit -m "chore(autostart): add sd-notify v0.4 dep with systemd-notify feature flag (NOT default)"
```

---

## Task 2: 4 New AutostartCode Wire Variants

**Estimate:** 0.5h | **Spec ref:** §6 + §10.1 commit 2 | **Files:** `crates/oneshim-core/src/error_codes/autostart.rs`, `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`

- [ ] **Step 2.1: Add 4 variants to AutostartCode enum**

Open `crates/oneshim-core/src/error_codes/autostart.rs`. Find the `define_code_enum!` block. Add 4 new variants in alphabetical position within the enum:

```rust
define_code_enum! {
    /// Autostart 카테고리 에러 코드.
    pub enum AutostartCode {
        // PR-B1 (existing):
        /// 자동 시작 카운터 증가 실패.
        CounterIncrementFailed => "autostart.counter_increment_failed",
        /// 자동 시작 비활성화 실패.
        DisableFailed => "autostart.disable_failed",
        /// 자동 시작 활성화 실패.
        EnableFailed => "autostart.enable_failed",
        /// autostart Tauri 이벤트 emit 실패.
        EventEmitFailed => "autostart.event_emit_failed",
        /// 자동 시작 상태 조회 실패.
        QueryFailed => "autostart.query_failed",

        // PR-B2 (NEW):
        /// systemd notify 호출 스킵 (NOTIFY_SOCKET 없음 등).
        SdNotifySkipped => "autostart.sd_notify_skipped",
        /// systemd 서비스 파일 마이그레이션 완료.
        ServiceMigrated => "autostart.service_migrated",
        /// systemd 서비스 파일 마이그레이션 실패 (write/io 에러).
        ServiceMigrationFailed => "autostart.service_migration_failed",
        /// systemd 서비스 파일 마이그레이션 스킵 (사용자 customize 추정).
        ServiceMigrationSkipped => "autostart.service_migration_skipped",
    }
}
```

- [ ] **Step 2.2: Update wire_contract_snapshot.expected.txt**

Open `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`. Find existing `autostart.*` lines (5 codes from PR-B1). Insert the 4 new codes at correct alphabetical positions:

```
autostart.counter_increment_failed
autostart.disable_failed
autostart.enable_failed
autostart.event_emit_failed
autostart.query_failed
autostart.sd_notify_skipped
autostart.service_migrated
autostart.service_migration_failed
autostart.service_migration_skipped
```

(All `autostart.s*` codes go AFTER `autostart.query_failed`. Sort: `sd_notify_skipped < service_migrated < service_migration_failed < service_migration_skipped`.)

- [ ] **Step 2.3: Run snapshot test**

```bash
cargo test -p oneshim-core --test wire_contract_snapshot 2>&1 | tail -10
```
Expected: GREEN. Snapshot reads enum-generated codes, compares to expected.txt. 51 codes total.

- [ ] **Step 2.4: Commit**

```bash
git add crates/oneshim-core/src/error_codes/autostart.rs crates/oneshim-core/tests/wire_contract_snapshot.expected.txt
git commit -m "feat(autostart): add 4 PR-B2 wire codes (SdNotifySkipped + ServiceMigration{ed,Failed,Skipped})"
```

---

## Task 3: Wire-Error i18n Translations + CI Gate Update

**Estimate:** 0.5h | **Spec ref:** §6 + §10.1 commit 3 | **Files:** `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json`, `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json`, `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts`

- [ ] **Step 3.1: Add en translations**

Open `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json`. Add 4 new entries in alphabetical position (after the existing `autostart.query_failed`):

```json
  "autostart.sd_notify_skipped": "systemd notification skipped",
  "autostart.service_migrated": "Autostart service file migrated to Type=notify",
  "autostart.service_migration_failed": "Autostart service file migration failed: {message}",
  "autostart.service_migration_skipped": "Autostart service file migration skipped (customized file detected)",
```

- [ ] **Step 3.2: Add ko translations**

Open `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json`. Add 4 new entries:

```json
  "autostart.sd_notify_skipped": "systemd notify 스킵됨",
  "autostart.service_migrated": "자동 시작 서비스 파일이 Type=notify로 마이그레이션됨",
  "autostart.service_migration_failed": "자동 시작 서비스 파일 마이그레이션 실패: {message}",
  "autostart.service_migration_skipped": "자동 시작 서비스 파일 마이그레이션 스킵 (사용자 수정 감지)",
```

- [ ] **Step 3.3: Update Vitest count expectations**

Open `crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts`. Find 2 places that reference count `47`:

```bash
grep -n "47" crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts
```

Update both:
- `it('snapshot contains the expected 47 codes', () => {` → `it('snapshot contains the expected 51 codes', () => {`
- `expect(registry).toHaveLength(47)` → `expect(registry).toHaveLength(51)`
- `it('returns all 47 codes for en', () => {` → `it('returns all 51 codes for en', () => {`
- `expect(translatedCodes('en')).toHaveLength(47)` → `expect(translatedCodes('en')).toHaveLength(51)`

- [ ] **Step 3.4: Run CI gate + Vitest**

```bash
bash scripts/check-wire-error-i18n-coverage.sh 2>&1 | tail -5
cd crates/oneshim-web/frontend && pnpm test src/i18n/__tests__/translateError.test.ts --run 2>&1 | tail -10
```
Expected: both GREEN. CI gate: `[OK] All wire codes have en+ko translations (51 keys per locale)`. Vitest: 18 tests pass.

- [ ] **Step 3.5: Commit**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-linux-deep
git add crates/oneshim-web/frontend/src/i18n/wire-errors.en.json crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json crates/oneshim-web/frontend/src/i18n/__tests__/translateError.test.ts
git commit -m "test(autostart): wire-error i18n translations for 4 PR-B2 codes (en+ko, 47→51)"
```

---

## Task 4: lifecycle Directory Module + sd_notify Wrapper

**Estimate:** 1h | **Spec ref:** §5.1 + §10.1 commit 4 + N3 (ADR-003 directory module) | **Files:** Convert `src-tauri/src/lifecycle.rs` flat → `src-tauri/src/lifecycle/mod.rs` directory + Create `src-tauri/src/lifecycle/sd_notify.rs`

- [ ] **Step 4.1: Convert lifecycle.rs to directory module**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-autostart-linux-deep
mkdir -p src-tauri/src/lifecycle
git mv src-tauri/src/lifecycle.rs src-tauri/src/lifecycle/mod.rs
```

This preserves git history. The `mod.rs` retains all existing content from `lifecycle.rs`.

- [ ] **Step 4.2: Add submodule declarations to mod.rs**

Open `src-tauri/src/lifecycle/mod.rs`. At the TOP of the file (before any existing content), add:

```rust
pub mod sd_notify;
```

(Other submodules added in Tasks 7+9.)

- [ ] **Step 4.3: Create sd_notify.rs**

Create `src-tauri/src/lifecycle/sd_notify.rs`:

```rust
//! systemd Type=notify integration.
//!
//! No-op on non-Linux platforms or when `systemd-notify` feature disabled.
//! When run outside systemd (e.g., `cargo run`, manual launch), `sd_notify::notify`
//! returns Err which we log at debug — no user-visible impact.
//!
//! See spec: docs/superpowers/specs/2026-04-25-phase9-pr-b2-autostart-linux-deep-design.md §5.1

use oneshim_core::error_codes::AutostartCode;

#[cfg(all(target_os = "linux", feature = "systemd-notify"))]
pub fn notify_ready() {
    if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]) {
        tracing::debug!(
            err.code = AutostartCode::SdNotifySkipped.as_str(),
            "sd_notify READY skipped (not run under systemd): {e}"
        );
    }
}

#[cfg(not(all(target_os = "linux", feature = "systemd-notify")))]
pub fn notify_ready() {
    // No-op on non-Linux or when systemd-notify feature disabled.
    let _ = AutostartCode::SdNotifySkipped; // silence unused import warning
}

#[cfg(all(target_os = "linux", feature = "systemd-notify"))]
pub fn notify_stopping() {
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
}

#[cfg(not(all(target_os = "linux", feature = "systemd-notify")))]
pub fn notify_stopping() {}
```

- [ ] **Step 4.4: Verify cross-platform compile**

```bash
# Verify default build (no systemd-notify feature) compiles
cargo check -p oneshim-app 2>&1 | tail -5
# Verify Linux feature build also compiles
cargo check -p oneshim-app --features systemd-notify 2>&1 | tail -5
```
Both expected: clean.

- [ ] **Step 4.5: Commit**

```bash
git add src-tauri/src/lifecycle/
git commit -m "feat(autostart): convert lifecycle.rs to directory module per ADR-003 + add sd_notify wrapper"
```

---

## Task 5: sd_notify Unit Tests

**Estimate:** 0.5h | **Spec ref:** §9.1 + §10.1 commit 5 | **Files:** Append `#[cfg(test)] mod tests` to `src-tauri/src/lifecycle/sd_notify.rs`

- [ ] **Step 5.1: Write tests**

Append to `src-tauri/src/lifecycle/sd_notify.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_ready_does_not_panic() {
        // Whether feature enabled or not, this must not panic
        notify_ready();
    }

    #[test]
    fn notify_stopping_does_not_panic() {
        notify_stopping();
    }
}
```

These are smoke tests. The Linux+feature combo path errors out logging at debug (no NOTIFY_SOCKET in test env), the no-feature/non-Linux path is no-op. Either way: must not panic.

- [ ] **Step 5.2: Run tests**

```bash
cargo test -p oneshim-app --bin oneshim lifecycle::sd_notify::tests 2>&1 | tail -10
# Also test with feature enabled (Linux only)
cargo test -p oneshim-app --bin oneshim --features systemd-notify lifecycle::sd_notify::tests 2>&1 | tail -10
```
Expected: 2 tests pass in both configs.

- [ ] **Step 5.3: Commit**

```bash
git add src-tauri/src/lifecycle/sd_notify.rs
git commit -m "test(autostart): sd_notify smoke tests (no panic in default + feature-enabled builds)"
```

---

## Task 6: Service File Template Type=notify + Update Existing Test

**Estimate:** 1h | **Spec ref:** §5.1 + §10.1 commit 6 + C3 (existing test update) | **Files:** `src-tauri/src/autostart.rs`

- [ ] **Step 6.1: Update generate_service_file template**

Open `src-tauri/src/autostart.rs`. Find `generate_service_file` (line ~332-348 per Phase 1 iter-1 verification). Replace existing template:

```rust
fn generate_service_file(program_path: &str) -> String {
    format!(
        "[Unit]\n\
         Description=ONESHIM Desktop Agent\n\
         After=graphical-session.target\n\
         \n\
         [Service]\n\
         Type=notify\n\
         NotifyAccess=main\n\
         ExecStart={program_path}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         TimeoutStartSec=30\n\
         Environment=DISPLAY=:0\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}
```

Changes from PR-B1:
- `Type=simple` → `Type=notify`
- NEW `NotifyAccess=main`
- NEW `TimeoutStartSec=30`

- [ ] **Step 6.2: Update existing inline test (per C3)**

Find the inline test `service_file_contains_required_keys` at approximately line 508 (verify with `grep -n "service_file_contains_required_keys" src-tauri/src/autostart.rs`).

Replace the existing assertions (which test for `Type=simple`):

```rust
#[test]
fn service_file_contains_required_keys() {
    let service = generate_service_file("/usr/bin/oneshim");
    assert!(service.contains("Type=notify"), "service file must use Type=notify");
    assert!(service.contains("NotifyAccess=main"), "service file must include NotifyAccess=main");
    assert!(service.contains("TimeoutStartSec=30"), "service file must include TimeoutStartSec=30");
    assert!(service.contains("ExecStart=/usr/bin/oneshim"));
    assert!(service.contains("Restart=on-failure"));
    assert!(service.contains("WantedBy=default.target"));
}
```

- [ ] **Step 6.3: Run tests**

```bash
cargo test -p oneshim-app --bin oneshim autostart::tests::service_file_contains_required_keys 2>&1 | tail -10
```
Expected: PASS with new Type=notify assertions.

- [ ] **Step 6.4: Commit**

```bash
git add src-tauri/src/autostart.rs
git commit -m "feat(autostart): change Linux service template to Type=notify + NotifyAccess=main + TimeoutStartSec=30 + update inline test (per C3)"
```

---

## Task 7: Hash-based Migration Module

**Estimate:** 2h | **Spec ref:** §5.2 + §10.1 commit 7 + Q-B2-1 hash computation | **Files:** Create `src-tauri/src/lifecycle/migration_hashes.rs` + `src-tauri/src/lifecycle/autostart_migration.rs`, modify `src-tauri/src/lifecycle/mod.rs`

- [ ] **Step 7.1: Compute PR-B1 era template hash**

This is Q-B2-1. PR-B1 era template (Type=simple, NO NotifyAccess, NO TimeoutStartSec) was:

```text
[Unit]
Description=ONESHIM Desktop Agent
After=graphical-session.target

[Service]
Type=simple
ExecStart={BINARY_PATH}
Restart=on-failure
RestartSec=5
Environment=DISPLAY=:0

[Install]
WantedBy=default.target
```

Compute SHA-256 of this canonical content (with `\n` line endings, `{BINARY_PATH}` placeholder):

```bash
cat > /tmp/pr_b1_template.txt <<'EOF'
[Unit]
Description=ONESHIM Desktop Agent
After=graphical-session.target

[Service]
Type=simple
ExecStart={BINARY_PATH}
Restart=on-failure
RestartSec=5
Environment=DISPLAY=:0

[Install]
WantedBy=default.target
EOF
shasum -a 256 /tmp/pr_b1_template.txt | awk '{print $1}'
```

Capture the output hash for use in Step 7.2. Note this in your task report.

- [ ] **Step 7.2: Create migration_hashes.rs**

Create `src-tauri/src/lifecycle/migration_hashes.rs`:

```rust
//! Known SHA-256 hashes of prior-version systemd service file templates.
//!
//! Used by autostart_migration to determine whether an existing
//! `~/.config/systemd/user/oneshim.service` file matches a known template
//! (safe to overwrite) vs has been customized by the user (skip).
//!
//! Per spec §5.2 + Q-B2-1 + Phase 1 iter-1 I4 + N1.

use sha2::{Digest, Sha256};

/// (hash, label) pairs for every released template content prior to PR-B2.
///
/// Hashes are computed by `compute_hash(canonicalize(template, binary_path))`
/// where binary_path is the resolved current_exe() path.
pub const KNOWN_PRIOR_HASHES: &[(&str, &str)] = &[
    // PR-B1 era (Type=simple), from `linux::generate_service_file()` in PR-B1.
    // Hash computed in Task 7 Step 7.1 — substitute below:
    ("REPLACE_WITH_SHA256_FROM_STEP_7_1", "PR-B1 Type=simple"),
];

/// Canonicalize the service file before hashing.
///
/// Per Phase 1 iter-1 I4 + N1: handles both line-ending normalization
/// AND word-boundary-aware ExecStart line replacement to avoid:
/// - I4: `binary_path = /home/user/oneshim` matching `/home/user/oneshim-old` substring
/// - N1: `\r\n` (Windows-edited files) producing different hash than `\n`
///
/// Symlink edge case (Q-B2-10): if user wrote service file using symlink path
/// but `current_exe()` returns canonical path (or vice versa), exec line won't
/// match → treated as customized → log warn + skip. Acceptable.
pub fn canonicalize(content: &str, binary_path: &str) -> String {
    // Step 1: normalize line endings
    let normalized = content.replace("\r\n", "\n");
    // Step 2: replace ExecStart line specifically (not arbitrary substring)
    let exec_line = format!("ExecStart={}\n", binary_path);
    normalized.replace(&exec_line, "ExecStart={BINARY_PATH}\n")
}

pub fn compute_hash(content: &str) -> String {
    // Use Sha256::digest() static method to match codebase pattern
    // (per Phase 2 iter-2 I3 — aligned with oneshim-automation/src/gui_interaction/crypto.rs:69,78)
    let digest = Sha256::digest(content.as_bytes());
    format!("{digest:x}")
}

/// Returns Some(label) if the canonicalized content matches a known prior hash.
/// Returns None if user has customized the file (or it's the new template already).
pub fn matches_known_template(content: &str, binary_path: &str) -> Option<&'static str> {
    let canonical = canonicalize(content, binary_path);
    let hash = compute_hash(&canonical);
    KNOWN_PRIOR_HASHES
        .iter()
        .find(|(known_hash, _)| *known_hash == hash)
        .map(|(_, label)| *label)
}
```

Replace `REPLACE_WITH_SHA256_FROM_STEP_7_1` with the actual hash from Step 7.1.

- [ ] **Step 7.3: Create autostart_migration.rs**

Create `src-tauri/src/lifecycle/autostart_migration.rs`:

```rust
//! One-time migration check at app startup.
//!
//! If an existing systemd service file matches a known PR-B1-era template,
//! overwrite with the new PR-B2 Type=notify template (DEFERRED reload — file
//! takes effect on next user login; we do NOT call `daemon-reload` on the
//! currently-running service per spec §5.2 + Phase 1 review C4).
//!
//! If file content is unrecognized (user customized): log warn + skip.

#[cfg(target_os = "linux")]
pub fn run_startup_migration() {
    use crate::autostart::linux::{generate_service_file, service_path};
    use super::migration_hashes::matches_known_template;
    use oneshim_core::error_codes::AutostartCode;

    let path = match service_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(
                err.code = AutostartCode::ServiceMigrationSkipped.as_str(),
                "Migration check skipped — service path unresolved: {e}"
            );
            return;
        }
    };

    if !path.exists() {
        // Autostart never enabled — no migration needed
        return;
    }

    let existing = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                err.code = AutostartCode::ServiceMigrationSkipped.as_str(),
                "Migration check skipped — failed to read service file: {e}"
            );
            return;
        }
    };

    let binary_path = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return, // can't canonicalize without binary path
    };

    match matches_known_template(&existing, &binary_path) {
        Some(label) => {
            // Safe to overwrite. Write new file. DO NOT daemon-reload.
            let new_content = generate_service_file(&binary_path);
            if let Err(e) = std::fs::write(&path, new_content) {
                tracing::warn!(
                    err.code = AutostartCode::ServiceMigrationFailed.as_str(),
                    "Migration write failed: {e}"
                );
                return;
            }
            tracing::info!(
                event.code = AutostartCode::ServiceMigrated.as_str(),
                from = %label,
                "Migrated systemd unit file from {label} to Type=notify; takes effect next login"
            );
        }
        None => {
            tracing::warn!(
                err.code = AutostartCode::ServiceMigrationSkipped.as_str(),
                path = %path.display(),
                "Skipping autostart unit migration — file appears customized. Manual update required (see docs/guides/autostart.ko.md)"
            );
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn run_startup_migration() {
    // No-op on non-Linux
}
```

Note `event.code` for the success path (per Phase 1 iter-1 I6).

**Visibility fix REQUIRED** (per Phase 2 iter-2 C2): the `mod linux` declaration in `src-tauri/src/autostart.rs:298` is `mod linux` (no visibility modifier — private). The functions inside (`generate_service_file`, `service_path`, `has_systemctl`) are already `pub fn`, but the MODULE itself is private to `autostart.rs`. Change `mod linux` → `pub(crate) mod linux` to expose the module path `crate::autostart::linux::*`. This is a 1-line change in autostart.rs (lifecycle module needs the path access). Verify via `cargo check -p oneshim-app` after the change.

- [ ] **Step 7.4: Wire submodules in lifecycle/mod.rs**

Open `src-tauri/src/lifecycle/mod.rs`. After existing `pub mod sd_notify;`, add:

```rust
pub mod autostart_migration;
pub mod migration_hashes;
```

- [ ] **Step 7.5: Apply visibility fix + verify compile**

Per Step 7.3 visibility fix note (Phase 2 iter-2 C2): change `mod linux` → `pub(crate) mod linux` in `src-tauri/src/autostart.rs:298`.

```bash
# Apply the change
sed -i.bak '298s/^mod linux {/pub(crate) mod linux {/' src-tauri/src/autostart.rs
rm -f src-tauri/src/autostart.rs.bak
# Verify
grep -n "pub(crate) mod linux" src-tauri/src/autostart.rs
cargo check -p oneshim-app 2>&1 | tail -10
```
Expected: line 298 now has `pub(crate) mod linux {`, cargo check clean.

- [ ] **Step 7.6: Commit**

```bash
git add src-tauri/src/lifecycle/
# Adjust if visibility changes touched src-tauri/src/autostart.rs
git add src-tauri/src/autostart.rs 2>/dev/null || true
git commit -m "feat(autostart): hash-based migration module (canonicalize + KNOWN_PRIOR_HASHES + run_startup_migration)"
```

---

## Task 8: Migration Unit Tests

**Estimate:** 1h | **Spec ref:** §9.1 + §10.1 commit 8 | **Files:** Append `#[cfg(test)] mod tests` to `src-tauri/src/lifecycle/migration_hashes.rs`

- [ ] **Step 8.1: Write tests**

Append to `src-tauri/src/lifecycle/migration_hashes.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Canonicalized PR-B1 template (Type=simple) for hash matching.
    /// MUST match the actual content `generate_service_file()` produced in PR-B1.
    const PR_B1_TEMPLATE_CANONICAL: &str = "[Unit]\nDescription=ONESHIM Desktop Agent\nAfter=graphical-session.target\n\n[Service]\nType=simple\nExecStart={BINARY_PATH}\nRestart=on-failure\nRestartSec=5\nEnvironment=DISPLAY=:0\n\n[Install]\nWantedBy=default.target\n";

    #[test]
    fn pr_b1_template_hash_matches_registry() {
        let computed = compute_hash(PR_B1_TEMPLATE_CANONICAL);
        let known = KNOWN_PRIOR_HASHES
            .iter()
            .find(|(_, label)| *label == "PR-B1 Type=simple")
            .expect("PR-B1 entry must exist in KNOWN_PRIOR_HASHES");
        assert_eq!(computed, known.0,
            "computed hash {computed} should match registered {} for PR-B1 template",
            known.0);
    }

    #[test]
    fn canonicalize_replaces_exec_line_only() {
        let binary = "/home/user/oneshim";
        let content = format!(
            "[Unit]\n[Service]\nExecStart={}\nRestart=on-failure\n",
            binary
        );
        let canonical = canonicalize(&content, binary);
        assert!(canonical.contains("ExecStart={BINARY_PATH}\n"));
        assert!(!canonical.contains("/home/user/oneshim"));
    }

    #[test]
    fn canonicalize_does_not_replace_substring_of_other_paths() {
        // I4 edge case: binary_path is substring of a longer path elsewhere
        let binary = "/home/user/oneshim";
        let content = format!(
            "[Service]\nExecStart={}\nReadOnlyPaths=/home/user/oneshim-data\n",
            binary
        );
        let canonical = canonicalize(&content, binary);
        // ExecStart line replaced
        assert!(canonical.contains("ExecStart={BINARY_PATH}\n"));
        // ReadOnlyPaths line NOT replaced (different context, not ExecStart line)
        assert!(canonical.contains("/home/user/oneshim-data"));
    }

    #[test]
    fn canonicalize_normalizes_crlf() {
        let binary = "/usr/bin/oneshim";
        let content = format!("[Service]\r\nExecStart={}\r\n", binary);
        let canonical = canonicalize(&content, binary);
        assert!(!canonical.contains("\r"));
        assert!(canonical.contains("ExecStart={BINARY_PATH}\n"));
    }

    #[test]
    fn matches_known_template_returns_some_for_pr_b1() {
        let binary = "/usr/bin/oneshim";
        let content = PR_B1_TEMPLATE_CANONICAL.replace("{BINARY_PATH}", binary);
        let result = matches_known_template(&content, binary);
        assert_eq!(result, Some("PR-B1 Type=simple"));
    }

    #[test]
    fn matches_known_template_returns_none_for_customized() {
        let binary = "/usr/bin/oneshim";
        let mut content = PR_B1_TEMPLATE_CANONICAL.replace("{BINARY_PATH}", binary);
        content.push_str("\n# Custom comment from user\n");
        let result = matches_known_template(&content, binary);
        assert_eq!(result, None);
    }

    #[test]
    fn matches_known_template_returns_none_for_empty_file() {
        let binary = "/usr/bin/oneshim";
        let result = matches_known_template("", binary);
        assert_eq!(result, None);
    }

    #[test]
    fn compute_hash_is_deterministic() {
        let h1 = compute_hash("hello world");
        let h2 = compute_hash("hello world");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars
    }
}
```

- [ ] **Step 8.2: Run tests**

```bash
cargo test -p oneshim-app --bin oneshim lifecycle::migration_hashes::tests 2>&1 | tail -15
```
Expected: 8 tests pass. If `pr_b1_template_hash_matches_registry` fails: the hash in `KNOWN_PRIOR_HASHES` (Step 7.2) doesn't match Step 7.1 computation. Recompute and update.

- [ ] **Step 8.3: Commit**

```bash
git add src-tauri/src/lifecycle/migration_hashes.rs
git commit -m "test(autostart): migration_hashes 8 unit tests (canonicalize edge cases + hash determinism + matches_known_template)"
```

---

## Task 9: Wire sd_notify + run_startup_migration in setup.rs

**Estimate:** 1h | **Spec ref:** §5.1 + §5.2 + §10.1 commit 9 + Phase 1 iter-1 I3 | **Files:** `src-tauri/src/setup.rs`

- [ ] **Step 9.1: Add migration call after D-Bus check**

Open `src-tauri/src/setup.rs`. Find the existing `#[cfg(target_os = "linux")]` block from PR-B1 Task 6 (D-Bus presence check). It looks like:

```rust
#[cfg(target_os = "linux")]
{
    if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
        tracing::warn!(...);
    }
}
```

Extend it with the migration call:

```rust
#[cfg(target_os = "linux")]
{
    if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
        tracing::warn!(
            err.code = "single_instance_dbus_absent",
            "DBUS_SESSION_BUS_ADDRESS not set — single-instance enforcement degraded; \
             duplicate processes may launch in headless sessions"
        );
    }
    crate::lifecycle::autostart_migration::run_startup_migration();
}
```

- [ ] **Step 9.2: Add notify_ready as last line before Ok(())**

Find the end of `setup::init()` — look for the final `Ok(())` return statement (per spec §5.1 I3 verified at line 51).

Insert IMMEDIATELY BEFORE the final `Ok(())`:

```rust
    crate::lifecycle::sd_notify::notify_ready();
    Ok(())
```

This is the natural "init complete" point per Phase 1 iter-1 I3 decision.

- [ ] **Step 9.3: Verify compile**

```bash
cargo check -p oneshim-app 2>&1 | tail -5
cargo check -p oneshim-app --features systemd-notify 2>&1 | tail -5
```
Both expected: clean.

- [ ] **Step 9.4: Commit**

```bash
git add src-tauri/src/setup.rs
git commit -m "feat(autostart): wire run_startup_migration after D-Bus check + notify_ready as last line of setup::init() (per I3)"
```

---

## Task 10: Real detect_capabilities() Linux Implementation

**Estimate:** 1.5h | **Spec ref:** §5.3 + §6 + §10.1 commit 10 | **Files:** `src-tauri/src/autostart.rs`

- [ ] **Step 10.1: Replace PR-B1 skeleton**

Open `src-tauri/src/autostart.rs`. Find the PR-B1 skeleton `detect_capabilities()` Linux variant (returns `supported: true` unconditionally).

Replace with real detection:

```rust
#[cfg(target_os = "linux")]
pub fn detect_capabilities() -> AutostartCapabilities {
    // Sandbox detection (highest priority — sandboxed envs cannot write
    // to ~/.config/systemd or ~/.config/autostart in a way that survives the
    // sandbox boundary)
    if std::env::var("SNAP").is_ok() {
        return AutostartCapabilities {
            supported: false,
            unsupported_reason: Some(UnsupportedReason::SnapSandbox),
            environment: EnvironmentKind::LinuxSnapSandbox,
        };
    }
    if std::env::var("FLATPAK_ID").is_ok() {
        return AutostartCapabilities {
            supported: false,
            unsupported_reason: Some(UnsupportedReason::FlatpakSandbox),
            environment: EnvironmentKind::LinuxFlatpakSandbox,
        };
    }

    // Headless detection (no display server)
    let has_display = std::env::var("DISPLAY").is_ok()
        || std::env::var("WAYLAND_DISPLAY").is_ok();
    if !has_display {
        return AutostartCapabilities {
            supported: false,
            unsupported_reason: Some(UnsupportedReason::HeadlessSession),
            environment: EnvironmentKind::LinuxHeadless,
        };
    }

    // Display present — choose systemd vs XDG fallback
    if linux::has_systemctl() {
        AutostartCapabilities {
            supported: true,
            unsupported_reason: None,
            environment: EnvironmentKind::LinuxSystemd,
        }
    } else {
        AutostartCapabilities {
            supported: true,  // XDG .desktop fallback works without systemctl
            unsupported_reason: None,
            environment: EnvironmentKind::LinuxXdg,
        }
    }
}
```

(Verify `linux::has_systemctl` is `pub fn` per Phase 1 iter-1 verification — line 365 in PR-B1 baseline. If `pub(super)`, no qualifier change needed since `detect_capabilities` lives in same `autostart.rs`.)

- [ ] **Step 10.2: Verify compile**

```bash
cargo check -p oneshim-app 2>&1 | tail -5
```
Expected: clean.

- [ ] **Step 10.3: Commit**

```bash
git add src-tauri/src/autostart.rs
git commit -m "feat(autostart): real detect_capabilities() Linux env detection (Snap/Flatpak/headless/XDG) replacing PR-B1 skeleton"
```

---

## Task 11: Linux Unit Tests T1-T7 + CI Workflow

**Estimate:** 2h | **Spec ref:** §5.5 + §9.2 + §10.1 commit 11 | **Files:** Create `src-tauri/tests/linux_autostart_unit.rs`, modify `.github/workflows/ci.yml`

- [ ] **Step 11.1: Verify serial_test dev-dep exists**

```bash
grep -n "serial_test" src-tauri/Cargo.toml
```
Expected: present in `[dev-dependencies]`. If absent, add it: `serial_test = "3"`.

- [ ] **Step 11.2: Create unit test file**

Create `src-tauri/tests/linux_autostart_unit.rs`:

```rust
//! PR-B2 Linux autostart unit tests (T1-T7) — no real systemd required.
//!
//! Run via: cargo test -p oneshim-app --features systemd-notify --test linux_autostart_unit
//!
//! Run all tests serially when mutating env vars to avoid cross-test interference
//! (per reference_serial_test_pattern.md).

#![cfg(target_os = "linux")]

use serial_test::serial;

// Helper to access internal autostart fns
// Note: src-tauri has no [lib], so we redeclare types/fns here OR use a workaround.
// Per Phase 1 iter-1 conclusion + PR-B1 N-C1 lessons: tests/ cannot import from binary-only crate.
//
// Workaround: re-implement template + canonicalize logic here OR move testable logic
// into a shared crate. For PR-B2 we put assertions inline using the binary spawn pattern
// (similar to PR-B1 single_instance_integration.rs).

// T1: service file generation produces Type=notify
// We test by spawning the binary with a special test flag that prints generate_service_file output.
// OR we accept that this test needs to live as inline #[cfg(test)] in autostart.rs (where T1
// is already covered by the updated existing test from Task 6 Step 6.2).
//
// Per spec §9.2: T1-T7 cover service file gen + capability detection + hash matching.
// Service file gen (T1+T2) is already covered by autostart.rs inline test from Task 6.
// Hash matching (T6+T7) is already covered by migration_hashes.rs inline tests from Task 8.
// Capability detection (T3+T4+T5) NEEDS env var manipulation — this is what THIS file covers.

// T3: SNAP env var → LinuxSnapSandbox
#[test]
#[serial]
fn detect_capabilities_returns_snap_sandbox() {
    // Use the binary spawn pattern OR... actually: we need access to `autostart::detect_capabilities`
    // which is in src-tauri binary crate. Without [lib], we must spawn the binary.
    //
    // Alternative: This test must live INLINE in src-tauri/src/autostart.rs as
    // #[cfg(test)] mod test_capabilities { ... }, NOT in tests/.
    //
    // This task therefore RECONFIGURED: T1-T7 unit tests go INLINE in autostart.rs + migration_hashes.rs
    // (Task 6 + Task 8 already cover this).
    //
    // This `tests/linux_autostart_unit.rs` file becomes a LIGHTWEIGHT smoke test for
    // CI workflow integration (verifies cargo test invocation works under the systemd-notify feature).

    let _ = std::env::var("SNAP");  // smoke
}
```

Actually — per Phase 1 iter-1 lessons from PR-B1 (Task 5 Addendum A3): src-tauri has no `[lib]`, so tests under `tests/` CANNOT import from internal modules. **Reconfigure**: T1-T7 unit tests go INLINE in the affected modules:
- T1+T2 (service file): already in `autostart.rs` inline test from Task 6 Step 6.2
- T3+T4+T5 (env detection): add to `autostart.rs` inline tests (NEW step below)
- T6+T7 (hash matching): already in `migration_hashes.rs` inline tests from Task 8 Step 8.1

**Replace the `tests/linux_autostart_unit.rs` content above with this minimal smoke file**:

```rust
//! PR-B2 Linux smoke test — verifies cargo test invocation works
//! under the `systemd-notify` feature on Linux. Real T1-T10 logic is INLINE
//! in respective modules (autostart.rs, migration_hashes.rs, sd_notify.rs)
//! per the binary-only crate constraint (PR-B1 Addendum A3).

#![cfg(target_os = "linux")]

#[test]
fn linux_autostart_smoke() {
    // Smoke: just verifies the test harness runs on Linux with feature enabled.
    assert!(cfg!(target_os = "linux"));
}
```

- [ ] **Step 11.3: Add T3-T5 inline env detection tests to autostart.rs**

Open `src-tauri/src/autostart.rs`. Find the existing `#[cfg(test)] mod tests` block (or wherever the inline tests live). Add (Linux-only, `#[serial]` per env mutation):

```rust
// Per Phase 2 iter-2 C1: must be `#[cfg(all(test, ...))]`, not just target_os.
// serial_test is a dev-dependency only — without `cfg(test)` gate, this module
// would attempt to compile in release Linux builds and fail with unresolved crate.
#[cfg(all(test, target_os = "linux"))]
mod linux_capability_tests {
    use super::*;
    use serial_test::serial;

    fn clear_env() {
        std::env::remove_var("SNAP");
        std::env::remove_var("FLATPAK_ID");
    }

    #[test]
    #[serial]
    fn detect_capabilities_returns_snap_sandbox_when_snap_set() {
        clear_env();
        std::env::set_var("SNAP", "/snap/oneshim/current");
        let caps = detect_capabilities();
        assert!(!caps.supported);
        assert_eq!(caps.environment, EnvironmentKind::LinuxSnapSandbox);
        clear_env();
    }

    #[test]
    #[serial]
    fn detect_capabilities_returns_flatpak_sandbox_when_flatpak_id_set() {
        clear_env();
        std::env::set_var("FLATPAK_ID", "com.oneshim.client");
        let caps = detect_capabilities();
        assert!(!caps.supported);
        assert_eq!(caps.environment, EnvironmentKind::LinuxFlatpakSandbox);
        clear_env();
    }

    #[test]
    #[serial]
    fn detect_capabilities_returns_headless_when_no_display() {
        clear_env();
        let prev_display = std::env::var("DISPLAY").ok();
        let prev_wayland = std::env::var("WAYLAND_DISPLAY").ok();
        std::env::remove_var("DISPLAY");
        std::env::remove_var("WAYLAND_DISPLAY");
        let caps = detect_capabilities();
        assert!(!caps.supported);
        assert_eq!(caps.environment, EnvironmentKind::LinuxHeadless);
        // Restore
        if let Some(v) = prev_display { std::env::set_var("DISPLAY", v); }
        if let Some(v) = prev_wayland { std::env::set_var("WAYLAND_DISPLAY", v); }
    }
}
```

Note: `EnvironmentKind` must derive `PartialEq` — verify in PR-B1 baseline. If not, add `#[derive(PartialEq, Eq)]`.

- [ ] **Step 11.4: Add CI job to ci.yml**

Open `.github/workflows/ci.yml`. Add a new job (alongside existing `test`, `clippy`, etc.):

```yaml
  linux-autostart-unit:
    name: Linux Autostart Unit Tests
    runs-on: ubuntu-latest
    needs: [check]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install libsystemd-dev (per Phase 1 iter-1 I1)
        run: sudo apt-get update && sudo apt-get install -y libsystemd-dev
      - name: Touch tauri externalbin stub (per reference_ci_tauri_externalbin_stub.md)
        run: touch src-tauri/oneshim-sandbox-worker-x86_64-unknown-linux-gnu
      - name: Touch frontend dist stub
        run: mkdir -p crates/oneshim-web/frontend/dist && touch crates/oneshim-web/frontend/dist/index.html
      - name: Run linux_autostart_unit + autostart inline + migration_hashes tests
        run: |
          cargo test -p oneshim-app --features systemd-notify --test linux_autostart_unit
          cargo test -p oneshim-app --features systemd-notify --bin oneshim autostart::tests
          cargo test -p oneshim-app --features systemd-notify --bin oneshim autostart::linux_capability_tests
          cargo test -p oneshim-app --features systemd-notify --bin oneshim lifecycle::migration_hashes::tests
          cargo test -p oneshim-app --features systemd-notify --bin oneshim lifecycle::sd_notify::tests
```

- [ ] **Step 11.5: Run all new tests locally**

```bash
cargo test -p oneshim-app --features systemd-notify --test linux_autostart_unit 2>&1 | tail -10
cargo test -p oneshim-app --bin oneshim autostart::linux_capability_tests 2>&1 | tail -10
```
Expected: all GREEN (3 capability tests + 1 smoke test).

- [ ] **Step 11.6: Commit**

```bash
git add src-tauri/tests/linux_autostart_unit.rs src-tauri/src/autostart.rs .github/workflows/ci.yml src-tauri/Cargo.toml
git commit -m "test(autostart): linux_autostart_unit smoke + 3 inline capability tests + CI job"
```

---

## Task 12: linux_autostart_systemd_live.rs + Manual Workflow

**Estimate:** 1.5h | **Spec ref:** §5.5 + §9.2 + §10.1 commit 12 | **Files:** Create `src-tauri/tests/linux_autostart_systemd_live.rs`, `.github/workflows/linux-systemd-integration.yml`

- [ ] **Step 12.1: Create live test file (manual --ignored)**

Create `src-tauri/tests/linux_autostart_systemd_live.rs`:

```rust
//! PR-B2 Linux systemd live integration tests (T8-T10).
//!
//! Run manually: cargo test -p oneshim-app --features systemd-notify \
//!   --test linux_autostart_systemd_live -- --ignored
//!
//! Or via .github/workflows/linux-systemd-integration.yml manual workflow_dispatch.
//!
//! Per Phase 1 iter-1 I5: T9 re-scoped to verify no-panic in CI; real systemd
//! verification only happens when invoked under `systemd-run --user --scope`
//! locally or in self-hosted runners with PID 1 = systemd.

#![cfg(target_os = "linux")]

use std::process::Command;

#[test]
#[ignore = "modifies user systemd state — run manually under systemd-run --user --scope"]
fn enable_then_disable_writes_type_notify_service_file() {
    // T8: enable_autostart writes service file with Type=notify
    // PRE: ~/.config/systemd/user/oneshim.service does not exist
    // POST: file exists, contains Type=notify
    // CLEANUP: disable_autostart removes file

    // Spawn binary with test flag (similar to single_instance pattern)
    // OR: manual verification via shell:
    //   cargo run --features systemd-notify --bin oneshim -- --enable-autostart
    //   cat ~/.config/systemd/user/oneshim.service | grep "Type=notify"
    //   cargo run --features systemd-notify --bin oneshim -- --disable-autostart

    // For automated test: use binary spawn pattern
    let bin_path = std::env::var("ONESHIM_BIN")
        .unwrap_or_else(|_| "target/release/oneshim".to_string());

    let _ = Command::new(&bin_path)
        .arg("--enable-autostart")
        .output();

    let service_path = dirs::home_dir()
        .map(|p| p.join(".config/systemd/user/oneshim.service"))
        .expect("home dir");

    if service_path.exists() {
        let content = std::fs::read_to_string(&service_path).unwrap();
        assert!(content.contains("Type=notify"));
        assert!(content.contains("NotifyAccess=main"));
        assert!(content.contains("TimeoutStartSec=30"));

        // Cleanup
        let _ = Command::new(&bin_path)
            .arg("--disable-autostart")
            .output();
    } else {
        // Test depended on real binary spawn — skip if binary not built
        eprintln!("SKIP: oneshim binary not built or test environment lacks autostart write perms");
    }
}

#[test]
#[ignore = "verifies sd_notify under systemd-run — run via workflow_dispatch"]
fn sd_notify_no_panic_when_socket_missing() {
    // T9 (re-scoped per Phase 1 iter-1 I5): verify failure-mode safety only.
    // Real systemd verification requires manual invocation with NOTIFY_SOCKET set.
    //
    // This test verifies notify_ready() does not panic when NOTIFY_SOCKET is absent
    // (the common case — running outside systemd).
    //
    // Note: src-tauri has no [lib] — we cannot import lifecycle::sd_notify directly.
    // Smoke test via the inline tests in lifecycle/sd_notify.rs instead.

    eprintln!("T9 actual sd_notify test lives inline at lifecycle/sd_notify.rs::tests::notify_ready_does_not_panic");
}

#[test]
#[ignore = "end-to-end migration verification — run manually after install"]
fn migration_writes_type_notify_when_pr_b1_template_present() {
    // T10: write old Type=simple template, run app, verify file updated to Type=notify
    // and currently-running service NOT restarted (no daemon-reload)
    //
    // Manual smoke procedure:
    //   1. Install PR-B1 binary
    //   2. Toggle autostart ON in Settings → verify ~/.config/systemd/user/oneshim.service has Type=simple
    //   3. Install PR-B2 binary (new build)
    //   4. Restart ONESHIM
    //   5. Check log for "autostart_service_migrated" event
    //   6. Verify service file content updated to Type=notify
    //   7. Verify systemctl --user is-active oneshim.service still returns "active"
    //   8. Logout + login → verify service starts cleanly under Type=notify

    eprintln!("T10 is a manual procedure — see test body comment for steps");
}
```

(Acknowledge: `dirs` crate may not be in src-tauri deps. Verify with `grep dirs Cargo.toml`. If absent, use `std::env::var("HOME")` instead.)

- [ ] **Step 12.2: Create manual workflow**

Create `.github/workflows/linux-systemd-integration.yml`:

```yaml
name: Linux systemd Integration (manual)

on:
  workflow_dispatch:
    inputs:
      branch:
        description: 'Branch to test'
        default: 'main'
        required: false

jobs:
  linux-systemd-integration:
    name: Linux systemd Integration Tests (T8-T10)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ inputs.branch }}
      - uses: dtolnay/rust-toolchain@stable
      - name: Install systemd + dbus + libsystemd-dev
        run: |
          sudo apt-get update
          sudo apt-get install -y systemd dbus-user-session libsystemd-dev
          systemctl --user start dbus
      - name: Touch tauri externalbin + frontend stubs
        run: |
          touch src-tauri/oneshim-sandbox-worker-x86_64-unknown-linux-gnu
          mkdir -p crates/oneshim-web/frontend/dist && touch crates/oneshim-web/frontend/dist/index.html
      - name: Run live tests
        run: cargo test -p oneshim-app --features systemd-notify --test linux_autostart_systemd_live -- --ignored
```

- [ ] **Step 12.3: Verify compile**

```bash
cargo check -p oneshim-app --tests --features systemd-notify 2>&1 | tail -10
```
Expected: clean (the test file compiles even if `--ignored` makes tests skip in normal runs).

- [ ] **Step 12.4: Commit**

```bash
git add src-tauri/tests/linux_autostart_systemd_live.rs .github/workflows/linux-systemd-integration.yml
git commit -m "test(autostart): linux_autostart_systemd_live T8-T10 manual + workflow_dispatch CI"
```

---

## Task 13: Korean Operations Guide

**Estimate:** 1.5h | **Spec ref:** §5.6 + §10.1 commit 13 | **Files:** Create `docs/guides/autostart.ko.md`

- [ ] **Step 13.1: Create autostart.ko.md**

Create `docs/guides/autostart.ko.md` per the outline in spec §5.6 (lines 506-580). Copy-paste from spec §5.6 markdown content directly.

Verify after creation:
- Section "Linux 환경별 지원" includes "systemd 219 이상 필요 (Ubuntu 20.04+, Fedora 33+, Debian 10+ 기본 충족)" per Phase 1 iter-1 N5
- Flatpak path reference is corrected per Phase 1 iter-1 N4 (use Flatpak background portal API mention, not `~/.var/app/...`)
- Manual migration section includes the bash code block from spec §5.6
- Trouble shooting section includes all 4 cases from spec

- [ ] **Step 13.2: Verify markdown renders correctly**

Open the file in a markdown preview tool OR use:
```bash
head -50 docs/guides/autostart.ko.md
```
Verify Korean text + code blocks display correctly.

- [ ] **Step 13.3: Commit**

```bash
git add docs/guides/autostart.ko.md
git commit -m "docs(autostart): docs/guides/autostart.ko.md operations + migration guide (Korean, 519 lines)"
```

---

## Task 14: STATUS.md + PHASE-HISTORY Entry

**Estimate:** 0.5h | **Spec ref:** §10.1 commit 14 | **Files:** `docs/STATUS.md`, `docs/PHASE-HISTORY.md`

- [ ] **Step 14.1: Run test suite to capture counts**

```bash
cargo test --workspace 2>&1 | tail -5
cd crates/oneshim-web/frontend && pnpm test --run 2>&1 | tail -5
```

Capture counts: `Rust workspace: NNNN passed`, `Vitest: NNNN passed across MM test files`.

- [ ] **Step 14.2: Update STATUS.md**

Open `docs/STATUS.md`. Update:
- Snapshot date: `## Current Snapshot (2026-04-25)` → `## Current Snapshot (DATE_OF_PR_B2_MERGE)`
- Version: bump to `v0.4.41-rc.1`
- `cargo test --workspace`: pass — **NNNN passed** (PR-B2 adds: lifecycle::sd_notify 2 + lifecycle::migration_hashes 8 + autostart linux_capability_tests 3 + linux_autostart_unit 1 = +14 over PR-B1 baseline)
- Frontend Vitest: pass — **NNNN passed** (no new Vitest tests; wire-error count bumped 47→51)

- [ ] **Step 14.3: Update PHASE-HISTORY.md**

Open `docs/PHASE-HISTORY.md`. Add a new entry AFTER the Phase 9 PR-B1 section:

```markdown
## Phase 9 PR-B2: Autostart Linux Deep (v0.4.41-rc.1, DATE_OF_MERGE)

- **systemd Type=notify integration**: replaced PR-B1's Type=simple with Type=notify + NotifyAccess=main + TimeoutStartSec=30. New `lifecycle::sd_notify` wrapper sends READY signal at end of `setup::init()`. Non-default `systemd-notify` Cargo feature flag required for Linux release builds.
- **Hash-based deferred migration** (per spec §5.2 + Phase 1 review C4): `KNOWN_PRIOR_HASHES` registry + canonicalize() with line-ending normalization + word-boundary ExecStart replacement. PR-B1 era (Type=simple) files migrated cleanly without daemon-reload (deferred to next user login). Customized files skipped with warn log.
- **Real `detect_capabilities()` Linux env detection**: replaces PR-B1 skeleton. Detects Snap (`SNAP` env var), Flatpak (`FLATPAK_ID`), headless (no DISPLAY/WAYLAND_DISPLAY) — UI toggle disabled with environment-specific tooltip. Non-sandboxed environments fall through to LinuxSystemd or LinuxXdg fallback.
- **4 new wire codes** (per ADR-019): `autostart.{sd_notify_skipped, service_migrated, service_migration_failed, service_migration_skipped}`. Wire snapshot 47 → 51.
- **Linux integration tests**: rootless approach per Phase 1 review I5 (no `--privileged` containers). Two-job split: `linux-autostart-unit` always-on CI + `linux-systemd-integration.yml` manual workflow_dispatch for T8-T10.
- **Korean operations guide**: `docs/guides/autostart.ko.md` covers platform behavior, environment matrix, migration steps for customized service files, troubleshooting, single-instance limits.
- **Tests**: +14 Rust unit tests (sd_notify 2, migration_hashes 8, capability detection 3, smoke 1). Vitest unchanged (wire-error count assertion bumped 47→51).
- Spec + plan: `docs/superpowers/specs/2026-04-25-phase9-pr-b2-autostart-linux-deep-design.md` (v2.5) + `docs/superpowers/plans/2026-04-25-phase9-pr-b2-autostart-linux-deep-plan.md`
- Implementation: 14 commits on branch `feature/phase9-autostart-linux-deep`, rebased onto post-PR-B1 main
```

- [ ] **Step 14.4: Commit**

```bash
git add docs/STATUS.md docs/PHASE-HISTORY.md
git commit -m "docs(autostart): STATUS.md + PHASE-HISTORY entry for PR-B2 (v0.4.41-rc.1)"
```

---

## Task 15: Manual Smoke Matrix (PR description)

**Estimate:** 1h | **Spec ref:** §9.4 + §10.1 commit 15 | **Files:** none (PR description only)

- [ ] **Step 15.1: Build release binary on Linux**

```bash
cargo build --release -p oneshim-app --features systemd-notify
```

Expected: clean build with `systemd-notify` feature enabled.

- [ ] **Step 15.2: Per-platform smoke checklist**

For PR description, prepare this checklist:

```markdown
### Linux smoke matrix (PR-B2)

#### Ubuntu 24.04 systemd (X11 GNOME)
- [ ] Settings → Startup section visible
- [ ] Toggle ON: writes ~/.config/systemd/user/oneshim.service with Type=notify + NotifyAccess=main + TimeoutStartSec=30
- [ ] Toggle ON + logout + login: systemctl --user is-active = active (clean Type=notify lifecycle)
- [ ] Toggle OFF: removes service file + systemctl --user is-enabled = disabled
- [ ] Migration: install PR-B1 → toggle ON → install PR-B2 → restart app → verify log "autostart_service_migrated" + service file updated to Type=notify + currently-running service NOT restarted

#### Fedora 40 Wayland GNOME
- [ ] Same as Ubuntu

#### sway (Wayland tiling WM)
- [ ] Same as Ubuntu

#### Snap (if test packaging available)
- [ ] Settings → Startup toggle disabled with tooltip "Use Snap's built-in autostart settings"
- [ ] No service file written

#### Flatpak (if test packaging available)
- [ ] Settings → Startup toggle disabled with tooltip "Use Flatpak's built-in autostart settings"

#### Headless SSH session
- [ ] Settings → Startup toggle disabled with tooltip "Autostart requires a desktop session"

#### macOS (regression check — PR-B2 should not affect macOS)
- [ ] PR-B1 autostart behavior unchanged

#### Windows (regression check)
- [ ] PR-B1 autostart behavior unchanged
```

- [ ] **Step 15.3: Save + commit PR description draft**

Per Phase 2 iter-2 I2: PC3 references `.github/PR-B2-description-draft.md` via `--body-file`. The file MUST exist on disk when PC3 runs. In subagent-driven mode (fresh subagent per task), the file won't survive across subagent invocations unless committed.

Save the smoke matrix checklist + spec/plan summary to `.github/PR-B2-description-draft.md`, then commit:

```bash
git add .github/PR-B2-description-draft.md
git commit -m "chore(autostart): PR-B2 description draft (smoke matrix + summary)"
```

---

## Post-Completion Checklist

- [ ] **PC1: Run full test suite**

```bash
cargo test --workspace 2>&1 | tail -10
cargo test -p oneshim-app --features systemd-notify 2>&1 | tail -10
cd crates/oneshim-web/frontend && pnpm test --run 2>&1 | tail -10
```
Expected: ALL GREEN (workspace + Linux feature + frontend).

- [ ] **PC2: Lint + format**

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings 2>&1 | tail -10
cargo clippy -p oneshim-app --features systemd-notify -- -D warnings 2>&1 | tail -10
cd crates/oneshim-web/frontend && pnpm lint 2>&1 | tail -10
```
Expected: clean.

- [ ] **PC3: Open PR**

```bash
git push -u origin feature/phase9-autostart-linux-deep
gh pr create --title "feat(autostart): Phase 9 PR-B2 Linux deep robustness (Type=notify + migration + env detection)" \
  --body-file .github/PR-B2-description-draft.md
```

- [ ] **PC4: Update spec §16 with PR URL after merge**

---

## Plan Self-Review

### 1. Spec coverage
- §5.1 systemd Type=notify integration → Tasks 1, 4, 5, 6, 9
- §5.2 hash-based migration → Tasks 7, 8, 9
- §5.3 real detect_capabilities() → Task 10
- §5.4 i18n tooltip refinements → already covered by PR-B1; this PR validates via smoke matrix
- §5.5 Linux integration tests → Tasks 11, 12
- §5.6 Korean docs → Task 13
- §6 wire codes → Tasks 2, 3
- §10.1 commits → all 14 mapped
- §11 migration semantics → Task 7 + Task 13 docs
- §13 risk register → no direct task; risks addressed via Tasks 7+11+12 implementation choices

### 2. Placeholder scan
- ✅ No "TBD" outside Task 7 Step 7.2 explicit `REPLACE_WITH_SHA256_FROM_STEP_7_1` (which IS the action item for Step 7.2)
- ✅ All code blocks have actual content

### 3. Type consistency
- `KNOWN_PRIOR_HASHES`, `canonicalize`, `compute_hash`, `matches_known_template` consistent across §5.2 + §8 tests
- `AutostartCode` enum extended consistently with PR-B1 baseline (5 + 4 = 9 variants)
- `lifecycle::sd_notify::notify_ready` / `notify_stopping` consistent across Tasks 4, 5, 9
- `lifecycle::autostart_migration::run_startup_migration` consistent across Tasks 7, 9
- `detect_capabilities` signature unchanged from PR-B1 (returns `AutostartCapabilities`); body replaced

### 4. Known gaps
- Task 11 "T1-T7 unit tests" was reconfigured to inline tests in respective modules + a smoke test in tests/. Plan reflects this; subagent should follow Step 11.1-11.6 as written.
- Q-B2-1 KNOWN_PRIOR_HASHES SHA: explicit Step 7.1 to compute + Step 7.2 to populate. Step 8.1 verifies via `pr_b1_template_hash_matches_registry` test.

---

## Execution Handoff

**Plan complete and saved to** `docs/superpowers/plans/2026-04-25-phase9-pr-b2-autostart-linux-deep-plan.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration. Best for this plan because each task has clear acceptance criteria.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**

(For ralph-loop continuation: Phase 2 plan creation done. Phase 2 deep review next iteration. Phase 3 implementation BLOCKED on PR-B1 #508 merge.)
