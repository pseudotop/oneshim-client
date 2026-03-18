#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Allow `cfg(feature = "cargo-clippy")` inside `objc::msg_send!` macro from the `objc` crate.
// See: https://doc.rust-lang.org/nightly/rustc/check-cfg/cargo-specifics.html
#![allow(unexpected_cfgs)]

//! ONESHIM Desktop Agent — Tauri v2 진입점
//!
//! iced GUI에서 Tauri v2로 마이그레이션된 데스크톱 에이전트.
//! 시스템 트레이, WebView 대시보드, IPC 커맨드를 통합 관리합니다.

mod agent_runtime;
mod agent_runtime_support;
mod app_runtime_launch;
mod auth_cli;
mod automation_controller_builder;
mod automation_runtime;
mod autostart;
mod background_runtime;
mod bootstrap_preflight;
mod bootstrap_runtime;
mod bridge_cli;
mod cli_subscription_bridge;
mod commands;
mod desktop_startup;
#[cfg(any(feature = "server", test))]
#[cfg(feature = "server")]
mod event_bus;
mod feature_capabilities;
mod focus_analyzer;
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
#[cfg(target_os = "macos")]
mod macos_integration;
mod memory_profiler;
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
mod setup;
mod skill_loader;
mod storage_runtime;
mod subprocess_provider;
mod tray;
mod update_coordinator;
mod update_runtime;
mod updater;
mod web_server_runtime;
mod workflow_intelligence;

use tauri::{Manager, RunEvent};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("oneshim=info,oneshim_app=info,oneshim_core=info,oneshim_monitor=info,oneshim_vision=info,oneshim_storage=info,oneshim_network=info,oneshim_suggestion=info")
            }),
        )
        .init();

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
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_notification::init());

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
            commands::get_metrics,
            commands::get_settings,
            commands::update_setting,
            commands::get_update_status,
            commands::approve_update,
            commands::defer_update,
            commands::get_automation_status,
            commands::get_web_port,
            commands::get_secret_backend_capabilities,
            commands::get_feature_capabilities,
            commands::probe_provider_surface_endpoint,
            commands::get_allowed_setting_keys,
            commands::integration_auth_status,
            commands::integration_start_device_authorization,
            commands::integration_poll_device_authorization,
            commands::integration_cancel_device_authorization,
            commands::integration_reset_auth_state,
            commands::oauth_start_flow,
            commands::oauth_flow_status,
            commands::oauth_cancel_flow,
            commands::oauth_revoke,
            commands::oauth_connection_status,
            commands::get_analysis_config,
            commands::update_analysis_config,
            commands::get_analysis_status,
            commands::semantic_search,
            commands::get_weekly_digest,
            commands::get_dashboard_day,
            commands::get_daily_digest,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ONESHIM");

    app.run(|app_handle, event| match event {
        RunEvent::Exit => {
            info!("Tauri exit: sending shutdown signal");
            if let Some(state) = app_handle.try_state::<runtime_state::AppState>() {
                if state.shutdown_tx.send(true).is_err() {
                    warn!("shutdown signal send failed (receivers already dropped)");
                }
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            // macOS dock 아이콘 클릭 시 메인 윈도우 표시
            if let Some(w) = app_handle.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        _ => {}
    });
}
