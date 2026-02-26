use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdatePhase {
    Idle,
    Checking,
    PendingApproval,
    Installing,
    Updated,
    Deferred,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub release_name: Option<String>,
    pub published_at: Option<String>,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub enabled: bool,
    pub auto_install: bool,
    pub phase: UpdatePhase,
    pub message: Option<String>,
    pub pending: Option<PendingUpdateInfo>,
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
