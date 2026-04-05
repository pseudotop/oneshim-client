#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unexpected_cfgs)]
// Cast safety: UI metrics, scheduler counters, coordinates — precision loss acceptable.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

//! ONESHIM Desktop Agent — Tauri v2 진입점
//!
//! iced GUI에서 Tauri v2로 마이그레이션된 데스크톱 에이전트.
//! 시스템 트레이, WebView 대시보드, IPC 커맨드를 통합 관리합니다.

mod agent_runtime;
mod agent_runtime_support;
mod app_runtime_launch;
mod auditing_session;
mod auth_cli;
mod automation_controller_builder;
mod automation_runtime;
mod autostart;
mod background_runtime;
mod bootstrap_preflight;
mod bootstrap_runtime;
mod bridge_cli;
mod capture_services;
mod cli_subscription_bridge;
mod commands;
mod desktop_permissions;
mod desktop_startup;
mod fallback_stt;
mod feature_capabilities;
mod focus_analyzer;
mod focus_mode;
mod focus_probe_adapter;
#[cfg(feature = "server")]
mod integration_insight_source;
mod integration_policy;
#[cfg(feature = "server")]
mod integration_prompt_delivery;
#[cfg(feature = "server")]
mod integration_runtime;
mod integrity_guard;
mod launch_resources;
mod lifecycle;
mod log_retention;
#[cfg(target_os = "macos")]
mod macos_integration;
mod magic_overlay;
mod magic_overlay_driver;
mod memory_profiler;
mod native_border;
mod notification_manager;
mod oauth_provider_registry;
mod platform_accessibility;
mod platform_overlay;
mod provider_adapters;
mod provider_secret_backend;
mod runtime_bridges;
mod runtime_state;
mod scheduler;
mod secret_cli;
#[cfg(feature = "server")]
mod server_runtime_context;
mod services;
mod session_adapters;
mod session_context;
mod session_manager;
mod setup;
mod setup_platform;
mod setup_shortcuts;
mod setup_windows;
mod skill_loader;
mod storage_runtime;
mod subprocess_provider;
mod suggestion_manager;
mod sync_engine;
mod tray;
mod tray_icon;
mod update_coordinator;
mod update_runtime;
mod updater;
mod web_server_runtime;
mod workflow_intelligence;

