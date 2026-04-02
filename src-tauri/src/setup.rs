use anyhow::Result;
use tauri::App;
use tracing::info;

use crate::app_runtime_launch::{AppRuntimeLaunchBuilder, AppRuntimeLaunchResult};
use crate::bootstrap_runtime::BootstrapRuntimeBuilder;
use crate::desktop_startup::DesktopStartupCoordinator;

/// Tauri setup 함수 — gui_runner.rs의 Agent + WebServer 초기화 이전
pub fn init(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    info!("Tauri setup: initializing ONESHIM agent");
    let AppRuntimeLaunchResult {
        frontend_web_port,
        state_builder,
    } = AppRuntimeLaunchBuilder::new(
        BootstrapRuntimeBuilder::new().build()?,
        app.handle().clone(),
    )
    .build_and_spawn()?;

    state_builder.build().register_on(app);
    crate::setup_shortcuts::register_all(app);

    // 12. Desktop shell startup
    DesktopStartupCoordinator::apply(app, frontend_web_port)?;
    crate::setup_windows::prepare(app);
    crate::setup_platform::apply(app);

    info!("Tauri setup complete");
    Ok(())
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
