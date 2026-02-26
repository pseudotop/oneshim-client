//!

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, warn};

use oneshim_core::ai_model_lifecycle_policy::{self, ModelLifecycleDecision};
use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};
use oneshim_core::error::CoreError;
use oneshim_core::ports::llm_provider::{InterpretedAction, LlmProvider, ScreenContext};

///
/// - Claude (Anthropic): `POST /v1/messages`
///
#[derive(Debug)]
pub struct RemoteLlmProvider {
    http_client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    provider_type: AiProviderType,
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl RemoteLlmProvider {
    pub fn new(config: &ExternalApiEndpoint) -> Result<Self, CoreError> {
        if config.api_key.is_empty() {
            return Err(CoreError::Config(
                "AI LLM API key is not configured. Set it in Settings.".into(),
            ));
        }
        let api_key = config.api_key.clone();

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
            api_key,
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

        match self.provider_type {
            AiProviderType::Anthropic => {
                builder = builder
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01");
            }
            AiProviderType::Google => {
                builder = builder.header("x-goog-api-key", &self.api_key);
            }
            AiProviderType::OpenAi | AiProviderType::Generic => {
                builder = builder.header("Authorization", format!("Bearer {}", self.api_key));
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
}
