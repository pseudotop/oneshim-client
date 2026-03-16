use std::time::Duration;

use oneshim_api_contracts::ai_providers::{
    ProviderDiscoveredModel, ProviderModelSupportStatus, ProviderModelsRequest,
    ProviderModelsResponse,
};
use oneshim_api_contracts::provider_specs::{
    default_surface_id_for_access_mode as default_surface_id_from_catalog,
    model_capability_status_for_surface, resolved_model_catalog_strategy, resolved_surface_spec,
    ModelCatalogStrategy, ProviderSurfaceSpec, SurfaceCapabilityKind, SurfaceModelCapabilityKind,
};
use oneshim_core::config::{AiProviderConfig, AiProviderType, ExternalApiEndpoint};
use oneshim_core::ports::credential_source::CredentialSource;
use serde_json::Value;

use crate::error::ApiError;
use crate::services::ai_provider_spec_service::{
    self, ModelCatalogResponseShape, ProviderAuthScheme,
};
use crate::services::settings_service::is_masked_key;
use crate::AppState;

const MODEL_DISCOVERY_TIMEOUT_SECS: u64 = 20;
const MAX_ERROR_SNIPPET_CHARS: usize = 220;

pub async fn fetch_provider_models(
    request: &ProviderModelsRequest,
    state: &AppState,
) -> Result<ProviderModelsResponse, ApiError> {
    let requested_surface_id = normalize_optional_surface_id(request.surface_id.as_deref());
    let provider_type = resolve_requested_provider_type(
        request.provider_type.as_str(),
        requested_surface_id.as_deref(),
    )?;
    let endpoint = resolve_models_endpoint(
        provider_type,
        requested_surface_id.as_deref(),
        request.endpoint.as_deref(),
    )?;
    let auth_scheme = ai_provider_spec_service::model_catalog_auth_scheme_for_surface(
        provider_type,
        requested_surface_id.as_deref(),
    )?;
    let api_key = if matches!(auth_scheme, ProviderAuthScheme::None) {
        None
    } else {
        Some(resolve_model_discovery_api_key(request, state, provider_type).await?)
    };
    if let Some(notice) = ai_provider_spec_service::ocr_model_catalog_notice_for_surface(
        provider_type,
        requested_surface_id.as_deref(),
        &endpoint,
    )? {
        return Ok(ProviderModelsResponse {
            models: Vec::new(),
            model_details: Vec::new(),
            notice: Some(notice),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(MODEL_DISCOVERY_TIMEOUT_SECS))
        .build()
        .map_err(|e| ApiError::Internal(format!("Failed to create model discovery client: {e}")))?;

    let mut builder = client.get(&endpoint);
    match auth_scheme {
        ProviderAuthScheme::None => {}
        ProviderAuthScheme::Bearer => {
            let api_key = api_key.as_deref().unwrap_or_default();
            builder = builder.header("Authorization", format!("Bearer {api_key}"));
        }
        ProviderAuthScheme::XApiKey => {
            let api_key = api_key.as_deref().unwrap_or_default();
            builder = builder
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
        }
        ProviderAuthScheme::XGoogApiKey => {
            let api_key = api_key.as_deref().unwrap_or_default();
            builder = builder.header("x-goog-api-key", api_key);
        }
    }

    let response = builder.send().await.map_err(|e| {
        ApiError::ServiceUnavailable(format!("Model discovery request failed: {e}"))
    })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        ApiError::ServiceUnavailable(format!("Failed to read model discovery response: {e}"))
    })?;
    if !status.is_success() {
        return Err(ApiError::ServiceUnavailable(format!(
            "Model discovery failed ({}): {}",
            status,
            truncate_error(&body)
        )));
    }

    let mut discovered_models = parse_models(
        ai_provider_spec_service::model_catalog_response_shape_for_surface(
            provider_type,
            requested_surface_id.as_deref(),
        )?,
        &body,
    )?;
    discovered_models.sort_by(|left, right| left.id.cmp(&right.id));
    discovered_models.dedup_by(|left, right| left.id == right.id);
    let model_details = build_model_details(
        provider_type,
        requested_surface_id.as_deref(),
        &discovered_models,
    )?;
    let models = discovered_models
        .iter()
        .map(|model| model.id.clone())
        .collect::<Vec<_>>();

    Ok(ProviderModelsResponse {
        model_details,
        notice: if models.is_empty() {
            Some("Provider returned no models for this configuration.".to_string())
        } else {
            None
        },
        models,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedModelRecord {
    id: String,
    display_name: Option<String>,
}

fn parse_models(
    shape: ModelCatalogResponseShape,
    body: &str,
) -> Result<Vec<ParsedModelRecord>, ApiError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| ApiError::BadRequest(format!("Invalid model catalog response JSON: {e}")))?;

    match shape {
        ModelCatalogResponseShape::GoogleModels => parse_google_models(&value),
        ModelCatalogResponseShape::StandardDataOrModels => parse_standard_models(&value),
    }
}

fn parse_google_models(value: &Value) -> Result<Vec<ParsedModelRecord>, ApiError> {
    let Some(entries) = value.get("models").and_then(|m| m.as_array()) else {
        return Err(ApiError::BadRequest(
            "Google model catalog response missing `models`.".to_string(),
        ));
    };

    let mut generation_models = Vec::new();
    let mut fallback_models = Vec::new();
    for entry in entries {
        let raw_name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| entry.get("displayName").and_then(|v| v.as_str()))
            .unwrap_or("")
            .trim();
        if raw_name.is_empty() {
            continue;
        }
        let normalized = raw_name
            .strip_prefix("models/")
            .unwrap_or(raw_name)
            .to_string();
        let display_name = entry
            .get("displayName")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let record = ParsedModelRecord {
            id: normalized.clone(),
            display_name,
        };
        fallback_models.push(record.clone());

        let supports_generation = entry
            .get("supportedGenerationMethods")
            .and_then(|v| v.as_array())
            .map(|methods| {
                methods
                    .iter()
                    .filter_map(|m| m.as_str())
                    .any(|method| method.eq_ignore_ascii_case("generateContent"))
            })
            .unwrap_or(false);
        if supports_generation {
            generation_models.push(record);
        }
    }

    if !generation_models.is_empty() {
        return Ok(generation_models);
    }
    Ok(fallback_models)
}

