use tracing::{debug, info, warn};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, ShowWindow,
    SW_HIDE, SW_SHOW,
};

fn get_process_windows() -> Vec<HWND> {
    let mut windows: Vec<HWND> = Vec::new();
    let _current_pid = std::process::id();

    // SAFETY: lparam is cast from a valid &mut Vec<HWND> pointer passed by
    // EnumWindows below. The callback runs synchronously on the same thread,
    // so the Vec reference is valid for the entire enumeration.
    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: isize) -> i32 {
        let windows = &mut *(lparam as *mut Vec<HWND>);
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);

        if pid == std::process::id() {
            windows.push(hwnd);
        }
        1
    }

    // SAFETY: EnumWindows calls enum_callback synchronously for each top-level
    // window. The &mut windows pointer is valid for the duration of the call.
    // enum_callback returns 1 to continue enumeration for all windows.
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
        // SAFETY: hwnd is a valid window handle from EnumWindows for this process.
        // ShowWindow with SW_HIDE is a non-destructive visibility change.
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
        // SAFETY: hwnd is a valid window handle from EnumWindows for this process.
        // ShowWindow with SW_SHOW is a non-destructive visibility change.
        unsafe {
            ShowWindow(*hwnd, SW_SHOW);
        }
    }

    if let Some(hwnd) = windows.first() {
        // SAFETY: hwnd is a valid window handle from EnumWindows for this process.
        // SetForegroundWindow may silently fail (returns BOOL) but is safe to call.
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
        // SAFETY: hwnd is a valid window handle from EnumWindows for this process.
        // IsWindowVisible is a read-only query that returns BOOL.
        .map_or(true, |hwnd| unsafe { IsWindowVisible(*hwnd) == 0 });
    debug!("Windows: app state = {}", hidden);
    hidden
}

#[cfg(test)]
mod tests {}
