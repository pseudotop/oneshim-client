use tauri::{App, Manager};
#[allow(unused_imports)]
use tracing::info;

pub(crate) struct DesktopStartupCoordinator;

impl DesktopStartupCoordinator {
    pub(crate) fn apply(
        app: &App,
        frontend_web_port: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::tray::setup_tray(app)?;

        #[cfg(target_os = "macos")]
        {
            crate::macos_integration::set_dock_icon();
            info!("macOS dock icon set from embedded icon.png");
        }

        Self::show_main_window(app, frontend_web_port);
        Ok(())
    }

    fn show_main_window(app: &App, frontend_web_port: u16) {
        if let Some(window) = app.get_webview_window("main") {
            #[cfg(not(target_os = "macos"))]
            {
                let _ = window.set_decorations(false);
            }

            let port_js = format!("window.__ONESHIM_WEB_PORT__ = {};", frontend_web_port);
            let _ = window.eval(&port_js);

            let _ = window.show();
            let _ = window.set_focus();
            debug_assert!(
                window.is_visible().unwrap_or(false),
                "main window must be visible after desktop startup"
            );
        } else {
            debug_assert!(false, "main window not found during desktop startup");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn desktop_startup_contains_window_show_call() {
        let startup_src = include_str!("desktop_startup.rs");

        assert!(
            startup_src.contains("window.show()"),
            "desktop startup must call window.show()"
        );
        assert!(
            startup_src.contains("window.set_focus()"),
            "desktop startup must call window.set_focus() after show()"
        );
    }
}
