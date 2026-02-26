//!

use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use oneshim_core::error::CoreError;
use oneshim_core::models::context::{MousePosition, WindowBounds, WindowInfo};
use tracing::debug;

pub fn get_active_window_macos() -> Result<Option<WindowInfo>, CoreError> {
    use std::process::Command;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(
            r#"tell application "System Events"
            set frontApp to first application process whose frontmost is true
            set appName to name of frontApp
            set winTitle to ""
            set winPos to {0, 0}
            set winSize to {0, 0}
            try
                set frontWin to front window of frontApp
                set winTitle to name of frontWin
                set winPos to position of frontWin
                set winSize to size of frontWin
            end try
            return appName & "|" & winTitle & "|" & (item 1 of winPos as integer) & "|" & (item 2 of winPos as integer) & "|" & (item 1 of winSize as integer) & "|" & (item 2 of winSize as integer)
        end tell"#,
        )
        .output()
        .map_err(|e| CoreError::Internal(format!("osascript execution failure: {e}")))?;

    if !output.status.success() {
        debug!("active window detection failure (osascript)");
        return Ok(None);
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let parts: Vec<&str> = result.split('|').collect();

    if parts.is_empty() {
        return Ok(None);
    }

    let app_name = parts[0].to_string();
    let title = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

    let bounds = if parts.len() >= 6 {
        let x = parts[2].parse::<i32>().unwrap_or(0);
        let y = parts[3].parse::<i32>().unwrap_or(0);
        let width = parts[4].parse::<u32>().unwrap_or(0);
        let height = parts[5].parse::<u32>().unwrap_or(0);

        if width > 0 && height > 0 {
            Some(WindowBounds {
                x,
                y,
                width,
                height,
            })
        } else {
            None
        }
    } else {
        None
    };

    debug!(
        "active 창: {app_name} — {title} ({:?})",
        bounds.map(|b| format!("{}x{} at ({},{})", b.width, b.height, b.x, b.y))
    );

    Ok(Some(WindowInfo {
        title,
        app_name,
        pid: 0, // osascript cannot easily resolve PID
        bounds,
    }))
}

///
pub fn get_idle_time_macos() -> Option<u64> {
    use std::process::Command;

    let output = Command::new("ioreg")
        .args(["-c", "IOHIDSystem", "-d", "4"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains("HIDIdleTime") {
            if let Some(value_str) = line.split('=').nth(1) {
                let value_str = value_str.trim();
                if let Ok(nanos) = value_str.parse::<u64>() {
                    return Some(nanos / 1_000_000_000);
                }
            }
        }
    }

    None
}

///
pub fn get_mouse_position_macos() -> Option<MousePosition> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;

    let event = CGEvent::new(source).ok()?;
    let location = event.location();

    Some(MousePosition {
        x: location.x as i32,
        y: location.y as i32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_active_window_returns_result() {
        let result = get_active_window_macos();
        assert!(result.is_ok());
    }

    #[test]
    fn get_idle_time_returns_result() {
        let idle = get_idle_time_macos();
        if let Some(secs) = idle {
            assert!(secs < 86400 * 365); // less than 1 year
        }
    }

    #[test]
    fn get_mouse_position_returns_result() {
        let pos = get_mouse_position_macos();
        if let Some(p) = pos {
            assert!(p.x >= 0 && p.x < 32000);
            assert!(p.y >= 0 && p.y < 32000);
        }
    }
}
