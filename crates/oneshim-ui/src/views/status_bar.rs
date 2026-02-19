//! 상태바 뷰.

/// 상태바 상태
#[derive(Debug, Clone)]
pub struct StatusBarState {
    /// 연결 상태 아이콘
    pub connection_icon: ConnectionIcon,
    /// 업로드 진행률 (0.0~1.0, None이면 숨김)
    pub upload_progress: Option<f32>,
    /// 큐 대기 이벤트 수
    pub pending_events: usize,
    /// SSE 지연 (밀리초)
    pub sse_latency_ms: Option<f64>,
    /// 자동화 활성화 상태
    pub automation_enabled: bool,
}

/// 연결 상태 아이콘
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
