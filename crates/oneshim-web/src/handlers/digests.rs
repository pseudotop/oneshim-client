use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use tracing::debug;

use oneshim_core::models::weekly_digest::WeeklyDigest;

use crate::error::ApiError;
use crate::services::web_contexts::StorageWebContext;

/// Query parameters for listing digests.
#[derive(Debug, Deserialize)]
pub struct DigestListQuery {
    /// Maximum number of digests to return (default: 4).
    pub limit: Option<usize>,
}

/// GET /api/digests?limit=4 — list recent weekly digests.
pub async fn list_digests(
    State(context): State<StorageWebContext>,
    Query(params): Query<DigestListQuery>,
) -> Result<Json<Vec<WeeklyDigest>>, ApiError> {
    let limit = params.limit.unwrap_or(4).min(52);
    debug!("GET /api/digests limit={}", limit);

    let digests = context
        .storage
        .list_weekly_digests(limit)
        .map_err(|e| ApiError::Internal(format!("Failed to list digests: {e}")))?;

    Ok(Json(digests))
}

/// GET /api/digests/current — get the current (partial) week digest.
pub async fn current_digest(
    State(context): State<StorageWebContext>,
) -> Result<Json<Option<WeeklyDigest>>, ApiError> {
    debug!("GET /api/digests/current");

    let digest = context
        .storage
        .get_current_week_digest()
        .map_err(|e| ApiError::Internal(format!("Failed to get current digest: {e}")))?;

    Ok(Json(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_list_query_defaults() {
        let json = r#"{}"#;
        let query: DigestListQuery = serde_json::from_str(json).unwrap();
        assert!(query.limit.is_none());
    }

    #[test]
    fn digest_list_query_with_limit() {
        let json = r#"{"limit": 8}"#;
        let query: DigestListQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.limit, Some(8));
    }
}
