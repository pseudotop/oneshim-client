//! # oneshim-app
//!
//! ONESHIM 클라이언트 바이너리 진입점.
//! DI 컨테이너 역할, 라이프사이클 관리, 스케줄러 오케스트레이션.

mod automation_runtime;
mod autostart;
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

use anyhow::Result;
use clap::Parser;
use directories::ProjectDirs;
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::controller::AutomationController;
use oneshim_automation::policy::PolicyClient;
use oneshim_automation::sandbox::create_platform_sandbox;
use oneshim_core::config::AppConfig;
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
use crate::event_bus::EventBus;
use crate::focus_analyzer::FocusAnalyzer;
use crate::lifecycle::LifecycleManager;
use crate::notification_manager::NotificationManager;
use crate::scheduler::{Scheduler, SchedulerConfig};

/// ONESHIM 데스크톱 클라이언트
///
/// AI 기반 자율 사무 업무 지원 에이전트
#[derive(Parser, Debug)]
#[command(name = "oneshim")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 오프라인 모드로 실행 (서버 연결 없이 로컬 기능만 사용)
    #[arg(long, short = 'o')]
    offline: bool,

    /// 서버 URL 지정 (기본: http://localhost:8000)
    #[arg(long, short = 's')]
    server: Option<String>,

    /// 로그 레벨 (trace, debug, info, warn, error)
    #[arg(long, short = 'l', default_value = "info")]
    log_level: String,

    /// 모니터링 간격 (밀리초)
    #[arg(long, default_value = "1000")]
    poll_interval: u64,

    /// 데이터 저장 경로 (기본: 인메모리)
    #[arg(long)]
    data_dir: Option<String>,

    /// 로그인 시 자동 시작 활성화 (macOS/Windows)
    #[arg(long)]
    enable_autostart: bool,

    /// 로그인 시 자동 시작 비활성화
    #[arg(long)]
    disable_autostart: bool,

    /// 자동 시작 상태 확인
    #[arg(long)]
    autostart_status: bool,

    /// GUI 모드로 실행 (iced 윈도우)
    #[arg(long, short = 'g')]
    gui: bool,

    #[arg(long)]
    auto_update: bool,

    #[arg(long)]
    approve_update: bool,
}

/// 세션 ID 생성 -- 타임스탬프 기반
fn generate_session_id() -> String {
    use std::hash::{Hash, Hasher};

    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    let rand_part = hasher.finish() as u32;
    format!("sess_{ts}_{rand_part:08x}")
}

/// 데이터베이스 경로 결정 (CLI 인자 또는 플랫폼별 기본 경로)
///
/// # 플랫폼별 기본 경로:
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

