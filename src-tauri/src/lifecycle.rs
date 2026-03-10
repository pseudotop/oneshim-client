use tokio::sync::watch;
use tracing::info;

pub struct LifecycleManager {
    shutdown_tx: watch::Sender<bool>,
    #[allow(dead_code)] // used by subscribe()
    shutdown_rx: watch::Receiver<bool>,
}

impl LifecycleManager {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            shutdown_tx: tx,
            shutdown_rx: rx,
        }
    }

    #[allow(dead_code)] // available for direct subscriber wiring
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.shutdown_rx.clone()
    }

    pub fn shutdown(&self) {
        info!("ended sent");
        let _ = self.shutdown_tx.send(true);
    }

    pub async fn wait_for_signal(&self) {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint =
                signal(SignalKind::interrupt()).expect("Failed to register SIGINT handler");
            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");

            tokio::select! {
                _ = sigint.recv() => {
                    info!("SIGINT received");
                }
                _ = sigterm.recv() => {
                    info!("SIGTERM received");
                }
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to register Ctrl+C handler");
            info!("Ctrl+C received");
        }

        self.shutdown();
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_creation() {
        let lm = LifecycleManager::new();
        let rx = lm.subscribe();
        assert!(!*rx.borrow());
    }

    #[test]
    fn shutdown_signal() {
        let lm = LifecycleManager::new();
        let rx = lm.subscribe();
        lm.shutdown();
        assert!(*rx.borrow());
    }
}
