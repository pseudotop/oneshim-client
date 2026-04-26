# Phase 9 PR-B2 — Autostart Linux Deep Robustness Design Spec

**Date:** 2026-04-25
**Version:** v2 (Phase 1 iter-1 review fixes applied: 3 Critical + 6 Important)
**Review history:**
- v1 (2026-04-25): initial extraction from PR-B1 spec §6
- v2 (2026-04-25): incorporates `.claude/pr-b2-review/phase1-iter1-findings.md` fixes
**Baseline:** main `0827e071` (post-PR #505 + cumulative Quick Wins + features2 #491)
**Target release:** v0.4.41-rc.1 (after PR-B1 #508 lands as v0.4.40-rc.1)
**Implementation gate:** PR-B1 (#508) MUST merge first — PR-B2 builds on PR-B1's `AutostartCapabilities` skeleton + `autostart_helper.rs` + `commands/autostart.rs` + `AutostartConfig` types
**Estimated effort:** ~15h (per PR-B1 spec §10.2 commit list)
**Source spec:** `feature/phase9-autostart-foundation` commit `48ffbfb5` §6 + §11.4 + §13

**Phase 1 review status:** initial extraction from PR-B1 spec; deep review iters pending

---

## 1. Background

PR-B1 (#508) shipped the cross-platform autostart foundation:
- 6 Tauri IPC commands wired to Settings UI toggle
- `tauri-plugin-single-instance` v2 single-instance enforcement
- Opt-in onboarding prompt after first 25-min productive session
- AutostartConfig (no enabled cache — OS state sole source of truth)
- Productive-session detection in monitor.rs via FocusBlockState
- AutostartCapabilities **skeleton** (returns `supported: true` unconditionally for cross-platform UI parity)

PR-B2 completes the Linux story by:
1. Replacing the systemd Type=simple service with Type=notify (proper readiness signaling)
2. Implementing real `detect_capabilities()` for Linux (Snap/Flatpak/headless detection vs the PR-B1 skeleton)
3. Adding capability-aware UI tooltip refinements per environment
4. Adding rootless Linux integration tests in CI
5. Publishing a Korean operations guide

PR-B2 is Linux-only. macOS + Windows are unaffected (no behavior change).

---

## 2. Goals & Non-Goals

### 2.1 Goals
1. **G1**: systemd `oneshim.service` unit uses `Type=notify` so systemd knows when initialization completes (not just process spawn)
2. **G2**: Existing PR-B1 users with `Type=simple` service files migrate safely without breaking running services or clobbering customizations
3. **G3**: Linux Settings UI toggle reflects environment capability (disabled with informative tooltip in Snap/Flatpak/headless)
4. **G4**: CI catches regressions in service file generation + capability detection without requiring `--privileged` containers (per PR-B1 review I5 security concern)
5. **G5**: Korean-speaking ops users have authoritative troubleshooting guide for Linux-specific autostart edge cases

### 2.2 Non-Goals
- **NG1**: Snap/Flatpak best-effort autostart (we detect-and-refuse cleanly per U4)
- **NG2**: D-Bus method exposure for external automation tools (still NG2 from PR-B1)
- **NG3**: macOS/Windows behavior changes (PR-B2 = Linux-only)
- **NG4**: KeepAlive=true / auto-restart on crash for systemd unit (defer; current `Restart=on-failure` + `RestartSec=5` retained)
- **NG5**: Wayland kept-hidden window fallback (PR-B1 risk register §13 accepts as known limitation; PR-B2 documents it but does not implement code fallback unless PR-B1 smoke matrix surfaces issue)

---

## 3. User-Locked Decisions Carried From PR-B1

These decisions were locked during PR-B1 brainstorming and continue to apply:

| ID | Decision | Continued PR-B2 implication |
|----|----------|-----------------------------|
| **U1** | Scope = full robustness + basic IPC additions | PR-B2 adds Linux deep robustness without changing IPC scope |
| **U4** | Linux env matrix = Detect + clean refusal (Snap/Flatpak/headless) | PR-B2 implements this detection (PR-B1 was skeleton) |
| **U5** | 2-PR split (PR-B1 foundation + PR-B2 Linux deep) | This is PR-B2 |

---

## 4. Architecture Overview

### 4.1 Components Modified vs Created

| Layer | Modified (existing from PR-B1) | NEW in PR-B2 |
|-------|--------------------------------|---------------|
| `src-tauri/Cargo.toml` | + `sd-notify = "0.4"` (Linux-only, optional via feature flag `systemd-notify`) | — |
| `src-tauri/src/autostart.rs` Linux mod | `generate_service_file()` template change; real `detect_capabilities()` impl | — |
| `src-tauri/src/setup.rs` | Call `lifecycle::sd_notify::notify_ready()` as LAST line of `init()` before `Ok(())` (per §5.1 I3 decision — NOT main.rs) | — |
| `src-tauri/src/commands/autostart.rs` | (no changes — already exposes `autostart_capabilities` IPC) | — |
| `src-tauri/src/lifecycle/sd_notify.rs` | — | NEW: sd-notify wrapper (no-op on non-Linux) |
| `src-tauri/src/lifecycle/migration_hashes.rs` | — | NEW: `KNOWN_PRIOR_HASHES: &[(&str, &str)]` registry |
| `src-tauri/src/lifecycle/autostart_migration.rs` (or in `setup.rs`) | — | NEW: startup migration check |
| `src-tauri/tests/linux_autostart_unit.rs` | — | NEW: T1-T7 unit tests |
| `src-tauri/tests/linux_autostart_systemd_live.rs` | — | NEW: T8-T10 manual integration tests |
| `crates/oneshim-web/frontend/src/i18n/locales/{en,ko}.json` | extend `settings.autostart.unsupported_*` keys with refined Linux variant copy | — |
| `.github/workflows/ci.yml` | + `linux-autostart-unit` job | — |
| `.github/workflows/linux-systemd-integration.yml` | — | NEW: manual workflow_dispatch |
| `docs/guides/autostart.ko.md` | — | NEW: Korean operations guide |

### 4.2 Key constraint: backward compatibility with PR-B1 users

PR-B1 ships service files with `Type=simple`. After PR-B2 deploys:
- New autostart enables write `Type=notify` directly (clean state)
- Existing `Type=simple` files require migration WITHOUT:
  - Breaking running services (systemd would expect READY signal it never gets → restart loop)
  - Destroying user customizations (e.g., custom `Environment=` lines)

The hash-based deferred migration (§5.2) handles both.

---

## 5. Components Detail

### 5.1 systemd Type=notify Integration

**File**: `src-tauri/Cargo.toml`

```toml
[dependencies]
sd-notify = { version = "0.4", optional = true }

[features]
default = [...existing...]
systemd-notify = ["dep:sd-notify"]
```

**File**: `src-tauri/src/autostart.rs` (Linux mod), update `generate_service_file()`:

```rust
fn generate_service_file(binary_path: &str) -> String {
    format!(r#"[Unit]
Description=ONESHIM Desktop Agent
After=graphical-session.target

[Service]
Type=notify
NotifyAccess=main
ExecStart={binary_path}
Restart=on-failure
RestartSec=5
TimeoutStartSec=30
Environment=DISPLAY=:0

[Install]
WantedBy=default.target
"#, binary_path = binary_path)
}
```

Changes from PR-B1:
- `Type=simple` → `Type=notify`
- New `NotifyAccess=main` (only main process can send notify; child processes ignored)
- New `TimeoutStartSec=30` (bounds startup window — fails unit if init >30s)

**Existing test update REQUIRED** (per Phase 1 iter-1 C3): `src-tauri/src/autostart.rs:508-509` has inline test `service_file_contains_required_keys` that asserts `Type=simple`. Commit 6 MUST update this assertion to `Type=notify` and add new assertions for `NotifyAccess=main` + `TimeoutStartSec=30`. Without this update the existing test will fail.

**systemd minimum version requirement** (per N5): `Type=notify` requires systemd 219+ (2015). All modern distros qualify (Ubuntu 20.04+, Fedora 33+, Debian 10+, RHEL 7+). Documented in `docs/guides/autostart.ko.md`.

**File**: `src-tauri/src/lifecycle/sd_notify.rs` (NEW)

```rust
//! systemd Type=notify integration.
//!
//! No-op on non-Linux platforms or when `systemd-notify` feature disabled.
//! When run outside systemd (e.g., `cargo run`, manual launch), `sd_notify::notify`
//! returns Err which we log at debug — no user-visible impact.

#[cfg(all(target_os = "linux", feature = "systemd-notify"))]
pub fn notify_ready() {
    if let Err(e) = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]) {
        tracing::debug!(
            err.code = oneshim_core::error_codes::AutostartCode::SdNotifySkipped.as_str(),
            "sd_notify READY skipped (not run under systemd): {e}"
        );
    }
}

#[cfg(not(all(target_os = "linux", feature = "systemd-notify")))]
pub fn notify_ready() {
    // No-op on non-Linux or when systemd-notify feature disabled.
}

#[cfg(all(target_os = "linux", feature = "systemd-notify"))]
pub fn notify_stopping() {
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
}

#[cfg(not(all(target_os = "linux", feature = "systemd-notify")))]
pub fn notify_stopping() {}
```

Note: `AutostartCode::SdNotifySkipped` = NEW wire code variant (must be added to `crates/oneshim-core/src/error_codes/autostart.rs` per ADR-019 — see §6 Wire codes).

**Init hook placement** (per Phase 1 iter-1 I3 — RESOLVED):

Verified `src-tauri/src/setup.rs::init()` (line 12-52) ALREADY encompasses scheduler loops + DB migrations + window shown via `BootstrapRuntimeBuilder` (line 31) + `DesktopStartupCoordinator` (line 37). Returns `Ok(())` at line 51. main.rs only calls `setup::init` at line 240.

**Decision**: Call `lifecycle::sd_notify::notify_ready()` as the LAST line in `setup::init()` BEFORE the `Ok(())` return at line 51. This is the natural "init complete" point — all scheduler loops spawned, DB migrated, window shown.

NOT in main.rs (would require additional plumbing to know when setup completed).

---

### 5.2 Migration Strategy (hash-based, deferred reload)

**File**: `src-tauri/src/lifecycle/migration_hashes.rs` (NEW)

```rust
//! Known SHA-256 hashes of prior-version systemd service file templates.
//!
//! Used by autostart_migration to determine whether an existing
//! `~/.config/systemd/user/oneshim.service` file matches a known template
//! (safe to overwrite) vs has been customized by the user (skip).

/// (hash, label) pairs. Must include EVERY released template content prior
/// to PR-B2's Type=notify version.
pub const KNOWN_PRIOR_HASHES: &[(&str, &str)] = &[
    // PR-B1 era (Type=simple), from `linux::generate_service_file()` in commit 5618558c
    // SHA-256 of file content with `binary_path` placeholder replaced by `{BINARY_PATH}`
    // (see compute_known_hash() in migration tests for canonicalization rules)
    ("TBD-COMPUTE-FROM-PR-B1-TEMPLATE", "PR-B1 Type=simple"),
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
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn matches_known_template(content: &str, binary_path: &str) -> Option<&'static str> {
    let canonical = canonicalize(content, binary_path);
    let hash = compute_hash(&canonical);
    KNOWN_PRIOR_HASHES
        .iter()
        .find(|(known_hash, _)| *known_hash == hash)
        .map(|(_, label)| *label)
}
```

**No Cargo.toml change for sha2** — verified during Phase 1 iter-1: workspace `Cargo.toml` line 119 already pins `sha2 = "0.11"`, and `src-tauri/Cargo.toml` line 54 already has `sha2 = { workspace = true }`. Q-B2-6 RESOLVED: no action needed.

**File**: `src-tauri/src/lifecycle/autostart_migration.rs` (NEW)

```rust
//! One-time migration check at app startup.
//!
//! If an existing systemd service file matches a known PR-B1-era template,
//! overwrite with the new PR-B2 Type=notify template (DEFERRED reload — file
//! takes effect on next user login; we do NOT call `daemon-reload` on the
//! currently-running service).
//!
//! If file content is unrecognized (user customized): log warn + skip.

#[cfg(target_os = "linux")]
pub fn run_startup_migration() {
    use crate::autostart::linux::{service_path, generate_service_file};
    use super::migration_hashes::{matches_known_template, KNOWN_PRIOR_HASHES};

    let path = match service_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(
                err.code = oneshim_core::error_codes::AutostartCode::ServiceMigrationSkipped.as_str(),
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
                err.code = oneshim_core::error_codes::AutostartCode::ServiceMigrationSkipped.as_str(),
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
                    err.code = oneshim_core::error_codes::AutostartCode::ServiceMigrationFailed.as_str(),
                    "Migration write failed: {e}"
                );
                return;
            }
            tracing::info!(
                err.code = oneshim_core::error_codes::AutostartCode::ServiceMigrated.as_str(),
                from = %label,
                "Migrated systemd unit file from {label} to Type=notify; takes effect next login"
            );
        }
        None => {
            tracing::warn!(
                err.code = oneshim_core::error_codes::AutostartCode::ServiceMigrationSkipped.as_str(),
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

Wired in `setup.rs` after the existing D-Bus presence check (PR-B1 Task 6 placement):

```rust
#[cfg(target_os = "linux")]
{
    // ... existing D-Bus check from PR-B1 ...
    crate::lifecycle::autostart_migration::run_startup_migration();
}
```

---

### 5.3 Real `detect_capabilities()` for Linux

**File**: `src-tauri/src/autostart.rs` (Linux mod), replace skeleton:

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
    if has_systemctl() {
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

(macOS + Windows impls unchanged from PR-B1 skeleton.)

**`has_systemctl()` helper**: already exists in PR-B1 as private fn — verify visibility for use in `detect_capabilities()`. If private, change to `pub(super)` or move into the same scope.

---

### 5.4 i18n Refinements for Capability Tooltips

PR-B1 already added these keys (per Task 8 commit `288d307e`):
- `settings.autostart.unsupported_snap_sandbox`
- `settings.autostart.unsupported_flatpak_sandbox`
- `settings.autostart.unsupported_headless_session`
- `settings.autostart.unsupported_systemctl_unavailable`
- `settings.autostart.unsupported_unsupported_platform`
- `settings.autostart.unsupported_unknown`

PR-B2 verifies wording quality + adds any missing variants discovered during Linux smoke testing. No new keys needed unless `EnvironmentKind` enum gains new variants.

**Refinement scope** (post-PR-B1 review):
- Korean translations for any additional clarifying detail (e.g., "Use snap connect oneshim:autostart-files" workaround hint for Snap if applicable)
- English wording polish based on Linux smoke matrix UX feedback

---

### 5.5 Linux Integration Tests (rootless systemd)

**File**: `src-tauri/tests/linux_autostart_unit.rs` (NEW, `#[cfg(target_os = "linux")]`)

Tests that don't require real systemd:

| ID | Test | What it verifies |
|----|------|------------------|
| T1 | `service_file_has_type_notify` | `generate_service_file()` output contains `Type=notify` |
| T2 | `service_file_has_notify_access_main_and_timeout` | Output contains `NotifyAccess=main` and `TimeoutStartSec=30` |
| T3 | `detect_capabilities_returns_snap_sandbox` | With SNAP env var set, returns `LinuxSnapSandbox` + `supported=false` |
| T4 | `detect_capabilities_returns_flatpak_sandbox` | With FLATPAK_ID env var set |
| T5 | `detect_capabilities_returns_headless` | With DISPLAY + WAYLAND_DISPLAY both unset |
| T6 | `migration_hash_matches_known_template` | Synthesized PR-B1 template content matches `KNOWN_PRIOR_HASHES` |
| T7 | `migration_hash_skips_on_unknown` | Tampered content returns `None` from `matches_known_template` |

Use `serial_test` crate to serialize tests that mutate process env vars (T3-T5). Per memory `reference_serial_test_pattern`.

**File**: `src-tauri/tests/linux_autostart_systemd_live.rs` (NEW, `#[cfg(target_os = "linux")]`, manual `--ignored`)

Tests that require real systemd:

| ID | Test | What it verifies |
|----|------|------------------|
| T8 | `enable_autostart_writes_service_file_with_type_notify` | After `autostart::enable_autostart()`, file at `~/.config/systemd/user/oneshim.service` contains `Type=notify` + `systemctl --user is-enabled` returns `enabled` |
| T9 | `sd_notify_no_panic_when_socket_missing` | `lifecycle::sd_notify::notify_ready()` does NOT panic when `NOTIFY_SOCKET` env unset (returns Err logged at debug). Per Phase 1 iter-1 I5: ubuntu-latest CI runners lack real systemd PID 1, so we cannot verify actual notify-to-systemd succeeds in CI. T9 verifies the failure-mode safety only. Real systemd verification is in §9.4 manual smoke matrix. |
| T10 | `migration_end_to_end_defers_reload` | Write old Type=simple template, run `run_startup_migration()`, verify file updated + `systemctl --user is-active` still returns previous state (no daemon-reload triggered) |

All marked `#[ignore = "modifies user systemd state — run manually"]`.

**CI workflows** (per Phase 1 review I5 — no `--privileged`):

```yaml
# .github/workflows/ci.yml — add new job (always-on)
linux-autostart-unit:
  runs-on: ubuntu-latest
  needs: [check]  # parallel with other test jobs
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Touch tauri externalbin stub (per reference_ci_tauri_externalbin_stub.md)
      run: touch src-tauri/oneshim-sandbox-worker-x86_64-unknown-linux-gnu
    - name: Touch frontend dist stub
      run: mkdir -p crates/oneshim-web/frontend/dist && touch crates/oneshim-web/frontend/dist/index.html
    - run: sudo apt-get update && sudo apt-get install -y libsystemd-dev
    - run: cargo test -p oneshim-app --features systemd-notify --test linux_autostart_unit
```

(per Phase 1 iter-1 I1 — `sd-notify` crate links against native `libsystemd` on Linux)

```yaml
# .github/workflows/linux-systemd-integration.yml — NEW manual workflow
name: Linux systemd integration
on:
  workflow_dispatch:
    inputs:
      branch:
        description: 'Branch to test'
        default: 'main'
jobs:
  linux-systemd-integration:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { ref: ${{ inputs.branch }} }
      - run: |
          sudo apt-get update
          sudo apt-get install -y systemd dbus-user-session libsystemd-dev
          systemctl --user start dbus
      - uses: dtolnay/rust-toolchain@stable
      - run: touch src-tauri/oneshim-sandbox-worker-x86_64-unknown-linux-gnu
      - run: mkdir -p crates/oneshim-web/frontend/dist && touch crates/oneshim-web/frontend/dist/index.html
      - run: cargo test -p oneshim-app --features systemd-notify --test linux_autostart_systemd_live -- --ignored
```

---

### 5.6 Korean Operations Documentation

**File**: `docs/guides/autostart.ko.md` (NEW)

Outline:

```markdown
# 자동 시작 (Autostart) 운영 가이드

## 개요
- 기능: 사용자 로그인 시 ONESHIM 자동 실행
- 정책: opt-in (사용자 명시적 동의 필요)
- 활성화: Settings → Startup 토글

## 플랫폼별 동작

### macOS
- 위치: `~/Library/LaunchAgents/com.oneshim.agent.plist`
- 메커니즘: launchctl load/unload
- 단일 인스턴스: Unix domain socket via tauri-plugin-single-instance

### Windows
- 위치: HKCU `Software\Microsoft\Windows\CurrentVersion\Run` Registry
- 메커니즘: Windows API
- 단일 인스턴스: 명명된 파이프 via tauri-plugin-single-instance

### Linux
- 1차: systemd user service `~/.config/systemd/user/oneshim.service` (Type=notify)
- 2차 fallback: XDG `~/.config/autostart/oneshim.desktop` (systemctl 미설치 환경)
- 단일 인스턴스: D-Bus name `com.oneshim.client.SingleInstance`

## Linux 환경별 지원

| 환경 | 지원 | 비고 |
|------|------|------|
| systemd (대부분의 데스크톱 배포판) | ✅ | Type=notify로 정확한 readiness 신호 |
| systemd 부재 (XDG fallback) | ✅ | .desktop 파일 사용; readiness 신호 없음 |
| Snap 패키지 | ❌ | Snap의 내장 자동 시작 사용 권장 |
| Flatpak 패키지 | ❌ | Flatpak portal autostart API 사용 권장 |
| Headless (SSH, no display) | ❌ | 데스크톱 세션 필요 |

## 마이그레이션 (PR-B1 → PR-B2 업그레이드)

PR-B1은 systemd `Type=simple` 서비스 파일을 사용했습니다. PR-B2는 더 정확한 readiness 신호를 위해 `Type=notify`로 변경합니다.

### 자동 마이그레이션
- ONESHIM v0.4.41+ 첫 실행 시 자동으로 service 파일 검사
- PR-B1-era 템플릿과 hash 매치되면 자동 overwrite (단, daemon-reload는 다음 로그인까지 지연 — 현재 실행 중인 서비스 중단 없음)
- 수동으로 customize한 service 파일 (e.g., `Environment=` 추가)은 자동 마이그레이션에서 제외 — log warn + 다음 단계 안내

### 수동 마이그레이션 (customize한 사용자)
```bash
# 1. 기존 customization 백업
cp ~/.config/systemd/user/oneshim.service ~/.config/systemd/user/oneshim.service.backup

# 2. ONESHIM Settings → Startup 토글 OFF → ON 재설정 (새 템플릿 생성)
# 또는 수동으로 다음 사항 적용:
#   Type=simple → Type=notify
#   추가: NotifyAccess=main
#   추가: TimeoutStartSec=30

# 3. systemd 리로드
systemctl --user daemon-reload
systemctl --user restart oneshim.service
```

## 트러블슈팅

### "Settings → Startup 토글이 회색이에요"
환경별 capability 매트릭스를 확인하세요. Snap/Flatpak/headless 환경에서는 토글이 비활성화됩니다.
- Tooltip에 표시된 사유 확인
- Snap 사용자: `snap services` 또는 Snap Center에서 "Run on system startup" 옵션 확인
- Flatpak 사용자: `~/.var/app/com.oneshim.client/...` portal API 설정 확인
- Headless 사용자: SSH 세션은 자동 시작 대상 아님 — 데스크톱 세션 필요

### "활성화했는데 시작이 안 돼요"
1. systemd 상태 확인:
   ```bash
   systemctl --user status oneshim.service
   journalctl --user -u oneshim.service -n 50
   ```
2. `Type=notify` timeout 발생 시 (`TimeoutStartSec=30` 초과):
   - 초기화가 30초 이상 걸리는 환경 (HDD, 큰 DB) — `TimeoutStartSec`을 60-90으로 증가
   - 파일 수정: `~/.config/systemd/user/oneshim.service`
3. D-Bus 부재 (headless SSH 세션에서 실행) → single-instance 강제 약화 — duplicate process 가능성
4. 로그 위치:
   - macOS: `~/Library/Logs/ONESHIM/`
   - Windows: `%LOCALAPPDATA%\ONESHIM\logs\`
   - Linux: `~/.local/share/oneshim/logs/` 또는 `journalctl --user -u oneshim`

### "service 파일이 customize되어 마이그레이션 스킵됨"
앞 섹션의 "수동 마이그레이션" 절차 따름.

## 단일 인스턴스 동작
- 첫 실행: 정상 시작
- 두번째 실행: 첫 실행 윈도우로 포커스 이동 후 즉시 종료 (D-Bus / NamedPipe / UnixSocket 통해 신호)
- 알려진 한계 (Wayland kept-hidden window): 첫 실행이 tray-only로 시작 (메인 윈도우 한 번도 표시 안 됨) → 두번째 실행이 dock 아이콘 클릭으로 발생 시 윈도우가 표시 안 될 수 있음. PR-B1 risk register §13에서 known limitation으로 수용. 실제 발생 시 follow-up PR로 `window.create()` fallback 추가.

## 참고
- PR-B1 spec: `docs/superpowers/specs/2026-04-25-phase9-pr-b-autostart-ipc-foundation-design.md`
- PR-B2 spec: `docs/superpowers/specs/2026-04-25-phase9-pr-b2-autostart-linux-deep-design.md`
- ADR-019: 와이어 코드 인프라
- 단일 인스턴스 플러그인: tauri-plugin-single-instance v2
```

---

## 6. Wire Codes (per ADR-019)

PR-B2 adds 4 NEW wire code variants to `AutostartCode` enum (currently has 5 from PR-B1):

```rust
// In crates/oneshim-core/src/error_codes/autostart.rs
define_code_enum! {
    pub enum AutostartCode {
        // PR-B1 (existing):
        EnableFailed => "autostart.enable_failed",
        DisableFailed => "autostart.disable_failed",
        QueryFailed => "autostart.query_failed",
        CounterIncrementFailed => "autostart.counter_increment_failed",
        EventEmitFailed => "autostart.event_emit_failed",

        // PR-B2 (NEW):
        SdNotifySkipped => "autostart.sd_notify_skipped",
        ServiceMigrated => "autostart.service_migrated",
        ServiceMigrationSkipped => "autostart.service_migration_skipped",
        ServiceMigrationFailed => "autostart.service_migration_failed",
    }
}
```

Each new variant:
- Added to `wire_contract_snapshot.expected.txt` in alphabetical order
- Added to `wire-errors.{en,ko}.json` with translations
- CI gate `scripts/check-wire-error-i18n-coverage.sh` validates parity

**Wire code count assumption** (per Phase 1 iter-1 C2):
- Implementation gate per §10.2 requires PR-B1 (#508) merge first
- After PR-B1 merge to main: wire snapshot has 47 codes (42 baseline + 5 PR-B1)
- After PR-B2 merge: 47 + 4 = **51** total

PR-B2 commit 2 ("AutostartCode wire code variants for PR-B2") ONLY adds the 4 new variants (`SdNotifySkipped`, `ServiceMigrated`, `ServiceMigrationSkipped`, `ServiceMigrationFailed`) — the 5 PR-B1 variants are inherited from rebased main.

---

## 7. Data Flow

### 7.1 Fresh PR-B2 install: enable autostart

```
User toggles Settings → Startup ON
  → invoke('enable_autostart')
    → autostart::enable_autostart()
      → linux::enable_autostart()
        → has_systemctl() = true → write Type=notify service file (no Type=simple legacy)
        → systemctl --user enable oneshim.service
  → User logs out + back in
    → systemd starts oneshim.service
    → ExecStart={binary} runs with NOTIFY_SOCKET env set
    → Tauri builder.setup() runs
    → All scheduler loops spawned, SQLite migrations applied, main window shown
    → setup::init() (last line before Ok()) calls lifecycle::sd_notify::notify_ready() (per §5.1 I3)
      → sd_notify::notify(false, &[NotifyState::Ready])
      → systemd marks unit "active (running)"
```

### 7.2 PR-B1 user upgrades to PR-B2: first launch

```
User installs PR-B2 binary; existing ~/.config/systemd/user/oneshim.service is Type=simple
  → User starts ONESHIM (e.g., via dock or autostart-fired existing instance)
  → main.rs setup() runs
  → lifecycle::autostart_migration::run_startup_migration() executes
    → Reads existing service file
    → Computes canonicalized hash
    → Matches PR-B1 KNOWN_PRIOR_HASHES → "PR-B1 Type=simple"
    → Writes new Type=notify content to file
    → Logs info: "Migrated systemd unit file from PR-B1 Type=simple to Type=notify; takes effect next login"
    → DOES NOT call daemon-reload (per C4 — currently-running service stays Type=simple)
  → User continues working normally
  → Next user logout + login:
    → systemd reads updated unit file (Type=notify)
    → ExecStart runs new binary with NOTIFY_SOCKET set
    → notify_ready() fires correctly → systemd marks active
```

### 7.3 Customized PR-B1 user upgrades to PR-B2

```
User has manually edited ~/.config/systemd/user/oneshim.service (e.g., added Environment=FOO=bar)
  → run_startup_migration() reads file
  → matches_known_template returns None (hash doesn't match)
  → Logs warn: "Skipping autostart unit migration — file appears customized. Manual update required (see docs/guides/autostart.ko.md)"
  → User responsibility: manual migration per autostart.ko.md
  → Until manual migration: service continues running as Type=simple (PR-B1 behavior, no breakage)
```

### 7.4 Snap user attempts to enable autostart

```
ONESHIM running inside Snap container (SNAP env var set)
  → Settings → Startup section loads
  → invoke('autostart_capabilities')
    → detect_capabilities() returns { supported: false, unsupported_reason: SnapSandbox, environment: LinuxSnapSandbox }
  → UI: toggle disabled, tooltip: "Use Snap's built-in autostart settings"
  → User cannot click toggle ON → no broken systemctl invocation
```

---

## 8. Error Handling

### 8.1 Failure modes & mitigations

| Failure mode | Detection | User-facing behavior | Code path |
|--------------|-----------|----------------------|-----------|
| sd_notify NOTIFY_SOCKET missing (run outside systemd) | `sd_notify::notify` returns Err | Logged at debug, app continues. Only matters when run under systemd; harmless otherwise. | `lifecycle::sd_notify` |
| systemd timeout (>30s init) | systemd kills process after TimeoutStartSec | "Service failed to start" in journalctl. Mitigation: ensure init <5s typical. Documented in autostart.ko.md as user remediation: increase TimeoutStartSec | systemd policy |
| Migration: read service file fails (permissions) | `std::fs::read_to_string` Err | Logged at warn, migration skipped, no user impact. Service continues with old file. | `autostart_migration` |
| Migration: write service file fails (disk full) | `std::fs::write` Err | Logged at warn, file unchanged, no user impact | `autostart_migration` |
| Migration: hash matches but `current_exe()` fails | None possible — early return | Migration skipped silently | `autostart_migration` |
| Migration: file doesn't match any known hash (user customized) | `matches_known_template` returns None | Logged at warn, skipped, autostart.ko.md provides manual migration steps | `autostart_migration` |
| User on Snap attempts toggle | `detect_capabilities` returns `SnapSandbox` | Toggle disabled, tooltip explains | UI gating (PR-B1 already handles, PR-B2 fills capabilities accurately) |
| User on Flatpak attempts toggle | `FlatpakSandbox` | Same | Same |
| User on headless Linux attempts toggle | `HeadlessSession` | Same | Same |
| systemctl missing on Linux | `has_systemctl` false → `LinuxXdg` | Toggle still enabled (XDG fallback works) | UI uses capability info |

### 8.2 Logging conventions (4 NEW wire codes per §6)

```rust
// debug: sd_notify skipped (informational, may indicate misconfig)
debug!(err.code = AutostartCode::SdNotifySkipped.as_str(), "...");
// info (success): use event.code NOT err.code (per Phase 1 iter-1 I6 — avoid polluting Loki err-rate dashboards)
info!(event.code = AutostartCode::ServiceMigrated.as_str(), from = %label, "...");
// warn (failure): err.code is correct
warn!(err.code = AutostartCode::ServiceMigrationSkipped.as_str(), path = %p.display(), "...");
warn!(err.code = AutostartCode::ServiceMigrationFailed.as_str(), "...");
```

Per Phase 1 iter-1 I6: ADR-019 `err.code` field is for ERROR codes. Using on success-path info logs would pollute Loki/Grafana error-rate metrics. Use `event.code` for success-path info logs instead.

Loki/Grafana queries can group by `err.code` for migration health observability.

---

## 9. Testing Strategy

### 9.1 Unit tests
- `crates/oneshim-core/src/error_codes/autostart.rs` — extend existing tests to cover 4 new variants (3 boilerplate tests via macro auto-coverage)
- `src-tauri/src/lifecycle/migration_hashes.rs` — pure function tests for `compute_hash`, `canonicalize`, `matches_known_template`
- `src-tauri/src/lifecycle/sd_notify.rs` — no-op stub coverage on non-Linux

### 9.2 Integration tests
- `src-tauri/tests/linux_autostart_unit.rs` — T1-T7 (no real systemd needed)
- `src-tauri/tests/linux_autostart_systemd_live.rs` — T8-T10 manual `--ignored`

### 9.3 Frontend Vitest
- No new tests required (PR-B1 GeneralTab tests already cover capability-aware UI gating; PR-B2 just refines tooltip wording in i18n which doesn't change behavior)

### 9.4 Manual smoke matrix
- Linux Ubuntu 24.04 + systemd: enable → verify Type=notify file → logout/login → verify systemctl is-active = active
- Linux Snap (if test packaging available): toggle disabled with tooltip
- Linux Flatpak: same
- Linux SSH (headless): toggle disabled with tooltip
- Linux Wayland (Fedora 40 GNOME): same as systemd Ubuntu + verify Wayland kept-hidden case from PR-B1 doesn't regress
- Linux X11 (Ubuntu 22.04 GNOME): same

### 9.5 Migration manual smoke
- Install PR-B1 → enable autostart → verify Type=simple file
- Install PR-B2 binary (no reinstall) → run app → check log for `autostart.service_migrated` event
- Verify file content updated to Type=notify
- Verify `systemctl --user is-active` still returns running state (no daemon-reload mid-session)
- Logout/login → verify service starts as Type=notify cleanly

### 9.6 Pass criteria
- All unit tests GREEN
- All integration tests GREEN (T1-T7 in CI, T8-T10 manual)
- `cargo check/test/clippy/fmt --workspace` GREEN
- Wire snapshot test GREEN (51 codes)
- i18n CI GREEN (51 codes per locale)

---

## 10. Delivery Plan

### 10.1 PR-B2 commit structure (~15h, ~12 commits)

| # | Commit | Estimate |
|---|--------|----------|
| 1 | `chore(autostart): add sd-notify dep with systemd-notify feature flag` (NO sha2 — already in workspace per C1) | 0.5h |
| 2 | `feat(autostart): AutostartCode wire code variants for PR-B2 (sd_notify_skipped + 3 service_migration_*)` | 0.5h |
| 3 | `test(autostart): wire-error-i18n CI gate update for new variants (en+ko)` | 0.5h |
| 4 | `feat(autostart): lifecycle::sd_notify wrapper module` | 1h |
| 5 | `test(autostart): sd_notify unit + non-Linux no-op coverage` | 0.5h |
| 6 | `feat(autostart): change Linux service file template to Type=notify + NotifyAccess=main + TimeoutStartSec=30 + update existing inline test (per C3)` | 1h |
| 7 | `feat(autostart): hash-based migration (migration_hashes.rs + autostart_migration.rs)` | 2h |
| 8 | `test(autostart): migration hash check + defer behavior unit tests` | 1h |
| 9 | `feat(autostart): wire sd_notify::notify_ready/stopping in setup.rs::init() last line (per I3) + run_startup_migration in setup` | 1h |
| 10 | `feat(autostart): real detect_capabilities replacing PR-B1 skeleton (Snap/Flatpak/headless detection)` | 1.5h |
| 11 | `test(autostart): linux_autostart_unit.rs T1-T7 + CI workflow integration` | 2h |
| 12 | `feat(autostart): manual-trigger linux_autostart_systemd_live.rs workflow + T8-T10 tests` | 1.5h |
| 13 | `docs(autostart): docs/guides/autostart.ko.md operations + migration guide` | 1.5h |
| 14 | `docs(autostart): STATUS.md + PHASE-HISTORY.md entry for PR-B2` | 0.5h |
| 15 | `chore(autostart): manual smoke matrix per Linux env (PR description checklist)` | 1h (no commit) |

**Total**: ~14.5h. Bundle test commits per `feedback_lefthook_clippy_cost`.

### 10.2 Dependencies & blockers

- **Hard blocker**: PR-B1 (#508) MUST merge first. All commits 1-12 reference PR-B1 types/files.
- **Soft blocker**: features2 (#491) already merged (per main HEAD `0827e071`). No conflict expected.
- **Cross-PR**: PR #506 (`serve_external_inner`) + PR #507 (`live_reload_harness`) are external-grpc only — disjoint from PR-B2.

### 10.3 Branch + release

- Branch: `feature/phase9-autostart-linux-deep` (this worktree)
- Release: `0.4.41-rc.1` after PR-B2 merge
- Stable promote via `promote-stable.sh` after RC validation

---

## 11. Migration & Backward Compatibility

### 11.1 Existing user upgrade paths

| Pre-PR-B2 state | Post-PR-B2 first launch behavior |
|-----------------|----------------------------------|
| autostart never enabled, no service file | No-op. Future enable writes Type=notify directly. |
| PR-B1 era Type=simple service file (unmodified) | `run_startup_migration()` overwrites with Type=notify (deferred reload). Next login starts cleanly. |
| Customized service file (Environment= added etc.) | Skip + log warn + autostart.ko.md guidance. Service continues running as before until manual migration. |

### 11.2 Downgrade safety

- PR-B2 → downgrade to PR-B1: service file has `Type=notify` but PR-B1 binary doesn't call `sd_notify_ready()`. systemd marks unit failed after `TimeoutStartSec=30`. **Recovery**:
  - Option A: PR-B2 release notes recommend disable+re-enable on downgrade (PR-B1 binary writes Type=simple)
  - Option B: Manual edit `~/.config/systemd/user/oneshim.service` removing `Type=notify` + `NotifyAccess=main` + `TimeoutStartSec` lines
  - Document in PR-B2 release notes + autostart.ko.md

### 11.3 sd-notify feature flag handling

Per Phase 1 iter-1 I2 (option A — non-default feature):

- **`systemd-notify` is non-default** in Cargo.toml. Cargo does NOT support per-platform default features without a `build.rs` workaround.
- **Linux release builds**: must explicitly pass `--features systemd-notify` (CI workflow §5.5 + release scripts handle this; users compiling from source on Linux must enable manually)
- **macOS/Windows builds**: do NOT pass `--features systemd-notify`. The cfg-gated code in `lifecycle::sd_notify` becomes no-op stubs. The `sd-notify` crate is NOT compiled.
- **Custom Linux build without `systemd-notify`**: `notify_ready()` no-op, service file still Type=notify but never gets READY signal → systemd `TimeoutStartSec=30` fail → restart loop. **Document in release-build CI as a verification gate**: every Linux release build MUST include `--features systemd-notify`. If absent, fail build.

---

## 12. Open Questions for Phase 1 Deep Review

| # | Question | Resolution path |
|---|----------|-----------------|
| Q-B2-1 | What's the exact SHA-256 of PR-B1 era service file template (canonicalized)? | Compute during impl Task 1; capture in `KNOWN_PRIOR_HASHES`. Verify by reading PR-B1 commit `c3e8685a`'s `linux::generate_service_file()` output. |
| Q-B2-2 | Should PR-B2 add a "fix migration" CLI command for users with customized files? | Punt to follow-up PR (NG3 from PR-B1 — no CLI commands). Manual migration via autostart.ko.md sufficient. |
| Q-B2-3 | TimeoutStartSec=30 — is 30s enough for slow init? | Bench on dev machine (typical <5s). If slow init env (HDD, large DB), document increasing TimeoutStartSec in autostart.ko.md. Default 30s is conservative. |
| Q-B2-4 | Linux integration test CI — single GitHub Actions runner OK or need self-hosted? | `ubuntu-latest` runner is sufficient for unit tests. For systemd-live tests (T8-T10), use `systemd-run --user --scope` per spec — no self-hosted needed. |
| Q-B2-5 | Korean docs — should `autostart.ko.md` also have an English `autostart.md` companion? | Per project memory `feedback_holistic_pre_merge_review` + memory `feedback_route_refactor_e2e_completeness`: bilingual parity preferred. **Recommendation**: ship Korean-only in PR-B2, add English in follow-up PR. Document gap in spec §13. |
| Q-B2-6 | Does `sha2` crate already exist in workspace deps? | ✅ RESOLVED (Phase 1 iter-1 C1): `sha2 = "0.11"` in workspace `Cargo.toml:119`; `src-tauri/Cargo.toml:54` has `sha2 = { workspace = true }`. No Cargo.toml change needed.
| Q-B2-10 (NEW) | Migration: symlink path mismatch — `current_exe()` resolves canonical path on Linux but user may have written service with symlink path | Treated as "customized" → skipped + warn log. Acceptable per spec §13 risk register. Documented in `docs/guides/autostart.ko.md` troubleshooting. |
| Q-B2-7 (NEW) | Where does `setup.rs` live and how is it wired in `main.rs`? | Verified during PR-B1 Task 6 — `src-tauri/src/setup.rs::init()` called from main.rs builder.setup() chain. PR-B2 extends with `autostart_migration::run_startup_migration()` call. |
| Q-B2-8 (NEW) | After service file migration, does the user need to logout AND login, OR only login? | Logout AND login. systemd user instance reads unit files at session start. Document in autostart.ko.md. |
| Q-B2-9 (NEW) | What if user's existing service file ends with extra trailing whitespace / different line endings? | Hash mismatch — treated as customized. Mitigation: include canonical line-ending normalization in `canonicalize()` (replace `\r\n` → `\n`, trim trailing whitespace per line). Add to spec §5.2 if confirmed needed. |

---

## 13. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Hash matching too strict (line-ending differences trip benign cases) | Medium | Medium | Add line-ending normalization in `canonicalize()` per Q-B2-9 |
| Migration logs become noisy in healthy environments (per-launch warn) | Low | Low | Migration runs once per app start; healthy state = no log emission (file matches new template, no migration needed → silent return) |
| sd-notify silently fails in user-installed builds without `systemd-notify` feature | Medium | Medium | Document explicitly: do NOT disable systemd-notify on Linux. Release-build CI verifies feature is enabled by default. |
| Snap detection false-positive (SNAP env var leaked from non-Snap shell) | Very Low | Low | SNAP env is automatically unset by /etc/profile in non-Snap shells per Snap design. Acceptable risk. |
| Wayland kept-hidden window regression introduced by Linux refactoring | Low | Medium | PR-B1 risk register §13 already accepted as known limitation. PR-B2 manual smoke matrix verifies no NEW regression. |
| User customization heuristic too narrow (some line orderings trip even non-customized files) | Low | Medium | Generate hash from `linux::generate_service_file()` directly during PR-B2 impl — guarantees the canonical form matches what we write. Add regression test in T6. |
| `lifecycle::autostart_migration` runs before AppConfig loads → potential race | Low | Low | Migration is filesystem-only; no AppConfig dependency. Order doesn't matter. |
| Korean docs become stale vs implementation drift | Medium | Low | Tier 1 doc per CLAUDE.md DOCUMENTATION-STRATEGY: update alongside ADR/spec changes. autostart.ko.md links spec for source of truth. |

---

## 14. Cross-Consumer Dependencies

| Branch | Files in conflict with PR-B2 | Conflict severity | Mitigation |
|--------|------------------------------|-------------------|------------|
| `feature/phase9-autostart-foundation` (PR #508) | EVERYTHING — PR-B2 builds on PR-B1 | **Hard dependency** | PR-B2 implementation BLOCKED until PR-B1 merges. Current state: PR-B1 in review. |
| `refactor/serve-external-inner-extraction` (PR #506) | None (oneshim-web/grpc only) | None | Disjoint scope |
| `refactor/live-reload-harness-extraction` (PR #507) | None (oneshim-web/tests only) | None | Disjoint scope |
| Future Phase 9 PR-C (Timeline bulk tag) | None expected | None | Different domain |

### 14.1 Recommended merge order
1. PR-B1 (#508) merges → release `0.4.40-rc.1`
2. PR-B2 (this branch) opens → merges → release `0.4.41-rc.1`

---

## 15. Spec Self-Review (v1)

### 15.1 Placeholder scan
- ✅ No "TBD" beyond Q-B2-1 (KNOWN_PRIOR_HASHES SHA value — to be computed during impl Task 7)
- ⚠ Q-B2-1 through Q-B2-9 are open questions for Phase 1 deep review iter
- ⚠ §13 risk "Migration logs become noisy" — acknowledge but no concrete mitigation written; review during iter-2

### 15.2 Internal consistency
- ✅ Wire codes table (§6) consistent with logging examples (§8.2)
- ✅ Migration policy (§5.2) consistent with data flow §7.2 + §7.3 + §11.1
- ✅ Test plan (§9) covers all NEW components from §5

### 15.3 Scope check
- ✅ Bounded: Linux-only, no macOS/Windows behavior change
- ✅ Implementation gate explicit: PR-B1 must merge first

### 15.4 Ambiguity check
- ⚠ "Customized" definition is hash-based — what if user's customization is byte-identical to template? (Edge case; theoretically impossible due to {BINARY_PATH} replacement)
- ⚠ "Fast init <5s typical" — bench data needed. Add to Q-B2-3.

---

## 16. Implementation Status

- **Spec v1**: 2026-04-25 (this document — extracted from PR-B1 spec §6)
- **Phase 1 deep review iter-1**: PENDING (next ralph-loop iteration)
- **Phase 1 deep review iter-N**: until zero Critical+Important
- **Phase 2 plan creation**: PENDING (after Phase 1 closes)
- **Phase 3 implementation**: BLOCKED on PR-B1 (#508) merge
- **Worktree**: `.claude/worktrees/phase9-autostart-linux-deep` on `feature/phase9-autostart-linux-deep`
- **Base**: `0827e071` (origin/main)

---

**End of spec v1.**
