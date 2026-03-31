#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

//! Local embedding provider — fastembed-rs (ONNX Runtime) wrapper.
//!
//! Wraps the synchronous fastembed `TextEmbedding` API behind the async
//! `EmbeddingProvider` port defined in `oneshim-core`.  All blocking calls
//! are dispatched via `tokio::task::spawn_blocking` to avoid starving the
//! async runtime.
//!
//! When the `fastembed-local` feature is disabled (or if the ONNX runtime
//! cannot be loaded on the host platform) a compile-time stub is provided
//! that returns `CoreError::Internal` for every operation so that dependent
//! crates can still compile.

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::ports::embedding_provider::EmbeddingProvider;

// ── fastembed-backed implementation ────────────────────────────────────────

#[cfg(feature = "fastembed-local")]
mod fastembed_impl {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Local embedding provider backed by fastembed-rs (ONNX Runtime).
    ///
    /// Thread-safe (`Send + Sync`) — the inner `TextEmbedding` is wrapped
    /// in `Arc<Mutex>` and accessed only through `spawn_blocking`.
    pub struct LocalEmbeddingProvider {
        model: Arc<Mutex<fastembed::TextEmbedding>>,
        model_id: String,
        dimensions: usize,
    }

    impl LocalEmbeddingProvider {
        /// Create a new provider with the given fastembed model.
        ///
        /// `model_name` is an `EmbeddingModel` variant name such as
        /// `"AllMiniLML6V2"`. If omitted or unrecognised the default model
        /// (`AllMiniLML6V2`, 384-dim) is used.
        pub fn new(model_name: Option<&str>) -> Result<Self, CoreError> {
            let (model_enum, id, dims) = resolve_model(model_name);

            let options = fastembed::InitOptions::new(model_enum).with_show_download_progress(true);

            let model = fastembed::TextEmbedding::try_new(options)
                .map_err(|e| CoreError::Internal(format!("fastembed init failed: {e}")))?;

            Ok(Self {
                model: Arc::new(Mutex::new(model)),
                model_id: id,
                dimensions: dims,
            })
        }
    }

    #[async_trait]
    impl EmbeddingProvider for LocalEmbeddingProvider {
        async fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
            let model = Arc::clone(&self.model);
            let text = text.to_owned();

            tokio::task::spawn_blocking(move || {
                let mut guard = model
                    .lock()
                    .map_err(|e| CoreError::Internal(format!("fastembed lock poisoned: {e}")))?;
                let results = guard
                    .embed(vec![text], None)
                    .map_err(|e| CoreError::Internal(format!("fastembed embed failed: {e}")))?;
                results
                    .into_iter()
                    .next()
                    .ok_or_else(|| CoreError::Internal("fastembed returned empty result".into()))
            })
            .await
            .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
            let model = Arc::clone(&self.model);
            let texts = texts.to_vec();

            tokio::task::spawn_blocking(move || {
                let mut guard = model
                    .lock()
                    .map_err(|e| CoreError::Internal(format!("fastembed lock poisoned: {e}")))?;
                guard
                    .embed(texts, None)
                    .map_err(|e| CoreError::Internal(format!("fastembed batch embed failed: {e}")))
            })
            .await
            .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }

        fn model_id(&self) -> &str {
            &self.model_id
        }
    }

    /// Resolve a human-readable model name to fastembed enum + metadata.
    ///
    /// The default model is `AllMiniLML6V2Q` — the quantized (INT8) variant of
    /// all-MiniLM-L6-v2.  It provides ~3x faster CPU inference with less than
    /// 1% accuracy degradation compared to the full-precision (FP32) version.
    /// Users who need maximum accuracy can override `local_model` in config to
    /// `"AllMiniLML6V2"` (or any other supported variant) to switch back.
    pub(super) fn resolve_model(name: Option<&str>) -> (fastembed::EmbeddingModel, String, usize) {
        match name {
            // Quantized variants (INT8 ONNX — ~3x faster, ~1% accuracy loss)
            Some("AllMiniLML6V2Q") | Some("all-MiniLM-L6-v2-Q") | None => (
                fastembed::EmbeddingModel::AllMiniLML6V2Q,
                "all-MiniLM-L6-v2-Q".to_owned(),
                384,
            ),
            Some("AllMiniLML12V2Q") | Some("all-MiniLM-L12-v2-Q") => (
                fastembed::EmbeddingModel::AllMiniLML12V2Q,
                "all-MiniLM-L12-v2-Q".to_owned(),
                384,
            ),
            Some("BGESmallENV15Q") | Some("bge-small-en-v1.5-Q") => (
                fastembed::EmbeddingModel::BGESmallENV15Q,
                "bge-small-en-v1.5-Q".to_owned(),
                384,
            ),
            Some("BGEBaseENV15Q") | Some("bge-base-en-v1.5-Q") => (
                fastembed::EmbeddingModel::BGEBaseENV15Q,
                "bge-base-en-v1.5-Q".to_owned(),
                768,
            ),
            // Full-precision variants (FP32 ONNX — higher accuracy, slower)
            Some("AllMiniLML6V2") | Some("all-MiniLM-L6-v2") => (
                fastembed::EmbeddingModel::AllMiniLML6V2,
                "all-MiniLM-L6-v2".to_owned(),
                384,
            ),
            Some("AllMiniLML12V2") | Some("all-MiniLM-L12-v2") => (
                fastembed::EmbeddingModel::AllMiniLML12V2,
                "all-MiniLM-L12-v2".to_owned(),
                384,
            ),
            Some("BGESmallENV15") | Some("bge-small-en-v1.5") => (
                fastembed::EmbeddingModel::BGESmallENV15,
                "bge-small-en-v1.5".to_owned(),
                384,
            ),
            Some("BGEBaseENV15") | Some("bge-base-en-v1.5") => (
                fastembed::EmbeddingModel::BGEBaseENV15,
                "bge-base-en-v1.5".to_owned(),
                768,
            ),
            Some(other) => {
                tracing::warn!(
                    model = other,
                    "Unknown embedding model, falling back to AllMiniLML6V2Q"
                );
                (
                    fastembed::EmbeddingModel::AllMiniLML6V2Q,
                    "all-MiniLM-L6-v2-Q".to_owned(),
                    384,
                )
            }
        }
    }
}

