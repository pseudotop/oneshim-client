//!

use oneshim_core::models::suggestion::Suggestion;
use oneshim_core::models::system::SystemMetrics;
use tokio::sync::broadcast;
use tracing::debug;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppEvent {
    SuggestionReceived(Suggestion),
    MetricsUpdated(SystemMetrics),
    ConnectionChanged(ConnectionState),
    BatchUploaded { count: usize },
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
}

pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: AppEvent) {
        debug!("event publish: {:?}", std::mem::discriminant(&event));
        let _ = self.tx.send(event);
    }

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
