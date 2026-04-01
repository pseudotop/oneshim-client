use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("request timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("OAuth error for {provider}: {message}")]
    OAuth { provider: String, message: String },

    #[error("OAuth refresh failed for {provider}: [{kind:?}] {message}")]
    OAuthRefresh {
        provider: String,
        kind: oneshim_core::ports::oauth::OAuthErrorKind,
        message: String,
    },

    #[error("{resource_type} not found: {id}")]
    NotFound { resource_type: String, id: String },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("validation error — {field}: {message}")]
    Validation { field: String, message: String },

    #[error("analysis API error: {0}")]
    Analysis(String),

    #[error("policy denied: {0}")]
    PolicyDenied(String),

    #[error("OCR error: {0}")]
    Ocr(String),

    #[error("secret store error: {0}")]
    SecretStore(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<NetworkError> for CoreError {
    fn from(err: NetworkError) -> Self {
        match err {
            NetworkError::Core(e) => e,
            NetworkError::Http(msg) => CoreError::Network(msg),
            NetworkError::Timeout { timeout_ms } => CoreError::RequestTimeout { timeout_ms },
            NetworkError::RateLimited { retry_after_secs } => {
                CoreError::RateLimit { retry_after_secs }
            }
            NetworkError::ServiceUnavailable(msg) => CoreError::ServiceUnavailable(msg),
            NetworkError::Auth(msg) => CoreError::Auth(msg),
            NetworkError::OAuth { provider, message } => {
                CoreError::OAuthError { provider, message }
            }
            NetworkError::OAuthRefresh {
                provider,
                kind,
                message,
            } => CoreError::OAuthRefreshError {
                provider,
                kind,
                message,
            },
            NetworkError::NotFound { resource_type, id } => {
                CoreError::NotFound { resource_type, id }
            }
            NetworkError::Serialization(msg) => {
                CoreError::Internal(format!("serialization: {msg}"))
            }
            NetworkError::Config(msg) => CoreError::Config(msg),
            NetworkError::Validation { field, message } => CoreError::Validation { field, message },
            NetworkError::Analysis(msg) => CoreError::Analysis(msg),
            NetworkError::PolicyDenied(msg) => CoreError::PolicyDenied(msg),
            NetworkError::Ocr(msg) => CoreError::OcrError(msg),
            NetworkError::SecretStore(msg) => CoreError::SecretStoreError(msg),
            NetworkError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
