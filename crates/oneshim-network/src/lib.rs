//! # oneshim-network
//!
//!
//! ## Feature Flags
//!
//!
//!
//! ```rust,ignore
//! use oneshim_network::http_client::HttpApiClient;
//! use oneshim_network::sse_client::SseStreamClient;
//!
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

#[cfg(feature = "grpc")]
pub mod grpc;
#[cfg(feature = "grpc")]
pub mod proto;
