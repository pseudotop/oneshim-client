use crate::updater::{UpdateCheckResult, UpdateError, Updater};
use async_trait::async_trait;
use chrono::Utc;
use oneshim_core::config::UpdateConfig;
use oneshim_web::update_control::{PendingUpdateInfo, UpdateAction, UpdatePhase, UpdateStatus};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info, warn};

#[async_trait]
pub trait UpdateExecutor: Send + Sync {
    fn should_check_for_updates(&self) -> bool;
    async fn check_for_updates(&self) -> Result<UpdateCheckResult, UpdateError>;
    fn save_last_check_time(&self) -> Result<(), UpdateError>;
    async fn download_update(&self, download_url: &str) -> Result<PathBuf, UpdateError>;
    fn install_and_restart(&self, downloaded_path: &PathBuf) -> Result<(), UpdateError>;
}

#[async_trait]
impl UpdateExecutor for Updater {
    fn should_check_for_updates(&self) -> bool {
        self.should_check_for_updates()
    }

    async fn check_for_updates(&self) -> Result<UpdateCheckResult, UpdateError> {
        self.check_for_updates().await
    }

    fn save_last_check_time(&self) -> Result<(), UpdateError> {
        self.save_last_check_time()
    }

    async fn download_update(&self, download_url: &str) -> Result<PathBuf, UpdateError> {
        self.download_update(download_url).await
    }

    fn install_and_restart(&self, downloaded_path: &PathBuf) -> Result<(), UpdateError> {
        self.install_and_restart(downloaded_path)
    }
}

pub fn initial_status(config: &UpdateConfig, auto_install: bool) -> UpdateStatus {
    UpdateStatus {
        enabled: config.enabled,
        auto_install,
        phase: UpdatePhase::Idle,
        message: None,
        pending: None,
        revision: 0,
        updated_at: Utc::now().to_rfc3339(),
    }
}

pub async fn run_update_coordinator(
    config: UpdateConfig,
    state: Arc<RwLock<UpdateStatus>>,
    action_rx: mpsc::UnboundedReceiver<UpdateAction>,
    status_tx: Option<broadcast::Sender<UpdateStatus>>,
    auto_install: bool,
) {
    if !config.enabled {
        let mut guard = state.write().await;
        guard.phase = UpdatePhase::Idle;
        guard.message = Some("Update feature is disabled".to_string());
        guard.pending = None;
        guard.touch();
        if let Some(tx) = &status_tx {
            let _ = tx.send(guard.clone());
        }
        return;
    }

    let updater = Updater::new(config);

    run_update_coordinator_with_executor(updater, state, action_rx, status_tx, auto_install).await;
}

pub async fn run_update_coordinator_with_executor<E: UpdateExecutor + 'static>(
    updater: E,
    state: Arc<RwLock<UpdateStatus>>,
    mut action_rx: mpsc::UnboundedReceiver<UpdateAction>,
    status_tx: Option<broadcast::Sender<UpdateStatus>>,
    auto_install: bool,
) {
    if let Some(tx) = &status_tx {
        let snapshot = state.read().await.clone();
        let _ = tx.send(snapshot);
    }

    if updater.should_check_for_updates() {
        run_check(&updater, &state, status_tx.as_ref(), auto_install).await;
    }

    while let Some(action) = action_rx.recv().await {
        match action {
            UpdateAction::CheckNow => {
                run_check(&updater, &state, status_tx.as_ref(), auto_install).await;
            }
            UpdateAction::Approve => {
                if let Err(e) = apply_pending_update(&updater, &state, status_tx.as_ref()).await {
                    let mut guard = state.write().await;
                    guard.phase = UpdatePhase::Error;
                    guard.message = Some(format!("Failed to apply update: {}", e));
                    guard.touch();
                    if let Some(tx) = &status_tx {
                        let _ = tx.send(guard.clone());
                    }
                }
            }
            UpdateAction::Defer => {
                let mut guard = state.write().await;
                guard.phase = UpdatePhase::Deferred;
                guard.message = Some("Update was deferred".to_string());
                guard.pending = None;
                guard.touch();
                if let Some(tx) = &status_tx {
                    let _ = tx.send(guard.clone());
                }
            }
        }
    }
}

