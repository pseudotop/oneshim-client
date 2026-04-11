use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdatePhase {
    Idle,
    Checking,
    PendingApproval,
    Downloading,
    ReadyToInstall,
    Installing,
    Updated,
    Deferred,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PendingUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub download_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub enabled: bool,
    pub auto_install: bool,
    pub phase: UpdatePhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingUpdateInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_progress: Option<DownloadProgress>,
    pub revision: u64,
    pub updated_at: String,
}

impl Default for UpdateStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_install: false,
            phase: UpdatePhase::Idle,
            message: None,
            pending: None,
            download_progress: None,
            revision: 0,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

impl UpdateStatus {
    pub fn touch(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.updated_at = Utc::now().to_rfc3339();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateAction {
    Approve,
    Defer,
    CheckNow,
}

#[derive(Debug, Deserialize)]
pub struct UpdateActionRequest {
    pub action: UpdateAction,
}

#[derive(Debug, Serialize)]
pub struct UpdateActionResponse {
    pub accepted: bool,
    pub status: UpdateStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_update_phase() {
        for phase in [
            UpdatePhase::Idle,
            UpdatePhase::Checking,
            UpdatePhase::PendingApproval,
            UpdatePhase::Downloading,
            UpdatePhase::ReadyToInstall,
            UpdatePhase::Installing,
            UpdatePhase::Updated,
            UpdatePhase::Deferred,
            UpdatePhase::Error,
        ] {
            let json = serde_json::to_string(&phase).unwrap();
            let decoded: UpdatePhase = serde_json::from_str(&json).unwrap();
            assert_eq!(phase, decoded);
        }
    }

    #[test]
    fn round_trip_pending_update_info() {
        let original = PendingUpdateInfo {
            current_version: "0.4.32".to_string(),
            latest_version: "0.4.33".to_string(),
            release_url: "https://github.com/example/releases/v0.4.33".to_string(),
            release_name: Some("v0.4.33".to_string()),
            published_at: Some("2026-04-11T00:00:00Z".to_string()),
            download_url: "https://github.com/example/releases/v0.4.33/app.tar.gz".to_string(),
            release_notes: Some("Bug fixes and performance improvements.".to_string()),
            download_size_bytes: Some(12_345_678),
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: PendingUpdateInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn round_trip_download_progress() {
        let original = DownloadProgress {
            bytes_downloaded: 6_172_839,
            total_bytes: 12_345_678,
            percent: 50.0,
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: DownloadProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn round_trip_update_action() {
        for action in [
            UpdateAction::Approve,
            UpdateAction::Defer,
            UpdateAction::CheckNow,
        ] {
            let json = serde_json::to_string(&action).unwrap();
            let decoded: UpdateAction = serde_json::from_str(&json).unwrap();
            assert_eq!(action, decoded);
        }
    }

    #[test]
    fn pending_update_info_optional_fields_skipped_when_none() {
        let original = PendingUpdateInfo {
            current_version: "0.4.32".to_string(),
            latest_version: "0.4.33".to_string(),
            release_url: "https://github.com/example/releases/v0.4.33".to_string(),
            release_name: None,
            published_at: None,
            download_url: "https://example.com/app.tar.gz".to_string(),
            release_notes: None,
            download_size_bytes: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(!json.contains("release_name"));
        assert!(!json.contains("release_notes"));
        assert!(!json.contains("download_size_bytes"));
        let decoded: PendingUpdateInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }
}
