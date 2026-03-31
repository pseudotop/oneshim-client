use crate::error::VisionError;
use image::DynamicImage;
use oneshim_core::models::context::WindowBounds;
use tracing::debug;
use xcap::Monitor;

#[derive(Clone)]
pub struct ScreenCapture;

impl ScreenCapture {
    pub fn new() -> Self {
        Self
    }

    pub fn capture_primary(&self) -> Result<DynamicImage, VisionError> {
        let monitors = Monitor::all()
            .map_err(|e| VisionError::Internal(format!("Failed to query monitor list: {e}")))?;

        let monitor = monitors
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| Monitor::all().ok()?.into_iter().next())
            .ok_or_else(|| VisionError::Internal("Monitor not found".to_string()))?;

        let image = monitor
            .capture_image()
            .map_err(|e| VisionError::Internal(format!("Screen capture failed: {e}")))?;

        debug!(
            "screen capture completed: {}x{}",
            image.width(),
            image.height()
        );

        Ok(DynamicImage::ImageRgba8(image))
    }

    /// Capture the monitor containing the active window.
    ///
    /// Falls back to the primary monitor when:
    /// - only one monitor exists,
    /// - no window bounds are provided, or
    /// - the window center does not intersect any known monitor.
    pub fn capture_for_window(
        &self,
        bounds: Option<&WindowBounds>,
    ) -> Result<DynamicImage, VisionError> {
        let monitors = Monitor::all()
            .map_err(|e| VisionError::Internal(format!("Failed to query monitor list: {e}")))?;

        // Single monitor or no bounds — primary capture
        if monitors.len() <= 1 || bounds.is_none() {
            return self.capture_primary();
        }

        let bounds = bounds.unwrap();
        // Window center point
        let cx = bounds.x + (bounds.width as i32 / 2);
        let cy = bounds.y + (bounds.height as i32 / 2);

        // Find the monitor whose rect contains the window center
        let target = monitors.iter().find(|m| {
            let Ok(mx) = m.x() else { return false };
            let Ok(my) = m.y() else { return false };
            let Ok(mw) = m.width() else { return false };
            let Ok(mh) = m.height() else { return false };
            let mw = mw as i32;
            let mh = mh as i32;
            cx >= mx && cx < mx + mw && cy >= my && cy < my + mh
        });

        let monitor = target
            .or_else(|| monitors.iter().find(|m| m.is_primary().unwrap_or(false)))
            .or(monitors.first())
            .ok_or_else(|| VisionError::Internal("No monitor found".to_string()))?;

        let image = monitor
            .capture_image()
            .map_err(|e| VisionError::Internal(format!("Screen capture failed: {e}")))?;

        debug!(
            "captured monitor at ({}, {}) — {}x{}",
            monitor.x().unwrap_or(0),
            monitor.y().unwrap_or(0),
            image.width(),
            image.height()
        );

        Ok(DynamicImage::ImageRgba8(image))
    }

    pub fn capture_monitor(&self, index: usize) -> Result<DynamicImage, VisionError> {
        let monitors = Monitor::all()
            .map_err(|e| VisionError::Internal(format!("Failed to query monitor list: {e}")))?;

        let monitor = monitors
            .into_iter()
            .nth(index)
            .ok_or_else(|| VisionError::Internal(format!("Monitor index {index} not found")))?;

        let image = monitor
            .capture_image()
            .map_err(|e| VisionError::Internal(format!("Screen capture failed: {e}")))?;

        Ok(DynamicImage::ImageRgba8(image))
    }

    pub fn monitor_count() -> Result<usize, VisionError> {
        Monitor::all()
            .map(|m| m.len())
            .map_err(|e| VisionError::Internal(format!("Failed to query monitor list: {e}")))
    }
}

impl Default for ScreenCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_for_window_with_none_bounds_uses_primary() {
        let capture = ScreenCapture::new();
        // Should not panic — falls back to primary.
        // May fail in headless CI (no display), so we only assert no panic.
        let _ = capture.capture_for_window(None);
    }

    #[test]
    fn capture_for_window_with_bounds_does_not_panic() {
        let capture = ScreenCapture::new();
        let bounds = WindowBounds {
            x: 100,
            y: 200,
            width: 800,
            height: 600,
        };
        // Should not panic regardless of monitor configuration.
        let _ = capture.capture_for_window(Some(&bounds));
    }
}
