use async_trait::async_trait;
use notify_rust::Notification;
use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::{Priority, Suggestion};
use oneshim_core::ports::notifier::DesktopNotifier;
use tracing::{debug, warn};

pub struct DesktopNotifierImpl;

impl DesktopNotifierImpl {
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
            Priority::Critical => "🔴 긴급 suggestion",
            Priority::High => "🟠 중요 suggestion",
            Priority::Medium => "🔵 suggestion",
            Priority::Low => "⚪ 참고",
        };

        let body = if suggestion.content.len() > 100 {
            format!("{}...", &suggestion.content[..100])
        } else {
            suggestion.content.clone()
        };

        debug!(
            "suggestion notification: {} ({:?})",
            suggestion.suggestion_id, suggestion.priority
        );

        Notification::new()
            .summary(title)
            .body(&body)
            .appname("ONESHIM")
            .show()
            .map_err(|e| CoreError::Internal(format!("notification display failure: {e}")))?;

        Ok(())
    }

    async fn show_notification(&self, title: &str, body: &str) -> Result<(), CoreError> {
        debug!("notification: {title}");

        Notification::new()
            .summary(title)
            .body(body)
            .appname("ONESHIM")
            .show()
            .map_err(|e| CoreError::Internal(format!("notification display failure: {e}")))?;

        Ok(())
    }

    async fn show_error(&self, message: &str) -> Result<(), CoreError> {
        warn!("error notification: {message}");

        Notification::new()
            .summary("ONESHIM error")
            .body(message)
            .appname("ONESHIM")
            .show()
            .map_err(|e| CoreError::Internal(format!("error notification display failure: {e}")))?;

        Ok(())
    }
}
