use oneshim_core::config::UpdateConfig;
use oneshim_web::update_control::{UpdateAction, UpdateControl};
use tokio::runtime::Handle;
use tracing::{debug, info};

use crate::update_coordinator;
use crate::updater::{UpdateCheckResult, Updater};

pub(crate) struct UpdateRuntimeBundle {
    pub(crate) update_control: UpdateControl,
    pub(crate) update_action_tx: tokio::sync::mpsc::UnboundedSender<UpdateAction>,
}

pub(crate) struct UpdateRuntimeBuilder<'a> {
    config: &'a UpdateConfig,
    runtime_handle: &'a Handle,
}

impl<'a> UpdateRuntimeBuilder<'a> {
    pub(crate) fn new(config: &'a UpdateConfig, runtime_handle: &'a Handle) -> Self {
        Self {
            config,
            runtime_handle,
        }
    }

    pub(crate) fn build_and_spawn(&self) -> UpdateRuntimeBundle {
        let runtime_auto_update = self.config.auto_install;
        let (update_action_tx, update_action_rx) =
            tokio::sync::mpsc::unbounded_channel::<UpdateAction>();
        let update_control = UpdateControl::new(
            update_action_tx.clone(),
            update_coordinator::initial_status(self.config, runtime_auto_update),
        );

        if self.config.enabled {
            // Fire-and-forget startup update check (non-blocking, 3s timeout)
            let startup_config = self.config.clone();
            self.runtime_handle.spawn(async move {
                let updater = Updater::new(startup_config);
                match tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    updater.check_for_updates(),
                )
                .await
                {
                    Ok(Ok(UpdateCheckResult::Available { latest, .. })) => {
                        info!("startup update check: v{latest} available");
                    }
                    Ok(Ok(UpdateCheckResult::UpToDate { .. })) => {
                        debug!("startup update check: up to date");
                    }
                    _ => {
                        debug!("startup update check: skipped");
                    }
                }
            });

            let update_config = self.config.clone();
            let update_state = update_control.state.clone();
            let update_status_tx = Some(update_control.event_tx.clone());
            self.runtime_handle.spawn(async move {
                update_coordinator::run_update_coordinator(
                    update_config,
                    update_state,
                    update_action_rx,
                    update_status_tx,
                    runtime_auto_update,
                )
                .await;
            });
        }

        UpdateRuntimeBundle {
            update_control,
            update_action_tx,
        }
    }
}
