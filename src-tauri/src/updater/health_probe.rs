//! Post-install self-healthy probe with 2-failed-boot automatic rollback.
//!
//! See `docs/reviews/2026-04-18-phase4-updater-hardening-design.md` §4 for
//! the full design. This module is scaffolding-only in Task 1; real behavior
//! lands in Task 5.
//!
//! State-machine summary (Task 5 implements):
//! - On every startup, `check_startup_state` inspects `.install_pending_{VERSION}`,
//!   `.boot_count_{VERSION}`, `.self_healthy_{VERSION}` in the install directory.
//! - If boot_count reaches `failed_boot_threshold` (2) without a self-healthy
//!   marker, returns `RollbackRequired` with the backup path recorded at install.
//! - Otherwise returns `Normal`; the scheduler later calls `spawn_healthy_writer`
//!   which writes the self-healthy marker after `healthy_threshold` (30s) of
//!   continuous wall-clock uptime.
//!
//! Ownership (spec Amendment 1 — applied in Task 1): both `check_startup_state`
//! and `spawn_healthy_writer` take `&self` so a single probe instance can be
//! created in `app_runtime_launch.rs`, used for the startup check, then shared
//! via `Arc` into the scheduler for the healthy-writer spawn.

#![allow(clippy::todo)] // Stubs filled in Task 5 per plan §"Commit + push cadence"

use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

/// Outcome of a startup probe check.
#[derive(Debug, Clone, PartialEq)]
pub enum StartupAction {
    /// Proceed with normal startup.
    Normal,
    /// Boot counter reached the failed-boot threshold without a self-healthy
    /// marker; caller should invoke `execute_rollback` with the enclosed metadata.
    RollbackRequired {
        from_version: String,
        to_version: String,
        backup_path: PathBuf,
        reason: RollbackReason,
    },
}

/// Why the probe escalated to `RollbackRequired`.
///
/// Additive enum — new reasons can be added without breaking existing consumers.
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
    #[error("install_pending_{{version}} file malformed: {0}")]
    InstallPendingParse(String),

    #[error("install_pending_{{version}} missing the required `backup_path` field")]
    InstallPendingMissingBackupPath,

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
        // Task 5 implements per plan.
        todo!("Task 5 — implement step 0 (staleness) → 1-2 (short-circuits) → 3-5 (boot count)")
    }

    /// Spawn a tokio background task that waits `healthy_threshold` then
    /// writes the self-healthy marker and cleans related state files.
    ///
    /// Takes `&self` (spec Amendment 1) — the probe instance stays owned by
    /// the launch path; the spawned task captures the data it needs by
    /// value at spawn time.
    pub fn spawn_healthy_writer(&self) -> tokio::task::JoinHandle<()> {
        // Task 5 implements per plan.
        let _dir = self.install_dir.clone();
        let _version = self.current_version.clone();
        let _threshold = self.healthy_threshold;
        tokio::spawn(async move {
            todo!("Task 5 — wait threshold, write self_healthy marker, clean state")
        })
    }
}
