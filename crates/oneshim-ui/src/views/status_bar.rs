#[derive(Debug, Clone)]
pub struct StatusBarState {
    pub connection_icon: ConnectionIcon,
    pub upload_progress: Option<f32>,
    pub pending_events: usize,
    pub sse_latency_ms: Option<f64>,
    pub automation_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionIcon {
    Connected,
    Connecting,
    Disconnected,
    Error,
}

impl StatusBarState {
    pub fn new() -> Self {
        Self {
            connection_icon: ConnectionIcon::Disconnected,
            upload_progress: None,
            pending_events: 0,
            sse_latency_ms: None,
            automation_enabled: false,
        }
    }
}

impl Default for StatusBarState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state() {
        let state = StatusBarState::new();
        assert_eq!(state.connection_icon, ConnectionIcon::Disconnected);
        assert!(state.upload_progress.is_none());
    }
}
