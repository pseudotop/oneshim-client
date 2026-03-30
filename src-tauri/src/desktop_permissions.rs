use serde::Serialize;

use oneshim_core::ports::accessibility::AccessibilityExtractor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum DesktopPermissionState {
    Granted,
    NeedsAttention,
    NotRequired,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesktopPermissionEntry {
    pub state: DesktopPermissionState,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesktopPermissionSnapshot {
    pub platform: String,
    pub accessibility: DesktopPermissionEntry,
    pub screen_capture: DesktopPermissionEntry,
}

pub fn get_desktop_permission_snapshot() -> DesktopPermissionSnapshot {
    DesktopPermissionSnapshot {
        platform: current_platform().to_string(),
        accessibility: accessibility_permission_entry(),
        screen_capture: screen_capture_permission_entry(),
    }
}

fn current_platform() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "unknown"
    }
}

fn granted(status_reason: Option<&str>) -> DesktopPermissionEntry {
    DesktopPermissionEntry {
        state: DesktopPermissionState::Granted,
        status_reason: status_reason.map(ToOwned::to_owned),
    }
}

fn needs_attention(status_reason: &str) -> DesktopPermissionEntry {
    DesktopPermissionEntry {
        state: DesktopPermissionState::NeedsAttention,
        status_reason: Some(status_reason.to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
fn not_required(status_reason: Option<&str>) -> DesktopPermissionEntry {
    DesktopPermissionEntry {
        state: DesktopPermissionState::NotRequired,
        status_reason: status_reason.map(ToOwned::to_owned),
    }
}

#[cfg(not(target_os = "macos"))]
fn unavailable(status_reason: &str) -> DesktopPermissionEntry {
    DesktopPermissionEntry {
        state: DesktopPermissionState::Unavailable,
        status_reason: Some(status_reason.to_string()),
    }
}

#[cfg(target_os = "macos")]
fn accessibility_permission_entry() -> DesktopPermissionEntry {
    let extractor = oneshim_vision::accessibility::MacOsNativeAccessibility::new();
    if extractor.has_permission() {
        granted(Some("macos_accessibility_granted"))
    } else {
        needs_attention("macos_accessibility_missing")
    }
}

#[cfg(target_os = "windows")]
fn accessibility_permission_entry() -> DesktopPermissionEntry {
    not_required(Some("windows_uia_no_permission_required"))
}

#[cfg(target_os = "linux")]
fn accessibility_permission_entry() -> DesktopPermissionEntry {
    let extractor = oneshim_vision::accessibility::LinuxAccessibility::new();
    if extractor.has_permission() {
        not_required(Some("linux_atspi_ready"))
    } else {
        needs_attention("linux_atspi_session_unavailable")
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn accessibility_permission_entry() -> DesktopPermissionEntry {
    unavailable("accessibility_unsupported")
}

#[cfg(target_os = "macos")]
fn screen_capture_permission_entry() -> DesktopPermissionEntry {
    if macos_screen_capture_access_granted() {
        granted(Some("macos_screen_capture_granted"))
    } else {
        needs_attention("macos_screen_capture_missing")
    }
}

#[cfg(not(target_os = "macos"))]
fn screen_capture_permission_entry() -> DesktopPermissionEntry {
    match oneshim_vision::capture::ScreenCapture::monitor_count() {
        Ok(count) if count > 0 => not_required(Some("screen_capture_ready")),
        Ok(_) => unavailable("screen_capture_no_monitors"),
        Err(_) => unavailable("screen_capture_probe_failed"),
    }
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
}

#[cfg(target_os = "macos")]
fn macos_screen_capture_access_granted() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_name_is_known() {
        assert!(!current_platform().is_empty());
    }
}
