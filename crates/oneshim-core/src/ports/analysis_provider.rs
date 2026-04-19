//! LLM-backed analysis port for context-to-suggestion and summarization pipelines.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::suggestion::Suggestion;

#[async_trait]
pub trait AnalysisProvider: Send + Sync {
    /// Analyze assembled context and return productivity suggestions.
    /// The adapter is responsible for parsing the LLM response into Suggestions.
    async fn analyze(
        &self,
        context_json: &str,
        system_prompt: &str,
    ) -> Result<Vec<Suggestion>, CoreError>;

    /// Generate a plain text summary from context.
    /// Default returns an error — adapters must override with a proper implementation.
    /// The previous default called `analyze()` which parses JSON suggestions,
    /// incompatible with the plain text expected by `summarize_text()`.
    async fn summarize_text(
        &self,
        _context_json: &str,
        _system_prompt: &str,
    ) -> Result<String, CoreError> {
        Err(CoreError::Analysis {
            code: crate::error_codes::ProviderCode::AnalysisFailed,
            message: "summarize_text not implemented for this provider".into(),
        })
    }

    fn provider_name(&self) -> &str;
}
