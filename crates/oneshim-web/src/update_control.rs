use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_increments_revision() {
        let mut status = UpdateStatus::default();
        let initial_revision = status.revision;
        status.touch();
        assert_eq!(status.revision, initial_revision + 1);
    }

    #[test]
    fn default_status_has_timestamp() {
        let status = UpdateStatus::default();
        assert!(!status.updated_at.is_empty());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpdateAction {
    Approve,
    Defer,
    CheckNow,
}

#[derive(Clone)]
pub struct UpdateControl {
    pub state: Arc<RwLock<UpdateStatus>>,
    pub action_tx: mpsc::UnboundedSender<UpdateAction>,
    pub event_tx: broadcast::Sender<UpdateStatus>,
}

impl UpdateControl {
    pub fn new(action_tx: mpsc::UnboundedSender<UpdateAction>, initial: UpdateStatus) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            state: Arc::new(RwLock::new(initial)),
            action_tx,
            event_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<UpdateStatus> {
        self.event_tx.subscribe()
    }
}
