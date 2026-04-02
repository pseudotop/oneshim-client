use async_trait::async_trait;
use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};
use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::{Priority, Suggestion, SuggestionSource, SuggestionType};
use oneshim_core::ports::analysis_provider::AnalysisProvider;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::error::NetworkError;

/// Adapter implementing `AnalysisProvider` by calling a remote LLM API.
/// Reuses the same multi-provider HTTP pattern as `RemoteLlmProvider`.
pub struct AnalysisClient {
    http_client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    provider_type: AiProviderType,
    timeout_secs: u64,
}

/// Private struct for parsing LLM suggestion candidates from JSON.
#[derive(Debug, Deserialize)]
struct SuggestionCandidate {
    #[serde(rename = "type")]
    suggestion_type: String,
    content: String,
    confidence: f64,
    #[serde(default)]
    reasoning: Option<String>,
}

impl AnalysisClient {
    pub fn new(config: &ExternalApiEndpoint) -> Self {
        if !matches!(config.provider_type, AiProviderType::Ollama) && config.api_key.is_empty() {
            warn!(
                "AnalysisClient: empty API key for {:?} provider",
                config.provider_type
            );
        }

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .unwrap_or_default();

        Self {
            http_client,
            endpoint: config.endpoint.clone(),
            api_key: config.api_key.clone(),
            model: config.model.clone().unwrap_or_else(|| {
                crate::default_model_for_provider(&config.provider_type).to_string()
            }),
            provider_type: config.provider_type,
            timeout_secs: config.timeout_secs,
        }
    }

    /// Build provider-specific request body.
    fn build_request_body(&self, context_json: &str, system_prompt: &str) -> serde_json::Value {
        match self.provider_type {
            AiProviderType::Anthropic => {
                serde_json::json!({
                    "model": self.model,
                    "max_tokens": 1024,
                    "system": system_prompt,
                    "messages": [{"role": "user", "content": context_json}]
                })
            }
            AiProviderType::OpenAi
            | AiProviderType::Google
            | AiProviderType::Ollama
            | AiProviderType::Bedrock
            | AiProviderType::Copilot
            | AiProviderType::Generic => {
                serde_json::json!({
                    "model": self.model,
                    "messages": [
                        {"role": "system", "content": system_prompt},
                        {"role": "user", "content": context_json}
                    ],
                    "max_tokens": 1024,
                    "temperature": 0.3
                })
            }
        }
    }

    /// Extract text content from provider-specific response format.
    fn extract_text(&self, body: &serde_json::Value) -> Result<String, NetworkError> {
        match self.provider_type {
            AiProviderType::Anthropic => body
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|block| block.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| NetworkError::Analysis("No text in Anthropic response".to_string())),
            _ => {
                // OpenAI / Generic / Ollama: choices[0].message.content
                body.get("choices")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|choice| choice.get("message"))
                    .and_then(|msg| msg.get("content"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        NetworkError::Analysis("No text in OpenAI/Generic response".to_string())
                    })
            }
        }
    }

    /// Parse candidates from JSON text extracted from LLM response.
    fn parse_candidates(text: &str) -> Result<Vec<SuggestionCandidate>, NetworkError> {
        // Strip markdown fences if present
        let trimmed = text.trim();
        let json_str = if trimmed.starts_with("```") {
            let inner = trimmed
                .strip_prefix("```json")
                .or_else(|| trimmed.strip_prefix("```"))
                .unwrap_or(trimmed);
            inner.strip_suffix("```").unwrap_or(inner).trim()
        } else {
            trimmed
        };

        // Find the JSON array in the text
        let start = json_str.find('[').ok_or_else(|| {
            NetworkError::Analysis(format!(
                "No JSON array found in LLM response: {}",
                json_str.chars().take(200).collect::<String>()
            ))
        })?;
        let end = json_str.rfind(']').ok_or_else(|| {
            NetworkError::Analysis("No closing bracket in LLM response".to_string())
        })?;

        let array_str = &json_str[start..=end];
        serde_json::from_str(array_str).map_err(|e| {
            NetworkError::Analysis(format!(
                "Failed to parse suggestion candidates: {} (raw: {})",
                e,
                array_str.chars().take(200).collect::<String>()
            ))
        })
    }

    /// Convert a parsed candidate into a domain `Suggestion`.
    fn candidate_to_suggestion(candidate: SuggestionCandidate) -> Suggestion {
        let suggestion_type = match candidate.suggestion_type.as_str() {
            "ProductivityTip" => SuggestionType::ProductivityTip,
            "WorkflowOptimization" => SuggestionType::WorkflowOptimization,
            "ContextBased" => SuggestionType::ContextBased,
            "WorkGuidance" => SuggestionType::WorkGuidance,
            _ => SuggestionType::ContextBased,
        };

        let priority = if candidate.confidence >= 0.9 {
            Priority::High
        } else if candidate.confidence >= 0.7 {
            Priority::Medium
        } else {
            Priority::Low
        };

        Suggestion {
            suggestion_id: uuid::Uuid::new_v4().to_string(),
            suggestion_type,
            content: candidate.content,
            priority,
            confidence_score: candidate.confidence,
            relevance_score: candidate.confidence,
            is_actionable: true,
            created_at: chrono::Utc::now(),
            expires_at: None,
            source: SuggestionSource::LlmLocal,
            reasoning: candidate.reasoning,
        }
    }
}

