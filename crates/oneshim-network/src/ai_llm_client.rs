//! 외부 AI LLM 클라이언트.
//!
//! 외부 AI API (Claude, GPT 등)를 호출하여 UI 자동화 의도를 해석한다.
//! **중요**: LLM에는 이미지를 전송하지 않으며, 오직 세정된 텍스트만 전달한다.
//! Privacy Gateway를 통해 텍스트 PII 필터가 적용된 후 전송된다.

use async_trait::async_trait;
use tracing::{debug, warn};

use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};
use oneshim_core::error::CoreError;
use oneshim_core::ports::llm_provider::{InterpretedAction, LlmProvider, ScreenContext};

// ============================================================
// RemoteLlmProvider — 외부 AI LLM 클라이언트
// ============================================================

/// 외부 AI LLM 클라이언트 — 의도 해석용
///
/// 지원 API:
/// - Claude (Anthropic): `POST /v1/messages`
/// - OpenAI 호환: `POST /v1/chat/completions`
/// - 커스텀 엔드포인트
///
/// **보안**:
/// - API 키는 config.json에서 로드 → Settings UI에서 입력
/// - 이미지 전송 금지 — 텍스트만 전달
/// - PII 필터 적용 후 전송
#[derive(Debug)]
pub struct RemoteLlmProvider {
    /// HTTP 클라이언트
    http_client: reqwest::Client,
    /// API 엔드포인트 URL
    endpoint: String,
    /// API 키 (메모리에만 유지)
    api_key: String,
    /// 모델 이름
    model: String,
    /// AI 제공자 타입 — 요청/응답 형식 결정에 사용
    provider_type: AiProviderType,
    /// 요청 타임아웃 (초) — 향후 동적 타임아웃 조정용
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl RemoteLlmProvider {
    /// 새 RemoteLlmProvider 생성
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
            .map_err(|e| CoreError::Network(format!("HTTP 클라이언트 생성 실패: {}", e)))?;

        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-5-20250929".to_string());

        debug!(
            endpoint = %config.endpoint,
            model = %model,
            timeout = config.timeout_secs,
            "RemoteLlmProvider 초기화"
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

    /// 시스템 프롬프트 생성 (UI 자동화 에이전트 역할)
    fn system_prompt() -> &'static str {
        r#"당신은 UI 자동화 에이전트입니다.
사용자의 의도를 해석하여 어떤 UI 요소를 조작해야 하는지 JSON으로 응답하세요.

응답 형식:
{
  "target_text": "클릭할 텍스트 (없으면 null)",
  "target_role": "button, input, link, menu 등 (없으면 null)",
  "action_type": "click, type, hotkey, wait, activate 중 하나",
  "confidence": 0.0~1.0 사이 신뢰도
}

화면에 보이는 텍스트 목록과 사용자 의도를 기반으로 판단하세요.
반드시 JSON만 응답하세요."#
    }

    /// 사용자 프롬프트 구성
    fn build_user_prompt(screen_context: &ScreenContext, intent_hint: &str) -> String {
        let mut prompt = String::new();
        prompt.push_str(&format!("활성 앱: {}\n", screen_context.active_app));
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

    /// Claude API 응답 파싱
    fn parse_claude_response(body: &str) -> Result<InterpretedAction, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::Internal(format!("LLM 응답 JSON 파싱 실패: {}", e)))?;

        // content[0].text에서 JSON 추출
        let text = response
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|block| block.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| CoreError::Internal("LLM 응답에서 텍스트를 찾을 수 없음".to_string()))?;

        // JSON 블록 추출 (마크다운 코드 블록 처리)
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
                "LLM 응답 InterpretedAction 파싱 실패: {} (raw: {})",
                e,
                json_str.chars().take(200).collect::<String>()
            ))
        })?;

        Ok(action)
    }

    /// OpenAI API 응답 파싱
    fn parse_openai_response(body: &str) -> Result<InterpretedAction, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::Internal(format!("LLM 응답 JSON 파싱 실패: {}", e)))?;

        // choices[0].message.content에서 JSON 추출
        let text = response
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                CoreError::Internal("OpenAI 응답에서 텍스트를 찾을 수 없음".to_string())
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
            .map_err(|e| CoreError::Internal(format!("OpenAI 응답 파싱 실패: {}", e)))
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

        // 제공자 타입에 따라 요청 형식 결정
        let is_anthropic = self.provider_type == AiProviderType::Anthropic;

        let request_body = if is_anthropic {
            serde_json::json!({
                "model": self.model,
                "max_tokens": 512,
                "system": Self::system_prompt(),
                "messages": [{
                    "role": "user",
                    "content": user_prompt
                }]
            })
        } else {
            // OpenAI 호환 형식
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
        };

        // API 호출
        let mut builder = self
            .http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body);

        if is_anthropic {
            builder = builder
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01");
        } else {
            builder = builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = builder
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("LLM API 호출 실패: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| CoreError::Network(format!("LLM API 응답 읽기 실패: {}", e)))?;

        if !status.is_success() {
            warn!(status = %status, "LLM API 오류 응답");
            return Err(CoreError::Network(format!(
                "LLM API 오류 ({}): {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        // 응답 파싱
        let action = if is_anthropic {
            Self::parse_claude_response(&body)?
        } else {
            Self::parse_openai_response(&body)?
        };

        debug!(
            action_type = %action.action_type,
            target = ?action.target_text,
            confidence = action.confidence,
            "LLM 의도 해석 완료"
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

// ============================================================
// 테스트
// ============================================================

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
            visible_texts: vec!["파일".to_string(), "저장".to_string()],
            active_app: "VSCode".to_string(),
            active_window_title: "main.rs".to_string(),
            layout_description: None,
        };
        let prompt = RemoteLlmProvider::build_user_prompt(&ctx, "저장 버튼 클릭");
        assert!(prompt.contains("VSCode"));
        assert!(prompt.contains("파일"));
        assert!(prompt.contains("저장 버튼 클릭"));
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
                "text": "{\"target_text\": \"저장\", \"target_role\": \"button\", \"action_type\": \"click\", \"confidence\": 0.92}"
            }]
        }"#;
        let action = RemoteLlmProvider::parse_claude_response(body).unwrap();
        assert_eq!(action.target_text.unwrap(), "저장");
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
