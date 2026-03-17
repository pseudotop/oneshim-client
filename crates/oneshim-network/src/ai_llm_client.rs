use async_trait::async_trait;
use oneshim_api_contracts::provider_specs::{
    self, ProviderAuthScheme, ProviderRequestShape, ProviderTransportKind,
};
use serde_json::Value;
use tracing::{debug, warn};

use oneshim_core::ai_model_lifecycle_policy::{self, ModelLifecycleDecision};
use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};
use oneshim_core::error::CoreError;
use oneshim_core::ports::credential_source::CredentialSource;
use oneshim_core::ports::llm_provider::{
    InterpretedAction, LlmProvider, ScreenContext, SkillContext,
};

/// - Claude (Anthropic): `POST /v1/messages`
pub struct RemoteLlmProvider {
    http_client: reqwest::Client,
    endpoint: String,
    credential: CredentialSource,
    model: String,
    provider_type: AiProviderType,
    surface_id: Option<String>,
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl std::fmt::Debug for RemoteLlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteLlmProvider")
            .field("endpoint", &self.endpoint)
            .field("credential", &self.credential)
            .field("model", &self.model)
            .field("provider_type", &self.provider_type)
            .field("surface_id", &self.surface_id)
            .finish()
    }
}

impl RemoteLlmProvider {
    fn fallback_llm_model(provider_type: AiProviderType) -> &'static str {
        match provider_type {
            AiProviderType::Anthropic => "claude-sonnet-4-20250514",
            AiProviderType::OpenAi => "gpt-5.4",
            AiProviderType::Google => "gemini-2.5-flash",
            AiProviderType::Ollama => "qwen3:8b",
            AiProviderType::Generic => "gpt-5-mini",
        }
    }

    fn resolved_runtime_endpoint(
        config: &ExternalApiEndpoint,
        model: &str,
    ) -> Result<String, CoreError> {
        let shape = provider_specs::resolved_request_shape(
            config.provider_type,
            config.surface_id.as_deref(),
            ProviderTransportKind::Llm,
        )
        .map_err(CoreError::Internal)?;

        Ok(match shape {
            ProviderRequestShape::GoogleGenerateContent => {
                Self::rewrite_google_generate_content_endpoint(&config.endpoint, model)
            }
            _ => config.endpoint.clone(),
        })
    }

    fn rewrite_google_generate_content_endpoint(endpoint: &str, model: &str) -> String {
        let model = model.trim();
        if model.is_empty() {
            return endpoint.to_string();
        }

        let Some(models_idx) = endpoint.find("/models/") else {
            return endpoint.to_string();
        };
        let prefix_end = models_idx + "/models/".len();
        let rest = &endpoint[prefix_end..];
        let Some(action_idx) = rest.find(':') else {
            return endpoint.to_string();
        };

        let prefix = &endpoint[..prefix_end];
        let suffix = &rest[action_idx..];
        format!("{prefix}{model}{suffix}")
    }

    pub fn new(config: &ExternalApiEndpoint) -> Result<Self, CoreError> {
        let auth_scheme = provider_specs::resolved_auth_scheme(
            config.provider_type,
            config.surface_id.as_deref(),
            ProviderTransportKind::Llm,
        )
        .map_err(CoreError::Internal)?;
        if !matches!(auth_scheme, ProviderAuthScheme::None) && config.api_key.is_empty() {
            return Err(CoreError::Config(
                "AI LLM API key is not configured. Set it in Settings.".into(),
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
            provider_specs::SurfaceCapabilityKind::Llm,
        )
        .map_err(CoreError::Internal)?;
        let model = config
            .model
            .clone()
            .or_else(|| {
                provider_specs::resolved_default_model(
                    config.provider_type,
                    config.surface_id.as_deref(),
                    provider_specs::SurfaceCapabilityKind::Llm,
                )
                .ok()
                .flatten()
            })
            .or_else(|| {
                if supports_model {
                    None
                } else {
                    Some(Self::fallback_llm_model(config.provider_type).to_string())
                }
            })
            .ok_or_else(|| {
                CoreError::Config(
                    "The selected LLM provider surface requires an explicit model selection."
                        .to_string(),
                )
            })?;
        if !supports_model {
            return Err(CoreError::Config(
                "The selected LLM provider surface does not support configurable model selection."
                    .to_string(),
            ));
        }

        match ai_model_lifecycle_policy::evaluate_model_lifecycle_now_for_surface(
            config.provider_type,
            config.surface_id.as_deref(),
            &model,
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
            provider_specs::SurfaceCapabilityKind::Llm,
            &model,
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
            provider_specs::SurfaceCapabilityKind::Llm,
            &model,
        )
        .map_err(CoreError::Config)?;

        debug!(
            endpoint = %config.endpoint,
            model = %model,
            timeout = config.timeout_secs,
            "RemoteLlmProvider initialize"
        );

        let endpoint = Self::resolved_runtime_endpoint(config, &model)?;

        Ok(Self {
            http_client,
            endpoint,
            credential,
            model,
            provider_type: config.provider_type,
            surface_id: config.surface_id.clone(),
            timeout_secs: config.timeout_secs,
        })
    }

    /// Create a provider with a managed credential source (e.g., OAuth).
    ///
    /// When the credential is `ManagedOAuth`, the API base URL from the
    /// credential is used instead of the config endpoint (ChatGPT OAuth
    /// uses `chatgpt.com/backend-api/codex`, not `api.openai.com/v1`).
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
            provider_specs::SurfaceCapabilityKind::Llm,
        )
        .map_err(CoreError::Internal)?;
        let model = config
            .model
            .clone()
            .or_else(|| {
                provider_specs::resolved_default_model(
                    config.provider_type,
                    config.surface_id.as_deref(),
                    provider_specs::SurfaceCapabilityKind::Llm,
                )
                .ok()
                .flatten()
            })
            .or_else(|| {
                if supports_model {
                    None
                } else {
                    Some(Self::fallback_llm_model(config.provider_type).to_string())
                }
            })
            .ok_or_else(|| {
                CoreError::Config(
                    "The selected LLM provider surface requires an explicit model selection."
                        .to_string(),
                )
            })?;
        if !supports_model {
            return Err(CoreError::Config(
                "The selected LLM provider surface does not support configurable model selection."
                    .to_string(),
            ));
        }

        match ai_model_lifecycle_policy::evaluate_model_lifecycle_now_for_surface(
            config.provider_type,
            config.surface_id.as_deref(),
            &model,
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
            provider_specs::SurfaceCapabilityKind::Llm,
            &model,
        )
        .map_err(CoreError::Config)?;

        // Use OAuth-provided base URL when available (ChatGPT OAuth uses
        // a different endpoint than the standard OpenAI API).
        let endpoint = credential
            .api_base_url()
            .map(String::from)
            .unwrap_or_else(|| {
                Self::resolved_runtime_endpoint(config, &model)
                    .unwrap_or_else(|_| config.endpoint.clone())
            });

        Ok(Self {
            http_client,
            endpoint,
            credential,
            model,
            provider_type: config.provider_type,
            surface_id: config.surface_id.clone(),
            timeout_secs: config.timeout_secs,
        })
    }

    fn system_prompt() -> &'static str {
        r#"You are a UI automation agent.
