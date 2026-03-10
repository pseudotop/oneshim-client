#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! ONESHIM Desktop Agent — Tauri v2 진입점
//!
//! iced GUI에서 Tauri v2로 마이그레이션된 데스크톱 에이전트.
//! 시스템 트레이, WebView 대시보드, IPC 커맨드를 통합 관리합니다.

mod automation_runtime;
mod autostart;
mod cli_subscription_bridge;
mod commands;
#[cfg(feature = "server")]
mod event_bus;
mod focus_analyzer;
mod focus_probe_adapter;
mod integrity_guard;
mod lifecycle;
#[cfg(target_os = "macos")]
mod macos_integration;
mod memory_profiler;
mod notification_manager;
mod platform_accessibility;
mod platform_overlay;
mod provider_adapters;
mod scheduler;
mod setup;
mod tray;
mod update_coordinator;
mod updater;
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

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building ONESHIM");

    app.run(|app_handle, event| match event {
        RunEvent::Exit => {
            info!("Tauri exit: sending shutdown signal");
            if let Some(state) = app_handle.try_state::<setup::AppState>() {
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
