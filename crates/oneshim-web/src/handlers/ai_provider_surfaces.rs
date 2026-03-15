use axum::Json;
use oneshim_api_contracts::provider_surface_specs::ProviderSurfaceCatalog;

use crate::error::ApiError;
use crate::services::ai_provider_spec_service;

pub async fn list_provider_surfaces() -> Result<Json<ProviderSurfaceCatalog>, ApiError> {
    let response = ai_provider_spec_service::list_provider_surface_specs()?;
    Ok(Json(response))
}
