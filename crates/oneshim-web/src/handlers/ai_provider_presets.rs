use axum::Json;

use crate::error::ApiError;
use crate::services::ai_provider_preset_service;
use crate::services::ai_provider_preset_service::ProviderPresetCatalog;

pub async fn list_provider_presets() -> Result<Json<ProviderPresetCatalog>, ApiError> {
    let response = ai_provider_preset_service::list_provider_presets()?;
    Ok(Json(response))
}
