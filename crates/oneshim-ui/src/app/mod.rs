mod message;
mod update;
mod view;

pub use message::{
    CollectedMetrics, Message, MetricsViewMode, Screen, UpdateStatusSnapshot, UpdateUserAction,
};

use directories::ProjectDirs;
use iced::{event, Event, Subscription, Theme};
use oneshim_core::ports::storage::StorageService;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

use crate::autostart;
use crate::i18n::{Locale, Strings};
use crate::theme::AppTheme;
use crate::tray::TrayEvent;
use crate::views::main_window::MainWindowState;
use crate::views::settings::SettingsState;

// ---------------------------------------------------------------------------
// LocalMonitor — 로컬 시스템 메트릭 수집
// ---------------------------------------------------------------------------

pub(super) struct LocalMonitor {
    sys: sysinfo::System,
    pid: sysinfo::Pid,
}

impl LocalMonitor {
    fn new() -> Self {
        let pid = sysinfo::Pid::from_u32(std::process::id());
        let mut sys = sysinfo::System::new_all();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);

        Self { sys, pid }
    }

    fn collect_metrics(&mut self) -> CollectedMetrics {
        self.sys
            .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[self.pid]), true);

        self.sys.refresh_cpu_all();

        self.sys.refresh_memory();

        let (agent_cpu, agent_memory_mb) = if let Some(process) = self.sys.process(self.pid) {
            let cpu = process.cpu_usage(); // CPU (%)
            let memory_mb = process.memory() as f64 / 1024.0 / 1024.0; // MB
            (cpu, memory_mb)
        } else {
            (0.0, 0.0)
        };

        let system_cpu = self.sys.global_cpu_usage();

        let total_memory = self.sys.total_memory(); // bytes
        let used_memory = self.sys.used_memory(); // bytes
        let system_memory_total_mb = total_memory as f64 / 1024.0 / 1024.0;
        let system_memory_used_mb = used_memory as f64 / 1024.0 / 1024.0;

        CollectedMetrics {
            agent_cpu,
            agent_memory_mb,
            system_cpu,
            system_memory_used_mb,
            system_memory_total_mb,
        }
    }
}

// ---------------------------------------------------------------------------
// 유틸리티 함수
// ---------------------------------------------------------------------------