#[async_trait]
impl AnalysisProvider for AnalysisClient {
    async fn analyze(
        &self,
        context_json: &str,
        system_prompt: &str,
    ) -> Result<Vec<Suggestion>, CoreError> {
        debug!(
            endpoint = %self.endpoint,
            model = %self.model,
            provider = ?self.provider_type,
            timeout = self.timeout_secs,
            "Calling analysis LLM API"
        );

        let body = self.build_request_body(context_json, system_prompt);

        let mut builder = self
            .http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&body);

        // Set auth headers based on provider type
        match self.provider_type {
            AiProviderType::Anthropic => {
                builder = builder
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", crate::ANTHROPIC_API_VERSION);
            }
            _ => {
                if !self.api_key.is_empty() {
                    builder = builder.header("Authorization", format!("Bearer {}", self.api_key));
                }
            }
        }

        let response = builder
            .send()
            .await
            .map_err(|e| NetworkError::Analysis(format!("Analysis API request failed: {}", e)))?;

        let status = response.status();
        let response_text = response.text().await.map_err(|e| {
            NetworkError::Analysis(format!("Failed to read analysis response: {}", e))
        })?;

        if !status.is_success() {
            warn!(status = %status, "Analysis API error response");
            return Err(NetworkError::Analysis(format!(
                "Analysis API error ({}): {}",
                status,
                response_text.chars().take(200).collect::<String>()
            ))
            .into());
        }