Interpret the user's intent and return which UI element to act on as JSON.

Response schema:
{
  "target_text": "Text to target (or null)",
  "target_role": "button, input, link, menu, etc. (or null)",
  "action_type": "one of click, type, hotkey, wait, activate",
  "confidence": "confidence between 0.0 and 1.0"
}

Decide based on visible screen text and the user intent.
Return JSON only."#
    }

    /// Build system prompt with optional skill context (progressive disclosure).
    fn build_system_prompt(skill_ctx: &SkillContext) -> String {
        let mut prompt = String::from(Self::system_prompt());

        if !skill_ctx.available_skills.is_empty() {
            prompt.push_str("\n\nAvailable skills:");
            for skill in &skill_ctx.available_skills {
                prompt.push_str(&format!("\n  - {}: {}", skill.name, skill.description));
            }
        }

        if let Some(ref body) = skill_ctx.active_skill_body {
            prompt.push_str("\n\n--- Active Skill ---\n");
            prompt.push_str(body);
            prompt.push_str("\n--- End Skill ---");
        }

        prompt
    }

    /// Build request body for OpenAI Responses API (`/v1/responses`).
    ///
    /// Used whenever the provider spec resolves to the Responses API.
    /// Managed OAuth also uses this path, but API-key OpenAI now does too.
    /// Ref: <https://platform.openai.com/docs/api-reference/responses>
    fn build_responses_api_body(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "model": self.model,
            "instructions": system_prompt,
            "input": user_prompt,
            "max_output_tokens": 512,
        })
    }

    fn llm_request_shape(&self) -> Result<ProviderRequestShape, CoreError> {
        provider_specs::resolved_request_shape(
            self.provider_type,
            self.surface_id.as_deref(),
            ProviderTransportKind::Llm,
        )
        .map_err(CoreError::Internal)
    }

    fn uses_responses_api(&self) -> bool {
        matches!(
            self.llm_request_shape(),
            Ok(ProviderRequestShape::OpenAiResponses)
        )
    }

    fn llm_auth_scheme(&self) -> Result<ProviderAuthScheme, CoreError> {
        provider_specs::resolved_auth_scheme(
            self.provider_type,
            self.surface_id.as_deref(),
            ProviderTransportKind::Llm,
        )
        .map_err(CoreError::Internal)
    }

    fn ensure_llm_parameters_supported(&self, parameters: &[&str]) -> Result<(), CoreError> {
        provider_specs::validate_supported_parameters(
            self.provider_type,
            self.surface_id.as_deref(),
            provider_specs::SurfaceCapabilityKind::Llm,
            parameters,
        )
        .map_err(CoreError::Internal)
    }

    /// Build the provider-specific request body given a system and user prompt.
    fn build_chat_body(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<serde_json::Value, CoreError> {
        let body = match self.llm_request_shape()? {
            ProviderRequestShape::AnthropicMessages
            | ProviderRequestShape::AnthropicVisionMessages => {
                self.ensure_llm_parameters_supported(&[
                    "model",
                    "max_tokens",
                    "system",
                    "messages",
                ])?;
                serde_json::json!({
                    "model": self.model,
                    "max_tokens": 512,
                    "system": system_prompt,
                    "messages": [{"role": "user", "content": user_prompt}]
                })
            }
            ProviderRequestShape::GoogleGenerateContent => {
                self.ensure_llm_parameters_supported(&[
                    "contents",
                    "system_instruction",
                    "generationConfig.maxOutputTokens",
                ])?;
                serde_json::json!({
                    "contents": [{"role": "user", "parts": [{"text": user_prompt}]}],
                    "system_instruction": {"parts": [{"text": system_prompt}]},
                    "generationConfig": {"maxOutputTokens": 512}
                })
            }
            ProviderRequestShape::OpenAiResponses => {
                self.ensure_llm_parameters_supported(&[
                    "model",
                    "instructions",
                    "input",
                    "max_output_tokens",
                ])?;
                self.build_responses_api_body(system_prompt, user_prompt)
            }
            ProviderRequestShape::OpenAiChatCompletions
            | ProviderRequestShape::OpenAiVisionChatCompletions => {
                self.ensure_llm_parameters_supported(&["model", "max_tokens", "messages"])?;
                serde_json::json!({
                    "model": self.model,
                    "max_tokens": 512,
                    "messages": [
                        {"role": "system", "content": system_prompt},
                        {"role": "user", "content": user_prompt}
                    ]
                })
            }
            ProviderRequestShape::GoogleVisionAnnotate => {
                return Err(CoreError::Internal(
                    "LLM transport shape resolved to OCR-only Google Vision Annotate".to_string(),
                ));
            }
        };
        Ok(body)
    }

    /// Send a request body to the LLM API, authenticate, and parse the response.
    async fn send_and_parse(
        &self,
        request_body: &serde_json::Value,
    ) -> Result<InterpretedAction, CoreError> {
        let mut builder = self
            .http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(request_body);

        match self.llm_auth_scheme()? {
            ProviderAuthScheme::None => {}
            ProviderAuthScheme::XApiKey => {
                let bearer_token = self.credential.resolve_bearer_token().await?;
                builder = builder
                    .header("x-api-key", &bearer_token)
                    .header("anthropic-version", "2023-06-01");
            }
            ProviderAuthScheme::XGoogApiKey => {
                let bearer_token = self.credential.resolve_bearer_token().await?;
                builder = builder.header("x-goog-api-key", &bearer_token);
            }
            ProviderAuthScheme::Bearer => {
                let bearer_token = self.credential.resolve_bearer_token().await?;
                builder = builder.header("Authorization", format!("Bearer {}", bearer_token));
                if self.credential.is_managed() {
                    builder = builder.header("version", env!("CARGO_PKG_VERSION"));
                }
            }
        }

        let response = builder
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("LLM API request failed: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| CoreError::Network(format!("LLM API response read failure: {}", e)))?;

        if !status.is_success() {
            warn!(status = %status, "LLM API error response");
            return Err(CoreError::Network(format!(
                "LLM API error ({}): {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        let action = match self.llm_request_shape()? {
            ProviderRequestShape::AnthropicMessages
            | ProviderRequestShape::AnthropicVisionMessages => Self::parse_claude_response(&body)?,
            ProviderRequestShape::GoogleGenerateContent => Self::parse_google_response(&body)?,
            ProviderRequestShape::OpenAiChatCompletions
            | ProviderRequestShape::OpenAiVisionChatCompletions
            | ProviderRequestShape::OpenAiResponses => Self::parse_openai_response(&body)?,
            ProviderRequestShape::GoogleVisionAnnotate => {
                return Err(CoreError::Internal(
                    "LLM transport shape resolved to OCR-only Google Vision Annotate".to_string(),
                ));
            }
        };

        debug!(
            action_type = %action.action_type,
            target = ?action.target_text,
            confidence = action.confidence,
            "LLM intent interpretation completed"
        );

        Ok(action)
    }

    fn build_user_prompt(screen_context: &ScreenContext, intent_hint: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str(&format!("Active app: {}\n", screen_context.active_app));
        prompt.push_str(&format!(
            "Window title: {}\n",
            screen_context.active_window_title
        ));

        if !screen_context.visible_texts.is_empty() {
            prompt.push_str("Visible screen text:\n");
            for text in &screen_context.visible_texts {
                prompt.push_str(&format!("  - {}\n", text));
            }
        }

        if let Some(layout) = &screen_context.layout_description {
            prompt.push_str(&format!("Layout: {}\n", layout));
        }

        prompt.push_str(&format!("\nUser intent: {}\n", intent_hint));
        prompt
    }

    fn parse_claude_response(body: &str) -> Result<InterpretedAction, CoreError> {
        let response: Value = serde_json::from_str(body).map_err(|e| {
            CoreError::Internal(format!("LLM Failed to parse response JSON: {}", e))
        })?;

        let text = response
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|block| block.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| CoreError::Internal("No text found in LLM response".to_string()))?;

        Self::parse_action_json(text)
    }

    fn parse_openai_response(body: &str) -> Result<InterpretedAction, CoreError> {
        let response: Value = serde_json::from_str(body).map_err(|e| {
            CoreError::Internal(format!("LLM Failed to parse response JSON: {}", e))
        })?;

        let text = Self::extract_openai_text(&response)
            .ok_or_else(|| CoreError::Internal("No text found in OpenAI response".to_string()))?;

        Self::parse_action_json(&text)
    }

    fn parse_google_response(body: &str) -> Result<InterpretedAction, CoreError> {
        let response: Value = serde_json::from_str(body).map_err(|e| {
            CoreError::Internal(format!("LLM Failed to parse response JSON: {}", e))
        })?;

        let text = response
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.as_array())
            .and_then(|parts| parts.first())
            .and_then(|part| part.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| CoreError::Internal("No text found in Google response".to_string()))?;

        Self::parse_action_json(text)
    }

    fn parse_action_json(text: &str) -> Result<InterpretedAction, CoreError> {
        let json_str = if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                &text[start..=end]
            } else {
                text
            }
        } else {
            text
        };

        serde_json::from_str(json_str).map_err(|e| {
            CoreError::Internal(format!(
                "Failed to parse InterpretedAction from LLM response: {} (raw: {})",
                e,
                json_str.chars().take(200).collect::<String>()
            ))
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
            if let Some(text) = Self::value_to_text(content) {
                return Some(text);
            }
        }

        if let Some(text) = response.get("output_text").and_then(|value| value.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

        let mut chunks = Vec::new();
        if let Some(outputs) = response.get("output").and_then(|value| value.as_array()) {
            for output in outputs {
                if let Some(content) = output.get("content").and_then(|value| value.as_array()) {
                    for part in content {
                        if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
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

    fn value_to_text(value: &Value) -> Option<String> {
        match value {
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            Value::Array(items) => {
                let mut chunks = Vec::new();
                for item in items {
                    if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            chunks.push(trimmed.to_string());
                        }
                    }
                }

                if chunks.is_empty() {
                    None
                } else {
                    Some(chunks.join("\n"))
                }
            }
            _ => None,
        }
    }
}

#[async_trait]
impl LlmProvider for RemoteLlmProvider {
    async fn interpret_intent(
        &self,
        screen_context: &ScreenContext,
        intent_hint: &str,
    ) -> Result<InterpretedAction, CoreError> {
        let user_prompt = Self::build_user_prompt(screen_context, intent_hint);

        debug!(
            endpoint = %self.endpoint,
            model = %self.model,
            hint = %intent_hint,
            "Calling external LLM API"
        );

        let request_body = self.build_chat_body(Self::system_prompt(), &user_prompt)?;
        self.send_and_parse(&request_body).await
    }

    async fn interpret_intent_with_skills(
        &self,
        screen_context: &ScreenContext,
        intent_hint: &str,
        skill_ctx: &SkillContext,
    ) -> Result<InterpretedAction, CoreError> {
        let user_prompt = Self::build_user_prompt(screen_context, intent_hint);
        let system_prompt = Self::build_system_prompt(skill_ctx);

        debug!(
            endpoint = %self.endpoint,
            model = %self.model,
            hint = %intent_hint,
            skills = skill_ctx.available_skills.len(),
            has_active_skill = skill_ctx.active_skill_body.is_some(),
            responses_api = self.uses_responses_api(),
            "Calling external LLM API (with skills)"
        );

        let request_body = self.build_chat_body(&system_prompt, &user_prompt)?;
        self.send_and_parse(&request_body).await
    }

    fn provider_name(&self) -> &str {
        &self.model
    }

    fn is_external(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::ExternalApiEndpoint;

    #[test]
    fn system_prompt_not_empty() {
        let prompt = RemoteLlmProvider::system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn new_remote_llm_rejects_retired_model_by_policy() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: "test-api-key".to_string(),
            model: Some("gpt-3.5-turbo".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        };

        let result = RemoteLlmProvider::new(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("retired as of"));
    }

    #[test]
    fn openai_llm_uses_spec_default_model() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: "test-api-key".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        };

        let provider = RemoteLlmProvider::new(&config).expect("provider should initialize");
        assert_eq!(provider.model, "gpt-5.4");
        assert_eq!(
            provider.llm_request_shape().expect("shape should resolve"),
            ProviderRequestShape::OpenAiResponses
        );
    }

    #[test]
    fn new_remote_llm_rejects_known_non_llm_model() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: "test-api-key".to_string(),
            model: Some("text-embedding-3-small".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            credential: None,
        };

        let result = RemoteLlmProvider::new(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not marked as LLM-capable"));
    }

    #[test]
    fn ollama_llm_initializes_without_api_key() {
        let config = ExternalApiEndpoint {
            endpoint: "http://localhost:11434/v1/responses".to_string(),
            api_key: String::new(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Ollama,
            surface_id: Some("provider_surface.ollama.local_http".to_string()),
            credential: None,
        };

        let provider = RemoteLlmProvider::new(&config).expect("ollama llm should initialize");
        assert_eq!(provider.model, "qwen3:8b");
        assert_eq!(
            provider.llm_request_shape().expect("shape should resolve"),
            ProviderRequestShape::OpenAiResponses
        );
    }

    #[test]
    fn google_llm_rewrites_endpoint_for_selected_model() {
        let config = ExternalApiEndpoint {
            endpoint: "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
                .to_string(),
            api_key: "goog-api-key".to_string(),
            model: Some("gemini-2.5-pro".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::Google,
            surface_id: Some("provider_surface.google.direct_api".to_string()),
            credential: None,
        };

        let provider = RemoteLlmProvider::new(&config).expect("google llm should initialize");
        assert_eq!(
            provider.endpoint,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
    }

    #[test]
    fn build_user_prompt_basic() {
        let ctx = ScreenContext {
            visible_texts: vec!["file".to_string(), "save".to_string()],
            active_app: "VSCode".to_string(),
            active_window_title: "main.rs".to_string(),
            layout_description: None,
        };
        let prompt = RemoteLlmProvider::build_user_prompt(&ctx, "click the save button");
        assert!(prompt.contains("VSCode"));
        assert!(prompt.contains("file"));
        assert!(prompt.contains("click the save button"));
    }

    #[test]
    fn build_user_prompt_with_layout() {
        let ctx = ScreenContext {
            visible_texts: vec![],
            active_app: "Chrome".to_string(),
            active_window_title: "Google".to_string(),
            layout_description: Some("Search bar is centered at the top".to_string()),
        };
        let prompt = RemoteLlmProvider::build_user_prompt(&ctx, "search");
        assert!(prompt.contains("Layout"));
        assert!(prompt.contains("Search bar is centered at the top"));
    }

    #[test]
    fn parse_claude_response_valid() {
        let body = r#"{
            "content": [{
                "type": "text",
                "text": "{\"target_text\": \"save\", \"target_role\": \"button\", \"action_type\": \"click\", \"confidence\": 0.92}"
            }]
        }"#;
        let action = RemoteLlmProvider::parse_claude_response(body).unwrap();
        assert_eq!(action.target_text.unwrap(), "save");
        assert_eq!(action.action_type, "click");
        assert!((action.confidence - 0.92).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_claude_response_with_markdown() {
        let body = r#"{
            "content": [{
                "type": "text",
                "text": "Analysis result:\n```json\n{\"target_text\": \"Confirm\", \"target_role\": null, \"action_type\": \"click\", \"confidence\": 0.85}\n```"
            }]
        }"#;
        let action = RemoteLlmProvider::parse_claude_response(body).unwrap();
        assert_eq!(action.target_text.unwrap(), "Confirm");
        assert_eq!(action.action_type, "click");
    }

    #[test]
    fn parse_openai_response_valid() {
        let body = r#"{
            "choices": [{
                "message": {
                    "content": "{\"target_text\": \"Submit\", \"target_role\": \"button\", \"action_type\": \"click\", \"confidence\": 0.88}"
                }
            }]
        }"#;
        let action = RemoteLlmProvider::parse_openai_response(body).unwrap();
        assert_eq!(action.target_text.unwrap(), "Submit");
        assert_eq!(action.target_role.unwrap(), "button");
    }

    #[test]
    fn parse_openai_response_with_content_array() {
        let body = r#"{
            "choices": [{
                "message": {
                    "content": [
                        {
                            "type": "text",
                            "text": "{\"target_text\": \"Apply\", \"target_role\": \"button\", \"action_type\": \"click\", \"confidence\": 0.74}"
                        }
                    ]
                }
            }]
        }"#;

        let action = RemoteLlmProvider::parse_openai_response(body).unwrap();
        assert_eq!(action.target_text.unwrap(), "Apply");
        assert_eq!(action.action_type, "click");
    }

    #[test]
    fn parse_openai_response_with_output_text() {
        let body = r#"{
            "output_text": "{\"target_text\": \"Save\", \"target_role\": \"button\", \"action_type\": \"click\", \"confidence\": 0.91}"
        }"#;

        let action = RemoteLlmProvider::parse_openai_response(body).unwrap();
        assert_eq!(action.target_text.unwrap(), "Save");
        assert_eq!(action.action_type, "click");
    }

    #[test]
    fn parse_claude_response_invalid_json() {
        let body = r#"{"content": [{"type": "text", "text": "not json at all"}]}"#;
        let result = RemoteLlmProvider::parse_claude_response(body);
        assert!(result.is_err());
    }

    #[test]
    fn parse_openai_response_no_choices() {
        let body = r#"{"choices": []}"#;
        let result = RemoteLlmProvider::parse_openai_response(body);
        assert!(result.is_err());
    }

    #[test]
    fn build_system_prompt_no_skills() {
        let ctx = SkillContext::default();
        let prompt = RemoteLlmProvider::build_system_prompt(&ctx);
        assert!(prompt.contains("UI automation agent"));
        assert!(!prompt.contains("Available skills"));
    }

    #[test]
    fn build_system_prompt_with_available_skills() {
        let ctx = SkillContext {
            available_skills: vec![
                oneshim_core::models::skill::SkillMeta {
                    name: "coding".into(),
                    description: "Write code".into(),
                },
                oneshim_core::models::skill::SkillMeta {
                    name: "review".into(),
                    description: "Review code".into(),
                },
            ],
            active_skill_body: None,
        };
        let prompt = RemoteLlmProvider::build_system_prompt(&ctx);
        assert!(prompt.contains("Available skills:"));
        assert!(prompt.contains("coding: Write code"));
        assert!(prompt.contains("review: Review code"));
        assert!(!prompt.contains("Active Skill"));
    }

    #[test]
    fn build_system_prompt_with_active_skill() {
        let ctx = SkillContext {
            available_skills: vec![],
            active_skill_body: Some("# Do the thing\nStep 1: click.".into()),
        };
        let prompt = RemoteLlmProvider::build_system_prompt(&ctx);
        assert!(prompt.contains("--- Active Skill ---"));
        assert!(prompt.contains("Do the thing"));
        assert!(prompt.contains("--- End Skill ---"));
    }

    #[test]
    fn responses_api_body_format() {
        let config = ExternalApiEndpoint {
            endpoint: "https://chatgpt.com/backend-api/codex".to_string(),
            api_key: "test-key".to_string(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        };
        let provider = RemoteLlmProvider::new(&config).unwrap();
        let body = provider.build_responses_api_body("system prompt", "user input");

        assert_eq!(body["model"], "gpt-5.4");
        assert_eq!(body["instructions"], "system prompt");
        assert_eq!(body["input"], "user input");
        assert_eq!(body["max_output_tokens"], 512);
        // Responses API should NOT have "messages" field.
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn openai_llm_uses_responses_api_from_spec() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: "test-key".to_string(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        };
        let provider = RemoteLlmProvider::new(&config).unwrap();
        assert!(provider.uses_responses_api());
    }

    #[test]
    fn managed_openai_surface_uses_surface_shape() {
        let config = ExternalApiEndpoint {
            endpoint: "https://chatgpt.com/backend-api/codex".to_string(),
            api_key: "test-key".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.managed_oauth".to_string()),
            credential: None,
        };
        let provider = RemoteLlmProvider::new(&config).unwrap();
        assert_eq!(provider.model, "gpt-5.4");
        assert_eq!(
            provider.llm_request_shape().expect("shape should resolve"),
            ProviderRequestShape::OpenAiResponses
        );
    }

    #[test]
    fn local_openai_compatible_llm_requires_explicit_model_selection() {
        let config = ExternalApiEndpoint {
            endpoint: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Generic,
            surface_id: Some("provider_surface.generic.local_openai_compatible".to_string()),
            credential: None,
        };
        let result = RemoteLlmProvider::new(&config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("requires an explicit model selection"));
    }
}
