use async_trait::async_trait;
use oneshim_api_contracts::ai_providers::ProviderModelSupportStatus;
use oneshim_api_contracts::provider_specs::{
    self, ProviderAuthScheme, ProviderRequestShape, ProviderTransportKind,
    SurfaceModelCapabilityKind,
};
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, warn};

use oneshim_core::ai_model_lifecycle_policy::{self, ModelLifecycleDecision};
use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};
use oneshim_core::error::CoreError;
use oneshim_core::ports::credential_source::CredentialSource;
use oneshim_core::ports::ocr_provider::{OcrProvider, OcrResult};

/// - Claude Vision (Anthropic): `POST /v1/messages` + image content block
/// - Google Cloud Vision: `POST /v1/images:annotate` + TEXT_DETECTION
pub struct RemoteOcrProvider {
    http_client: reqwest::Client,
    endpoint: String,
    credential: CredentialSource,
    model: Option<String>,
    provider_type: AiProviderType,
    surface_id: Option<String>,
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl std::fmt::Debug for RemoteOcrProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteOcrProvider")
            .field("endpoint", &self.endpoint)
            .field("credential", &self.credential)
            .field("model", &self.model)
            .field("provider_type", &self.provider_type)
            .field("surface_id", &self.surface_id)
            .finish()
    }
}

const OCR_LINE_INSTRUCTION: &str =
    "List all visible text from the image line by line. Output exactly one text item per line.";
const OCR_JSON_INSTRUCTION: &str = "Extract all visible text from the image and return strict JSON only in this schema: {\"results\":[{\"text\":\"...\",\"x\":0,\"y\":0,\"width\":0,\"height\":0,\"confidence\":0.0}]}. If exact geometry is unknown, use 0 for coordinates and size.";

#[derive(Debug, Clone, Copy)]
enum OcrProviderStrategy {
    Anthropic,
    OpenAi,
    Google,
}

impl TryFrom<ProviderRequestShape> for OcrProviderStrategy {
    type Error = CoreError;

    fn try_from(value: ProviderRequestShape) -> Result<Self, Self::Error> {
        match value {
            ProviderRequestShape::AnthropicMessages
            | ProviderRequestShape::AnthropicVisionMessages => Ok(Self::Anthropic),
            ProviderRequestShape::OpenAiChatCompletions
            | ProviderRequestShape::OpenAiVisionChatCompletions
            | ProviderRequestShape::OpenAiResponses => Ok(Self::OpenAi),
            ProviderRequestShape::GoogleGenerateContent
            | ProviderRequestShape::GoogleVisionAnnotate => Ok(Self::Google),
        }
    }
}

impl OcrProviderStrategy {
    fn build_request_body(self, encoded: &str, media_type: &str, model: &str) -> Value {
        match self {
            Self::Google => serde_json::json!({
                "requests": [{
                    "image": { "content": encoded },
                    "features": [{
                        "type": "TEXT_DETECTION",
                        "maxResults": 64
                    }]
                }]
            }),
            Self::OpenAi => {
                let data_uri = format!("data:{media_type};base64,{encoded}");
                serde_json::json!({
                    "model": model,
                    "max_tokens": 2048,
                    "response_format": { "type": "json_object" },
                    "messages": [{
                        "role": "user",
                        "content": [
                            {
                                "type": "text",
                                "text": OCR_JSON_INSTRUCTION
                            },
                            {
                                "type": "image_url",
                                "image_url": { "url": data_uri }
                            }
                        ]
                    }]
                })
            }
            Self::Anthropic => serde_json::json!({
                "model": model,
                "max_tokens": 4096,
                "messages": [{
                    "role": "user",
                    "content": [
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": media_type,
                                "data": encoded
                            }
                        },
                        {
                            "type": "text",
                            "text": OCR_LINE_INSTRUCTION
                        }
                    ]
                }]
            }),
        }
    }

    fn parse_response(self, body: &str) -> Result<Vec<OcrResult>, CoreError> {
        match self {
            Self::Anthropic => RemoteOcrProvider::parse_claude_vision_response(body),
            Self::Google => RemoteOcrProvider::parse_google_vision_response(body),
            Self::OpenAi => RemoteOcrProvider::parse_openai_vision_response(body),
        }
    }
}