        let response_json: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| NetworkError::Analysis(format!("Invalid JSON response: {}", e)))?;

        let text = self.extract_text(&response_json)?;
        let candidates = Self::parse_candidates(&text)?;

        debug!(
            candidate_count = candidates.len(),
            "Parsed suggestion candidates"
        );

        let suggestions: Vec<Suggestion> = candidates
            .into_iter()
            .map(Self::candidate_to_suggestion)
            .collect();

        Ok(suggestions)
    }

    /// Efficient single-completion call that returns raw text without JSON parsing.
    async fn summarize_text(
        &self,
        context_json: &str,
        system_prompt: &str,
    ) -> Result<String, CoreError> {
        debug!(
            endpoint = %self.endpoint,
            model = %self.model,
            provider = ?self.provider_type,
            "Calling summarize_text LLM API"
        );

        let body = self.build_request_body(context_json, system_prompt);

        let mut builder = self
            .http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&body);

        match self.provider_type {
            AiProviderType::Anthropic => {
                builder = builder
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", crate::ANTHROPIC_API_VERSION);
            }
            _ => {
                if !self.api_key.is_empty() {
                    builder = builder.header("Authorization", format!("Bearer {}", self.api_key));
                }
            }
        }

        let response = builder
            .send()
            .await
            .map_err(|e| NetworkError::Analysis(format!("Summarize API request failed: {}", e)))?;

        let status = response.status();
        let response_text = response.text().await.map_err(|e| {
            NetworkError::Analysis(format!("Failed to read summary response: {}", e))
        })?;

        if !status.is_success() {
            warn!(status = %status, "Summarize API error response");
            return Err(NetworkError::Analysis(format!(
                "Summarize API error ({}): {}",
                status,
                response_text.chars().take(200).collect::<String>()
            ))
            .into());
        }

        let response_json: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| NetworkError::Analysis(format!("Invalid JSON response: {}", e)))?;

        let text = self.extract_text(&response_json)?;

        if text.trim().is_empty() {
            return Err(NetworkError::Analysis("Empty summary response".into()).into());
        }

        Ok(text.trim().to_string())
    }

    fn provider_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_json_response() {
        let text = r#"[
            {"type": "ProductivityTip", "content": "Take a break", "confidence": 0.85, "reasoning": "Long session"},
            {"type": "WorkflowOptimization", "content": "Batch emails", "confidence": 0.72}
        ]"#;

        let candidates = AnalysisClient::parse_candidates(text).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].suggestion_type, "ProductivityTip");
        assert_eq!(candidates[0].content, "Take a break");
        assert!((candidates[0].confidence - 0.85).abs() < f64::EPSILON);
        assert_eq!(candidates[0].reasoning.as_deref(), Some("Long session"));
        assert!(candidates[1].reasoning.is_none());
    }

    #[test]
    fn parse_malformed_response_returns_error() {
        let text = "This is not JSON at all";
        let result = AnalysisClient::parse_candidates(text);
        assert!(result.is_err());
        match result.unwrap_err() {
            NetworkError::Analysis(msg) => {
                assert!(msg.contains("No JSON array found"));
            }
            other => panic!("Expected NetworkError::Analysis, got: {:?}", other),
        }
    }

    #[test]
    fn parse_empty_array_returns_empty_vec() {
        let text = "[]";
        let candidates = AnalysisClient::parse_candidates(text).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn parse_json_with_markdown_fences() {
        let text = r#"```json
[{"type": "ContextBased", "content": "Focus on current task", "confidence": 0.9}]
```"#;
        let candidates = AnalysisClient::parse_candidates(text).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].suggestion_type, "ContextBased");
    }

    #[test]
    fn candidate_to_suggestion_maps_fields() {
        let candidate = SuggestionCandidate {
            suggestion_type: "ProductivityTip".to_string(),
            content: "Take a break after 90 minutes".to_string(),
            confidence: 0.85,
            reasoning: Some("Extended focus session detected".to_string()),
        };

        let suggestion = AnalysisClient::candidate_to_suggestion(candidate);
        assert_eq!(suggestion.suggestion_type, SuggestionType::ProductivityTip);
        assert_eq!(suggestion.content, "Take a break after 90 minutes");
        assert!((suggestion.confidence_score - 0.85).abs() < f64::EPSILON);
        assert_eq!(suggestion.source, SuggestionSource::LlmLocal);
        assert_eq!(suggestion.priority, Priority::Medium);
        assert!(suggestion.reasoning.is_some());
    }

    #[test]
    fn candidate_high_confidence_gets_high_priority() {
        let candidate = SuggestionCandidate {
            suggestion_type: "WorkGuidance".to_string(),
            content: "Critical".to_string(),
            confidence: 0.95,
            reasoning: None,
        };
        let suggestion = AnalysisClient::candidate_to_suggestion(candidate);
        assert_eq!(suggestion.priority, Priority::High);
    }

    #[test]
    fn candidate_low_confidence_gets_low_priority() {
        let candidate = SuggestionCandidate {
            suggestion_type: "ContextBased".to_string(),
            content: "Maybe try this".to_string(),
            confidence: 0.62,
            reasoning: None,
        };
        let suggestion = AnalysisClient::candidate_to_suggestion(candidate);
        assert_eq!(suggestion.priority, Priority::Low);
    }

    #[test]
    fn unknown_suggestion_type_defaults_to_context_based() {
        let candidate = SuggestionCandidate {
            suggestion_type: "UnknownType".to_string(),
            content: "test".to_string(),
            confidence: 0.7,
            reasoning: None,
        };
        let suggestion = AnalysisClient::candidate_to_suggestion(candidate);
        assert_eq!(suggestion.suggestion_type, SuggestionType::ContextBased);
    }

    #[test]
    fn build_anthropic_request_body() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.anthropic.com/v1/messages".to_string(),
            api_key: "test-key".to_string(),
            model: Some("claude-sonnet-4-20250514".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::Anthropic,
            surface_id: None,
            credential: None,
        };
        let client = AnalysisClient::new(&config);
        let body = client.build_request_body("ctx", "sys");

        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["system"], "sys");
        assert_eq!(body["max_tokens"], 1024);
        assert!(body["messages"].is_array());
    }

    #[test]
    fn build_openai_request_body() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: "test-key".to_string(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        };
        let client = AnalysisClient::new(&config);
        let body = client.build_request_body("ctx", "sys");

        assert_eq!(body["model"], "gpt-5.4");
        assert_eq!(body["temperature"], 0.3);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn extract_text_anthropic_format() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.anthropic.com/v1/messages".to_string(),
            api_key: "key".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Anthropic,
            surface_id: None,
            credential: None,
        };
        let client = AnalysisClient::new(&config);
        let body = serde_json::json!({
            "content": [{"type": "text", "text": "[{\"type\": \"ProductivityTip\", \"content\": \"test\", \"confidence\": 0.8}]"}]
        });
        let text = client.extract_text(&body).unwrap();
        assert!(text.contains("ProductivityTip"));
    }

    #[test]
    fn extract_text_openai_format() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: "key".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        };
        let client = AnalysisClient::new(&config);
        let body = serde_json::json!({
            "choices": [{"message": {"content": "[]"}}]
        });
        let text = client.extract_text(&body).unwrap();
        assert_eq!(text, "[]");
    }

    #[test]
    fn extract_text_missing_content_returns_error() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: "key".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        };
        let client = AnalysisClient::new(&config);
        let body = serde_json::json!({"choices": []});
        assert!(client.extract_text(&body).is_err());
    }
}
