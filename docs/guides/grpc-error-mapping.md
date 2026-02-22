[English](./grpc-error-mapping.md) | [한국어](./grpc-error-mapping.ko.md)

# gRPC Error Mapping Guide

This guide defines how ONESHIM client maps gRPC status errors into `CoreError`.

## Source

- Implementation: `crates/oneshim-network/src/grpc/error_mapping.rs`
- Consumers:
  - `crates/oneshim-network/src/grpc/auth_client.rs`
  - `crates/oneshim-network/src/grpc/session_client.rs`
  - `crates/oneshim-network/src/grpc/context_client.rs`
  - `crates/oneshim-network/src/grpc/health_client.rs`

## Mapping Policy

| gRPC `Code` | Mapped `CoreError` | Notes |
|---|---|---|
| `Unauthenticated`, `PermissionDenied` | `CoreError::Auth` | Authentication/authorization failure |
| `NotFound` | `CoreError::NotFound` | Operation name and status message are propagated |
| `InvalidArgument`, `FailedPrecondition`, `OutOfRange` | `CoreError::Validation` | Reported as request validation failure |
| `ResourceExhausted` | `CoreError::RateLimit` | Uses `retry-after` or `x-retry-after-seconds`, default `60` |
| `Unavailable` | `CoreError::ServiceUnavailable` | Service availability outage |
| other codes | `CoreError::Network` | Generic network/transport domain fallback |

## Verification

```bash
cargo test -p oneshim-network --features grpc error_mapping
./scripts/verify-grpc-readiness.sh
```
