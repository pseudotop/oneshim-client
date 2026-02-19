//! 설정 화면 뷰.

/// 설정 화면 상태
#[derive(Debug, Clone)]
pub struct SettingsState {
    /// 표시 여부
    pub is_visible: bool,
    /// 서버 URL
    pub server_url: String,
    /// 모니터링 활성화
    pub monitoring_enabled: bool,
    /// 캡처 활성화
    pub capture_enabled: bool,
    /// 알림 활성화
    pub notifications_enabled: bool,
    /// 테마 모드 (0=다크, 1=라이트)
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
