use crate::error::CoreError;
use crate::models::suggestion::SuggestionHistoryEntry;

/// Storage port for few-shot prompt example retrieval and feedback recording.
/// Synchronous — matches StorageService / FocusStorage pattern (SQLite sync ops).
pub trait FewShotStorage: Send + Sync {
    fn get_suggestions_with_feedback(
        &self,
        limit: usize,
    ) -> Result<Vec<SuggestionHistoryEntry>, CoreError>;
    fn record_suggestion_feedback(
        &self,
        suggestion_id: &str,
        feedback_type: &str,
        context_app: &str,
        context_window: &str,
        regime_label: Option<&str>,
    ) -> Result<(), CoreError>;
}
