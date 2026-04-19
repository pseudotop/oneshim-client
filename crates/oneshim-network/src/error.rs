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

    #[error("circuit breaker open — requests are being fast-failed")]
    CircuitOpen,
}

impl From<NetworkError> for CoreError {
    fn from(err: NetworkError) -> Self {
        match err {
            NetworkError::Core(e) => e,
            NetworkError::Http(msg) => CoreError::NetworkV2 {
                code: oneshim_core::error_codes::NetworkCode::Generic,
                message: msg,
            },
            NetworkError::Timeout { timeout_ms } => CoreError::RequestTimeoutV2 {
                code: oneshim_core::error_codes::NetworkCode::Timeout,
                timeout_ms,
            },
            NetworkError::RateLimited { retry_after_secs } => CoreError::RateLimitV2 {
                code: oneshim_core::error_codes::NetworkCode::RateLimit,
                retry_after_secs,
            },
            NetworkError::ServiceUnavailable(msg) => CoreError::ServiceUnavailableV2 {
                code: oneshim_core::error_codes::ServiceCode::Unavailable,
                message: msg,
            },
            NetworkError::Auth(msg) => CoreError::AuthV2 {
                code: oneshim_core::error_codes::AuthCode::Failed,
                message: msg,
            },
            NetworkError::OAuth { provider, message } => CoreError::OAuthErrorV2 {
                code: oneshim_core::error_codes::OAuthCode::Failed,
                provider,
                message,
            },
            NetworkError::OAuthRefresh {
                provider,
                kind,
                message,
            } => CoreError::OAuthRefreshErrorV2 {
                code: oneshim_core::error_codes::OAuthCode::RefreshFailed,
                provider,
                kind,
                message,
            },
            NetworkError::NotFound { resource_type, id } => CoreError::NotFoundV2 {
                code: oneshim_core::error_codes::NotFoundCode::ResourceMissing,
                resource_type,
                id,
            },
            NetworkError::Serialization(msg) => CoreError::InternalV2 {
                code: oneshim_core::error_codes::InternalCode::Generic,
                message: format!("serialization: {msg}"),
            },
            NetworkError::Config(msg) => CoreError::ConfigV2 {
                code: oneshim_core::error_codes::ConfigCode::Invalid,
                message: msg,
            },
            NetworkError::Validation { field, message } => CoreError::ValidationV2 {
                code: oneshim_core::error_codes::ValidationCode::InvalidField,
                field,
                message,
            },
            NetworkError::Analysis(msg) => CoreError::AnalysisV2 {
                code: oneshim_core::error_codes::ProviderCode::AnalysisFailed,
                message: msg,
            },
            NetworkError::PolicyDenied(msg) => CoreError::PolicyDeniedV2 {
                code: oneshim_core::error_codes::PolicyCode::Denied,
                message: msg,
            },
            NetworkError::Ocr(msg) => CoreError::OcrErrorV2 {
                code: oneshim_core::error_codes::ProviderCode::OcrFailed,
                message: msg,
            },
            NetworkError::SecretStore(msg) => CoreError::SecretStoreErrorV2 {
                code: oneshim_core::error_codes::SecretCode::Failed,
                message: msg,
            },
            NetworkError::Internal(msg) => CoreError::InternalV2 {
                code: oneshim_core::error_codes::InternalCode::Generic,
                message: msg,
            },
            NetworkError::CircuitOpen => CoreError::ServiceUnavailableV2 {
                code: oneshim_core::error_codes::ServiceCode::Unavailable,
                message: "circuit breaker open".into(),
            },
        }
    }
}
