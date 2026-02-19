//! # oneshim-network
//!
//! HTTP/SSE/WebSocket/gRPC 네트워크 어댑터.
//! 서버와의 REST API, SSE 스트림, WebSocket, gRPC 통신을 담당하며
//! JWT 인증, 배치 업로드, 압축(gzip/zstd/lz4)을 지원한다.
//!
//! ## Feature Flags
//!
//! - `grpc`: gRPC 클라이언트 활성화 (tonic + prost)
//!
//! ## 사용 예시
//!
//! ```rust,ignore
//! // REST/SSE 클라이언트 (기본)
//! use oneshim_network::http_client::HttpApiClient;
//! use oneshim_network::sse_client::SseStreamClient;
//!
//! // gRPC 클라이언트 (grpc feature 필요)
//! #[cfg(feature = "grpc")]
//! use oneshim_network::grpc::{GrpcAuthClient, GrpcConfig};
//! ```

pub mod ai_llm_client;
pub mod ai_ocr_client;
pub mod auth;
pub mod batch_uploader;
pub mod compression;
pub mod connectivity;
pub mod http_client;
pub mod sse_client;
pub mod ws_client;

// gRPC 모듈 (Phase 34 - grpc feature 활성화 시)
#[cfg(feature = "grpc")]
pub mod grpc;
#[cfg(feature = "grpc")]
pub mod proto;
