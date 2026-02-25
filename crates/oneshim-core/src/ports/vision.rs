//!

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::event::ContextEvent;
use crate::models::frame::ProcessedFrame;

pub trait CaptureTrigger: Send + Sync {
    ///
    fn should_capture(&mut self, event: &ContextEvent) -> Option<CaptureRequest>;
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
    ///
    async fn capture_and_process(
        &mut self,
        capture_request: &CaptureRequest,
    ) -> Result<ProcessedFrame, CoreError>;
}
