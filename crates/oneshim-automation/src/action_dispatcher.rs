//! 자동화 액션 디스패처.
//!
//! `AutomationController`에서 실제 액션 실행 책임을 분리해
//! 정책/감사 오케스트레이션과 실행 구현을 분리한다.

use async_trait::async_trait;
use std::sync::Arc;

use oneshim_core::config::SandboxConfig;
use oneshim_core::ports::sandbox::Sandbox;

use crate::controller::{AutomationAction, CommandResult};

/// 자동화 액션 실행 포트.
#[async_trait]
pub trait AutomationActionDispatcher: Send + Sync {
    async fn dispatch(&self, action: &AutomationAction, config: &SandboxConfig) -> CommandResult;
}

/// 샌드박스 기반 기본 액션 디스패처.
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
            "자동화 명령 실행 (정책 기반 샌드박스 경유)"
        );

        if let Err(e) = self.sandbox.execute_sandboxed(action, config).await {
            tracing::error!(error = %e, "샌드박스 실행 실패");
            return CommandResult::Failed(format!("샌드박스 실행 실패: {}", e));
        }

        // 실제 입력 시뮬레이션은 enigo 통합 시 구현
        // 현재는 로깅만 수행
        match action {
            AutomationAction::MouseMove { x, y } => {
                tracing::debug!(x, y, "마우스 이동");
                CommandResult::Success
            }
            AutomationAction::MouseClick { button, x, y } => {
                tracing::debug!(button, x, y, "마우스 클릭");
                CommandResult::Success
            }
            AutomationAction::KeyType { text } => {
                tracing::debug!(text_len = text.len(), "텍스트 입력");
                CommandResult::Success
            }
            AutomationAction::KeyPress { key } => {
                tracing::debug!(key, "키 누름");
                CommandResult::Success
            }
            AutomationAction::KeyRelease { key } => {
                tracing::debug!(key, "키 놓음");
                CommandResult::Success
            }
            AutomationAction::Hotkey { keys } => {
                tracing::debug!(?keys, "단축키 실행");
                CommandResult::Success
            }
        }
    }
}