/// 배너 출력
fn print_banner(offline: bool) {
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                                                              ║");
    println!("║   ██████╗ ███╗   ██╗███████╗███████╗██╗  ██╗██╗███╗   ███╗  ║");
    println!("║  ██╔═══██╗████╗  ██║██╔════╝██╔════╝██║  ██║██║████╗ ████║  ║");
    println!("║  ██║   ██║██╔██╗ ██║█████╗  ███████╗███████║██║██╔████╔██║  ║");
    println!("║  ██║   ██║██║╚██╗██║██╔══╝  ╚════██║██╔══██║██║██║╚██╔╝██║  ║");
    println!("║  ╚██████╔╝██║ ╚████║███████╗███████║██║  ██║██║██║ ╚═╝ ██║  ║");
    println!("║   ╚═════╝ ╚═╝  ╚═══╝╚══════╝╚══════╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝  ║");
    println!("║                                                              ║");
    if offline {
        println!("║           🔌 오프라인 모드 (로컬 전용)                        ║");
    } else {
        println!("║           AI 기반 자율 사무 업무 지원 에이전트                  ║");
    }
    println!("║                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
}

/// 오프라인 모드 안내 출력
fn print_offline_features() {
    println!("┌─────────────────────────────────────────────────────────────────┐");
    println!("│ 📊 오프라인 모드에서 사용 가능한 기능:                            │");
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!("│ ✅ 시스템 모니터링     - CPU, 메모리, 디스크 사용량 수집         │");
    println!("│ ✅ 활성 창 추적        - 현재 작업 중인 애플리케이션 감지         │");
    println!("│ ✅ 스크린샷 캡처       - 화면 캡처 및 델타 인코딩                │");
    println!("│ ✅ 로컬 데이터 저장    - SQLite에 이벤트/프레임 저장             │");
    println!("│ ✅ PII 필터링          - 민감 정보 자동 마스킹                   │");
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!("│ ❌ 서버 업로드         - 오프라인에서는 비활성화                 │");
    println!("│ ❌ AI 제안 수신        - 서버 연결 필요                         │");
    println!("│ ❌ 실시간 동기화       - 서버 연결 필요                         │");
    println!("└─────────────────────────────────────────────────────────────────┘");
    println!();
    println!("💡 서버 연결: oneshim --server http://your-server:8000");
    println!();
}

/// 자동 시작 명령 처리 (활성화/비활성화/상태 확인)
/// 명령 처리 후 true 반환 (프로그램 종료), 명령 없으면 false 반환 (계속 실행)
fn handle_autostart_commands(args: &Args) -> bool {
    // 자동 시작 상태 확인
    if args.autostart_status {
        match autostart::is_autostart_enabled() {
            Ok(enabled) => {
                if enabled {
                    println!("✅ 자동 시작: 활성화됨");
                    println!("   로그인 시 ONESHIM이 자동으로 시작됩니다.");
                } else {
                    println!("❌ 자동 시작: 비활성화됨");
                    println!("   활성화하려면: oneshim --enable-autostart");
                }
            }
            Err(e) => {
                eprintln!("⚠️  자동 시작 상태 확인 실패: {e}");
            }
        }
        return true;
    }

    // 자동 시작 활성화
    if args.enable_autostart {
        println!("🔧 자동 시작 설정 중...");
        match autostart::enable_autostart() {
            Ok(()) => {
                println!("✅ 자동 시작이 활성화되었습니다.");
                println!("   다음 로그인 시 ONESHIM이 자동으로 시작됩니다.");
                #[cfg(target_os = "macos")]
                println!("   위치: ~/Library/LaunchAgents/com.oneshim.agent.plist");
                #[cfg(target_os = "windows")]
                println!("   위치: HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run");
            }
            Err(e) => {
                eprintln!("❌ 자동 시작 활성화 실패: {e}");
                std::process::exit(1);
            }
        }
        return true;
    }

    // 자동 시작 비활성화
    if args.disable_autostart {
        println!("🔧 자동 시작 해제 중...");
        match autostart::disable_autostart() {
            Ok(()) => {
                println!("✅ 자동 시작이 비활성화되었습니다.");
                println!("   로그인 시 ONESHIM이 더 이상 자동 시작되지 않습니다.");
            }
            Err(e) => {
                eprintln!("❌ 자동 시작 비활성화 실패: {e}");
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

    // 자동 시작 명령 처리 (즉시 종료)
    if handle_autostart_commands(&args) {
        return Ok(());
    }

    // tracing 초기화 (GUI 모드 포함 모든 모드에서 필요)
    let log_filter = format!(
        "oneshim={},oneshim_app={},oneshim_ui={},oneshim_core={},oneshim_monitor={},oneshim_vision={},oneshim_storage={},oneshim_network={},oneshim_suggestion={}",
        args.log_level, args.log_level, args.log_level, args.log_level, args.log_level, args.log_level, args.log_level, args.log_level, args.log_level
    );
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_filter)),
        )
        .init();

    // GUI 모드 실행 (별도 이벤트 루프)
    if args.gui {
        return gui_runner::run_gui(args.offline, args.data_dir.as_deref());
    }

    // 배너 출력
    print_banner(args.offline);

    if args.offline {
        print_offline_features();
    }

    info!("ONESHIM 클라이언트 시작");

    // 설정 로드
    let mut config = AppConfig::default_config();

    // CLI 인자로 설정 오버라이드
    if let Some(ref server_url) = args.server {
        config.server.base_url = server_url.clone();
    }
    config.monitor.poll_interval_ms = args.poll_interval;

    if args.offline {
        info!("오프라인 모드: 로컬 기능만 활성화");
    } else {
        info!("서버: {}", config.server.base_url);
    }

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

    // ── 어댑터 생성 (DI 와이어링) ──

    // 1. 인증 (온라인 모드에서만 사용)
    let token_manager = Arc::new(TokenManager::new(&config.server.base_url));

    // gRPC 설정 로깅
    info!(
        "네트워크 설정: gRPC Auth={}, gRPC Context={}, Endpoint={}",
        config.grpc.use_grpc_auth, config.grpc.use_grpc_context, config.grpc.grpc_endpoint
    );

    // gRPC 통합 클라이언트 생성
    let grpc_config = GrpcConfig::from_core_with_rest(&config.grpc, &config.server.base_url);
    let unified_client = Arc::new(UnifiedClient::new(
        grpc_config.clone(),
        token_manager.clone(),
    )?);

    // 로그인 (오프라인 모드에서는 스킵)
    if !args.offline {
        let email =
            std::env::var("ONESHIM_EMAIL").unwrap_or_else(|_| "user@example.com".to_string());
        let password = std::env::var("ONESHIM_PASSWORD").unwrap_or_default();
        let org_id = std::env::var("ONESHIM_ORG_ID").unwrap_or_else(|_| "default".to_string());

        info!("서버 로그인 시도: {email}");

        // Feature flag에 따라 gRPC 또는 REST 로그인 사용
        if config.grpc.use_grpc_auth {
            match unified_client.login(&email, &password, &org_id).await {
                Ok(auth_response) => {
                    info!("gRPC 로그인 성공: user_id={:?}", auth_response.user_id);
                }
                Err(e) => {
                    warn!("gRPC 로그인 실패: {e}");
                    warn!("REST fallback 또는 --offline 모드를 사용하세요.");
                }
            }
        } else if let Err(e) = token_manager.login(&email, &password).await {
            warn!("로그인 실패: {e}");
            warn!("환경변수 ONESHIM_EMAIL, ONESHIM_PASSWORD를 설정하거나 --offline 모드를 사용하세요.");
        }
    }

    // 2. HTTP API 클라이언트 (REST fallback)
    let api_client = Arc::new(HttpApiClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.request_timeout(),
    )?);

    // 3. SSE 클라이언트
    let sse_client = Arc::new(SseStreamClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.server.sse_max_retry_secs,
    ));

    // 4. 데스크톱 알림
    let notifier: Arc<dyn oneshim_core::ports::notifier::DesktopNotifier> =
        Arc::new(DesktopNotifierImpl::new());

    // 5. 모니터링
    let system_monitor = Arc::new(SysInfoMonitor::new());
    let process_monitor: Arc<dyn oneshim_core::ports::monitor::ProcessMonitor> =
        Arc::new(ProcessTracker::new());
    let activity_monitor = Arc::new(ActivityTracker::new(process_monitor.clone()));

    // 6. 비전 파이프라인
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

    // 7. 스토리지 (파일 기반 SQLite)
    let db_path = resolve_db_path(args.data_dir.as_deref());
    let data_dir = db_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&data_dir)?;

    let sqlite_storage = Arc::new(oneshim_storage::sqlite::SqliteStorage::open(
        &db_path,
        config.storage.retention_days,
    )?);
    let storage: Arc<dyn oneshim_core::ports::storage::StorageService> = sqlite_storage.clone();
    info!("SQLite 저장소: {}", db_path.display());

    // 8. 프레임 파일 저장소
    let frame_storage = oneshim_storage::frame_storage::FrameFileStorage::new(
        data_dir.clone(),
        config.storage.max_storage_mb,
        config.storage.retention_days,
    )
    .await?;
    let frame_storage = Arc::new(frame_storage);
    info!("프레임 저장소: {}", frame_storage.frames_dir().display());

    // 9. 배치 업로더 (오프라인 모드에서는 noop)
    let session_id = generate_session_id();
    let batch_uploader = Arc::new(BatchUploader::new(
        api_client.clone(),
        session_id.clone(),
        100,
        3,
    ));

    // 10. 제안 수신기
    let suggestion_queue = Arc::new(Mutex::new(SuggestionQueue::new(50)));
    let (suggestion_tx, mut suggestion_rx) = mpsc::channel(32);

    let receiver = SuggestionReceiver::new(
        sse_client.clone(),
        Some(notifier.clone()),
        suggestion_queue.clone(),
        suggestion_tx,
    );

    // 11. 이벤트 버스
    let event_bus = Arc::new(EventBus::new(128));

    // 12. 라이프사이클
    let lifecycle = Arc::new(LifecycleManager::new());

    // 13. 알림 관리자
    let notification_manager = Arc::new(NotificationManager::new(
        config.notification.clone(),
        notifier.clone(),
    ));

    // 14. 집중도 분석기 (Edge Intelligence)
    let focus_analyzer = Arc::new(FocusAnalyzer::with_defaults(
        sqlite_storage.clone(),
        notifier.clone(),
    ));

    // ── 태스크 시작 ──

    // 스케줄러 (로컬 모니터링은 항상 실행)
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

    // ── 설정 관리자 + 감사 로거 (웹 대시보드 DI용) ──
    let config_manager = ConfigManager::new().unwrap_or_else(|e| {
        warn!("설정 관리자 초기화 실패, 기본 설정 사용: {e}");
        let fallback_path = data_dir.join("config.json");
        ConfigManager::with_path(fallback_path).expect("설정 관리자 생성 실패")
    });
    info!("설정 파일: {:?}", config_manager.config_path());

    let audit_logger = Arc::new(RwLock::new(AuditLogger::default()));

    // ── 자동화 컨트롤러 (config.automation.enabled일 때만) ──
    let automation_controller = if config.automation.enabled {
        let runtime = build_automation_runtime(&config.ai_provider, Some(frame_storage.clone()));
        match runtime {
            Ok(runtime) => {
                info!(
                    access_mode = ?runtime.access_mode,
                    ocr_provider = runtime.ocr_provider_name,
                    ocr_source = runtime.ocr_source.as_str(),
                    llm_provider = runtime.llm_provider_name,
                    llm_source = runtime.llm_source.as_str(),
                    "AI 제공자 어댑터 해석 완료"
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
                controller.set_intent_executor(runtime.intent_executor);
                controller.set_intent_planner(runtime.intent_planner);
                Some(Arc::new(controller))
            }
            Err(err) => {
                if config.ai_provider.fallback_to_local {
                    warn!(
                        error = %err,
                        fallback_enabled = true,
                        "AI 제공자 어댑터 해석 실패; NoOp 자동화 실행기로 폴백"
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
                        "AI 제공자 어댑터 해석 실패; fallback_to_local=false 이므로 자동화 컨트롤러를 비활성화합니다"
                    );
                    None
                }
            }
        }
    } else {
        None
    };

    // ── 웹 대시보드 서버 (config.web.enabled일 때만) ──
    if config.web.enabled {
        let mut web_server = WebServer::new(sqlite_storage.clone(), config.web.clone())
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
                error!("웹 서버 오류: {e}");
            }
        });
        info!("웹 대시보드: http://localhost:{}", web_port);
    }

    // SSE 제안 수신 (온라인 모드에서만)
    if !args.offline {
        let sid = session_id.clone();
        tokio::spawn(async move {
            if let Err(e) = receiver.run(&sid).await {
                error!("제안 수신 에러: {e}");
            }
        });

        // 제안 로깅 (터미널 출력)
        let bus = event_bus.clone();
        tokio::spawn(async move {
            while let Some(suggestion) = suggestion_rx.recv().await {
                info!(
                    "새 제안: [{:?}] {} (신뢰도 {:.0}%)",
                    suggestion.priority,
                    suggestion.content,
                    suggestion.confidence_score * 100.0
                );
                bus.publish(crate::event_bus::AppEvent::SuggestionReceived(suggestion));
            }
        });
    }

    if args.offline {
        info!("ONESHIM 오프라인 모드 실행 중 (Ctrl+C로 종료)");
        info!("로컬 모니터링 간격: {}ms", args.poll_interval);
    } else {
        info!("ONESHIM 클라이언트 실행 중 (Ctrl+C로 종료)");
    }

    // OS 시그널 대기
    lifecycle.wait_for_signal().await;

    info!("ONESHIM 클라이언트 종료");
    Ok(())
}
