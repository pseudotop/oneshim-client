use crate::updater::{UpdateCheckResult, UpdateError, Updater};
use oneshim_core::config::UpdateConfig;
use oneshim_web::update_control::{PendingUpdateInfo, UpdateAction, UpdatePhase, UpdateStatus};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

pub fn initial_status(config: &UpdateConfig, auto_install: bool) -> UpdateStatus {
    UpdateStatus {
        enabled: config.enabled,
        auto_install,
        phase: UpdatePhase::Idle,
        message: None,
        pending: None,
    }
}

pub async fn run_update_coordinator(
    config: UpdateConfig,
    state: Arc<RwLock<UpdateStatus>>,
    mut action_rx: mpsc::UnboundedReceiver<UpdateAction>,
    auto_install: bool,
) {
    if !config.enabled {
        let mut guard = state.write().await;
        guard.phase = UpdatePhase::Idle;
        guard.message = Some("업데이트 기능이 비활성화되어 있습니다".to_string());
        guard.pending = None;
        return;
    }

    let updater = Updater::new(config);

    if updater.should_check_for_updates() {
        run_check(&updater, &state, auto_install).await;
    }

    while let Some(action) = action_rx.recv().await {
        match action {
            UpdateAction::CheckNow => {
                run_check(&updater, &state, auto_install).await;
            }
            UpdateAction::Approve => {
                if let Err(e) = apply_pending_update(&updater, &state).await {
                    let mut guard = state.write().await;
                    guard.phase = UpdatePhase::Error;
                    guard.message = Some(format!("업데이트 적용 실패: {}", e));
                }
            }
            UpdateAction::Defer => {
                let mut guard = state.write().await;
                guard.phase = UpdatePhase::Deferred;
                guard.message = Some("업데이트를 나중에 설치하도록 연기했습니다".to_string());
                guard.pending = None;
            }
        }
    }
}

async fn run_check(updater: &Updater, state: &Arc<RwLock<UpdateStatus>>, auto_install: bool) {
    {
        let mut guard = state.write().await;
        guard.phase = UpdatePhase::Checking;
        guard.message = Some("새 버전을 확인하고 있습니다".to_string());
        guard.pending = None;
    }

    let result = updater.check_for_updates().await;
    match result {
        Ok(UpdateCheckResult::UpToDate { current }) => {
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Idle;
            guard.message = Some(format!("최신 버전 사용 중: {}", current));
            guard.pending = None;
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
                guard.message = Some(format!(
                    "새 버전이 감지되었습니다: {} -> {}",
                    current, latest
                ));
                guard.pending = Some(PendingUpdateInfo {
                    current_version: current.to_string(),
                    latest_version: latest.to_string(),
                    release_url: release.html_url.clone(),
                    release_name: release.name.clone(),
                    published_at: release.published_at.clone(),
                    download_url,
                });
            }

            if auto_install {
                info!("자동 업데이트 모드: 즉시 설치를 진행합니다");
                if let Err(e) = apply_pending_update(updater, state).await {
                    let mut guard = state.write().await;
                    guard.phase = UpdatePhase::Error;
                    guard.message = Some(format!("자동 설치 실패: {}", e));
                }
            }
        }
        Err(UpdateError::Disabled) => {
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Idle;
            guard.message = Some("업데이트 기능이 비활성화되어 있습니다".to_string());
            guard.pending = None;
        }
        Err(e) => {
            warn!("업데이트 확인 실패: {}", e);
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Error;
            guard.message = Some(format!("업데이트 확인 실패: {}", e));
            guard.pending = None;
        }
    }

    if let Err(e) = updater.save_last_check_time() {
        warn!("업데이트 확인 시각 저장 실패: {}", e);
    }
}

async fn apply_pending_update(
    updater: &Updater,
    state: &Arc<RwLock<UpdateStatus>>,
) -> Result<(), UpdateError> {
    let pending = {
        let guard = state.read().await;
        guard.pending.clone()
    };

    let Some(pending) = pending else {
        let mut guard = state.write().await;
        guard.phase = UpdatePhase::Idle;
        guard.message = Some("적용할 대기 중 업데이트가 없습니다".to_string());
        return Ok(());
    };

    {
        let mut guard = state.write().await;
        guard.phase = UpdatePhase::Installing;
        guard.message = Some(format!("{} 버전을 설치 중입니다", pending.latest_version));
    }

    let path = updater.download_update(&pending.download_url).await?;
    match updater.install_and_restart(&path) {
        Ok(()) => {
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Updated;
            guard.message = Some(format!(
                "{} 버전으로 업데이트했습니다",
                pending.latest_version
            ));
            guard.pending = None;
            Ok(())
        }
        Err(e) => {
            error!("업데이트 설치 실패: {}", e);
            let mut guard = state.write().await;
            guard.phase = UpdatePhase::Error;
            guard.message = Some(format!("업데이트 설치 실패: {}", e));
            Err(e)
        }
    }
}
