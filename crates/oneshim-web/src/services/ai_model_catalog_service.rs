use std::time::Duration;

use oneshim_core::config::AiProviderType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ApiError;
use crate::services::settings_service::is_masked_key;

const MODEL_DISCOVERY_TIMEOUT_SECS: u64 = 20;
const MAX_ERROR_SNIPPET_CHARS: usize = 220;

#[derive(Debug, Deserialize)]
pub struct ProviderModelsRequest {
    pub provider_type: String,
    pub api_key: String,
    pub endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderModelsResponse {
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

pub async fn fetch_provider_models(
    request: &ProviderModelsRequest,
) -> Result<ProviderModelsResponse, ApiError> {
    let provider_type = parse_provider_type(&request.provider_type)?;
    let api_key = request.api_key.trim();
    if api_key.is_empty() || is_masked_key(api_key) {
        return Err(ApiError::BadRequest(
            "A full API key is required to fetch model catalog.".to_string(),
        ));
    }

    let endpoint = resolve_models_endpoint(provider_type, request.endpoint.as_deref());
    if provider_type == AiProviderType::Google && is_google_vision_endpoint(&endpoint) {
        return Ok(ProviderModelsResponse {
            models: Vec::new(),
            notice: Some(
                "Google Vision OCR endpoint does not expose a selectable model catalog."
                    .to_string(),
            ),
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
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01");
        }
        AiProviderType::Google => {
            builder = builder.header("x-goog-api-key", api_key);
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

fn resolve_models_endpoint(provider_type: AiProviderType, endpoint: Option<&str>) -> String {
    let endpoint = endpoint.and_then(normalize_optional_endpoint);
    match provider_type {
        AiProviderType::Anthropic => endpoint
            .as_deref()
            .and_then(derive_anthropic_models_endpoint)
            .unwrap_or_else(|| "https://api.anthropic.com/v1/models".to_string()),
        AiProviderType::OpenAi => endpoint
            .as_deref()
            .and_then(derive_openai_models_endpoint)
            .unwrap_or_else(|| "https://api.openai.com/v1/models".to_string()),
        AiProviderType::Google => endpoint
            .as_deref()
            .and_then(derive_google_models_endpoint)
            .unwrap_or_else(|| {
                "https://generativelanguage.googleapis.com/v1beta/models".to_string()
            }),
        AiProviderType::Generic => endpoint
            .as_deref()
            .map(ToString::to_string)
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

fn is_google_vision_endpoint(endpoint: &str) -> bool {
    endpoint.contains("vision.googleapis.com")
}

fn parse_provider_type(raw: &str) -> Result<AiProviderType, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Ok(AiProviderType::Anthropic),
        "openai" | "open_ai" | "open-ai" | "openai-compatible" => Ok(AiProviderType::OpenAi),
        "google" | "gemini" => Ok(AiProviderType::Google),
        "generic" => Ok(AiProviderType::Generic),
        _ => Err(ApiError::BadRequest(format!(
            "Unsupported provider_type: {}",
            raw
        ))),
    }
}

fn truncate_error(raw: &str) -> String {
    let compact = raw.replace(['\n', '\r'], " ");
    let compact = compact.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(MAX_ERROR_SNIPPET_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