fn parse_standard_models(value: &Value) -> Result<Vec<ParsedModelRecord>, ApiError> {
    let entries = value
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| value.get("models").and_then(|d| d.as_array()))
        .ok_or_else(|| {
            ApiError::BadRequest(
                "Model catalog response missing `data` (or `models`) array.".to_string(),
            )
        })?;

    let models = entries
        .iter()
        .filter_map(|entry| {
            let id = entry
                .get("id")
                .and_then(|v| v.as_str())
                .or_else(|| entry.get("name").and_then(|v| v.as_str()))
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)?;
            let display_name = entry
                .get("display_name")
                .and_then(|v| v.as_str())
                .or_else(|| entry.get("displayName").and_then(|v| v.as_str()))
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string);
            Some(ParsedModelRecord { id, display_name })
        })
        .collect::<Vec<_>>();

    Ok(models)
}

fn build_model_details(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    models: &[ParsedModelRecord],
) -> Result<Vec<ProviderDiscoveredModel>, ApiError> {
    let Some(surface_id) = surface_id else {
        return Ok(Vec::new());
    };

    models
        .iter()
        .map(|model| {
            let known = oneshim_api_contracts::provider_specs::known_model_spec_for_surface(
                surface_id, &model.id,
            )
            .map_err(ApiError::Internal)?;
            let llm_support = Some(
                model_capability_status_for_surface(
                    surface_id,
                    SurfaceModelCapabilityKind::Llm,
                    &model.id,
                )
                .map_err(ApiError::Internal)?,
            );
            let ocr_support = Some(
                model_capability_status_for_surface(
                    surface_id,
                    SurfaceModelCapabilityKind::Ocr,
                    &model.id,
                )
                .map_err(ApiError::Internal)?,
            );
            let image_input_support = {
                let resolved = model_capability_status_for_surface(
                    surface_id,
                    SurfaceModelCapabilityKind::ImageInput,
                    &model.id,
                )
                .map_err(ApiError::Internal)?;
                Some(
                    if provider_type == AiProviderType::Google
                        && resolved == ProviderModelSupportStatus::Unknown
                        && llm_support == Some(ProviderModelSupportStatus::Supported)
                    {
                        ProviderModelSupportStatus::Supported
                    } else {
                        resolved
                    },
                )
            };
            let structured_output_support = Some(
                model_capability_status_for_surface(
                    surface_id,
                    SurfaceModelCapabilityKind::StructuredOutput,
                    &model.id,
                )
                .map_err(ApiError::Internal)?,
            );
            let capability_source = if known.is_some() {
                Some("known_model_catalog".to_string())
            } else if [
                llm_support,
                ocr_support,
                image_input_support,
                structured_output_support,
            ]
            .into_iter()
            .flatten()
            .any(|status| status != ProviderModelSupportStatus::Unknown)
            {
                Some("capability_rules".to_string())
            } else {
                Some("surface_unknown".to_string())
            };

            Ok(ProviderDiscoveredModel {
                id: model.id.clone(),
                display_name: model.display_name.clone(),
                llm_support,
                supports_ocr: ocr_support
                    .map(|status| status == ProviderModelSupportStatus::Supported),
                ocr_support,
                image_input_support,
                structured_output_support,
                capability_source,
            })
        })
        .collect()
}

