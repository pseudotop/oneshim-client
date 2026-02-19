//! 데스크톱 알림 포트.
//!
//! 구현: `oneshim-ui` crate (notify-rust, tray-icon)

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::suggestion::Suggestion;

/// 데스크톱 알림 인터페이스
#[async_trait]
pub trait DesktopNotifier: Send + Sync {
    /// 제안 수신 알림 표시
    async fn show_suggestion(&self, suggestion: &Suggestion) -> Result<(), CoreError>;

    /// 일반 알림 표시 (제목 + 본문)
    async fn show_notification(&self, title: &str, body: &str) -> Result<(), CoreError>;

    /// 에러 알림 표시
    async fn show_error(&self, message: &str) -> Result<(), CoreError>;
}
