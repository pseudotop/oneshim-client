//!

use async_trait::async_trait;
use tracing::{debug, warn};

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
                "AI LLM API 키 미설정. Settings에서 입력하세요.".into(),
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
        r#"당신은 UI 자동화 에이전트입니다.
사용자의 의도를 해석하여 어떤 UI 요소를 조작해야 하는지 JSON으로 response하세요.

response 형식:
{
  "target_text": "클릭할 텍스트 (없으면 null)",
  "target_role": "button, input, link, menu 등 (없으면 null)",
  "action_type": "click, type, hotkey, wait, activate 중 하나",
  "confidence": 0.0~1.0 사이 신뢰도
}

화면에 보이는 텍스트 list과 사용자 의도를 기반으로 판단하세요.
반드시 JSON만 response하세요."#
    }

    fn build_user_prompt(screen_context: &ScreenContext, intent_hint: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str(&format!("active 앱: {}\n", screen_context.active_app));
        prompt.push_str(&format!(
            "창 제목: {}\n",
            screen_context.active_window_title
        ));

        if !screen_context.visible_texts.is_empty() {
            prompt.push_str("화면 텍스트:\n");
            for text in &screen_context.visible_texts {
                prompt.push_str(&format!("  - {}\n", text));
            }
        }

        if let Some(layout) = &screen_context.layout_description {
            prompt.push_str(&format!("레이아웃: {}\n", layout));
        }

        prompt.push_str(&format!("\n사용자 의도: {}\n", intent_hint));
        prompt
    }

    fn parse_claude_response(body: &str) -> Result<InterpretedAction, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::Internal(format!("LLM response JSON 파싱 failure: {}", e)))?;

        let text = response
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|block| block.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| CoreError::Internal("LLM response에서 텍스트를 찾을 수 none".to_string()))?;

        let json_str = if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                &text[start..=end]
            } else {
                text
            }
        } else {
            text
        };

        let action: InterpretedAction = serde_json::from_str(json_str).map_err(|e| {
            CoreError::Internal(format!(
                "LLM response InterpretedAction 파싱 failure: {} (raw: {})",
                e,
                json_str.chars().take(200).collect::<String>()
            ))
        })?;

        Ok(action)
    }

    fn parse_openai_response(body: &str) -> Result<InterpretedAction, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::Internal(format!("LLM response JSON 파싱 failure: {}", e)))?;

        let text = response
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                CoreError::Internal("OpenAI response에서 텍스트를 찾을 수 none".to_string())
            })?;

        let json_str = if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                &text[start..=end]
            } else {
                text
            }
        } else {
            text
        };

        serde_json::from_str(json_str)
            .map_err(|e| CoreError::Internal(format!("OpenAI response 파싱 failure: {}", e)))
    }

    fn parse_google_response(body: &str) -> Result<InterpretedAction, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::Internal(format!("LLM response JSON 파싱 failure: {}", e)))?;

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
            .ok_or_else(|| {
                CoreError::Internal("Google response에서 텍스트를 찾을 수 none".to_string())
            })?;

        let json_str = if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                &text[start..=end]
            } else {
                text
            }
        } else {
            text
        };

        serde_json::from_str(json_str)
            .map_err(|e| CoreError::Internal(format!("Google response 파싱 failure: {}", e)))
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
            "외부 LLM API 호출"
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
            .map_err(|e| CoreError::Network(format!("LLM API 호출 failure: {}", e)))?;

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
            "LLM 의도 해석 completed"
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

    #[test]
    fn system_prompt_not_empty() {
        let prompt = RemoteLlmProvider::system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn build_user_prompt_basic() {
        let ctx = ScreenContext {
            visible_texts: vec!["file".to_string(), "save".to_string()],
            active_app: "VSCode".to_string(),
            active_window_title: "main.rs".to_string(),
            layout_description: None,
        };
        let prompt = RemoteLlmProvider::build_user_prompt(&ctx, "save 버튼 클릭");
        assert!(prompt.contains("VSCode"));
        assert!(prompt.contains("file"));
        assert!(prompt.contains("save 버튼 클릭"));
    }

    #[test]
    fn build_user_prompt_with_layout() {
        let ctx = ScreenContext {
            visible_texts: vec![],
            active_app: "Chrome".to_string(),
            active_window_title: "Google".to_string(),
            layout_description: Some("검색바가 상단 중앙에 위치".to_string()),
        };
        let prompt = RemoteLlmProvider::build_user_prompt(&ctx, "검색");
        assert!(prompt.contains("레이아웃"));
        assert!(prompt.contains("검색바가 상단 중앙에 위치"));
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
                "text": "분석 결과:\n```json\n{\"target_text\": \"확인\", \"target_role\": null, \"action_type\": \"click\", \"confidence\": 0.85}\n```"
            }]
        }"#;
        let action = RemoteLlmProvider::parse_claude_response(body).unwrap();
        assert_eq!(action.target_text.unwrap(), "확인");
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
