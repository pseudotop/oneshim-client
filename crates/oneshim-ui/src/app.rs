//! ONESHIM 메인 애플리케이션.
//!
//! iced 0.13 기반 GUI 애플리케이션.
//! 메인 윈도우, 설정 화면, 시스템 메트릭 표시.
//! i18n 지원 (한국어/영어).
//! 로컬 시스템 모니터링 통합.

use directories::ProjectDirs;
use iced::widget::{button, column, container, horizontal_rule, row, text, toggler, Column};
use iced::{event, Alignment, Element, Event, Length, Subscription, Task, Theme};
use oneshim_storage::sqlite::SqliteStorage;
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

/// 로컬 프로세스 + 시스템 모니터 (sysinfo 기반)
/// ONESHIM 클라이언트 자체의 리소스 사용량 + 시스템 전체 사용량 추적
struct LocalMonitor {
    sys: System,
    pid: sysinfo::Pid,
}

/// 수집된 메트릭 (에이전트 + 시스템)
#[derive(Debug, Clone, Copy)]
pub struct CollectedMetrics {
    /// 에이전트 CPU 사용률 (%)
    pub agent_cpu: f32,
    /// 에이전트 메모리 사용량 (MB)
    pub agent_memory_mb: f64,
    /// 시스템 전체 CPU 사용률 (%)
    pub system_cpu: f32,
    /// 시스템 사용 메모리 (MB)
    pub system_memory_used_mb: f64,
    /// 시스템 전체 메모리 (MB)
    pub system_memory_total_mb: f64,
}

impl LocalMonitor {
    fn new() -> Self {
        let pid = sysinfo::Pid::from_u32(std::process::id());
        let mut sys = System::new_all(); // 시스템 전체 정보 로드
                                         // 초기 프로세스 정보 로드
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);

        Self { sys, pid }
    }

    /// 현재 프로세스 + 시스템 전체 메트릭 수집
    fn collect_metrics(&mut self) -> CollectedMetrics {
        // 프로세스 정보 갱신
        self.sys
            .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[self.pid]), true);

        // 시스템 CPU 갱신
        self.sys.refresh_cpu_all();

        // 시스템 메모리 갱신
        self.sys.refresh_memory();

        // 에이전트 프로세스 메트릭
        let (agent_cpu, agent_memory_mb) = if let Some(process) = self.sys.process(self.pid) {
            let cpu = process.cpu_usage(); // CPU 사용률 (%)
            let memory_mb = process.memory() as f64 / 1024.0 / 1024.0; // 메모리 (MB)
            (cpu, memory_mb)
        } else {
            (0.0, 0.0)
        };

        // 시스템 전체 CPU 사용률
        let system_cpu = self.sys.global_cpu_usage();

        // 시스템 메모리 (bytes → MB)
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

/// 데이터 디렉토리 경로 결정 (플랫폼별)
fn get_data_dir() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "oneshim", "agent") {
        proj_dirs.data_dir().to_path_buf()
    } else {
        // 폴백: 현재 디렉토리
        PathBuf::from(".")
    }
}

/// 활성 창 정보 가져오기 (플랫폼별)
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
        // Windows: 간단한 구현 (향후 Win32 API 사용)
        None
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// 앱 메시지 (사용자 액션 및 이벤트)
#[derive(Debug, Clone)]
pub enum Message {
    // 네비게이션
    /// 설정 화면 열기/닫기
    ToggleSettings,
    /// 메트릭 뷰 모드 토글 (Simple ↔ Detail)
    ToggleMetricsView,
    /// 앱 종료
    Quit,
    /// 창 숨기기 (X 버튼 클릭 시) - window ID 포함
    HideWindow(iced::window::Id),
    /// 창 표시
    ShowWindow,
    /// 창 ID 저장 (초기화용)
    WindowOpened(iced::window::Id),

    // 설정 변경
    /// 모니터링 토글
    ToggleMonitoring(bool),
    /// 캡처 토글
    ToggleCapture(bool),
    /// 알림 토글
    ToggleNotifications(bool),
    /// 자동 시작 토글
    ToggleAutostart(bool),
    /// 테마 변경 (0=다크, 1=라이트)
    ChangeTheme(u8),
    /// 언어 변경
    ChangeLanguage(Locale),

