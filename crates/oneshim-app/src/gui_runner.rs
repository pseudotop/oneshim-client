use anyhow::Result;
use directories::ProjectDirs;
use iced::{window, Font, Size, Task};
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::controller::AutomationController;
use oneshim_automation::policy::PolicyClient;
use oneshim_automation::sandbox::create_platform_sandbox;
use oneshim_core::config::{AiAccessMode, AppConfig};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::ports::storage::StorageService;
use oneshim_monitor::{activity::ActivityTracker, process::ProcessTracker, system::SysInfoMonitor};
#[cfg(feature = "server")]
use oneshim_network::auth::TokenManager;
#[cfg(feature = "server")]
use oneshim_network::batch_uploader::BatchUploader;
#[cfg(feature = "server")]
use oneshim_network::http_client::HttpApiClient;
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_ui::notifier::DesktopNotifierImpl;
use oneshim_ui::tray::TrayManager;
use oneshim_ui::{OneshimApp, UpdateStatusSnapshot, UpdateUserAction};
use oneshim_vision::processor::EdgeFrameProcessor;
use oneshim_vision::trigger::SmartCaptureTrigger;
use oneshim_web::update_control::{UpdateAction, UpdateControl};
use oneshim_web::{AiRuntimeStatus, RealtimeEvent, WebServer};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::automation_runtime::{build_automation_runtime, build_noop_intent_executor};
use crate::cli_subscription_bridge::{
    default_context_export_path, should_autoinstall_bridge_files, should_include_user_scope,
    sync_bridge_files,
};
use crate::focus_analyzer::{FocusAnalyzer, FocusStorage};
use crate::notification_manager::NotificationManager;
use crate::scheduler::{Scheduler, SchedulerConfig, SchedulerStorage};
use crate::update_coordinator;

const KOREAN_FONT: &[u8] = include_bytes!("../../oneshim-ui/assets/fonts/Pretendard-Regular.otf");

const KOREAN_FONT_NAME: Font = Font::with_name("Pretendard");

fn resolve_db_path(data_dir: Option<&str>) -> PathBuf {
    data_dir
        .map(|d| PathBuf::from(d).join("oneshim.db"))
        .or_else(|| {
            ProjectDirs::from("com", "oneshim", "agent").map(|p| p.data_dir().join("oneshim.db"))
        })
        .unwrap_or_else(|| PathBuf::from("./oneshim.db"))
}

fn generate_session_id() -> String {
    use std::hash::{Hash, Hasher};
    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    let rand_part = hasher.finish() as u32;
    format!("sess_{ts}_{rand_part:08x}")
}

fn maybe_sync_cli_subscription_bridge(config: &AppConfig, data_dir: &std::path::Path) {
    if config.ai_provider.access_mode != AiAccessMode::ProviderSubscriptionCli {
        return;
    }

    if !should_autoinstall_bridge_files() {
        info!(
            "ProviderSubscriptionCli mode detection: CLI 브리지 자동 설치 비active화 (ONESHIM_CLI_BRIDGE_AUTOINSTALL=1로 active화)"
        );
        return;
    }

    let project_root = std::env::current_dir().unwrap_or_else(|_| data_dir.to_path_buf());
    let include_user_scope = should_include_user_scope();
    let context_export_path = default_context_export_path(data_dir);
    let report = sync_bridge_files(&project_root, &context_export_path, include_user_scope);

    info!(
        project_root = %project_root.display(),
        context_export = %context_export_path.display(),
        include_user_scope,
        written_files = report.written_files.len(),
        unchanged_files = report.unchanged_files.len(),
        errors = report.errors.len(),
        "CLI subscribe 브리지 file 동기화 completed"
    );

    if !report.is_successful() {
        for err in report.errors {
            warn!(error = %err, "CLI subscribe file failure");
        }
    }
}

