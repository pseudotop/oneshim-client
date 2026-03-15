use std::sync::OnceLock;

use oneshim_core::config::AiProviderType;

use crate::ai_providers::{ProviderSpec, ProviderSpecCatalog, ProviderTransportSpec};

const PROVIDER_SPECS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/providers/provider-catalog.v1.json"
));

static SPEC_CATALOG: OnceLock<Result<ProviderSpecCatalog, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTransportKind {
    Llm,
    Ocr,
    ModelCatalog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthScheme {
    Bearer,
    XApiKey,
    XGoogApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRequestShape {
    AnthropicMessages,
    AnthropicVisionMessages,
    OpenAiChatCompletions,
    OpenAiVisionChatCompletions,
    OpenAiResponses,
    GoogleGenerateContent,
    GoogleVisionAnnotate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogResponseShape {
    StandardDataOrModels,
    GoogleModels,
}

pub fn list_provider_specs() -> Result<ProviderSpecCatalog, String> {
    Ok(catalog()?.clone())
}

pub fn resolve_provider_type(raw: &str) -> Option<AiProviderType> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    for provider in catalog().ok()?.providers.iter() {
        let canonical = provider.provider_type.to_ascii_lowercase();
        if canonical == normalized
            || provider
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&normalized))
        {
            if let Some(parsed) = parse_provider_type_name(&provider.provider_type) {
                return Some(parsed);
            }
        }
    }

    parse_provider_type_name(&normalized)
}

pub fn provider_spec(provider_type: AiProviderType) -> Result<&'static ProviderSpec, String> {
    let label = provider_type_label(provider_type);
    catalog()?
        .providers
        .iter()
        .find(|provider| provider.provider_type.eq_ignore_ascii_case(label))
        .ok_or_else(|| format!("Provider spec for {label} is missing from the spec catalog."))
}

pub fn transport_spec(
    provider_type: AiProviderType,
    kind: ProviderTransportKind,
) -> Result<&'static ProviderTransportSpec, String> {
    let spec = provider_spec(provider_type)?;
    match kind {
        ProviderTransportKind::Llm => Ok(&spec.transports.llm),
        ProviderTransportKind::Ocr => Ok(&spec.transports.ocr),
        ProviderTransportKind::ModelCatalog => Err(
            "Model catalog transport uses a dedicated shape and must be resolved separately."
                .to_string(),
        ),
    }
}

pub fn auth_scheme(
    provider_type: AiProviderType,
    kind: ProviderTransportKind,
) -> Result<ProviderAuthScheme, String> {
    let raw = match kind {
        ProviderTransportKind::Llm | ProviderTransportKind::Ocr => {
            transport_spec(provider_type, kind)?.auth_scheme.as_str()
        }
        ProviderTransportKind::ModelCatalog => provider_spec(provider_type)?
            .transports
            .model_catalog
            .auth_scheme
            .as_str(),
    };
    parse_auth_scheme(raw)
}

pub fn request_shape(
    provider_type: AiProviderType,
    kind: ProviderTransportKind,
) -> Result<ProviderRequestShape, String> {
    parse_request_shape(&transport_spec(provider_type, kind)?.request_shape)
}

pub fn model_catalog_response_shape(
    provider_type: AiProviderType,
) -> Result<ModelCatalogResponseShape, String> {
    let raw = provider_spec(provider_type)?
        .transports
        .model_catalog
        .response_shape
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "standard_data_or_models" => Ok(ModelCatalogResponseShape::StandardDataOrModels),
        "google_models" => Ok(ModelCatalogResponseShape::GoogleModels),
        _ => Err(format!(
            "Unsupported model catalog response shape '{raw}' for {}",
            provider_type_label(provider_type)
        )),
    }
}

pub fn default_llm_model(provider_type: AiProviderType) -> Result<Option<String>, String> {
    Ok(provider_spec(provider_type)?
        .defaults
        .llm_models
        .first()
        .cloned())
}

pub fn default_ocr_model(provider_type: AiProviderType) -> Result<Option<String>, String> {
    Ok(provider_spec(provider_type)?
        .defaults
        .ocr_models
        .first()
        .cloned())
}

fn catalog() -> Result<&'static ProviderSpecCatalog, String> {
    match SPEC_CATALOG.get_or_init(load_spec_catalog) {
        Ok(catalog) => Ok(catalog),
        Err(message) => Err(message.clone()),
    }
}

fn load_spec_catalog() -> Result<ProviderSpecCatalog, String> {
    serde_json::from_str::<ProviderSpecCatalog>(PROVIDER_SPECS_JSON)
        .map_err(|e| format!("Failed to parse provider spec catalog: {e}"))
}

fn parse_auth_scheme(raw: &str) -> Result<ProviderAuthScheme, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bearer" => Ok(ProviderAuthScheme::Bearer),
        "x_api_key" => Ok(ProviderAuthScheme::XApiKey),
        "x_goog_api_key" => Ok(ProviderAuthScheme::XGoogApiKey),
        _ => Err(format!("Unsupported provider auth scheme '{raw}'")),
    }
}

fn parse_request_shape(raw: &str) -> Result<ProviderRequestShape, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "anthropic_messages" => Ok(ProviderRequestShape::AnthropicMessages),
        "anthropic_vision_messages" => Ok(ProviderRequestShape::AnthropicVisionMessages),
        "openai_chat_completions" => Ok(ProviderRequestShape::OpenAiChatCompletions),
        "openai_vision_chat_completions" => Ok(ProviderRequestShape::OpenAiVisionChatCompletions),
        "openai_responses" => Ok(ProviderRequestShape::OpenAiResponses),
        "google_generate_content" => Ok(ProviderRequestShape::GoogleGenerateContent),
        "google_vision_annotate" => Ok(ProviderRequestShape::GoogleVisionAnnotate),
        _ => Err(format!("Unsupported provider request shape '{raw}'")),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_provider_specs() {
        let catalog = list_provider_specs().expect("provider specs should load");
        assert!(catalog.providers.len() >= 6);
    }

    #[test]
    fn resolves_aliases() {
        assert_eq!(
            resolve_provider_type("openai-compatible"),
            Some(AiProviderType::OpenAi)
        );
        assert_eq!(
            resolve_provider_type("gemini"),
            Some(AiProviderType::Google)
        );
    }

    #[test]
    fn returns_openai_llm_shape() {
        let shape = request_shape(AiProviderType::OpenAi, ProviderTransportKind::Llm)
            .expect("llm shape should resolve");
        assert_eq!(shape, ProviderRequestShape::OpenAiChatCompletions);
    }

    #[test]
    fn returns_google_catalog_shape() {
        let shape = model_catalog_response_shape(AiProviderType::Google)
            .expect("catalog shape should resolve");
        assert_eq!(shape, ModelCatalogResponseShape::GoogleModels);
    }
}
