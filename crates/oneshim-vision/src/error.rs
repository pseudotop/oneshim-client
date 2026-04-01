use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VisionError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("OCR error: {0}")]
    Ocr(String),
    #[error("element not found: {0}")]
    ElementNotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<VisionError> for CoreError {
    fn from(err: VisionError) -> Self {
        match err {
            VisionError::Core(e) => e,
            VisionError::PermissionDenied(msg) => CoreError::PermissionDenied(msg),
            VisionError::Ocr(msg) => CoreError::OcrError(msg),
            VisionError::ElementNotFound(msg) => CoreError::ElementNotFound(msg),
            VisionError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
