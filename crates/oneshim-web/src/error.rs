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
        match err {
            oneshim_core::error::CoreError::Validation { field, message } => {
                ApiError::BadRequest(format!("{field}: {message}"))
            }
            oneshim_core::error::CoreError::Auth(message)
            | oneshim_core::error::CoreError::ConsentRequired(message)
            | oneshim_core::error::CoreError::OAuthError { message, .. }
            | oneshim_core::error::CoreError::OAuthRefreshError { message, .. } => {
                ApiError::Unauthorized(message)
            }
            oneshim_core::error::CoreError::NotFound { resource_type, id } => {
                ApiError::NotFound(format!("{resource_type}: {id}"))
            }
            oneshim_core::error::CoreError::ServiceUnavailable(message)
            | oneshim_core::error::CoreError::SandboxUnsupported(message) => {
                ApiError::ServiceUnavailable(message)
            }
            oneshim_core::error::CoreError::PolicyDenied(message)
            | oneshim_core::error::CoreError::PrivacyDenied(message)
            | oneshim_core::error::CoreError::ProcessNotAllowed(message) => {
                ApiError::Forbidden(message)
            }
            oneshim_core::error::CoreError::InvalidArguments(message)
            | oneshim_core::error::CoreError::Config(message)
            | oneshim_core::error::CoreError::Network(message)
            | oneshim_core::error::CoreError::OcrError(message)
            | oneshim_core::error::CoreError::SecretStoreError(message)
            | oneshim_core::error::CoreError::ElementNotFound(message)
            | oneshim_core::error::CoreError::SandboxInit(message)
            | oneshim_core::error::CoreError::SandboxExecution(message) => {
                ApiError::BadRequest(message)
            }
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
