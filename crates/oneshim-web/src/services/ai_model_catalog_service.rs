use std::time::Duration;

use oneshim_api_contracts::ai_providers::{ProviderModelsRequest, ProviderModelsResponse};
use oneshim_core::config::{AiProviderConfig, AiProviderType, ExternalApiEndpoint};
use serde_json::Value;

use crate::error::ApiError;
use crate::services::ai_provider_preset_service;
use crate::services::settings_service::is_masked_key;
use crate::AppState;

const MODEL_DISCOVERY_TIMEOUT_SECS: u64 = 20;
const MAX_ERROR_SNIPPET_CHARS: usize = 220;

pub async fn fetch_provider_models(
    request: &ProviderModelsRequest,
    state: &AppState,
) -> Result<ProviderModelsResponse, ApiError> {
    let provider_type = ai_provider_preset_service::resolve_provider_type(&request.provider_type)?;
    let api_key = resolve_model_discovery_api_key(request, state, provider_type).await?;

    let endpoint = resolve_models_endpoint(provider_type, request.endpoint.as_deref());
    if let Some(notice) =
        ai_provider_preset_service::ocr_model_catalog_notice_for_endpoint(provider_type, &endpoint)
    {
        return Ok(ProviderModelsResponse {
            models: Vec::new(),
            notice: Some(notice),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(MODEL_DISCOVERY_TIMEOUT_SECS))
        .build()
        .map_err(|e| ApiError::Internal(format!("Failed to create model discovery client: {e}")))?;

    let mut builder = client.get(&endpoint);
    match provider_type {
        AiProviderType::Anthropic => {
            builder = builder
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01");
        }
        AiProviderType::Google => {
            builder = builder.header("x-goog-api-key", &api_key);
        }
        AiProviderType::OpenAi | AiProviderType::Generic => {
            builder = builder.header("Authorization", format!("Bearer {api_key}"));
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

    let mut models = parse_models(provider_type, &body)?;
    models.sort_unstable();
    models.dedup();

    Ok(ProviderModelsResponse {
        notice: if models.is_empty() {
            Some("Provider returned no models for this configuration.".to_string())
        } else {
            None
        },
        models,
    })
}

fn parse_models(provider_type: AiProviderType, body: &str) -> Result<Vec<String>, ApiError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| ApiError::BadRequest(format!("Invalid model catalog response JSON: {e}")))?;

    match provider_type {
        AiProviderType::Google => parse_google_models(&value),
        AiProviderType::Anthropic | AiProviderType::OpenAi | AiProviderType::Generic => {
            parse_standard_models(&value)
        }
    }
}

fn parse_google_models(value: &Value) -> Result<Vec<String>, ApiError> {
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
        fallback_models.push(normalized.clone());

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
            generation_models.push(normalized);
        }
    }

    if !generation_models.is_empty() {
        return Ok(generation_models);
    }
    Ok(fallback_models)
}

fn parse_standard_models(value: &Value) -> Result<Vec<String>, ApiError> {
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
            entry
                .get("id")
                .and_then(|v| v.as_str())
                .or_else(|| entry.get("name").and_then(|v| v.as_str()))
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToString::to_string)
        })
        .collect::<Vec<_>>();

    Ok(models)
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

    if let Some(request_endpoint) = request.endpoint.as_deref() {
        let request_endpoint = normalize_optional_endpoint(request_endpoint);
        let saved_endpoint_normalized = normalize_optional_endpoint(&saved_endpoint.endpoint);
        if request_endpoint != saved_endpoint_normalized {
            return Ok(None);
        }
    }

    if let (Some(secret_store), Some(secret_ref)) = (
        state.secret_store.as_ref(),
        saved_endpoint
            .credential
            .as_ref()
            .and_then(|binding| binding.secret_ref.as_ref()),
    ) {
        if let Some(secret) = secret_store
            .retrieve(&secret_ref.namespace, &secret_ref.key)
            .await
            .map_err(|e| {
                ApiError::Internal(format!("Failed to read stored model discovery secret: {e}"))
            })?
        {
            if !secret.trim().is_empty() {
                return Ok(Some(secret));
            }
        }
    }

    let plaintext = saved_endpoint.api_key.trim();
    if !plaintext.is_empty() {
        return Ok(Some(plaintext.to_string()));
    }

    Ok(None)
}

#[derive(Clone, Copy)]
enum ModelSurface {
    Ocr,
    Llm,
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

fn resolve_models_endpoint(provider_type: AiProviderType, endpoint: Option<&str>) -> String {
    let endpoint = endpoint.and_then(normalize_optional_endpoint);
    match provider_type {
        AiProviderType::Anthropic => endpoint
            .as_deref()
            .and_then(derive_anthropic_models_endpoint)
            .or_else(|| ai_provider_preset_service::default_model_catalog_endpoint(provider_type))
            .unwrap_or_else(|| "https://api.anthropic.com/v1/models".to_string()),
        AiProviderType::OpenAi => endpoint
            .as_deref()
            .and_then(derive_openai_models_endpoint)
            .or_else(|| ai_provider_preset_service::default_model_catalog_endpoint(provider_type))
            .unwrap_or_else(|| "https://api.openai.com/v1/models".to_string()),
        AiProviderType::Google => endpoint
            .as_deref()
            .and_then(derive_google_models_endpoint)
            .or_else(|| ai_provider_preset_service::default_model_catalog_endpoint(provider_type))
            .unwrap_or_else(|| {
                "https://generativelanguage.googleapis.com/v1beta/models".to_string()
            }),
        AiProviderType::Generic => endpoint
            .as_deref()
            .map(ToString::to_string)
            .or_else(|| ai_provider_preset_service::default_model_catalog_endpoint(provider_type))
            .unwrap_or_else(|| "https://api.openai.com/v1/models".to_string()),
    }
}

fn normalize_optional_endpoint(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.trim_end_matches('/').to_string())
}

