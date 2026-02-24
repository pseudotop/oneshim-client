
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub is_visible: bool,
    pub server_url: String,
    pub monitoring_enabled: bool,
    pub capture_enabled: bool,
    pub notifications_enabled: bool,
    pub theme_mode: u8,
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            is_visible: false,
            server_url: "http://localhost:8000".to_string(),
            monitoring_enabled: true,
            capture_enabled: true,
            notifications_enabled: true,
            theme_mode: 0,
        }
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}
