use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Validation failed - {field}: {message}")]
    Validation { field: String, message: String },

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("{resource_type} not found: {id}")]
    NotFound { resource_type: String, id: String },

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Request timed out after {timeout_ms}ms")]
    RequestTimeout { timeout_ms: u64 },

    #[error("Request rate limit exceeded, retry after {retry_after_secs}s")]
    RateLimit { retry_after_secs: u64 },

    #[error("Service temporarily unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Policy denied: {0}")]
    PolicyDenied(String),

    #[error("Process is not allowed: {0}")]
    ProcessNotAllowed(String),

    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Binary hash mismatch: expected={expected}, actual={actual}")]
    BinaryHashMismatch { expected: String, actual: String },

    #[error("Consent required: {0}")]
    ConsentRequired(String),

    #[error("Consent expired - re-consent required")]
    ConsentExpired,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Sandbox initialization failed: {0}")]
    SandboxInit(String),

    #[error("Sandbox execution failed: {0}")]
    SandboxExecution(String),

    #[error("Sandbox unsupported on platform: {0}")]
    SandboxUnsupported(String),

    #[error("Execution timeout exceeded: {timeout_ms}ms")]
    ExecutionTimeout { timeout_ms: u64 },

    #[error("UI element not found: {0}")]
    ElementNotFound(String),

    #[error("Privacy denied: {0}")]
    PrivacyDenied(String),

    #[error("OCR error: {0}")]
    OcrError(String),
}

/// GUI 상호작용 전용 에러 (AutomationPort GUI 메서드에서 사용)
///
/// 이전에는 `oneshim-automation::gui_interaction::GuiInteractionError`에 정의되었으나,
/// AutomationPort 추상화를 위해 oneshim-core로 이동 (ADR-001 §7)
#[derive(Debug, Error)]
pub enum GuiInteractionError {
    #[error("GUI session token is invalid")]
    Unauthorized,

    #[error("GUI session '{0}' not found")]
    NotFound(String),

    #[error("Invalid GUI request: {0}")]
    BadRequest(String),

    #[error("GUI request forbidden: {0}")]
    Forbidden(String),

    #[error("GUI focus drift detected: {0}")]
    FocusDrift(String),

    #[error("GUI ticket is no longer valid: {0}")]
    TicketInvalid(String),

    #[error("GUI runtime unavailable: {0}")]
    Unavailable(String),

    #[error("GUI runtime failed: {0}")]
    Internal(String),
}
