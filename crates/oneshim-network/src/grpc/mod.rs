//! gRPC 클라이언트 모듈
//!
//! 이 모듈은 서버와의 gRPC 통신을 담당합니다.
//! `grpc` feature가 활성화되어 있어야 합니다.
//!
//! ## Feature Flag 기반 전환
//!
//! `UnifiedClient`를 사용하면 REST와 gRPC를 설정으로 전환할 수 있습니다.
//! - `use_grpc_auth: true` → 인증에 gRPC 사용
//! - `use_grpc_context: true` → 컨텍스트 전송에 gRPC 사용
//!
//! ## Health Check
//!
//! `GrpcHealthClient`를 사용하여 서버 상태를 확인할 수 있습니다.
//! ```rust,ignore
//! let mut health = GrpcHealthClient::connect(config).await?;
//! if health.is_healthy().await {
//!     println!("서버 정상");
//! }
//! ```

#[cfg(feature = "grpc")]
mod auth_client;
#[cfg(feature = "grpc")]
mod config;
#[cfg(feature = "grpc")]
mod context_client;
#[cfg(feature = "grpc")]
mod health_client;
#[cfg(feature = "grpc")]
mod session_client;
#[cfg(feature = "grpc")]
mod unified_client;

#[cfg(feature = "grpc")]
pub use auth_client::GrpcAuthClient;
#[cfg(feature = "grpc")]
pub use config::GrpcConfig;
#[cfg(feature = "grpc")]
pub use context_client::GrpcContextClient;
#[cfg(feature = "grpc")]
pub use health_client::{GrpcHealthClient, ServiceHealth, ServingStatus};
#[cfg(feature = "grpc")]
pub use session_client::GrpcSessionClient;
#[cfg(feature = "grpc")]
pub use unified_client::{
    AuthResponse, ContextBatchUploadRequest, ContextBatchUploadResponse, FeedbackType,
    ListSuggestionsResponse, SessionResponse, Streaming, Suggestion, SuggestionType, UnifiedClient,
};