fn apply_auth_headers(
    auth_scheme: ProviderAuthScheme,
    builder: reqwest::RequestBuilder,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match auth_scheme {
        ProviderAuthScheme::None => builder,
        ProviderAuthScheme::XApiKey => builder
            .header("x-api-key", api_key)
            .header("anthropic-version", crate::ANTHROPIC_API_VERSION),
        ProviderAuthScheme::XGoogApiKey => builder.header("x-goog-api-key", api_key),
        ProviderAuthScheme::Bearer => builder.header("Authorization", format!("Bearer {api_key}")),
    }
}

impl RemoteOcrProvider {
    fn ocr_request_shape(&self) -> Result<ProviderRequestShape, CoreError> {
        provider_specs::resolved_request_shape(
            self.provider_type,
            self.surface_id.as_deref(),
            ProviderTransportKind::Ocr,
        )
        .map_err(CoreError::Internal)
    }

    fn ocr_auth_scheme(&self) -> Result<ProviderAuthScheme, CoreError> {
        provider_specs::resolved_auth_scheme(
            self.provider_type,
            self.surface_id.as_deref(),
            ProviderTransportKind::Ocr,
        )
        .map_err(CoreError::Internal)
    }

    fn ensure_ocr_parameters_supported(&self, parameters: &[&str]) -> Result<(), CoreError> {
        provider_specs::validate_supported_parameters(
            self.provider_type,
            self.surface_id.as_deref(),
            provider_specs::SurfaceCapabilityKind::Ocr,
            parameters,
        )
        .map_err(CoreError::Internal)
    }

    async fn ensure_runtime_ocr_model_ready(&self, model: &str) -> Result<(), CoreError> {
        if model.trim().is_empty()
            || !self.surface_id.as_deref().is_some_and(|surface_id| {
                surface_id.eq_ignore_ascii_case("provider_surface.ollama.local_http")
            })
        {
            return Ok(());
        }

        match probe_ollama_model_supports_ocr(&self.http_client, &self.endpoint, model).await {
            Ok(Some(true)) | Ok(None) => Ok(()),
            Ok(Some(false)) => Err(CoreError::Config(format!(
                "Selected Ollama model '{model}' does not advertise image support. Choose a multimodal model such as 'qwen3-vl:8b' or 'gemma3:4b'."
            ))),
            Err(error) => {
                warn!(
                    endpoint = %self.endpoint,
                    model = %model,
                    error = %error,
                    "Failed to verify Ollama OCR model capability; proceeding with request."
                );
                Ok(())
            }
        }
    }

    pub fn new(config: &ExternalApiEndpoint) -> Result<Self, CoreError> {
        let auth_scheme = provider_specs::resolved_auth_scheme(
            config.provider_type,
            config.surface_id.as_deref(),
            ProviderTransportKind::Ocr,
        )
        .map_err(CoreError::Internal)?;
        if !matches!(auth_scheme, ProviderAuthScheme::None) && config.api_key.is_empty() {
            return Err(CoreError::Config(
                "AI OCR API key is not configured. Set it in Settings.".into(),
            ));
        }
        let credential = if matches!(auth_scheme, ProviderAuthScheme::None) {
            CredentialSource::NoAuth
        } else {
            CredentialSource::ApiKey(config.api_key.clone())
        };
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| CoreError::Network(format!("HTTP client create failure: {}", e)))?;

