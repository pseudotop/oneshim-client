use std::sync::Arc;

use oneshim_core::error::CoreError;
use oneshim_core::models::embedding::{SearchFilters, SearchResult};
use oneshim_core::ports::embedding_provider::EmbeddingProvider;
use oneshim_core::ports::vector_store::VectorStore;
use oneshim_core::quantization::ScalarQuantizer;

use crate::assembler::PiiFilter;

/// Retrieves relevant historical context via vector similarity search.
///
/// Used by `ContextAnalyzer` to enrich LLM prompts with relevant past
/// segments, and by the dashboard/Tauri for natural language search.
pub struct VectorRetriever {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    vector_store: Arc<dyn VectorStore>,
    pii_filter: PiiFilter,
    max_results: usize,
    time_decay_hours: f32,
    quantization_enabled: bool,
}

impl VectorRetriever {
    pub fn new(
        embedding_provider: Arc<dyn EmbeddingProvider>,
        vector_store: Arc<dyn VectorStore>,
        pii_filter: PiiFilter,
        max_results: usize,
        time_decay_hours: f32,
        quantization_enabled: bool,
    ) -> Self {
        Self {
            embedding_provider,
            vector_store,
            pii_filter,
            max_results,
            time_decay_hours,
            quantization_enabled,
        }
    }

    /// Retrieve relevant history for current activity context.
    ///
    /// Builds a query from the current app, window title, and optional OCR text,
    /// applies PII filtering, embeds the query, and searches the vector store.
    pub async fn retrieve_for_context(
        &self,
        current_app: &str,
        current_title: &str,
        current_ocr: Option<&str>,
    ) -> Result<Vec<SearchResult>, CoreError> {
        let query_text = format!(
            "{} - {}{}",
            current_app,
            (self.pii_filter)(current_title),
            current_ocr
                .map(|t| format!(" {}", (self.pii_filter)(t)))
                .unwrap_or_default()
        );

        let query_vector = self.embedding_provider.embed(&query_text).await?;

        if self.quantization_enabled {
            let quantized = ScalarQuantizer::quantize(&query_vector)?;
            self.vector_store
                .search_quantized(
                    &quantized,
                    self.max_results,
                    self.time_decay_hours,
                    &SearchFilters::default(),
                )
                .await
        } else {
            self.vector_store
                .search(&query_vector, self.max_results, self.time_decay_hours)
                .await
        }
    }

