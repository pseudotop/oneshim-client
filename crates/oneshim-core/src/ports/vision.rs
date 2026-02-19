//! 비전(이미지 처리) 포트.
//!
//! 구현: `oneshim-vision` crate (xcap, image, fast_image_resize, webp)

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::event::ContextEvent;
use crate::models::frame::ProcessedFrame;

/// 스크린 캡처 트리거 — 캡처 필요 여부 판단
pub trait CaptureTrigger: Send + Sync {
    /// 현재 컨텍스트 이벤트를 분석하여 캡처 필요 여부 + 중요도 반환.
    ///
    /// 반환값이 `Some`이면 캡처 실행, `None`이면 스킵 (쓰로틀 등).
    fn should_capture(&mut self, event: &ContextEvent) -> Option<CaptureRequest>;
}

/// 캡처 요청 (트리거가 승인한 경우)
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// 트리거 유형 (예: "WindowChange", "ErrorDetected")
    pub trigger_type: String,
    /// 중요도 점수 (0.0 ~ 1.0)
    pub importance: f32,
    /// 활성 앱 이름
    pub app_name: String,
    /// 창 제목
    pub window_title: String,
}

/// 프레임 처리기 — 스크린 캡처 → Edge 전처리 파이프라인
#[async_trait]
pub trait FrameProcessor: Send + Sync {
    /// 스크린 캡처 수행 후 Edge 전처리(델타/썸네일/OCR) 실행.
    ///
    /// `capture_request`의 중요도에 따라 처리 수준이 결정된다:
    /// - >= 0.8: 전체 프레임 + OCR
    /// - >= 0.5: 델타 인코딩
    /// - >= 0.3: 썸네일만
    /// - < 0.3: 메타데이터만
    async fn capture_and_process(
        &mut self,
        capture_request: &CaptureRequest,
    ) -> Result<ProcessedFrame, CoreError>;
}
