// 스토리지/무결성/알림/업데이트/텔레메트리 설정 — 데이터 생명주기 및 시스템 상태 관리
use crate::error::CoreError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── StorageConfig ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub db_path: Option<PathBuf>,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_max_storage_mb")]
    pub max_storage_mb: u64,
}

// ── IntegrityConfig ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityConfig {
    #[serde(default = "default_integrity_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub require_signed_policy_bundle: bool,
    #[serde(default)]
    pub policy_file_path: Option<String>,
    #[serde(default)]
    pub policy_signature_path: Option<String>,
    #[serde(default)]
    pub policy_public_key: Option<String>,
}

impl Default for IntegrityConfig {
    fn default() -> Self {
        Self {
            enabled: default_integrity_enabled(),
            require_signed_policy_bundle: true,
            policy_file_path: None,
            policy_signature_path: None,
            policy_public_key: None,
        }
    }
}

// ── TelemetryConfig ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub crash_reports: bool,
    #[serde(default)]
    pub usage_analytics: bool,
    #[serde(default)]
    pub performance_metrics: bool,
}

// ── NotificationConfig ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_notification_enabled")]
    pub enabled: bool,
    #[serde(default = "default_idle_notification")]
    pub idle_notification: bool,
    #[serde(default = "default_idle_notification_mins")]
    pub idle_notification_mins: u32,
    #[serde(default = "default_long_session_notification")]
    pub long_session_notification: bool,
    #[serde(default = "default_long_session_mins")]
    pub long_session_mins: u32,
    #[serde(default = "default_high_usage_notification")]
    pub high_usage_notification: bool,
    #[serde(default = "default_high_usage_threshold")]
    pub high_usage_threshold: u32,
    #[serde(default)]
    pub daily_summary_notification: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: default_notification_enabled(),
            idle_notification: default_idle_notification(),
            idle_notification_mins: default_idle_notification_mins(),
            long_session_notification: default_long_session_notification(),
            long_session_mins: default_long_session_mins(),
            high_usage_notification: default_high_usage_notification(),
            high_usage_threshold: default_high_usage_threshold(),
            daily_summary_notification: false,
        }
    }
}

// ── UpdateConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_update_enabled")]
    pub enabled: bool,
    #[serde(default = "default_repo_owner")]
    pub repo_owner: String,
    #[serde(default = "default_repo_name")]
    pub repo_name: String,
    #[serde(default = "default_check_interval_hours")]
    pub check_interval_hours: u32,
    #[serde(default)]
    pub include_prerelease: bool,
    #[serde(default)]
    pub auto_install: bool,
    #[serde(default = "default_update_require_signature")]
    pub require_signature_verification: bool,
    #[serde(default = "default_update_signature_public_key")]
    pub signature_public_key: String,
    #[serde(default)]
    pub min_allowed_version: Option<String>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: default_update_enabled(),
            repo_owner: default_repo_owner(),
            repo_name: default_repo_name(),
            check_interval_hours: default_check_interval_hours(),
            include_prerelease: false,
            auto_install: false,
            require_signature_verification: default_update_require_signature(),
            signature_public_key: default_update_signature_public_key(),
            min_allowed_version: None,
        }
    }
}

impl UpdateConfig {
    pub fn validate_integrity_policy(&self) -> Result<(), CoreError> {
        if !self.enabled {
            return Ok(());
        }

        if !self.require_signature_verification {
            return Err(CoreError::Config(
                "update.require_signature_verification must be true when updates are enabled"
                    .to_string(),
            ));
        }

        let key_b64 = self
            .signature_public_key
            .split_whitespace()
            .next()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                CoreError::Config(
                    "update.signature_public_key is missing while updates are enabled".to_string(),
                )
            })?;

        let key_bytes = BASE64.decode(key_b64).map_err(|e| {
            CoreError::Config(format!(
                "update.signature_public_key must be valid base64: {}",
                e
            ))
        })?;

        if key_bytes.len() != 32 {
            return Err(CoreError::Config(format!(
                "update.signature_public_key must decode to 32 bytes, got {}",
                key_bytes.len()
            )));
        }

        if let Some(version_floor) = self
            .min_allowed_version
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            semver::Version::parse(version_floor).map_err(|e| {
                CoreError::Config(format!(
                    "update.min_allowed_version must be valid semver: {}",
                    e
                ))
            })?;
        }

        Ok(())
    }
}

// ── Default / helper functions (pub(super) — config/mod.rs 에서 사용) ─

pub(crate) fn default_retention_days() -> u32 {
    30
}

pub(crate) fn default_max_storage_mb() -> u64 {
    500
}

// ── Private default helpers ─────────────────────────────────────────

fn default_integrity_enabled() -> bool {
    true
}

fn default_notification_enabled() -> bool {
    true
}

fn default_idle_notification() -> bool {
    true
}

fn default_idle_notification_mins() -> u32 {
    30
}

fn default_long_session_notification() -> bool {
    true
}

fn default_long_session_mins() -> u32 {
    60
}

fn default_high_usage_notification() -> bool {
    false
}

fn default_high_usage_threshold() -> u32 {
    90
}

fn default_update_enabled() -> bool {
    true
}

fn default_repo_owner() -> String {
    "pseudotop".to_string()
}

fn default_repo_name() -> String {
    "oneshim-client".to_string()
}

fn default_check_interval_hours() -> u32 {
    24
}

fn default_update_require_signature() -> bool {
    true
}

fn default_update_signature_public_key() -> String {
    "GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E=".to_string()
}