async fn resolve_model_discovery_api_key(
    request: &ProviderModelsRequest,
    state: &AppState,
    provider_type: AiProviderType,
) -> Result<String, ApiError> {
    let api_key = request.api_key.trim();
    if !api_key.is_empty() && !is_masked_key(api_key) {
        return Ok(api_key.to_string());
    }

    if request.use_saved_secret {
        if let Some(saved) =
            resolve_saved_model_discovery_api_key(request, state, provider_type).await?
        {
            return Ok(saved);
        }
    }

    Err(ApiError::BadRequest(
        "A full API key is required to fetch model catalog.".to_string(),
    ))
}

async fn resolve_saved_model_discovery_api_key(
    request: &ProviderModelsRequest,
    state: &AppState,
    provider_type: AiProviderType,
) -> Result<Option<String>, ApiError> {
    let Some(config_manager) = state.config_manager.as_ref() else {
        return Ok(None);
    };
    let Some(surface) = parse_model_surface(request.surface.as_deref()) else {
        return Ok(None);
    };

    let saved_config = config_manager.get();
    let Some(saved_endpoint) = endpoint_for_surface(&saved_config.ai_provider, surface) else {
        return Ok(None);
    };

    if saved_endpoint.provider_type != provider_type {
        return Ok(None);
    }

    if let Some(request_surface_id) = normalize_optional_surface_id(request.surface_id.as_deref()) {
        let saved_surface_id = saved_endpoint_surface_id(
            &saved_config.ai_provider,
            saved_endpoint,
            request.surface.as_deref(),
        );
        if saved_surface_id.as_deref() != Some(request_surface_id.as_str()) {
            return Ok(None);
        }
    }

    if let Some(request_endpoint) = request.endpoint.as_deref() {
        let request_endpoint = normalize_optional_endpoint(request_endpoint);
        let saved_endpoint_normalized = normalize_optional_endpoint(&saved_endpoint.endpoint);
        if request_endpoint != saved_endpoint_normalized {
            return Ok(None);
        }
    }

    if let Ok(source) = CredentialSource::from_api_key_endpoint_for_profile(
        saved_endpoint,
        Some(surface.profile_id()),
        state
            .secret_stores
            .as_ref()
            .and_then(|stores| stores.for_binding(saved_endpoint.credential.as_ref()))
            .or_else(|| state.secret_store.clone()),
    ) {
        if let Ok(secret) = source.resolve_bearer_token().await {
            if !secret.trim().is_empty() {
                return Ok(Some(secret));
            }
        }
    }

    Ok(None)
}

#[derive(Clone, Copy)]
enum ModelSurface {
    Ocr,
    Llm,
}

impl ModelSurface {
    fn profile_id(self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Llm => "llm",
        }
    }
}

