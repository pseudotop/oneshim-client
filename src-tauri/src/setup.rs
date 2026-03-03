use anyhow::Result;
use directories::ProjectDirs;
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::controller::AutomationController;
use oneshim_automation::policy::PolicyClient;
use oneshim_automation::sandbox::create_platform_sandbox;
use oneshim_core::config::{AiAccessMode, AppConfig};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::ports::storage::StorageService;
use oneshim_monitor::activity::ActivityTracker;
use oneshim_monitor::process::ProcessTracker;
use oneshim_monitor::system::SysInfoMonitor;
#[cfg(feature = "server")]
use oneshim_network::auth::TokenManager;
#[cfg(feature = "server")]
use oneshim_network::batch_uploader::BatchUploader;
#[cfg(feature = "server")]
use oneshim_network::http_client::HttpApiClient;
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_vision::processor::EdgeFrameProcessor;
use oneshim_vision::trigger::SmartCaptureTrigger;
use oneshim_web::update_control::{UpdateAction, UpdateControl};
use oneshim_web::{AiRuntimeStatus, RealtimeEvent, WebServer};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{App, Emitter, Manager};
use tokio::runtime::Runtime;
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

/// Tauri managed state — tokio Handle과 공유 리소스
#[allow(dead_code)] // fields used by IPC commands (currently todo!())
pub struct AppState {
    pub runtime_handle: tokio::runtime::Handle,
    pub config: AppConfig,
    pub storage: Arc<SqliteStorage>,
    pub config_manager: ConfigManager,
    pub update_control: Option<UpdateControl>,
    pub update_action_tx: tokio::sync::mpsc::UnboundedSender<UpdateAction>,
    pub automation_controller: Option<Arc<AutomationController>>,
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
}

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
        info!("ProviderSubscriptionCli mode: CLI bridge auto-install disabled (ONESHIM_CLI_BRIDGE_AUTOINSTALL=1)");
        return;
    }
    let project_root = std::env::current_dir().unwrap_or_else(|_| data_dir.to_path_buf());
    let include_user_scope = should_include_user_scope();
    let context_export_path = default_context_export_path(data_dir);
    let report = sync_bridge_files(&project_root, &context_export_path, include_user_scope);
    info!(
        project_root = %project_root.display(),
        context_export = %context_export_path.display(),
        written = report.written_files.len(),
        unchanged = report.unchanged_files.len(),
        errors = report.errors.len(),
        "CLI subscription bridge sync complete"
    );
    if !report.is_successful() {
        for err in report.errors {
            warn!(error = %err, "CLI subscribe file failure");
        }
    }
}

