//! Post-install self-healthy probe with 2-failed-boot automatic rollback.
//!
//! See `docs/reviews/2026-04-18-phase4-updater-hardening-design.md` §4 for
//! the full design.
//!
//! State-machine summary:
//! - On every startup, `check_startup_state` inspects `.install_pending_{VERSION}`,
//!   `.boot_count_{VERSION}`, `.self_healthy_{VERSION}` in the install directory.
//! - If boot_count reaches `failed_boot_threshold` (default 2) without a
//!   self-healthy marker, returns `RollbackRequired` with the backup path
//!   recorded at install.
//! - Otherwise returns `Normal`; the scheduler later calls `spawn_healthy_writer`
//!   which writes the self-healthy marker after `healthy_threshold` (default 30s)
//!   of continuous wall-clock uptime.
//! - Staleness rule (§4.3): an `.install_pending_{VERSION}` that is > 24h old
//!   with no healthy marker is treated as abandoned (same-version manual
//!   reinstall or long-idle device). Probe deletes state and returns Normal
//!   without triggering rollback.
//!
//! Ownership (spec Amendment 1 — applied in Task 1): both `check_startup_state`
//! and `spawn_healthy_writer` take `&self` so a single probe instance can be
//! created in `app_runtime_launch.rs`, used for the startup check, then shared
//! via `Arc` into the scheduler for the healthy-writer spawn.
//!
//! # Known limitations (Loop 3 iter 1 review)
//!
//! - **Boot-counter ordering** (I-4): `failed_boot_threshold = 2` triggers
//!   rollback on the THIRD boot that fails to reach a self-healthy marker
//!   (boot 1 increments 0→1, boot 2 increments 1→2, boot 3 reads 2 ≥ 2
//!   and rolls back). The "2" in the name refers to the maximum retry
//!   count, not the total boot count. This matches the standard
//!   read-then-increment-after-threshold-check pattern.
//!
//! - **Concurrent-process race** (I-3): the "read count; compare threshold;
//!   write count+1" sequence is NOT atomic against two processes of the
//!   same version starting simultaneously (e.g., desktop shortcut + Tauri
//!   autolaunch firing in the same second on first-install). Both reads
//!   see the old value; both writes race. Mitigation deferred to a
//!   follow-up — low-probability path and the worst-case outcome is an
//!   unnecessary rollback after a single real success, which the user
//!   can work around by upgrading again. Fix candidate:
//!   `OpenOptions::new().create_new(true)` for `.boot_count_pid_{PID}`
//!   sub-files, then sum at read-time.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Staleness cutoff for an `.install_pending_{VERSION}` marker (§4.3).
/// Entries older than this age without a self-healthy marker are treated as
/// abandoned — probe deletes them without triggering rollback.
const STALENESS_CUTOFF: Duration = Duration::from_secs(24 * 60 * 60);

/// Persistent content of `.install_pending_{VERSION}`.
///
/// Written by `install.rs::write_install_pending` (Task 6) after a successful
/// `replace_binary` and before `restart_app`. Consumed by the probe on the
/// next startup to determine rollback eligibility and backup selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct InstallPending {
    /// ISO-8601 UTC timestamp at which the install completed.
    pub installed_at: String,
    /// The semver string of the version that was installed BEFORE this one
    /// — the rollback target.
    pub previous_version: String,
    /// Absolute filesystem path to the backup binary created by
    /// `install.rs::backup_path_for` before the binary swap. On rollback, the
    /// probe reads this field and the caller swaps it back into place.
    pub backup_path: PathBuf,
}

/// Outcome of a startup probe check.
#[derive(Debug, Clone, PartialEq)]
pub enum StartupAction {
    /// Proceed with normal startup.
    Normal,
    /// Boot counter reached the failed-boot threshold without a self-healthy
    /// marker; caller should invoke `execute_rollback` with the enclosed
    /// metadata.
    RollbackRequired {
        from_version: String,
        to_version: String,
        backup_path: PathBuf,
        reason: RollbackReason,
    },
}

