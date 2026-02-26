//!

use directories::ProjectDirs;
use iced::widget::{button, column, container, horizontal_rule, row, text, toggler, Column};
use iced::{event, Alignment, Element, Event, Length, Subscription, Task, Theme};
use oneshim_core::ports::storage::StorageService;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::System;
use tracing::{debug, info, warn};

use crate::autostart;
use crate::i18n::{Locale, Strings};
use crate::theme::AppTheme;
use crate::tray::TrayEvent;
use crate::views::main_window::MainWindowState;
use crate::views::metrics_chart::{cpu_chart, memory_chart};
use crate::views::settings::SettingsState;

struct LocalMonitor {
    sys: System,
    pid: sysinfo::Pid,
}

#[derive(Debug, Clone, Copy)]
pub struct CollectedMetrics {
    pub agent_cpu: f32,
    pub agent_memory_mb: f64,
    pub system_cpu: f32,
    pub system_memory_used_mb: f64,
    pub system_memory_total_mb: f64,
}

impl LocalMonitor {
    fn new() -> Self {
        let pid = sysinfo::Pid::from_u32(std::process::id());
        let mut sys = System::new_all();
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

fn get_data_dir() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "oneshim", "agent") {
        proj_dirs.data_dir().to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

fn get_active_window_name() -> Option<String> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateUserAction {
    Approve,
    Defer,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetricsViewMode {
    #[default]
    Simple,
    Detail,
}

pub struct OneshimApp {
    screen: Screen,
    metrics_view_mode: MetricsViewMode,
    main_state: MainWindowState,
    settings: SettingsState,
    autostart_enabled: bool,
    theme: AppTheme,
    locale: Locale,
    recent_suggestions: Vec<String>,
    offline_mode: bool,
    monitor: Mutex<LocalMonitor>,
    data_dir: PathBuf,
    tray_rx: Option<std::sync::mpsc::Receiver<TrayEvent>>,
    window_visible: bool,
    window_id: Option<iced::window::Id>,
    storage: Option<Arc<dyn StorageService>>,
    update_action_tx: Option<std::sync::mpsc::Sender<UpdateUserAction>>,
    update_status_rx: Option<std::sync::mpsc::Receiver<UpdateStatusSnapshot>>,
    update_status: UpdateStatusSnapshot,
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

    fn strings(&self) -> &'static Strings {
        Strings::for_locale(self.locale)
    }

    pub fn title(&self) -> String {
        let s = self.strings();
        match self.screen {
            Screen::Dashboard => s.app_title.to_string(),
            Screen::Settings => s.app_title_settings.to_string(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleSettings => {
                self.screen = match self.screen {
                    Screen::Dashboard => Screen::Settings,
                    Screen::Settings => Screen::Dashboard,
                };
                debug!("switch: {:?}", self.screen);
            }

            Message::ToggleMetricsView => {
                self.metrics_view_mode = match self.metrics_view_mode {
                    MetricsViewMode::Simple => MetricsViewMode::Detail,
                    MetricsViewMode::Detail => MetricsViewMode::Simple,
                };
                debug!("mode: {:?}", self.metrics_view_mode);
            }

            Message::Quit => {
                info!("app ended request");
                return iced::exit();
            }

            Message::HideWindow(id) => {
                self.window_visible = false;
                self.window_id = Some(id);
                info!("window hide (tray )");

                #[cfg(target_os = "macos")]
                {
                    crate::native_macos::hide_app();
                    return Task::none();
                }

                #[cfg(target_os = "windows")]
                {
                    crate::native_windows::hide_app();
                    return Task::none();
                }

                #[cfg(target_os = "linux")]
                {
                    return iced::window::minimize(id, true);
                }
            }

            Message::ShowWindow => {
                self.window_visible = true;
                info!("window display");

                #[cfg(target_os = "macos")]
                {
                    crate::native_macos::show_app();
                    return Task::none();
                }

                #[cfg(target_os = "windows")]
                {
                    crate::native_windows::show_app();
                    return Task::none();
                }

                #[cfg(target_os = "linux")]
                if let Some(id) = self.window_id {
                    return Task::batch([
                        iced::window::minimize(id, false),
                        iced::window::gain_focus(id),
                    ]);
                }
            }

            Message::WindowOpened(id) => {
                self.window_id = Some(id);
                debug!("ID save: {:?}", id);
            }

            Message::ToggleMonitoring(enabled) => {
                self.settings.monitoring_enabled = enabled;
                debug!("monitoring: {enabled}");
            }

            Message::ToggleCapture(enabled) => {
                self.settings.capture_enabled = enabled;
                debug!("capture: {enabled}");
            }

            Message::ToggleNotifications(enabled) => {
                self.settings.notifications_enabled = enabled;
                debug!("notification: {enabled}");
            }

            Message::ToggleAutostart(enabled) => {
                if enabled {
                    if let Err(e) = autostart::enable_autostart() {
                        debug!("auto-start enabled failure: {e}");
                    } else {
                        self.autostart_enabled = true;
                    }
                } else if let Err(e) = autostart::disable_autostart() {
                    debug!("auto-start disabled failure: {e}");
                } else {
                    self.autostart_enabled = false;
                }
            }

            Message::ChangeTheme(mode) => {
                self.settings.theme_mode = mode;
                self.theme = if mode == 0 {
                    AppTheme::Dark
                } else {
                    AppTheme::Light
                };
                debug!("theme change: {:?}", self.theme);
            }

            Message::ChangeLanguage(locale) => {
                self.locale = locale;
                if self.offline_mode {
                    let s = self.strings();
                    self.main_state.connection_status = s.connection_offline.to_string();
                }
                debug!("language change: {}", locale.code());
            }

            Message::UpdateMetrics(metrics) => {
                self.main_state.update_metrics(
                    metrics.agent_cpu,
                    metrics.agent_memory_mb,
                    metrics.system_cpu,
                    metrics.system_memory_used_mb,
                    metrics.system_memory_total_mb,
                );
            }

            Message::UpdateConnectionStatus(status) => {
                self.main_state.update_connection(&status);
            }

            Message::UpdateActiveApp(app) => {
                self.main_state.active_app = app;
            }

            Message::SuggestionReceived(content) => {
                self.recent_suggestions.insert(0, content);
                if self.recent_suggestions.len() > 5 {
                    self.recent_suggestions.pop();
                }
                self.main_state.recent_suggestion_count = self.recent_suggestions.len();
            }

            Message::Tick(_) => {
                if let Ok(mut monitor) = self.monitor.lock() {
                    let metrics = monitor.collect_metrics();
                    self.main_state.update_metrics(
                        metrics.agent_cpu,
                        metrics.agent_memory_mb,
                        metrics.system_cpu,
                        metrics.system_memory_used_mb,
                        metrics.system_memory_total_mb,
                    );
                }

                self.main_state.active_app = get_active_window_name();

                if let Some(ref rx) = self.update_status_rx {
                    while let Ok(status) = rx.try_recv() {
                        self.update_status = status;
                    }
                }

                if let Some(ref rx) = self.tray_rx {
                    while let Ok(tray_event) = rx.try_recv() {
                        debug!("tray event received (): {:?}", tray_event);
                        match tray_event {
                            TrayEvent::ToggleWindow => {
                                if self.window_visible {
                                    self.window_visible = false;
                                    info!("tray: window hide");

                                    #[cfg(target_os = "macos")]
                                    {
                                        crate::native_macos::hide_app();
                                    }

                                    #[cfg(not(target_os = "macos"))]
                                    if let Some(id) = self.window_id {
                                        return iced::window::minimize(id, true);
                                    }
                                } else {
                                    self.window_visible = true;
                                    info!("tray: window display");

                                    #[cfg(target_os = "macos")]
                                    {
                                        crate::native_macos::show_app();
                                    }

                                    #[cfg(not(target_os = "macos"))]
                                    if let Some(id) = self.window_id {
                                        return Task::batch([
                                            iced::window::minimize(id, false),
                                            iced::window::gain_focus(id),
                                        ]);
                                    }
                                }
                            }
                            TrayEvent::OpenSettings => {
                                self.screen = Screen::Settings;
                                if !self.window_visible {
                                    self.window_visible = true;
                                    info!("tray: settings open + window display");

                                    #[cfg(target_os = "macos")]
                                    {
                                        crate::native_macos::show_app();
                                    }

                                    #[cfg(not(target_os = "macos"))]
                                    if let Some(id) = self.window_id {
                                        return Task::batch([
                                            iced::window::minimize(id, false),
                                            iced::window::gain_focus(id),
                                        ]);
                                    }
                                } else if let Some(id) = self.window_id {
                                    return iced::window::gain_focus(id);
                                }
                            }
                            TrayEvent::ToggleAutomation => {
                                info!("tray: ()");
                            }
                            TrayEvent::ApproveUpdate => {
                                if self.update_status.is_pending_approval() {
                                    if let Some(ref tx) = self.update_action_tx {
                                        if tx.send(UpdateUserAction::Approve).is_err() {
                                            warn!("update approval event sent failure");
                                        }
                                    }
                                } else {
                                    debug!("tray approval request: approval waiting state");
                                }
                            }
                            TrayEvent::DeferUpdate => {
                                if self.update_status.is_pending_approval() {
                                    if let Some(ref tx) = self.update_action_tx {
                                        if tx.send(UpdateUserAction::Defer).is_err() {
                                            warn!("update defer event sent failure");
                                        }
                                    }
                                } else {
                                    debug!("tray defer request: approval waiting state");
                                }
                            }
                            TrayEvent::Quit => {
                                info!("tray ended request");
                                return iced::exit();
                            }
                        }
                    }
                }
            }

            Message::Tray(tray_event) => {
                debug!("tray event received: {:?}", tray_event);
                match tray_event {
                    TrayEvent::ToggleWindow => {
                        if self.window_visible {
                            self.window_visible = false;
                            info!("Tray message: window hide");

                            #[cfg(target_os = "macos")]
                            {
                                crate::native_macos::hide_app();
                                return Task::none();
                            }

                            #[cfg(target_os = "windows")]
                            {
                                crate::native_windows::hide_app();
                                return Task::none();
                            }

                            #[cfg(target_os = "linux")]
                            if let Some(id) = self.window_id {
                                return iced::window::minimize(id, true);
                            }
                        } else {
                            self.window_visible = true;
                            info!("Tray message: window display");

                            #[cfg(target_os = "macos")]
                            {
                                crate::native_macos::show_app();
                                return Task::none();
                            }

                            #[cfg(target_os = "windows")]
                            {
                                crate::native_windows::show_app();
                                return Task::none();
                            }

                            #[cfg(target_os = "linux")]
                            if let Some(id) = self.window_id {
                                return Task::batch([
                                    iced::window::minimize(id, false),
                                    iced::window::gain_focus(id),
                                ]);
                            }
                        }
                    }
                    TrayEvent::OpenSettings => {
                        self.screen = Screen::Settings;
                        if !self.window_visible {
                            self.window_visible = true;
                            info!("Tray message: settings open + window display");

                            #[cfg(target_os = "macos")]
                            {
                                crate::native_macos::show_app();
                                return Task::none();
                            }

                            #[cfg(target_os = "windows")]
                            {
                                crate::native_windows::show_app();
                                return Task::none();
                            }

                            #[cfg(target_os = "linux")]
                            if let Some(id) = self.window_id {
                                return Task::batch([
                                    iced::window::minimize(id, false),
                                    iced::window::gain_focus(id),
                                ]);
                            }
                        } else if let Some(id) = self.window_id {
                            return iced::window::gain_focus(id);
                        }
                    }
                    TrayEvent::ToggleAutomation => {
                        info!("Tray message:");
                    }
                    TrayEvent::ApproveUpdate => {
                        if self.update_status.is_pending_approval() {
                            if let Some(ref tx) = self.update_action_tx {
                                if tx.send(UpdateUserAction::Approve).is_err() {
                                    warn!("update approval event sent failure");
                                }
                            }
                        } else {
                            debug!("tray approval request: approval waiting state");
                        }
                    }
                    TrayEvent::DeferUpdate => {
                        if self.update_status.is_pending_approval() {
                            if let Some(ref tx) = self.update_action_tx {
                                if tx.send(UpdateUserAction::Defer).is_err() {
                                    warn!("update defer event sent failure");
                                }
                            }
                        } else {
                            debug!("tray defer request: approval waiting state");
                        }
                    }
                    TrayEvent::Quit => {
                        info!("tray ended request");
                        return iced::exit();
                    }
                }
            }
        }

        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = match self.screen {
            Screen::Dashboard => self.view_dashboard(),
            Screen::Settings => self.view_settings(),
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .into()
    }

    fn view_dashboard(&self) -> Element<'_, Message> {
        let s = self.strings();

        let header = row![
            text(s.app_title).size(28),
            iced::widget::horizontal_space(),
            button(text(s.settings)).on_press(Message::ToggleSettings),
        ]
        .align_y(Alignment::Center)
        .spacing(10);

        let status_icon = if self.offline_mode { "[x]" } else { "[*]" };
        let connection_status = row![
            text(status_icon).size(14),
            text(&self.main_state.connection_status).size(14),
        ]
        .spacing(8);

        let update_phase_text = match self.update_status.phase.as_str() {
            "PendingApproval" => s.update_pending,
            "Installing" => s.update_installing,
            "Updated" => s.update_updated,
            "Deferred" => s.update_deferred,
            "Error" => s.update_error,
            _ => "-",
        };

        let update_status = row![
            text(format!("{}:", s.update_status)).size(14),
            text(update_phase_text).size(14),
            text(
                self.update_status
                    .pending_latest_version
                    .clone()
                    .unwrap_or_default(),
            )
            .size(12),
        ]
        .spacing(8);

        let update_message = text(
            self.update_status
                .message
                .clone()
                .unwrap_or_else(|| "-".to_string()),
        )
        .size(12);

        let metrics = self.view_metrics_panel();

        let active_app_text = match &self.main_state.active_app {
            Some(app) => format!("{}: {app}", s.active_app),
            None => format!("{}: -", s.active_app),
        };

        let suggestions = self.view_suggestions_panel();

        column![
            header,
            horizontal_rule(1),
            connection_status,
            update_status,
            update_message,
            text(active_app_text).size(14),
            horizontal_rule(1),
            metrics,
            horizontal_rule(1),
            suggestions,
            iced::widget::vertical_space(),
            button(text(s.quit)).on_press(Message::Quit),
        ]
        .spacing(15)
        .into()
    }

    fn view_metrics_panel(&self) -> Element<'_, Message> {
        let s = self.strings();

        let format_memory_gb = |mb: f64| -> String {
            if mb >= 1024.0 {
                format!("{:.1} GB", mb / 1024.0)
            } else {
                format!("{:.0} MB", mb)
            }
        };

        let toggle_label = match self.metrics_view_mode {
            MetricsViewMode::Simple => s.detail_view,
            MetricsViewMode::Detail => s.simple_view,
        };
        let header = row![
            text(s.system_metrics).size(16),
            iced::widget::horizontal_space(),
            button(text(toggle_label).size(11)).on_press(Message::ToggleMetricsView),
        ]
        .align_y(Alignment::Center);

        match self.metrics_view_mode {
            MetricsViewMode::Simple => column![
                header,
                row![
                    text(s.cpu).size(12).width(Length::Fixed(60.0)),
                    text(format!(
                        "{}: {:.1}%  /  {}: {:.1}%",
                        s.agent,
                        self.main_state.cpu_usage,
                        s.system,
                        self.main_state.system_cpu_usage
                    ))
                    .size(12),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    text(s.memory).size(12).width(Length::Fixed(60.0)),
                    text(format!(
                        "{}: {:.1} MB  /  {}: {} / {}",
                        s.agent,
                        self.main_state.memory_usage_mb,
                        s.system,
                        format_memory_gb(self.main_state.system_memory_used_mb),
                        format_memory_gb(self.main_state.system_memory_total_mb)
                    ))
                    .size(12),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ]
            .spacing(8)
            .into(),
            MetricsViewMode::Detail => column![
                header,
                row![
                    text(s.cpu).size(12).width(Length::Fixed(60.0)),
                    text(format!(
                        "{}: {:.1}%  /  {}: {:.1}%",
                        s.agent,
                        self.main_state.cpu_usage,
                        s.system,
                        self.main_state.system_cpu_usage
                    ))
                    .size(12),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                cpu_chart(self.main_state.cpu_history_slice()),
                row![
                    text(s.memory).size(12).width(Length::Fixed(60.0)),
                    text(format!(
                        "{}: {:.1} MB  /  {}: {} / {}",
                        s.agent,
                        self.main_state.memory_usage_mb,
                        s.system,
                        format_memory_gb(self.main_state.system_memory_used_mb),
                        format_memory_gb(self.main_state.system_memory_total_mb)
                    ))
                    .size(12),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                memory_chart(self.main_state.memory_history_slice()),
            ]
            .spacing(8)
            .into(),
        }
    }

    fn view_suggestions_panel(&self) -> Element<'_, Message> {
        let s = self.strings();
        let mut items = Column::new().spacing(5);

        if self.recent_suggestions.is_empty() {
            items = items.push(text(s.no_suggestions).size(12));
        } else {
            for (i, suggestion) in self.recent_suggestions.iter().take(5).enumerate() {
                let display = if suggestion.len() > 50 {
                    format!("{}. {}...", i + 1, &suggestion[..50])
                } else {
                    format!("{}. {}", i + 1, suggestion)
                };
                items = items.push(text(display).size(12));
            }
        }

        column![text(s.recent_suggestions).size(16), items,]
            .spacing(8)
            .into()
    }

    fn view_settings(&self) -> Element<'_, Message> {
        let s = self.strings();

        let header = row![
            button(text(s.back)).on_press(Message::ToggleSettings),
            text(s.settings.trim_matches(|c| c == '[' || c == ']')).size(24),
        ]
        .spacing(15)
        .align_y(Alignment::Center);

        let monitoring_toggle = row![
            text(s.system_monitoring)
                .size(14)
                .width(Length::Fixed(150.0)),
            toggler(self.settings.monitoring_enabled)
                .on_toggle(Message::ToggleMonitoring)
                .width(Length::Shrink),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let capture_toggle = row![
            text(s.screenshot_capture)
                .size(14)
                .width(Length::Fixed(150.0)),
            toggler(self.settings.capture_enabled)
                .on_toggle(Message::ToggleCapture)
                .width(Length::Shrink),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let notifications_toggle = row![
            text(s.desktop_notifications)
                .size(14)
                .width(Length::Fixed(150.0)),
            toggler(self.settings.notifications_enabled)
                .on_toggle(Message::ToggleNotifications)
                .width(Length::Shrink),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let autostart_toggle = row![
            text(s.auto_start).size(14).width(Length::Fixed(150.0)),
            toggler(self.autostart_enabled)
                .on_toggle(Message::ToggleAutostart)
                .width(Length::Shrink),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let theme_buttons = row![
            text(s.theme).size(14).width(Length::Fixed(150.0)),
            button(text(s.theme_dark))
                .on_press(Message::ChangeTheme(0))
                .style(if self.settings.theme_mode == 0 {
                    button::primary
                } else {
                    button::secondary
                }),
            button(text(s.theme_light))
                .on_press(Message::ChangeTheme(1))
                .style(if self.settings.theme_mode == 1 {
                    button::primary
                } else {
                    button::secondary
                }),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let language_buttons = row![
            text(s.language).size(14).width(Length::Fixed(150.0)),
            button(text(Locale::Ko.name()))
                .on_press(Message::ChangeLanguage(Locale::Ko))
                .style(if self.locale == Locale::Ko {
                    button::primary
                } else {
                    button::secondary
                }),
            button(text(Locale::En.name()))
                .on_press(Message::ChangeLanguage(Locale::En))
                .style(if self.locale == Locale::En {
                    button::primary
                } else {
                    button::secondary
                }),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let server_info = column![
            text(s.server_url).size(14),
            text(&self.settings.server_url).size(12),
        ]
        .spacing(5);

        let data_path_info = column![
            text(s.data_path).size(14),
            text(self.data_dir.display().to_string()).size(11),
        ]
        .spacing(5);

        let version_info = text(format!("{}: {}", s.version, env!("CARGO_PKG_VERSION"))).size(12);

        column![
            header,
            horizontal_rule(1),
            text(s.general).size(18),
            monitoring_toggle,
            capture_toggle,
            notifications_toggle,
            horizontal_rule(1),
            text(s.startup).size(18),
            autostart_toggle,
            horizontal_rule(1),
            text(s.appearance).size(18),
            theme_buttons,
            language_buttons,
            horizontal_rule(1),
            text(s.connection).size(18),
            server_info,
            data_path_info,
            iced::widget::vertical_space(),
            version_info,
        ]
        .spacing(12)
        .into()
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
            iced::time::every(Duration::from_secs(1)).map(Message::Tick),
            window_events,
        ])
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }
}

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
