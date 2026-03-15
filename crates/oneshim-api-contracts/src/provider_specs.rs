use std::collections::HashSet;
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
    let catalog = serde_json::from_str::<ProviderSpecCatalog>(PROVIDER_SPECS_JSON)
        .map_err(|e| format!("Failed to parse provider spec catalog: {e}"))?;
    validate_spec_catalog(&catalog)?;
    Ok(catalog)
}

fn validate_spec_catalog(catalog: &ProviderSpecCatalog) -> Result<(), String> {
    if catalog.providers.is_empty() {
        return Err("Provider spec catalog must contain at least one provider.".to_string());
    }

    let mut provider_types = HashSet::new();
    let mut aliases = HashSet::new();

    for provider in &catalog.providers {
        let provider_key = provider.provider_type.trim().to_ascii_lowercase();
        if provider_key.is_empty() {
            return Err("Provider spec contains an empty provider_type.".to_string());
        }
        if !provider_types.insert(provider_key.clone()) {
            return Err(format!(
                "Provider spec catalog contains duplicate provider_type '{}'.",
                provider.provider_type
            ));
        }

        if provider.display_name.trim().is_empty() {
            return Err(format!(
                "Provider spec '{}' is missing a display_name.",
                provider.provider_type
            ));
        }
        if provider.references.is_empty() {
            return Err(format!(
                "Provider spec '{}' must include at least one reference URL.",
                provider.provider_type
            ));
        }

        for alias in &provider.aliases {
            let normalized = alias.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                return Err(format!(
                    "Provider spec '{}' contains an empty alias.",
                    provider.provider_type
                ));
            }
            if normalized == provider_key {
                continue;
            }
            if !aliases.insert(normalized.clone()) {
                return Err(format!(
                    "Provider spec alias '{}' is defined more than once.",
                    alias
                ));
            }
        }

        validate_transport_spec(
            &provider.provider_type,
            "llm",
            &provider.transports.llm.url,
            &provider.transports.llm.auth_scheme,
            Some(&provider.transports.llm.request_shape),
        )?;
        validate_transport_spec(
            &provider.provider_type,
            "ocr",
            &provider.transports.ocr.url,
            &provider.transports.ocr.auth_scheme,
            Some(&provider.transports.ocr.request_shape),
        )?;
        validate_transport_spec(
            &provider.provider_type,
            "model_catalog",
            &provider.transports.model_catalog.url,
            &provider.transports.model_catalog.auth_scheme,
            None,
        )?;

        if !provider.transports.model_catalog.ocr_supported
            && provider
                .transports
                .model_catalog
                .ocr_notice
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err(format!(
                "Provider spec '{}' must include an OCR notice when model catalog OCR is unsupported.",
                provider.provider_type
            ));
        }

        if provider.defaults.llm_models.is_empty() {
            return Err(format!(
                "Provider spec '{}' must define at least one default LLM model.",
                provider.provider_type
            ));
        }

        validate_parameter_profile(&provider.provider_type, "llm", &provider.parameters.llm)?;
        validate_parameter_profile(&provider.provider_type, "ocr", &provider.parameters.ocr)?;
    }

    Ok(())
}

fn validate_transport_spec(
    provider_type: &str,
    transport_name: &str,
    url: &str,
    auth_scheme: &str,
    request_shape: Option<&str>,
) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err(format!(
            "Provider spec '{}' transport '{}' is missing a URL.",
            provider_type, transport_name
        ));
    }
    if !url.trim().starts_with("https://") {
        return Err(format!(
            "Provider spec '{}' transport '{}' must use an https URL.",
            provider_type, transport_name
        ));
    }
    parse_auth_scheme(auth_scheme)?;
    if let Some(shape) = request_shape {
        parse_request_shape(shape)?;
    }
    Ok(())
}

fn validate_parameter_profile(
    provider_type: &str,
    profile_name: &str,
    profile: &crate::ai_providers::ProviderParameterProfile,
) -> Result<(), String> {
    let supported: HashSet<String> = profile
        .supported
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect();
    let unsupported: HashSet<String> = profile
        .unsupported
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect();

    if supported.iter().any(|value| value.is_empty())
        || unsupported.iter().any(|value| value.is_empty())
    {
        return Err(format!(
            "Provider spec '{}' profile '{}' contains an empty parameter entry.",
            provider_type, profile_name
        ));
    }

    if let Some(duplicate) = supported.intersection(&unsupported).next() {
        return Err(format!(
            "Provider spec '{}' profile '{}' lists '{}' as both supported and unsupported.",
            provider_type, profile_name, duplicate
        ));
    }

    Ok(())
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
    use crate::ai_providers::ProviderParameterProfile;

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

    #[test]
    fn rejects_duplicate_aliases() {
        let mut catalog = list_provider_specs().expect("provider specs should load");
        catalog.providers[1].aliases = vec!["shared".to_string()];
        catalog.providers[2].aliases = vec!["shared".to_string()];

        let err = validate_spec_catalog(&catalog).expect_err("duplicate aliases should fail");
        assert!(err.contains("defined more than once"));
    }

    #[test]
    fn rejects_overlapping_supported_and_unsupported_parameters() {
        let err = validate_parameter_profile(
            "OpenAi",
            "llm",
            &ProviderParameterProfile {
                supported: vec!["temperature".to_string()],
                unsupported: vec!["temperature".to_string()],
                notes: Vec::new(),
            },
        )
        .expect_err("overlapping parameters should fail");
        assert!(err.contains("both supported and unsupported"));
    }
}
