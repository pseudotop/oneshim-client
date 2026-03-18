use axum::extract::{Query, State};
use axum::Json;
use oneshim_api_contracts::search::{SemanticSearchQuery, SemanticSearchResult};
use tracing::{debug, warn};

use crate::error::ApiError;
use crate::AppState;

/// Basic PII sanitizer for query text before embedding.
/// Masks tokens that look like email addresses (contain '@').
fn sanitize_query(text: &str) -> String {
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

/// GET /api/semantic-search?q=auth+module&limit=10
pub async fn semantic_search(
    State(state): State<AppState>,
    Query(params): Query<SemanticSearchQuery>,
) -> Result<Json<Vec<SemanticSearchResult>>, ApiError> {
    debug!("GET /api/semantic-search q={}", params.q);

    let (vector_store, embedding_provider) = match (&state.vector_store, &state.embedding_provider)
    {
        (Some(vs), Some(ep)) => (vs.clone(), ep.clone()),
        _ => {
            return Err(ApiError::ServiceUnavailable(
                "Semantic search is not available (embedding pipeline not configured)".to_string(),
            ));
        }
    };

    // Apply PII filtering to query text before embedding
    let sanitized_query = sanitize_query(&params.q);

    let query_vector = embedding_provider
        .embed(&sanitized_query)
        .await
        .map_err(|e| ApiError::Internal(format!("Embedding failed: {e}")))?;

    let limit = params.limit.unwrap_or(10).min(50);
    let time_decay_hours = 168.0; // 1 week default

    let results = vector_store
        .search(&query_vector, limit, time_decay_hours)
        .await
        .map_err(|e| ApiError::Internal(format!("Vector search failed: {e}")))?;

    // Enrich results with segment details from storage
    let segment_ids: Vec<String> = results.iter().map(|r| r.segment_id.clone()).collect();
    let segment_details = state
        .storage
        .get_segment_details(&segment_ids)
        .unwrap_or_else(|e| {
            warn!("Failed to fetch segment details: {e}");
            std::collections::HashMap::new()
        });

    let response: Vec<SemanticSearchResult> = results
        .into_iter()
        .map(|r| {
            let detail = segment_details.get(&r.segment_id);
            SemanticSearchResult {
                segment_id: r.segment_id,
                content_type: format!("{:?}", r.content_type),
                content_label: r.content_label,
                original_text: r.original_text,
                score: r.score,
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
        .collect();

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_search_query_defaults() {
        let json = r#"{"q": "auth module"}"#;
        let query: SemanticSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.q, "auth module");
        assert!(query.limit.is_none());
    }

    #[test]
    fn semantic_search_result_serializes() {
        let result = SemanticSearchResult {
            segment_id: "seg-001".to_string(),
            content_type: "SegmentSummary".to_string(),
            content_label: Some("VSCode: main.rs".to_string()),
            original_text: "Focused coding on auth module".to_string(),
            score: 0.85,
            similarity: 0.9,
            time_decay: 0.95,
            timestamp: "2026-03-18T10:00:00+00:00".to_string(),
            segment_start: Some("2026-03-18T09:30:00+00:00".to_string()),
            segment_end: Some("2026-03-18T10:00:00+00:00".to_string()),
            duration_secs: Some(1800),
            llm_summary: Some("Focused development session".to_string()),
            dominant_category: Some("Development".to_string()),
            regime_label: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("seg-001"));
        assert!(json.contains("Focused coding"));
        assert!(json.contains("duration_secs"));
    }

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
        let sanitized = sanitize_query(q);
        assert_eq!(sanitized, q);
    }
}
