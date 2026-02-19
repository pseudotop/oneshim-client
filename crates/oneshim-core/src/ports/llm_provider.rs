//! LLM 제공자 포트.
//!
//! 의도 해석 및 복잡한 UI 이해를 위한 LLM 인터페이스를 정의한다.
//! LLM에는 이미지를 전송하지 않으며, 오직 세정된 텍스트만 전달한다.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// LLM에 전달할 화면 컨텍스트 (이미지 제외 — 텍스트만)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenContext {
    /// OCR로 추출한 텍스트 목록 (PII 필터 적용 후)
    pub visible_texts: Vec<String>,
    /// 활성 앱 이름
    pub active_app: String,
    /// 활성 창 제목
    pub active_window_title: String,
    /// UI 레이아웃 설명 (선택)
    pub layout_description: Option<String>,
}

/// LLM 해석 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpretedAction {
    /// 클릭 대상 텍스트
    pub target_text: Option<String>,
    /// 대상 역할 (button, input 등)
    pub target_role: Option<String>,
    /// 수행할 액션 종류 ("click", "type", "hotkey")
    pub action_type: String,
    /// 신뢰도 (0.0 ~ 1.0)
    pub confidence: f64,
}

/// LLM 제공자 — 의도 해석 및 복잡한 UI 이해
///
/// 구현체: `LocalLlmProvider` (규칙 기반), `RemoteLlmProvider` (Claude API 등)
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 화면 컨텍스트 기반 의도 해석 (텍스트만 전달, 이미지 없음)
    async fn interpret_intent(
        &self,
        screen_context: &ScreenContext,
        intent_hint: &str,
    ) -> Result<InterpretedAction, CoreError>;

    /// 제공자 이름 (예: "local-llm", "claude-api", "openai-api")
    fn provider_name(&self) -> &str;

    /// 외부 API인지 여부
    fn is_external(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_context_serde() {
        let ctx = ScreenContext {
            visible_texts: vec!["파일".to_string(), "편집".to_string()],
            active_app: "Visual Studio Code".to_string(),
            active_window_title: "main.rs — VSCode".to_string(),
            layout_description: None,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let deser: ScreenContext = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.visible_texts.len(), 2);
        assert_eq!(deser.active_app, "Visual Studio Code");
    }

    #[test]
    fn interpreted_action_serde() {
        let action = InterpretedAction {
            target_text: Some("저장".to_string()),
            target_role: Some("button".to_string()),
            action_type: "click".to_string(),
            confidence: 0.85,
        };
        let json = serde_json::to_string(&action).unwrap();
        let deser: InterpretedAction = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.target_text.unwrap(), "저장");
        assert!((deser.confidence - 0.85).abs() < f64::EPSILON);
    }
}
