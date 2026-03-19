//! Adaptive search coordinator that auto-selects the optimal vector search strategy
//! based on collection size and configuration.
//!
//! Strategies:
//! - `BruteForceInt8`: Full scan with INT8 cosine similarity (< 10K vectors)
//! - `IvfInt8`: IVF partitioned scan with INT8 cosine (10K - 100K vectors)
//! - `IvfBinaryRerank`: IVF + 2-bit Hamming filter + INT8 re-rank (>= 100K vectors)

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use oneshim_core::binary_quantizer::BinaryQuantizer;
use oneshim_core::error::CoreError;
use oneshim_core::models::embedding::{SearchFilters, SearchResult};
use oneshim_core::ports::vector_index::VectorIndex;
use oneshim_core::ports::vector_store::VectorStore;
use oneshim_core::quantization::ScalarQuantizer;
use tracing::debug;

/// Search strategies selected by the coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStrategy {
    BruteForceInt8,
    IvfInt8,
    IvfBinaryRerank,
}

/// Configuration for the adaptive search coordinator.
pub struct SearchConfig {
    /// Vector count below which brute-force is used. Default: 10_000.
    pub brute_force_threshold: u64,
    /// Vector count below which IVF-only is used (above = IVF+binary). Default: 100_000.
    pub ivf_threshold: u64,
    /// Oversample factor for 2-bit binary filter stage. Default: 10.
    pub oversample_factor: usize,
    /// Number of IVF partitions to probe. 0 = auto. Default: 0.
    pub default_nprobe: usize,
    /// Force a specific strategy. None = "auto". Values: "brute_force", "ivf", "ivf_binary".
    pub forced_strategy: Option<String>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            brute_force_threshold: 10_000,
            ivf_threshold: 100_000,
            oversample_factor: 10,
            default_nprobe: 0,
            forced_strategy: None,
        }
    }
}

/// Auto-selects the optimal search strategy based on collection size.
pub struct AdaptiveSearchCoordinator {
    vector_store: Arc<dyn VectorStore>,
    vector_index: Arc<dyn VectorIndex>,
    config: SearchConfig,
    /// Cached active vector count, refreshed periodically by the scheduler.
    cached_vector_count: AtomicU64,
}

impl AdaptiveSearchCoordinator {
    pub fn new(
        vector_store: Arc<dyn VectorStore>,
        vector_index: Arc<dyn VectorIndex>,
        config: SearchConfig,
    ) -> Self {
        Self {
            vector_store,
            vector_index,
            config,
            cached_vector_count: AtomicU64::new(0),
        }
    }

    /// Refresh the cached vector count from the store.
    /// Called from the scheduler aggregate loop (not the search hot path).
    pub async fn refresh_count(&self) -> Result<(), CoreError> {
        let count = self.vector_store.count_active_vectors().await?;
        self.cached_vector_count.store(count, Ordering::Relaxed);
        Ok(())
    }

    /// Determine the search strategy based on config and cached vector count.
    /// This is a sync method — reads an atomic counter, no I/O.
    pub fn determine_strategy(&self) -> SearchStrategy {
        if let Some(ref forced) = self.config.forced_strategy {
            return match forced.as_str() {
                "brute_force" => SearchStrategy::BruteForceInt8,
                "ivf" => SearchStrategy::IvfInt8,
                "ivf_binary" => SearchStrategy::IvfBinaryRerank,
                _ => SearchStrategy::BruteForceInt8,
            };
        }

        let count = self.cached_vector_count.load(Ordering::Relaxed);
        if count < self.config.brute_force_threshold {
            SearchStrategy::BruteForceInt8
        } else if count < self.config.ivf_threshold {
            SearchStrategy::IvfInt8
        } else {
            SearchStrategy::IvfBinaryRerank
        }
    }

