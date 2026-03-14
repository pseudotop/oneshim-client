use anyhow::Result;
use directories::ProjectDirs;
use oneshim_automation::audit::{AuditLogAdapter, AuditLogger};
use oneshim_automation::controller::AutomationController;
use oneshim_automation::policy::PolicyClient;
use oneshim_automation::sandbox::create_platform_sandbox;
use oneshim_core::config::{AiAccessMode, AppConfig};
use oneshim_core::config_manager::ConfigManager;
#[cfg(feature = "server")]
use oneshim_core::ports::oauth::OAuthPort;
use oneshim_core::ports::skill_loader::SkillLoader;
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
#[cfg(feature = "server")]
use oneshim_network::oauth::{provider_config::OAuthProviderConfig, OAuthClient};
use oneshim_storage::frame_storage::FrameFileStorage;
#[cfg(feature = "server")]
use oneshim_storage::keychain::{KeychainOps, KeychainSecretStore};
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_vision::processor::EdgeFrameProcessor;
use oneshim_vision::trigger::SmartCaptureTrigger;
use oneshim_web::update_control::{UpdateAction, UpdateControl};
use oneshim_web::{AiRuntimeStatus, RealtimeEvent, WebServer};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
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
use crate::provider_adapters::ExternalOcrPrivacyGuard;
use crate::scheduler::{Scheduler, SchedulerConfig, SchedulerStorage};
use crate::update_coordinator;

/// Type alias to avoid referencing oneshim_network when server feature is off.
#[cfg(feature = "server")]
type OAuthCoordinator =
    Option<Arc<oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator>>;
#[cfg(not(feature = "server"))]
type OAuthCoordinator = Option<()>;

/// Tauri managed state — tokio Handle과 공유 리소스.
/// Fields are `pub` for current and future IPC command handlers.
#[allow(dead_code)]
pub struct AppState {
    pub runtime_handle: tokio::runtime::Handle,
    pub config: AppConfig,
    pub web_port: Arc<AtomicU16>,
    pub storage: Arc<SqliteStorage>,
    pub config_manager: ConfigManager,
    pub update_control: Option<UpdateControl>,
    pub update_action_tx: tokio::sync::mpsc::UnboundedSender<UpdateAction>,
    pub automation_controller: Option<Arc<AutomationController>>,
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
}

/// Separate managed state for OAuthPort — kept outside AppState to avoid
/// conditional compilation in the main struct definition.
pub struct OAuthState(pub Option<Arc<dyn oneshim_core::ports::oauth::OAuthPort>>);

/// Managed state for the OAuth refresh coordinator — enables IPC commands
/// to reset backoff state after successful manual re-authentication.
#[allow(dead_code)] // Field is used via #[cfg(feature = "server")] in commands.rs
pub struct OAuthCoordinatorState(pub OAuthCoordinator);

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

/// Build the OAuth runtime port without probing the OS keychain at startup.
///
/// Keychain availability is determined lazily when OAuth is actually used,
/// which avoids write/delete prompts during normal app boot.
#[cfg(feature = "server")]
fn create_oauth_port(config_dir: &std::path::Path) -> Option<Arc<dyn OAuthPort>> {
    let registry_path = config_dir.join("oneshim-keychain-registry.json");
    match KeychainOps::new(registry_path) {
        Ok(ops) => {
            let secret_store = Arc::new(KeychainSecretStore::new(Arc::new(ops)));
            let providers = vec![OAuthProviderConfig::openai_codex()];
            Some(Arc::new(OAuthClient::new(secret_store, providers)) as Arc<dyn OAuthPort>)
        }
        Err(e) => {
            warn!("Failed to initialize OAuth secret store: {e}");
            None
        }
    }
}

#[cfg(feature = "server")]
fn preflight_provider_oauth_connection(
    handle: &tokio::runtime::Handle,
    ai_config: &oneshim_core::config::AiProviderConfig,
    oauth_port: Option<Arc<dyn OAuthPort>>,
) -> std::result::Result<Option<Arc<dyn OAuthPort>>, oneshim_core::error::CoreError> {
    if ai_config.access_mode != AiAccessMode::ProviderOAuth {
        return Ok(oauth_port);
    }

    let oauth = oauth_port.ok_or_else(|| {
        oneshim_core::error::CoreError::Config(
            "ProviderOAuth mode requires an available OS secret store".to_string(),
        )
    })?;
    let provider = OAuthProviderConfig::openai_codex();
    let status = handle
        .block_on(oauth.connection_status(&provider.provider_id))
        .map_err(|e| oneshim_core::error::CoreError::Config(e.to_string()))?;

    if !status.connected && !status.has_refresh_token {
        return Err(oneshim_core::error::CoreError::Config(
            "ProviderOAuth mode requires an active OAuth connection or a refresh token".to_string(),
        ));
    }

    Ok(Some(oauth))
}