/// Why the probe escalated to `RollbackRequired`.
///
/// Additive enum — new reasons can be added without breaking existing
/// consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackReason {
    /// The current version failed to reach the self-healthy threshold on
    /// `failed_boot_threshold` consecutive startups (default 2).
    RepeatedStartupFailure,
}

/// Errors raised by the internal probe implementation. The public
/// `check_startup_state` catches all of these and returns `Normal` after
/// logging a warning — probe I/O failures must never block user startup.
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("install_pending file malformed: {0}")]
    InstallPendingParse(String),

    #[error("filesystem error in health probe: {0}")]
    Io(#[from] std::io::Error),
}

/// Post-install health probe. Constructed once per process at
/// `app_runtime_launch.rs`, shared via `Arc` into the scheduler for the
/// healthy-writer spawn.
#[derive(Debug, Clone)]
pub struct HealthProbe {
    install_dir: PathBuf,
    current_version: String,
    healthy_threshold: Duration,
    failed_boot_threshold: u8,
}

impl HealthProbe {
    /// Default thresholds: 30s healthy + 2 failed boots before rollback.
    pub fn new(install_dir: PathBuf, current_version: String) -> Self {
        Self {
            install_dir,
            current_version,
            healthy_threshold: Duration::from_secs(30),
            failed_boot_threshold: 2,
        }
    }

    /// Builder: override the healthy-threshold. Primarily for tests
    /// (inject a short duration so `spawn_healthy_writer` fires quickly).
    pub fn with_threshold(mut self, threshold: Duration) -> Self {
        self.healthy_threshold = threshold;
        self
    }

    fn install_pending_path(&self) -> PathBuf {
        self.install_dir
            .join(format!(".install_pending_{}", self.current_version))
    }

    fn boot_count_path(&self) -> PathBuf {
        self.install_dir
            .join(format!(".boot_count_{}", self.current_version))
    }

    fn self_healthy_path(&self) -> PathBuf {
        self.install_dir
            .join(format!(".self_healthy_{}", self.current_version))
    }

    /// Inspect the probe state files and return the next action for startup.
    ///
    /// Contract: any filesystem error is treated as `Normal` with a warning
    /// log. Probe I/O failures must never block user startup.
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
        let self_healthy = self.self_healthy_path();
        let install_pending = self.install_pending_path();
        let boot_count_path = self.boot_count_path();

        // Step 1 (short-circuit): self-healthy already written → nothing to do.
        if self_healthy.exists() {
            return Ok(StartupAction::Normal);
        }

        // Step 2 (short-circuit): no pending-install marker → fresh install
        // (or healthy marker was written and cleanup ran). Caller proceeds
        // normally; the post-boot spawn_healthy_writer will write the marker
        // after the healthy threshold.
        if !install_pending.exists() {
            return Ok(StartupAction::Normal);
        }

        // Step 0 (staleness): if the pending marker is > 24h old and we still
        // have no healthy marker, treat as abandoned (same-version manual
        // reinstall or a device that was powered off for days between boots).
        let pending = read_install_pending(&install_pending)?;
        if is_stale(&pending.installed_at, STALENESS_CUTOFF) {
            tracing::info!(
                "health probe: stale install_pending ({}h+ old) — cleaning abandoned state",
                STALENESS_CUTOFF.as_secs() / 3600
            );
            let _ = std::fs::remove_file(&install_pending);
            let _ = std::fs::remove_file(&boot_count_path);
            return Ok(StartupAction::Normal);
        }

        // Steps 3-5: read boot count, check threshold, increment atomically.
        let current_count = read_boot_count(&boot_count_path).unwrap_or(0);

        if current_count >= u32::from(self.failed_boot_threshold) {
            tracing::warn!(
                "health probe: boot_count={current_count} >= threshold={}; triggering rollback",
                self.failed_boot_threshold
            );
            return Ok(StartupAction::RollbackRequired {
                from_version: self.current_version.clone(),
                to_version: pending.previous_version.clone(),
                backup_path: pending.backup_path.clone(),
                reason: RollbackReason::RepeatedStartupFailure,
            });
        }

