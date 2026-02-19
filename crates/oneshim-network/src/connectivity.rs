//! 연결 상태 관리.
//!
//! 서버 연결 상태를 자동으로 감지하고 관리.
//! 오프라인/온라인 상태 전환을 자동으로 처리.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, info, warn};

/// 연결 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// 연결됨
    Connected,
    /// 연결 끊김
    Disconnected,
    /// 재연결 시도 중
    Reconnecting,
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Connected => write!(f, "Connected"),
            ConnectionStatus::Disconnected => write!(f, "Disconnected"),
            ConnectionStatus::Reconnecting => write!(f, "Reconnecting"),
        }
    }
}

/// 연결 상태 관리자
///
/// 서버 연결 상태를 추적하고, 오프라인/온라인 모드를 자동으로 전환.
pub struct ConnectivityManager {
    /// 현재 온라인 상태 (atomic for lock-free access)
    is_online: AtomicBool,
    /// 마지막 성공한 연결 시각 (Unix timestamp)
    last_success: AtomicU64,
    /// 연속 실패 횟수
    failure_count: AtomicU64,
    /// 상태 변경 브로드캐스트
    status_tx: watch::Sender<ConnectionStatus>,
    /// 상태 수신기 (복제 가능)
    status_rx: watch::Receiver<ConnectionStatus>,
    /// 오프라인 전환 임계값 (연속 실패 횟수)
    offline_threshold: u64,
    /// 강제 오프라인 모드
    force_offline: AtomicBool,
}

impl ConnectivityManager {
    /// 새 연결 관리자 생성
    ///
    /// `offline_threshold`: 이 횟수만큼 연속 실패하면 오프라인 전환
    pub fn new(offline_threshold: u64) -> Self {
        let (status_tx, status_rx) = watch::channel(ConnectionStatus::Connected);
        Self {
            is_online: AtomicBool::new(true),
            last_success: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
            status_tx,
            status_rx,
            offline_threshold,
            force_offline: AtomicBool::new(false),
        }
    }

    /// 기본 임계값(3회 실패)으로 생성
    pub fn default_threshold() -> Self {
        Self::new(3)
    }

    /// 강제 오프라인 모드 설정
    pub fn set_force_offline(&self, force: bool) {
        self.force_offline.store(force, Ordering::Relaxed);
        if force {
            self.is_online.store(false, Ordering::Relaxed);
            let _ = self.status_tx.send(ConnectionStatus::Disconnected);
            info!("강제 오프라인 모드 활성화");
        }
    }

    /// 강제 오프라인 모드 여부
    pub fn is_force_offline(&self) -> bool {
        self.force_offline.load(Ordering::Relaxed)
    }

    /// 현재 온라인 상태
    pub fn is_online(&self) -> bool {
        !self.is_force_offline() && self.is_online.load(Ordering::Relaxed)
    }

    /// 현재 연결 상태
    pub fn status(&self) -> ConnectionStatus {
        *self.status_rx.borrow()
    }

    /// 상태 변경 수신기 생성
    pub fn subscribe(&self) -> watch::Receiver<ConnectionStatus> {
        self.status_rx.clone()
    }

    /// 연결 성공 기록
    ///
    /// 온라인 상태로 전환하고 실패 카운터 리셋.
    pub fn record_success(&self) {
        if self.is_force_offline() {
            return;
        }

        let was_offline = !self.is_online.load(Ordering::Relaxed);
        self.is_online.store(true, Ordering::Relaxed);
        self.failure_count.store(0, Ordering::Relaxed);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_success.store(now, Ordering::Relaxed);

        if was_offline {
            info!("서버 연결 복구됨 - 온라인 모드");
            let _ = self.status_tx.send(ConnectionStatus::Connected);
        }
    }

    /// 연결 실패 기록
    ///
    /// 임계값 도달 시 오프라인 전환.
    pub fn record_failure(&self) {
        if self.is_force_offline() {
            return;
        }

        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        debug!("연결 실패 기록 (연속 {}회)", count);

        if count >= self.offline_threshold {
            let was_online = self.is_online.swap(false, Ordering::Relaxed);
            if was_online {
                warn!(
                    "연속 {}회 실패 - 오프라인 모드 전환 (대기 이벤트 로컬 저장)",
                    count
                );
                let _ = self.status_tx.send(ConnectionStatus::Disconnected);
            }
        } else {
            let _ = self.status_tx.send(ConnectionStatus::Reconnecting);
        }
    }

