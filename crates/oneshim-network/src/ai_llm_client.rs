use async_trait::async_trait;
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
            .finish()
    }
}

impl RemoteLlmProvider {
    pub fn new(config: &ExternalApiEndpoint) -> Result<Self, CoreError> {
        if config.api_key.is_empty() {
            return Err(CoreError::Config(
                "AI LLM API key is not configured. Set it in Settings.".into(),
            ));
        }
        let credential = CredentialSource::ApiKey(config.api_key.clone());

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| CoreError::Network(format!("HTTP client create failure: {}", e)))?;

        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-5-20250929".to_string());

        match ai_model_lifecycle_policy::evaluate_model_lifecycle_now(config.provider_type, &model)?
        {
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

        debug!(
            endpoint = %config.endpoint,
            model = %model,
            timeout = config.timeout_secs,
            "RemoteLlmProvider initialize"
        );

        Ok(Self {
            http_client,
            endpoint: config.endpoint.clone(),
            credential,
            model,
            provider_type: config.provider_type,
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

        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-5-20250929".to_string());

        match ai_model_lifecycle_policy::evaluate_model_lifecycle_now(config.provider_type, &model)?
        {
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

        // Use OAuth-provided base URL when available (ChatGPT OAuth uses
        // a different endpoint than the standard OpenAI API).
        let endpoint = credential
            .api_base_url()
            .map(String::from)
            .unwrap_or_else(|| config.endpoint.clone());

        Ok(Self {
            http_client,
            endpoint,
            credential,
            model,
            provider_type: config.provider_type,
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
    /// Used when credential is ManagedOAuth (Codex CLI OAuth path).
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

    /// Whether this provider should use the OpenAI Responses API format.
    fn use_responses_api(&self) -> bool {
        self.credential.is_managed() && matches!(self.provider_type, AiProviderType::OpenAi)
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

        let request_body = match self.provider_type {
            AiProviderType::Anthropic => serde_json::json!({
                "model": self.model,
                "max_tokens": 512,
                "system": Self::system_prompt(),
                "messages": [{
                    "role": "user",
                    "content": user_prompt
                }]
            }),
            AiProviderType::Google => serde_json::json!({
                "contents": [{
                    "role": "user",
                    "parts": [{"text": user_prompt}]
                }],
                "system_instruction": {
                    "parts": [{"text": Self::system_prompt()}]
                },
                "generationConfig": {
                    "maxOutputTokens": 512
                }
            }),
            AiProviderType::OpenAi | AiProviderType::Generic => {
                serde_json::json!({
                    "model": self.model,
                    "max_tokens": 512,
                    "messages": [
                        {
                            "role": "system",
                            "content": Self::system_prompt()
                        },
                        {
                            "role": "user",
                            "content": user_prompt
                        }
                    ]
                })
            }
        };

        let mut builder = self
            .http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body);

        let bearer_token = self.credential.resolve_bearer_token().await?;
        match self.provider_type {
            AiProviderType::Anthropic => {
                builder = builder
                    .header("x-api-key", &bearer_token)
                    .header("anthropic-version", "2023-06-01");
            }
            AiProviderType::Google => {
                builder = builder.header("x-goog-api-key", &bearer_token);
            }
            AiProviderType::OpenAi | AiProviderType::Generic => {
                builder = builder.header("Authorization", format!("Bearer {}", bearer_token));
                // ChatGPT OAuth requires a version header for model access (GPT-5.4 etc.).
                // Ref: openai/codex codex-rs/core/src/model_provider_info.rs
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

        let action = match self.provider_type {
            AiProviderType::Anthropic => Self::parse_claude_response(&body)?,
            AiProviderType::Google => Self::parse_google_response(&body)?,
            AiProviderType::OpenAi | AiProviderType::Generic => Self::parse_openai_response(&body)?,
        };

        debug!(
            action_type = %action.action_type,
            target = ?action.target_text,
            confidence = action.confidence,
            "LLM intent interpretation completed"
        );

        Ok(action)
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
            responses_api = self.use_responses_api(),
            "Calling external LLM API (with skills)"
        );

        let request_body = if self.use_responses_api() {
            self.build_responses_api_body(&system_prompt, &user_prompt)
        } else {
            match self.provider_type {
                AiProviderType::Anthropic => serde_json::json!({
                    "model": self.model,
                    "max_tokens": 512,
                    "system": system_prompt,
                    "messages": [{
                        "role": "user",
                        "content": user_prompt
                    }]
                }),
                AiProviderType::Google => serde_json::json!({
                    "contents": [{
                        "role": "user",
                        "parts": [{"text": user_prompt}]
                    }],
                    "system_instruction": {
                        "parts": [{"text": system_prompt}]
                    },
                    "generationConfig": {
                        "maxOutputTokens": 512
                    }
                }),
                AiProviderType::OpenAi | AiProviderType::Generic => {
                    serde_json::json!({
                        "model": self.model,
                        "max_tokens": 512,
                        "messages": [
                            {"role": "system", "content": system_prompt},
                            {"role": "user", "content": user_prompt}
                        ]
                    })
                }
            }
        };

        let mut builder = self
            .http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body);

        let bearer_token = self.credential.resolve_bearer_token().await?;
        match self.provider_type {
            AiProviderType::Anthropic => {
                builder = builder
                    .header("x-api-key", &bearer_token)
                    .header("anthropic-version", "2023-06-01");
            }
            AiProviderType::Google => {
                builder = builder.header("x-goog-api-key", &bearer_token);
            }
            AiProviderType::OpenAi | AiProviderType::Generic => {
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

        let action = match self.provider_type {
            AiProviderType::Anthropic => Self::parse_claude_response(&body)?,
            AiProviderType::Google => Self::parse_google_response(&body)?,
            AiProviderType::OpenAi | AiProviderType::Generic => Self::parse_openai_response(&body)?,
        };

        debug!(
            action_type = %action.action_type,
            target = ?action.target_text,
            confidence = action.confidence,
            "LLM intent interpretation completed (with skills)"
        );

        Ok(action)
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
        };

        let result = RemoteLlmProvider::new(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("retired as of"));
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
            model: Some("gpt-4o".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
        };
        let provider = RemoteLlmProvider::new(&config).unwrap();
        let body = provider.build_responses_api_body("system prompt", "user input");

        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["instructions"], "system prompt");
        assert_eq!(body["input"], "user input");
        assert_eq!(body["max_output_tokens"], 512);
        // Responses API should NOT have "messages" field.
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn use_responses_api_only_for_managed_openai() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: "test-key".to_string(),
            model: Some("gpt-4o".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
        };
        // API key credential → should NOT use Responses API.
        let provider = RemoteLlmProvider::new(&config).unwrap();
        assert!(!provider.use_responses_api());
    }
}
