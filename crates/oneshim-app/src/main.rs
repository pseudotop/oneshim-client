//! # oneshim-app
//!

mod automation_runtime;
mod autostart;
mod cli_subscription_bridge;
mod event_bus;
mod focus_analyzer;
mod gui_runner;
mod integrity_guard;
mod lifecycle;
mod memory_profiler;
mod notification_manager;
mod provider_adapters;
mod scheduler;
mod update_coordinator;
mod updater;
mod workflow_intelligence;

use anyhow::Result;
use clap::Parser;
use directories::ProjectDirs;
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::controller::AutomationController;
use oneshim_automation::policy::PolicyClient;
use oneshim_automation::sandbox::create_platform_sandbox;
use oneshim_core::config::{AiAccessMode, AppConfig};
use oneshim_core::config_manager::ConfigManager;
use oneshim_monitor::activity::ActivityTracker;
use oneshim_monitor::process::ProcessTracker;
use oneshim_monitor::system::SysInfoMonitor;
use oneshim_network::auth::TokenManager;
use oneshim_network::batch_uploader::BatchUploader;
use oneshim_network::grpc::{GrpcConfig, UnifiedClient};
use oneshim_network::http_client::HttpApiClient;
use oneshim_network::sse_client::SseStreamClient;
use oneshim_suggestion::queue::SuggestionQueue;
use oneshim_suggestion::receiver::SuggestionReceiver;
use oneshim_ui::notifier::DesktopNotifierImpl;
use oneshim_vision::processor::EdgeFrameProcessor;
use oneshim_vision::trigger::SmartCaptureTrigger;
use oneshim_web::update_control::{UpdateAction, UpdateControl};
use oneshim_web::WebServer;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::automation_runtime::{build_automation_runtime, build_noop_intent_executor};
use crate::cli_subscription_bridge::{
    default_context_export_path, should_autoinstall_bridge_files, should_include_user_scope,
    sync_bridge_files,
};
use crate::event_bus::EventBus;
use crate::focus_analyzer::FocusAnalyzer;
use crate::lifecycle::LifecycleManager;
use crate::notification_manager::NotificationManager;
use crate::scheduler::{Scheduler, SchedulerConfig};

///
#[derive(Parser, Debug)]
#[command(name = "oneshim")]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, short = 'o')]
    offline: bool,

    #[arg(long, short = 's')]
    server: Option<String>,

    #[arg(long, short = 'l', default_value = "info")]
    log_level: String,

    #[arg(long, default_value = "1000")]
    poll_interval: u64,

    #[arg(long)]
    data_dir: Option<String>,

    #[arg(long)]
    enable_autostart: bool,

    #[arg(long)]
    disable_autostart: bool,

    #[arg(long)]
    autostart_status: bool,

    #[arg(long, short = 'g')]
    gui: bool,

    #[arg(long)]
    auto_update: bool,

    #[arg(long)]
    approve_update: bool,
}

fn generate_session_id() -> String {
    use std::hash::{Hash, Hasher};

    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    let rand_part = hasher.finish() as u32;
    format!("sess_{ts}_{rand_part:08x}")
}

///
/// - macOS: `~/Library/Application Support/com.oneshim.agent/oneshim.db`
/// - Windows: `%APPDATA%\oneshim\agent\oneshim.db`
/// - Linux: `~/.local/share/oneshim/agent/oneshim.db`
fn resolve_db_path(data_dir: Option<&str>) -> PathBuf {
    data_dir
        .map(|d| PathBuf::from(d).join("oneshim.db"))
        .or_else(|| {
            ProjectDirs::from("com", "oneshim", "agent").map(|p| p.data_dir().join("oneshim.db"))
        })
        .unwrap_or_else(|| PathBuf::from("./oneshim.db"))
}

