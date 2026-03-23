use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, Runtime,
};
use tracing::{info, warn};

fn focus_main_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        window.show().unwrap_or_default();
        window.set_focus().unwrap_or_default();
    }
}

/// 시스템 트레이 메뉴 설정 — 아이콘 + 메뉴 + 이벤트 핸들러 통합.
/// tauri.conf.json의 trayIcon은 null로 설정하고 여기서 전부 처리.
pub fn setup_tray<R: Runtime>(app: &tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
    // Read initial state from AppState (config-driven, registered before tray setup)
    let (paused, indicator_visible) = app
        .try_state::<crate::runtime_state::AppState>()
        .map(|s| {
            (
                s.capture_paused.load(std::sync::atomic::Ordering::Relaxed),
                s.indicator_visible
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
        })
        .unwrap_or((false, true));

    let status_text = if paused {
        "⏸ Paused"
    } else {
        "🟢 Capturing"
    };
    let toggle_text = if paused {
        "Resume Capture"
    } else {
        "Pause Capture"
    };
    let indicator_text = if indicator_visible {
        "Hide Indicator"
    } else {
        "Show Indicator"
    };

    // Status items (prepended before existing menu items)
    let capture_status =
        MenuItem::with_id(app, "capture-status", status_text, false, None::<&str>)?;
    let toggle_capture = MenuItem::with_id(app, "toggle-capture", toggle_text, true, None::<&str>)?;
    let toggle_indicator =
        MenuItem::with_id(app, "toggle-indicator", indicator_text, true, None::<&str>)?;
    let status_sep = PredefinedMenuItem::separator(app)?;

    // Existing menu items
    let show = MenuItem::with_id(app, "show", "Toggle Window", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let automation =
        MenuItem::with_id(app, "automation", "Automation Settings", true, None::<&str>)?;
    let approve = MenuItem::with_id(app, "approve_update", "Apply Update", true, None::<&str>)?;
    let defer = MenuItem::with_id(app, "defer_update", "Defer Update", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &capture_status,
            &toggle_capture,
            &toggle_indicator,
            &status_sep,
            &show,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &automation,
            &approve,
            &defer,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // Initial icon with status dot (non-template for color support)
    let (rgba, w, h) = crate::tray_icon::status_icon(paused);
    let initial_icon = Image::new_owned(rgba, w, h);

    TrayIconBuilder::with_id("main-tray")
        .icon(initial_icon)
        .icon_as_template(false)
        .menu(&menu)
        .tooltip("ONESHIM")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle-capture" => {
                if let Some(state) = app.try_state::<crate::runtime_state::AppState>() {
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
                    let _ = app.emit_to("magic-overlay", "overlay:capture-state-changed", &payload);
                    let _ =
                        app.emit_to("tracking-panel", "overlay:capture-state-changed", &payload);
                    let _ = sync_tray_state(app, new_paused, indicator_visible);
                }
            }
            "toggle-indicator" => {
                if let Some(state) = app.try_state::<crate::runtime_state::AppState>() {
                    let was_visible = state
                        .indicator_visible
                        .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
                    let new_visible = !was_visible;
                    let paused = state
                        .capture_paused
                        .load(std::sync::atomic::Ordering::Relaxed);
                    let payload = serde_json::json!({
                        "paused": paused,
                        "indicator_visible": new_visible
                    });
                    let _ = app.emit_to("magic-overlay", "overlay:capture-state-changed", &payload);
                    let _ =
                        app.emit_to("tracking-panel", "overlay:capture-state-changed", &payload);
                    if let Some(panel) = app.get_webview_window("tracking-panel") {
                        if new_visible {
                            let _ = panel.show();
                        } else {
                            let _ = panel.hide();
                        }
                    }
                    let _ = sync_tray_state(app, paused, new_visible);
                }
            }
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        w.hide().unwrap_or_default();
                    } else {
                        w.show().unwrap_or_default();
                        w.set_focus().unwrap_or_default();
                    }
                }
            }
            "settings" => {
                focus_main_window(app);
                app.emit_to("main", "navigate", "/settings")
                    .unwrap_or_default();
            }
            "automation" => {
                focus_main_window(app);
                app.emit_to("main", "tray-toggle-automation", ())
                    .unwrap_or_default();
            }
            "approve_update" => {
                focus_main_window(app);
                if let Some(state) = app.try_state::<crate::runtime_state::AppState>() {
                    use oneshim_web::update_control::UpdateAction;
                    if let Err(e) = state.update_action_tx.send(UpdateAction::Approve) {
                        warn!("tray: approve_update send failed: {e}");
                    }
                }
                app.emit_to("main", "tray-approve-update", ())
                    .unwrap_or_default();
            }
            "defer_update" => {
                focus_main_window(app);
                if let Some(state) = app.try_state::<crate::runtime_state::AppState>() {
                    use oneshim_web::update_control::UpdateAction;
                    if let Err(e) = state.update_action_tx.send(UpdateAction::Defer) {
                        warn!("tray: defer_update send failed: {e}");
                    }
                }
                app.emit_to("main", "tray-defer-update", ())
                    .unwrap_or_default();
            }
            "quit" => {
                info!("tray: quit requested");
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    info!("system tray initialized with status icon");
    Ok(())
}

/// Update tray icon and menu to reflect current capture state.
///
/// Combines icon swap (colored status dot) with menu text rebuild.
/// Called from tray event handlers and IPC commands.
pub fn sync_tray_state<R: Runtime>(
    app: &tauri::AppHandle<R>,
    paused: bool,
    indicator_visible: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(tray) = app.tray_by_id("main-tray") {
        // Update icon with status dot
        let (rgba, w, h) = crate::tray_icon::status_icon(paused);
        let icon = Image::new_owned(rgba, w, h);
        tray.set_icon(Some(icon))?;
        tray.set_icon_as_template(false)?;

        // Rebuild menu text
        let status_text = if paused {
            "⏸ Paused"
        } else {
            "🟢 Capturing"
        };
        let toggle_text = if paused {
            "Resume Capture"
        } else {
            "Pause Capture"
        };
        let indicator_text = if indicator_visible {
            "Hide Indicator"
        } else {
            "Show Indicator"
        };

        // Recreate all menu items (immutable after creation in Tauri v2)
        let capture_status =
            MenuItem::with_id(app, "capture-status", status_text, false, None::<&str>)?;
        let toggle_capture =
            MenuItem::with_id(app, "toggle-capture", toggle_text, true, None::<&str>)?;
        let toggle_indicator =
            MenuItem::with_id(app, "toggle-indicator", indicator_text, true, None::<&str>)?;
        let status_sep = PredefinedMenuItem::separator(app)?;

        let show = MenuItem::with_id(app, "show", "Toggle Window", true, None::<&str>)?;
        let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
        let automation =
            MenuItem::with_id(app, "automation", "Automation Settings", true, None::<&str>)?;
        let approve = MenuItem::with_id(app, "approve_update", "Apply Update", true, None::<&str>)?;
        let defer = MenuItem::with_id(app, "defer_update", "Defer Update", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

        let menu = Menu::with_items(
            app,
            &[
                &capture_status,
                &toggle_capture,
                &toggle_indicator,
                &status_sep,
                &show,
                &PredefinedMenuItem::separator(app)?,
                &settings,
                &automation,
                &approve,
                &defer,
                &PredefinedMenuItem::separator(app)?,
                &quit,
            ],
        )?;

        tray.set_menu(Some(menu))?;
    }
    Ok(())
}
