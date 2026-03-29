use anyhow::Result;
use tauri::{App, Manager};
use tracing::info;

use crate::app_runtime_launch::{AppRuntimeLaunchBuilder, AppRuntimeLaunchResult};
use crate::bootstrap_runtime::BootstrapRuntimeBuilder;
use crate::desktop_startup::DesktopStartupCoordinator;

/// Tauri setup 함수 — gui_runner.rs의 Agent + WebServer 초기화 이전
pub fn init(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let _app_handle = app.handle().clone();
    info!("Tauri setup: initializing ONESHIM agent");
    let AppRuntimeLaunchResult {
        frontend_web_port,
        state_builder,
    } = AppRuntimeLaunchBuilder::new(BootstrapRuntimeBuilder::new().build()?, _app_handle.clone())
        .build_and_spawn()?;

    state_builder.build().register_on(app);

    // Register global overlay toggle shortcut (Cmd+Shift+O / Ctrl+Shift+O)
    register_overlay_shortcut(app);

    // Register capture toggle shortcut (Cmd+Shift+\ / Ctrl+Shift+\)
    register_capture_shortcut(app);

    // Register suggestions panel toggle shortcut (Cmd+Shift+S / Ctrl+Shift+S)
    register_suggestions_shortcut(app);

    // Register detection overlay toggle shortcut (Cmd+Shift+D / Ctrl+Shift+D)
    register_detection_shortcut(app);

    // Register detection overlay refresh shortcut (Cmd+Shift+R / Ctrl+Shift+R)
    register_detection_refresh_shortcut(app);

    // 12. Desktop shell startup
    DesktopStartupCoordinator::apply(app, frontend_web_port)?;

    // Create tracking panel window (starts hidden, auto-shown if indicator is configured visible)
    let _ = crate::magic_overlay::create_tracking_panel(&_app_handle);
    if let Some(state) = _app_handle.try_state::<crate::runtime_state::AppState>() {
        if state
            .indicator_visible
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            if let Some(panel) = _app_handle.get_webview_window("tracking-panel") {
                let _ = panel.show();
            }
        }
    }

    // Pre-create MagicOverlay window (hidden). Shown on-demand when coaching
    // popups, suggestions, or focus highlights are triggered.
    // NOTE: Do NOT show() here — a visible full-screen overlay at the same
    // window level blocks tracking panel drag on macOS.
    if let Some(state) = _app_handle.try_state::<crate::runtime_state::AppState>() {
        if let Some(ref overlay) = state.magic_overlay {
            let _ = overlay.ensure_window();
        }
    }

    // Create native border indicator (macOS only, main thread)
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        if let Some(mtm) = MainThreadMarker::new() {
            info!("Native border: MainThreadMarker acquired");
            match crate::native_border::NativeBorderIndicator::new(mtm) {
                Some(border) => {
                    let border = std::sync::Arc::new(border);
                    let visible = _app_handle
                        .try_state::<crate::runtime_state::AppState>()
                        .map(|s| {
                            s.indicator_visible
                                .load(std::sync::atomic::Ordering::Relaxed)
                        })
                        .unwrap_or(false);
                    if visible {
                        border.show();
                    }

                    // Spawn periodic screen topology monitor (every 5s)
                    let border_for_task = border.clone();
                    tauri::async_runtime::spawn(async move {
                        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                        loop {
                            interval.tick().await;
                            border_for_task.check_and_rebuild();
                        }
                    });

                    app.manage(crate::native_border::NativeBorderState(border));
                    info!("Native border indicator created (visible={visible})");
                }
                None => {
                    tracing::warn!("Native border: window creation failed");
                }
            }
        } else {
            tracing::warn!("Native border: not on main thread");
        }
    }

    // macOS: enable native drag on tracking panel (workaround for -webkit-app-region
    // not working on transparent borderless windows with WKWebView)
    #[cfg(target_os = "macos")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Some(panel) = _app_handle.get_webview_window("tracking-panel") {
            if let Ok(handle) = panel.window_handle() {
                if let RawWindowHandle::AppKit(appkit) = handle.as_raw() {
                    let ns_view =
                        unsafe { &*(appkit.ns_view.as_ptr() as *const objc2_app_kit::NSView) };
                    if let Some(ns_window) = ns_view.window() {
                        ns_window.setMovableByWindowBackground(true);
                        info!("Tracking panel: movableByWindowBackground enabled");
                    }
                }
            }
        }
    }

    info!("Tauri setup complete");
    Ok(())
}

