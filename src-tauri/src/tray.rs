use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, Runtime,
};
use tracing::{debug, info, warn};

use crate::tray_icon::TrayIconState;

fn focus_main_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        window.show().unwrap_or_default();
        window.set_focus().unwrap_or_default();
    }
}

/// Read connection status flags from AppState.
///
/// Returns `(server, llm, cli)` booleans. Falls back to `(false, false, false)`
/// if AppState is not yet registered.
fn read_connection_status<R: Runtime>(app: &impl Manager<R>) -> (bool, bool, bool) {
    app.try_state::<crate::runtime_state::AppState>()
        .map(|s| {
            let ord = std::sync::atomic::Ordering::Relaxed;
            (
                s.connection.server_connected.load(ord),
                s.connection.llm_connected.load(ord),
                s.connection.cli_connected.load(ord),
            )
        })
        .unwrap_or((false, false, false))
}

/// Determine the tray icon state from capture and connection flags.
fn resolve_icon_state(paused: bool, any_disconnected: bool) -> TrayIconState {
    if paused {
        TrayIconState::Paused
    } else if any_disconnected {
        TrayIconState::Disabled
    } else {
        TrayIconState::Active
    }
}

/// Build the connection status menu items (disabled / info-only).
#[allow(clippy::type_complexity)]
fn build_connection_items<R: Runtime>(
    app: &impl Manager<R>,
    srv: bool,
    llm: bool,
    cli: bool,
) -> Result<(MenuItem<R>, MenuItem<R>, MenuItem<R>), Box<dyn std::error::Error>> {
    let srv_item = MenuItem::with_id(
        app,
        "conn-server",
        format!(
            "  Server API    {}",
            if srv { "\u{2713}" } else { "\u{2717}" }
        ),
        false,
        None::<&str>,
    )?;
    let llm_item = MenuItem::with_id(
        app,
        "conn-llm",
        format!(
            "  Local LLM     {}",
            if llm { "\u{2713}" } else { "\u{2717}" }
        ),
        false,
        None::<&str>,
    )?;
    let cli_item = MenuItem::with_id(
        app,
        "conn-cli",
        format!(
            "  CLI Bridge    {}",
            if cli { "\u{2713}" } else { "\u{2717}" }
        ),
        false,
        None::<&str>,
    )?;
    Ok((srv_item, llm_item, cli_item))
}

/// Determine status text from capture/connection state (no emoji — template icon handles visual).
fn status_text(paused: bool, any_disconnected: bool) -> &'static str {
    if paused {
        "Paused"
    } else if any_disconnected {
        "Partially Connected"
    } else {
        "Active"
    }
}

