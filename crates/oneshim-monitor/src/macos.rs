//! macOS 플랫폼 — 활성 창 감지, 유휴 시간, 마우스 위치.
//!
//! CoreGraphics + Accessibility API 기반.

use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use oneshim_core::error::CoreError;
use oneshim_core::models::context::{MousePosition, WindowBounds, WindowInfo};
use tracing::debug;

/// macOS 활성 창 정보 조회 (위치/크기 포함)
pub fn get_active_window_macos() -> Result<Option<WindowInfo>, CoreError> {
    use std::process::Command;

    // AppleScript로 활성 앱 정보 + 창 위치/크기 가져오기
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
        .map_err(|e| CoreError::Internal(format!("osascript 실행 실패: {e}")))?;

    if !output.status.success() {
        debug!("활성 창 감지 실패 (osascript)");
        return Ok(None);
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let parts: Vec<&str> = result.split('|').collect();

    if parts.is_empty() {
        return Ok(None);
    }

    let app_name = parts[0].to_string();
    let title = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

    // 창 위치/크기 파싱
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
        "활성 창: {app_name} — {title} ({:?})",
        bounds.map(|b| format!("{}x{} at ({},{})", b.width, b.height, b.x, b.y))
    );

    Ok(Some(WindowInfo {
        title,
        app_name,
        pid: 0, // osascript로는 PID 가져오기 어려움
        bounds,
    }))
}

/// macOS 유휴 시간 조회 (초 단위)
///
/// IOKit의 HIDIdleTime을 사용하여 마지막 사용자 입력 이후 경과 시간을 반환.
/// 실패 시 None 반환.
pub fn get_idle_time_macos() -> Option<u64> {
    use std::process::Command;

    // ioreg를 사용하여 HIDIdleTime 조회
    // HIDIdleTime은 나노초 단위
    let output = Command::new("ioreg")
        .args(["-c", "IOHIDSystem", "-d", "4"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // HIDIdleTime 파싱
    for line in stdout.lines() {
        if line.contains("HIDIdleTime") {
            // "HIDIdleTime" = 1234567890 형식
            if let Some(value_str) = line.split('=').nth(1) {
                let value_str = value_str.trim();
                if let Ok(nanos) = value_str.parse::<u64>() {
                    // 나노초 → 초 변환
                    return Some(nanos / 1_000_000_000);
                }
            }
        }
    }

    None
}

/// macOS 마우스 커서 위치 조회
///
/// Core Graphics API를 사용하여 현재 마우스 위치를 반환.
/// HIDSystemState 이벤트 소스로 synthetic 이벤트를 생성하면 현재 마우스 위치를 포함.
/// 실패 시 None 반환.
pub fn get_mouse_position_macos() -> Option<MousePosition> {
    // HIDSystemState에서 이벤트 소스 생성
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;

    // 이벤트 생성 - 현재 마우스 위치 포함
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
        // CI 환경에서는 창 감지가 실패할 수 있음
        let result = get_active_window_macos();
        assert!(result.is_ok());
    }

    #[test]
    fn get_idle_time_returns_result() {
        // 유휴 시간 조회 테스트
        let idle = get_idle_time_macos();
        // macOS에서는 대부분 값을 반환해야 함
        // CI 환경에서는 None일 수 있음
        if let Some(secs) = idle {
            // 유휴 시간이 음수가 아닌지 확인 (항상 true)
            assert!(secs < 86400 * 365); // 1년 미만
        }
    }

    #[test]
    fn get_mouse_position_returns_result() {
        // 마우스 위치 조회 테스트
        let pos = get_mouse_position_macos();
        // CI 환경에서는 None일 수 있음
        if let Some(p) = pos {
            // 일반적인 화면 해상도 범위 내
            assert!(p.x >= 0 && p.x < 32000);
            assert!(p.y >= 0 && p.y < 32000);
        }
    }
}
