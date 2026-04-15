//! Port for adaptive vector search that auto-selects the optimal strategy
//! (brute-force, IVF, IVF+binary, HNSW) based on collection size.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::embedding::{SearchFilters, SearchResult};

/// Port for adaptive vector search with strategy auto-selection.
///
/// Implementations determine the best search strategy based on
/// collection size and available indices, then execute the search.
///
/// Primary adapter: `AdaptiveSearchCoordinator` in oneshim-analysis.
#[async_trait]
pub trait AdaptiveSearchPort: Send + Sync {
    /// Search using the auto-selected (or forced) strategy.
    ///
    /// `query_f32` is the raw float32 query vector (quantization is handled
    /// internally). Returns results sorted by descending score.
    async fn search(
        &self,
        query_f32: &[f32],
        limit: usize,
        time_decay_hours: f32,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, CoreError>;

    /// Refresh the cached vector count from the store.
    /// Should be called periodically by the scheduler.
    async fn refresh_count(&self) -> Result<(), CoreError>;
}
