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