    // 시스템 이벤트
    /// 메트릭 업데이트 (에이전트 + 시스템)
    UpdateMetrics(CollectedMetrics),
    /// 연결 상태 변경
    UpdateConnectionStatus(String),
    /// 활성 앱 변경
    UpdateActiveApp(Option<String>),
    /// 제안 수신
    SuggestionReceived(String),

    // 주기적 업데이트
    /// 틱 (1초마다)
    Tick(Instant),

    // 트레이 이벤트
    /// 시스템 트레이 이벤트
    Tray(TrayEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateUserAction {
    Approve,
    Defer,
}

/// 앱 화면 (현재 표시 중인 뷰)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// 메인 대시보드
    Dashboard,
    /// 설정 화면
    Settings,
}

/// 메트릭 뷰 모드
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetricsViewMode {
    /// 간단 뷰 (기본) - 숫자만
    #[default]
    Simple,
    /// 상세 뷰 - 차트 + 프로그레스 바 포함
    Detail,
}

/// ONESHIM 메인 애플리케이션
pub struct OneshimApp {
    /// 현재 화면
    screen: Screen,
    /// 메트릭 뷰 모드 (Simple/Detail)
    metrics_view_mode: MetricsViewMode,
    /// 메인 윈도우 상태
    main_state: MainWindowState,
    /// 설정 상태
    settings: SettingsState,
    /// 자동 시작 활성화 여부
    autostart_enabled: bool,
    /// 앱 테마
    theme: AppTheme,
    /// 현재 로케일
    locale: Locale,
    /// 최근 제안 목록
    recent_suggestions: Vec<String>,
    /// 오프라인 모드
    offline_mode: bool,
    /// 로컬 시스템 모니터
    monitor: Mutex<LocalMonitor>,
    /// 데이터 디렉토리 경로
    data_dir: PathBuf,
    /// 트레이 이벤트 수신 채널 (옵션)
    tray_rx: Option<std::sync::mpsc::Receiver<TrayEvent>>,
    /// 창 표시 상태 (트레이 최소화용)
    window_visible: bool,
    /// 메인 윈도우 ID (최소화/복원용)
    window_id: Option<iced::window::Id>,
    /// SQLite 저장소 (Agent와 공유, 타임라인 조회용)
    storage: Option<Arc<SqliteStorage>>,
    update_action_tx: Option<std::sync::mpsc::Sender<UpdateUserAction>>,
}

impl Default for OneshimApp {
    fn default() -> Self {
        Self::new()
    }
}

impl OneshimApp {
    /// 새 앱 인스턴스 생성
    pub fn new() -> Self {
        // 자동 시작 상태 확인
        let autostart_enabled = autostart::check_autostart_status();
        // 시스템 로케일 감지
        let locale = Locale::detect_system();
        // 데이터 디렉토리 결정
        let data_dir = get_data_dir();

        // 데이터 디렉토리 생성 (없으면)
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            warn!("데이터 디렉토리 생성 실패: {e}");
        }

        info!(
            "ONESHIM GUI 초기화 (locale: {}, data_dir: {})",
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
        }
    }

    /// 트레이 이벤트 수신 채널 설정
    pub fn with_tray_receiver(mut self, rx: std::sync::mpsc::Receiver<TrayEvent>) -> Self {
        self.tray_rx = Some(rx);
        self
    }

    /// 오프라인 모드 설정
    pub fn with_offline_mode(mut self, offline: bool) -> Self {
        self.offline_mode = offline;
        if offline {
            let s = self.strings();
            self.main_state.connection_status = s.connection_offline.to_string();
        }
        self
    }

    /// SQLite 저장소 설정 (Agent와 공유)
    pub fn with_storage(mut self, storage: Arc<SqliteStorage>) -> Self {
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

    /// 로케일 설정
    pub fn with_locale(mut self, locale: Locale) -> Self {
        self.locale = locale;
        self
    }

    /// 현재 문자열 반환
    fn strings(&self) -> &'static Strings {
        Strings::for_locale(self.locale)
    }

    /// 앱 제목
    pub fn title(&self) -> String {
        let s = self.strings();
        match self.screen {
            Screen::Dashboard => s.app_title.to_string(),
            Screen::Settings => s.app_title_settings.to_string(),
        }
    }

