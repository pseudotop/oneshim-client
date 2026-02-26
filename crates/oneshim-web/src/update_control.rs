pub use oneshim_api_contracts::update::{
    PendingUpdateInfo, UpdateAction, UpdatePhase, UpdateStatus,
};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

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
