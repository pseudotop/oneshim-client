use std::sync::mpsc;
use tracing::{debug, info};

#[derive(Debug, Clone, PartialEq)]
pub enum TrayEvent {
    ToggleWindow,
    OpenSettings,
    ToggleAutomation,
    ApproveUpdate,
    DeferUpdate,
    Quit,
}

pub type TrayAction = TrayEvent;

pub struct SystemTray {
    is_visible: bool,
    has_badge: bool,
}

impl SystemTray {
    pub fn new() -> Self {
        info!("system tray initialize");
        Self {
            is_visible: true,
            has_badge: false,
        }
    }

    pub fn set_badge(&mut self, has_badge: bool) {
        self.has_badge = has_badge;
        debug!("tray: {has_badge}");
    }

    pub fn has_badge(&self) -> bool {
        self.has_badge
    }

    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.is_visible = visible;
    }
}

impl Default for SystemTray {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_os = "linux"))]
const TRAY_ICON_DATA: &[u8] = include_bytes!("../assets/tray_icon.png");

#[cfg(not(target_os = "linux"))]
struct MenuIds {
    show_id: tray_icon::menu::MenuId,
    settings_id: tray_icon::menu::MenuId,
    automation_id: tray_icon::menu::MenuId,
    approve_update_id: tray_icon::menu::MenuId,
    defer_update_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
}

#[cfg(not(target_os = "linux"))]
pub struct TrayManager {
    #[allow(dead_code)]
    event_tx: mpsc::Sender<TrayEvent>,
    #[allow(dead_code)]
    _tray_icon: tray_icon::TrayIcon,
}

#[cfg(not(target_os = "linux"))]
impl TrayManager {
    /// # Returns
    ///
    /// # Panics
    pub fn new() -> Result<(Self, mpsc::Receiver<TrayEvent>), String> {
        #[cfg(not(target_os = "macos"))]
        use tray_icon::menu::PredefinedMenuItem;
        use tray_icon::{
            menu::{Menu, MenuEvent, MenuItem},
            TrayIconBuilder,
        };

        info!("system tray initialize ( )");

        let menu = Menu::new();

        let show_item = MenuItem::new("창 보기/숨기기", true, None);
        let settings_item = MenuItem::new("설정", true, None);
        let automation_item = MenuItem::new("자동화 켜기/끄기", true, None);
        let approve_update_item = MenuItem::new("update 적용", true, None);
        let defer_update_item = MenuItem::new("update 나중에", true, None);
        let quit_item = MenuItem::new("ended", true, None);

        menu.append(&show_item).map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "macos"))]
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&settings_item).map_err(|e| e.to_string())?;
        menu.append(&automation_item).map_err(|e| e.to_string())?;
        menu.append(&approve_update_item)
            .map_err(|e| e.to_string())?;
        menu.append(&defer_update_item).map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "macos"))]
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&quit_item).map_err(|e| e.to_string())?;

        let icon = load_icon()?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("ONESHIM")
            .with_icon(icon)
            .with_menu_on_left_click(true) // macOS: show menu on left-click
            .build()
            .map_err(|e| e.to_string())?;

        info!("system tray create completed");

        let menu_ids = MenuIds {
            show_id: show_item.id().clone(),
            settings_id: settings_item.id().clone(),
            automation_id: automation_item.id().clone(),
            approve_update_id: approve_update_item.id().clone(),
            defer_update_id: defer_update_item.id().clone(),
            quit_id: quit_item.id().clone(),
        };

        let (event_tx, event_rx) = mpsc::channel();

        let tx = event_tx.clone();
        std::thread::spawn(move || {
            let menu_event_rx = MenuEvent::receiver();

            loop {
                let event = match menu_event_rx.recv() {
                    Ok(event) => event,
                    Err(_) => {
                        info!("tray menu event receiver closed");
                        break;
                    }
                };

                let tray_event = if event.id == menu_ids.show_id {
                    Some(TrayEvent::ToggleWindow)
                } else if event.id == menu_ids.settings_id {
                    Some(TrayEvent::OpenSettings)
                } else if event.id == menu_ids.automation_id {
                    Some(TrayEvent::ToggleAutomation)
                } else if event.id == menu_ids.approve_update_id {
                    Some(TrayEvent::ApproveUpdate)
                } else if event.id == menu_ids.defer_update_id {
                    Some(TrayEvent::DeferUpdate)
                } else if event.id == menu_ids.quit_id {
                    Some(TrayEvent::Quit)
                } else {
                    None
                };

                if let Some(e) = tray_event {
                    debug!("tray event: {:?}", e);
                    if tx.send(e).is_err() {
                        info!("tray event channel closed, ended");
                        break;
                    }
                }
            }
        });

        Ok((
            Self {
                event_tx,
                _tray_icon: tray_icon,
            },
            event_rx,
        ))
    }

    #[cfg(test)]
    pub fn send_event(&self, event: TrayEvent) {
        let _ = self.event_tx.send(event);
    }
}

#[cfg(not(target_os = "linux"))]
fn load_icon() -> Result<tray_icon::Icon, String> {
    use tray_icon::Icon;

    let image = image::load_from_memory(TRAY_ICON_DATA)
        .map_err(|e| format!("Icon load failed: {e}"))?
        .into_rgba8();

    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    Icon::from_rgba(rgba, width, height).map_err(|e| format!("Icon creation failed: {e}"))
}

#[cfg(target_os = "linux")]
pub struct TrayManager;

#[cfg(target_os = "linux")]
impl TrayManager {
    pub fn new() -> Result<(Self, mpsc::Receiver<TrayEvent>), String> {
        let (_tx, rx) = mpsc::channel();
        info!("Linux: system tray (appindicator required)");
        Ok((Self, rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_creation() {
        let tray = SystemTray::new();
        assert!(tray.is_visible());
        assert!(!tray.has_badge());
    }

    #[test]
    fn badge_toggle() {
        let mut tray = SystemTray::new();
        tray.set_badge(true);
        assert!(tray.has_badge());
        tray.set_badge(false);
        assert!(!tray.has_badge());
    }

    #[test]
    fn tray_event_equality() {
        assert_eq!(TrayEvent::Quit, TrayEvent::Quit);
        assert_ne!(TrayEvent::Quit, TrayEvent::ToggleWindow);
    }
}