    /// Compute nprobe: use configured value or auto-select.
    fn compute_nprobe(&self) -> usize {
        if self.config.default_nprobe > 0 {
            return self.config.default_nprobe;
        }
        // Auto: sqrt(n_clusters) ≈ 4th-root(n_vectors), minimum 1
        let count = self.cached_vector_count.load(Ordering::Relaxed) as f64;
        let n_clusters = count.sqrt();
        let nprobe = (n_clusters / 10.0).ceil() as usize;
        nprobe.max(1)
    }

    /// Search using the auto-selected (or forced) strategy.
    pub async fn search(
        &self,
        query_f32: &[f32],
        limit: usize,
        time_decay_hours: f32,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, CoreError> {
        let strategy = self.determine_strategy();
        debug!(?strategy, "AdaptiveSearchCoordinator selected strategy");

        let quantized = ScalarQuantizer::quantize(query_f32)?;

        match strategy {
            SearchStrategy::BruteForceInt8 => {
                self.vector_store
                    .search_quantized(&quantized, limit, time_decay_hours, filters)
                    .await
            }
            SearchStrategy::IvfInt8 => {
                let nprobe = self.compute_nprobe();
                self.vector_index
                    .search_ivf(&quantized, nprobe, limit, time_decay_hours, filters)
                    .await
            }
            SearchStrategy::IvfBinaryRerank => {
                let nprobe = self.compute_nprobe();
                let thresholds = self.vector_index.load_quantile_thresholds().await?;

                match thresholds {
                    Some(t) => {
                        let binary_code = BinaryQuantizer::encode(query_f32, &t)?;
                        self.vector_index
                            .search_ivf_binary(
                                &quantized,
                                &binary_code,
                                nprobe,
                                self.config.oversample_factor,
                                limit,
                                time_decay_hours,
                                filters,
                            )
                            .await
                    }
                    None => {
                        // Thresholds not built yet — fall back to IVF-only
                        debug!("quantile thresholds not available, falling back to IVF-only");
                        self.vector_index
                            .search_ivf(&quantized, nprobe, limit, time_decay_hours, filters)
                            .await
                    }
                }
            }
        }
    }

    /// Expose cached count for testing.
    #[cfg(test)]
    pub fn set_cached_count(&self, count: u64) {
        self.cached_vector_count.store(count, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_core::binary_quantizer::{BinaryCode, QuantileThresholds};
    use oneshim_core::models::embedding::{EmbeddingMetadata, SearchResult};
    use oneshim_core::ports::vector_index::IndexMeta;
    use oneshim_core::quantization::QuantizedVector;
    use std::sync::atomic::AtomicBool;

    // ── Mock VectorStore ───────────────────────────────────────────

    struct MockVectorStore {
        brute_force_called: AtomicBool,
        active_count: u64,
    }

    impl MockVectorStore {
        fn new(active_count: u64) -> Self {
            Self {
                brute_force_called: AtomicBool::new(false),
                active_count,
            }
        }
    }

    #[async_trait]
    impl VectorStore for MockVectorStore {
        async fn store(
            &self,
            _vector: Vec<f32>,
            _metadata: EmbeddingMetadata,
        ) -> Result<(), CoreError> {
            Ok(())
        }
        async fn search(
            &self,
            _query: &[f32],
            _limit: usize,
            _time_decay: f32,
        ) -> Result<Vec<SearchResult>, CoreError> {
            Ok(vec![])
        }
        async fn search_filtered(
            &self,
            _query: &[f32],
            _limit: usize,
            _time_decay: f32,
            _filters: &SearchFilters,
        ) -> Result<Vec<SearchResult>, CoreError> {
            Ok(vec![])
        }
        async fn enforce_retention(&self, _max_days: u32) -> Result<u64, CoreError> {
            Ok(0)
        }
        async fn mark_stale(&self, _old_model_id: &str) -> Result<u64, CoreError> {
            Ok(0)
        }
        async fn get_current_model_id(&self) -> Result<Option<String>, CoreError> {
            Ok(None)
        }
        async fn get_stale_vectors(&self, _limit: usize) -> Result<Vec<(i64, String)>, CoreError> {
            Ok(vec![])
        }
        async fn update_vector(
            &self,
            _id: i64,
            _vector: Vec<f32>,
            _model_id: &str,
        ) -> Result<(), CoreError> {
            Ok(())
        }
        async fn search_quantized(
            &self,
            _query: &QuantizedVector,
            _limit: usize,
            _time_decay: f32,
            _filters: &SearchFilters,
        ) -> Result<Vec<SearchResult>, CoreError> {
            self.brute_force_called.store(true, Ordering::Relaxed);
            Ok(vec![])
        }
        async fn count_active_vectors(&self) -> Result<u64, CoreError> {
            Ok(self.active_count)
        }
    }

    // ── Mock VectorIndex ──────────────────────────────────────────

    struct MockVectorIndex {
        ivf_called: AtomicBool,
        ivf_binary_called: AtomicBool,
    }

    impl MockVectorIndex {
        fn new() -> Self {
            Self {
                ivf_called: AtomicBool::new(false),
                ivf_binary_called: AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    #[allow(clippy::too_many_arguments)]
    impl VectorIndex for MockVectorIndex {
        async fn search_ivf(
            &self,
            _query: &QuantizedVector,
            _nprobe: usize,
            _limit: usize,
            _time_decay: f32,
            _filters: &SearchFilters,
        ) -> Result<Vec<SearchResult>, CoreError> {
            self.ivf_called.store(true, Ordering::Relaxed);
            Ok(vec![])
        }

        async fn search_ivf_binary(
            &self,
            _query: &QuantizedVector,
            _query_binary: &BinaryCode,
            _nprobe: usize,
            _oversample: usize,
            _limit: usize,
            _time_decay: f32,
            _filters: &SearchFilters,
        ) -> Result<Vec<SearchResult>, CoreError> {
            self.ivf_binary_called.store(true, Ordering::Relaxed);
            Ok(vec![])
        }

        async fn load_quantile_thresholds(&self) -> Result<Option<QuantileThresholds>, CoreError> {
            Ok(Some(QuantileThresholds {
                q25: vec![0.0; 3],
                q50: vec![0.5; 3],
                q75: vec![1.0; 3],
                dimensions: 3,
            }))
        }

        async fn get_index_meta(&self) -> Result<IndexMeta, CoreError> {
            Ok(IndexMeta {
                ivf_built_at: None,
                ivf_vector_count: 0,
                binary_built_at: None,
                total_vector_count: 0,
                unindexed_count: 0,
            })
        }
    }

    // ── Tests ─────────────────────────────────────────────────────

    #[test]
    fn strategy_auto_brute_force() {
        let store = Arc::new(MockVectorStore::new(0));
        let index = Arc::new(MockVectorIndex::new());
        let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default());
        coordinator.set_cached_count(5_000);
        assert_eq!(
            coordinator.determine_strategy(),
            SearchStrategy::BruteForceInt8
        );
    }

    #[test]
    fn strategy_auto_ivf() {
        let store = Arc::new(MockVectorStore::new(0));
        let index = Arc::new(MockVectorIndex::new());
        let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default());
        coordinator.set_cached_count(50_000);
        assert_eq!(coordinator.determine_strategy(), SearchStrategy::IvfInt8);
    }

    #[test]
    fn strategy_auto_ivf_binary() {
        let store = Arc::new(MockVectorStore::new(0));
        let index = Arc::new(MockVectorIndex::new());
        let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default());
        coordinator.set_cached_count(200_000);
        assert_eq!(
            coordinator.determine_strategy(),
            SearchStrategy::IvfBinaryRerank
        );
    }

    #[test]
    fn strategy_forced_brute_force() {
        let store = Arc::new(MockVectorStore::new(0));
        let index = Arc::new(MockVectorIndex::new());
        let config = SearchConfig {
            forced_strategy: Some("brute_force".to_string()),
            ..Default::default()
        };
        let coordinator = AdaptiveSearchCoordinator::new(store, index, config);
        coordinator.set_cached_count(999_999); // Would be IvfBinaryRerank in auto mode
        assert_eq!(
            coordinator.determine_strategy(),
            SearchStrategy::BruteForceInt8
        );
    }

    #[test]
    fn strategy_forced_ivf() {
        let store = Arc::new(MockVectorStore::new(0));
        let index = Arc::new(MockVectorIndex::new());
        let config = SearchConfig {
            forced_strategy: Some("ivf".to_string()),
            ..Default::default()
        };
        let coordinator = AdaptiveSearchCoordinator::new(store, index, config);
        coordinator.set_cached_count(100);
        assert_eq!(coordinator.determine_strategy(), SearchStrategy::IvfInt8);
    }

    #[test]
    fn strategy_forced_ivf_binary() {
        let store = Arc::new(MockVectorStore::new(0));
        let index = Arc::new(MockVectorIndex::new());
        let config = SearchConfig {
            forced_strategy: Some("ivf_binary".to_string()),
            ..Default::default()
        };
        let coordinator = AdaptiveSearchCoordinator::new(store, index, config);
        coordinator.set_cached_count(0);
        assert_eq!(
            coordinator.determine_strategy(),
            SearchStrategy::IvfBinaryRerank
        );
    }

    #[tokio::test]
    async fn refresh_count_updates_atomic() {
        let store = Arc::new(MockVectorStore::new(42_000));
        let index = Arc::new(MockVectorIndex::new());
        let coordinator = AdaptiveSearchCoordinator::new(store, index, SearchConfig::default());
        assert_eq!(coordinator.cached_vector_count.load(Ordering::Relaxed), 0);

        coordinator.refresh_count().await.unwrap();
        assert_eq!(
            coordinator.cached_vector_count.load(Ordering::Relaxed),
            42_000
        );
    }

    #[tokio::test]
    async fn search_delegates_to_brute_force() {
        let store = Arc::new(MockVectorStore::new(100));
        let index = Arc::new(MockVectorIndex::new());
        let coordinator =
            AdaptiveSearchCoordinator::new(store.clone(), index.clone(), SearchConfig::default());
        coordinator.set_cached_count(100);

        let _ = coordinator
            .search(&[0.1, 0.2, 0.3], 5, 168.0, &SearchFilters::default())
            .await;

        assert!(store.brute_force_called.load(Ordering::Relaxed));
        assert!(!index.ivf_called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn search_delegates_to_ivf() {
        let store = Arc::new(MockVectorStore::new(50_000));
        let index = Arc::new(MockVectorIndex::new());
        let coordinator =
            AdaptiveSearchCoordinator::new(store.clone(), index.clone(), SearchConfig::default());
        coordinator.set_cached_count(50_000);

        let _ = coordinator
            .search(&[0.1, 0.2, 0.3], 5, 168.0, &SearchFilters::default())
            .await;

        assert!(!store.brute_force_called.load(Ordering::Relaxed));
        assert!(index.ivf_called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn search_delegates_to_ivf_binary() {
        let store = Arc::new(MockVectorStore::new(200_000));
        let index = Arc::new(MockVectorIndex::new());
        let coordinator =
            AdaptiveSearchCoordinator::new(store.clone(), index.clone(), SearchConfig::default());
        coordinator.set_cached_count(200_000);

        let _ = coordinator
            .search(&[0.1, 0.2, 0.3], 5, 168.0, &SearchFilters::default())
            .await;

        assert!(!store.brute_force_called.load(Ordering::Relaxed));
        assert!(index.ivf_binary_called.load(Ordering::Relaxed));
    }
}
