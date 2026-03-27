//! Overlay driver port — defines the contract for rendering transparent
//! highlight overlays on screen elements (MagicOverlay, heatmap ghosts).
//! Implemented by Tauri WebView overlay in `src-tauri`.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::gui::{HighlightHandle, HighlightRequest};
use crate::models::ui_scene::UiScene;

#[async_trait]
pub trait OverlayDriver: Send + Sync {
    async fn show_highlights(&self, req: HighlightRequest) -> Result<HighlightHandle, CoreError>;

    async fn clear_highlights(&self, handle_id: &str) -> Result<(), CoreError>;

    /// Render all elements from a UiScene as detection overlay boxes.
    async fn show_detection(&self, scene: &UiScene) -> Result<(), CoreError>;

    /// Clear all detection overlay boxes.
    async fn clear_detection(&self) -> Result<(), CoreError>;
}
