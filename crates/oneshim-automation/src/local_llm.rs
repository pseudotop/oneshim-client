//! 로컬 LLM 제공자 — 규칙 기반 매칭.
//!
//! LLM 없이도 기본적인 의도 해석이 동작하도록 규칙 기반 매칭을 제공한다.
//! 향후 candle/llama.cpp 바인딩으로 확장 예정.

use async_trait::async_trait;

use oneshim_core::error::CoreError;
use oneshim_core::ports::llm_provider::{InterpretedAction, LlmProvider, ScreenContext};

// ============================================================
// LocalLlmProvider — 규칙 기반 의도 해석
// ============================================================

/// 로컬 LLM 제공자 (규칙 기반 매칭)
///
/// 외부 API 없이 intent_hint에서 키워드를 추출하여
/// 가장 유사한 텍스트를 visible_texts에서 찾는다.
pub struct LocalLlmProvider;

impl LocalLlmProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for LocalLlmProvider {
    async fn interpret_intent(
        &self,
        screen_context: &ScreenContext,
        intent_hint: &str,
    ) -> Result<InterpretedAction, CoreError> {
        let hint_lower = intent_hint.to_lowercase();

        // 1. 액션 타입 추출 (키워드 기반)
        let action_type = detect_action_type(&hint_lower);

        // 2. visible_texts에서 가장 유사한 텍스트 찾기
        let (target_text, confidence) = find_best_match(&screen_context.visible_texts, &hint_lower);

        // 3. 역할 추론
        let target_role = infer_role(&action_type, &hint_lower);

        Ok(InterpretedAction {
            target_text,
            target_role,
            action_type,
            confidence,
        })
    }

    fn provider_name(&self) -> &str {
        "local-rule-based"
    }

    fn is_external(&self) -> bool {
        false
    }
}

/// 키워드 기반 액션 타입 감지
fn detect_action_type(hint: &str) -> String {
    // 한국어 + 영어 키워드 매칭
    if hint.contains("클릭")
        || hint.contains("click")
        || hint.contains("누르")
        || hint.contains("press")
    {
        "click".to_string()
    } else if hint.contains("입력")
        || hint.contains("type")
        || hint.contains("작성")
        || hint.contains("write")
    {
        "type".to_string()
    } else if hint.contains("단축키") || hint.contains("hotkey") || hint.contains("shortcut") {
        "hotkey".to_string()
    } else if hint.contains("대기") || hint.contains("wait") {
        "wait".to_string()
    } else if hint.contains("활성화")
        || hint.contains("activate")
        || hint.contains("열기")
        || hint.contains("open")
    {
        "activate".to_string()
    } else {
        "click".to_string() // 기본값
    }
}

/// visible_texts에서 hint와 가장 유사한 텍스트 찾기
fn find_best_match(visible_texts: &[String], hint: &str) -> (Option<String>, f64) {
    if visible_texts.is_empty() {
        return (None, 0.0);
    }

    let mut best_text: Option<String> = None;
    let mut best_score: f64 = 0.0;

    for text in visible_texts {
        let text_lower = text.to_lowercase();
        let score = simple_similarity(&text_lower, hint);
        if score > best_score {
            best_score = score;
            best_text = Some(text.clone());
        }
    }

    // 최소 임계값 0.1
    if best_score < 0.1 {
        (None, 0.0)
    } else {
        (best_text, best_score)
    }
}

/// 간단한 유사도 계산 (부분 문자열 + 단어 일치)
fn simple_similarity(text: &str, hint: &str) -> f64 {
    // 정확 일치
    if text == hint {
        return 1.0;
    }

    // hint에 포함된 모든 단어 중 text에 존재하는 비율
    let hint_words: Vec<&str> = hint.split_whitespace().collect();
    if hint_words.is_empty() {
        return 0.0;
    }

    let matched = hint_words.iter().filter(|w| text.contains(*w)).count();

    matched as f64 / hint_words.len() as f64
}

/// 액션 타입과 힌트에서 역할 추론
fn infer_role(action_type: &str, hint: &str) -> Option<String> {
    match action_type {
        "click" => {
            if hint.contains("버튼") || hint.contains("button") {
                Some("button".to_string())
            } else if hint.contains("링크") || hint.contains("link") {
                Some("link".to_string())
            } else if hint.contains("메뉴") || hint.contains("menu") {
                Some("menu".to_string())
            } else {
                None
            }
        }
        "type" => {
            if hint.contains("검색") || hint.contains("search") {
                Some("search".to_string())
            } else {
                Some("input".to_string())
            }
        }
        _ => None,
    }
}

// ============================================================
// 테스트
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_action_type_click() {
        assert_eq!(detect_action_type("저장 버튼 클릭"), "click");
        assert_eq!(detect_action_type("click the save button"), "click");
        assert_eq!(detect_action_type("확인 누르기"), "click");
    }

    #[test]
    fn detect_action_type_type() {
        assert_eq!(detect_action_type("텍스트 입력"), "type");
        assert_eq!(detect_action_type("type some text"), "type");
    }

    #[test]
    fn detect_action_type_hotkey() {
        assert_eq!(detect_action_type("단축키 실행"), "hotkey");
        assert_eq!(detect_action_type("hotkey Ctrl+S"), "hotkey");
    }

    #[test]
    fn detect_action_type_default() {
        assert_eq!(detect_action_type("unknown action"), "click");
    }

    #[test]
    fn find_best_match_exact() {
        let texts = vec!["저장".to_string(), "편집".to_string()];
        let (text, score) = find_best_match(&texts, "저장");
        assert_eq!(text.unwrap(), "저장");
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn find_best_match_partial() {
        let texts = vec!["파일 저장".to_string(), "편집".to_string()];
        let (text, score) = find_best_match(&texts, "파일 저장");
        assert_eq!(text.unwrap(), "파일 저장");
        assert!(score > 0.0);
    }

    #[test]
    fn find_best_match_empty() {
        let texts: Vec<String> = vec![];
        let (text, score) = find_best_match(&texts, "저장");
        assert!(text.is_none());
        assert!((score).abs() < f64::EPSILON);
    }

    #[test]
    fn infer_role_button() {
        assert_eq!(infer_role("click", "버튼 클릭"), Some("button".to_string()));
        assert_eq!(
            infer_role("click", "click button"),
            Some("button".to_string())
        );
    }

    #[test]
    fn infer_role_input() {
        assert_eq!(infer_role("type", "입력"), Some("input".to_string()));
        assert_eq!(infer_role("type", "검색 입력"), Some("search".to_string()));
    }

    #[test]
    fn infer_role_none() {
        assert_eq!(infer_role("click", "아무거나"), None);
        assert_eq!(infer_role("hotkey", "단축키"), None);
    }

    #[tokio::test]
    async fn local_llm_interpret_intent() {
        let provider = LocalLlmProvider::new();
        let ctx = ScreenContext {
            visible_texts: vec!["파일".to_string(), "저장".to_string(), "편집".to_string()],
            active_app: "VSCode".to_string(),
            active_window_title: "main.rs".to_string(),
            layout_description: None,
        };

        let result = provider
            .interpret_intent(&ctx, "저장 버튼 클릭")
            .await
            .unwrap();
        assert_eq!(result.action_type, "click");
        assert!(result.target_text.is_some());
        assert_eq!(result.target_role, Some("button".to_string()));
    }

    #[test]
    fn local_llm_provider_info() {
        let provider = LocalLlmProvider::new();
        assert_eq!(provider.provider_name(), "local-rule-based");
        assert!(!provider.is_external());
    }
}
