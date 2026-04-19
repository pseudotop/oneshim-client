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
            VisionError::PermissionDenied(msg) => CoreError::PermissionDeniedV2 {
                code: oneshim_core::error_codes::PermissionCode::PermissionDenied,
                message: msg,
            },
            VisionError::Ocr(msg) => CoreError::OcrErrorV2 {
                code: oneshim_core::error_codes::ProviderCode::OcrFailed,
                message: msg,
            },
            VisionError::ElementNotFound(msg) => CoreError::ElementNotFoundV2 {
                code: oneshim_core::error_codes::UiCode::ElementMissing,
                name: msg,
            },
            VisionError::Internal(msg) => CoreError::InternalV2 {
                code: oneshim_core::error_codes::InternalCode::Generic,
                message: msg,
            },
        }
    }
}