        // Increment the counter AFTER the threshold check so a single bad
        // boot is represented as count=1 next time, not count=2. Use a
        // temp-file + rename for atomicity against abrupt termination.
        write_boot_count_atomic(&boot_count_path, current_count + 1)?;
        Ok(StartupAction::Normal)
    }

    /// Spawn a tokio background task that waits `healthy_threshold` then
    /// writes the self-healthy marker and cleans related state files.
    ///
    /// Takes `&self` (spec Amendment 1) — the probe instance stays owned by
    /// the launch path; the spawned task captures the data it needs by
    /// value at spawn time.
    pub fn spawn_healthy_writer(&self) -> tokio::task::JoinHandle<()> {
        let install_dir = self.install_dir.clone();
        let version = self.current_version.clone();
        let threshold = self.healthy_threshold;

        tokio::spawn(async move {
            tokio::time::sleep(threshold).await;
            if let Err(err) = write_self_healthy_and_cleanup(&install_dir, &version) {
                tracing::warn!("spawn_healthy_writer: cleanup error — {err}");
            }
        })
    }
}

// ── Internal helpers (file-level for testability) ─────────────────────

fn read_install_pending(path: &Path) -> Result<InstallPending, ProbeError> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice::<InstallPending>(&bytes)
        .map_err(|e| ProbeError::InstallPendingParse(e.to_string()))
}

fn read_boot_count(path: &Path) -> Option<u32> {
    let bytes = std::fs::read(path).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    text.trim().parse::<u32>().ok()
}

