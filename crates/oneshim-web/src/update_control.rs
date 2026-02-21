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
}

impl Default for UpdateStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_install: false,
            phase: UpdatePhase::Idle,
            message: None,
            pending: None,
        }
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
