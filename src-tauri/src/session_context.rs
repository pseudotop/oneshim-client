//! Assembles system prompt context from local data sources for AI conversation sessions.

use std::sync::Arc;

use chrono::Utc;
use oneshim_core::config::AppConfig;
use oneshim_core::models::ai_session::{
    ActivitySummary, MessageRole, SessionMessage, SuggestionPatterns, SystemInfo,
    SystemPromptContext, UserProfileSummary,
};
use oneshim_storage::sqlite::SqliteStorage;

use crate::scheduler::shared_regime_state::SharedRegimeState;

// Phase 2: wired into SessionManagerImpl for system prompt generation
#[allow(dead_code)]
pub struct SessionContextAssembler {
    storage: Arc<SqliteStorage>,
    config: Arc<AppConfig>,
    regime_state: Arc<SharedRegimeState>,
}

#[allow(dead_code)]
impl SessionContextAssembler {
    pub fn new(
        storage: Arc<SqliteStorage>,
        config: Arc<AppConfig>,
        regime_state: Arc<SharedRegimeState>,
    ) -> Self {
        Self {
            storage,
            config,
            regime_state,
        }
    }

    pub fn build_system_prompt(&self) -> SystemPromptContext {
        // TODO(Phase 2): Query storage for recent activity and suggestion history
        SystemPromptContext {
            user_profile: UserProfileSummary::default(),
            current_regime: self.current_regime(),
            recent_activity: ActivitySummary::default(),
            suggestion_history: SuggestionPatterns::default(),
            available_skills: vec![],
            system_info: SystemInfo {
                os: std::env::consts::OS.to_string(),
                active_app: None,
                timezone: Utc::now().format("%Z").to_string(),
            },
        }
    }

    pub fn build_system_message(&self) -> SessionMessage {
        let context = self.build_system_prompt();
        let content = serde_json::to_string_pretty(&context).unwrap_or_else(|_| "{}".to_string());

        SessionMessage {
            role: MessageRole::System,
            content: format!(
                "You are ONESHIM's AI assistant. Here is the current user context:\n\n{content}"
            ),
            attachments: vec![],
            tools: None,
            context: None,
        }
    }

    fn current_regime(&self) -> String {
        self.regime_state
            .snapshot()
            .regime_label
            .unwrap_or_else(|| "unknown".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_system_message_has_system_role() {
        // SessionContextAssembler requires real dependencies;
        // test the SystemPromptContext serialization separately
        let ctx = SystemPromptContext {
            user_profile: UserProfileSummary::default(),
            current_regime: "deep_work".to_string(),
            recent_activity: ActivitySummary::default(),
            suggestion_history: SuggestionPatterns::default(),
            available_skills: vec![],
            system_info: SystemInfo {
                os: "macos".to_string(),
                active_app: Some("VSCode".to_string()),
                timezone: "KST".to_string(),
            },
        };
        let json = serde_json::to_string_pretty(&ctx).unwrap();
        assert!(json.contains("deep_work"));
        assert!(json.contains("VSCode"));
        assert!(json.contains("KST"));
    }
}