        let supports_model = provider_specs::resolved_surface_supports_model_selection(
            config.provider_type,
            config.surface_id.as_deref(),
            provider_specs::SurfaceCapabilityKind::Ocr,
        )
        .map_err(CoreError::Internal)?;
        let resolved_model = config.model.clone().or_else(|| {
            provider_specs::resolved_default_model(
                config.provider_type,
                config.surface_id.as_deref(),
                provider_specs::SurfaceCapabilityKind::Ocr,
            )
            .ok()
            .flatten()
        });
        if resolved_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
            && supports_model
        {
            return Err(CoreError::Config(
                "The selected OCR provider surface requires an explicit model selection."
                    .to_string(),
            ));
        }

        if let Some(model) = resolved_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !supports_model {
                return Err(CoreError::Config(
                    "The selected OCR provider surface does not support configurable model selection."
                        .to_string(),
                ));
            }
            match ai_model_lifecycle_policy::evaluate_model_lifecycle_now_for_surface(
                config.provider_type,
                config.surface_id.as_deref(),
                model,
            )? {
                ModelLifecycleDecision::Allowed => {}
                ModelLifecycleDecision::Warn {
                    message,
                    replacement,
                } => {
                    warn!(
                        provider = ?config.provider_type,
                        model = %model,
                        replacement = ?replacement,
                        "{}", message
                    );
                }
                ModelLifecycleDecision::Block { message, .. } => {
                    return Err(CoreError::PolicyDenied(message));
                }
            }
            if let Some(message) = provider_specs::known_model_capability_warning(
                config.provider_type,
                config.surface_id.as_deref(),
                provider_specs::SurfaceCapabilityKind::Ocr,
                model,
            )
            .map_err(CoreError::Internal)?
            {
                warn!(
                    provider = ?config.provider_type,
                    surface_id = ?config.surface_id,
                    model = %model,
                    "{message}"
                );
            }
            provider_specs::validate_known_model_capability(
                config.provider_type,
                config.surface_id.as_deref(),
                provider_specs::SurfaceCapabilityKind::Ocr,
                model,
            )
            .map_err(CoreError::Config)?;
            if provider_specs::resolved_ocr_requires_structured_output_model(
                config.provider_type,
                config.surface_id.as_deref(),
            )
            .map_err(CoreError::Internal)?
            {
                match provider_specs::resolved_model_capability_status(
                    config.provider_type,
                    config.surface_id.as_deref(),
                    SurfaceModelCapabilityKind::StructuredOutput,
                    model,
                )
                .map_err(CoreError::Internal)?
                {
                    ProviderModelSupportStatus::Supported => {}
                    ProviderModelSupportStatus::Unsupported => {
                        return Err(CoreError::Config(format!(
                            "Selected OCR model '{model}' is not marked as supporting structured JSON output required by this provider surface."
                        )));
                    }
                    ProviderModelSupportStatus::Unknown => {
                        warn!(
                            provider = ?config.provider_type,
                            surface_id = ?config.surface_id,
                            model = %model,
                            "OCR surface requires structured output, but the selected model's support is unknown."
                        );
                    }
                }
            }
        }

        debug!(
            endpoint = %config.endpoint,
            model = ?config.model,
            timeout = config.timeout_secs,
            "RemoteOcrProvider initialize"
        );

        Ok(Self {
            http_client,
            endpoint: config.endpoint.clone(),
            credential,
            model: resolved_model,
            provider_type: config.provider_type,
            surface_id: config.surface_id.clone(),
            timeout_secs: config.timeout_secs,
        })
    }

    /// Create a provider with a managed credential source (e.g., OAuth).
    ///
    /// When the credential is `ManagedOAuth`, the API base URL from the
    /// credential is used instead of the config endpoint.
    pub fn new_with_credential(
        config: &ExternalApiEndpoint,
        credential: CredentialSource,
    ) -> Result<Self, CoreError> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| CoreError::Network(format!("HTTP client create failure: {}", e)))?;
        let supports_model = provider_specs::resolved_surface_supports_model_selection(
            config.provider_type,
            config.surface_id.as_deref(),
            provider_specs::SurfaceCapabilityKind::Ocr,
        )
        .map_err(CoreError::Internal)?;
        let resolved_model = config.model.clone().or_else(|| {
            provider_specs::resolved_default_model(
                config.provider_type,
                config.surface_id.as_deref(),
                provider_specs::SurfaceCapabilityKind::Ocr,
            )
            .ok()
            .flatten()
        });
        if resolved_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
            && supports_model
        {
            return Err(CoreError::Config(
                "The selected OCR provider surface requires an explicit model selection."
                    .to_string(),
            ));
        }

        if let Some(model) = resolved_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !supports_model {
                return Err(CoreError::Config(
                    "The selected OCR provider surface does not support configurable model selection."
                        .to_string(),
                ));
            }
            match ai_model_lifecycle_policy::evaluate_model_lifecycle_now_for_surface(
                config.provider_type,
                config.surface_id.as_deref(),
                model,
            )? {
                ModelLifecycleDecision::Allowed => {}
                ModelLifecycleDecision::Warn {
                    message,
                    replacement,
                } => {
                    warn!(
                        provider = ?config.provider_type,
                        model = %model,
                        replacement = ?replacement,
                        "{}", message
                    );
                }
                ModelLifecycleDecision::Block { message, .. } => {
                    return Err(CoreError::PolicyDenied(message));
                }
            }
            provider_specs::validate_known_model_capability(
                config.provider_type,
                config.surface_id.as_deref(),
                provider_specs::SurfaceCapabilityKind::Ocr,
                model,
            )
            .map_err(CoreError::Config)?;
            if provider_specs::resolved_ocr_requires_structured_output_model(
                config.provider_type,
                config.surface_id.as_deref(),
            )
            .map_err(CoreError::Internal)?
            {
                match provider_specs::resolved_model_capability_status(
                    config.provider_type,
                    config.surface_id.as_deref(),
                    SurfaceModelCapabilityKind::StructuredOutput,
                    model,
                )
                .map_err(CoreError::Internal)?
                {
                    ProviderModelSupportStatus::Supported => {}
                    ProviderModelSupportStatus::Unsupported => {
                        return Err(CoreError::Config(format!(
                            "Selected OCR model '{model}' is not marked as supporting structured JSON output required by this provider surface."
                        )));
                    }
                    ProviderModelSupportStatus::Unknown => {
                        warn!(
                            provider = ?config.provider_type,
                            surface_id = ?config.surface_id,
                            model = %model,
                            "OCR surface requires structured output, but the selected model's support is unknown."
                        );
                    }
                }
            }
        }

        // Use OAuth-provided base URL when available.
        let endpoint = credential
            .api_base_url()
            .map(String::from)
            .unwrap_or_else(|| config.endpoint.clone());

        Ok(Self {
            http_client,
            endpoint,
            credential,
            model: resolved_model,
            provider_type: config.provider_type,
            surface_id: config.surface_id.clone(),
            timeout_secs: config.timeout_secs,
        })
    }

    fn parse_claude_vision_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("Failed to parse response JSON: {}", e)))?;

        let mut results = Vec::new();

        if let Some(content) = response.get("content").and_then(|c| c.as_array()) {
            for block in content {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    for (i, line) in text.lines().enumerate() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            results.push(OcrResult {
                                text: trimmed.to_string(),
                                x: 0,
                                y: (i as i32) * 20, // temporary line height
                                width: (trimmed.len() as u32) * 8, // temporary char width
                                height: 20,
                                confidence: 0.8, // API default confidence
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    fn parse_openai_vision_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        if let Ok(results) = Self::parse_generic_response(body) {
            return Ok(results);
        }

        let response: Value = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("Failed to parse OpenAI response: {e}")))?;

        let text = Self::extract_openai_text(&response).ok_or_else(|| {
            CoreError::OcrError("No text content found in OpenAI OCR response".to_string())
        })?;

        if let Some(json_fragment) = extract_json_fragment(&text) {
            if let Ok(results) = Self::parse_generic_response(&json_fragment) {
                return Ok(results);
            }
        }

        Ok(parse_text_lines_to_results(&text))
    }

    fn parse_generic_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        #[derive(Deserialize)]
        struct GenericResponse {
            results: Option<Vec<OcrResult>>,
        }

        let response: GenericResponse = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("Failed to parse generic response: {}", e)))?;

        response.results.ok_or_else(|| {
            CoreError::OcrError("Generic OCR response missing `results` field".to_string())
        })
    }

    fn extract_openai_text(response: &Value) -> Option<String> {
        if let Some(content) = response
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
        {
            if let Some(text) = value_to_text(content) {
                return Some(text);
            }
        }

        let mut chunks = Vec::new();
        if let Some(outputs) = response.get("output").and_then(|o| o.as_array()) {
            for output in outputs {
                if let Some(parts) = output.get("content").and_then(|c| c.as_array()) {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                chunks.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }

        if chunks.is_empty() {
            None
        } else {
            Some(chunks.join("\n"))
        }
    }

    fn parse_google_vision_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body).map_err(|e| {
            CoreError::OcrError(format!("Failed to parse Google Vision response: {}", e))
        })?;

        let mut results = Vec::new();
        let annotations = response
            .get("responses")
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.first())
            .and_then(|entry| entry.get("textAnnotations"))
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();

        for annotation in annotations {
            let text = annotation
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                continue;
            }

            let vertices = annotation
                .get("boundingPoly")
                .and_then(|poly| poly.get("vertices"))
                .and_then(|v| v.as_array());
            let (x, y, width, height) = parse_bounding_vertices(vertices);

            results.push(OcrResult {
                text: text.to_string(),
                x,
                y,
                width,
                height,
                confidence: 0.8,
            });
        }

        Ok(results)
    }
}

