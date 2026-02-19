//! macOS 네이티브 API.
//!
//! NSApplication을 사용한 앱 숨기기/표시.
//! Docker Desktop처럼 X 버튼 클릭 시 앱을 완전히 숨기고,
//! 트레이에서 다시 표시할 수 있도록 함.
//!
//! objc2-app-kit 기반 구현 (최신 Rust-ObjC 바인딩)

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use tracing::{debug, info, warn};

/// MainThreadMarker 획득 (GUI 앱이므로 메인 스레드에서 호출됨)
fn get_mtm() -> Option<MainThreadMarker> {
    // GUI 앱은 메인 스레드에서 실행되므로 안전하게 획득 가능
    // iced 앱은 메인 스레드에서 update() 호출
    MainThreadMarker::new()
}

/// 앱 숨기기 (Dock에서도 안 보임)
///
/// NSApplication.hide() 호출 - 앱이 완전히 숨겨짐
pub fn hide_app() {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: 메인 스레드가 아니므로 앱 숨기기 실패");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    app.hide(None);
    info!("macOS: 앱 숨김 (NSApplication.hide)");
}

/// 앱 표시 (활성화)
///
/// NSApplication.unhide() + activate 호출
#[allow(deprecated)]
pub fn show_app() {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: 메인 스레드가 아니므로 앱 표시 실패");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    app.unhide(None);
    // activateIgnoringOtherApps는 deprecated되었지만 아직 사용 가능
    app.activateIgnoringOtherApps(true);
    info!("macOS: 앱 표시 (NSApplication.unhide + activate)");
}

/// 앱이 숨겨져 있는지 확인
pub fn is_app_hidden() -> bool {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: 메인 스레드가 아니므로 상태 확인 실패");
        return false;
    };

    let app = NSApplication::sharedApplication(mtm);
    let hidden = app.isHidden();
    debug!("macOS: 앱 숨김 상태 = {}", hidden);
    hidden
}

/// Activation Policy를 Accessory로 설정 (Dock 아이콘 숨김)
///
/// 트레이 전용 앱으로 만들 때 사용.
/// 주의: 이 함수 호출 후에는 Dock에 아이콘이 표시되지 않음.
#[allow(dead_code)]
pub fn set_accessory_mode() {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: 메인 스레드가 아니므로 모드 변경 실패");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    info!("macOS: Accessory 모드 설정 (Dock 아이콘 숨김)");
}

/// Activation Policy를 Regular로 복원 (Dock 아이콘 표시)
#[allow(dead_code)]
pub fn set_regular_mode() {
    let Some(mtm) = get_mtm() else {
        warn!("macOS: 메인 스레드가 아니므로 모드 변경 실패");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    info!("macOS: Regular 모드 설정 (Dock 아이콘 표시)");
}

#[cfg(test)]
mod tests {
    // 테스트는 GUI 환경에서만 가능
}
