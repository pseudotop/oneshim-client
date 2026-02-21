//! GUI 런너 모듈.
//!
//! iced 애플리케이션 실행 및 백그라운드 Agent 통합.
//! 단일 프로세스: GUI (main thread) + Agent (tokio task)

use anyhow::Result;
use directories::ProjectDirs;
use iced::{window, Font, Size, Task};
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::controller::AutomationController;
use oneshim_automation::input_driver::{NoOpElementFinder, NoOpInputDriver};
use oneshim_automation::intent_resolver::{IntentExecutor, IntentResolver};
use oneshim_automation::policy::PolicyClient;
use oneshim_automation::sandbox::create_platform_sandbox;
use oneshim_core::config::AppConfig;
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::models::intent::IntentConfig;
use oneshim_monitor::{activity::ActivityTracker, process::ProcessTracker, system::SysInfoMonitor};
use oneshim_network::auth::TokenManager;
use oneshim_network::batch_uploader::BatchUploader;
use oneshim_network::http_client::HttpApiClient;
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_ui::notifier::DesktopNotifierImpl;
use oneshim_ui::tray::TrayManager;
use oneshim_ui::{OneshimApp, UpdateUserAction};
use oneshim_vision::processor::EdgeFrameProcessor;
use oneshim_vision::trigger::SmartCaptureTrigger;
use oneshim_web::update_control::{UpdateAction, UpdateControl};
use oneshim_web::{RealtimeEvent, WebServer};
use std::panic;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::notification_manager::NotificationManager;
use crate::scheduler::{Scheduler, SchedulerConfig};
use crate::update_coordinator;

/// 한글 지원 폰트 (Pretendard - 오픈소스 한글 폰트)
const KOREAN_FONT: &[u8] = include_bytes!("../../oneshim-ui/assets/fonts/Pretendard-Regular.otf");

/// 한글 폰트 정의
const KOREAN_FONT_NAME: Font = Font::with_name("Pretendard");

/// 데이터베이스 경로 결정 (플랫폼별 기본 경로)
fn resolve_db_path(data_dir: Option<&str>) -> PathBuf {
    data_dir
        .map(|d| PathBuf::from(d).join("oneshim.db"))
        .or_else(|| {
            ProjectDirs::from("com", "oneshim", "agent").map(|p| p.data_dir().join("oneshim.db"))
        })
        .unwrap_or_else(|| PathBuf::from("./oneshim.db"))
}

/// 세션 ID 생성
fn generate_session_id() -> String {
    use std::hash::{Hash, Hasher};
    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    let rand_part = hasher.finish() as u32;
    format!("sess_{ts}_{rand_part:08x}")
}