#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    details: Option<OllamaShowDetails>,
    #[serde(default)]
    projector_info: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct OllamaShowDetails {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    families: Vec<String>,
}

fn derive_ollama_show_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    for suffix in [
        "/v1/responses",
        "/v1/chat/completions",
        "/api/tags",
        "/api/show",
    ] {
        if let Some(prefix) = trimmed.strip_suffix(suffix) {
            return format!("{prefix}/api/show");
        }
    }
    format!("{trimmed}/api/show")
}

fn infer_ollama_vision_support(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    [
        "vision",
        "vl",
        "llava",
        "bakllava",
        "moondream",
        "minicpm-v",
        "minicpmv",
        "gemma3",
    ]
    .iter()
    .any(|token| normalized.contains(token))
}

fn parse_ollama_show_supports_ocr(body: &str, model: &str) -> Result<Option<bool>, CoreError> {
    let parsed: OllamaShowResponse = serde_json::from_str(body).map_err(|error| {
        CoreError::Network(format!("Failed to parse Ollama model details: {error}"))
    })?;
    let mut capabilities = parsed.capabilities;
    if let Some(details) = parsed.details {
        capabilities.extend(details.capabilities);
        capabilities.extend(details.families);
    }
    if parsed.projector_info.is_some() {
        capabilities.push("projector".to_string());
    }

    if capabilities.is_empty() {
        return Ok(Some(infer_ollama_vision_support(model)));
    }

    let supports_vision = capabilities.iter().any(|entry| {
        let normalized = entry.trim().to_ascii_lowercase();
        normalized.contains("vision")
            || normalized.contains("clip")
            || normalized.contains("projector")
            || normalized.contains("vl")
            || normalized.contains("llava")
    });
    Ok(Some(supports_vision))
}

