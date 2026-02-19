//! Proto 메시지 및 gRPC 클라이언트 정의
//!
//! tonic-build에서 생성된 Protobuf 메시지와 gRPC 클라이언트를 포함합니다.
//! `grpc` feature가 활성화되어 있어야 합니다.

/// 공통 타입 (UUID, Pagination, Enums, Errors)
#[cfg(feature = "grpc")]
pub mod common {
    #![allow(clippy::all)]
    #![allow(warnings)]
    include!("generated/oneshim.v1.common.rs");
}

/// 인증 도메인 (Login, Logout, Token, Session, Device)
#[cfg(feature = "grpc")]
pub mod auth {
    #![allow(clippy::all)]
    #![allow(warnings)]
    include!("generated/oneshim.v1.auth.rs");
}

/// 사용자 컨텍스트 도메인 (Events, Frames, Suggestions, Batch)
#[cfg(feature = "grpc")]
pub mod user_context {
    #![allow(clippy::all)]
    #![allow(warnings)]
    include!("generated/oneshim.v1.user_context.rs");
}

/// 모니터링 도메인 (Metrics, Alerts)
#[cfg(feature = "grpc")]
pub mod monitoring {
    #![allow(clippy::all)]
    #![allow(warnings)]
    include!("generated/oneshim.v1.monitoring.rs");
}
