// Cast safety: network metrics and buffer sizes — precision loss acceptable.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

//! # oneshim-network
//! ## Feature Flags
//! ```rust,ignore
//! use oneshim_network::http_client::HttpApiClient;
//! use oneshim_network::sse_client::SseStreamClient;
//! #[cfg(feature = "grpc")]
//! use oneshim_network::grpc::{GrpcAuthClient, GrpcConfig};
//! ```

/// Anthropic API version header value — shared across analysis_client,
/// ai_llm_client, and ai_ocr_client.
pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Default model name per AI provider type.
///
/// Used as a last-resort fallback when neither the user config nor the
/// provider-spec registry supply a model name.
pub fn default_model_for_provider(provider: &oneshim_core::config::AiProviderType) -> &'static str {
    use oneshim_core::config::AiProviderType;
    match provider {
        AiProviderType::Anthropic => "claude-sonnet-4-20250514",
        AiProviderType::OpenAi => "gpt-5.4",
        AiProviderType::Google => "gemini-2.5-flash",
        AiProviderType::Ollama => "qwen3:8b",
        AiProviderType::Bedrock => "anthropic.claude-3-5-sonnet-20241022-v2:0",
        AiProviderType::Copilot => "gpt-5.4",
        AiProviderType::Generic => "gpt-5-mini",
    }
}

pub mod error;
pub use error::NetworkError;

pub mod ai_llm_client;
pub mod ai_ocr_client;
pub mod analysis_client;
pub mod auth;
pub mod batch_uploader;
pub mod circuit_breaker;
pub mod compression;
pub mod connectivity;
pub mod http_api_session;
pub mod http_client;
pub mod integration;
pub mod local_llm_session;
pub mod oauth;
pub mod remote_embedding_client;
pub mod resilience;
pub mod sse_client;

pub mod sync;

#[cfg(feature = "grpc")]
pub mod grpc;
#[cfg(feature = "grpc")]
pub mod proto;