fn oauth_runtime_error_status(
    ai_config: &oneshim_core::config::AiProviderConfig,
    reason: String,
) -> AiRuntimeStatus {
    let ocr_source = match ai_config.ocr_provider {
        oneshim_core::config::OcrProviderType::Remote => "remote",
        oneshim_core::config::OcrProviderType::Local => "local",
    };

    AiRuntimeStatus {
        ocr_source: ocr_source.to_string(),
        llm_source: "oauth".to_string(),
        ocr_fallback_reason: Some(reason.clone()),
        llm_fallback_reason: Some(reason),
    }
}

fn should_fallback_to_noop(ai_config: &oneshim_core::config::AiProviderConfig) -> bool {
    ai_config.fallback_to_local && ai_config.access_mode != AiAccessMode::ProviderOAuth
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
    let web_port = Arc::new(AtomicU16::new(config.web.port));
    maybe_sync_cli_subscription_bridge(&config, &data_dir_path);

    // 2b. Integrity preflight — signed policy bundle verification
    if let Err(e) = crate::integrity_guard::run_preflight(&config, false) {
        warn!("integrity preflight failed (non-fatal): {e}");
    }

    // 3. tokio runtime — Handle만 추출, Runtime은 전용 스레드에 파킹
    let runtime = Runtime::new()?;
    let handle = runtime.handle().clone();
    #[cfg(feature = "server")]
    let config_dir = oneshim_core::config_manager::ConfigManager::config_dir()
        .unwrap_or_else(|_| data_dir_path.clone());
    #[cfg(feature = "server")]
    let oauth_port = create_oauth_port(&config_dir);
    #[cfg(feature = "server")]
    let oauth_coordinator: OAuthCoordinator = {
        use oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator;

        if matches!(config.ai_provider.access_mode, AiAccessMode::ProviderOAuth) {
            oauth_port.as_ref().map(|port| {
                let (token_event_tx, _) = tokio::sync::broadcast::channel(32);
                Arc::new(TokenRefreshCoordinator::new(
                    Arc::clone(port),
                    token_event_tx,
                ))
            })
        } else {
            None
        }
    };
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

    // 5. SQLite storage (with encryption key provisioning)
    let encryption_key =
        match oneshim_storage::encryption::EncryptionKey::load_or_create(&data_dir_path) {
            Ok(key) => {
                info!(
                    "DB encryption key ready ({})",
                    data_dir_path.join(".db_key").display()
                );
                Some(key)
            }
            Err(e) => {
                warn!("DB encryption key provisioning failed (non-fatal): {e}");
                None
            }
        };
    let sqlite_storage = Arc::new(SqliteStorage::open(
        &db_path,
        config.storage.retention_days,
    )?);
    if encryption_key.is_some() {
        info!(
            "SQLite initialized: {} (encryption key provisioned, SQLCipher pending)",
            db_path.display()
        );
    } else {
        info!("SQLite initialized: {} (plaintext)", db_path.display());
    }

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
    #[cfg(feature = "server")]
    let agent_oauth_coordinator = oauth_coordinator.clone();
    #[cfg(not(feature = "server"))]
    let agent_oauth_coordinator: OAuthCoordinator = None;

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
            agent_oauth_coordinator,
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
        let web_event_tx = event_tx.clone();
        let web_config_manager = config_manager.clone();
        let web_audit_logger = Arc::new(tokio::sync::RwLock::new(AuditLogger::default()));
        let web_update_control = update_control.clone();
        let web_port_state = web_port.clone();
        let (bound_port_tx, bound_port_rx) = tokio::sync::oneshot::channel::<u16>();

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
            let process_monitor = Arc::new(ProcessTracker::new());
            let external_ocr_privacy_guard = ExternalOcrPrivacyGuard::new(
                data_dir_path.join("consent.json"),
                config.privacy.pii_filter_level,
                config.ai_provider.external_data_policy,
                config.privacy.clone(),
                process_monitor.clone(),
                Some(web_audit_logger.clone()),
            );

            // Discover skill definitions from user's home directory.
            let skill_loader: Option<Arc<dyn oneshim_core::ports::skill_loader::SkillLoader>> = {
                let mut roots = Vec::new();
                if let Some(home) = directories::BaseDirs::new() {
                    roots.push(home.home_dir().to_path_buf());
                }
                let loader = crate::skill_loader::FileSkillLoader::new(roots);
                if loader.list_skills().is_empty() {
                    None
                } else {
                    Some(Arc::new(loader))
                }
            };

            #[cfg(feature = "server")]
            let runtime = preflight_provider_oauth_connection(
                &handle,
                &config.ai_provider,
                oauth_port.clone(),
            )
            .and_then(|validated_oauth_port| {
                build_automation_runtime(
                    &config.ai_provider,
                    config.privacy.pii_filter_level,
                    automation_frame_storage.clone(),
                    Some(external_ocr_privacy_guard.clone()),
                    skill_loader.clone(),
                    validated_oauth_port,
                )
            });
            #[cfg(not(feature = "server"))]
            let runtime = build_automation_runtime(
                &config.ai_provider,
                config.privacy.pii_filter_level,
                automation_frame_storage,
                Some(external_ocr_privacy_guard),
                skill_loader,
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
                    // Wire GUI interaction (focus probe + overlay driver)
                    let focus_probe: Arc<dyn oneshim_core::ports::focus_probe::FocusProbe> =
                        Arc::new(crate::focus_probe_adapter::ProcessMonitorFocusProbe::new(
                            process_monitor,
                        ));
                    let overlay_driver = crate::platform_overlay::create_platform_overlay_driver();
                    let hmac_secret = std::env::var("ONESHIM_GUI_TICKET_HMAC_SECRET").ok();
                    if let Err(e) = controller.configure_gui_interaction(
                        focus_probe,
                        overlay_driver,
                        hmac_secret,
                    ) {
                        warn!(error = %e, "GUI interaction setup failed (non-fatal)");
                    }
                    Some(Arc::new(controller))
                }
                Err(err) => {
                    if should_fallback_to_noop(&config.ai_provider) {
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
                        if config.ai_provider.access_mode == AiAccessMode::ProviderOAuth {
                            ai_runtime_status = Some(oauth_runtime_error_status(
                                &config.ai_provider,
                                err.to_string(),
                            ));
                        }
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
                .with_audit_logger(Arc::new(AuditLogAdapter::new(web_audit_logger)))
                .with_update_control(web_update_control)
                .with_bound_port_state(web_port_state)
                .with_bound_port_notifier(bound_port_tx);
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
        let frontend_web_port = handle.block_on(async {
            tokio::time::timeout(Duration::from_secs(3), bound_port_rx)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_else(|| web_port.load(Ordering::Relaxed))
        });
        info!("WebServer: http://localhost:{}", frontend_web_port);

        automation_controller_for_state
    } else {
        None
    };

    // 9. SIGINT/SIGTERM → shutdown 브릿지 (Tauri RunEvent::Exit 보완)
    {
        let signal_shutdown_tx = shutdown_tx.clone();
        handle.spawn(async move {
            let lifecycle = crate::lifecycle::LifecycleManager::default();
            lifecycle.wait_for_signal().await;
            info!("OS signal received — triggering shutdown");
            let _ = signal_shutdown_tx.send(true);
        });
    }

    // 10. 실시간 이벤트 → Tauri emit 브릿지 (main window only)
    let app_handle_for_events = _app_handle.clone();
    let mut event_rx = event_tx.subscribe();
    handle.spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            if let Err(e) = app_handle_for_events.emit_to("main", "realtime-event", &event) {
                tracing::debug!("emit error (window may be hidden): {e}");
            }
        }
    });

    // 11. Tauri managed state 등록
    let frontend_web_port = web_port.load(Ordering::Relaxed);

    #[cfg(feature = "server")]
    let oauth_state = OAuthState(oauth_port);
    #[cfg(not(feature = "server"))]
    let oauth_state = OAuthState(None);

    #[cfg(feature = "server")]
    let oauth_coordinator_state = OAuthCoordinatorState(oauth_coordinator);
    #[cfg(not(feature = "server"))]
    let oauth_coordinator_state = OAuthCoordinatorState(None);

    app.manage(AppState {
        runtime_handle: handle,
        config,
        web_port,
        storage: sqlite_storage,
        config_manager,
        update_control: Some(update_control),
        update_action_tx,
        automation_controller,
        shutdown_tx,
    });
    app.manage(oauth_state);
    app.manage(oauth_coordinator_state);

    // 12. 시스템 트레이 초기화
    crate::tray::setup_tray(app)?;

    // 13. macOS dock icon — bare binary (non-.app) needs runtime icon setting
    #[cfg(target_os = "macos")]
    {
        crate::macos_integration::set_dock_icon();
        info!("macOS dock icon set from embedded icon.png");
    }

    // 14. 메인 윈도우 표시 (setup 완료 후)
    if let Some(window) = app.get_webview_window("main") {
        // Windows/Linux: disable decorations for custom titlebar controls
        // macOS: keep decorations=true with titleBarStyle=Overlay for native traffic lights
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window.set_decorations(false);
        }

        // Inject web server port into frontend globals before page loads API calls.
        // Uses Tauri's webview eval to set a simple numeric constant — no user input involved.
        let port_js = format!("window.__ONESHIM_WEB_PORT__ = {};", frontend_web_port);
        let _ = window.eval(&port_js);

        let _ = window.show();
        let _ = window.set_focus();
        debug_assert!(
            window.is_visible().unwrap_or(false),
            "main window must be visible after setup::init()"
        );
    } else {
        debug_assert!(false, "main window not found after setup::init()");
    }

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
    _oauth_coordinator: OAuthCoordinator,
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
    let token_manager = Arc::new(
        TokenManager::new_with_tls(
            &config.server.base_url,
            &config.tls,
            Some(config.request_timeout()),
        )
        .map_err(|e| anyhow::anyhow!("failed to build TLS-aware TokenManager: {e}"))?,
    );
    #[cfg(feature = "server")]
    let api_client = Arc::new(HttpApiClient::new_with_tls(
        &config.server.base_url,
        token_manager.clone(),
        config.request_timeout(),
        &config.tls,
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
    let notifier: Arc<dyn oneshim_core::ports::notifier::DesktopNotifier> = Arc::new(NoOpNotifier);
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

    #[cfg(feature = "server")]
    if let Some(coord) = _oauth_coordinator {
        scheduler = scheduler.with_oauth_coordinator(coord);
    }

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
        tracing::debug!(
            title,
            body,
            "notification suppressed (Tauri handles notifications)"
        );
        Ok(())
    }

    async fn show_error(&self, message: &str) -> Result<(), oneshim_core::error::CoreError> {
        tracing::debug!(
            message,
            "error notification suppressed (Tauri handles notifications)"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tauri.conf.json의 window 설정이 setup::init()의 show() 로직과 일관성 있는지 검증.
    /// visible=false + setup에서 show() 호출하는 패턴이 유지되어야 함.
    #[test]
    fn tauri_conf_window_starts_hidden_for_setup_controlled_show() {
        let conf_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        let conf_str = std::fs::read_to_string(&conf_path).expect("tauri.conf.json must exist");
        let conf: serde_json::Value =
            serde_json::from_str(&conf_str).expect("tauri.conf.json must be valid JSON");

        let windows = conf["app"]["windows"]
            .as_array()
            .expect("app.windows must be an array");
        assert!(!windows.is_empty(), "at least one window must be defined");

        let main_window = windows
            .iter()
            .find(|w| w["label"].as_str() == Some("main"))
            .expect("main window must be defined in tauri.conf.json");

        // visible=false 확인 — setup::init()에서 show()를 호출하는 패턴
        assert_eq!(
            main_window["visible"].as_bool(),
            Some(false),
            "main window must start hidden (visible=false); setup::init() calls show() after initialization"
        );
    }

    /// setup::init() 소스 코드에 window.show() 호출이 포함되어 있는지 정적 검증.
    /// 향후 리팩토링 시 show() 호출이 실수로 제거되는 것을 방지.
    #[test]
    fn setup_init_contains_window_show_call() {
        let setup_src = include_str!("setup.rs");

        assert!(
            setup_src.contains("window.show()"),
            "setup::init() must call window.show() — without this, the GUI window is invisible on launch"
        );
        assert!(
            setup_src.contains("window.set_focus()"),
            "setup::init() must call window.set_focus() after show()"
        );
    }

    /// main.rs에 RunEvent::Reopen 핸들러가 있는지 검증.
    /// macOS dock 아이콘 클릭 시 윈도우를 다시 표시하기 위해 필수.
    #[test]
    fn main_contains_reopen_handler() {
        let main_src = include_str!("main.rs");

        assert!(
            main_src.contains("RunEvent::Reopen"),
            "main.rs must handle RunEvent::Reopen for macOS dock icon clicks"
        );
    }

    #[test]
    fn resolve_db_path_default() {
        let path = resolve_db_path(None);
        assert!(path.to_string_lossy().contains("oneshim.db"));
    }

    #[test]
    fn resolve_db_path_custom() {
        let path = resolve_db_path(Some("/tmp/test_data"));
        assert_eq!(path, PathBuf::from("/tmp/test_data/oneshim.db"));
    }

    #[test]
    fn generate_session_id_format() {
        let id = generate_session_id();
        assert!(id.starts_with("sess_"));
        assert!(id.len() > 20); // sess_ + timestamp + _ + hex
    }

    #[test]
    fn provider_oauth_never_uses_noop_fallback() {
        let config = oneshim_core::config::AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            fallback_to_local: true,
            ..oneshim_core::config::AiProviderConfig::default()
        };

        assert!(!should_fallback_to_noop(&config));
    }

    #[test]
    fn oauth_runtime_error_status_reports_oauth_source() {
        let config = oneshim_core::config::AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            ..oneshim_core::config::AiProviderConfig::default()
        };

        let status = oauth_runtime_error_status(&config, "not authenticated".to_string());
        assert_eq!(status.ocr_source, "local");
        assert_eq!(status.llm_source, "oauth");
        assert_eq!(
            status.llm_fallback_reason.as_deref(),
            Some("not authenticated")
        );
    }
}