fn parse_model_surface(value: Option<&str>) -> Option<ModelSurface> {
    match value?.trim() {
        "ocr_api" => Some(ModelSurface::Ocr),
        "llm_api" => Some(ModelSurface::Llm),
        _ => None,
    }
}

fn endpoint_for_surface(
    config: &AiProviderConfig,
    surface: ModelSurface,
) -> Option<&ExternalApiEndpoint> {
    match surface {
        ModelSurface::Ocr => config.ocr_api.as_ref(),
        ModelSurface::Llm => config.llm_api.as_ref(),
    }
}

fn resolve_models_endpoint(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    endpoint: Option<&str>,
) -> Result<String, ApiError> {
    let endpoint = endpoint.and_then(normalize_optional_endpoint);
    let surface = resolved_surface_spec(provider_type, surface_id).map_err(ApiError::Internal)?;
    if !surface.supports.model_catalog || surface.model_catalog_transport.is_none() {
        return Err(ApiError::BadRequest(format!(
            "Selected provider surface '{}' does not support model discovery.",
            surface.surface_id
        )));
    }
    let catalog_strategy =
        resolved_model_catalog_strategy(provider_type, Some(surface.surface_id.as_str()))
            .map_err(ApiError::Internal)?;

    let default_endpoint = ai_provider_spec_service::default_model_catalog_endpoint_for_surface(
        provider_type,
        Some(surface.surface_id.as_str()),
    )?;

    if let Some(endpoint) = endpoint {
        return match catalog_strategy {
            ModelCatalogStrategy::HttpModelsEndpoint => {
                if let Some(derived) =
                    derive_model_catalog_endpoint_from_surface(surface, &endpoint)
                {
                    Ok(derived)
                } else {
                    Err(ApiError::BadRequest(format!(
                        "Could not derive a model catalog endpoint from '{}' for surface '{}'.",
                        endpoint, surface.surface_id
                    )))
                }
            }
            ModelCatalogStrategy::None | ModelCatalogStrategy::SubprocessProbe => {
                Err(ApiError::BadRequest(format!(
                    "Surface '{}' does not support HTTP model discovery from a custom endpoint.",
                    surface.surface_id
                )))
            }
        };
    }

    Ok(default_endpoint)
}

fn resolve_requested_provider_type(
    raw_provider_type: &str,
    surface_id: Option<&str>,
) -> Result<AiProviderType, ApiError> {
    if let Some(surface_id) = surface_id {
        let surface = oneshim_api_contracts::provider_specs::provider_surface_spec(surface_id)
            .map_err(ApiError::BadRequest)?;
        return ai_provider_spec_service::resolve_provider_type(&surface.provider_type);
    }

    ai_provider_spec_service::resolve_provider_type(raw_provider_type)
}

fn saved_endpoint_surface_id(
    config: &AiProviderConfig,
    endpoint: &ExternalApiEndpoint,
    requested_surface_kind: Option<&str>,
) -> Option<String> {
    endpoint
        .surface_id
        .as_deref()
        .and_then(|value| normalize_optional_surface_id(Some(value)))
        .or_else(|| {
            default_surface_id_from_catalog(
                endpoint.provider_type,
                config.access_mode,
                match requested_surface_kind
                    .map(str::trim)
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "ocr" | "ocr_api" => SurfaceCapabilityKind::Ocr,
                    _ => SurfaceCapabilityKind::Llm,
                },
            )
            .ok()
            .flatten()
            .map(|value| value.to_ascii_lowercase())
        })
}

fn normalize_optional_surface_id(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

fn normalize_optional_endpoint(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.trim_end_matches('/').to_string())
}

