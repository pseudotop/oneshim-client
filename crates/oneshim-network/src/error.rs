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

    #[error("{resource_type} not found: {id}")]
    NotFound { resource_type: String, id: String },

    #[error("configuration error: {0}")]
    Config(String),

    #[error("validation error — {field}: {message}")]
    Validation { field: String, message: String },

    #[error("analysis API error: {0}")]
    Analysis(String),

    #[error("policy denied: {0}")]
    PolicyDenied(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("circuit breaker open — requests are being fast-failed")]
    CircuitOpen,
}

impl From<NetworkError> for CoreError {
    fn from(err: NetworkError) -> Self {
        match err {
            NetworkError::Core(e) => e,
            NetworkError::Http(msg) => CoreError::Network {
                code: oneshim_core::error_codes::NetworkCode::Generic,
                message: msg,
            },
            NetworkError::Timeout { timeout_ms } => CoreError::RequestTimeout {
                code: oneshim_core::error_codes::NetworkCode::Timeout,
                timeout_ms,
            },
            NetworkError::RateLimited { retry_after_secs } => CoreError::RateLimit {
                code: oneshim_core::error_codes::NetworkCode::RateLimit,
                retry_after_secs,
            },
            NetworkError::ServiceUnavailable(msg) => CoreError::ServiceUnavailable {
                code: oneshim_core::error_codes::ServiceCode::Unavailable,
                message: msg,
            },
            NetworkError::Auth(msg) => CoreError::Auth {
                code: oneshim_core::error_codes::AuthCode::Failed,
                message: msg,
            },
            NetworkError::NotFound { resource_type, id } => CoreError::NotFound {
                code: oneshim_core::error_codes::NotFoundCode::ResourceMissing,
                resource_type,
                id,
            },
            NetworkError::Config(msg) => CoreError::Config {
                code: oneshim_core::error_codes::ConfigCode::Invalid,
                message: msg,
            },
            NetworkError::Validation { field, message } => CoreError::Validation {
                code: oneshim_core::error_codes::ValidationCode::InvalidField,
                field,
                message,
            },
            NetworkError::Analysis(msg) => CoreError::Analysis {
                code: oneshim_core::error_codes::ProviderCode::AnalysisFailed,
                message: msg,
            },
            NetworkError::PolicyDenied(msg) => CoreError::PolicyDenied {
                code: oneshim_core::error_codes::PolicyCode::Denied,
                message: msg,
            },
            NetworkError::Internal(msg) => CoreError::Internal {
                code: oneshim_core::error_codes::InternalCode::Generic,
                message: msg,
            },
            NetworkError::CircuitOpen => CoreError::ServiceUnavailable {
                code: oneshim_core::error_codes::ServiceCode::Unavailable,
                message: "circuit breaker open".into(),
            },
        }
    }
}
