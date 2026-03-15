use std::sync::OnceLock;

use oneshim_api_contracts::ai_providers::{
    ProviderPreset, ProviderPresetCatalog, ProviderSpec, ProviderSpecCatalog,
};
use oneshim_core::config::AiProviderType;

use crate::error::ApiError;

const PROVIDER_SPECS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/providers/provider-catalog.v1.json"
));

static SPEC_CATALOG: OnceLock<Result<ProviderSpecCatalog, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogResponseShape {
    StandardDataOrModels,
    GoogleModels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthScheme {
    Bearer,
    XApiKey,
    XGoogApiKey,
}

pub fn list_provider_specs() -> Result<ProviderSpecCatalog, ApiError> {
    Ok(catalog()?.clone())
}

pub fn list_provider_presets() -> Result<ProviderPresetCatalog, ApiError> {
    let catalog = catalog()?;
    Ok(ProviderPresetCatalog {
        version: catalog.version,
        updated_at: catalog.updated_at.clone(),
        providers: catalog
            .providers
            .iter()
            .filter_map(compatibility_preset_from_spec)
            .collect(),
    })
}

pub fn resolve_provider_type(raw: &str) -> Result<AiProviderType, ApiError> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(ApiError::BadRequest(
            "provider_type is required for model discovery.".to_string(),
        ));
    }

    let catalog = catalog()?;
    for provider in &catalog.providers {
        let canonical = provider.provider_type.to_ascii_lowercase();
        if canonical == normalized
            || provider
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&normalized))
        {
            if let Some(parsed) = parse_provider_type_name(&provider.provider_type) {
                return Ok(parsed);
            }
        }
    }

    parse_provider_type_name(&normalized)
        .ok_or_else(|| ApiError::BadRequest(format!("Unsupported provider_type: {raw}")))
}

pub fn default_model_catalog_endpoint(provider_type: AiProviderType) -> Result<String, ApiError> {
    Ok(spec_for_provider_type(provider_type)?
        .transports
        .model_catalog
        .url
        .clone())
}

pub fn ocr_model_catalog_notice_for_endpoint(
    provider_type: AiProviderType,
    endpoint: &str,
) -> Result<Option<String>, ApiError> {
    let spec = spec_for_provider_type(provider_type)?;
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

    Ok(None)
}

pub fn model_catalog_response_shape(
    provider_type: AiProviderType,
) -> Result<ModelCatalogResponseShape, ApiError> {
    let raw = spec_for_provider_type(provider_type)?
        .transports
        .model_catalog
        .response_shape
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "standard_data_or_models" => Ok(ModelCatalogResponseShape::StandardDataOrModels),
        "google_models" => Ok(ModelCatalogResponseShape::GoogleModels),
        _ => Err(ApiError::Internal(format!(
            "Unsupported model catalog response shape '{raw}' for {:?}",
            provider_type
        ))),
    }
}

pub fn model_catalog_auth_scheme(
    provider_type: AiProviderType,
) -> Result<ProviderAuthScheme, ApiError> {
    let raw = spec_for_provider_type(provider_type)?
        .transports
        .model_catalog
        .auth_scheme
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "bearer" => Ok(ProviderAuthScheme::Bearer),
        "x_api_key" => Ok(ProviderAuthScheme::XApiKey),
        "x_goog_api_key" => Ok(ProviderAuthScheme::XGoogApiKey),
        _ => Err(ApiError::Internal(format!(
            "Unsupported provider auth scheme '{raw}' for {:?}",
            provider_type
        ))),
    }
}

fn catalog() -> Result<&'static ProviderSpecCatalog, ApiError> {
    match SPEC_CATALOG.get_or_init(load_spec_catalog) {
        Ok(catalog) => Ok(catalog),
        Err(message) => Err(ApiError::Internal(message.clone())),
    }
}

fn load_spec_catalog() -> Result<ProviderSpecCatalog, String> {
    serde_json::from_str::<ProviderSpecCatalog>(PROVIDER_SPECS_JSON)
        .map_err(|e| format!("Failed to parse provider spec catalog: {e}"))
}

fn compatibility_preset_from_spec(spec: &ProviderSpec) -> Option<ProviderPreset> {
    parse_provider_type_name(&spec.provider_type)?;

    Some(ProviderPreset {
        provider_type: spec.provider_type.clone(),
        aliases: spec.aliases.clone(),
        display_name: spec.display_name.clone(),
        llm_endpoint: spec.transports.llm.url.clone(),
        ocr_endpoint: spec.transports.ocr.url.clone(),
        model_catalog_endpoint: spec.transports.model_catalog.url.clone(),
        ocr_model_catalog_supported: spec.transports.model_catalog.ocr_supported,
        ocr_model_catalog_notice: spec.transports.model_catalog.ocr_notice.clone(),
        llm_models: spec.defaults.llm_models.clone(),
        ocr_models: spec.defaults.ocr_models.clone(),
    })
}

fn spec_for_provider_type(
    provider_type: AiProviderType,
) -> Result<&'static ProviderSpec, ApiError> {
    let label = provider_type_label(provider_type);
    catalog()?
        .providers
        .iter()
        .find(|provider| provider.provider_type.eq_ignore_ascii_case(label))
        .ok_or_else(|| {
            ApiError::Internal(format!(
                "Provider spec for {} is missing from the spec catalog.",
                label
            ))
        })
}

fn provider_type_label(provider_type: AiProviderType) -> &'static str {
    match provider_type {
        AiProviderType::Anthropic => "Anthropic",
        AiProviderType::OpenAi => "OpenAi",
        AiProviderType::Google => "Google",
        AiProviderType::Generic => "Generic",
    }
}

fn parse_provider_type_name(raw: &str) -> Option<AiProviderType> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Some(AiProviderType::Anthropic),
        "openai" | "open_ai" | "open-ai" | "openai-compatible" => Some(AiProviderType::OpenAi),
        "google" | "gemini" => Some(AiProviderType::Google),
        "generic" => Some(AiProviderType::Generic),
        _ => None,
    }
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
        assert!(catalog.providers.len() >= 6);
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
    }

    #[test]
    fn resolves_provider_alias() {
        let provider_type = resolve_provider_type("openai-compatible").expect("alias should parse");
        assert_eq!(provider_type, AiProviderType::OpenAi);
    }

    #[test]
    fn returns_google_model_catalog_shape() {
        let shape = model_catalog_response_shape(AiProviderType::Google)
            .expect("google response shape should resolve");
        assert_eq!(shape, ModelCatalogResponseShape::GoogleModels);
    }

    #[test]
    fn returns_google_ocr_notice() {
        let notice = ocr_model_catalog_notice_for_endpoint(
            AiProviderType::Google,
            "https://vision.googleapis.com/v1/images:annotate",
        )
        .expect("google ocr notice should resolve")
        .expect("google ocr notice should exist");
        assert!(notice.contains("does not expose"));
    }
}