/// Build the full tray menu with connection status items.
fn build_tray_menu<R: Runtime>(
    app: &impl Manager<R>,
    paused: bool,
    indicator_visible: bool,
    srv: bool,
    llm: bool,
    cli: bool,
) -> Result<Menu<R>, Box<dyn std::error::Error>> {
    let any_disconnected = !srv || !llm || !cli;

    let capture_status = MenuItem::with_id(
        app,
        "capture-status",
        status_text(paused, any_disconnected),
        false,
        None::<&str>,
    )?;
    let (srv_item, llm_item, cli_item) = build_connection_items(app, srv, llm, cli)?;

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

    let toggle_capture = MenuItem::with_id(app, "toggle-capture", toggle_text, true, None::<&str>)?;
    let toggle_indicator =
        MenuItem::with_id(app, "toggle-indicator", indicator_text, true, None::<&str>)?;

    let show = MenuItem::with_id(app, "show", "Toggle Window", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let automation = MenuItem::with_id(
        app,
        "automation",
        "AI Automation Preferences",
        true,
        None::<&str>,
    )?;
    let run_preset = MenuItem::with_id(app, "run-preset", "Automation Page", true, None::<&str>)?;
    let approve = MenuItem::with_id(app, "approve_update", "Apply Update", true, None::<&str>)?;
    let defer = MenuItem::with_id(app, "defer_update", "Defer Update", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &capture_status,
            &srv_item,
            &llm_item,
            &cli_item,
            &PredefinedMenuItem::separator(app)?,
            &toggle_capture,
            &toggle_indicator,
            &PredefinedMenuItem::separator(app)?,
            &show,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &automation,
            &run_preset,
            &approve,
            &defer,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    Ok(menu)
}

/// System tray setup — template icon + menu + event handler.
/// tauri.conf.json trayIcon is null; everything is handled here.
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

    let (srv, llm, cli) = read_connection_status(app);
    let any_disconnected = !srv || !llm || !cli;

    let icon_state = resolve_icon_state(paused, any_disconnected);
    let (rgba, w, h) = crate::tray_icon::status_icon(icon_state);
    let initial_icon = Image::new_owned(rgba, w, h);

    let menu = build_tray_menu(app, paused, indicator_visible, srv, llm, cli)?;

    TrayIconBuilder::with_id("main-tray")
        .icon(initial_icon)
        .icon_as_template(true)
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
                    if let Err(e) =
                        app.emit_to("magic-overlay", "overlay:capture-state-changed", &payload)
                    {
                        debug!("emit magic-overlay failed: {e}");
                    }
                    let _ =
                        app.emit_to("tracking-panel", "overlay:capture-state-changed", &payload);
                    if let Err(e) = sync_tray_state(app, new_paused, indicator_visible) {
                        debug!("sync_tray_state failed: {e}");
                    }
                    #[cfg(target_os = "macos")]
                    if let Some(border) = app.try_state::<crate::native_border::NativeBorderState>()
                    {
                        border.0.set_paused(new_paused);
                    }
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
                    if let Err(e) =
                        app.emit_to("magic-overlay", "overlay:capture-state-changed", &payload)
                    {
                        debug!("emit magic-overlay failed: {e}");
                    }
                    let _ =
                        app.emit_to("tracking-panel", "overlay:capture-state-changed", &payload);
                    if let Some(panel) = app.get_webview_window("tracking-panel") {
                        if new_visible {
                            if let Err(e) = panel.show() {
                                debug!("window show failed: {e}");
                            }
                        } else if let Err(e) = panel.hide() {
                            debug!("window hide failed: {e}");
                        }
                    }
                    if let Err(e) = sync_tray_state(app, paused, new_visible) {
                        debug!("sync_tray_state failed: {e}");
                    }
                    #[cfg(target_os = "macos")]
                    if let Some(border) = app.try_state::<crate::native_border::NativeBorderState>()
                    {
                        if new_visible {
                            border.0.show();
                        } else {
                            border.0.hide();
                        }
                    }
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
            // All three menu items emit the standard `navigate` event with a
            // deep-link path, matching the "Settings" entry. Earlier revisions
            // used custom `tray-toggle-automation` / `automation:quick-access`
            // events that were bridged back to navigate calls in the frontend
            // listener — the indirection added nothing beyond a React Query
            // invalidation the target routes did not even consume. Unified so
            // new tray entries only need a path, not a bespoke event name.
            "settings" => {
                focus_main_window(app);
                app.emit_to("main", "navigate", "/settings")
                    .unwrap_or_default();
            }
            "automation" => {
                focus_main_window(app);
                app.emit_to("main", "navigate", "/settings/ai-automation")
                    .unwrap_or_default();
            }
            "run-preset" => {
                focus_main_window(app);
                app.emit_to("main", "navigate", "/automation")
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

    info!("system tray initialized with template icon");
    Ok(())
}

/// Update tray icon and menu to reflect current capture/connection state.
///
/// Combines icon swap (template shape overlay) with menu text rebuild.
/// Called from tray event handlers and IPC commands.
pub fn sync_tray_state<R: Runtime>(
    app: &tauri::AppHandle<R>,
    paused: bool,
    indicator_visible: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let (srv, llm, cli) = read_connection_status(app);
        let any_disconnected = !srv || !llm || !cli;

        // Update icon with template shape overlay
        let icon_state = resolve_icon_state(paused, any_disconnected);
        let (rgba, w, h) = crate::tray_icon::status_icon(icon_state);
        let icon = Image::new_owned(rgba, w, h);
        tray.set_icon(Some(icon))?;
        tray.set_icon_as_template(true)?;

        // Rebuild menu with connection status
        let menu = build_tray_menu(app, paused, indicator_visible, srv, llm, cli)?;
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}
