use crate::error::CoreError;
use crate::models::intent::ElementBounds;
use crate::models::ui_scene::NormalizedBounds;

/// Detected rectangular GUI boundary from image analysis.
#[derive(Debug, Clone)]
pub struct DetectedRectangle {
    pub bounds: ElementBounds,
    pub bounds_normalized: NormalizedBounds,
    pub confidence: f64,
    pub classification: Option<String>,
}

/// Detects rectangular GUI element boundaries in a screen capture.
/// Synchronous — callers wrap in spawn_blocking if needed.
pub trait RectangleDetector: Send + Sync {
    fn detect_rectangles(
        &self,
        image: &[u8],
        image_width: u32,
        image_height: u32,
        min_size: f32,
        max_results: usize,
    ) -> Result<Vec<DetectedRectangle>, CoreError>;

    fn provider_name(&self) -> &str;
}
