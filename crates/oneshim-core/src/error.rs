//!

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