fn derive_model_catalog_endpoint_from_surface(
    surface: &ProviderSurfaceSpec,
    endpoint: &str,
) -> Option<String> {
    let normalized_endpoint = normalize_optional_endpoint(endpoint)?;
    let configured = reqwest::Url::parse(&normalized_endpoint).ok()?;
    let catalog_transport = surface.model_catalog_transport.as_ref()?;
    let catalog_url = reqwest::Url::parse(&catalog_transport.url).ok()?;

    if configured.path() == catalog_url.path() {
        return Some(normalized_endpoint);
    }

    let candidate_transports = [
        surface
            .llm_transport
            .as_ref()
            .map(|transport| transport.url.as_str()),
        surface
            .ocr_transport
            .as_ref()
            .map(|transport| transport.url.as_str()),
    ];

    for candidate in candidate_transports.into_iter().flatten() {
        let default_transport = reqwest::Url::parse(candidate).ok()?;
        if let Some(derived) = derive_model_catalog_endpoint_from_transport(
            &configured,
            &default_transport,
            &catalog_url,
        ) {
            return Some(derived);
        }
    }

    if configured.path().is_empty() || configured.path() == "/" {
        return Some(rebased_url(&configured, &catalog_url));
    }

    if same_origin(&configured, &catalog_url) {
        return Some(rebased_url(&configured, &catalog_url));
    }

    None
}

fn derive_model_catalog_endpoint_from_transport(
    configured: &reqwest::Url,
    default_transport: &reqwest::Url,
    catalog_url: &reqwest::Url,
) -> Option<String> {
    let configured_path = configured.path();
    let default_transport_path = default_transport.path();

    if configured_path.ends_with(default_transport_path) {
        let prefix_len = configured_path
            .len()
            .saturating_sub(default_transport_path.len());
        let derived_path = format!("{}{}", &configured_path[..prefix_len], catalog_url.path());
        return Some(rebased_url_with_path(
            configured,
            &derived_path,
            catalog_url,
        ));
    }

    if path_is_prefix_of(configured_path, default_transport_path) {
        return Some(rebased_url(configured, catalog_url));
    }

    None
}

fn rebased_url(base: &reqwest::Url, catalog_url: &reqwest::Url) -> String {
    rebased_url_with_path(base, catalog_url.path(), catalog_url)
}

fn rebased_url_with_path(base: &reqwest::Url, path: &str, catalog_url: &reqwest::Url) -> String {
    let mut resolved = base.clone();
    resolved.set_path(path);
    resolved.set_query(catalog_url.query());
    resolved.set_fragment(None);
    resolved.to_string()
}

fn path_is_prefix_of(prefix_path: &str, full_path: &str) -> bool {
    let prefix = prefix_path.trim_end_matches('/');
    let full = full_path.trim_end_matches('/');
    if prefix.is_empty() || prefix == "/" {
        return true;
    }
    full == prefix || full.starts_with(&format!("{prefix}/"))
}

fn same_origin(left: &reqwest::Url, right: &reqwest::Url) -> bool {
    left.scheme().eq_ignore_ascii_case(right.scheme())
        && left
            .host_str()
            .zip(right.host_str())
            .is_some_and(|(l, r)| l.eq_ignore_ascii_case(r))
        && left.port_or_known_default() == right.port_or_known_default()
}

