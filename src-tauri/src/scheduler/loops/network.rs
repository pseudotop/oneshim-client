use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use super::super::config::PlatformEgressPolicy;
use super::super::Scheduler;

impl Scheduler {
    #[tracing::instrument(skip_all)]
    pub(in crate::scheduler) fn spawn_sync_loop(
        &self,
        sync_interval: Duration,
        egress_policy: Arc<PlatformEgressPolicy>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let uploader4 = self.batch_sink.clone();
        let storage4 = self.storage.clone();
        let frame_storage4 = self.frame_storage.clone();
        let egress4 = egress_policy;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(sync_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Some(ref sink) = uploader4 {
                            if egress4.is_enabled() {
                                match sink.flush().await {
                                    Ok(count) => {
                                        if count > 0 {
                                            debug!("batch: {count}items sent");
                                            if let Err(e) = storage4.mark_unsent_as_sent_before(Utc::now()).await {
                                                warn!("mark sent failure: {e}");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("batch failure: {e}");
                                    }
                                }
                            }
                        }

                        if let Err(e) = storage4.enforce_retention().await {
                            warn!("event policy failure: {e}");
                        }

                        if let Some(ref fs) = frame_storage4 {
                            if let Err(e) = fs.enforce_retention().await {
                                warn!("frame policy failure: {e}");
                            }
                            if let Err(e) = fs.enforce_storage_limit().await {
                                warn!("frame failure: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("ended");
                        break;
                    }
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    pub(in crate::scheduler) fn spawn_heartbeat_loop(
        &self,
        heartbeat_interval: Duration,
        session_id: String,
        egress_policy: Arc<PlatformEgressPolicy>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let api = self.api_client.clone();
        let sid = session_id;

        tokio::spawn(async move {
            let api = match api {
                Some(a) => a,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };

            if !egress_policy.is_enabled() {
                let _ = shutdown_rx.changed().await;
                return;
            }

            let mut interval = tokio::time::interval(heartbeat_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = api.send_heartbeat(&sid).await {
                            warn!("heartbeat failure: {e}");
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("heartbeat ended");
                        break;
                    }
                }
            }
        })
    }
}
