//! API 에러 처리.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

/// API 에러
#[derive(Debug, Error)]
pub enum ApiError {
    /// 내부 서버 오류
    #[error("내부 서버 오류: {0}")]
    Internal(String),

    /// 리소스를 찾을 수 없음
    #[error("리소스를 찾을 수 없음: {0}")]
    NotFound(String),

    /// 잘못된 요청
    #[error("잘못된 요청: {0}")]
    BadRequest(String),
}

/// 에러 응답 본문
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// 에러 메시지
    pub error: String,
    /// HTTP 상태 코드
    pub status: u16,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
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
        ApiError::Internal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = ApiError::NotFound("세션".to_string());
        assert!(err.to_string().contains("세션"));
    }
}
