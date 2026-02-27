//! Linux(Landlock+seccomp), macOS(Seatbelt), Windows(Job Objects)

use async_trait::async_trait;

use crate::config::SandboxConfig;
use crate::error::CoreError;
use crate::models::automation::AutomationAction;

#[derive(Debug, Clone)]
pub struct SandboxCapabilities {
    pub filesystem_isolation: bool,
    pub syscall_filtering: bool,
    pub network_isolation: bool,
    pub resource_limits: bool,
    pub process_isolation: bool,
}

#[async_trait]
pub trait Sandbox: Send + Sync {
    fn platform(&self) -> &str;

    fn is_available(&self) -> bool;

    async fn execute_sandboxed(
        &self,
        action: &AutomationAction,
        config: &SandboxConfig,
    ) -> Result<(), CoreError>;

    fn capabilities(&self) -> SandboxCapabilities;
}