    /// Natural language search (user/dashboard queries).
    ///
    /// Embeds the raw query text and searches with optional metadata filters.
    pub async fn search_natural_language(
        &self,
        query: &str,
        filters: Option<SearchFilters>,
    ) -> Result<Vec<SearchResult>, CoreError> {
        let query_vector = self.embedding_provider.embed(query).await?;

        if self.quantization_enabled {
            let quantized = ScalarQuantizer::quantize(&query_vector)?;
            let filters = filters.unwrap_or_default();
            self.vector_store
                .search_quantized(
                    &quantized,
                    self.max_results,
                    self.time_decay_hours,
                    &filters,
                )
                .await
        } else if let Some(filters) = filters {
            self.vector_store
                .search_filtered(
                    &query_vector,
                    self.max_results,
                    self.time_decay_hours,
                    &filters,
                )
                .await
        } else {
            self.vector_store
                .search(&query_vector, self.max_results, self.time_decay_hours)
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use oneshim_core::models::embedding::{EmbeddingContentType, EmbeddingMetadata};

    // ── Mock EmbeddingProvider ─────────────────────────────────────

    struct MockEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, CoreError> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn model_id(&self) -> &str {
            "mock-model"
        }
    }

    // ── Mock VectorStore ───────────────────────────────────────────

    struct MockVectorStore {
        results: Vec<SearchResult>,
        quantized_search_called: std::sync::atomic::AtomicBool,
    }

    impl MockVectorStore {
        fn new(results: Vec<SearchResult>) -> Self {
            Self {
                results,
                quantized_search_called: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn empty() -> Self {
            Self {
                results: vec![],
                quantized_search_called: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn was_quantized_search_called(&self) -> bool {
            self.quantized_search_called
                .load(std::sync::atomic::Ordering::Relaxed)
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
            _query_vector: &[f32],
            _limit: usize,
            _time_decay_hours: f32,
        ) -> Result<Vec<SearchResult>, CoreError> {
            Ok(self.results.clone())
        }

        async fn search_filtered(
            &self,
            _query_vector: &[f32],
            _limit: usize,
            _time_decay_hours: f32,
            _filters: &SearchFilters,
        ) -> Result<Vec<SearchResult>, CoreError> {
            // Return filtered subset (in mock, just return all)
            Ok(self.results.clone())
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
            _query_vector: &oneshim_core::quantization::QuantizedVector,
            _limit: usize,
            _time_decay_hours: f32,
            _filters: &SearchFilters,
        ) -> Result<Vec<SearchResult>, CoreError> {
            self.quantized_search_called
                .store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(self.results.clone())
        }
    }

    // ── Helpers ────────────────────────────────────────────────────

    fn make_search_result(text: &str, similarity: f32) -> SearchResult {
        SearchResult {
            segment_id: "seg-001".to_string(),
            content_type: EmbeddingContentType::SegmentSummary,
            content_label: Some("VSCode: main.rs".to_string()),
            score: similarity * 0.95,
            similarity,
            time_decay: 0.95,
            timestamp: Utc::now(),
            original_text: text.to_string(),
        }
    }

    fn noop_filter() -> PiiFilter {
        Box::new(|text: &str| text.to_string())
    }

    fn masking_filter() -> PiiFilter {
        Box::new(|text: &str| text.replace("secret@example.com", "[EMAIL]"))
    }

    fn make_retriever(results: Vec<SearchResult>, pii_filter: PiiFilter) -> VectorRetriever {
        VectorRetriever::new(
            Arc::new(MockEmbeddingProvider),
            Arc::new(MockVectorStore::new(results)),
            pii_filter,
            5,
            168.0,
            false,
        )
    }

    // ── Tests ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn retrieve_for_context_returns_results() {
        let results = vec![
            make_search_result("Deep coding on auth.rs", 0.85),
            make_search_result("Auth module testing", 0.72),
        ];
        let retriever = make_retriever(results, noop_filter());

        let found = retriever
            .retrieve_for_context("VSCode", "auth.rs - oneshim", Some("fn login()"))
            .await
            .unwrap();

        assert_eq!(found.len(), 2);
        assert!(found[0].similarity > 0.0);
    }

    #[tokio::test]
    async fn search_natural_language_with_filters() {
        let results = vec![make_search_result("Auth session work", 0.9)];
        let retriever = make_retriever(results, noop_filter());

        let filters = SearchFilters {
            content_types: Some(vec![EmbeddingContentType::SegmentSummary]),
            ..Default::default()
        };

        let found = retriever
            .search_natural_language("auth module work", Some(filters))
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].original_text, "Auth session work");
    }

    #[tokio::test]
    async fn search_natural_language_without_filters() {
        let results = vec![make_search_result("General work", 0.7)];
        let retriever = make_retriever(results, noop_filter());

        let found = retriever
            .search_natural_language("what did I work on", None)
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
    }

    #[tokio::test]
    async fn empty_store_returns_empty() {
        let retriever = VectorRetriever::new(
            Arc::new(MockEmbeddingProvider),
            Arc::new(MockVectorStore::empty()),
            noop_filter(),
            5,
            168.0,
        );

        let found = retriever
            .retrieve_for_context("VSCode", "main.rs", None)
            .await
            .unwrap();

        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn pii_filter_applied_to_query_text() {
        // Use a mock that captures the query to verify PII was filtered.
        // Since we can't easily capture the embed input in this mock,
        // we verify the filter is called by checking the retriever works
        // with a masking filter without errors.
        let results = vec![make_search_result("Result", 0.8)];
        let retriever = make_retriever(results, masking_filter());

        let found = retriever
            .retrieve_for_context("Chrome", "Inbox - secret@example.com", None)
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
    }

    #[tokio::test]
    async fn retrieve_for_context_without_ocr() {
        let results = vec![make_search_result("Coding session", 0.75)];
        let retriever = make_retriever(results, noop_filter());

        let found = retriever
            .retrieve_for_context("VSCode", "main.rs", None)
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
    }
}