/// Register Cmd+Shift+\ (macOS) / Ctrl+Shift+\ (Windows/Linux) to toggle
/// capture pause state. Emits state change events to overlay and tracking panel,
/// and rebuilds the tray menu to reflect the new state.
fn register_capture_shortcut(app: &App) {
    use tauri::{Emitter, Manager};
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let result =
        app.global_shortcut()
            .on_shortcut("CmdOrCtrl+Shift+\\", |app_handle, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let handle = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(state) = handle.try_state::<crate::runtime_state::AppState>() {
                            let was_paused = state
                                .capture_paused
                                .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                            let new_paused = !was_paused;
                            let indicator_visible = state
                                .indicator_visible
                                .load(std::sync::atomic::Ordering::Relaxed);
                            let payload = serde_json::json!({
                                "paused": new_paused,
                                "indicator_visible": indicator_visible
                            });
                            let _ = handle.emit_to(
                                "magic-overlay",
                                "overlay:capture-state-changed",
                                &payload,
                            );
                            let _ = handle.emit_to(
                                "tracking-panel",
                                "overlay:capture-state-changed",
                                &payload,
                            );
                            let _ = crate::tray::sync_tray_state(
                                &handle,
                                new_paused,
                                indicator_visible,
                            );
                            #[cfg(target_os = "macos")]
                            if let Some(border) =
                                handle.try_state::<crate::native_border::NativeBorderState>()
                            {
                                border.0.set_paused(new_paused);
                            }
                        }
                    });
                }
            });

    match result {
        Ok(()) => info!("Global shortcut registered: CmdOrCtrl+Shift+\\ (capture toggle)"),
        Err(e) => tracing::warn!("Failed to register capture shortcut: {e}"),
    }
}

/// Register Cmd+Shift+O (macOS) / Ctrl+Shift+O (Windows/Linux) to toggle
/// the MagicOverlay into interactive mode so users can click coaching popup buttons.
/// The overlay reverts to click-through automatically on dismiss or auto-dismiss timeout.
fn register_overlay_shortcut(app: &App) {
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let result =
        app.global_shortcut()
            .on_shortcut("CmdOrCtrl+Shift+O", |app_handle, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let handle = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, crate::runtime_state::AppState> =
                            handle.state();
                        if let Some(ref overlay) = state.magic_overlay {
                            overlay.set_interactive(true);
                        }
                    });
                }
            });

    match result {
        Ok(()) => info!("Global shortcut registered: CmdOrCtrl+Shift+O (overlay toggle)"),
        Err(e) => tracing::warn!("Failed to register overlay shortcut: {e}"),
    }
}

/// Register Cmd+Shift+S (macOS) / Ctrl+Shift+S (Windows/Linux) to toggle
/// the suggestions panel in the MagicOverlay. Makes the overlay interactive
/// so the user can accept/reject/defer suggestions.
fn register_suggestions_shortcut(app: &App) {
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let result =
        app.global_shortcut()
            .on_shortcut("CmdOrCtrl+Shift+S", |app_handle, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let handle = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Some(state) = handle.try_state::<crate::runtime_state::AppState>() {
                            if let Some(ref overlay) = state.magic_overlay {
                                overlay.emit_toggle_suggestions();
                                overlay.set_interactive(true);
                            }
                        }
                    });
                }
            });

    match result {
        Ok(()) => info!("Global shortcut registered: CmdOrCtrl+Shift+S (suggestions panel)"),
        Err(e) => tracing::warn!("Failed to register suggestions shortcut: {e}"),
    }
}

fn register_detection_shortcut(app: &App) {
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    if let Err(e) =
        app.global_shortcut()
            .on_shortcut("CmdOrCtrl+Shift+D", |app_handle, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let handle = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, crate::runtime_state::AppState> =
                            handle.state();
                        let was_active = state
                            .detection_active
                            .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                        let now_active = !was_active;

                        if now_active {
                            tracing::info!("detection overlay toggled ON via shortcut");
                            if let Some(ref overlay) = state.magic_overlay {
                                overlay.set_interactive(true);
                            }
                            crate::commands::detection::spawn_detection_analysis_from_state(&state);
                        } else {
                            tracing::info!("detection overlay toggled OFF via shortcut");
                            if let Some(ref overlay) = state.magic_overlay {
                                overlay.clear_detection_scene().await;
                                overlay.set_interactive(false);
                            }
                        }
                    });
                }
            })
    {
        tracing::warn!("failed to register detection shortcut: {e}");
    }
}

fn register_detection_refresh_shortcut(app: &App) {
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    if let Err(e) =
        app.global_shortcut()
            .on_shortcut("CmdOrCtrl+Shift+R", |app_handle, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let handle = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, crate::runtime_state::AppState> =
                            handle.state();
                        if state
                            .detection_active
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            tracing::info!("detection overlay refresh via shortcut");
                            crate::commands::detection::spawn_detection_analysis_from_state(&state);
                        }
                    });
                }
            })
    {
        tracing::warn!("failed to register detection refresh shortcut: {e}");
    }
}

#[cfg(test)]
mod tests {
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

    /// desktop startup helper에 window.show() 호출이 포함되어 있는지 정적 검증.
    /// 향후 리팩토링 시 show() 호출이 실수로 제거되는 것을 방지.
    #[test]
    fn desktop_startup_contains_window_show_call() {
        let setup_src = include_str!("desktop_startup.rs");

        assert!(
            setup_src.contains("window.show()"),
            "desktop startup must call window.show() — without this, the GUI window is invisible on launch"
        );
        assert!(
            setup_src.contains("window.set_focus()"),
            "desktop startup must call window.set_focus() after show()"
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
}
