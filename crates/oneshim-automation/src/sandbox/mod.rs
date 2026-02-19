//! 크로스 플랫폼 OS 네이티브 샌드박스.
//!
//! Docker 없이 Linux/macOS/Windows 각각의 커널 기능을 조합하여
//! 최소 권한 실행 환경을 제공한다.
//!
//! - Linux: Landlock (FS) + seccomp-bpf (시스콜) + setrlimit (리소스)
//! - macOS: Seatbelt/sandbox-exec (.sbpl 프로필) + setrlimit
//! - Windows: Job Objects (리소스) + Restricted Tokens (권한 축소)
//! - NoOp: 미지원 플랫폼 또는 비활성 시 폴백

mod noop;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub use noop::NoOpSandbox;

#[cfg(target_os = "linux")]
pub use linux::LinuxSandbox;
#[cfg(target_os = "macos")]
pub use macos::MacOsSandbox;
#[cfg(target_os = "windows")]
pub use windows::WindowsSandbox;

use oneshim_core::config::SandboxConfig;
use oneshim_core::ports::sandbox::Sandbox;
use std::sync::Arc;

/// 현재 플랫폼에 맞는 샌드박스 생성
///
/// `config.enabled`가 false이면 항상 NoOp 반환.
/// 플랫폼 샌드박스가 사용 불가능(`is_available()` false)해도 NoOp 폴백.
pub fn create_platform_sandbox(config: &SandboxConfig) -> Arc<dyn Sandbox> {
    if !config.enabled {
        return Arc::new(NoOpSandbox);
    }

    create_native_sandbox()
}

/// 플랫폼 네이티브 샌드박스 생성 (내부)
fn create_native_sandbox() -> Arc<dyn Sandbox> {
    #[cfg(target_os = "linux")]
    {
        let sandbox = LinuxSandbox::new();
        if sandbox.is_available() {
            return Arc::new(sandbox);
        }
        tracing::warn!("Linux 샌드박스 사용 불가 — NoOp 폴백");
    }

    #[cfg(target_os = "macos")]
    {
        let sandbox = MacOsSandbox::new();
        if sandbox.is_available() {
            return Arc::new(sandbox);
        }
        tracing::warn!("macOS 샌드박스 사용 불가 — NoOp 폴백");
    }

    #[cfg(target_os = "windows")]
    {
        let sandbox = WindowsSandbox::new();
        if sandbox.is_available() {
            return Arc::new(sandbox);
        }
        tracing::warn!("Windows 샌드박스 사용 불가 — NoOp 폴백");
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        tracing::warn!("미지원 플랫폼 — NoOp 폴백");
    }

    Arc::new(NoOpSandbox)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_config_returns_noop() {
        let config = SandboxConfig {
            enabled: false,
            ..Default::default()
        };
        let sandbox = create_platform_sandbox(&config);
        assert_eq!(sandbox.platform(), "noop");
    }

    #[test]
    fn enabled_config_returns_platform_sandbox() {
        let config = SandboxConfig {
            enabled: true,
            ..Default::default()
        };
        let sandbox = create_platform_sandbox(&config);
        // 플랫폼에 따라 다른 sandbox가 반환되지만, 항상 유효해야 함
        assert!(sandbox.is_available());
    }

    #[test]
    fn factory_returns_send_sync() {
        let config = SandboxConfig::default();
        let sandbox = create_platform_sandbox(&config);
        // Arc<dyn Sandbox>는 Send + Sync여야 함
        fn assert_send_sync<T: Send + Sync>(_: &T) {}
        assert_send_sync(&sandbox);
    }
}