/// - Main thread: iced GUI
pub fn run_gui(offline_mode: bool, data_dir: Option<&str>) -> Result<()> {
    info!("GUI + Agent mode started");

    let db_path = resolve_db_path(data_dir);
    let data_dir_path = db_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    std::fs::create_dir_all(&data_dir_path)?;
    info!("data save path: {}", data_dir_path.display());

    let disable_tray = std::env::var("ONESHIM_DISABLE_TRAY")
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);

    let tray_rx = if disable_tray {
        info!("system tray initialize skipped (ONESHIM_DISABLE_TRAY)");
        None
    } else {
        match TrayManager::new() {
            Ok((manager, rx)) => {
                Box::leak(Box::new(manager));
                info!("system tray initialize completed");
                Some(rx)
            }
            Err(e) => {
                warn!("system tray initialize failure: {e}");
                None
            }
        }
    };

    let config_manager = match ConfigManager::new() {
        Ok(manager) => manager,
        Err(init_err) => {
            warn!("settings initialize failure, attempting fallback: {init_err}");
            let fallback_path = data_dir_path.join("config.json");
            match ConfigManager::with_path(fallback_path) {
                Ok(manager) => manager,
                Err(fallback_err) => {
                    return Err(anyhow::anyhow!(
                        "failed to initialize config manager (init: {init_err}; fallback: {fallback_err})"
                    ));
                }
            }
        }
    };
    info!("settings file: {:?}", config_manager.config_path());

    let config = config_manager.get();
    maybe_sync_cli_subscription_bridge(&config, &data_dir_path);

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
    info!("SQLite save initialize: {}", db_path.display());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    if !offline_mode && config.update.enabled {
        let update_config = config.update.clone();
        let update_state = update_control.state.clone();
        let update_status_tx = Some(update_control.event_tx.clone());
        runtime.spawn(async move {
            update_coordinator::run_update_coordinator(
                update_config,
                update_state,
                update_action_rx,
                update_status_tx,
                runtime_auto_update,
            )
            .await;
        });
    }

    let (event_tx, _event_rx) = broadcast::channel::<RealtimeEvent>(256);

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let agent_storage: Arc<dyn StorageService> = sqlite_storage.clone();
    let agent_scheduler_storage: Arc<dyn SchedulerStorage> = sqlite_storage.clone();
    let agent_focus_storage: Arc<dyn FocusStorage> = sqlite_storage.clone();
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
            agent_scheduler_storage,
            agent_focus_storage,
            agent_data_dir,
            agent_config,
            offline_mode,
            shutdown_rx,
            agent_event_tx,
        )
        .await
        {
            error!("Agent error: {e}");
        }
    });

    info!("Agent started");

    if config.web.enabled {
        let web_storage = sqlite_storage.clone();
        let web_config = config.web.clone();
        let web_shutdown_rx = shutdown_tx.subscribe();
        let web_port = web_config.port;
        let web_event_tx = event_tx;
        let web_config_manager = config_manager.clone();
        let web_audit_logger = Arc::new(tokio::sync::RwLock::new(AuditLogger::default()));
        let web_update_control = update_control.clone();
        let automation_frame_storage = match runtime.block_on(async {
            FrameFileStorage::new(
                data_dir_path.clone(),
                config.storage.max_storage_mb,
                config.storage.retention_days,
            )
            .await
        }) {
            Ok(storage) => Some(Arc::new(storage)),
            Err(err) => {
                warn!(
                    error = %err,
                    "자동화 전용 frame save소 initialize failure: NoOp 요소 탐색기로 폴백"
                );
                None
            }
        };

        let mut ai_runtime_status: Option<AiRuntimeStatus> = None;
        let automation_controller = if config.automation.enabled {
            let runtime = build_automation_runtime(
                &config.ai_provider,
                config.privacy.pii_filter_level,
                automation_frame_storage,
            );
            match runtime {
                Ok(runtime) => {
                    ai_runtime_status = Some(AiRuntimeStatus {
                        ocr_source: runtime.ocr_source.as_str().to_string(),
                        llm_source: runtime.llm_source.as_str().to_string(),
                        ocr_fallback_reason: runtime.ocr_fallback_reason.clone(),
                        llm_fallback_reason: runtime.llm_fallback_reason.clone(),
                    });
                    info!(
                        access_mode = ?runtime.access_mode,
                        ocr_provider = runtime.ocr_provider_name,
                        ocr_source = runtime.ocr_source.as_str(),
                        ocr_fallback_reason = ?runtime.ocr_fallback_reason,
                        llm_provider = runtime.llm_provider_name,
                        llm_source = runtime.llm_source.as_str(),
                        llm_fallback_reason = ?runtime.llm_fallback_reason,
                        "AI 제공자 어댑터 해석 completed"
                    );

                    let policy_client = Arc::new(PolicyClient::new());
                    let sandbox = create_platform_sandbox(&config.automation.sandbox);
                    let mut controller = AutomationController::new(
                        policy_client,
                        web_audit_logger.clone(),
                        sandbox,
                        config.automation.sandbox.clone(),
                    );
                    controller.set_enabled(true);
                    controller.set_scene_finder(runtime.element_finder.clone());
                    controller.set_intent_executor(runtime.intent_executor);
                    controller.set_intent_planner(runtime.intent_planner);
                    Some(Arc::new(controller))
                }
                Err(err) => {
                    if config.ai_provider.fallback_to_local {
                        let fallback_reason = err.to_string();
                        ai_runtime_status = Some(AiRuntimeStatus {
                            ocr_source: "local-fallback".to_string(),
                            llm_source: "local-fallback".to_string(),
                            ocr_fallback_reason: Some(fallback_reason.clone()),
                            llm_fallback_reason: Some(fallback_reason.clone()),
                        });
                        warn!(
                            error = %err,
                            fallback_enabled = true,
                            "AI 제공자 어댑터 해석 failure; NoOp 자동화 execution기로 폴백"
                        );

                        let policy_client = Arc::new(PolicyClient::new());
                        let sandbox = create_platform_sandbox(&config.automation.sandbox);
                        let mut controller = AutomationController::new(
                            policy_client,
                            web_audit_logger.clone(),
                            sandbox,
                            config.automation.sandbox.clone(),
                        );
                        controller.set_enabled(true);
                        controller.set_intent_executor(build_noop_intent_executor());
                        Some(Arc::new(controller))
                    } else {
                        error!(
                            error = %err,
                            fallback_enabled = false,
                            "AI 제공자 어댑터 해석 failure; fallback_to_local=false 이므로 자동화 컨트롤러를 비active화합니다"
                        );
                        None
                    }
                }
            }
        } else {
            None
        };

        runtime.spawn(async move {
            let mut web_server = WebServer::new(web_storage, web_config)
                .with_event_tx(web_event_tx)
                .with_frames_dir(data_dir_path.clone())
                .with_config_manager(web_config_manager)
                .with_audit_logger(web_audit_logger)
                .with_update_control(web_update_control.clone());
            if let Some(status) = ai_runtime_status {
                web_server = web_server.with_ai_runtime_status(status);
            }
            if let Some(ctrl) = automation_controller {
                web_server = web_server.with_automation_controller(ctrl);
            }
            if let Err(e) = web_server.run(web_shutdown_rx).await {
                error!("server error: {e}");
            }
        });
        info!("web server task started (port configured: {})", web_port);
    }

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

    let (ui_update_status_tx, ui_update_status_rx) =
        std::sync::mpsc::channel::<UpdateStatusSnapshot>();
    let initial_update_status =
        runtime.block_on(async { update_control.state.read().await.clone() });
    let _ = ui_update_status_tx.send(UpdateStatusSnapshot {
        phase: format!("{:?}", initial_update_status.phase),
        message: initial_update_status.message,
        pending_latest_version: initial_update_status.pending.map(|p| p.latest_version),
        auto_install: initial_update_status.auto_install,
    });
    let mut update_rx = update_control.subscribe();
    runtime.spawn(async move {
        while let Ok(status) = update_rx.recv().await {
            let snapshot = UpdateStatusSnapshot {
                phase: format!("{:?}", status.phase),
                message: status.message,
                pending_latest_version: status.pending.map(|p| p.latest_version),
                auto_install: status.auto_install,
            };
            if ui_update_status_tx.send(snapshot).is_err() {
                break;
            }
        }
    });

    let mut app = OneshimApp::new()
        .with_offline_mode(offline_mode)
        .with_storage(sqlite_storage)
        .with_update_action_sender(ui_update_tx)
        .with_update_status_receiver(ui_update_status_rx);

    if let Some(rx) = tray_rx {
        app = app.with_tray_receiver(rx);
    }

    let app_cell = std::cell::RefCell::new(Some(app));
    let result = iced::application(
        move || {
            let state = app_cell
                .borrow_mut()
                .take()
                .expect("boot called more than once");
            (state, Task::none())
        },
        OneshimApp::update,
        OneshimApp::view,
    )
    .title(OneshimApp::title)
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
    .run();

    info!("GUI ended, Agent in progress...");
    let _ = shutdown_tx.send(true);

    runtime.block_on(async {
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    result.map_err(|e| anyhow::anyhow!("GUI execution error: {e}"))
}

#[allow(clippy::too_many_arguments)]
async fn run_agent(
    storage: Arc<dyn StorageService>,
    scheduler_storage: Arc<dyn SchedulerStorage>,
    focus_storage: Arc<dyn FocusStorage>,
    data_dir: PathBuf,
    config: AppConfig,
    offline_mode: bool,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    event_tx: Option<broadcast::Sender<RealtimeEvent>>,
) -> Result<()> {
    info!("Agent initialize started");

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
    let capture_trigger: Arc<dyn oneshim_core::ports::vision::CaptureTrigger> =
        Arc::new(SmartCaptureTrigger::new(config.vision.capture_throttle_ms));
    let ocr_tessdata = std::env::var("ONESHIM_TESSDATA").ok().map(PathBuf::from);
    let frame_processor: Arc<dyn oneshim_core::ports::vision::FrameProcessor> =
        Arc::new(EdgeFrameProcessor::new(
            config.vision.thumbnail_width,
            config.vision.thumbnail_height,
            ocr_tessdata,
        ));

    #[cfg(feature = "server")]
    let token_manager = Arc::new(TokenManager::new(&config.server.base_url));
    #[cfg(feature = "server")]
    let api_client = Arc::new(HttpApiClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.request_timeout(),
    )?);
    let session_id = generate_session_id();
    #[cfg(feature = "server")]
    let batch_uploader = Arc::new(BatchUploader::new(
        api_client.clone(),
        session_id.clone(),
        100, // max_batch_size
        3,   // max_retries
    ));

    let notifier: Arc<dyn oneshim_core::ports::notifier::DesktopNotifier> =
        Arc::new(DesktopNotifierImpl::new());
    let notification_manager = Arc::new(NotificationManager::new(
        config.notification.clone(),
        notifier.clone(),
    ));
    let focus_analyzer = Arc::new(FocusAnalyzer::with_defaults(focus_storage, notifier));

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
        ai_access_mode: config.ai_provider.access_mode,
        external_data_policy: config.ai_provider.external_data_policy,
        privacy_config: config.privacy.clone(),
        idle_threshold_secs: 300,
    };

    #[cfg(feature = "server")]
    let batch_sink_opt: Option<Arc<dyn oneshim_core::ports::batch_sink::BatchSink>> =
        Some(batch_uploader.clone());
    #[cfg(not(feature = "server"))]
    let batch_sink_opt: Option<Arc<dyn oneshim_core::ports::batch_sink::BatchSink>> = None;

    #[cfg(feature = "server")]
    let api_client_opt: Option<Arc<dyn oneshim_core::ports::api_client::ApiClient>> =
        Some(api_client.clone());
    #[cfg(not(feature = "server"))]
    let api_client_opt: Option<Arc<dyn oneshim_core::ports::api_client::ApiClient>> = None;

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
        scheduler_storage,
        Some(frame_storage.clone()),
        batch_sink_opt,
        api_client_opt,
    )
    .with_notification_manager(notification_manager)
    .with_focus_analyzer(focus_analyzer);

    if let Some(tx) = event_tx {
        scheduler = scheduler.with_event_tx(tx);
    }

    info!("Agent started (offline={})", offline_mode);
    scheduler.run(shutdown_rx).await;

    info!("Agent ended");
    Ok(())
}

#[cfg(test)]
mod tests {}
