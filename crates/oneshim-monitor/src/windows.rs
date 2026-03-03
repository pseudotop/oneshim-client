#![cfg(target_os = "windows")]

use oneshim_core::error::CoreError;
use oneshim_core::models::context::{MousePosition, WindowBounds, WindowInfo};
use tracing::debug;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
};

pub fn get_active_window_windows() -> Result<Option<WindowInfo>, CoreError> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.is_null() {
            debug!("no active window (GetForegroundWindow returned null)");
            return Ok(None);
        }

        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
        let title = if len > 0 {
            String::from_utf16_lossy(&title_buf[..len as usize])
        } else {
            String::new()
        };

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);

        let app_name = get_process_name(pid).unwrap_or_else(|| "Unknown".to_string());

        let bounds = get_window_bounds(hwnd);

        debug!(
            "active window: {app_name} - {title} (PID: {pid}, {:?})",
            bounds.map(|b| format!("{}x{} at ({},{})", b.width, b.height, b.x, b.y))
        );

        Ok(Some(WindowInfo {
            title,
            app_name,
            pid,
            bounds,
        }))
    }
}

fn get_window_bounds(hwnd: HWND) -> Option<WindowBounds> {
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) != 0 {
            let width = (rect.right - rect.left) as u32;
            let height = (rect.bottom - rect.top) as u32;

            if width > 0 && height > 0 {
                Some(WindowBounds {
                    x: rect.left,
                    y: rect.top,
                    width,
                    height,
                })
            } else {
                None
            }
        } else {
            None
        }
    }
}

fn get_process_name(pid: u32) -> Option<String> {
    use sysinfo::{Pid, System};

    let mut sys = System::new();
    sys.refresh_processes(
        sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
    );

    sys.process(Pid::from_u32(pid))
        .map(|p| p.name().to_string_lossy().to_string())
}

pub fn get_idle_time_windows() -> Option<u64> {
    unsafe {
        let mut last_input: LASTINPUTINFO = std::mem::zeroed();
        last_input.cbSize = std::mem::size_of::<LASTINPUTINFO>() as u32;

        if GetLastInputInfo(&mut last_input) != 0 {
            let current_tick = windows_sys::Win32::System::SystemInformation::GetTickCount();
            let idle_ms = current_tick.wrapping_sub(last_input.dwTime);
            Some((idle_ms / 1000) as u64)
        } else {
            None
        }
    }
}

pub fn get_mouse_position_windows() -> Option<MousePosition> {
    unsafe {
        let mut point: POINT = std::mem::zeroed();
        if GetCursorPos(&mut point) != 0 {
            Some(MousePosition {
                x: point.x,
                y: point.y,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {}
