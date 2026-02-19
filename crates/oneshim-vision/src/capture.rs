//! 스크린 캡처.
//!
//! xcap 기반 멀티모니터 캡처.

use image::DynamicImage;
use oneshim_core::error::CoreError;
use tracing::debug;
use xcap::Monitor;

/// 스크린 캡처 — xcap 기반
pub struct ScreenCapture;

impl ScreenCapture {
    /// 새 캡처 인스턴스 생성
    pub fn new() -> Self {
        Self
    }

    /// 주 모니터 스크린 캡처
    pub fn capture_primary(&self) -> Result<DynamicImage, CoreError> {
        let monitors = Monitor::all()
            .map_err(|e| CoreError::Internal(format!("모니터 목록 조회 실패: {e}")))?;

        let monitor = monitors
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| Monitor::all().ok()?.into_iter().next())
            .ok_or_else(|| CoreError::Internal("모니터를 찾을 수 없음".to_string()))?;

        let image = monitor
            .capture_image()
            .map_err(|e| CoreError::Internal(format!("스크린 캡처 실패: {e}")))?;

        debug!("스크린 캡처 완료: {}x{}", image.width(), image.height());

        Ok(DynamicImage::ImageRgba8(image))
    }

    /// 특정 모니터 캡처
    pub fn capture_monitor(&self, index: usize) -> Result<DynamicImage, CoreError> {
        let monitors = Monitor::all()
            .map_err(|e| CoreError::Internal(format!("모니터 목록 조회 실패: {e}")))?;

        let monitor = monitors
            .into_iter()
            .nth(index)
            .ok_or_else(|| CoreError::Internal(format!("모니터 인덱스 {index} 없음")))?;

        let image = monitor
            .capture_image()
            .map_err(|e| CoreError::Internal(format!("스크린 캡처 실패: {e}")))?;

        Ok(DynamicImage::ImageRgba8(image))
    }

    /// 사용 가능한 모니터 수
    pub fn monitor_count() -> Result<usize, CoreError> {
        Monitor::all()
            .map(|m| m.len())
            .map_err(|e| CoreError::Internal(format!("모니터 목록 조회 실패: {e}")))
    }
}

impl Default for ScreenCapture {
    fn default() -> Self {
        Self::new()
    }
}
