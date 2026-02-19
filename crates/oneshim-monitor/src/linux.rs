//! Linux 플랫폼 지원.
//!
//! X11 및 Wayland 환경에서 활성 창 감지 및 유휴 시간 측정.
//!
//! ## X11 지원
//! - `xdotool`을 통한 활성 창 감지
//! - `xprintidle`을 통한 유휴 시간 측정
//!
//! ## Wayland 지원
//! Wayland는 보안상 이유로 표준 API가 제한적입니다.
//! GNOME/KDE 등 컴포지터별로 다른 접근 방식이 필요합니다.
//! 현재는 X11 fallback (XWayland)에 의존합니다.

use oneshim_core::error::CoreError;
use oneshim_core::models::context::{MousePosition, WindowInfo};
use std::process::Command;
use tracing::{debug, warn};

/// 현재 디스플레이 서버 유형
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    X11,
    Wayland,
    Unknown,
}

/// 현재 사용 중인 디스플레이 서버 감지
pub fn detect_display_server() -> DisplayServer {
    // XDG_SESSION_TYPE 환경변수 확인 (systemd 기반 시스템)
    if let Ok(session_type) = std::env::var("XDG_SESSION_TYPE") {
        match session_type.to_lowercase().as_str() {
            "x11" => return DisplayServer::X11,
            "wayland" => return DisplayServer::Wayland,
            _ => {}
        }
    }

    // WAYLAND_DISPLAY 환경변수 확인
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return DisplayServer::Wayland;
    }

    // DISPLAY 환경변수 확인 (X11)
    if std::env::var("DISPLAY").is_ok() {
        return DisplayServer::X11;
    }

    DisplayServer::Unknown
}

/// Linux에서 활성 창 정보 가져오기
///
/// X11에서는 `xdotool`을 사용하고, Wayland에서는 XWayland fallback을 시도합니다.
pub fn get_active_window_linux() -> Result<Option<WindowInfo>, CoreError> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_active_window_x11(),
        DisplayServer::Wayland => {
            // Wayland에서는 XWayland를 통한 X11 앱만 감지 가능
            debug!("Wayland 감지됨 - XWayland fallback 시도");
            get_active_window_x11().or_else(|_| {
                warn!("Wayland에서 활성 창 감지 제한됨 - X11 앱만 지원");
                Ok(None)
            })
        }
        DisplayServer::Unknown => {
            debug!("디스플레이 서버 감지 실패");
            Ok(None)
        }
    }
}

/// X11에서 xdotool을 사용하여 활성 창 정보 가져오기
fn get_active_window_x11() -> Result<Option<WindowInfo>, CoreError> {
    // 활성 창 ID 가져오기
    let window_id = match Command::new("xdotool").arg("getactivewindow").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!("xdotool 실패: {}", stderr);
            return Ok(None);
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xdotool 미설치 - 'sudo apt install xdotool' 실행 필요");
            } else {
                debug!("xdotool 실행 실패: {}", e);
            }
            return Ok(None);
        }
    };

    if window_id.is_empty() {
        return Ok(None);
    }

    // 창 제목 가져오기
    let title = Command::new("xdotool")
        .args(["getwindowname", &window_id])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // 창 PID 가져오기
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

    // 프로세스 이름 가져오기 (PID로부터)
    let app_name = if pid > 0 {
        get_process_name(pid).unwrap_or_else(|| "Unknown".to_string())
    } else {
        "Unknown".to_string()
    };

    // 창 위치/크기 가져오기
    let bounds = get_window_geometry_x11(&window_id);

    debug!("활성 창: {} - {} (PID: {})", app_name, title, pid);

    Ok(Some(WindowInfo {
        title,
        app_name,
        pid,
        bounds,
    }))
}

/// xdotool을 사용하여 창 위치/크기 가져오기
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

