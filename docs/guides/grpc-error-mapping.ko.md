[English](./grpc-error-mapping.md) | [한국어](./grpc-error-mapping.ko.md)

# gRPC 에러 매핑 가이드

이 문서는 ONESHIM 클라이언트가 gRPC status 에러를 `CoreError`로 변환하는 기준을 정의합니다.

## 소스

- 구현: `crates/oneshim-network/src/grpc/error_mapping.rs`
- 사용처:
  - `crates/oneshim-network/src/grpc/auth_client.rs`
  - `crates/oneshim-network/src/grpc/session_client.rs`
  - `crates/oneshim-network/src/grpc/context_client.rs`
  - `crates/oneshim-network/src/grpc/health_client.rs`

## 매핑 정책

| gRPC `Code` | 매핑 `CoreError` | 설명 |
|---|---|---|
| `Unauthenticated`, `PermissionDenied` | `CoreError::Auth` | 인증/인가 실패 |
| `NotFound` | `CoreError::NotFound` | operation 이름과 status message를 전달 |
| `InvalidArgument`, `FailedPrecondition`, `OutOfRange` | `CoreError::Validation` | 요청 유효성 실패로 처리 |
| `ResourceExhausted` | `CoreError::RateLimit` | `retry-after` 또는 `x-retry-after-seconds` 사용, 기본값 `60` |
| `Unavailable` | `CoreError::ServiceUnavailable` | 서비스 가용성 장애 |
| 기타 코드 | `CoreError::Network` | 일반 네트워크/전송 도메인 fallback |

## 검증

```bash
cargo test -p oneshim-network --features grpc error_mapping
./scripts/verify-grpc-readiness.sh
```
