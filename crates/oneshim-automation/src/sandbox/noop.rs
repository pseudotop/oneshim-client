//! NoOp 샌드박스.
//!
//! 모든 액션을 제한 없이 통과시키는 폴백 구현.
//! 샌드박스 비활성 또는 미지원 플랫폼에서 사용.

use async_trait::async_trait;

use oneshim_core::config::SandboxConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::automation::AutomationAction;
use oneshim_core::ports::sandbox::{Sandbox, SandboxCapabilities};

/// 제한 없이 모든 액션을 통과시키는 NoOp 샌드박스
pub struct NoOpSandbox;

#[async_trait]
impl Sandbox for NoOpSandbox {
    fn platform(&self) -> &str {
        "noop"
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn execute_sandboxed(
        &self,
        action: &AutomationAction,
        _config: &SandboxConfig,
    ) -> Result<(), CoreError> {
        tracing::debug!(action = ?action, "NoOp 샌드박스: 제한 없이 실행");
        Ok(())
    }

    fn capabilities(&self) -> SandboxCapabilities {
        SandboxCapabilities {
            filesystem_isolation: false,
            syscall_filtering: false,
            network_isolation: false,
            resource_limits: false,
            process_isolation: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_execute_succeeds() {
        let sandbox = NoOpSandbox;
        let action = AutomationAction::MouseMove { x: 100, y: 200 };
        let config = SandboxConfig::default();
        let result = sandbox.execute_sandboxed(&action, &config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn noop_capabilities_all_false() {
        let sandbox = NoOpSandbox;
        let caps = sandbox.capabilities();
        assert!(!caps.filesystem_isolation);
        assert!(!caps.syscall_filtering);
        assert!(!caps.network_isolation);
        assert!(!caps.resource_limits);
        assert!(!caps.process_isolation);
    }

    #[test]
    fn noop_is_available() {
        let sandbox = NoOpSandbox;
        assert!(sandbox.is_available());
        assert_eq!(sandbox.platform(), "noop");
    }
}
