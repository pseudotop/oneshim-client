#[derive(Debug, Clone)]
pub struct ContextPanelState {
    pub active_app: Option<String>,
    pub window_title: Option<String>,
    pub cpu_usage: f32,
    pub memory_percent: f32,
    pub disk_percent: f32,
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
