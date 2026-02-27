use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::CoreError;

pub const CURRENT_POLICY_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsentPermissions {
    // --- Tier 1 ---
    #[serde(default)]
    pub screen_capture: bool,
    #[serde(default)]
    pub ocr_processing: bool,
    #[serde(default)]
    pub telemetry: bool,
    #[serde(default)]
    pub process_monitoring: bool,
    #[serde(default)]
    pub input_activity: bool,

    // --- Tier 2 ---
    #[serde(default)]
    pub window_title_collection: bool,
    #[serde(default)]
    pub app_usage_analytics: bool,

    // --- Tier 3 ---
    #[serde(default)]
    pub clipboard_monitoring: bool,
    #[serde(default)]
    pub file_access_monitoring: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub consent_id: String,
    pub version: String,
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub permissions: ConsentPermissions,
    pub data_retention_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentStatus {
    NotGranted,
    Valid,
    Expired,
    UpdateRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDataExport {
    pub exported_at: DateTime<Utc>,
    pub consent: Option<ConsentRecord>,
    pub settings: serde_json::Value,
    pub event_count: u64,
    pub frame_count: u64,
    pub export_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionResult {
    pub deleted_at: DateTime<Utc>,
    pub events_deleted: u64,
    pub frames_deleted: u64,
    pub metrics_deleted: u64,
    pub settings_reset: bool,
    pub consent_revoked: bool,
}

// ConsentManager

pub struct ConsentManager {
    storage_path: PathBuf,
    current_consent: Option<ConsentRecord>,
}

impl ConsentManager {
    pub fn new(storage_path: PathBuf) -> Self {
        let current_consent = Self::load_from_file(&storage_path);
        Self {
            storage_path,
            current_consent,
        }
    }

    pub fn check_consent(&self) -> ConsentStatus {
        match &self.current_consent {
            None => ConsentStatus::NotGranted,
            Some(record) => {
                if let Some(expires) = record.expires_at {
                    if Utc::now() > expires {
                        return ConsentStatus::Expired;
                    }
                }
                if record.version != CURRENT_POLICY_VERSION {
                    return ConsentStatus::UpdateRequired;
                }
                ConsentStatus::Valid
            }
        }
    }

    pub fn current_consent(&self) -> Option<&ConsentRecord> {
        self.current_consent.as_ref()
    }

    pub fn grant_consent(
        &mut self,
        permissions: ConsentPermissions,
        data_retention_days: u32,
    ) -> Result<(), CoreError> {
        let record = ConsentRecord {
            consent_id: uuid::Uuid::new_v4().to_string(),
            version: CURRENT_POLICY_VERSION.to_string(),
            granted_at: Utc::now(),
            expires_at: None,
            permissions,
            data_retention_days,
        };

        self.save_to_file(&record)?;
        self.current_consent = Some(record);
        Ok(())
    }

    pub fn revoke_consent(&mut self) -> Result<(), CoreError> {
        if self.storage_path.exists() {
            std::fs::remove_file(&self.storage_path)?;
        }
        self.current_consent = None;
        Ok(())
    }

    pub fn is_permitted(&self, check: impl Fn(&ConsentPermissions) -> bool) -> bool {
        self.current_consent
            .as_ref()
            .map(|r| check(&r.permissions))
            .unwrap_or(false)
    }

    fn load_from_file(path: &PathBuf) -> Option<ConsentRecord> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn save_to_file(&self, record: &ConsentRecord) -> Result<(), CoreError> {
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(record)?;
        std::fs::write(&self.storage_path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_permissions_default_all_false() {
        let perms = ConsentPermissions::default();
        assert!(!perms.screen_capture);
        assert!(!perms.telemetry);
        assert!(!perms.clipboard_monitoring);
    }

    #[test]
    fn consent_record_serde_roundtrip() {
        let record = ConsentRecord {
            consent_id: "test-001".to_string(),
            version: CURRENT_POLICY_VERSION.to_string(),
            granted_at: Utc::now(),
            expires_at: None,
            permissions: ConsentPermissions::default(),
            data_retention_days: 30,
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ConsentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.consent_id, "test-001");
        assert_eq!(deserialized.data_retention_days, 30);
    }

    #[test]
    fn consent_status_not_granted_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let manager = ConsentManager::new(path);
        assert_eq!(manager.check_consent(), ConsentStatus::NotGranted);
    }

    #[test]
    fn consent_grant_and_check() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        let perms = ConsentPermissions {
            screen_capture: true,
            ..Default::default()
        };
        manager.grant_consent(perms, 30).unwrap();

        assert_eq!(manager.check_consent(), ConsentStatus::Valid);
        assert!(manager.is_permitted(|p| p.screen_capture));
        assert!(!manager.is_permitted(|p| p.clipboard_monitoring));
    }

    #[test]
    fn consent_revoke() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        let perms = ConsentPermissions::default();
        manager.grant_consent(perms, 30).unwrap();
        assert_eq!(manager.check_consent(), ConsentStatus::Valid);

        manager.revoke_consent().unwrap();
        assert_eq!(manager.check_consent(), ConsentStatus::NotGranted);
    }

    #[test]
    fn consent_expired() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        let record = ConsentRecord {
            consent_id: "expired-001".to_string(),
            version: CURRENT_POLICY_VERSION.to_string(),
            granted_at: Utc::now() - chrono::Duration::days(365),
            expires_at: Some(Utc::now() - chrono::Duration::days(1)),
            permissions: ConsentPermissions::default(),
            data_retention_days: 30,
        };
        manager.current_consent = Some(record);
        assert_eq!(manager.check_consent(), ConsentStatus::Expired);
    }

    #[test]
    fn consent_update_required_on_version_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        let record = ConsentRecord {
            consent_id: "old-001".to_string(),
            version: "0.9.0".to_string(), // previous version
            granted_at: Utc::now(),
            expires_at: None,
            permissions: ConsentPermissions::default(),
            data_retention_days: 30,
        };
        manager.current_consent = Some(record);
        assert_eq!(manager.check_consent(), ConsentStatus::UpdateRequired);
    }
}