    /// 연속 실패 횟수
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// 마지막 성공 연결 이후 경과 시간
    pub fn time_since_last_success(&self) -> Duration {
        let last = self.last_success.load(Ordering::Relaxed);
        if last == 0 {
            return Duration::ZERO;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Duration::from_secs(now.saturating_sub(last))
    }

    /// 연결 상태 통계
    pub fn stats(&self) -> ConnectivityStats {
        ConnectivityStats {
            is_online: self.is_online(),
            status: self.status(),
            failure_count: self.failure_count(),
            time_since_last_success: self.time_since_last_success(),
            force_offline: self.is_force_offline(),
        }
    }
}

impl Default for ConnectivityManager {
    fn default() -> Self {
        Self::default_threshold()
    }
}

/// 연결 상태 통계
#[derive(Debug, Clone)]
pub struct ConnectivityStats {
    /// 현재 온라인 여부
    pub is_online: bool,
    /// 현재 연결 상태
    pub status: ConnectionStatus,
    /// 연속 실패 횟수
    pub failure_count: u64,
    /// 마지막 성공 이후 경과 시간
    pub time_since_last_success: Duration,
    /// 강제 오프라인 모드
    pub force_offline: bool,
}

/// Arc로 감싼 ConnectivityManager
pub type SharedConnectivityManager = Arc<ConnectivityManager>;

/// 새 공유 연결 관리자 생성
pub fn new_shared_connectivity_manager(offline_threshold: u64) -> SharedConnectivityManager {
    Arc::new(ConnectivityManager::new(offline_threshold))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_online() {
        let mgr = ConnectivityManager::default();
        assert!(mgr.is_online());
        assert_eq!(mgr.status(), ConnectionStatus::Connected);
        assert_eq!(mgr.failure_count(), 0);
    }

    #[test]
    fn success_resets_failures() {
        let mgr = ConnectivityManager::new(3);

        mgr.record_failure();
        mgr.record_failure();
        assert_eq!(mgr.failure_count(), 2);

        mgr.record_success();
        assert_eq!(mgr.failure_count(), 0);
        assert!(mgr.is_online());
    }

    #[test]
    fn threshold_triggers_offline() {
        let mgr = ConnectivityManager::new(3);

        mgr.record_failure();
        assert!(mgr.is_online()); // 1회 - 아직 온라인

        mgr.record_failure();
        assert!(mgr.is_online()); // 2회 - 아직 온라인

        mgr.record_failure();
        assert!(!mgr.is_online()); // 3회 - 오프라인!
        assert_eq!(mgr.status(), ConnectionStatus::Disconnected);
    }

    #[test]
    fn recovery_after_offline() {
        let mgr = ConnectivityManager::new(2);

        mgr.record_failure();
        mgr.record_failure();
        assert!(!mgr.is_online()); // 오프라인

        mgr.record_success();
        assert!(mgr.is_online()); // 복구됨
        assert_eq!(mgr.status(), ConnectionStatus::Connected);
    }

    #[test]
    fn force_offline_overrides() {
        let mgr = ConnectivityManager::default();

        mgr.set_force_offline(true);
        assert!(!mgr.is_online());

        mgr.record_success(); // 강제 모드에서는 무시됨
        assert!(!mgr.is_online());

        mgr.set_force_offline(false);
        mgr.record_success();
        assert!(mgr.is_online());
    }

    #[tokio::test]
    async fn subscribe_receives_changes() {
        let mgr = ConnectivityManager::new(1);
        let mut rx = mgr.subscribe();

        // 초기 상태
        assert_eq!(*rx.borrow(), ConnectionStatus::Connected);

        // 실패 → 오프라인
        mgr.record_failure();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), ConnectionStatus::Disconnected);

        // 성공 → 온라인
        mgr.record_success();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), ConnectionStatus::Connected);
    }
}
