use oneshim_core::error::CoreError;
use oneshim_core::models::context::{MousePosition, WindowInfo};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};

const SUBPROCESS_TIMEOUT_SECS: u64 = 5;

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

pub async fn get_active_window_linux() -> Result<Option<WindowInfo>, CoreError> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_active_window_x11().await,
        DisplayServer::Wayland => {
            debug!("Wayland detection - XWayland fallback attempt");
            match get_active_window_x11().await {
                Ok(result) => Ok(result),
                Err(_) => {
                    warn!("Wayland active window detection - X11 app");
                    Ok(None)
                }
            }
        }
        DisplayServer::Unknown => {
            debug!("server detection failure");
            Ok(None)
        }
    }
}

async fn get_active_window_x11() -> Result<Option<WindowInfo>, CoreError> {
    let window_id = match timeout(
        Duration::from_secs(SUBPROCESS_TIMEOUT_SECS),
        Command::new("xdotool").arg("getactivewindow").output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!("xdotool failure: {}", stderr);
            return Ok(None);
        }
        Ok(Err(e)) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xdotool - 'sudo apt install xdotool' execution required");
            } else {
                debug!("xdotool execution failure: {}", e);
            }
            return Ok(None);
        }
        Err(_) => {
            debug!("xdotool getactivewindow timed out");
            return Ok(None);
        }
    };

    if window_id.is_empty() {
        return Ok(None);
    }

    let title = timeout(
        Duration::from_secs(SUBPROCESS_TIMEOUT_SECS),
        Command::new("xdotool")
            .args(["getwindowname", &window_id])
            .output(),
    )
    .await
    .ok()
    .and_then(|r| r.ok())
    .filter(|o| o.status.success())
    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    .unwrap_or_default();

    let pid = timeout(
        Duration::from_secs(SUBPROCESS_TIMEOUT_SECS),
        Command::new("xdotool")
            .args(["getwindowpid", &window_id])
            .output(),
    )
    .await
    .ok()
    .and_then(|r| r.ok())
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

    let bounds = get_window_geometry_x11(&window_id).await;

    debug!("active window: {} - {} (PID: {})", app_name, title, pid);

    Ok(Some(WindowInfo {
        title,
        app_name,
        pid,
        bounds,
    }))
}

async fn get_window_geometry_x11(
    window_id: &str,
) -> Option<oneshim_core::models::context::WindowBounds> {
    let output = timeout(
        Duration::from_secs(SUBPROCESS_TIMEOUT_SECS),
        Command::new("xdotool")
            .args(["getwindowgeometry", "--shell", window_id])
            .output(),
    )
    .await
    .ok()?
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

pub async fn get_idle_time_linux() -> Option<u64> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_idle_time_x11().await,
        DisplayServer::Wayland => {
            // GNOME: org.gnome.Mutter.IdleMonitor D-Bus API
            // KDE: org.kde.KIdleTime D-Bus API
            match get_idle_time_x11().await {
                Some(t) => Some(t),
                None => {
                    debug!("Wayland idle detection");
                    None
                }
            }
        }
        DisplayServer::Unknown => None,
    }
}

async fn get_idle_time_x11() -> Option<u64> {
    let output = match timeout(
        Duration::from_secs(SUBPROCESS_TIMEOUT_SECS),
        Command::new("xprintidle").output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => output,
        Ok(Ok(_)) => {
            return None;
        }
        Ok(Err(e)) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xprintidle - 'sudo apt install xprintidle' execution required");
            }
            return None;
        }
        Err(_) => {
            debug!("xprintidle timed out");
            return None;
        }
    };

    let ms_str = String::from_utf8_lossy(&output.stdout);
    let ms: u64 = ms_str.trim().parse().ok()?;

    Some(ms / 1000)
}

pub async fn get_mouse_position_linux() -> Option<MousePosition> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_mouse_position_x11().await,
        DisplayServer::Wayland => match get_mouse_position_x11().await {
            Some(pos) => Some(pos),
            None => {
                debug!("Wayland mouse detection");
                None
            }
        },
        DisplayServer::Unknown => None,
    }
}

async fn get_mouse_position_x11() -> Option<MousePosition> {
    // x:1234 y:567 screen:0 window:12345678
    let output = match timeout(
        Duration::from_secs(SUBPROCESS_TIMEOUT_SECS),
        Command::new("xdotool").arg("getmouselocation").output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => output,
        Ok(Ok(_)) => {
            return None;
        }
        Ok(Err(e)) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xdotool - mouse detection not-available");
            }
            return None;
        }
        Err(_) => {
            debug!("xdotool getmouselocation timed out");
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

    #[tokio::test]
    async fn active_window_returns_option() {
        let result = get_active_window_linux().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn idle_time_returns_option() {
        let result = get_idle_time_linux().await;
        if let Some(secs) = result {
            // sanity bound: less than one year in seconds
            assert!(secs < 86400 * 365);
        }
    }

    #[tokio::test]
    async fn mouse_position_returns_option() {
        let result = get_mouse_position_linux().await;
        if let Some(pos) = result {
            assert!(pos.x >= 0 && pos.x < 32000);
            assert!(pos.y >= 0 && pos.y < 32000);
        }
    }
}
