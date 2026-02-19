//! 데스크톱 알림 어댑터.
//!
//! `DesktopNotifier` 포트 구현. notify-rust 기반.

use async_trait::async_trait;
use notify_rust::Notification;
use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::{Priority, Suggestion};
use oneshim_core::ports::notifier::DesktopNotifier;
use tracing::{debug, warn};

/// 데스크톱 알림 어댑터 — `DesktopNotifier` 포트 구현
pub struct DesktopNotifierImpl;

impl DesktopNotifierImpl {
    /// 새 알림 어댑터 생성
    pub fn new() -> Self {
        Self
    }
}

impl Default for DesktopNotifierImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DesktopNotifier for DesktopNotifierImpl {
    async fn show_suggestion(&self, suggestion: &Suggestion) -> Result<(), CoreError> {
        let title = match &suggestion.priority {
            Priority::Critical => "🔴 긴급 제안",
            Priority::High => "🟠 중요 제안",
            Priority::Medium => "🔵 제안",
            Priority::Low => "⚪ 참고",
        };

        let body = if suggestion.content.len() > 100 {
            format!("{}...", &suggestion.content[..100])
        } else {
            suggestion.content.clone()
        };

        debug!(
            "제안 알림: {} ({:?})",
            suggestion.suggestion_id, suggestion.priority
        );

        Notification::new()
            .summary(title)
            .body(&body)
            .appname("ONESHIM")
            .show()
            .map_err(|e| CoreError::Internal(format!("알림 표시 실패: {e}")))?;

        Ok(())
    }

    async fn show_notification(&self, title: &str, body: &str) -> Result<(), CoreError> {
        debug!("알림: {title}");

        Notification::new()
            .summary(title)
            .body(body)
            .appname("ONESHIM")
            .show()
            .map_err(|e| CoreError::Internal(format!("알림 표시 실패: {e}")))?;

        Ok(())
    }

    async fn show_error(&self, message: &str) -> Result<(), CoreError> {
        warn!("에러 알림: {message}");

        Notification::new()
            .summary("ONESHIM 에러")
            .body(message)
            .appname("ONESHIM")
            .show()
            .map_err(|e| CoreError::Internal(format!("에러 알림 표시 실패: {e}")))?;

        Ok(())
    }
}
