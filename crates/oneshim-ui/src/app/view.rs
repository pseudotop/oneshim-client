use iced::widget::{button, column, container, row, rule, text, toggler, Column, Space};
use iced::{Alignment, Element, Length};

use super::message::{Message, MetricsViewMode, Screen};
use super::OneshimApp;
use crate::i18n::Locale;
use crate::views::metrics_chart::{cpu_chart, memory_chart};

impl OneshimApp {
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
            Space::new().width(Length::Fill),
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
            rule::horizontal(1),
            connection_status,
            update_status,
            update_message,
            text(active_app_text).size(14),
            rule::horizontal(1),
            metrics,
            rule::horizontal(1),
            suggestions,
            Space::new().height(Length::Fill),
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
            Space::new().width(Length::Fill),
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
            rule::horizontal(1),
            text(s.general).size(18),
            monitoring_toggle,
            capture_toggle,
            notifications_toggle,
            rule::horizontal(1),
            text(s.startup).size(18),
            autostart_toggle,
            rule::horizontal(1),
            text(s.appearance).size(18),
            theme_buttons,
            language_buttons,
            rule::horizontal(1),
            text(s.connection).size(18),
            server_info,
            data_path_info,
            Space::new().height(Length::Fill),
            version_info,
        ]
        .spacing(12)
        .into()
    }
}
