//! Port for generating vector embeddings from text via local or remote models.

use async_trait::async_trait;

use crate::error::CoreError;

/// Port for generating vector embeddings from text.
/// Adapters: local (fastembed-rs) or remote (OpenAI text-embedding-3-small etc.)
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text into a vector of floats.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError>;

    /// Embed a batch of texts. Default implementation calls embed() in sequence.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Number of dimensions in the embedding vector.
    fn dimensions(&self) -> usize;

    /// Identifier of the embedding model (used for versioning stored vectors).
    fn model_id(&self) -> &str;
}

/// No-op embedding provider that returns zero vectors.
/// Used as fallback when both local and remote embedding are unavailable.
#[derive(Debug)]
pub struct NoOpEmbeddingProvider {
    dimensions: usize,
}

impl NoOpEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

#[async_trait]
impl EmbeddingProvider for NoOpEmbeddingProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, CoreError> {
        Ok(vec![0.0; self.dimensions])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        Ok(texts.iter().map(|_| vec![0.0; self.dimensions]).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        "noop"
    }
}
