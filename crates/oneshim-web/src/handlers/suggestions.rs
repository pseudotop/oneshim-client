use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use oneshim_api_contracts::suggestions::SuggestionDto;
use tracing::debug;

use crate::error::ApiError;
use crate::services::suggestions_service::{SuggestionsCommandService, SuggestionsQueryService};
use crate::services::web_contexts::StorageWebContext;

/// GET /api/suggestions — list non-dismissed suggestions, newest first.
pub async fn list_suggestions(
    State(context): State<StorageWebContext>,
) -> Result<Json<Vec<SuggestionDto>>, ApiError> {
    debug!("GET /api/suggestions");
    Ok(Json(
        SuggestionsQueryService::new(context).list_suggestions(50)?,
    ))
}

/// POST /api/suggestions/:id/dismiss — dismiss a suggestion by its UUID.
pub async fn dismiss_suggestion(
    State(context): State<StorageWebContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    debug!("POST /api/suggestions/{}/dismiss", id);
    let found = SuggestionsCommandService::new(context).dismiss(&id)?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!(
            "suggestion {id} not found or already dismissed"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestion_dto_serializes() {
        let dto = SuggestionDto {
            id: 1,
            suggestion_id: "abc-123".to_string(),
            suggestion_type: "WorkGuidance".to_string(),
            source: "LLM_LOCAL".to_string(),
            content: "Take a break".to_string(),
            priority: "Medium".to_string(),
            confidence_score: 0.85,
            relevance_score: 0.9,
            is_actionable: true,
            reasoning: Some("High focus duration".to_string()),
            shown_at: None,
            dismissed_at: None,
            acted_at: None,
            created_at: "2026-03-18T10:00:00Z".to_string(),
            expires_at: None,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("LLM_LOCAL"));
        assert!(json.contains("Take a break"));
    }
}