#[cfg(feature = "fastembed-local")]
pub use fastembed_impl::LocalEmbeddingProvider;

// ── Stub implementation (no fastembed feature) ─────────────────────────────

#[cfg(not(feature = "fastembed-local"))]
mod stub_impl {
    use super::*;

    /// Stub provider used when the `fastembed-local` feature is disabled.
    ///
    /// Every method returns `CoreError::Internal` with a descriptive message.
    pub struct LocalEmbeddingProvider {
        model_id: String,
        dimensions: usize,
    }

    impl LocalEmbeddingProvider {
        pub fn new(_model_name: Option<&str>) -> Result<Self, CoreError> {
            Ok(Self {
                model_id: "stub-no-fastembed".to_owned(),
                dimensions: 384,
            })
        }
    }

    #[async_trait]
    impl EmbeddingProvider for LocalEmbeddingProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, CoreError> {
            Err(CoreError::Internal(
                "fastembed-local feature is not enabled — cannot embed locally".into(),
            ))
        }

        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
            Err(CoreError::Internal(
                "fastembed-local feature is not enabled — cannot embed locally".into(),
            ))
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }

        fn model_id(&self) -> &str {
            &self.model_id
        }
    }
}

#[cfg(not(feature = "fastembed-local"))]
pub use stub_impl::LocalEmbeddingProvider;

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "fastembed-local")]
    #[test]
    fn resolve_model_defaults_to_quantized_minilm() {
        let (_model, model_id, dimensions) = fastembed_impl::resolve_model(None);
        assert_eq!(model_id, "all-MiniLM-L6-v2-Q");
        assert_eq!(dimensions, 384);
    }

    #[cfg(feature = "fastembed-local")]
    #[test]
    fn unknown_model_falls_back_to_quantized_minilm() {
        let (_model, model_id, dimensions) = fastembed_impl::resolve_model(Some("bogus-model"));
        assert_eq!(model_id, "all-MiniLM-L6-v2-Q");
        assert_eq!(dimensions, 384);
    }

    #[cfg(feature = "fastembed-local")]
    mod fastembed_tests {
        use super::*;

        #[test]
        #[ignore = "requires downloading the fastembed model"]
        fn provider_creates_successfully() {
            // Constructor coverage is kept as an ignored network test because
            // fastembed downloads model assets on first initialization.
            let provider = LocalEmbeddingProvider::new(None).expect("should create provider");
            assert_eq!(provider.dimensions(), 384);
            assert!(!provider.model_id().is_empty());
        }

        /// NOTE: This test downloads the model on first run (~25 MB).
        /// It is marked `#[ignore]` for CI — run with `cargo test -- --ignored`.
        #[tokio::test]
        #[ignore]
        async fn embed_returns_correct_dimensions() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            let vec = provider.embed("hello world").await.unwrap();
            assert_eq!(vec.len(), provider.dimensions());
        }

        /// Batch embedding test (also requires model download).
        #[tokio::test]
        #[ignore]
        async fn embed_batch_returns_correct_count() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            let texts = vec!["hello".to_owned(), "world".to_owned()];
            let vecs = provider.embed_batch(&texts).await.unwrap();
            assert_eq!(vecs.len(), 2);
            for v in &vecs {
                assert_eq!(v.len(), provider.dimensions());
            }
        }
    }

    #[cfg(not(feature = "fastembed-local"))]
    mod stub_tests {
        use super::*;

        #[tokio::test]
        async fn stub_embed_returns_error() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            let result = provider.embed("hello").await;
            assert!(result.is_err());
        }
    }
}