async fn probe_ollama_model_supports_ocr(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
) -> Result<Option<bool>, CoreError> {
    let response = client
        .post(derive_ollama_show_endpoint(endpoint))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "model": model }))
        .send()
        .await
        .map_err(|error| {
            CoreError::Network(format!("Ollama model capability probe failed: {error}"))
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        CoreError::Network(format!(
            "Failed to read Ollama model capability probe response: {error}"
        ))
    })?;
    if !status.is_success() {
        return Err(CoreError::Network(format!(
            "Ollama model capability probe failed ({status}): {body}"
        )));
    }

    parse_ollama_show_supports_ocr(&body, model)
}

#[async_trait]
impl OcrProvider for RemoteOcrProvider {
    async fn extract_elements(
        &self,
        image: &[u8],
        image_format: &str,
    ) -> Result<Vec<OcrResult>, CoreError> {
        use base64::Engine;

        let encoded = base64::engine::general_purpose::STANDARD.encode(image);
        let media_type = match image_format {
            "png" => "image/png",
            "jpeg" | "jpg" => "image/jpeg",
            "webp" => "image/webp",
            _ => "image/png",
        };

        let model = self.model.as_deref().unwrap_or("");
        self.ensure_runtime_ocr_model_ready(model).await?;
        let request_shape = self.ocr_request_shape()?;
        match request_shape {
            ProviderRequestShape::AnthropicMessages
            | ProviderRequestShape::AnthropicVisionMessages => {
                self.ensure_ocr_parameters_supported(&["model", "max_tokens", "messages"])?;
            }
            ProviderRequestShape::OpenAiChatCompletions
            | ProviderRequestShape::OpenAiVisionChatCompletions
            | ProviderRequestShape::OpenAiResponses => {
                self.ensure_ocr_parameters_supported(&[
                    "model",
                    "max_tokens",
                    "response_format",
                    "messages",
                ])?;
            }
            ProviderRequestShape::GoogleGenerateContent
            | ProviderRequestShape::GoogleVisionAnnotate => {
                self.ensure_ocr_parameters_supported(&[
                    "requests",
                    "TEXT_DETECTION",
                    "maxResults",
                ])?;
            }
        }
        let strategy = OcrProviderStrategy::try_from(request_shape)?;

        let request_body = strategy.build_request_body(&encoded, media_type, model);

        debug!(
            endpoint = %self.endpoint,
            model = model,
            image_size = image.len(),
            "Calling external OCR API"
        );

        let mut builder = self
            .http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body);

