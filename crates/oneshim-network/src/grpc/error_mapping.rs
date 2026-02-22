use oneshim_core::error::CoreError;
use tonic::{Code, Status};

const DEFAULT_RETRY_AFTER_SECS: u64 = 60;

pub fn map_grpc_status_error(operation: &str, status: Status) -> CoreError {
    let code = status.code();
    let message = status.message().to_string();

    match code {
        Code::Unauthenticated | Code::PermissionDenied => {
            CoreError::Auth(format!("{operation}: {message}"))
        }
        Code::NotFound => CoreError::NotFound {
            resource_type: operation.to_string(),
            id: message,
        },
        Code::InvalidArgument | Code::FailedPrecondition | Code::OutOfRange => {
            CoreError::Validation {
                field: "grpc_request".to_string(),
                message: format!("{operation}: {message} ({code})"),
            }
        }
        Code::ResourceExhausted => CoreError::RateLimit {
            retry_after_secs: extract_retry_after_secs(&status),
        },
        Code::Unavailable => CoreError::ServiceUnavailable(format!("{operation}: {message}")),
        _ => CoreError::Network(format!("{operation}: {message} ({code})")),
    }
}

fn extract_retry_after_secs(status: &Status) -> u64 {
    status
        .metadata()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            status
                .metadata()
                .get("x-retry-after-seconds")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap_or(DEFAULT_RETRY_AFTER_SECS)
}

#[cfg(test)]
mod tests {
    use tonic::Status;

    use super::*;

    #[test]
    fn maps_unauthenticated_to_auth_error() {
        let err = map_grpc_status_error("grpc login", Status::unauthenticated("bad token"));
        assert!(matches!(err, CoreError::Auth(_)));
    }

    #[test]
    fn maps_invalid_argument_to_validation_error() {
        let err = map_grpc_status_error("grpc upload", Status::invalid_argument("missing id"));

        match err {
            CoreError::Validation { field, message } => {
                assert_eq!(field, "grpc_request");
                assert!(message.contains("missing id"));
            }
            _ => panic!("expected validation error"),
        }
    }

    #[test]
    fn maps_resource_exhausted_to_rate_limit_with_default_retry() {
        let err = map_grpc_status_error("grpc list", Status::resource_exhausted("busy"));
        assert!(matches!(
            err,
            CoreError::RateLimit {
                retry_after_secs: DEFAULT_RETRY_AFTER_SECS
            }
        ));
    }

    #[test]
    fn maps_unavailable_to_service_unavailable() {
        let err = map_grpc_status_error("grpc heartbeat", Status::unavailable("down"));
        assert!(matches!(err, CoreError::ServiceUnavailable(_)));
    }
}
