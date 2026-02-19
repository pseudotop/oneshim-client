//! 내부 이벤트 버스.
//!
//! `tokio::broadcast` 기반 내부 이벤트 라우팅.

use oneshim_core::models::suggestion::Suggestion;
use oneshim_core::models::system::SystemMetrics;
use tokio::sync::broadcast;
use tracing::debug;

/// 내부 앱 이벤트
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppEvent {
    /// 새 제안 수신
    SuggestionReceived(Suggestion),
    /// 시스템 메트릭 업데이트
    MetricsUpdated(SystemMetrics),
    /// 연결 상태 변경
    ConnectionChanged(ConnectionState),
    /// 배치 업로드 완료
    BatchUploaded { count: usize },
    /// 에러 발생
    Error(String),
}

/// 연결 상태
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
}

/// 내부 이벤트 버스
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl EventBus {
    /// 새 이벤트 버스 생성
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// 이벤트 발행
    pub fn publish(&self, event: AppEvent) {
        debug!("이벤트 발행: {:?}", std::mem::discriminant(&event));
        let _ = self.tx.send(event);
    }

    /// 구독자 생성
    #[allow(dead_code)]
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_and_receive() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::ConnectionChanged(ConnectionState::Connected));

        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event,
            AppEvent::ConnectionChanged(ConnectionState::Connected)
        ));
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(AppEvent::Error("test".to_string()));

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert!(matches!(e1, AppEvent::Error(_)));
        assert!(matches!(e2, AppEvent::Error(_)));
    }
}