use tauri::{Manager, RunEvent};
#[cfg(target_os = "macos")]
use tracing::debug;
use tracing::{info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Wrapper for `tracing_appender::non_blocking::WorkerGuard`.
///
/// Stored as Tauri managed state so it is dropped (and flushed) when the
/// app exits rather than leaked.  The inner field is intentionally never
/// read — its purpose is to keep the guard alive for the duration of the
/// process.
#[allow(dead_code)] // RAII: inner guard kept alive for log flushing on Drop
pub(crate) struct LogWorkerGuard(tracing_appender::non_blocking::WorkerGuard);

fn main() {
    // Windows DLL search order hardening (Spec Section 9.2):
    // Remove CWD from DLL search path to prevent DLL hijacking.
    #[cfg(target_os = "windows")]
    unsafe {
        windows_sys::Win32::System::LibraryLoader::SetDllDirectoryW(windows_sys::core::w!(""));
    }

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("oneshim=info,oneshim_app=info,oneshim_core=info,oneshim_monitor=info,oneshim_vision=info,oneshim_storage=info,oneshim_network=info,oneshim_suggestion=info")
    });

    // Console layer — writes to stderr (same as previous fmt() subscriber).
    let console_layer = tracing_subscriber::fmt::layer().with_ansi(true);

    // File layer — daily rolling log files in {data_dir}/logs/.
    // WorkerGuard MUST outlive the subscriber; we store it in Tauri state.
    let log_dir = oneshim_core::config_manager::ConfigManager::data_dir()
        .map(|d| d.join("logs"))
        .unwrap_or_else(|_| std::path::PathBuf::from("logs"));

    std::fs::create_dir_all(&log_dir).ok();

    // Cleanup old log files before creating new appender
    let deleted = log_retention::cleanup_old_logs(&log_dir, log_retention::DEFAULT_MAX_AGE_DAYS);
    if deleted > 0 {
        // Cannot use tracing yet — subscriber not initialized.
        eprintln!("[oneshim] startup log cleanup: deleted {deleted} old log file(s)");
    }

    let file_appender = tracing_appender::rolling::daily(&log_dir, "oneshim.log");
    let (non_blocking, worker_guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    info!(log_dir = %log_dir.display(), "persistent file logging initialized");

    // CLI pre-dispatch: handle "auth" subcommand before Tauri boot
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "auth" {
        let config_dir = oneshim_core::config_manager::ConfigManager::config_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let exit_code = auth_cli::run(&args[2..], &config_dir);
        std::process::exit(exit_code);
    }
    if args.len() > 1 && args[1] == "secret" {
        let config_dir = oneshim_core::config_manager::ConfigManager::config_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let exit_code = secret_cli::run(&args[2..], &config_dir);
        std::process::exit(exit_code);
    }
    if args.len() > 1 && args[1] == "bridge" {
        let data_dir = oneshim_core::config_manager::ConfigManager::data_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let exit_code = bridge_cli::run(&args[2..], &data_dir);
        std::process::exit(exit_code);
    }

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(LogWorkerGuard(worker_guard));

    // WebDriver 서버 플러그인 — E2E 테스트용 (production 빌드에 절대 포함 금지)
    #[cfg(feature = "webdriver")]
    {
        let port = std::env::var("TAURI_WEBDRIVER_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(4445);
        info!("WebDriver plugin enabled on port {port}");
        builder = builder.plugin(tauri_plugin_webdriver::init_with_port(port));
    }

    let app = builder
        .setup(setup::init)
        .on_window_event(|window, event| {
            // Close-to-tray: 윈도우 닫기 시 숨기기 (실제 종료 아님)
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap_or_default();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::update_setting,
            commands::system::get_automation_status,
            commands::settings::get_web_port,
            commands::system::get_secret_backend_capabilities,
            commands::system::get_feature_capabilities,
            commands::system::get_runtime_log_snapshot,
            commands::system::record_frontend_log,
            commands::permissions::get_desktop_permission_status,
            commands::permissions::request_desktop_notification_permission,
            commands::permissions::open_desktop_permission_settings,
            commands::system::probe_provider_surface_endpoint,
            commands::system::preview_update,
            commands::settings::get_allowed_setting_keys,
            commands::integration::integration_auth_status,
            commands::integration::integration_start_device_authorization,
            commands::integration::integration_poll_device_authorization,
            commands::integration::integration_cancel_device_authorization,
            commands::integration::integration_reset_auth_state,
            commands::integration::oauth_start_flow,
            commands::integration::oauth_flow_status,
            commands::integration::oauth_cancel_flow,
            commands::integration::oauth_revoke,
            commands::integration::oauth_connection_status,
            commands::ai_session::create_ai_session,
            commands::ai_session::send_session_message,
            commands::ai_session::kill_ai_session,
            commands::ai_session::list_ai_sessions,
            commands::ai_session::retry_ai_session,
            commands::ai_session::get_token_usage,
            commands::ai_session::load_session_messages,
            commands::ai_session::delete_session_history,
            commands::analysis::get_analysis_config,
            commands::analysis::update_analysis_config,
            commands::analysis::get_analysis_status,
            commands::dashboard::semantic_search,
            commands::dashboard::get_weekly_digest,
            commands::dashboard::get_dashboard_day,
            commands::dashboard::get_daily_digest,
            commands::dashboard::create_override,
            commands::dashboard::delete_override,
            commands::dashboard::list_overrides,
            commands::dashboard::trigger_recluster,
            commands::coaching::dismiss_coaching_message,
            commands::coaching::submit_coaching_feedback,
            commands::coaching::set_overlay_mode,
            commands::coaching::toggle_overlay_mode,
            commands::coaching::get_overlay_state,
            commands::coaching::toggle_overlay_interactive,
            commands::coaching::get_coaching_history,
            commands::coaching::get_goal_progress,
            commands::coaching::update_regime_goals,
            commands::capture_status::get_capture_status,
            commands::capture_status::toggle_capture_pause,
            commands::capture_status::set_indicator_visible,
            commands::capture_status::get_connection_status,
            commands::capture_status::show_main_window,
            commands::capture_status::open_devtools,
            commands::capture_status::save_panel_position,
            commands::capture_status::get_panel_position,
            commands::onboarding::get_onboarding_status,
            commands::onboarding::complete_onboarding,
            commands::onboarding::reset_onboarding,
            commands::focus::toggle_focus_mode,
            commands::focus::get_focus_mode_status,
            commands::capture::trigger_manual_capture,
            commands::capture::analyze_current_scene,
            commands::suggestions::get_pending_suggestions,
            commands::suggestions::get_suggestion_history,
            commands::suggestions::submit_suggestion_feedback,
            commands::suggestions::request_chat_suggestions,
            commands::suggestions::explain_suggestion_in_chat,
            commands::suggestions::save_suggestion_state,
            commands::suggestions::get_suggestion_stats,
            commands::suggestions::get_deferred_suggestions,
            commands::suggestions::get_suggestion_daily_stats,
            commands::sync::get_sync_status,
            commands::sync::trigger_sync_cycle,
            commands::sync::discover_sync_peers,
            commands::automation::check_automation_available,
            commands::automation::list_automation_presets,
            commands::automation::run_automation_preset,
            commands::automation::execute_automation_hint,
            commands::automation::analyze_automation_scene,
            commands::automation::get_pending_confirmations,
            commands::automation::confirm_automation_command,
            commands::detection::toggle_detection_overlay,
            commands::detection::refresh_detection_overlay,
            commands::audio::start_audio_capture,
            commands::audio::stop_and_transcribe,
            commands::audio::get_audio_status,
            commands::audio::download_whisper_model,
            commands::audio::cancel_model_download,
            commands::audio::delete_whisper_model,
            commands::audio::reload_stt_engine,
            commands::audio::start_vad_listening,
            commands::audio::stop_vad_listening,
            commands::bug_report::export_bug_report,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ONESHIM");

    app.run(|app_handle, event| match event {
        RunEvent::Exit => {
            info!("Tauri exit: sending shutdown signal");

            // Persist suggestion queue before shutdown (best-effort).
            if let Some(srs) = app_handle.try_state::<runtime_state::SuggestionRuntimeState>() {
                if let Some(ref mgr) = srs.manager() {
                    let storage = mgr.storage();
                    // Save pending queue items.
                    if let Ok(queue) = mgr.queue().try_lock() {
                        for suggestion in queue.iter() {
                            if let Err(e) = storage.save_suggestion_with_state(suggestion, "pending", None) {
                                warn!(id = %suggestion.suggestion_id, "shutdown: failed to persist suggestion: {e}");
                            }
                        }
                    }
                    // Save deferred items with their resurface time.
                    if let Ok(deferred) = mgr.deferred().try_lock() {
                        for entry in deferred.list_deferred() {
                            let resurface = entry.resurface_at.to_rfc3339();
                            if let Err(e) = storage.save_suggestion_with_state(
                                &entry.suggestion,
                                "deferred",
                                Some(&resurface),
                            ) {
                                warn!(id = %entry.suggestion.suggestion_id, "shutdown: failed to persist deferred suggestion: {e}");
                            }
                        }
                    }
                }
            }

            if let Some(state) = app_handle.try_state::<runtime_state::AppState>() {
                // Terminate all active AI sessions before shutdown.
                if let Some(ai_session_state) =
                    app_handle.try_state::<runtime_state::AiSessionRuntimeState>()
                {
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        handle.block_on(async { ai_session_state.shutdown_all().await });
                    }
                }
                if state.shutdown_tx.send(true).is_err() {
                    warn!("shutdown signal send failed (receivers already dropped)");
                }
                state.background_runtime.shutdown_blocking();
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            // macOS dock 아이콘 클릭 시 메인 윈도우 표시
            if let Some(w) = app_handle.get_webview_window("main") {
                if let Err(e) = w.show() {
                    debug!("window show failed: {e}");
                }
                if let Err(e) = w.set_focus() {
                    debug!("set_focus failed: {e}");
                }
            }
        }
        _ => {}
    });
}
