use oneshim_api_contracts::ai_providers::ProviderPresetCatalog;
use oneshim_core::config::AiProviderType;

use crate::error::ApiError;
use crate::services::ai_provider_spec_service;

pub fn list_provider_presets() -> Result<ProviderPresetCatalog, ApiError> {
    ai_provider_spec_service::list_provider_presets()
}

pub fn resolve_provider_type(raw: &str) -> Result<AiProviderType, ApiError> {
    ai_provider_spec_service::resolve_provider_type(raw)
}

pub fn default_model_catalog_endpoint(provider_type: AiProviderType) -> Result<String, ApiError> {
    ai_provider_spec_service::default_model_catalog_endpoint(provider_type)
}

pub fn default_model_catalog_endpoint_for_surface(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<String, ApiError> {
    ai_provider_spec_service::default_model_catalog_endpoint_for_surface(provider_type, surface_id)
}

pub fn ocr_model_catalog_notice_for_endpoint(
    provider_type: AiProviderType,
    endpoint: &str,
) -> Result<Option<String>, ApiError> {
    ai_provider_spec_service::ocr_model_catalog_notice_for_endpoint(provider_type, endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_load_from_spec_catalog() {
        let catalog = list_provider_presets().expect("provider preset catalog should load");
        assert!(catalog.providers.len() >= 4);
        assert!(!catalog.updated_at.is_empty());
    }

    #[test]
    fn returns_default_catalog_endpoint() {
        let endpoint =
            default_model_catalog_endpoint(AiProviderType::Google).expect("google preset endpoint");
        assert_eq!(
            endpoint,
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
    }
}