fn maybe_sync_cli_subscription_bridge(config: &AppConfig, data_dir: &std::path::Path) {
    if config.ai_provider.access_mode != AiAccessMode::ProviderSubscriptionCli {
        return;
    }

    if !should_autoinstall_bridge_files() {
        info!(
            "ProviderSubscriptionCli mode detected: bridge auto-install disabled (set ONESHIM_CLI_BRIDGE_AUTOINSTALL=1 to enable)"
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
        "CLI subscription bridge file sync complete"
    );

    if !report.is_successful() {
        for err in report.errors {
            warn!(error = %err, "CLI subscribe file failure");
        }
    }
}

fn print_banner(offline: bool) {
    println!();
    println!("==============================================================");
    println!("ONESHIM");
    if offline {
        println!("Mode: offline (local-only)");
    } else {
        println!("Mode: connected (platform integration)");
    }
    println!("==============================================================");
    println!();
}

fn print_offline_features() {
    println!("Offline mode capabilities:");
    println!("- system monitoring: CPU, memory, and disk usage");
    println!("- active window tracking");
    println!("- screenshot capture and delta encoding");
    println!("- local data persistence (SQLite)");
    println!("- automatic PII filtering");
    println!("- server upload: disabled");
    println!("- AI suggestion stream: requires server connection");
    println!("- real-time sync: requires server connection");
    println!();
    println!("Tip: connect to a server with:");
    println!("  oneshim --server http://your-server:8000");
    println!();
}

fn handle_autostart_commands(args: &Args) -> bool {
    if args.autostart_status {
        match autostart::is_autostart_enabled() {
            Ok(enabled) => {
                if enabled {
                    println!("[OK] auto-start: enabled");
                    println!("ONESHIM will start on login.");
                } else {
                    println!("[INFO] auto-start: disabled");
                    println!("Enable with: oneshim --enable-autostart");
                }
            }
            Err(e) => {
                eprintln!("[WARN] failed to check auto-start state: {e}");
            }
        }
        return true;
    }

    if args.enable_autostart {
        println!("[INFO] enabling auto-start...");
        match autostart::enable_autostart() {
            Ok(()) => {
                println!("[OK] auto-start enabled.");
                println!("ONESHIM will start on next login.");
                #[cfg(target_os = "macos")]
                println!("Path: ~/Library/LaunchAgents/com.oneshim.agent.plist");
                #[cfg(target_os = "windows")]
                println!("Path: HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run");
            }
            Err(e) => {
                eprintln!("[ERROR] failed to enable auto-start: {e}");
                std::process::exit(1);
            }
        }
        return true;
    }

    if args.disable_autostart {
        println!("[INFO] disabling auto-start...");
        match autostart::disable_autostart() {
            Ok(()) => {
                println!("[OK] auto-start disabled.");
                println!("ONESHIM will no longer auto-start on login.");
            }
            Err(e) => {
                eprintln!("[ERROR] failed to disable auto-start: {e}");
                std::process::exit(1);
            }
        }
        return true;
    }

    false
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if handle_autostart_commands(&args) {
        return Ok(());
    }

    let log_filter = format!(
        "oneshim={},oneshim_app={},oneshim_ui={},oneshim_core={},oneshim_monitor={},oneshim_vision={},oneshim_storage={},oneshim_network={},oneshim_suggestion={}",
        args.log_level, args.log_level, args.log_level, args.log_level, args.log_level, args.log_level, args.log_level, args.log_level, args.log_level
    );
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_filter)),
        )
        .init();

    if args.gui {
        return gui_runner::run_gui(args.offline, args.data_dir.as_deref());
    }

    print_banner(args.offline);

    if args.offline {
        print_offline_features();
    }

    info!("ONESHIM client started");

    let mut config = AppConfig::default_config();

    if let Some(ref server_url) = args.server {
        config.server.base_url = server_url.clone();
    }
    config.monitor.poll_interval_ms = args.poll_interval;

    if args.offline {
        info!("offline mode: enabled");
    } else {
        info!("server: {}", config.server.base_url);
    }

    let platform_connected_mode =
        !args.offline && config.ai_provider.access_mode == AiAccessMode::PlatformConnected;
    info!(
        access_mode = ?config.ai_provider.access_mode,
        platform_sync_enabled = platform_connected_mode,
        "evaluated platform-connected mode"
    );

    integrity_guard::run_preflight(&config, args.offline)?;

    let runtime_auto_update = config.update.auto_install || args.auto_update || args.approve_update;
    let (update_action_tx, update_action_rx) = mpsc::unbounded_channel::<UpdateAction>();
    let update_control = UpdateControl::new(
        update_action_tx.clone(),
        update_coordinator::initial_status(&config.update, runtime_auto_update),
    );

    if !args.offline && config.update.enabled {
        let update_config = config.update.clone();
        let update_state = update_control.state.clone();
        let update_status_tx = Some(update_control.event_tx.clone());
        tokio::spawn(async move {
            update_coordinator::run_update_coordinator(
                update_config,
                update_state,
                update_action_rx,
                update_status_tx,
                runtime_auto_update,
            )
            .await;
        });
        if args.approve_update {
            let _ = update_action_tx.send(UpdateAction::CheckNow);
            let _ = update_action_tx.send(UpdateAction::Approve);
        }
    }

    let token_manager = Arc::new(TokenManager::new(&config.server.base_url));

    info!(
        "network configuration: grpc_auth={}, grpc_context={}, endpoint={}",
        config.grpc.use_grpc_auth, config.grpc.use_grpc_context, config.grpc.grpc_endpoint
    );

    let grpc_config = GrpcConfig::from_core_with_rest(&config.grpc, &config.server.base_url);
    let unified_client = Arc::new(UnifiedClient::new(
        grpc_config.clone(),
        token_manager.clone(),
    )?);

    if platform_connected_mode {
        let email =
            std::env::var("ONESHIM_EMAIL").unwrap_or_else(|_| "user@example.com".to_string());
        let password = std::env::var("ONESHIM_PASSWORD").unwrap_or_default();
        let org_id = std::env::var("ONESHIM_ORG_ID").unwrap_or_else(|_| "default".to_string());

        info!("server login attempt: {email}");

        if config.grpc.use_grpc_auth {
            match unified_client.login(&email, &password, &org_id).await {
                Ok(auth_response) => {
                    info!("gRPC login success: user_id={:?}", auth_response.user_id);
                }
                Err(e) => {
                    warn!("gRPC login failure: {e}");
                    warn!("REST fallback --offline mode.");
                }
            }
        } else if let Err(e) = token_manager.login(&email, &password).await {
            warn!("login failure: {e}");
            warn!("ONESHIM_EMAIL, ONESHIM_PASSWORD settings --offline mode.");
        }
    } else {
        info!("login: standalone/ mode");
    }

    let api_client = Arc::new(HttpApiClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.request_timeout(),
    )?);

    let sse_client = Arc::new(SseStreamClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.server.sse_max_retry_secs,
    ));

    let notifier: Arc<dyn oneshim_core::ports::notifier::DesktopNotifier> =
        Arc::new(DesktopNotifierImpl::new());

    let system_monitor = Arc::new(SysInfoMonitor::new());
    let process_monitor: Arc<dyn oneshim_core::ports::monitor::ProcessMonitor> =
        Arc::new(ProcessTracker::new());
    let activity_monitor = Arc::new(ActivityTracker::new(process_monitor.clone()));

    let capture_trigger: Box<dyn oneshim_core::ports::vision::CaptureTrigger> =
        Box::new(SmartCaptureTrigger::new(config.vision.capture_throttle_ms));
    let ocr_tessdata = std::env::var("ONESHIM_TESSDATA")
        .ok()
        .map(std::path::PathBuf::from);
    let frame_processor: Box<dyn oneshim_core::ports::vision::FrameProcessor> =
        Box::new(EdgeFrameProcessor::new(
            config.vision.thumbnail_width,
            config.vision.thumbnail_height,
            ocr_tessdata,
        ));

    let db_path = resolve_db_path(args.data_dir.as_deref());
    let data_dir = db_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&data_dir)?;
    maybe_sync_cli_subscription_bridge(&config, &data_dir);

    let sqlite_storage = Arc::new(oneshim_storage::sqlite::SqliteStorage::open(
        &db_path,
        config.storage.retention_days,
    )?);
    let storage: Arc<dyn oneshim_core::ports::storage::StorageService> = sqlite_storage.clone();
    info!("SQLite save: {}", db_path.display());

    let frame_storage = oneshim_storage::frame_storage::FrameFileStorage::new(
        data_dir.clone(),
        config.storage.max_storage_mb,
        config.storage.retention_days,
    )
    .await?;
    let frame_storage = Arc::new(frame_storage);
    info!("frame save: {}", frame_storage.frames_dir().display());

    let session_id = generate_session_id();
    let batch_uploader = Arc::new(BatchUploader::new(
        api_client.clone(),
        session_id.clone(),
        100,
        3,
    ));

    let suggestion_queue = Arc::new(Mutex::new(SuggestionQueue::new(50)));
    let (suggestion_tx, mut suggestion_rx) = mpsc::channel(32);

    let receiver = SuggestionReceiver::new(
        sse_client.clone(),
        Some(notifier.clone()),
        suggestion_queue.clone(),
        suggestion_tx,
    );

    let event_bus = Arc::new(EventBus::new(128));

    let lifecycle = Arc::new(LifecycleManager::new());

    let notification_manager = Arc::new(NotificationManager::new(
        config.notification.clone(),
        notifier.clone(),
    ));

    let focus_analyzer = Arc::new(FocusAnalyzer::with_defaults(
        sqlite_storage.clone(),
        notifier.clone(),
    ));

    let offline_mode = args.offline;
    let app_config = Arc::new(tokio::sync::RwLock::new(config.clone()));
    let sched = Scheduler::new(
        SchedulerConfig {
            poll_interval: Duration::from_millis(args.poll_interval),
            metrics_interval: Duration::from_secs(5),
            process_interval: Duration::from_secs(10),
            detailed_process_interval: Duration::from_secs(30),
            input_activity_interval: Duration::from_secs(30),
            sync_interval: config.sync_interval(),
            heartbeat_interval: Duration::from_millis(config.monitor.heartbeat_interval_ms),
            aggregation_interval: Duration::from_secs(3600),
            session_id: session_id.clone(),
            offline_mode,
            ai_access_mode: config.ai_provider.access_mode,
            external_data_policy: config.ai_provider.external_data_policy,
            privacy_config: config.privacy.clone(),
            idle_threshold_secs: 300,
        },
        app_config,
        system_monitor,
        activity_monitor,
        process_monitor,
        capture_trigger,
        frame_processor,
        storage.clone(),
        sqlite_storage.clone(),
        Some(frame_storage.clone()),
        batch_uploader.clone(),
        api_client.clone(),
    )
    .with_notification_manager(notification_manager)
    .with_focus_analyzer(focus_analyzer);

    let shutdown_rx = lifecycle.subscribe();
    tokio::spawn(async move {
        sched.run(shutdown_rx).await;
    });

    let config_manager = ConfigManager::new().unwrap_or_else(|e| {
        warn!("settings initialize failure, default settings: {e}");
        let fallback_path = data_dir.join("config.json");
        ConfigManager::with_path(fallback_path).expect("failed to create config manager")
    });
    info!("settings file: {:?}", config_manager.config_path());

    let audit_logger = Arc::new(RwLock::new(AuditLogger::default()));

    let automation_controller = if config.automation.enabled {
        let runtime = build_automation_runtime(
            &config.ai_provider,
            config.privacy.pii_filter_level,
            Some(frame_storage.clone()),
        );
        match runtime {
            Ok(runtime) => {
                info!(
                    access_mode = ?runtime.access_mode,
                    ocr_provider = runtime.ocr_provider_name,
                    ocr_source = runtime.ocr_source.as_str(),
                    llm_provider = runtime.llm_provider_name,
                    llm_source = runtime.llm_source.as_str(),
                    "resolved AI provider adapters"
                );

                let policy_client = Arc::new(PolicyClient::new());
                let sandbox = create_platform_sandbox(&config.automation.sandbox);
                let mut controller = AutomationController::new(
                    policy_client,
                    audit_logger.clone(),
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
                    warn!(
                        error = %err,
                        fallback_enabled = true,
                        "failed to resolve AI provider adapters; falling back to NoOp automation executor"
                    );

                    let policy_client = Arc::new(PolicyClient::new());
                    let sandbox = create_platform_sandbox(&config.automation.sandbox);
                    let mut controller = AutomationController::new(
                        policy_client,
                        audit_logger.clone(),
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
                        "failed to resolve AI provider adapters; disabling automation controller because fallback_to_local=false"
                    );
                    None
                }
            }
        }
    } else {
        None
    };

    if config.web.enabled {
        let mut web_server = WebServer::new(sqlite_storage.clone(), config.web.clone())
            .with_frames_dir(data_dir.clone())
            .with_config_manager(config_manager)
            .with_audit_logger(audit_logger.clone())
            .with_update_control(update_control.clone());
        if let Some(ref ctrl) = automation_controller {
            web_server = web_server.with_automation_controller(ctrl.clone());
        }
        let web_shutdown_rx = lifecycle.subscribe();
        let web_port = config.web.port;
        tokio::spawn(async move {
            if let Err(e) = web_server.run(web_shutdown_rx).await {
                error!("server error: {e}");
            }
        });
        info!(": http://localhost:{}", web_port);
    }

    if platform_connected_mode {
        let sid = session_id.clone();
        tokio::spawn(async move {
            if let Err(e) = receiver.run(&sid).await {
                error!("suggestion received error: {e}");
            }
        });

        let bus = event_bus.clone();
        tokio::spawn(async move {
            while let Some(suggestion) = suggestion_rx.recv().await {
                info!(
                    "new suggestion: [{:?}] {} (confidence {:.0}%)",
                    suggestion.priority,
                    suggestion.content,
                    suggestion.confidence_score * 100.0
                );
                bus.publish(crate::event_bus::AppEvent::SuggestionReceived(suggestion));
            }
        });
    }

    if args.offline {
        info!("ONESHIM offline mode execution in progress (Ctrl+C ended)");
        info!("monitoring: {}ms", args.poll_interval);
    } else if platform_connected_mode {
        info!("ONESHIM client execution in progress (, Ctrl+C ended)");
    } else {
        info!("ONESHIM client execution in progress (standalone mode, Ctrl+C ended)");
    }

    lifecycle.wait_for_signal().await;

    info!("ONESHIM client ended");
    Ok(())
}
