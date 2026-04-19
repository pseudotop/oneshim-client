use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use oneshim_api_contracts::error::ErrorResponse;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Unauthorized request: {0}")]
    Unauthorized(String),

    #[error("Forbidden request: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Unprocessable request: {0}")]
    Unprocessable(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            ApiError::Unprocessable(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            ApiError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
        };

        let body = ErrorResponse {
            error: message,
            status: status.as_u16(),
        };

        (status, Json(body)).into_response()
    }
}

impl From<oneshim_core::error::CoreError> for ApiError {
    fn from(err: oneshim_core::error::CoreError) -> Self {
        use oneshim_core::error::CoreError;
        #[allow(deprecated)]
        match err {
            // --- V2 struct variants (final shape post Phase 4) ---
            CoreError::Validation {
                code: oneshim_core::error_codes::ValidationCode::InvalidField,
                field,
                message,
                ..
            } => ApiError::BadRequest(format!("{field}: {message}")),
            CoreError::Auth { message, .. }
            | CoreError::ConsentRequired { message, .. }
            | CoreError::OAuthError {
                code: oneshim_core::error_codes::OAuthCode::Failed,
                message,
                ..
            }
            | CoreError::OAuthRefreshError {
                code: oneshim_core::error_codes::OAuthCode::RefreshFailed,
                message,
                ..
            } => ApiError::Unauthorized(message),
            CoreError::NotFound {
                code: oneshim_core::error_codes::NotFoundCode::ResourceMissing,
                resource_type,
                id,
                ..
            } => ApiError::NotFound(format!("{resource_type}: {id}")),
            CoreError::ServiceUnavailable { message, .. }
            | CoreError::SandboxUnsupported { message, .. } => {
                ApiError::ServiceUnavailable(message)
            }
            CoreError::PolicyDenied { message, .. }
            | CoreError::PrivacyDenied { message, .. }
            | CoreError::ProcessNotAllowed { message, .. } => ApiError::Forbidden(message),
            CoreError::InvalidArguments { message, .. }
            | CoreError::Config { message, .. }
            | CoreError::Network { message, .. }
            | CoreError::OcrError { message, .. }
            | CoreError::SecretStoreError { message, .. }
            | CoreError::SandboxInit { message, .. }
            | CoreError::SandboxExecution { message, .. } => ApiError::BadRequest(message),
            CoreError::ElementNotFound { name, .. } => ApiError::BadRequest(name),

            other => ApiError::Internal(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = ApiError::NotFound("session".to_string());
        assert!(err.to_string().contains("session"));
    }
}
