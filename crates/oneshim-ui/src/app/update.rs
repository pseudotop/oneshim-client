use iced::Task;
use tracing::{debug, info, warn};

use super::get_active_window_name;
use super::message::{Message, MetricsViewMode, Screen, UpdateUserAction};
use super::OneshimApp;
use crate::autostart;
use crate::theme::AppTheme;
use crate::tray::TrayEvent;

impl OneshimApp {
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
}
