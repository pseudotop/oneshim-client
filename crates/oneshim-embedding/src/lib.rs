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

pub mod error;
pub use error::EmbeddingError;

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
        pub fn new(model_name: Option<&str>) -> Result<Self, EmbeddingError> {
            let (model_enum, id, dims) = resolve_model(model_name);

            let options = fastembed::InitOptions::new(model_enum).with_show_download_progress(true);

            let model = fastembed::TextEmbedding::try_new(options)
                .map_err(|e| EmbeddingError::Internal(format!("fastembed init failed: {e}")))?;

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
        pub fn new(_model_name: Option<&str>) -> Result<Self, EmbeddingError> {
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

// ── Fallback chaining provider ────────────────────────────────────────────

/// Chains two embedding providers: tries primary first, falls back on error.
pub struct FallbackEmbeddingProvider {
    primary: std::sync::Arc<dyn EmbeddingProvider>,
    fallback: std::sync::Arc<dyn EmbeddingProvider>,
}

impl FallbackEmbeddingProvider {
    pub fn new(
        primary: std::sync::Arc<dyn EmbeddingProvider>,
        fallback: std::sync::Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl EmbeddingProvider for FallbackEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
        match self.primary.embed(text).await {
            Ok(v) => Ok(v),
            Err(e) => {
                tracing::warn!("primary embedding failed, trying fallback: {e}");
                self.fallback.embed(text).await
            }
        }
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        match self.primary.embed_batch(texts).await {
            Ok(v) => Ok(v),
            Err(e) => {
                tracing::warn!("primary batch embedding failed, trying fallback: {e}");
                self.fallback.embed_batch(texts).await
            }
        }
    }

    fn dimensions(&self) -> usize {
        self.primary.dimensions()
    }

    fn model_id(&self) -> &str {
        self.primary.model_id()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── resolve_model parameterized tests (fastembed feature) ─────────────

    #[cfg(feature = "fastembed-local")]
    mod resolve_model_tests {
        use super::*;

        /// Helper: assert resolve_model returns the expected (model_id, dimensions).
        fn assert_resolves(input: Option<&str>, expected_id: &str, expected_dims: usize) {
            let (_model, model_id, dimensions) = fastembed_impl::resolve_model(input);
            assert_eq!(
                model_id, expected_id,
                "model_id mismatch for input {input:?}"
            );
            assert_eq!(
                dimensions, expected_dims,
                "dimensions mismatch for input {input:?}"
            );
        }

        // ── Default (None) ───────────────────────────────────────────────

        #[test]
        fn none_defaults_to_quantized_minilm() {
            assert_resolves(None, "all-MiniLM-L6-v2-Q", 384);
        }

        // ── Quantized variants (INT8) ────────────────────────────────────

        #[test]
        fn all_minilm_l6_v2_q_pascal() {
            assert_resolves(Some("AllMiniLML6V2Q"), "all-MiniLM-L6-v2-Q", 384);
        }

        #[test]
        fn all_minilm_l6_v2_q_kebab() {
            assert_resolves(Some("all-MiniLM-L6-v2-Q"), "all-MiniLM-L6-v2-Q", 384);
        }

        #[test]
        fn all_minilm_l12_v2_q_pascal() {
            assert_resolves(Some("AllMiniLML12V2Q"), "all-MiniLM-L12-v2-Q", 384);
        }

        #[test]
        fn all_minilm_l12_v2_q_kebab() {
            assert_resolves(Some("all-MiniLM-L12-v2-Q"), "all-MiniLM-L12-v2-Q", 384);
        }

        #[test]
        fn bge_small_en_v15_q_pascal() {
            assert_resolves(Some("BGESmallENV15Q"), "bge-small-en-v1.5-Q", 384);
        }

        #[test]
        fn bge_small_en_v15_q_kebab() {
            assert_resolves(Some("bge-small-en-v1.5-Q"), "bge-small-en-v1.5-Q", 384);
        }

        #[test]
        fn bge_base_en_v15_q_pascal() {
            assert_resolves(Some("BGEBaseENV15Q"), "bge-base-en-v1.5-Q", 768);
        }

        #[test]
        fn bge_base_en_v15_q_kebab() {
            assert_resolves(Some("bge-base-en-v1.5-Q"), "bge-base-en-v1.5-Q", 768);
        }

        // ── Full-precision variants (FP32) ───────────────────────────────

        #[test]
        fn all_minilm_l6_v2_pascal() {
            assert_resolves(Some("AllMiniLML6V2"), "all-MiniLM-L6-v2", 384);
        }

        #[test]
        fn all_minilm_l6_v2_kebab() {
            assert_resolves(Some("all-MiniLM-L6-v2"), "all-MiniLM-L6-v2", 384);
        }

        #[test]
        fn all_minilm_l12_v2_pascal() {
            assert_resolves(Some("AllMiniLML12V2"), "all-MiniLM-L12-v2", 384);
        }

        #[test]
        fn all_minilm_l12_v2_kebab() {
            assert_resolves(Some("all-MiniLM-L12-v2"), "all-MiniLM-L12-v2", 384);
        }

        #[test]
        fn bge_small_en_v15_pascal() {
            assert_resolves(Some("BGESmallENV15"), "bge-small-en-v1.5", 384);
        }

        #[test]
        fn bge_small_en_v15_kebab() {
            assert_resolves(Some("bge-small-en-v1.5"), "bge-small-en-v1.5", 384);
        }

        #[test]
        fn bge_base_en_v15_pascal() {
            assert_resolves(Some("BGEBaseENV15"), "bge-base-en-v1.5", 768);
        }

        #[test]
        fn bge_base_en_v15_kebab() {
            assert_resolves(Some("bge-base-en-v1.5"), "bge-base-en-v1.5", 768);
        }

        // ── Unknown / fallback ───────────────────────────────────────────

        #[test]
        fn unknown_model_falls_back_to_quantized_minilm() {
            assert_resolves(Some("bogus-model"), "all-MiniLM-L6-v2-Q", 384);
        }

        #[test]
        fn empty_string_falls_back_to_quantized_minilm() {
            assert_resolves(Some(""), "all-MiniLM-L6-v2-Q", 384);
        }

        #[test]
        fn case_sensitive_mismatch_falls_back() {
            // "allminilml6v2q" is not a recognised name (lowercase)
            assert_resolves(Some("allminilml6v2q"), "all-MiniLM-L6-v2-Q", 384);
        }

        // ── Dimension grouping ───────────────────────────────────────────

        #[test]
        fn all_384_dim_models() {
            let names_384 = [
                "AllMiniLML6V2Q",
                "all-MiniLM-L6-v2-Q",
                "AllMiniLML12V2Q",
                "all-MiniLM-L12-v2-Q",
                "BGESmallENV15Q",
                "bge-small-en-v1.5-Q",
                "AllMiniLML6V2",
                "all-MiniLM-L6-v2",
                "AllMiniLML12V2",
                "all-MiniLM-L12-v2",
                "BGESmallENV15",
                "bge-small-en-v1.5",
            ];
            for name in names_384 {
                let (_, _, dims) = fastembed_impl::resolve_model(Some(name));
                assert_eq!(dims, 384, "expected 384 dims for {name}");
            }
        }

        #[test]
        fn all_768_dim_models() {
            let names_768 = [
                "BGEBaseENV15Q",
                "bge-base-en-v1.5-Q",
                "BGEBaseENV15",
                "bge-base-en-v1.5",
            ];
            for name in names_768 {
                let (_, _, dims) = fastembed_impl::resolve_model(Some(name));
                assert_eq!(dims, 768, "expected 768 dims for {name}");
            }
        }
    }

    // ── fastembed network-dependent tests (ignored) ──────────────────────

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

    // ── Stub provider tests (no fastembed feature) ───────────────────────

    #[cfg(not(feature = "fastembed-local"))]
    mod stub_tests {
        use super::*;
        use oneshim_core::ports::embedding_provider::EmbeddingProvider;

        #[test]
        fn stub_new_succeeds() {
            let provider = LocalEmbeddingProvider::new(None);
            assert!(provider.is_ok(), "stub constructor should always succeed");
        }

        #[test]
        fn stub_new_with_any_model_name_succeeds() {
            // Stub ignores the model_name parameter entirely.
            let provider = LocalEmbeddingProvider::new(Some("AllMiniLML6V2"));
            assert!(provider.is_ok());
        }

        #[test]
        fn stub_dimensions_returns_384() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            assert_eq!(provider.dimensions(), 384);
        }

        #[test]
        fn stub_model_id_is_stub_identifier() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            assert_eq!(provider.model_id(), "stub-no-fastembed");
        }

        #[tokio::test]
        async fn stub_embed_returns_error() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            let result = provider.embed("hello").await;
            assert!(result.is_err());
            let err = result.unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("fastembed-local feature is not enabled"),
                "error should explain the feature is disabled, got: {msg}"
            );
        }

        #[tokio::test]
        async fn stub_embed_batch_returns_error() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            let texts = vec!["a".to_owned(), "b".to_owned()];
            let result = provider.embed_batch(&texts).await;
            assert!(result.is_err());
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("fastembed-local feature is not enabled"),
                "batch error should explain the feature is disabled, got: {msg}"
            );
        }

        #[tokio::test]
        async fn stub_embed_empty_text_still_returns_error() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            let result = provider.embed("").await;
            assert!(result.is_err(), "stub should error even on empty input");
        }

        #[tokio::test]
        async fn stub_embed_batch_empty_slice_returns_error() {
            let provider = LocalEmbeddingProvider::new(None).unwrap();
            let result = provider.embed_batch(&[]).await;
            assert!(result.is_err(), "stub should error even on empty batch");
        }
    }

    // ── Error type tests (feature-independent) ──────────────────────────

    mod error_tests {
        use super::*;

        #[test]
        fn embedding_error_internal_display() {
            let err = EmbeddingError::Internal("test failure".to_owned());
            assert_eq!(err.to_string(), "internal error: test failure");
        }

        #[test]
        fn embedding_error_from_core_error() {
            let core = CoreError::Internal("core problem".to_owned());
            let emb: EmbeddingError = core.into();
            // The transparent variant should preserve the CoreError message.
            assert!(
                emb.to_string().contains("core problem"),
                "should contain original message, got: {}",
                emb
            );
        }

        #[test]
        fn embedding_error_into_core_error_internal() {
            let emb = EmbeddingError::Internal("embed fail".to_owned());
            let core: CoreError = emb.into();
            assert!(matches!(core, CoreError::Internal(_)));
            assert!(core.to_string().contains("embed fail"));
        }

        #[test]
        fn embedding_error_into_core_error_roundtrip() {
            // CoreError -> EmbeddingError -> CoreError preserves the variant.
            let original = CoreError::Internal("roundtrip".to_owned());
            let emb: EmbeddingError = original.into();
            let back: CoreError = emb.into();
            assert!(matches!(back, CoreError::Internal(_)));
            assert!(back.to_string().contains("roundtrip"));
        }

        #[test]
        fn embedding_error_internal_is_debug_printable() {
            let err = EmbeddingError::Internal("debug check".to_owned());
            let debug = format!("{err:?}");
            assert!(
                debug.contains("Internal"),
                "Debug should contain variant name, got: {debug}"
            );
        }

        #[test]
        fn embedding_error_core_variant_is_debug_printable() {
            let core = CoreError::Network("net err".to_owned());
            let emb: EmbeddingError = core.into();
            let debug = format!("{emb:?}");
            assert!(
                debug.contains("Core"),
                "Debug should contain Core variant, got: {debug}"
            );
        }

        #[test]
        fn core_error_network_converts_to_embedding_error() {
            let core = CoreError::Network("timeout".to_owned());
            let emb: EmbeddingError = core.into();
            // Converting back should preserve as CoreError (via transparent).
            let back: CoreError = emb.into();
            assert!(matches!(back, CoreError::Network(_)));
        }
    }

    // ── FallbackEmbeddingProvider tests ─────────────────────────────────

    mod fallback_tests {
        use super::*;
        use oneshim_core::ports::embedding_provider::EmbeddingProvider;
        use std::sync::Arc;

        /// Mock provider that always succeeds, returning vectors of a given value.
        struct OkProvider {
            value: f32,
            dims: usize,
        }

        #[async_trait]
        impl EmbeddingProvider for OkProvider {
            async fn embed(&self, _text: &str) -> Result<Vec<f32>, CoreError> {
                Ok(vec![self.value; self.dims])
            }

            async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
                Ok(texts.iter().map(|_| vec![self.value; self.dims]).collect())
            }

            fn dimensions(&self) -> usize {
                self.dims
            }

            fn model_id(&self) -> &str {
                "ok-mock"
            }
        }

        /// Mock provider that always fails.
        struct ErrProvider;

        #[async_trait]
        impl EmbeddingProvider for ErrProvider {
            async fn embed(&self, _text: &str) -> Result<Vec<f32>, CoreError> {
                Err(CoreError::Internal("mock primary failure".into()))
            }

            async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
                Err(CoreError::Internal("mock primary batch failure".into()))
            }

            fn dimensions(&self) -> usize {
                384
            }

            fn model_id(&self) -> &str {
                "err-mock"
            }
        }

        #[tokio::test]
        async fn test_fallback_primary_succeeds() {
            let primary: Arc<dyn EmbeddingProvider> = Arc::new(OkProvider {
                value: 1.0,
                dims: 4,
            });
            let fallback: Arc<dyn EmbeddingProvider> = Arc::new(OkProvider {
                value: 9.9,
                dims: 4,
            });
            let provider = FallbackEmbeddingProvider::new(primary, fallback);

            let result = provider.embed("hello").await.unwrap();
            // Should get primary's value (1.0), not fallback's (9.9)
            assert_eq!(result, vec![1.0; 4]);

            let batch = provider
                .embed_batch(&["a".to_owned(), "b".to_owned()])
                .await
                .unwrap();
            assert_eq!(batch.len(), 2);
            assert_eq!(batch[0], vec![1.0; 4]);
        }

        #[tokio::test]
        async fn test_fallback_primary_fails() {
            let primary: Arc<dyn EmbeddingProvider> = Arc::new(ErrProvider);
            let fallback: Arc<dyn EmbeddingProvider> = Arc::new(OkProvider {
                value: 2.0,
                dims: 4,
            });
            let provider = FallbackEmbeddingProvider::new(primary, fallback);

            let result = provider.embed("hello").await.unwrap();
            // Primary fails, should get fallback's value (2.0)
            assert_eq!(result, vec![2.0; 4]);

            let batch = provider.embed_batch(&["a".to_owned()]).await.unwrap();
            assert_eq!(batch[0], vec![2.0; 4]);
        }

        #[tokio::test]
        async fn test_fallback_both_fail() {
            let primary: Arc<dyn EmbeddingProvider> = Arc::new(ErrProvider);
            let fallback: Arc<dyn EmbeddingProvider> = Arc::new(ErrProvider);
            let provider = FallbackEmbeddingProvider::new(primary, fallback);

            let result = provider.embed("hello").await;
            assert!(result.is_err());

            let batch_result = provider.embed_batch(&["a".to_owned()]).await;
            assert!(batch_result.is_err());
        }
    }
}