    /// 메시지 처리
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleSettings => {
                self.screen = match self.screen {
                    Screen::Dashboard => Screen::Settings,
                    Screen::Settings => Screen::Dashboard,
                };
                debug!("화면 전환: {:?}", self.screen);
            }

            Message::ToggleMetricsView => {
                self.metrics_view_mode = match self.metrics_view_mode {
                    MetricsViewMode::Simple => MetricsViewMode::Detail,
                    MetricsViewMode::Detail => MetricsViewMode::Simple,
                };
                debug!("메트릭 뷰 모드: {:?}", self.metrics_view_mode);
            }

            Message::Quit => {
                info!("앱 종료 요청");
                return iced::exit();
            }

            Message::HideWindow(id) => {
                // 창 숨기기 - X 버튼 클릭 시 트레이로 이동
                self.window_visible = false;
                self.window_id = Some(id);
                info!("창 숨기기 (트레이로 이동)");

                // macOS: 네이티브 API로 앱 숨기기 (Dock에서도 안 보임)
                #[cfg(target_os = "macos")]
                {
                    crate::native_macos::hide_app();
                    return Task::none();
                }

                // Windows: 네이티브 API로 앱 숨기기 (작업 표시줄에서도 안 보임)
                #[cfg(target_os = "windows")]
                {
                    crate::native_windows::hide_app();
                    return Task::none();
                }

                // Linux: 창 최소화 (X11/Wayland 분열로 네이티브 API 미지원)
                #[cfg(target_os = "linux")]
                {
                    return iced::window::minimize(id, true);
                }
            }

            Message::ShowWindow => {
                // 창 표시 - 트레이에서 "창 보기" 클릭 시
                self.window_visible = true;
                info!("창 표시");

                // macOS: 네이티브 API로 앱 표시
                #[cfg(target_os = "macos")]
                {
                    crate::native_macos::show_app();
                    return Task::none();
                }

                // Windows: 네이티브 API로 앱 표시
                #[cfg(target_os = "windows")]
                {
                    crate::native_windows::show_app();
                    return Task::none();
                }

                // Linux: 창 복원 + 포커스
                #[cfg(target_os = "linux")]
                if let Some(id) = self.window_id {
                    return Task::batch([
                        iced::window::minimize(id, false),
                        iced::window::gain_focus(id),
                    ]);
                }
            }

            Message::WindowOpened(id) => {
                // 창 ID 저장
                self.window_id = Some(id);
                debug!("윈도우 ID 저장: {:?}", id);
            }

            Message::ToggleMonitoring(enabled) => {
                self.settings.monitoring_enabled = enabled;
                debug!("모니터링: {enabled}");
            }

            Message::ToggleCapture(enabled) => {
                self.settings.capture_enabled = enabled;
                debug!("캡처: {enabled}");
            }

            Message::ToggleNotifications(enabled) => {
                self.settings.notifications_enabled = enabled;
                debug!("알림: {enabled}");
            }

