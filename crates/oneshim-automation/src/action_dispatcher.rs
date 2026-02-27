use async_trait::async_trait;
use std::sync::Arc;

use oneshim_core::config::SandboxConfig;
use oneshim_core::ports::sandbox::Sandbox;

use crate::controller::{AutomationAction, CommandResult};

#[async_trait]
pub trait AutomationActionDispatcher: Send + Sync {
    async fn dispatch(&self, action: &AutomationAction, config: &SandboxConfig) -> CommandResult;
}

pub struct SandboxActionDispatcher {
    sandbox: Arc<dyn Sandbox>,
}

impl SandboxActionDispatcher {
    pub fn new(sandbox: Arc<dyn Sandbox>) -> Self {
        Self { sandbox }
    }
}

#[async_trait]
impl AutomationActionDispatcher for SandboxActionDispatcher {
    async fn dispatch(&self, action: &AutomationAction, config: &SandboxConfig) -> CommandResult {
        tracing::info!(
            action = ?action,
            sandbox = self.sandbox.platform(),
            profile = ?config.profile,
            "자동화 명령 execution (policy 기반 샌드박스 경유)"
        );

        if let Err(e) = self.sandbox.execute_sandboxed(action, config).await {
            tracing::error!(error = %e, "sandbox execution failure");
            return CommandResult::Failed(format!("Sandbox execution failed: {}", e));
        }

        match action {
            AutomationAction::MouseMove { x, y } => {
                tracing::debug!(x, y, "mouse");
                CommandResult::Success
            }
            AutomationAction::MouseClick { button, x, y } => {
                tracing::debug!(button, x, y, "mouse click");
                CommandResult::Success
            }
            AutomationAction::KeyType { text } => {
                tracing::debug!(text_len = text.len(), "text");
                CommandResult::Success
            }
            AutomationAction::KeyPress { key } => {
                tracing::debug!(key, "key");
                CommandResult::Success
            }
            AutomationAction::KeyRelease { key } => {
                tracing::debug!(key, "key");
                CommandResult::Success
            }
            AutomationAction::Hotkey { keys } => {
                tracing::debug!(?keys, "key execution");
                CommandResult::Success
            }
        }
    }
}
