use std::time::Instant;

use crate::i18n::Locale;
use crate::tray::TrayEvent;

// ---------------------------------------------------------------------------
// CollectedMetrics — 로컬 모니터 수집 결과
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct CollectedMetrics {
    pub agent_cpu: f32,
    pub agent_memory_mb: f64,
    pub system_cpu: f32,
    pub system_memory_used_mb: f64,
    pub system_memory_total_mb: f64,
}

// ---------------------------------------------------------------------------
// Message — iced 앱 전역 메시지
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    ToggleSettings,
    ToggleMetricsView,
    Quit,
    HideWindow(iced::window::Id),
    ShowWindow,
    WindowOpened(iced::window::Id),

    ToggleMonitoring(bool),
    ToggleCapture(bool),
    ToggleNotifications(bool),
    ToggleAutostart(bool),
    ChangeTheme(u8),
    ChangeLanguage(Locale),

    UpdateMetrics(CollectedMetrics),
    UpdateConnectionStatus(String),
    UpdateActiveApp(Option<String>),
    SuggestionReceived(String),

    Tick(Instant),

    Tray(TrayEvent),
}

// ---------------------------------------------------------------------------
// UpdateUserAction — 자동 업데이트 사용자 액션
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateUserAction {
    Approve,
    Defer,
}

// ---------------------------------------------------------------------------
// UpdateStatusSnapshot — 업데이트 상태 스냅샷
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UpdateStatusSnapshot {
    pub phase: String,
    pub message: Option<String>,
    pub pending_latest_version: Option<String>,
    pub auto_install: bool,
}

impl Default for UpdateStatusSnapshot {
    fn default() -> Self {
        Self {
            phase: "Idle".to_string(),
            message: None,
            pending_latest_version: None,
            auto_install: false,
        }
    }
}

impl UpdateStatusSnapshot {
    pub fn is_pending_approval(&self) -> bool {
        self.phase == "PendingApproval"
    }
}

// ---------------------------------------------------------------------------
// Screen — 현재 화면
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Settings,
}

// ---------------------------------------------------------------------------
// MetricsViewMode — 메트릭 표시 모드
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetricsViewMode {
    #[default]
    Simple,
    Detail,
}
