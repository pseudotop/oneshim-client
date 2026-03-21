//! Desktop notification port — defines the contract for displaying
//! OS-native notifications (suggestions, alerts, errors).
//! Implemented by `DesktopNotifierImpl` in platform-specific adapters.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::suggestion::Suggestion;

#[async_trait]
pub trait DesktopNotifier: Send + Sync {
    async fn show_suggestion(&self, suggestion: &Suggestion) -> Result<(), CoreError>;

    async fn show_notification(&self, title: &str, body: &str) -> Result<(), CoreError>;

    async fn show_error(&self, message: &str) -> Result<(), CoreError>;
}
