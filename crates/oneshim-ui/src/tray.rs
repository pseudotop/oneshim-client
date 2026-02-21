//! 시스템 트레이.
//!
//! tray-icon 기반 시스템 트레이 아이콘 + 메뉴.
//! macOS: 메인 스레드에서 초기화 필수 (muda 제약).
//! 이벤트 폴링은 별도 스레드에서 수행, mpsc 채널로 GUI에 전달.

use std::sync::mpsc;
use tracing::{debug, info};

/// 트레이 이벤트 (트레이 → GUI)
#[derive(Debug, Clone, PartialEq)]
pub enum TrayEvent {
    /// 메인 창 표시/숨기기
    ToggleWindow,
    /// 설정 화면 열기
    OpenSettings,
    /// 자동화 활성화/비활성화 토글
    ToggleAutomation,
    ApproveUpdate,
    DeferUpdate,
    /// 앱 종료
    Quit,
}

/// 트레이 메뉴 액션 (하위 호환)
pub type TrayAction = TrayEvent;

/// 시스템 트레이 상태 관리자
pub struct SystemTray {
    is_visible: bool,
    has_badge: bool,
}

impl SystemTray {
    /// 새 시스템 트레이 생성
    pub fn new() -> Self {
        info!("시스템 트레이 초기화");
        Self {
            is_visible: true,
            has_badge: false,
        }
    }

    /// 트레이 아이콘 뱃지 설정 (제안 수신 시)
    pub fn set_badge(&mut self, has_badge: bool) {
        self.has_badge = has_badge;
        debug!("트레이 뱃지: {has_badge}");
    }

    /// 트레이 뱃지 상태
    pub fn has_badge(&self) -> bool {
        self.has_badge
    }

    /// 트레이 표시 상태
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    /// 트레이 표시/숨기기
    pub fn set_visible(&mut self, visible: bool) {
        self.is_visible = visible;
    }
}

impl Default for SystemTray {
    fn default() -> Self {
        Self::new()
    }
}

/// 트레이 아이콘 데이터 (PNG, 32x32)
/// 빌드 시 바이너리에 포함
#[cfg(not(target_os = "linux"))]
const TRAY_ICON_DATA: &[u8] = include_bytes!("../assets/tray_icon.png");

/// 메뉴 아이템 ID 저장 (이벤트 매칭용)
#[cfg(not(target_os = "linux"))]
struct MenuIds {
    show_id: tray_icon::menu::MenuId,
    settings_id: tray_icon::menu::MenuId,
    automation_id: tray_icon::menu::MenuId,
    approve_update_id: tray_icon::menu::MenuId,
    defer_update_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
}

/// 트레이 매니저
///
/// macOS에서는 메인 스레드에서 `new()` 호출 필수.
/// 이벤트 폴링은 내부적으로 별도 스레드에서 수행.
#[cfg(not(target_os = "linux"))]
pub struct TrayManager {
    /// 이벤트 전송자 (향후 트레이 명령 전송용으로 보존)
    #[allow(dead_code)]
    event_tx: mpsc::Sender<TrayEvent>,
    /// 트레이 아이콘 (드롭 방지)
    #[allow(dead_code)]
    _tray_icon: tray_icon::TrayIcon,
}

#[cfg(not(target_os = "linux"))]
impl TrayManager {
    /// 트레이 매니저 생성 (메인 스레드에서 호출 필수)
    ///
    /// # Returns
    /// - `TrayManager` 인스턴스
    /// - 이벤트 수신 채널 (`mpsc::Receiver<TrayEvent>`)
    ///
    /// # Panics
    /// macOS에서 메인 스레드가 아닌 곳에서 호출 시 패닉
    pub fn new() -> Result<(Self, mpsc::Receiver<TrayEvent>), String> {
        use tray_icon::{
            menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
            TrayIconBuilder,
        };

        info!("시스템 트레이 초기화 (메인 스레드)");

        // 메뉴 생성 (메인 스레드 필수)
        let menu = Menu::new();

        let show_item = MenuItem::new("창 보기/숨기기", true, None);
        let settings_item = MenuItem::new("설정", true, None);
        let automation_item = MenuItem::new("자동화 켜기/끄기", true, None);
        let approve_update_item = MenuItem::new("업데이트 적용", true, None);
        let defer_update_item = MenuItem::new("업데이트 나중에", true, None);
        let quit_item = MenuItem::new("종료", true, None);

        menu.append(&show_item).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&settings_item).map_err(|e| e.to_string())?;
        menu.append(&automation_item).map_err(|e| e.to_string())?;
        menu.append(&approve_update_item)
            .map_err(|e| e.to_string())?;
        menu.append(&defer_update_item).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&quit_item).map_err(|e| e.to_string())?;

        // 아이콘 로드
        let icon = load_icon()?;

        // 트레이 아이콘 생성 (메인 스레드 필수)
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("ONESHIM")
            .with_icon(icon)
            .with_menu_on_left_click(true) // macOS: 좌클릭으로 메뉴 표시
            .build()
            .map_err(|e| e.to_string())?;

        info!("시스템 트레이 아이콘 생성 완료");

        // 메뉴 ID 저장
        let menu_ids = MenuIds {
            show_id: show_item.id().clone(),
            settings_id: settings_item.id().clone(),
            automation_id: automation_item.id().clone(),
            approve_update_id: approve_update_item.id().clone(),
            defer_update_id: defer_update_item.id().clone(),
            quit_id: quit_item.id().clone(),
        };

        // 이벤트 채널 생성
        let (event_tx, event_rx) = mpsc::channel();

        // 이벤트 폴링 스레드 시작 (MenuEvent::receiver는 스레드 안전)
        let tx = event_tx.clone();
        std::thread::spawn(move || {
            let menu_event_rx = MenuEvent::receiver();

            loop {
                // 메뉴 이벤트 대기 (블로킹)
                if let Ok(event) = menu_event_rx.recv() {
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
                        debug!("트레이 이벤트: {:?}", e);
                        if tx.send(e).is_err() {
                            // 수신자가 드롭됨 → 루프 종료
                            info!("트레이 이벤트 채널 닫힘, 루프 종료");
                            break;
                        }
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

    /// 트레이 이벤트 직접 전송 (테스트용)
    #[cfg(test)]
    pub fn send_event(&self, event: TrayEvent) {
        let _ = self.event_tx.send(event);
    }
}

/// PNG 아이콘 로드
#[cfg(not(target_os = "linux"))]
fn load_icon() -> Result<tray_icon::Icon, String> {
    use tray_icon::Icon;

    // PNG 디코딩
    let image = image::load_from_memory(TRAY_ICON_DATA)
        .map_err(|e| format!("아이콘 로드 실패: {e}"))?
        .into_rgba8();

    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    Icon::from_rgba(rgba, width, height).map_err(|e| format!("아이콘 생성 실패: {e}"))
}

// ── Linux: 스텁 구현 (appindicator 미지원) ──

#[cfg(target_os = "linux")]
pub struct TrayManager;

#[cfg(target_os = "linux")]
impl TrayManager {
    pub fn new() -> Result<(Self, mpsc::Receiver<TrayEvent>), String> {
        let (_tx, rx) = mpsc::channel();
        info!("Linux: 시스템 트레이 미지원 (appindicator 필요)");
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