async fn apply_pending_update<E: UpdateExecutor>(
    updater: &E,
    state: &Arc<RwLock<UpdateStatus>>,
    status_tx: Option<&broadcast::Sender<UpdateStatus>>,
) -> Result<(), UpdateError> {
    let pending = {
        let guard = state.read().await;
        guard.pending.clone()
    };

    let Some(pending) = pending else {
        let mut guard = state.write().await;
        guard.phase = UpdatePhase::Idle;
        guard.message = Some("No pending update to apply".to_string());
        guard.touch();
        if let Some(tx) = status_tx {
            let _ = tx.send(guard.clone());
        }
        return Ok(());
    };

    {
        let mut guard = state.write().await;
        guard.phase = UpdatePhase::Installing;
        guard.message = Some(format!("Installing version {}", pending.latest_version));
        guard.touch();
        if let Some(tx) = status_tx {
            let _ = tx.send(guard.clone());
        }
    }

    let path = updater.download_update(&pending.download_url).await?;
    match updater.install_and_restart(&path) {
        Ok(()) => {
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Updated;
            guard.message = Some(format!("Updated to version {}", pending.latest_version));
            guard.pending = None;
            guard.touch();
            if let Some(tx) = status_tx {
                let _ = tx.send(guard.clone());
            }
            Ok(())
        }
        Err(e) => {
            error!("Update installation failed: {}", e);
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Error;
            guard.message = Some(format!("Update installation failed: {}", e));
            guard.touch();
            if let Some(tx) = status_tx {
                let _ = tx.send(guard.clone());
            }
            Err(e)
        }
    }
}

