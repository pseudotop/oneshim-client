use axum::{extract::State, Json};

use crate::error::ApiError;
use crate::services::ai_model_catalog_service::{
    self, ProviderModelsRequest, ProviderModelsResponse,
};
use crate::AppState;

pub async fn discover_provider_models(
    State(_state): State<AppState>,
    Json(request): Json<ProviderModelsRequest>,
) -> Result<Json<ProviderModelsResponse>, ApiError> {
    let response = ai_model_catalog_service::fetch_provider_models(&request).await?;
    Ok(Json(response))
}
