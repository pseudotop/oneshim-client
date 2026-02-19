//! 라이프사이클 관리.
//!
//! 시작/종료, 시그널 핸들링, 리소스 정리.

use tokio::sync::watch;
use tracing::info;

/// 라이프사이클 관리자
pub struct LifecycleManager {
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl LifecycleManager {
    /// 새 라이프사이클 관리자 생성
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            shutdown_tx: tx,
            shutdown_rx: rx,
        }
    }

    /// 종료 수신기 복제
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.shutdown_rx.clone()
    }

    /// 종료 신호 발송
    pub fn shutdown(&self) {
        info!("종료 신호 발송");
        let _ = self.shutdown_tx.send(true);
    }

    /// OS 시그널 대기 (SIGINT, SIGTERM)
    pub async fn wait_for_signal(&self) {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint = signal(SignalKind::interrupt()).expect("SIGINT 핸들러 등록 실패");
            let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM 핸들러 등록 실패");

            tokio::select! {
                _ = sigint.recv() => {
                    info!("SIGINT 수신");
                }
                _ = sigterm.recv() => {
                    info!("SIGTERM 수신");
                }
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("Ctrl+C 핸들러 등록 실패");
            info!("Ctrl+C 수신");
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
