use axum::Json;
use oneshim_api_contracts::provider_specs::ProviderSurfaceCatalog;

use crate::error::ApiError;
use crate::services::ai_provider_spec_web_service::AiProviderSpecQueryService;

pub async fn list_provider_surfaces() -> Result<Json<ProviderSurfaceCatalog>, ApiError> {
    let response = AiProviderSpecQueryService::new().list_provider_surfaces()?;
    Ok(Json(response))
}