/// GUI + Agent 실행
///
/// 단일 프로세스에서 GUI와 Agent를 함께 실행합니다.
/// - Main thread: iced GUI
/// - Tokio task: Agent (모니터링, 스크린샷, 저장)
pub fn run_gui(offline_mode: bool, data_dir: Option<&str>) -> Result<()> {
    info!("GUI + Agent 모드 시작");

    // 1. 데이터베이스 경로 및 디렉토리 설정
    let db_path = resolve_db_path(data_dir);
    let data_dir_path = db_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    std::fs::create_dir_all(&data_dir_path)?;
    info!("데이터 저장 경로: {}", data_dir_path.display());

    // 2. 시스템 트레이 초기화 (메인 스레드 필수 - macOS)
    let tray_rx = match TrayManager::new() {
        Ok((manager, rx)) => {
            Box::leak(Box::new(manager));
            info!("시스템 트레이 초기화 완료");
            Some(rx)
        }
        Err(e) => {
            warn!("시스템 트레이 초기화 실패: {e}");
            None
        }
    };

    // 3. tokio 런타임 panic hook
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let panic_msg = info.to_string();
        if panic_msg.contains("Cannot drop a runtime")
            || panic_msg.contains("cannot drop a runtime")
        {
            info!("tokio 런타임 panic 무시 (트레이 모드)");
            return;
        }
        default_hook(info);
    }));

    // 4. 설정 관리자 초기화 (파일에서 로드 또는 기본값 생성)
    let config_manager = ConfigManager::new().unwrap_or_else(|e| {
        warn!("설정 관리자 초기화 실패, 기본 설정 사용: {e}");
        // 기본 경로로 재시도
        let fallback_path = data_dir_path.join("config.json");
        ConfigManager::with_path(fallback_path).expect("설정 관리자 생성 실패")
    });
    info!("설정 파일: {:?}", config_manager.config_path());

    let config = config_manager.get();

    let runtime_auto_update = config.update.auto_install;
    let (update_action_tx, update_action_rx) =
        tokio::sync::mpsc::unbounded_channel::<UpdateAction>();
    let update_control = UpdateControl::new(
        update_action_tx.clone(),
        update_coordinator::initial_status(&config.update, runtime_auto_update),
    );

    let sqlite_storage = Arc::new(SqliteStorage::open(
        &db_path,
        config.storage.retention_days,
    )?);
    info!("SQLite 저장소 초기화: {}", db_path.display());

    // 5. tokio 런타임 생성 (Agent 백그라운드 실행용)
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    if !offline_mode && config.update.enabled {
        let update_config = config.update.clone();
        let update_state = update_control.state.clone();
        runtime.spawn(async move {
            update_coordinator::run_update_coordinator(
                update_config,
                update_state,
                update_action_rx,
                runtime_auto_update,
            )
            .await;
        });
    }

    // 6. 실시간 이벤트 브로드캐스트 채널 생성 (웹 대시보드용)
    let (event_tx, _event_rx) = broadcast::channel::<RealtimeEvent>(256);

    // 7. Agent 백그라운드 태스크 시작
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let agent_storage = sqlite_storage.clone();
    let agent_data_dir = data_dir_path.clone();
    let agent_config = config.clone();
    let agent_event_tx = if config.web.enabled {
        Some(event_tx.clone())
    } else {
        None
    };

    let _agent_handle = runtime.spawn(async move {
        if let Err(e) = run_agent(
            agent_storage,
            agent_data_dir,
            agent_config,
            offline_mode,
            shutdown_rx,
            agent_event_tx,
        )
        .await
        {
            error!("Agent 오류: {e}");
        }
    });

    info!("Agent 백그라운드 태스크 시작");

    // 8. 자동화 컨트롤러 + 웹 대시보드 서버 시작
    if config.web.enabled {
        let web_storage = sqlite_storage.clone();
        let web_config = config.web.clone();
        let web_shutdown_rx = shutdown_tx.subscribe();
        let web_port = web_config.port;
        let web_event_tx = event_tx;
        let web_config_manager = config_manager.clone();
        let web_audit_logger = Arc::new(tokio::sync::RwLock::new(AuditLogger::default()));

        // 자동화 컨트롤러 (config.automation.enabled일 때만)
        let automation_controller = if config.automation.enabled {
            let policy_client = Arc::new(PolicyClient::new());
            let sandbox = create_platform_sandbox(&config.automation.sandbox);
            let mut controller = AutomationController::new(
                policy_client,
                web_audit_logger.clone(),
                sandbox,
                config.automation.sandbox.clone(),
            );
            controller.set_enabled(true);
            let input_driver: Arc<dyn oneshim_core::ports::input_driver::InputDriver> =
                Arc::new(NoOpInputDriver);
            let element_finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder> =
                Arc::new(NoOpElementFinder);
            let resolver =
                IntentResolver::new(element_finder, input_driver, IntentConfig::default());
            controller.set_intent_executor(Arc::new(IntentExecutor::new(
                resolver,
                IntentConfig::default(),
            )));
            Some(Arc::new(controller))
        } else {
            None
        };

        runtime.spawn(async move {
            let mut web_server = WebServer::new(web_storage, web_config)
                .with_event_tx(web_event_tx)
                .with_config_manager(web_config_manager)
                .with_audit_logger(web_audit_logger)
                .with_update_control(update_control.clone());
            if let Some(ctrl) = automation_controller {
                web_server = web_server.with_automation_controller(ctrl);
            }
            if let Err(e) = web_server.run(web_shutdown_rx).await {
                error!("웹 서버 오류: {e}");
            }
        });
        info!("웹 대시보드: http://localhost:{}", web_port);
    }

    // 7. OneshimApp 생성 (Storage 참조 전달)
    let (ui_update_tx, ui_update_rx) = std::sync::mpsc::channel::<UpdateUserAction>();
    let update_bridge_tx = update_action_tx.clone();
    std::thread::spawn(move || {
        while let Ok(action) = ui_update_rx.recv() {
            let mapped = match action {
                UpdateUserAction::Approve => UpdateAction::Approve,
                UpdateUserAction::Defer => UpdateAction::Defer,
            };
            if update_bridge_tx.send(mapped).is_err() {
                break;
            }
        }
    });

    let mut app = OneshimApp::new()
        .with_offline_mode(offline_mode)
        .with_storage(sqlite_storage)
        .with_update_action_sender(ui_update_tx);

    if let Some(rx) = tray_rx {
        app = app.with_tray_receiver(rx);
    }

    // 8. iced 애플리케이션 실행
    let result = iced::application(OneshimApp::title, OneshimApp::update, OneshimApp::view)
        .theme(OneshimApp::theme)
        .subscription(OneshimApp::subscription)
        .font(KOREAN_FONT)
        .default_font(KOREAN_FONT_NAME)
        .exit_on_close_request(false)
        .window(window::Settings {
            size: Size::new(420.0, 650.0),
            min_size: Some(Size::new(350.0, 450.0)),
            max_size: Some(Size::new(800.0, 900.0)),
            position: window::Position::Centered,
            resizable: true,
            decorations: true,
            transparent: false,
            exit_on_close_request: false,
            ..window::Settings::default()
        })
        .run_with(move || (app, Task::none()));

    // 9. 종료 시 Agent 정리
    info!("GUI 종료, Agent 정리 중...");
    let _ = shutdown_tx.send(true);

    // Agent 종료 대기 (최대 3초)
    runtime.block_on(async {
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    result.map_err(|e| anyhow::anyhow!("GUI 실행 오류: {e}"))
}

/// Agent 백그라운드 실행
async fn run_agent(
    sqlite_storage: Arc<SqliteStorage>,
    data_dir: PathBuf,
    config: AppConfig,
    offline_mode: bool,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    event_tx: Option<broadcast::Sender<RealtimeEvent>>,
) -> Result<()> {
    info!("Agent 초기화 시작");

    // Frame storage
    let frame_storage = FrameFileStorage::new(
        data_dir.clone(),
        config.storage.max_storage_mb,
        config.storage.retention_days,
    )
    .await?;
    let frame_storage = Arc::new(frame_storage);

    // Monitors
    let system_monitor = Arc::new(SysInfoMonitor::new());
    let process_monitor: Arc<dyn oneshim_core::ports::monitor::ProcessMonitor> =
        Arc::new(ProcessTracker::new());
    let activity_monitor = Arc::new(ActivityTracker::new(process_monitor.clone()));

    // Vision pipeline
    let capture_trigger = Box::new(SmartCaptureTrigger::new(config.vision.capture_throttle_ms));
    let ocr_tessdata = std::env::var("ONESHIM_TESSDATA").ok().map(PathBuf::from);
    let frame_processor = Box::new(EdgeFrameProcessor::new(
        config.vision.thumbnail_width,
        config.vision.thumbnail_height,
        ocr_tessdata,
    ));

    // Network (오프라인 모드에서도 생성하지만 사용 안 함)
    let token_manager = Arc::new(TokenManager::new(&config.server.base_url));
    let api_client = Arc::new(HttpApiClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.request_timeout(),
    )?);
    let session_id = generate_session_id();
    let batch_uploader = Arc::new(BatchUploader::new(
        api_client.clone(),
        session_id.clone(),
        100, // max_batch_size
        3,   // max_retries
    ));

    // Storage trait object
    let storage: Arc<dyn oneshim_core::ports::storage::StorageService> = sqlite_storage.clone();

    // 알림 관리자
    let notifier: Arc<dyn oneshim_core::ports::notifier::DesktopNotifier> =
        Arc::new(DesktopNotifierImpl::new());
    let notification_manager = Arc::new(NotificationManager::new(
        config.notification.clone(),
        notifier,
    ));

    // Scheduler
    let scheduler_config = SchedulerConfig {
        poll_interval: Duration::from_millis(config.monitor.poll_interval_ms),
        metrics_interval: Duration::from_secs(5),
        process_interval: Duration::from_secs(10),
        detailed_process_interval: Duration::from_secs(30),
        input_activity_interval: Duration::from_secs(30),
        sync_interval: Duration::from_millis(config.monitor.sync_interval_ms),
        heartbeat_interval: Duration::from_millis(config.monitor.heartbeat_interval_ms),
        aggregation_interval: Duration::from_secs(3600),
        session_id,
        offline_mode,
        idle_threshold_secs: 300,
    };

    let app_config = Arc::new(tokio::sync::RwLock::new(config.clone()));
    let mut scheduler = Scheduler::new(
        scheduler_config,
        app_config,
        system_monitor,
        activity_monitor,
        process_monitor,
        capture_trigger,
        frame_processor,
        storage,
        sqlite_storage,
        Some(frame_storage),
        batch_uploader,
        api_client,
    )
    .with_notification_manager(notification_manager);

    // 실시간 이벤트 브로드캐스트 채널 연결
    if let Some(tx) = event_tx {
        scheduler = scheduler.with_event_tx(tx);
    }

    info!("Agent 스케줄러 시작 (offline={})", offline_mode);
    scheduler.run(shutdown_rx).await;

    info!("Agent 종료");
    Ok(())
}

#[cfg(test)]
mod tests {
    // GUI 테스트는 headless 환경에서 실행 불가
}