            Message::ToggleAutostart(enabled) => {
                if enabled {
                    if let Err(e) = autostart::enable_autostart() {
                        debug!("자동 시작 활성화 실패: {e}");
                    } else {
                        self.autostart_enabled = true;
                    }
                } else if let Err(e) = autostart::disable_autostart() {
                    debug!("자동 시작 비활성화 실패: {e}");
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
                debug!("테마 변경: {:?}", self.theme);
            }

            Message::ChangeLanguage(locale) => {
                self.locale = locale;
                // 오프라인 모드일 때 연결 상태 텍스트 업데이트
                if self.offline_mode {
                    let s = self.strings();
                    self.main_state.connection_status = s.connection_offline.to_string();
                }
                debug!("언어 변경: {}", locale.code());
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
                // 시스템 + 에이전트 메트릭 수집
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

                // 활성 창 업데이트
                self.main_state.active_app = get_active_window_name();

                // 트레이 이벤트 폴링 (논블로킹)
                if let Some(ref rx) = self.tray_rx {
                    while let Ok(tray_event) = rx.try_recv() {
                        debug!("트레이 이벤트 수신 (폴링): {:?}", tray_event);
                        match tray_event {
                            TrayEvent::ToggleWindow => {
                                // 창 표시/숨기기 토글
                                if self.window_visible {
                                    // 창 숨기기
                                    self.window_visible = false;
                                    info!("트레이: 창 숨기기");

                                    #[cfg(target_os = "macos")]
                                    {
                                        crate::native_macos::hide_app();
                                    }

                                    #[cfg(not(target_os = "macos"))]
                                    if let Some(id) = self.window_id {
                                        return iced::window::minimize(id, true);
                                    }
                                } else {
                                    // 창 표시
                                    self.window_visible = true;
                                    info!("트레이: 창 표시");

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
                                // 설정 열기 시 창도 표시
                                if !self.window_visible {
                                    self.window_visible = true;
                                    info!("트레이: 설정 열기 + 창 표시");

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
                                info!("트레이: 자동화 토글 (폴링)");
                                // 자동화 토글은 웹 대시보드/설정에서 처리
                            }
                            TrayEvent::ApproveUpdate => {
                                if let Some(ref tx) = self.update_action_tx {
                                    if tx.send(UpdateUserAction::Approve).is_err() {
                                        warn!("업데이트 승인 이벤트 전송 실패");
                                    }
                                }
                            }
                            TrayEvent::DeferUpdate => {
                                if let Some(ref tx) = self.update_action_tx {
                                    if tx.send(UpdateUserAction::Defer).is_err() {
                                        warn!("업데이트 연기 이벤트 전송 실패");
                                    }
                                }
                            }
                            TrayEvent::Quit => {
                                info!("트레이에서 종료 요청");
                                return iced::exit();
                            }
                        }
                    }
                }
            }

            Message::Tray(tray_event) => {
                debug!("트레이 이벤트 수신: {:?}", tray_event);
                match tray_event {
                    TrayEvent::ToggleWindow => {
                        // 창 표시/숨기기 토글
                        if self.window_visible {
                            self.window_visible = false;
                            info!("Tray 메시지: 창 숨기기");

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
                            info!("Tray 메시지: 창 표시");

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
                        // 설정 열기 시 창도 표시
                        if !self.window_visible {
                            self.window_visible = true;
                            info!("Tray 메시지: 설정 열기 + 창 표시");

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
                        info!("Tray 메시지: 자동화 토글");
                        // 자동화 토글은 웹 대시보드/설정에서 처리
                    }
                    TrayEvent::ApproveUpdate => {
                        if let Some(ref tx) = self.update_action_tx {
                            if tx.send(UpdateUserAction::Approve).is_err() {
                                warn!("업데이트 승인 이벤트 전송 실패");
                            }
                        }
                    }
                    TrayEvent::DeferUpdate => {
                        if let Some(ref tx) = self.update_action_tx {
                            if tx.send(UpdateUserAction::Defer).is_err() {
                                warn!("업데이트 연기 이벤트 전송 실패");
                            }
                        }
                    }
                    TrayEvent::Quit => {
                        info!("트레이에서 종료 요청");
                        return iced::exit();
                    }
                }
            }
        }

        Task::none()
    }

    /// UI 렌더링
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

    /// 대시보드 화면
    fn view_dashboard(&self) -> Element<'_, Message> {
        let s = self.strings();

        // 헤더
        let header = row![
            text(s.app_title).size(28),
            iced::widget::horizontal_space(),
            button(text(s.settings)).on_press(Message::ToggleSettings),
        ]
        .align_y(Alignment::Center)
        .spacing(10);

        // 연결 상태
        let status_icon = if self.offline_mode { "[x]" } else { "[*]" };
        let connection_status = row![
            text(status_icon).size(14),
            text(&self.main_state.connection_status).size(14),
        ]
        .spacing(8);

        // 시스템 메트릭
        let metrics = self.view_metrics_panel();

        // 활성 앱
        let active_app_text = match &self.main_state.active_app {
            Some(app) => format!("{}: {app}", s.active_app),
            None => format!("{}: -", s.active_app),
        };

        // 최근 제안
        let suggestions = self.view_suggestions_panel();

        column![
            header,
            horizontal_rule(1),
            connection_status,
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

    /// 프로세스 메트릭 패널 (에이전트 vs 시스템 비교)
    fn view_metrics_panel(&self) -> Element<'_, Message> {
        let s = self.strings();

        // 메모리를 GB로 포맷 (시스템용)
        let format_memory_gb = |mb: f64| -> String {
            if mb >= 1024.0 {
                format!("{:.1} GB", mb / 1024.0)
            } else {
                format!("{:.0} MB", mb)
            }
        };

        // 헤더 (제목 + 토글 버튼)
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
            MetricsViewMode::Simple => {
                // ── 간단 뷰: 숫자만 표시 ──
                column![
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
                .into()
            }
            MetricsViewMode::Detail => {
                // ── 상세 뷰: 차트 포함 ──
                column![
                    header,
                    // CPU 섹션
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
                    // CPU 시계열 차트
                    cpu_chart(self.main_state.cpu_history_slice()),
                    // 메모리 섹션
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
                    // 메모리 시계열 차트
                    memory_chart(self.main_state.memory_history_slice()),
                ]
                .spacing(8)
                .into()
            }
        }
    }

    /// 최근 제안 패널
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

    /// 설정 화면
    fn view_settings(&self) -> Element<'_, Message> {
        let s = self.strings();

        // 헤더
        let header = row![
            button(text(s.back)).on_press(Message::ToggleSettings),
            text(s.settings.trim_matches(|c| c == '[' || c == ']')).size(24),
        ]
        .spacing(15)
        .align_y(Alignment::Center);

        // 모니터링 설정
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

        // 캡처 설정
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

        // 알림 설정
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

        // 자동 시작 설정
        let autostart_toggle = row![
            text(s.auto_start).size(14).width(Length::Fixed(150.0)),
            toggler(self.autostart_enabled)
                .on_toggle(Message::ToggleAutostart)
                .width(Length::Shrink),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        // 테마 설정
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

        // 언어 설정
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

        // 서버 정보
        let server_info = column![
            text(s.server_url).size(14),
            text(&self.settings.server_url).size(12),
        ]
        .spacing(5);

        // 데이터 경로 정보
        let data_path_info = column![
            text(s.data_path).size(14),
            text(self.data_dir.display().to_string()).size(11),
        ]
        .spacing(5);

        // 버전 정보
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

    /// 테마 반환
    pub fn theme(&self) -> Theme {
        match self.theme {
            AppTheme::Dark => Theme::Dark,
            AppTheme::Light => Theme::Light,
        }
    }

    /// 구독 (주기적 이벤트 + 윈도우 이벤트)
    pub fn subscription(&self) -> Subscription<Message> {
        // 윈도우 이벤트 핸들링
        let window_events = event::listen_with(|event, _status, id| match event {
            // X 버튼 클릭 시 창 숨기기 (exit_on_close_request(false) 필요)
            Event::Window(iced::window::Event::CloseRequested) => Some(Message::HideWindow(id)),
            // 창 열림 시 ID 저장
            Event::Window(iced::window::Event::Opened { .. }) => Some(Message::WindowOpened(id)),
            _ => None,
        });

        // 타이머 + 윈도우 이벤트 결합
        Subscription::batch([
            iced::time::every(Duration::from_secs(1)).map(Message::Tick),
            window_events,
        ])
    }

    /// 현재 로케일 반환
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
        assert_eq!(app.main_state.connection_status, "오프라인 모드");
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
        let _ = app.update(Message::SuggestionReceived("테스트 제안".to_string()));
        assert_eq!(app.recent_suggestions.len(), 1);
        assert_eq!(app.recent_suggestions[0], "테스트 제안");
    }

    #[test]
    fn suggestion_max_count() {
        let mut app = OneshimApp::new();
        // 6개 제안 추가 (최대 5개 유지)
        for i in 0..6 {
            let _ = app.update(Message::SuggestionReceived(format!("제안 {i}")));
        }
        assert_eq!(app.recent_suggestions.len(), 5);
        // 가장 최근 것이 맨 앞에
        assert_eq!(app.recent_suggestions[0], "제안 5");
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
        assert_eq!(app.settings.theme_mode, 0); // 기본 다크

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
        let _ = app.update(Message::UpdateConnectionStatus("연결됨".to_string()));
        assert_eq!(app.main_state.connection_status, "연결됨");
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
        assert_eq!(s.quit, "종료");
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