fn truncate_error(raw: &str) -> String {
    let compact = raw.replace(['\n', '\r'], " ");
    let compact = compact.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(MAX_ERROR_SNIPPET_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use async_trait::async_trait;
    use oneshim_core::config::{
        AiProviderConfig, AppConfig, CredentialAuthMode, CredentialBackendKind, CredentialBinding,
        ExternalApiEndpoint, SecretRef,
    };
    use oneshim_core::config_manager::ConfigManager;
    use oneshim_core::error::CoreError;
    use oneshim_core::ports::secret_store::SecretStore;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    struct TestSecretStore {
        values: Mutex<HashMap<(String, String), String>>,
    }

    impl TestSecretStore {
        fn new() -> Self {
            Self {
                values: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl SecretStore for TestSecretStore {
        async fn store(&self, namespace: &str, key: &str, value: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .insert((namespace.to_string(), key.to_string()), value.to_string());
            Ok(())
        }

        async fn retrieve(&self, namespace: &str, key: &str) -> Result<Option<String>, CoreError> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .get(&(namespace.to_string(), key.to_string()))
                .cloned())
        }

        async fn delete(&self, namespace: &str, key: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .remove(&(namespace.to_string(), key.to_string()));
            Ok(())
        }

        async fn delete_namespace(&self, namespace: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .retain(|(existing_namespace, _), _| existing_namespace != namespace);
            Ok(())
        }
    }

    fn test_state_with_saved_secret(
        config: AppConfig,
        secret_store: Arc<dyn SecretStore>,
    ) -> AppState {
        let temp_dir = TempDir::new().expect("temp dir");
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).expect("config manager");
        config_manager.update(config).expect("save config");
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: Some(config_manager),
            default_secret_backend_kind: oneshim_core::config::CredentialBackendKind::Unavailable,
            secret_store: Some(secret_store),
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            integration_runtime_status: None,
            integration_auth: None,
            integration_session: None,
            integration_outbox: None,
            integration_inbox: None,
            integration_inbox_store: None,
            integration_audit: None,
            update_control: None,
        }
    }

    #[test]
    fn derives_google_models_endpoint_from_generate_content_url() {
        let surface = resolved_surface_spec(
            AiProviderType::Google,
            Some("provider_surface.google.direct_api"),
        )
        .expect("google surface should resolve");
        let endpoint =
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";
        let derived = derive_model_catalog_endpoint_from_surface(surface, endpoint).unwrap();
        assert_eq!(
            derived,
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
    }

    #[test]
    fn derives_openai_models_endpoint_from_chat_completions_url() {
        let surface = resolved_surface_spec(
            AiProviderType::OpenAi,
            Some("provider_surface.openai.direct_api"),
        )
        .expect("openai surface should resolve");
        let endpoint = "https://api.openai.com/v1/chat/completions";
        let derived = derive_model_catalog_endpoint_from_surface(surface, endpoint).unwrap();
        assert_eq!(derived, "https://api.openai.com/v1/models");
    }

    #[test]
    fn derives_openai_models_endpoint_from_responses_url() {
        let surface = resolved_surface_spec(
            AiProviderType::OpenAi,
            Some("provider_surface.openai.direct_api"),
        )
        .expect("openai surface should resolve");
        let endpoint = "https://api.openai.com/v1/responses";
        let derived = derive_model_catalog_endpoint_from_surface(surface, endpoint).unwrap();
        assert_eq!(derived, "https://api.openai.com/v1/models");
    }

    #[test]
    fn derives_ollama_models_endpoint_from_responses_url() {
        let surface = resolved_surface_spec(
            AiProviderType::Ollama,
            Some("provider_surface.ollama.local_http"),
        )
        .expect("ollama surface should resolve");
        let endpoint = "http://localhost:11434/v1/responses";
        let derived = derive_model_catalog_endpoint_from_surface(surface, endpoint).unwrap();
        assert_eq!(derived, "http://localhost:11434/api/tags");
    }

    #[test]
    fn derives_generic_local_openai_compatible_models_endpoint_from_v1_base() {
        let surface = resolved_surface_spec(
            AiProviderType::Generic,
            Some("provider_surface.generic.local_openai_compatible"),
        )
        .expect("generic local openai-compatible surface should resolve");
        let endpoint = "http://127.0.0.1:1234/v1";
        let derived = derive_model_catalog_endpoint_from_surface(surface, endpoint).unwrap();
        assert_eq!(derived, "http://127.0.0.1:1234/v1/models");
    }

    #[test]
    fn parses_google_model_catalog() {
        let body = r#"{
          "models": [
            {
              "name": "models/gemini-2.5-flash",
              "displayName": "Gemini 2.5 Flash",
              "supportedGenerationMethods": ["generateContent"]
            },
            {
              "name": "models/text-embedding-004",
              "supportedGenerationMethods": ["embedContent"]
            }
          ]
        }"#;
        let value: Value = serde_json::from_str(body).unwrap();
        let parsed = parse_google_models(&value).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "gemini-2.5-flash");
        assert_eq!(parsed[0].display_name.as_deref(), Some("Gemini 2.5 Flash"));
    }

    #[test]
    fn parses_standard_model_catalog() {
        let body = r#"{
          "data": [
            {"id": "gpt-5.4"},
            {"id": "gpt-5.2"}
          ]
        }"#;
        let value: Value = serde_json::from_str(body).unwrap();
        let parsed = parse_standard_models(&value).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "gpt-5.4");
        assert_eq!(parsed[1].id, "gpt-5.2");
    }

    #[test]
    fn builds_model_details_from_known_surface_models() {
        let details = build_model_details(
            AiProviderType::OpenAi,
            Some("provider_surface.openai.direct_api"),
            &[ParsedModelRecord {
                id: "text-embedding-3-small".to_string(),
                display_name: Some("Text Embedding 3 Small".to_string()),
            }],
        )
        .expect("model details should build");
        assert_eq!(details.len(), 1);
        assert_eq!(
            details[0].llm_support,
            Some(ProviderModelSupportStatus::Unsupported)
        );
        assert_eq!(
            details[0].ocr_support,
            Some(ProviderModelSupportStatus::Unsupported)
        );
        assert_eq!(details[0].supports_ocr, Some(false));
        assert_eq!(
            details[0].image_input_support,
            Some(ProviderModelSupportStatus::Unsupported)
        );
        assert_eq!(
            details[0].structured_output_support,
            Some(ProviderModelSupportStatus::Unsupported)
        );
    }

    #[test]
    fn builds_google_image_input_support_from_known_models() {
        let details = build_model_details(
            AiProviderType::Google,
            Some("provider_surface.google.direct_api"),
            &[ParsedModelRecord {
                id: "gemini-2.5-flash".to_string(),
                display_name: Some("Gemini 2.5 Flash".to_string()),
            }],
        )
        .expect("google model details should build");
        assert_eq!(details.len(), 1);
        assert_eq!(
            details[0].image_input_support,
            Some(ProviderModelSupportStatus::Supported)
        );
        assert_eq!(
            details[0].ocr_support,
            Some(ProviderModelSupportStatus::Unsupported)
        );
    }

    #[test]
    fn builds_capability_rule_details_for_local_openai_compatible_models() {
        let details = build_model_details(
            AiProviderType::Generic,
            Some("provider_surface.generic.local_openai_compatible"),
            &[ParsedModelRecord {
                id: "qwen2.5-vl-7b-instruct".to_string(),
                display_name: Some("Qwen 2.5 VL 7B".to_string()),
            }],
        )
        .expect("local openai-compatible model details should build");
        assert_eq!(details.len(), 1);
        assert_eq!(
            details[0].ocr_support,
            Some(ProviderModelSupportStatus::Supported)
        );
        assert_eq!(
            details[0].image_input_support,
            Some(ProviderModelSupportStatus::Supported)
        );
        assert_eq!(
            details[0].capability_source.as_deref(),
            Some("capability_rules")
        );
    }

    #[tokio::test]
    async fn resolve_model_discovery_api_key_uses_saved_secret_binding() {
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        secret_store
            .store("provider/openai/llm", "api_key", "sk-saved")
            .await
            .unwrap();

        let mut config = AppConfig::default_config();
        config.ai_provider = AiProviderConfig {
            llm_api: Some(ExternalApiEndpoint {
                endpoint: "https://api.openai.com/v1".to_string(),
                api_key: String::new(),
                model: Some("gpt-5.4".to_string()),
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
                surface_id: None,
                credential: Some(CredentialBinding {
                    auth_mode: CredentialAuthMode::ApiKey,
                    backend_kind: CredentialBackendKind::OsSecretStore,
                    secret_ref: Some(SecretRef {
                        namespace: "provider/openai/llm".to_string(),
                        key: "api_key".to_string(),
                    }),
                    projection_enabled: false,
                }),
            }),
            ..AiProviderConfig::default()
        };
        let state = test_state_with_saved_secret(config, secret_store);
        let request = ProviderModelsRequest {
            provider_type: "OpenAi".to_string(),
            api_key: String::new(),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            surface: Some("llm_api".to_string()),
            surface_id: None,
            use_saved_secret: true,
        };

        let resolved = resolve_model_discovery_api_key(&request, &state, AiProviderType::OpenAi)
            .await
            .unwrap();
        assert_eq!(resolved, "sk-saved");
    }

    #[tokio::test]
    async fn resolve_model_discovery_api_key_accepts_legacy_default_surface_id() {
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        secret_store
            .store("provider/openai/llm", "api_key", "sk-legacy-surface")
            .await
            .unwrap();

        let mut config = AppConfig::default_config();
        config.ai_provider = AiProviderConfig {
            llm_api: Some(ExternalApiEndpoint {
                endpoint: "https://api.openai.com/v1".to_string(),
                api_key: String::new(),
                model: Some("gpt-5.4".to_string()),
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
                surface_id: None,
                credential: Some(CredentialBinding {
                    auth_mode: CredentialAuthMode::ApiKey,
                    backend_kind: CredentialBackendKind::OsSecretStore,
                    secret_ref: Some(SecretRef {
                        namespace: "provider/openai/llm".to_string(),
                        key: "api_key".to_string(),
                    }),
                    projection_enabled: false,
                }),
            }),
            ..AiProviderConfig::default()
        };

        let state = test_state_with_saved_secret(config, secret_store);
        let request = ProviderModelsRequest {
            provider_type: "OpenAi".to_string(),
            api_key: String::new(),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            surface: Some("llm_api".to_string()),
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            use_saved_secret: true,
        };

        let resolved = resolve_model_discovery_api_key(&request, &state, AiProviderType::OpenAi)
            .await
            .unwrap();
        assert_eq!(resolved, "sk-legacy-surface");
    }

    #[test]
    fn resolve_models_endpoint_rejects_unsupported_surface_catalog() {
        let error = resolve_models_endpoint(
            AiProviderType::OpenAi,
            Some("provider_surface.openai.managed_oauth"),
            None,
        )
        .expect_err("managed oauth should not expose model discovery");

        assert!(matches!(error, ApiError::BadRequest(_)));
    }

    #[tokio::test]
    async fn resolve_model_discovery_api_key_rejects_endpoint_mismatch_for_saved_secret() {
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        let mut config = AppConfig::default_config();
        config.ai_provider = AiProviderConfig {
            llm_api: Some(ExternalApiEndpoint {
                endpoint: "https://api.openai.com/v1".to_string(),
                api_key: "sk-legacy".to_string(),
                model: Some("gpt-5.4".to_string()),
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
                surface_id: None,
                credential: None,
            }),
            ..AiProviderConfig::default()
        };
        let state = test_state_with_saved_secret(config, secret_store);
        let request = ProviderModelsRequest {
            provider_type: "OpenAi".to_string(),
            api_key: String::new(),
            endpoint: Some("https://proxy.example.com/v1".to_string()),
            surface: Some("llm_api".to_string()),
            surface_id: None,
            use_saved_secret: true,
        };

        let resolved =
            resolve_saved_model_discovery_api_key(&request, &state, AiProviderType::OpenAi)
                .await
                .unwrap();
        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn resolve_model_discovery_api_key_uses_env_backend_without_secret_ref() {
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        secret_store
            .store("provider/openai/llm", "api_key", "sk-env")
            .await
            .unwrap();

        let mut config = AppConfig::default_config();
        config.ai_provider = AiProviderConfig {
            llm_api: Some(ExternalApiEndpoint {
                endpoint: "https://api.openai.com/v1".to_string(),
                api_key: String::new(),
                model: Some("gpt-5.4".to_string()),
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
                surface_id: None,
                credential: Some(CredentialBinding {
                    auth_mode: CredentialAuthMode::ApiKey,
                    backend_kind: CredentialBackendKind::Env,
                    secret_ref: None,
                    projection_enabled: false,
                }),
            }),
            ..AiProviderConfig::default()
        };
        let state = test_state_with_saved_secret(config, secret_store);
        let request = ProviderModelsRequest {
            provider_type: "OpenAi".to_string(),
            api_key: String::new(),
            endpoint: Some("https://api.openai.com/v1".to_string()),
            surface: Some("llm_api".to_string()),
            surface_id: None,
            use_saved_secret: true,
        };

        let resolved = resolve_model_discovery_api_key(&request, &state, AiProviderType::OpenAi)
            .await
            .unwrap();
        assert_eq!(resolved, "sk-env");
    }
}
