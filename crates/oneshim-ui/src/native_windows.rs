use tracing::{debug, info, warn};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, ShowWindow,
    SW_HIDE, SW_SHOW,
};

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
        1
    }

    unsafe {
        EnumWindows(Some(enum_callback), &mut windows as *mut Vec<HWND> as isize);
    }

    windows
}

pub fn hide_app() {
    let windows = get_process_windows();
    if windows.is_empty() {
        warn!("Windows: none");
        return;
    }

    for hwnd in windows {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
    info!("Windows: app (ShowWindow SW_HIDE)");
}

pub fn show_app() {
    let windows = get_process_windows();
    if windows.is_empty() {
        warn!("Windows: display none");
        return;
    }

    for hwnd in &windows {
        unsafe {
            ShowWindow(*hwnd, SW_SHOW);
        }
    }

    if let Some(hwnd) = windows.first() {
        unsafe {
            SetForegroundWindow(*hwnd);
        }
    }
    info!("Windows: app display (ShowWindow SW_SHOW + SetForegroundWindow)");
}

pub fn is_app_hidden() -> bool {
    let windows = get_process_windows();
    let hidden = windows
        .first()
        .map_or(true, |hwnd| unsafe { IsWindowVisible(*hwnd) == 0 });
    debug!("Windows: app state = {}", hidden);
    hidden
}

#[cfg(test)]
mod tests {}