fn get_data_dir() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "oneshim", "agent") {
        proj_dirs.data_dir().to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

pub(super) fn get_active_window_name() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("osascript")
            .args([
                "-e",
                r#"tell application "System Events"
                    set frontApp to name of first application process whose frontmost is true
                    return frontApp
                end tell"#,
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    {
        None
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// OneshimApp — 메인 애플리케이션 구조체
// ---------------------------------------------------------------------------

pub struct OneshimApp {
    pub(super) screen: Screen,
    pub(super) metrics_view_mode: MetricsViewMode,
    pub(super) main_state: MainWindowState,
    pub(super) settings: SettingsState,
    pub(super) autostart_enabled: bool,
    pub(super) theme: AppTheme,
    pub(super) locale: Locale,
    pub(super) recent_suggestions: Vec<String>,
    pub(super) offline_mode: bool,
    // std::sync::Mutex 사용이 올바름: iced update()는 동기 메서드이며,
    // await 지점 없이 짧은 시간 동안만 잠금을 유지함. tokio::sync::Mutex 불필요.
    pub(super) monitor: Mutex<LocalMonitor>,
    pub(super) data_dir: PathBuf,
    pub(super) tray_rx: Option<std::sync::mpsc::Receiver<TrayEvent>>,
    pub(super) window_visible: bool,
    pub(super) window_id: Option<iced::window::Id>,
    pub(super) storage: Option<Arc<dyn StorageService>>,
    pub(super) update_action_tx: Option<std::sync::mpsc::Sender<UpdateUserAction>>,
    pub(super) update_status_rx: Option<std::sync::mpsc::Receiver<UpdateStatusSnapshot>>,
    pub(super) update_status: UpdateStatusSnapshot,
}

impl Default for OneshimApp {
    fn default() -> Self {
        Self::new()
    }
}

impl OneshimApp {
    pub fn new() -> Self {
        let autostart_enabled = autostart::check_autostart_status();
        let locale = Locale::detect_system();
        let data_dir = get_data_dir();

        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            warn!("data create failure: {e}");
        }

        info!(
            "ONESHIM GUI initialize (locale: {}, data_dir: {})",
            locale.code(),
            data_dir.display()
        );

        Self {
            screen: Screen::Dashboard,
            metrics_view_mode: MetricsViewMode::Simple,
            main_state: MainWindowState::new(),
            settings: SettingsState::new(),
            autostart_enabled,
            theme: AppTheme::Dark,
            locale,
            recent_suggestions: Vec::new(),
            offline_mode: false,
            monitor: Mutex::new(LocalMonitor::new()),
            data_dir,
            tray_rx: None,
            window_visible: true,
            window_id: None,
            storage: None,
            update_action_tx: None,
            update_status_rx: None,
            update_status: UpdateStatusSnapshot::default(),
        }
    }

    pub fn with_tray_receiver(mut self, rx: std::sync::mpsc::Receiver<TrayEvent>) -> Self {
        self.tray_rx = Some(rx);
        self
    }

    pub fn with_offline_mode(mut self, offline: bool) -> Self {
        self.offline_mode = offline;
        if offline {
            let s = self.strings();
            self.main_state.connection_status = s.connection_offline.to_string();
        }
        self
    }

    pub fn with_storage(mut self, storage: Arc<dyn StorageService>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn with_update_action_sender(
        mut self,
        tx: std::sync::mpsc::Sender<UpdateUserAction>,
    ) -> Self {
        self.update_action_tx = Some(tx);
        self
    }

    pub fn with_update_status_receiver(
        mut self,
        rx: std::sync::mpsc::Receiver<UpdateStatusSnapshot>,
    ) -> Self {
        self.update_status_rx = Some(rx);
        self
    }

    pub fn with_locale(mut self, locale: Locale) -> Self {
        self.locale = locale;
        self
    }

    pub(super) fn strings(&self) -> &'static Strings {
        Strings::for_locale(self.locale)
    }

    pub fn title(&self) -> String {
        let s = self.strings();
        match self.screen {
            Screen::Dashboard => s.app_title.to_string(),
            Screen::Settings => s.app_title_settings.to_string(),
        }
    }

    pub fn theme(&self) -> Theme {
        match self.theme {
            AppTheme::Dark => Theme::Dark,
            AppTheme::Light => Theme::Light,
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let window_events = event::listen_with(|event, _status, id| match event {
            Event::Window(iced::window::Event::CloseRequested) => Some(Message::HideWindow(id)),
            Event::Window(iced::window::Event::Opened { .. }) => Some(Message::WindowOpened(id)),
            _ => None,
        });

        Subscription::batch([
            // 5초 간격: 데스크톱 모니터링 에이전트에 1초는 과도하게 빈번함
            iced::time::every(Duration::from_secs(5)).map(Message::Tick),
            window_events,
        ])
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_creation() {
        let app = OneshimApp::new();
        assert_eq!(app.screen, Screen::Dashboard);
        assert!(!app.offline_mode);
    }

    #[test]
    fn toggle_settings() {
        let mut app = OneshimApp::new();
        assert_eq!(app.screen, Screen::Dashboard);

        let _ = app.update(Message::ToggleSettings);
        assert_eq!(app.screen, Screen::Settings);

        let _ = app.update(Message::ToggleSettings);
        assert_eq!(app.screen, Screen::Dashboard);
    }

    #[test]
    fn update_metrics() {
        let mut app = OneshimApp::new();
        let metrics = CollectedMetrics {
            agent_cpu: 5.0,
            agent_memory_mb: 128.5,
            system_cpu: 45.0,
            system_memory_used_mb: 8192.0,
            system_memory_total_mb: 16384.0,
        };
        let _ = app.update(Message::UpdateMetrics(metrics));
        assert_eq!(app.main_state.cpu_usage, 5.0);
        assert_eq!(app.main_state.memory_usage_mb, 128.5);
        assert_eq!(app.main_state.system_cpu_usage, 45.0);
        assert_eq!(app.main_state.system_memory_used_mb, 8192.0);
        assert_eq!(app.main_state.system_memory_total_mb, 16384.0);
    }

    #[test]
    fn offline_mode_korean() {
        let app = OneshimApp::new()
            .with_locale(Locale::Ko)
            .with_offline_mode(true);
        assert!(app.offline_mode);
        assert_eq!(app.main_state.connection_status, "오프라인 mode");
    }

    #[test]
    fn offline_mode_english() {
        let app = OneshimApp::new()
            .with_locale(Locale::En)
            .with_offline_mode(true);
        assert!(app.offline_mode);
        assert_eq!(app.main_state.connection_status, "Offline Mode");
    }

    #[test]
    fn suggestion_received() {
        let mut app = OneshimApp::new();
        let _ = app.update(Message::SuggestionReceived("test suggestion".to_string()));
        assert_eq!(app.recent_suggestions.len(), 1);
        assert_eq!(app.recent_suggestions[0], "test suggestion");
    }

    #[test]
    fn suggestion_max_count() {
        let mut app = OneshimApp::new();
        for i in 0..6 {
            let _ = app.update(Message::SuggestionReceived(format!("suggestion {i}")));
        }
        assert_eq!(app.recent_suggestions.len(), 5);
        assert_eq!(app.recent_suggestions[0], "suggestion 5");
    }

    #[test]
    fn toggle_monitoring() {
        let mut app = OneshimApp::new();
        assert!(app.settings.monitoring_enabled);

        let _ = app.update(Message::ToggleMonitoring(false));
        assert!(!app.settings.monitoring_enabled);

        let _ = app.update(Message::ToggleMonitoring(true));
        assert!(app.settings.monitoring_enabled);
    }

    #[test]
    fn toggle_capture() {
        let mut app = OneshimApp::new();
        assert!(app.settings.capture_enabled);

        let _ = app.update(Message::ToggleCapture(false));
        assert!(!app.settings.capture_enabled);
    }

    #[test]
    fn toggle_notifications() {
        let mut app = OneshimApp::new();
        assert!(app.settings.notifications_enabled);

        let _ = app.update(Message::ToggleNotifications(false));
        assert!(!app.settings.notifications_enabled);
    }

    #[test]
    fn change_theme() {
        let mut app = OneshimApp::new();
        assert_eq!(app.settings.theme_mode, 0); // default
        let _ = app.update(Message::ChangeTheme(1));
        assert_eq!(app.settings.theme_mode, 1);
        assert_eq!(app.theme, AppTheme::Light);

        let _ = app.update(Message::ChangeTheme(0));
        assert_eq!(app.theme, AppTheme::Dark);
    }

    #[test]
    fn change_language() {
        let mut app = OneshimApp::new().with_locale(Locale::Ko);
        assert_eq!(app.locale, Locale::Ko);

        let _ = app.update(Message::ChangeLanguage(Locale::En));
        assert_eq!(app.locale, Locale::En);

        let _ = app.update(Message::ChangeLanguage(Locale::Ko));
        assert_eq!(app.locale, Locale::Ko);
    }

    #[test]
    fn title_changes_with_locale() {
        let mut app = OneshimApp::new().with_locale(Locale::Ko);
        assert_eq!(app.title(), "ONESHIM");

        let _ = app.update(Message::ToggleSettings);
        assert_eq!(app.title(), "ONESHIM - 설정");

        let _ = app.update(Message::ChangeLanguage(Locale::En));
        assert_eq!(app.title(), "ONESHIM - Settings");
    }

    #[test]
    fn update_connection_status() {
        let mut app = OneshimApp::new();
        let _ = app.update(Message::UpdateConnectionStatus("connected".to_string()));
        assert_eq!(app.main_state.connection_status, "connected");
    }

    #[test]
    fn update_active_app() {
        let mut app = OneshimApp::new();
        assert!(app.main_state.active_app.is_none());

        let _ = app.update(Message::UpdateActiveApp(Some("Safari".to_string())));
        assert_eq!(app.main_state.active_app, Some("Safari".to_string()));

        let _ = app.update(Message::UpdateActiveApp(None));
        assert!(app.main_state.active_app.is_none());
    }

    #[test]
    fn theme_returns_correct_iced_theme() {
        let mut app = OneshimApp::new();

        let _ = app.update(Message::ChangeTheme(0));
        assert!(matches!(app.theme(), iced::Theme::Dark));

        let _ = app.update(Message::ChangeTheme(1));
        assert!(matches!(app.theme(), iced::Theme::Light));
    }

    #[test]
    fn strings_for_korean() {
        let app = OneshimApp::new().with_locale(Locale::Ko);
        let s = app.strings();
        assert_eq!(s.quit, "ended");
        assert_eq!(s.settings, "[설정]");
    }

    #[test]
    fn strings_for_english() {
        let app = OneshimApp::new().with_locale(Locale::En);
        let s = app.strings();
        assert_eq!(s.quit, "Quit");
        assert_eq!(s.settings, "[Settings]");
    }
}
