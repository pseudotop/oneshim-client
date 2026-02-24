//!

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("직렬화 error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("설정 error: {0}")]
    Config(String),

    #[error("유효성 validation failure — {field}: {message}")]
    Validation {
        field: String,
        message: String,
    },

    #[error("인증 error: {0}")]
    Auth(String),

    #[error("{resource_type} 미발견: {id}")]
    NotFound {
        resource_type: String,
        id: String,
    },

    #[error("within부 error: {0}")]
    Internal(String),

    #[error("네트워크 error: {0}")]
    Network(String),

    #[error("request 한도 초과, {retry_after_secs}초 후 재attempt")]
    RateLimit {
        retry_after_secs: u64,
    },

    #[error("서비스 일시 not-available: {0}")]
    ServiceUnavailable(String),

    #[error("policy 거부: {0}")]
    PolicyDenied(String),

    #[error("허가되지 않은 프로세스: {0}")]
    ProcessNotAllowed(String),

    #[error("잘못된 인자: {0}")]
    InvalidArguments(String),

    #[error("바이너리 해시 불일치: expected={expected}, actual={actual}")]
    BinaryHashMismatch {
        expected: String,
        actual: String,
    },

    #[error("동의 필요: {0}")]
    ConsentRequired(String),

    #[error("동의 만료 — 재동의 필요")]
    ConsentExpired,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("샌드박스 initialize failure: {0}")]
    SandboxInit(String),

    #[error("샌드박스 execution failure: {0}")]
    SandboxExecution(String),

    #[error("샌드박스 미지원 플랫폼: {0}")]
    SandboxUnsupported(String),

    #[error("execution timeout: {timeout_ms}ms 초과")]
    ExecutionTimeout {
        timeout_ms: u64,
    },

    #[error("UI 요소 미발견: {0}")]
    ElementNotFound(String),

    #[error("프라이버시 거부: {0}")]
    PrivacyDenied(String),

    #[error("OCR error: {0}")]
    OcrError(String),
}
