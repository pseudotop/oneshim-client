//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenContext {
    pub visible_texts: Vec<String>,
    pub active_app: String,
    pub active_window_title: String,
    pub layout_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpretedAction {
    pub target_text: Option<String>,
    pub target_role: Option<String>,
    pub action_type: String,
    pub confidence: f64,
}

///
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn interpret_intent(
        &self,
        screen_context: &ScreenContext,
        intent_hint: &str,
    ) -> Result<InterpretedAction, CoreError>;

    fn provider_name(&self) -> &str;

    fn is_external(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_context_serde() {
        let ctx = ScreenContext {
            visible_texts: vec!["file".to_string(), "편집".to_string()],
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
            target_text: Some("save".to_string()),
            target_role: Some("button".to_string()),
            action_type: "click".to_string(),
            confidence: 0.85,
        };
        let json = serde_json::to_string(&action).unwrap();
        let deser: InterpretedAction = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.target_text.unwrap(), "save");
        assert!((deser.confidence - 0.85).abs() < f64::EPSILON);
    }
}