fn derive_openai_models_endpoint(endpoint: &str) -> Option<String> {
    if endpoint.ends_with("/models") {
        return Some(endpoint.to_string());
    }
    if let Some(prefix) = endpoint.split("/chat/completions").next() {
        if prefix != endpoint {
            return Some(format!("{prefix}/models"));
        }
    }
    if let Some(prefix) = endpoint.split("/responses").next() {
        if prefix != endpoint {
            return Some(format!("{prefix}/models"));
        }
    }
    if let Some(prefix) = endpoint.split("/models/").next() {
        if prefix != endpoint {
            return Some(format!("{prefix}/models"));
        }
    }
    if endpoint.contains("/v1") {
        let base = endpoint
            .split("/v1")
            .next()
            .unwrap_or(endpoint)
            .trim_end_matches('/');
        return Some(format!("{base}/v1/models"));
    }
    None
}

fn derive_anthropic_models_endpoint(endpoint: &str) -> Option<String> {
    if endpoint.ends_with("/v1/models") {
        return Some(endpoint.to_string());
    }
    if let Some(prefix) = endpoint.split("/v1/messages").next() {
        if prefix != endpoint {
            return Some(format!("{prefix}/v1/models"));
        }
    }
    if endpoint.contains("/v1") {
        let base = endpoint
            .split("/v1")
            .next()
            .unwrap_or(endpoint)
            .trim_end_matches('/');
        return Some(format!("{base}/v1/models"));
    }
    None
}

fn derive_google_models_endpoint(endpoint: &str) -> Option<String> {
    if endpoint.ends_with("/models") {
        return Some(endpoint.to_string());
    }
    if let Some(prefix) = endpoint.split("/models/").next() {
        if prefix != endpoint {
            return Some(format!("{prefix}/models"));
        }
    }
    if endpoint.contains("generativelanguage.googleapis.com") && endpoint.contains("/v1") {
        let base = endpoint
            .split("/v1")
            .next()
            .unwrap_or(endpoint)
            .trim_end_matches('/');
        let version = if endpoint.contains("/v1beta") {
            "v1beta"
        } else {
            "v1"
        };
        return Some(format!("{base}/{version}/models"));
    }
    Some(endpoint.to_string())
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
            secret_store: Some(secret_store),
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        }
    }

    #[test]
    fn derives_google_models_endpoint_from_generate_content_url() {
        let endpoint = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";
        let derived = derive_google_models_endpoint(endpoint).unwrap();
        assert_eq!(
            derived,
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
    }

    #[test]
    fn derives_openai_models_endpoint_from_chat_completions_url() {
        let endpoint = "https://api.openai.com/v1/chat/completions";
        let derived = derive_openai_models_endpoint(endpoint).unwrap();
        assert_eq!(derived, "https://api.openai.com/v1/models");
    }

    #[test]
    fn parses_google_model_catalog() {
        let body = r#"{
          "models": [
            {
              "name": "models/gemini-2.5-flash",
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
        assert_eq!(parsed, vec!["gemini-2.5-flash".to_string()]);
    }

    #[test]
    fn parses_standard_model_catalog() {
        let body = r#"{
          "data": [
            {"id": "gpt-4.1-mini"},
            {"id": "o3-mini"}
          ]
        }"#;
        let value: Value = serde_json::from_str(body).unwrap();
        let parsed = parse_standard_models(&value).unwrap();
        assert_eq!(parsed.len(), 2);
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
                model: Some("gpt-4.1-mini".to_string()),
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
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
            use_saved_secret: true,
        };

        let resolved = resolve_model_discovery_api_key(&request, &state, AiProviderType::OpenAi)
            .await
            .unwrap();
        assert_eq!(resolved, "sk-saved");
    }

    #[tokio::test]
    async fn resolve_model_discovery_api_key_rejects_endpoint_mismatch_for_saved_secret() {
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        let mut config = AppConfig::default_config();
        config.ai_provider = AiProviderConfig {
            llm_api: Some(ExternalApiEndpoint {
                endpoint: "https://api.openai.com/v1".to_string(),
                api_key: "sk-legacy".to_string(),
                model: Some("gpt-4.1-mini".to_string()),
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
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
            use_saved_secret: true,
        };

        let resolved =
            resolve_saved_model_discovery_api_key(&request, &state, AiProviderType::OpenAi)
                .await
                .unwrap();
        assert!(resolved.is_none());
    }
}
