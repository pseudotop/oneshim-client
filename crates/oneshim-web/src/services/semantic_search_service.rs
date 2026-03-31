//! Semantic search service — mode routing, embedding, hybrid scoring.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tracing::{debug, warn};

use oneshim_api_contracts::search::SemanticSearchResult;
use oneshim_core::error::CoreError;
use oneshim_core::ports::embedding_provider::EmbeddingProvider;
use oneshim_core::ports::text_search::TextSearchProvider;
use oneshim_core::ports::vector_store::VectorStore;

use crate::AppState;

/// Sanitize query text before embedding (masks email-like tokens).
pub(crate) fn sanitize_query(text: &str) -> String {
    text.split_whitespace()
        .map(|token| {
            if token.contains('@') && token.contains('.') {
                "[EMAIL]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve search mode from optional parameter. Defaults to "hybrid".
pub(crate) fn resolve_mode(mode: Option<&str>) -> &str {
    match mode {
        Some("semantic") => "semantic",
        Some("keyword") => "keyword",
        _ => "hybrid",
    }
}

/// Execute a search with mode dispatch and provider availability checks.
pub async fn execute(
    state: &AppState,
    query: &str,
    limit: usize,
    mode: &str,
) -> Result<Vec<SemanticSearchResult>, String> {
    match mode {
        "keyword" => {
            let ts = state.text_search.as_ref().ok_or_else(|| {
                "Keyword search is not available (text search provider not configured)".to_string()
            })?;
            keyword_search(ts, state, query, limit)
                .await
                .map_err(|e| format!("Keyword search failed: {e}"))
        }
        _ => {
            let vs = state.vector_store.as_ref().ok_or_else(|| {
                "Semantic search is not available (embedding pipeline not configured)".to_string()
            })?;
            let ep = state.embedding_provider.as_ref().ok_or_else(|| {
                "Semantic search is not available (embedding provider not configured)".to_string()
            })?;
            vector_search(vs, ep, state, query, limit, mode == "hybrid")
                .await
                .map_err(|e| format!("Vector search failed: {e}"))
        }
    }
}

/// Execute keyword-only search via TextSearchProvider.
pub async fn keyword_search(
    text_search: &Arc<dyn TextSearchProvider>,
    state: &AppState,
    query: &str,
    limit: usize,
) -> Result<Vec<SemanticSearchResult>, CoreError> {
    let sanitized = sanitize_query(query);
    let fts_results = text_search.search_fts(&sanitized, limit).await?;

    let segment_ids: Vec<String> = fts_results.iter().map(|r| r.segment_id.clone()).collect();
    let segment_details = state
        .storage
        .get_segment_details(&segment_ids)
        .unwrap_or_else(|e| {
            warn!("Failed to fetch segment details: {e}");
            HashMap::new()
        });

    Ok(fts_results
        .into_iter()
        .map(|r| {
            let detail = segment_details.get(&r.segment_id);
            SemanticSearchResult {
                segment_id: r.segment_id,
                content_type: r.content_type,
                content_label: None,
                original_text: r.matched_text,
                score: r.rank,
                similarity: 0.0,
                time_decay: 0.0,
                timestamp: detail.map(|d| d.start_time.clone()).unwrap_or_default(),
                segment_start: detail.map(|d| d.start_time.clone()),
                segment_end: detail.map(|d| d.end_time.clone()),
                duration_secs: detail.map(|d| d.duration_secs),
                llm_summary: detail.and_then(|d| d.llm_summary.clone()),
                dominant_category: detail.map(|d| d.dominant_category.clone()),
                regime_label: detail.and_then(|d| d.regime_label.clone()),
            }
        })
        .collect())
}

/// Execute semantic/hybrid search via VectorStore + EmbeddingProvider.
pub async fn vector_search(
    vector_store: &Arc<dyn VectorStore>,
    embedding_provider: &Arc<dyn EmbeddingProvider>,
    state: &AppState,
    query: &str,
    limit: usize,
    hybrid: bool,
) -> Result<Vec<SemanticSearchResult>, CoreError> {
    let sanitized = sanitize_query(query);

    let query_vector = embedding_provider
        .embed(&sanitized)
        .await
        .map_err(|e| CoreError::Internal(format!("Embedding failed: {e}")))?;
    let time_decay_hours = 168.0; // 1 week default
    let vector_results = vector_store
        .search(&query_vector, limit, time_decay_hours)
        .await?;

    // Hybrid: boost score for keyword matches
    let fts_boost_ids: HashSet<String> = if hybrid {
        if let Some(ref text_search) = state.text_search {
            match text_search.search_fts(&sanitized, limit).await {
                Ok(fts_results) => fts_results.into_iter().map(|r| r.segment_id).collect(),
                Err(e) => {
                    debug!("FTS fallback skipped (text search error): {e}");
                    HashSet::new()
                }
            }
        } else {
            debug!("Hybrid mode requested but TextSearchProvider not configured");
            HashSet::new()
        }
    } else {
        HashSet::new()
    };

    let segment_ids: Vec<String> = vector_results
        .iter()
        .map(|r| r.segment_id.clone())
        .collect();
    let segment_details = state
        .storage
        .get_segment_details(&segment_ids)
        .unwrap_or_else(|e| {
            warn!("Failed to fetch segment details: {e}");
            HashMap::new()
        });

    Ok(vector_results
        .into_iter()
        .map(|r| {
            let detail = segment_details.get(&r.segment_id);
            let score_boost = if fts_boost_ids.contains(&r.segment_id) {
                0.1
            } else {
                0.0
            };
            SemanticSearchResult {
                segment_id: r.segment_id,
                content_type: format!("{:?}", r.content_type),
                content_label: r.content_label,
                original_text: r.original_text,
                score: (r.score + score_boost).min(1.0),
                similarity: r.similarity,
                time_decay: r.time_decay,
                timestamp: r.timestamp.to_rfc3339(),
                segment_start: detail.map(|d| d.start_time.clone()),
                segment_end: detail.map(|d| d.end_time.clone()),
                duration_secs: detail.map(|d| d.duration_secs),
                llm_summary: detail.and_then(|d| d.llm_summary.clone()),
                dominant_category: detail.map(|d| d.dominant_category.clone()),
                regime_label: detail.and_then(|d| d.regime_label.clone()),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_query_masks_email() {
        let q = "emails from user@example.com about auth";
        let sanitized = sanitize_query(q);
        assert!(!sanitized.contains("user@example.com"));
        assert!(sanitized.contains("[EMAIL]"));
    }

    #[test]
    fn sanitize_query_passes_through_clean_text() {
        let q = "what did I work on yesterday";
        assert_eq!(sanitize_query(q), q);
    }

    #[test]
    fn resolve_mode_defaults_to_hybrid() {
        assert_eq!(resolve_mode(None), "hybrid");
        assert_eq!(resolve_mode(Some("unknown")), "hybrid");
    }

    #[test]
    fn resolve_mode_recognizes_valid_modes() {
        assert_eq!(resolve_mode(Some("semantic")), "semantic");
        assert_eq!(resolve_mode(Some("keyword")), "keyword");
    }
}