/// PID로부터 프로세스 이름 가져오기
fn get_process_name(pid: u32) -> Option<String> {
    // /proc/{pid}/comm 파일에서 프로세스 이름 읽기
    let comm_path = format!("/proc/{}/comm", pid);
    std::fs::read_to_string(&comm_path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Linux에서 유휴 시간 가져오기 (밀리초 → 초)
///
/// X11에서는 `xprintidle`을 사용하고, Wayland에서는 제한됩니다.
pub fn get_idle_time_linux() -> Option<u64> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_idle_time_x11(),
        DisplayServer::Wayland => {
            // Wayland에서 유휴 시간 감지는 컴포지터별로 다름
            // GNOME: org.gnome.Mutter.IdleMonitor D-Bus API
            // KDE: org.kde.KIdleTime D-Bus API
            // 현재는 XWayland fallback 시도
            get_idle_time_x11().or_else(|| {
                debug!("Wayland에서 유휴 감지 제한됨");
                None
            })
        }
        DisplayServer::Unknown => None,
    }
}

/// X11에서 xprintidle을 사용하여 유휴 시간 가져오기
fn get_idle_time_x11() -> Option<u64> {
    let output = match Command::new("xprintidle").output() {
        Ok(output) if output.status.success() => output,
        Ok(_) => {
            return None;
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xprintidle 미설치 - 'sudo apt install xprintidle' 실행 필요");
            }
            return None;
        }
    };

    // xprintidle 출력은 밀리초 단위
    let ms_str = String::from_utf8_lossy(&output.stdout);
    let ms: u64 = ms_str.trim().parse().ok()?;

    // 초 단위로 변환
    Some(ms / 1000)
}

/// Linux 마우스 커서 위치 조회
///
/// X11에서는 `xdotool getmouselocation`을 사용합니다.
/// Wayland에서는 XWayland fallback을 시도합니다.
pub fn get_mouse_position_linux() -> Option<MousePosition> {
    let display_server = detect_display_server();

    match display_server {
        DisplayServer::X11 => get_mouse_position_x11(),
        DisplayServer::Wayland => {
            // Wayland에서는 XWayland를 통한 마우스 위치 감지 시도
            get_mouse_position_x11().or_else(|| {
                debug!("Wayland에서 마우스 위치 감지 제한됨");
                None
            })
        }
        DisplayServer::Unknown => None,
    }
}

/// X11에서 xdotool을 사용하여 마우스 위치 가져오기
fn get_mouse_position_x11() -> Option<MousePosition> {
    // xdotool getmouselocation 출력 예시:
    // x:1234 y:567 screen:0 window:12345678
    let output = match Command::new("xdotool").arg("getmouselocation").output() {
        Ok(output) if output.status.success() => output,
        Ok(_) => {
            return None;
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("xdotool 미설치 - 마우스 위치 감지 불가");
            }
            return None;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut x: Option<i32> = None;
    let mut y: Option<i32> = None;

    // "x:1234 y:567" 형식 파싱
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
        // 테스트 환경에 따라 다른 결과가 나올 수 있음
        assert!(matches!(
            server,
            DisplayServer::X11 | DisplayServer::Wayland | DisplayServer::Unknown
        ));
    }

    #[test]
    fn get_process_name_from_proc() {
        // PID 1은 항상 존재 (init/systemd)
        let name = get_process_name(1);
        assert!(name.is_some());
        let name = name.unwrap();
        assert!(!name.is_empty());
    }

    #[test]
    fn active_window_returns_option() {
        // xdotool이 없어도 패닉하지 않아야 함
        let result = get_active_window_linux();
        assert!(result.is_ok());
    }

    #[test]
    fn idle_time_returns_option() {
        // xprintidle이 없어도 패닉하지 않아야 함
        let result = get_idle_time_linux();
        // None이거나 Some(0 이상)
        if let Some(secs) = result {
            assert!(secs < 86400 * 365); // 1년 미만이어야 합리적
        }
    }

    #[test]
    fn mouse_position_returns_option() {
        // xdotool이 없어도 패닉하지 않아야 함
        let result = get_mouse_position_linux();
        // None이거나 Some(합리적인 좌표)
        if let Some(pos) = result {
            // 일반적인 화면 해상도 범위 내
            assert!(pos.x >= 0 && pos.x < 32000);
            assert!(pos.y >= 0 && pos.y < 32000);
        }
    }
}
