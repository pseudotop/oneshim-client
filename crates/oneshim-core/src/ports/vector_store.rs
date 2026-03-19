use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::embedding::{EmbeddingMetadata, SearchFilters, SearchResult};

/// Port for storing and searching embedding vectors.
/// Primary adapter: brute-force cosine similarity implementation in oneshim-storage.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Store a vector with its associated metadata.
    async fn store(&self, vector: Vec<f32>, metadata: EmbeddingMetadata) -> Result<(), CoreError>;

    /// Search for the top-k most similar vectors with time decay weighting.
    async fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
        time_decay_hours: f32,
    ) -> Result<Vec<SearchResult>, CoreError>;

    /// Search with additional metadata filters (time range, content type, regime).
    async fn search_filtered(
        &self,
        query_vector: &[f32],
        limit: usize,
        time_decay_hours: f32,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, CoreError>;

    /// Delete embedding vectors older than max_days. Returns count of deleted rows.
    async fn enforce_retention(&self, max_days: u32) -> Result<u64, CoreError>;

    /// Mark vectors produced by an old model as stale. Returns count of marked rows.
    async fn mark_stale(&self, old_model_id: &str) -> Result<u64, CoreError>;

    /// Get the model_id of the most recent non-stale vector, if any.
    async fn get_current_model_id(&self) -> Result<Option<String>, CoreError>;

    /// Fetch a batch of stale vectors for re-embedding.
    /// Returns (id, original_text) pairs. Limit controls batch size.
    async fn get_stale_vectors(&self, limit: usize) -> Result<Vec<(i64, String)>, CoreError>;

    /// Update a re-embedded vector: replace the BLOB, model_id, and clear stale flag.
    async fn update_vector(
        &self,
        id: i64,
        vector: Vec<f32>,
        model_id: &str,
    ) -> Result<(), CoreError>;
}
