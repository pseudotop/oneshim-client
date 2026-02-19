//! Windows 네이티브 API.
//!
//! Win32 API를 사용한 앱 숨기기/표시.
//! Docker Desktop처럼 X 버튼 클릭 시 앱을 완전히 숨기고,
//! 트레이에서 다시 표시할 수 있도록 함.

use tracing::{debug, info, warn};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, ShowWindow,
    SW_HIDE, SW_SHOW,
};

/// 현재 프로세스의 모든 윈도우 핸들 수집
fn get_process_windows() -> Vec<HWND> {
    let mut windows: Vec<HWND> = Vec::new();
    let _current_pid = std::process::id();

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: isize) -> i32 {
        let windows = &mut *(lparam as *mut Vec<HWND>);
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);

        if pid == std::process::id() {
            windows.push(hwnd);
        }
        1 // 계속 열거
    }

    unsafe {
        EnumWindows(Some(enum_callback), &mut windows as *mut Vec<HWND> as isize);
    }

    windows
}

/// 앱 숨기기 (작업 표시줄에서도 안 보임)
///
/// 모든 윈도우에 SW_HIDE 적용
pub fn hide_app() {
    let windows = get_process_windows();
    if windows.is_empty() {
        warn!("Windows: 숨길 윈도우 없음");
        return;
    }

    for hwnd in windows {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
    info!("Windows: 앱 숨김 (ShowWindow SW_HIDE)");
}

/// 앱 표시 (활성화)
///
/// 모든 윈도우에 SW_SHOW + SetForegroundWindow 적용
pub fn show_app() {
    let windows = get_process_windows();
    if windows.is_empty() {
        warn!("Windows: 표시할 윈도우 없음");
        return;
    }

    for hwnd in &windows {
        unsafe {
            ShowWindow(*hwnd, SW_SHOW);
        }
    }

    // 첫 번째 윈도우를 전면으로
    if let Some(hwnd) = windows.first() {
        unsafe {
            SetForegroundWindow(*hwnd);
        }
    }
    info!("Windows: 앱 표시 (ShowWindow SW_SHOW + SetForegroundWindow)");
}

/// 앱이 숨겨져 있는지 확인 (첫 번째 윈도우 기준)
pub fn is_app_hidden() -> bool {
    let windows = get_process_windows();
    let hidden = windows
        .first()
        .map_or(true, |hwnd| unsafe { IsWindowVisible(*hwnd) == 0 });
    debug!("Windows: 앱 숨김 상태 = {}", hidden);
    hidden
}

#[cfg(test)]
mod tests {
    // 테스트는 GUI 환경에서만 가능
}
