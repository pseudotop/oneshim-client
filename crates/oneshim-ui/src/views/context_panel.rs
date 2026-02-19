//! 컨텍스트 패널 뷰.

/// 컨텍스트 패널 상태
#[derive(Debug, Clone)]
pub struct ContextPanelState {
    /// 활성 앱 이름
    pub active_app: Option<String>,
    /// 활성 창 제목
    pub window_title: Option<String>,
    /// CPU 사용률 (%)
    pub cpu_usage: f32,
    /// 메모리 사용률 (%)
    pub memory_percent: f32,
    /// 디스크 사용률 (%)
    pub disk_percent: f32,
    /// 네트워크 연결 상태
    pub network_connected: bool,
}

impl ContextPanelState {
    pub fn new() -> Self {
        Self {
            active_app: None,
            window_title: None,
            cpu_usage: 0.0,
            memory_percent: 0.0,
            disk_percent: 0.0,
            network_connected: false,
        }
    }

    /// 활성 앱 업데이트
    pub fn update_active_app(&mut self, app: &str, title: &str) {
        self.active_app = Some(app.to_string());
        self.window_title = Some(title.to_string());
    }
}

impl Default for ContextPanelState {
    fn default() -> Self {
        Self::new()
    }
}