async fn run_check<E: UpdateExecutor>(
    updater: &E,
    state: &Arc<RwLock<UpdateStatus>>,
    status_tx: Option<&broadcast::Sender<UpdateStatus>>,
    auto_install: bool,
) {
    {
        let mut guard = state.write().await;
        guard.phase = UpdatePhase::Checking;
        guard.message = Some("Checking for new version".to_string());
        guard.pending = None;
        guard.touch();
        if let Some(tx) = status_tx {
            let _ = tx.send(guard.clone());
        }
    }

    let result = updater.check_for_updates().await;
    match result {
        Ok(UpdateCheckResult::UpToDate { current }) => {
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Idle;
            guard.message = Some(format!("Already on latest version: {}", current));
            guard.pending = None;
            guard.touch();
            if let Some(tx) = status_tx {
                let _ = tx.send(guard.clone());
            }
        }
        Ok(UpdateCheckResult::Available {
            current,
            latest,
            release,
            download_url,
        }) => {
            {
                let mut guard = state.write().await;
                guard.phase = UpdatePhase::PendingApproval;
                guard.message = Some(format!("New version detected: {} -> {}", current, latest));
                guard.pending = Some(PendingUpdateInfo {
                    current_version: current.to_string(),
                    latest_version: latest.to_string(),
                    release_url: release.html_url.clone(),
                    release_name: release.name.clone(),
                    published_at: release.published_at.clone(),
                    download_url,
                });
                guard.touch();
                if let Some(tx) = status_tx {
                    let _ = tx.send(guard.clone());
                }
            }

            if auto_install {
                info!("Auto-update mode enabled: installing immediately");
                if let Err(e) = apply_pending_update(updater, state, status_tx).await {
                    let mut guard = state.write().await;
                    guard.phase = UpdatePhase::Error;
                    guard.message = Some(format!("Auto-install failed: {}", e));
                    guard.touch();
                    if let Some(tx) = status_tx {
                        let _ = tx.send(guard.clone());
                    }
                }
            }
        }
        Err(UpdateError::Disabled) => {
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Idle;
            guard.message = Some("Update feature is disabled".to_string());
            guard.pending = None;
            guard.touch();
            if let Some(tx) = status_tx {
                let _ = tx.send(guard.clone());
            }
        }
        Err(e) => {
            warn!("Failed to check for updates: {}", e);
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Error;
            guard.message = Some(format!("Failed to check for updates: {}", e));
            guard.pending = None;
            guard.touch();
            if let Some(tx) = status_tx {
                let _ = tx.send(guard.clone());
            }
        }
    }

    if let Err(e) = updater.save_last_check_time() {
        warn!("Failed to persist last update check timestamp: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::{ReleaseAsset, ReleaseInfo};
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::Mutex;

    #[derive(Clone)]
    struct FakeUpdater {
        should_check: bool,
        check_results: Arc<Mutex<VecDeque<Result<UpdateCheckResult, UpdateError>>>>,
        install_error: Arc<StdMutex<Option<String>>>,
    }

    impl FakeUpdater {
        fn with_result(result: Result<UpdateCheckResult, UpdateError>) -> Self {
            Self {
                should_check: false,
                check_results: Arc::new(Mutex::new(VecDeque::from([result]))),
                install_error: Arc::new(StdMutex::new(None)),
            }
        }

        fn set_install_error(&self, err: &str) {
            *self
                .install_error
                .lock()
                .expect("install_error mutex poisoned") = Some(err.to_string());
        }
    }

    #[async_trait]
    impl UpdateExecutor for FakeUpdater {
        fn should_check_for_updates(&self) -> bool {
            self.should_check
        }

        async fn check_for_updates(&self) -> Result<UpdateCheckResult, UpdateError> {
            self.check_results
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| {
                    Ok(UpdateCheckResult::UpToDate {
                        current: semver::Version::parse("0.0.1").expect("valid test semver"),
                    })
                })
        }

        fn save_last_check_time(&self) -> Result<(), UpdateError> {
            Ok(())
        }

        async fn download_update(&self, _download_url: &str) -> Result<PathBuf, UpdateError> {
            Ok(PathBuf::from("/tmp/oneshim-test-update.tar.gz"))
        }

        fn install_and_restart(&self, _downloaded_path: &PathBuf) -> Result<(), UpdateError> {
            if let Some(err) = self
                .install_error
                .lock()
                .expect("install_error mutex poisoned")
                .clone()
            {
                return Err(UpdateError::Install(err));
            }
            Ok(())
        }
    }

    fn make_available_result(
        current: &str,
        latest: &str,
    ) -> Result<UpdateCheckResult, UpdateError> {
        Ok(UpdateCheckResult::Available {
            current: semver::Version::parse(current).expect("valid current version"),
            latest: semver::Version::parse(latest).expect("valid latest version"),
            release: Box::new(ReleaseInfo {
                tag_name: format!("v{}", latest),
                name: Some(format!("Release {}", latest)),
                body: None,
                prerelease: false,
                assets: vec![ReleaseAsset {
                    name: "oneshim-macos-arm64.tar.gz".to_string(),
                    browser_download_url:
                        "https://github.com/pseudotop/oneshim-client/releases/download/v1.2.0/oneshim-macos-arm64.tar.gz"
                            .to_string(),
                    size: 123,
                    content_type: "application/gzip".to_string(),
                }],
                html_url: "https://github.com/pseudotop/oneshim-client/releases/v1.2.0".to_string(),
                published_at: Some("2026-02-21T10:00:00Z".to_string()),
            }),
            download_url:
                "https://github.com/pseudotop/oneshim-client/releases/download/v1.2.0/oneshim-macos-arm64.tar.gz"
                    .to_string(),
        })
    }

    #[tokio::test]
    async fn e2e_update_flow_detect_to_approval_to_install_success() {
        let fake = FakeUpdater::with_result(make_available_result("1.0.0", "1.2.0"));
        let state = Arc::new(RwLock::new(UpdateStatus::default()));
        let (tx, rx) = mpsc::unbounded_channel();

        let coordinator = tokio::spawn(run_update_coordinator_with_executor(
            fake,
            state.clone(),
            rx,
            None,
            false,
        ));

        tx.send(UpdateAction::CheckNow).expect("send check action");
        tx.send(UpdateAction::Approve).expect("send approve action");
        drop(tx);
        coordinator.await.expect("join coordinator task");

        let final_state = state.read().await.clone();
        assert_eq!(final_state.phase, UpdatePhase::Updated);
        assert!(final_state.pending.is_none());
        assert!(
            final_state
                .message
                .expect("message")
                .to_lowercase()
                .contains("update")
        );
    }

    #[tokio::test]
    async fn e2e_update_flow_install_failure_sets_error() {
        let fake = FakeUpdater::with_result(make_available_result("1.0.0", "1.2.0"));
        fake.set_install_error("simulated restart failure");
        let state = Arc::new(RwLock::new(UpdateStatus::default()));
        let (tx, rx) = mpsc::unbounded_channel();

        let coordinator = tokio::spawn(run_update_coordinator_with_executor(
            fake,
            state.clone(),
            rx,
            None,
            false,
        ));

        tx.send(UpdateAction::CheckNow).expect("send check action");
        tx.send(UpdateAction::Approve).expect("send approve action");
        drop(tx);
        coordinator.await.expect("join coordinator task");

        let final_state = state.read().await.clone();
        assert_eq!(final_state.phase, UpdatePhase::Error);
        assert!(
            final_state
                .message
                .expect("message")
                .to_lowercase()
                .contains("failed")
        );
    }

    #[tokio::test]
    async fn e2e_update_flow_defer_after_detect() {
        let fake = FakeUpdater::with_result(make_available_result("1.0.0", "1.2.0"));
        let state = Arc::new(RwLock::new(UpdateStatus::default()));
        let (tx, rx) = mpsc::unbounded_channel();

        let coordinator = tokio::spawn(run_update_coordinator_with_executor(
            fake,
            state.clone(),
            rx,
            None,
            false,
        ));

        tx.send(UpdateAction::CheckNow).expect("send check action");
        tx.send(UpdateAction::Defer).expect("send defer action");
        drop(tx);
        coordinator.await.expect("join coordinator task");

        let final_state = state.read().await.clone();
        assert_eq!(final_state.phase, UpdatePhase::Deferred);
        assert!(final_state.pending.is_none());
    }
}