        let auth_scheme = self.ocr_auth_scheme()?;
        if matches!(auth_scheme, ProviderAuthScheme::None) {
            builder = apply_auth_headers(auth_scheme, builder, "");
        } else {
            let bearer_token = self.credential.resolve_bearer_token().await?;
            builder = apply_auth_headers(auth_scheme, builder, &bearer_token);
        }

        // ChatGPT OAuth requires a version header for model access (GPT-5.4 etc.).
        // Only applies to OpenAI-compatible providers, matching LLM client behaviour.
        // Ref: openai/codex codex-rs/core/src/model_provider_info.rs
        if self.credential.is_managed() && matches!(auth_scheme, ProviderAuthScheme::Bearer) {
            builder = builder.header("version", env!("CARGO_PKG_VERSION"));
        }

        let response = builder
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("OCR API request failed: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| CoreError::Network(format!("OCR API response read failure: {}", e)))?;

        if !status.is_success() {
            warn!(status = %status, "OCR API error response");
            return Err(CoreError::OcrError(format!(
                "OCR API error ({}): {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        let results = strategy.parse_response(&body)?;

        debug!(count = results.len(), "OCR received");
        Ok(results)
    }

    fn provider_name(&self) -> &str {
        "remote-ocr"
    }

    fn is_external(&self) -> bool {
        true
    }
}

fn parse_bounding_vertices(vertices: Option<&Vec<serde_json::Value>>) -> (i32, i32, u32, u32) {
    let Some(vertices) = vertices else {
        return (0, 0, 0, 0);
    };

    let points: Vec<(i32, i32)> = vertices
        .iter()
        .map(|vertex| {
            let x = vertex.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let y = vertex.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            (x, y)
        })
        .collect();

    if points.is_empty() {
        return (0, 0, 0, 0);
    }

    let min_x = points.iter().map(|(x, _)| *x).min().unwrap_or(0);
    let max_x = points.iter().map(|(x, _)| *x).max().unwrap_or(0);
    let min_y = points.iter().map(|(_, y)| *y).min().unwrap_or(0);
    let max_y = points.iter().map(|(_, y)| *y).max().unwrap_or(0);

    (
        min_x,
        min_y,
        (max_x - min_x).max(0) as u32,
        (max_y - min_y).max(0) as u32,
    )
}

fn value_to_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(items) = value.as_array() {
        let mut parts = Vec::new();
        for item in items {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
        if !parts.is_empty() {
            return Some(parts.join("\n"));
        }
    }

    None
}

fn extract_json_fragment(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    Some(text[start..=end].to_string())
}

fn parse_text_lines_to_results(text: &str) -> Vec<OcrResult> {
    text.lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(OcrResult {
                text: trimmed.to_string(),
                x: 0,
                y: (idx as i32) * 20,
                width: (trimmed.len() as u32) * 8,
                height: 20,
                confidence: 0.8,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_remote_ocr_empty_key_error() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.example.com".to_string(),
            api_key: "".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Generic,
            surface_id: None,
            credential: None,
        };
        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not configured"));
    }

    #[test]
    fn new_remote_ocr_with_key() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.example.com".to_string(),
            api_key: "test-api-key-placeholder".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Generic,
            surface_id: None,
            credential: None,
        };
        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn generic_ocr_uses_spec_shape_and_default_model() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.example.com".to_string(),
            api_key: "test-api-key-placeholder".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Generic,
            surface_id: None,
            credential: None,
        };

        let provider = RemoteOcrProvider::new(&config).expect("generic OCR provider should build");
        assert_eq!(
            provider.ocr_request_shape().expect("shape should resolve"),
            ProviderRequestShape::OpenAiVisionChatCompletions
        );
        assert_eq!(provider.model.as_deref(), Some("gpt-5-mini"));
    }

    #[test]
    fn ollama_ocr_initializes_without_api_key() {
        let config = ExternalApiEndpoint {
            endpoint: "http://localhost:11434/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Ollama,
            surface_id: Some("provider_surface.ollama.local_http".to_string()),
            credential: None,
        };

        let provider = RemoteOcrProvider::new(&config).expect("ollama OCR provider should build");
        assert_eq!(
            provider.ocr_request_shape().expect("shape should resolve"),
            ProviderRequestShape::OpenAiVisionChatCompletions
        );
        assert_eq!(provider.model.as_deref(), Some("qwen3-vl:8b"));
    }

    #[test]
    fn ollama_ocr_rejects_known_text_only_model() {
        let config = ExternalApiEndpoint {
            endpoint: "http://localhost:11434/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: Some("qwen3:8b".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::Ollama,
            surface_id: Some("provider_surface.ollama.local_http".to_string()),
            credential: None,
        };

        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("OCR-capable"));
    }

