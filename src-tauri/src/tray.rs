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
    let show = MenuItem::with_id(app, "show", "Toggle Window", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let automation = MenuItem::with_id(app, "automation", "Toggle Automation", true, None::<&str>)?;
    let approve = MenuItem::with_id(app, "approve_update", "Apply Update", true, None::<&str>)?;
    let defer = MenuItem::with_id(app, "defer_update", "Defer Update", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
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

    let tray_icon = Image::from_bytes(include_bytes!("../icons/tray_icon.png"))
        .expect("embedded tray_icon.png must be valid");

    TrayIconBuilder::new()
        .icon(tray_icon)
        .icon_as_template(true)
        .menu(&menu)
        .tooltip("ONESHIM")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
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
                if let Some(state) = app.try_state::<crate::setup::AppState>() {
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
                if let Some(state) = app.try_state::<crate::setup::AppState>() {
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

    info!("system tray initialized with 6 menu items");
    Ok(())
}