/// Atomic write via tempfile + rename (same directory as target).
fn write_boot_count_atomic(path: &Path, value: u32) -> Result<(), ProbeError> {
    let parent = path.parent().ok_or_else(|| {
        ProbeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "boot_count path has no parent",
        ))
    })?;
    let tmp_path = parent.join(format!(".boot_count.tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, value.to_string().as_bytes())?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Returns true when `iso_ts_utc` parses successfully AND is older than `cutoff`.
///
/// If the timestamp cannot be parsed, returns `false` (conservative — do NOT
/// treat a malformed timestamp as stale, since that could cause lost state on
/// a device with a corrupted clock).
fn is_stale(iso_ts_utc: &str, cutoff: Duration) -> bool {
    match DateTime::parse_from_rfc3339(iso_ts_utc) {
        Ok(dt) => {
            let age = Utc::now().signed_duration_since(dt.with_timezone(&Utc));
            age.to_std().map(|d| d > cutoff).unwrap_or(false)
        }
        Err(_) => false,
    }
}

/// Write `.self_healthy_{VERSION}` and clean up the state files that are no
/// longer needed. Also removes old `{binary_name}.rollback.{ts}` files EXCEPT
/// the one currently recorded in the pending marker (which is the canonical
/// rollback target and must remain available).
fn write_self_healthy_and_cleanup(install_dir: &Path, version: &str) -> Result<(), ProbeError> {
    // Read the pending marker FIRST to capture backup_path before deleting it.
    let install_pending_path = install_dir.join(format!(".install_pending_{version}"));
    let keep_backup: Option<PathBuf> = match std::fs::read(&install_pending_path) {
        Ok(bytes) => serde_json::from_slice::<InstallPending>(&bytes)
            .ok()
            .map(|p| p.backup_path),
        Err(_) => None,
    };

    // Write the self-healthy marker.
    let marker_path = install_dir.join(format!(".self_healthy_{version}"));
    std::fs::write(&marker_path, Utc::now().to_rfc3339())?;

    // Remove now-stale pending + boot_count files (ignore failures — cleanup
    // is best-effort).
    let _ = std::fs::remove_file(&install_pending_path);
    let _ = std::fs::remove_file(install_dir.join(format!(".boot_count_{version}")));

    // Sweep sibling rollback backups + foreign-version state files.
    //
    // Loop 3 iter 1 fix (I-2): previously this sweep only removed
    // `*.rollback.*` files, leaving stale `.install_pending_{OLDER}` /
    // `.boot_count_{OLDER}` / `.self_healthy_{OLDER}` files to accrete
    // across upgrades. Now also reclaim state files whose version suffix
    // does NOT match the current version — the current probe has just
    // written its own self_healthy marker, so anything else is stale.
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            // (a) Rollback backup sweep (existing behavior).
            if name.contains(".rollback.") {
                if let Some(keep) = &keep_backup {
                    if path == *keep {
                        continue;
                    }
                }
                let _ = std::fs::remove_file(&path);
                continue;
            }

            // (b) Foreign-version state-file sweep. Match
            //     `.install_pending_<VER>`, `.boot_count_<VER>`,
            //     `.self_healthy_<VER>` where VER != the current version.
            for prefix in [".install_pending_", ".boot_count_", ".self_healthy_"] {
                if let Some(ver_suffix) = name.strip_prefix(prefix) {
                    if ver_suffix != version {
                        let _ = std::fs::remove_file(&path);
                    }
                    break;
                }
            }
        }
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use tempfile::tempdir;

    fn write_pending(dir: &Path, version: &str, installed_at: &str, previous: &str, backup: &Path) {
        let pending = InstallPending {
            installed_at: installed_at.to_string(),
            previous_version: previous.to_string(),
            backup_path: backup.to_path_buf(),
        };
        let bytes = serde_json::to_vec(&pending).unwrap();
        std::fs::write(dir.join(format!(".install_pending_{version}")), bytes).unwrap();
    }

    fn write_boot_count(dir: &Path, version: &str, count: u32) {
        std::fs::write(
            dir.join(format!(".boot_count_{version}")),
            count.to_string(),
        )
        .unwrap();
    }

    fn write_self_healthy(dir: &Path, version: &str) {
        std::fs::write(
            dir.join(format!(".self_healthy_{version}")),
            Utc::now().to_rfc3339(),
        )
        .unwrap();
    }

    #[test]
    fn check_startup_no_pending_install_is_normal() {
        let dir = tempdir().unwrap();
        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);
    }

    #[test]
    fn check_startup_with_healthy_marker_is_normal() {
        let dir = tempdir().unwrap();
        // Even with a pending install, a present healthy marker short-circuits to Normal.
        let backup = dir.path().join("oneshim.rollback.1");
        std::fs::write(&backup, b"backup-bytes").unwrap();
        write_pending(
            dir.path(),
            "0.5.0",
            &Utc::now().to_rfc3339(),
            "0.4.39",
            &backup,
        );
        write_self_healthy(dir.path(), "0.5.0");

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);
    }

    #[test]
    fn check_startup_below_failed_boot_threshold_is_normal() {
        let dir = tempdir().unwrap();
        let backup = dir.path().join("oneshim.rollback.1");
        std::fs::write(&backup, b"backup-bytes").unwrap();
        write_pending(
            dir.path(),
            "0.5.0",
            &Utc::now().to_rfc3339(),
            "0.4.39",
            &backup,
        );
        // boot_count=0 on disk → probe reads 0, sees 0 < 2, increments to 1, returns Normal.

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);

        // Confirm counter was bumped.
        let new_count = read_boot_count(&probe.boot_count_path()).unwrap();
        assert_eq!(new_count, 1);
    }

    #[test]
    fn check_startup_at_failed_boot_threshold_triggers_rollback() {
        let dir = tempdir().unwrap();
        let backup = dir.path().join("oneshim.rollback.1");
        std::fs::write(&backup, b"backup-bytes").unwrap();
        write_pending(
            dir.path(),
            "0.5.0",
            &Utc::now().to_rfc3339(),
            "0.4.39",
            &backup,
        );
        write_boot_count(dir.path(), "0.5.0", 2); // at threshold

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        match probe.check_startup_state() {
            StartupAction::RollbackRequired {
                from_version,
                to_version,
                backup_path,
                reason,
            } => {
                assert_eq!(from_version, "0.5.0");
                assert_eq!(to_version, "0.4.39");
                assert_eq!(backup_path, backup);
                assert_eq!(reason, RollbackReason::RepeatedStartupFailure);
            }
            other => panic!("Expected RollbackRequired, got {:?}", other),
        }

        // At-threshold does NOT bump the counter further; the next boot's
        // probe will still see count=2 if rollback somehow fails to execute.
        let count_after = read_boot_count(&probe.boot_count_path()).unwrap();
        assert_eq!(count_after, 2);
    }

    #[test]
    fn stale_install_pending_older_than_24h_returns_normal_without_rollback() {
        let dir = tempdir().unwrap();
        let backup = dir.path().join("oneshim.rollback.1");
        std::fs::write(&backup, b"backup-bytes").unwrap();
        // installed_at 25 hours ago
        let old_ts = (Utc::now() - ChronoDuration::hours(25)).to_rfc3339();
        write_pending(dir.path(), "0.5.0", &old_ts, "0.4.39", &backup);
        write_boot_count(dir.path(), "0.5.0", 5); // would trigger if not stale

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);

        // Staleness rule deletes the pending + boot_count files.
        assert!(!probe.install_pending_path().exists());
        assert!(!probe.boot_count_path().exists());
    }

    #[tokio::test]
    async fn spawn_healthy_writer_sets_marker_after_injected_short_delay() {
        let dir = tempdir().unwrap();
        let backup = dir.path().join("oneshim.rollback.1");
        std::fs::write(&backup, b"backup-bytes").unwrap();
        write_pending(
            dir.path(),
            "0.5.0",
            &Utc::now().to_rfc3339(),
            "0.4.39",
            &backup,
        );
        write_boot_count(dir.path(), "0.5.0", 0);

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into())
            .with_threshold(Duration::from_millis(50));

        let handle = probe.spawn_healthy_writer();
        handle.await.unwrap();

        let marker = dir.path().join(".self_healthy_0.5.0");
        assert!(marker.exists(), "healthy marker should have been written");
        // Cleanup should have removed install_pending + boot_count.
        assert!(!probe.install_pending_path().exists());
        assert!(!probe.boot_count_path().exists());
        // Backup_path recorded in pending should survive the sweep.
        assert!(backup.exists(), "canonical backup should remain");
    }

    #[test]
    fn healthy_writer_cleanup_sweeps_foreign_version_state_files() {
        let dir = tempdir().unwrap();
        let current_version = "0.5.0";

        // Seed a fresh install_pending for the CURRENT version (no backup;
        // the sweep doesn't touch the non-existent path, but the code path
        // only uses backup_path for the `.rollback.` exclusion).
        write_pending(
            dir.path(),
            current_version,
            &Utc::now().to_rfc3339(),
            "0.4.40",
            &dir.path().join("nonexistent-backup"),
        );

        // Seed stale state from a previous version.
        std::fs::write(dir.path().join(".install_pending_0.4.40"), "stale-content").unwrap();
        std::fs::write(dir.path().join(".boot_count_0.4.40"), "2").unwrap();
        std::fs::write(
            dir.path().join(".self_healthy_0.4.40"),
            Utc::now().to_rfc3339(),
        )
        .unwrap();

        // Also seed a LOOKALIKE file that should NOT be swept (different prefix).
        std::fs::write(dir.path().join("unrelated.txt"), "keep me").unwrap();

        // Invoke the cleanup helper directly.
        write_self_healthy_and_cleanup(dir.path(), current_version).unwrap();

        // Self-healthy for current version: written.
        assert!(dir.path().join(".self_healthy_0.5.0").exists());
        // Current version's pending + boot_count: removed.
        assert!(!dir.path().join(".install_pending_0.5.0").exists());
        assert!(!dir.path().join(".boot_count_0.5.0").exists());

        // Foreign-version state files: swept.
        assert!(!dir.path().join(".install_pending_0.4.40").exists());
        assert!(!dir.path().join(".boot_count_0.4.40").exists());
        assert!(!dir.path().join(".self_healthy_0.4.40").exists());

        // Unrelated file: untouched.
        assert!(dir.path().join("unrelated.txt").exists());
    }

    #[test]
    fn probe_io_error_is_non_fatal() {
        // Point install_dir at a non-existent sub-path. The inner probe will
        // encounter read failures (no install_pending → Normal short-circuit);
        // to actually exercise the error path, create a malformed pending file.
        let dir = tempdir().unwrap();
        let pending_path = dir.path().join(".install_pending_0.5.0");
        std::fs::write(&pending_path, b"NOT VALID JSON {{{").unwrap();

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        // Public wrapper catches InstallPendingParse → returns Normal.
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);
    }
}
