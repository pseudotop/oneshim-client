use std::sync::LazyLock;

use oneshim_core::config::AiProviderType;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

const PRESETS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/config/ai_provider_presets.json"
));

static PRESET_CATALOG: LazyLock<Result<ProviderPresetCatalog, String>> = LazyLock::new(|| {
    serde_json::from_str::<ProviderPresetCatalog>(PRESETS_JSON)
        .map_err(|e| format!("Failed to parse ai provider preset catalog: {e}"))
});

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderPresetCatalog {
    pub version: u32,
    pub providers: Vec<ProviderPreset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderPreset {
    pub provider_type: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub display_name: String,
    pub llm_endpoint: String,
    pub ocr_endpoint: String,
    pub model_catalog_endpoint: String,
    #[serde(default = "default_true")]
    pub ocr_model_catalog_supported: bool,
    #[serde(default)]
    pub ocr_model_catalog_notice: Option<String>,
}

fn default_true() -> bool {
    true
}

pub fn list_provider_presets() -> Result<ProviderPresetCatalog, ApiError> {
    Ok(catalog()?.clone())
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

    // Backward-compatible fallback in case presets do not enumerate a known alias.
    parse_provider_type_name(&normalized)
        .ok_or_else(|| ApiError::BadRequest(format!("Unsupported provider_type: {raw}")))
}

pub fn default_model_catalog_endpoint(provider_type: AiProviderType) -> Option<String> {
    let label = provider_type_label(provider_type);
    find_provider_preset(label)
        .ok()
        .flatten()
        .map(|preset| preset.model_catalog_endpoint.clone())
}

pub fn ocr_model_catalog_notice_for_endpoint(
    provider_type: AiProviderType,
    endpoint: &str,
) -> Option<String> {
    let label = provider_type_label(provider_type);
    let preset = find_provider_preset(label).ok().flatten()?;
    if preset.ocr_model_catalog_supported {
        return None;
    }

    let ocr_host = extract_host(&preset.ocr_endpoint)?;
    if endpoint
        .to_ascii_lowercase()
        .contains(&ocr_host.to_ascii_lowercase())
    {
        return Some(preset.ocr_model_catalog_notice.clone().unwrap_or_else(|| {
            "This OCR endpoint does not expose a selectable model catalog.".to_string()
        }));
    }

    None
}

fn catalog() -> Result<&'static ProviderPresetCatalog, ApiError> {
    match &*PRESET_CATALOG {
        Ok(catalog) => Ok(catalog),
        Err(message) => Err(ApiError::Internal(message.clone())),
    }
}

fn find_provider_preset(provider_type_label: &str) -> Result<Option<ProviderPreset>, ApiError> {
    let catalog = catalog()?;
    Ok(catalog
        .providers
        .iter()
        .find(|provider| {
            provider
                .provider_type
                .eq_ignore_ascii_case(provider_type_label)
        })
        .cloned())
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
    fn presets_load_from_json() {
        let catalog = list_provider_presets().expect("provider preset catalog should load");
        assert!(catalog.providers.len() >= 4);
    }

    #[test]
    fn resolves_provider_alias() {
        let provider_type = resolve_provider_type("openai-compatible").expect("alias should parse");
        assert_eq!(provider_type, AiProviderType::OpenAi);
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

    #[test]
    fn returns_google_vision_notice_for_ocr_endpoint() {
        let notice = ocr_model_catalog_notice_for_endpoint(
            AiProviderType::Google,
            "https://vision.googleapis.com/v1/images:annotate",
        )
        .expect("google ocr notice should exist");

        assert!(notice.contains("does not expose"));
    }
}