/// Tauri setup 함수 — gui_runner.rs의 Agent + WebServer 초기화 이전
pub fn init(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let _app_handle = app.handle().clone();
    info!("Tauri setup: initializing ONESHIM agent");

    // 1. DB 경로 + 데이터 디렉토리
    let db_path = resolve_db_path(None);
    let data_dir_path = db_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&data_dir_path)?;
    info!("data directory: {}", data_dir_path.display());

    // 2. Config 로드
    let config_manager = ConfigManager::new().unwrap_or_else(|e| {
        warn!("settings init failure, using defaults: {e}");
        let fallback_path = data_dir_path.join("config.json");
        ConfigManager::with_path(fallback_path).expect("failed to create config manager")
    });
    info!("settings file: {:?}", config_manager.config_path());

    let config = config_manager.get();
    maybe_sync_cli_subscription_bridge(&config, &data_dir_path);

    // 3. tokio runtime — Handle만 추출, Runtime은 전용 스레드에 파킹
    let runtime = Runtime::new()?;
    let handle = runtime.handle().clone();
    std::thread::spawn(move || {
        runtime.block_on(std::future::pending::<()>());
    });

    // 4. Auto-updater
    let runtime_auto_update = config.update.auto_install;
    let (update_action_tx, update_action_rx) =
        tokio::sync::mpsc::unbounded_channel::<UpdateAction>();
    let update_control = UpdateControl::new(
        update_action_tx.clone(),
        update_coordinator::initial_status(&config.update, runtime_auto_update),
    );

    if config.update.enabled {
        let update_config = config.update.clone();
        let update_state = update_control.state.clone();
        let update_status_tx = Some(update_control.event_tx.clone());
        handle.spawn(async move {
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

    // 5. SQLite storage
    let sqlite_storage = Arc::new(SqliteStorage::open(
        &db_path,
        config.storage.retention_days,
    )?);
    info!("SQLite initialized: {}", db_path.display());

    // 6. broadcast channel (SSE events)
    let (event_tx, _event_rx) = broadcast::channel::<RealtimeEvent>(256);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // 7. Agent 태스크 (gui_runner.rs run_agent 동일)
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

    handle.spawn(async move {
        if let Err(e) = run_agent(
            agent_storage,
            agent_scheduler_storage,
            agent_focus_storage,
            agent_data_dir,
            agent_config,
            false, // offline_mode — Tauri 모드는 항상 online
            shutdown_rx,
            agent_event_tx,
        )
        .await
        {
            error!("Agent error: {e}");
        }
    });
    info!("Agent started");

    // 8. WebServer (TCP 서버 — 기존 호환)
    let automation_controller = if config.web.enabled {
        let web_storage = sqlite_storage.clone();
        let web_config = config.web.clone();
        let web_shutdown_rx = shutdown_tx.subscribe();
        let web_port = web_config.port;
        let web_event_tx = event_tx.clone();
        let web_config_manager = config_manager.clone();
        let web_audit_logger = Arc::new(tokio::sync::RwLock::new(AuditLogger::default()));
        let web_update_control = update_control.clone();

        let automation_frame_storage = match handle.block_on(async {
            FrameFileStorage::new(
                data_dir_path.clone(),
                config.storage.max_storage_mb,
                config.storage.retention_days,
            )
            .await
        }) {
            Ok(storage) => Some(Arc::new(storage)),
            Err(err) => {
                warn!(error = %err, "frame storage init failure, falling back to NoOp");
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
                        ocr = runtime.ocr_provider_name,
                        llm = runtime.llm_provider_name,
                        "AI provider adapters resolved"
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
                            llm_fallback_reason: Some(fallback_reason),
                        });
                        warn!(error = %err, "AI provider fallback to NoOp executor");
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
                        error!(error = %err, "AI provider failed, automation disabled");
                        None
                    }
                }
            }
        } else {
            None
        };

        let automation_controller_for_state = automation_controller.clone();
        handle.spawn(async move {
            let mut web_server = WebServer::new(web_storage, web_config)
                .with_event_tx(web_event_tx)
                .with_frames_dir(data_dir_path)
                .with_config_manager(web_config_manager)
                .with_audit_logger(web_audit_logger)
                .with_update_control(web_update_control);
            if let Some(status) = ai_runtime_status {
                web_server = web_server.with_ai_runtime_status(status);
            }
            if let Some(ctrl) = automation_controller {
                web_server = web_server.with_automation_controller(ctrl);
            }
            if let Err(e) = web_server.run(web_shutdown_rx).await {
                error!("WebServer error: {e}");
            }
        });
        info!("WebServer: http://localhost:{}", web_port);

        automation_controller_for_state
    } else {
        None
    };

    // 9. 실시간 이벤트 → Tauri emit 브릿지 (main window only)
    let app_handle_for_events = _app_handle.clone();
    let mut event_rx = event_tx.subscribe();
    handle.spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            if let Err(e) = app_handle_for_events.emit_to("main", "realtime-event", &event) {
                tracing::debug!("emit error (window may be hidden): {e}");
            }
        }
    });

    // 10. Tauri managed state 등록
    app.manage(AppState {
        runtime_handle: handle,
        config,
        storage: sqlite_storage,
        config_manager,
        update_control: Some(update_control),
        update_action_tx,
        automation_controller,
        shutdown_tx,
    });

    // 11. 시스템 트레이 초기화
    crate::tray::setup_tray(app)?;

    info!("Tauri setup complete");
    Ok(())
}

/// Agent 태스크 — gui_runner.rs의 run_agent() 동일
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
    info!("Agent initializing");

    let frame_storage = FrameFileStorage::new(
        data_dir.clone(),
        config.storage.max_storage_mb,
        config.storage.retention_days,
    )
    .await?;
    let frame_storage = Arc::new(frame_storage);

    let system_monitor = Arc::new(SysInfoMonitor::new());
    let process_monitor: Arc<dyn oneshim_core::ports::monitor::ProcessMonitor> =
        Arc::new(ProcessTracker::new());
    let activity_monitor = Arc::new(ActivityTracker::new(process_monitor.clone()));

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
        100,
        3,
    ));

    // 데스크톱 알림 — Tauri 모드에서는 NoOp (Tauri 플러그인으로 대체)
    let notifier: Arc<dyn oneshim_core::ports::notifier::DesktopNotifier> =
        Arc::new(NoOpNotifier);
    let notification_manager = Arc::new(NotificationManager::new(
        config.notification.clone(),
        notifier.clone(),
    ));
    let focus_analyzer = Arc::new(FocusAnalyzer::with_defaults(focus_storage, notifier));

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

    let app_config = Arc::new(tokio::sync::RwLock::new(config));
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
        Some(frame_storage),
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

/// NoOp 데스크톱 알림 — Tauri 플러그인으로 대체되므로 Agent에서는 사용 안함
struct NoOpNotifier;

#[async_trait::async_trait]
impl oneshim_core::ports::notifier::DesktopNotifier for NoOpNotifier {
    async fn show_suggestion(
        &self,
        suggestion: &oneshim_core::models::suggestion::Suggestion,
    ) -> Result<(), oneshim_core::error::CoreError> {
        tracing::debug!(id = %suggestion.suggestion_id, "suggestion notification suppressed (Tauri handles notifications)");
        Ok(())
    }

    async fn show_notification(
        &self,
        title: &str,
        body: &str,
    ) -> Result<(), oneshim_core::error::CoreError> {
        tracing::debug!(title, body, "notification suppressed (Tauri handles notifications)");
        Ok(())
    }

    async fn show_error(&self, message: &str) -> Result<(), oneshim_core::error::CoreError> {
        tracing::debug!(message, "error notification suppressed (Tauri handles notifications)");
        Ok(())
    }
}
