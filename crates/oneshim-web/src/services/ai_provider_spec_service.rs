use oneshim_api_contracts::ai_providers::{ProviderPresetCatalog, ProviderSpecCatalog};
use oneshim_api_contracts::provider_specs;
use oneshim_core::config::AiProviderType;

use crate::error::ApiError;

pub use oneshim_api_contracts::provider_specs::ProviderSurfaceCatalog;
pub use oneshim_api_contracts::provider_specs::{ModelCatalogResponseShape, ProviderAuthScheme};

pub fn list_provider_specs() -> Result<ProviderSpecCatalog, ApiError> {
    provider_specs::list_provider_specs().map_err(ApiError::Internal)
}

pub fn list_provider_surface_specs() -> Result<ProviderSurfaceCatalog, ApiError> {
    provider_specs::list_provider_surface_specs().map_err(ApiError::Internal)
}

pub fn list_provider_presets() -> Result<ProviderPresetCatalog, ApiError> {
    provider_specs::list_compatibility_provider_presets().map_err(ApiError::Internal)
}

pub fn resolve_provider_type(raw: &str) -> Result<AiProviderType, ApiError> {
    provider_specs::resolve_provider_type(raw)
        .ok_or_else(|| ApiError::BadRequest(format!("Unsupported provider_type: {raw}")))
}

pub fn default_model_catalog_endpoint(provider_type: AiProviderType) -> Result<String, ApiError> {
    default_model_catalog_endpoint_for_surface(provider_type, None)
}

pub fn default_model_catalog_endpoint_for_surface(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<String, ApiError> {
    provider_specs::resolved_model_catalog_transport(provider_type, surface_id)
        .map(|transport| transport.url.clone())
        .map_err(ApiError::Internal)
}

pub fn model_catalog_supported_for_surface(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<bool, ApiError> {
    let surface = provider_specs::resolved_surface_spec(provider_type, surface_id)
        .map_err(ApiError::Internal)?;
    Ok(surface.supports.model_catalog && surface.model_catalog_transport.is_some())
}

pub fn ocr_model_catalog_notice_for_endpoint(
    provider_type: AiProviderType,
    endpoint: &str,
) -> Result<Option<String>, ApiError> {
    ocr_model_catalog_notice_for_surface(provider_type, None, endpoint)
}

pub fn ocr_model_catalog_notice_for_surface(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    endpoint: &str,
) -> Result<Option<String>, ApiError> {
    if surface_id.is_none() {
        let spec = provider_specs::provider_spec(provider_type).map_err(ApiError::Internal)?;
        if spec.transports.model_catalog.ocr_supported {
            return Ok(None);
        }

        let ocr_host = extract_host(&spec.transports.ocr.url).ok_or_else(|| {
            ApiError::Internal(format!(
                "Provider spec for {} has an invalid OCR endpoint host",
                spec.provider_type
            ))
        })?;
        if endpoint
            .to_ascii_lowercase()
            .contains(&ocr_host.to_ascii_lowercase())
        {
            return Ok(Some(
                spec.transports
                    .model_catalog
                    .ocr_notice
                    .clone()
                    .unwrap_or_else(|| {
                        "This OCR endpoint does not expose a selectable model catalog.".to_string()
                    }),
            ));
        }

        return Ok(None);
    }

    let surface = provider_specs::resolved_surface_spec(provider_type, surface_id)
        .map_err(ApiError::Internal)?;
    let Some(ocr_transport) = surface.ocr_transport.as_ref() else {
        return Ok(None);
    };
    let Some(model_catalog_transport) = surface.model_catalog_transport.as_ref() else {
        return Ok(None);
    };
    if model_catalog_transport.ocr_supported {
        return Ok(None);
    }

    let ocr_host = extract_host(&ocr_transport.url).ok_or_else(|| {
        ApiError::Internal(format!(
            "Provider surface '{}' has an invalid OCR endpoint host",
            surface.surface_id
        ))
    })?;
    if endpoint
        .to_ascii_lowercase()
        .contains(&ocr_host.to_ascii_lowercase())
    {
        return Ok(Some(
            model_catalog_transport
                .ocr_notice
                .clone()
                .unwrap_or_else(|| {
                    "This OCR endpoint does not expose a selectable model catalog.".to_string()
                }),
        ));
    }

    Ok(None)
}

pub fn model_catalog_response_shape(
    provider_type: AiProviderType,
) -> Result<ModelCatalogResponseShape, ApiError> {
    model_catalog_response_shape_for_surface(provider_type, None)
}

pub fn model_catalog_response_shape_for_surface(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<ModelCatalogResponseShape, ApiError> {
    provider_specs::resolved_model_catalog_response_shape(provider_type, surface_id)
        .map_err(ApiError::Internal)
}

pub fn model_catalog_auth_scheme(
    provider_type: AiProviderType,
) -> Result<ProviderAuthScheme, ApiError> {
    model_catalog_auth_scheme_for_surface(provider_type, None)
}

pub fn model_catalog_auth_scheme_for_surface(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<ProviderAuthScheme, ApiError> {
    provider_specs::resolved_auth_scheme(
        provider_type,
        surface_id,
        oneshim_api_contracts::provider_specs::ProviderTransportKind::ModelCatalog,
    )
    .map_err(ApiError::Internal)
}

fn extract_host(endpoint: &str) -> Option<String> {
    let (_, right) = endpoint.split_once("://")?;
    let host = right.split('/').next()?.trim();
    if host.is_empty() {
        return None;
    }
    Some(host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_load_from_json() {
        let catalog = list_provider_specs().expect("provider spec catalog should load");
        assert_eq!(catalog.providers.len(), 4);
        assert!(!catalog.updated_at.is_empty());
    }

    #[test]
    fn compatibility_presets_are_derived_from_specs() {
        let catalog = list_provider_presets().expect("provider preset catalog should load");
        let google = catalog
            .providers
            .iter()
            .find(|provider| provider.provider_type == "Google")
            .expect("google preset should exist");
        assert_eq!(
            google.model_catalog_endpoint,
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
        assert_eq!(
            google.llm_models.first().map(String::as_str),
            Some("gemini-2.5-flash")
        );
    }

    #[test]
    fn surface_catalog_loads_from_json() {
        let catalog = list_provider_surface_specs().expect("provider surface catalog should load");
        assert!(catalog.surfaces.len() >= 6);
    }

    #[test]
    fn resolves_provider_alias() {
        let provider_type = resolve_provider_type("openai-compatible").expect("alias should parse");
        assert_eq!(provider_type, AiProviderType::OpenAi);
    }

    #[test]
    fn returns_google_model_catalog_shape() {
        let shape =
            model_catalog_response_shape(AiProviderType::Google).expect("shape should resolve");
        assert_eq!(shape, ModelCatalogResponseShape::GoogleModels);
    }

    #[test]
    fn returns_google_ocr_notice() {
        let notice = ocr_model_catalog_notice_for_endpoint(
            AiProviderType::Google,
            "https://vision.googleapis.com/v1/images:annotate",
        )
        .expect("notice should resolve");
        assert!(notice.is_some());
    }

    #[test]
    fn managed_oauth_surface_reports_model_catalog_as_unsupported() {
        let supported = model_catalog_supported_for_surface(
            AiProviderType::OpenAi,
            Some("provider_surface.openai.managed_oauth"),
        )
        .expect("surface support should resolve");
        assert!(!supported);
    }
}
