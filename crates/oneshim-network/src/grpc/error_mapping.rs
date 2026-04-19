use crate::error::NetworkError;
use tonic::{Code, Status};

const DEFAULT_RETRY_AFTER_SECS: u64 = 60;

pub fn map_grpc_status_error(operation: &str, status: Status) -> NetworkError {
    let code = status.code();
    let message = status.message().to_string();

    match code {
        Code::Unauthenticated | Code::PermissionDenied => {
            NetworkError::Auth(format!("{operation}: {message}"))
        }
        Code::NotFound => NetworkError::NotFound {
            resource_type: operation.to_string(),
            id: message,
        },
        Code::InvalidArgument | Code::FailedPrecondition | Code::OutOfRange => {
            NetworkError::Validation {
                field: "grpc_request".to_string(),
                message: format!("{operation}: {message} ({code})"),
            }
        }
        Code::ResourceExhausted => NetworkError::RateLimited {
            retry_after_secs: extract_retry_after_secs(&status),
        },
        Code::Unavailable => NetworkError::ServiceUnavailable(format!("{operation}: {message}")),
        Code::DeadlineExceeded => NetworkError::Timeout {
            // gRPC client-side deadline elapsed. We don't know the exact timeout
            // value from Status alone, so use 0 as a sentinel; actual request
            // timeout is already logged at request-site.
            timeout_ms: 0,
        },
        _ => NetworkError::Http(format!("{operation}: {message} ({code})")),
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
        assert!(matches!(err, NetworkError::Auth(_)));
    }

    #[test]
    fn maps_invalid_argument_to_validation_error() {
        let err = map_grpc_status_error("grpc upload", Status::invalid_argument("missing id"));

        match err {
            NetworkError::Validation { field, message } => {
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
            NetworkError::RateLimited {
                retry_after_secs: DEFAULT_RETRY_AFTER_SECS
            }
        ));
    }

    #[test]
    fn maps_unavailable_to_service_unavailable() {
        let err = map_grpc_status_error("grpc heartbeat", Status::unavailable("down"));
        assert!(matches!(err, NetworkError::ServiceUnavailable(_)));
    }

    /// iter-52 regression guard: gRPC Code::DeadlineExceeded is a client-
    /// observed timeout and must map to NetworkError::Timeout (not fall into
    /// the Http wildcard). This preserves retry semantics — Timeout is
    /// explicitly retryable via is_retryable, Http is retryable-ambiguous,
    /// and downstream CoreError wire code `network.timeout` vs `network.generic`
    /// lets telemetry isolate timeout issues from generic HTTP failures.
    #[test]
    fn maps_deadline_exceeded_to_timeout() {
        let err =
            map_grpc_status_error("grpc streaming", Status::deadline_exceeded("took too long"));
        assert!(
            matches!(err, NetworkError::Timeout { .. }),
            "DeadlineExceeded must map to NetworkError::Timeout, got: {err:?}"
        );
    }
}
