use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::event::ContextEvent;
use crate::models::frame::ProcessedFrame;

pub trait CaptureTrigger: Send + Sync {
    fn should_capture(&self, event: &ContextEvent) -> Option<CaptureRequest>;
}

#[derive(Debug, Clone)]
pub struct CaptureRequest {
    pub trigger_type: String,
    pub importance: f32,
    pub app_name: String,
    pub window_title: String,
}

#[async_trait]
pub trait FrameProcessor: Send + Sync {
    async fn capture_and_process(
        &self,
        capture_request: &CaptureRequest,
    ) -> Result<ProcessedFrame, CoreError>;

    /// Capture a lightweight thumbnail for ring buffer use.
    /// Returns raw WebP bytes at low quality. Default returns unsupported error.
    async fn capture_thumbnail(&self) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::Internal(
            "thumbnail capture not supported".to_string(),
        ))
    }
}
