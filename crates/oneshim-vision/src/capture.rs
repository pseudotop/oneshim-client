use image::DynamicImage;
use oneshim_core::error::CoreError;
use tracing::debug;
use xcap::Monitor;

pub struct ScreenCapture;

impl ScreenCapture {
    pub fn new() -> Self {
        Self
    }

    pub fn capture_primary(&self) -> Result<DynamicImage, CoreError> {
        let monitors = Monitor::all()
            .map_err(|e| CoreError::Internal(format!("Failed to query monitor list: {e}")))?;

        let monitor = monitors
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| Monitor::all().ok()?.into_iter().next())
            .ok_or_else(|| CoreError::Internal("Monitor not found".to_string()))?;

        let image = monitor
            .capture_image()
            .map_err(|e| CoreError::Internal(format!("Screen capture failed: {e}")))?;

        debug!(
            "screen capture completed: {}x{}",
            image.width(),
            image.height()
        );

        Ok(DynamicImage::ImageRgba8(image))
    }

    pub fn capture_monitor(&self, index: usize) -> Result<DynamicImage, CoreError> {
        let monitors = Monitor::all()
            .map_err(|e| CoreError::Internal(format!("Failed to query monitor list: {e}")))?;

        let monitor = monitors
            .into_iter()
            .nth(index)
            .ok_or_else(|| CoreError::Internal(format!("Monitor index {index} not found")))?;

        let image = monitor
            .capture_image()
            .map_err(|e| CoreError::Internal(format!("Screen capture failed: {e}")))?;

        Ok(DynamicImage::ImageRgba8(image))
    }

    pub fn monitor_count() -> Result<usize, CoreError> {
        Monitor::all()
            .map(|m| m.len())
            .map_err(|e| CoreError::Internal(format!("Failed to query monitor list: {e}")))
    }
}

impl Default for ScreenCapture {
    fn default() -> Self {
        Self::new()
    }
}