    #[test]
    fn local_openai_compatible_ocr_requires_explicit_model_selection() {
        let config = ExternalApiEndpoint {
            endpoint: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Generic,
            surface_id: Some("provider_surface.generic.local_openai_compatible".to_string()),
            credential: None,
        };

        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("requires an explicit model selection"));
    }

    #[test]
    fn local_openai_compatible_ocr_rejects_model_without_structured_output() {
        let config = ExternalApiEndpoint {
            endpoint: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: Some("text-embedding-3-small".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::Generic,
            surface_id: Some("provider_surface.generic.local_openai_compatible".to_string()),
            credential: None,
        };

        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(
            message.contains("structured JSON output")
                || message.contains("OCR-capable")
                || message.contains("not marked as OCR-capable")
        );
    }

    #[test]
    fn new_remote_ocr_rejects_retired_model_by_policy() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: "test-api-key-placeholder".to_string(),
            model: Some("gpt-3.5-turbo".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        };

        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("retired as of"));
    }

    #[test]
    fn google_surface_uses_surface_transport_shape() {
        let config = ExternalApiEndpoint {
            endpoint: "https://vision.googleapis.com/v1/images:annotate".to_string(),
            api_key: "test-api-key-placeholder".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Google,
            surface_id: Some("provider_surface.google.direct_api".to_string()),
            credential: None,
        };

        let provider = RemoteOcrProvider::new(&config).expect("google OCR provider should build");
        assert_eq!(
            provider.ocr_request_shape().expect("shape should resolve"),
            ProviderRequestShape::GoogleVisionAnnotate
        );
    }

    #[test]
    fn new_remote_ocr_rejects_known_non_ocr_model() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: "test-api-key-placeholder".to_string(),
            model: Some("text-embedding-3-small".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            credential: None,
        };

        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not marked as OCR-capable"));
    }

    #[test]
    fn google_ocr_rejects_explicit_model_selection() {
        let config = ExternalApiEndpoint {
            endpoint: "https://vision.googleapis.com/v1/images:annotate".to_string(),
            api_key: "test-api-key-placeholder".to_string(),
            model: Some("gemini-2.5-flash".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::Google,
            surface_id: Some("provider_surface.google.direct_api".to_string()),
            credential: None,
        };

        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not support configurable model selection"));
    }

    #[test]
    fn parse_claude_vision_response_valid() {
        let response = r#"{
            "content": [
                {
                    "type": "text",
                    "text": "file\nedit\nview\nsave"
                }
            ]
        }"#;
        let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].text, "file");
        assert_eq!(results[3].text, "save");
    }

    #[test]
    fn parse_claude_vision_response_empty() {
        let response = r#"{"content": [{"type": "text", "text": ""}]}"#;
        let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn parse_generic_response_valid() {
        let response = r#"{
            "results": [
                {"text": "save", "x": 100, "y": 200, "width": 60, "height": 25, "confidence": 0.95}
            ]
        }"#;
        let results = RemoteOcrProvider::parse_generic_response(response).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "save");
        assert_eq!(results[0].x, 100);
    }

    #[test]
    fn parse_generic_response_empty() {
        let response = r#"{"results": []}"#;
        let results = RemoteOcrProvider::parse_generic_response(response).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn parse_openai_vision_response_json_content() {
        let response = r#"{
            "choices": [{
                "message": {
                    "content": "{\"results\":[{\"text\":\"Save\",\"x\":12,\"y\":20,\"width\":48,\"height\":18,\"confidence\":0.93}]}"
                }
            }]
        }"#;

        let results = RemoteOcrProvider::parse_openai_vision_response(response).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "Save");
        assert_eq!(results[0].x, 12);
        assert!((results[0].confidence - 0.93).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_openai_vision_response_line_fallback() {
        let response = r#"{
            "choices": [{
                "message": {
                    "content": "File\nEdit\nSave"
                }
            }]
        }"#;

        let results = RemoteOcrProvider::parse_openai_vision_response(response).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[2].text, "Save");
    }

    #[test]
    fn strategy_openai_request_uses_image_url_and_json_mode() {
        let payload =
            OcrProviderStrategy::OpenAi.build_request_body("ZmFrZS1pbWFnZQ==", "image/png", "gpt");
        assert_eq!(payload["response_format"]["type"], "json_object");
        assert_eq!(payload["messages"][0]["content"][1]["type"], "image_url");
        let url = payload["messages"][0]["content"][1]["image_url"]["url"]
            .as_str()
            .unwrap_or("");
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn remote_ocr_provider_info() {
        let response = r#"{"content": [{"type": "text", "text": "test\nline2"}]}"#;
        let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
        assert_eq!(results.len(), 2);
        assert!((results[0].confidence - 0.8).abs() < f64::EPSILON);
    }
}
