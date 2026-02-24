//!
//!
//!

use oneshim_core::error::CoreError;
use oneshim_core::models::context::{MousePosition, WindowInfo};
use std::process::Command;
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    X11,
    Wayland,
    Unknown,
}

pub fn detect_display_server() -> DisplayServer {
    if let Ok(session_type) = std::env::var("XDG_SESSION_TYPE") {
        match session_type.to_lowercase().as_str() {
            "x11" => return DisplayServer::X11,
            "wayland" => return DisplayServer::Wayland,
            _ => {}
        }
    }

    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return DisplayServer::Wayland;
    }

    if std::env::var("DISPLAY").is_ok() {
        return DisplayServer::X11;
    }

    DisplayServer::Unknown
}

///
pub fn get_active_window_linux() -> Result<Option<WindowInfo>, CoreError> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_active_window_x11(),
        DisplayServer::Wayland => {
            debug!("Wayland detection - XWayland fallback attempt");
            get_active_window_x11().or_else(|_| {
                warn!("Wayland active window detection - X11 app");
                Ok(None)
            })
        }
        DisplayServer::Unknown => {
            debug!("server detection failure");
            Ok(None)
        }
    }
}

fn get_active_window_x11() -> Result<Option<WindowInfo>, CoreError> {
    let window_id = match Command::new("xdotool").arg("getactivewindow").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!("xdotool failure: {}", stderr);
            return Ok(None);
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xdotool - 'sudo apt install xdotool' execution required");
            } else {
                debug!("xdotool execution failure: {}", e);
            }
            return Ok(None);
        }
    };

    if window_id.is_empty() {
        return Ok(None);
    }

    let title = Command::new("xdotool")
        .args(["getwindowname", &window_id])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let pid = Command::new("xdotool")
        .args(["getwindowpid", &window_id])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(0);

    let app_name = if pid > 0 {
        get_process_name(pid).unwrap_or_else(|| "Unknown".to_string())
    } else {
        "Unknown".to_string()
    };

    let bounds = get_window_geometry_x11(&window_id);

    debug!("active window: {} - {} (PID: {})", app_name, title, pid);

    Ok(Some(WindowInfo {
        title,
        app_name,
        pid,
        bounds,
    }))
}

fn get_window_geometry_x11(window_id: &str) -> Option<oneshim_core::models::context::WindowBounds> {
    let output = Command::new("xdotool")
        .args(["getwindowgeometry", "--shell", window_id])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut x = 0i32;
    let mut y = 0i32;
    let mut width = 0u32;
    let mut height = 0u32;

    for line in stdout.lines() {
        if let Some(val) = line.strip_prefix("X=") {
            x = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("Y=") {
            y = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("WIDTH=") {
            width = val.parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("HEIGHT=") {
            height = val.parse().unwrap_or(0);
        }
    }

    Some(oneshim_core::models::context::WindowBounds {
        x,
        y,
        width,
        height,
    })
}

fn get_process_name(pid: u32) -> Option<String> {
    let comm_path = format!("/proc/{}/comm", pid);
    std::fs::read_to_string(&comm_path)
        .ok()
        .map(|s| s.trim().to_string())
}

///
pub fn get_idle_time_linux() -> Option<u64> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_idle_time_x11(),
        DisplayServer::Wayland => {
            // GNOME: org.gnome.Mutter.IdleMonitor D-Bus API
            // KDE: org.kde.KIdleTime D-Bus API
            get_idle_time_x11().or_else(|| {
                debug!("Wayland idle detection");
                None
            })
        }
        DisplayServer::Unknown => None,
    }
}

fn get_idle_time_x11() -> Option<u64> {
    let output = match Command::new("xprintidle").output() {
        Ok(output) if output.status.success() => output,
        Ok(_) => {
            return None;
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xprintidle - 'sudo apt install xprintidle' execution required");
            }
            return None;
        }
    };

    let ms_str = String::from_utf8_lossy(&output.stdout);
    let ms: u64 = ms_str.trim().parse().ok()?;

    Some(ms / 1000)
}

///
pub fn get_mouse_position_linux() -> Option<MousePosition> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_mouse_position_x11(),
        DisplayServer::Wayland => {
            get_mouse_position_x11().or_else(|| {
                debug!("Wayland mouse detection");
                None
            })
        }
        DisplayServer::Unknown => None,
    }
}

fn get_mouse_position_x11() -> Option<MousePosition> {
    // x:1234 y:567 screen:0 window:12345678
    let output = match Command::new("xdotool").arg("getmouselocation").output() {
        Ok(output) if output.status.success() => output,
        Ok(_) => {
            return None;
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xdotool - mouse detection not-available");
            }
            return None;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut x: Option<i32> = None;
    let mut y: Option<i32> = None;

    for part in stdout.split_whitespace() {
        if let Some(val) = part.strip_prefix("x:") {
            x = val.parse().ok();
        } else if let Some(val) = part.strip_prefix("y:") {
            y = val.parse().ok();
        }
    }

    match (x, y) {
        (Some(x), Some(y)) => Some(MousePosition { x, y }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_display_server_works() {
        let server = detect_display_server();
        assert!(matches!(
            server,
            DisplayServer::X11 | DisplayServer::Wayland | DisplayServer::Unknown
        ));
    }

    #[test]
    fn get_process_name_from_proc() {
        let name = get_process_name(1);
        assert!(name.is_some());
        let name = name.unwrap();
        assert!(!name.is_empty());
    }

    #[test]
    fn active_window_returns_option() {
        let result = get_active_window_linux();
        assert!(result.is_ok());
    }

    #[test]
    fn idle_time_returns_option() {
        let result = get_idle_time_linux();
        if let Some(secs) = result {
            assert!(secs < 86400 * 365); // 1        }
    }

    #[test]
    fn mouse_position_returns_option() {
        let result = get_mouse_position_linux();
        if let Some(pos) = result {
            assert!(pos.x >= 0 && pos.x < 32000);
            assert!(pos.y >= 0 && pos.y < 32000);
        }
    }
}
