//! LLM-backed analysis port for context-to-suggestion and summarization pipelines.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::suggestion::Suggestion;

/// LLM-backed analysis port.
///
/// All methods use `CoreError::Analysis` (wire: `provider.analysis_failed`)
/// for provider-side failures (LLM returned bad JSON, empty body,
/// non-parseable intent). HTTP-layer failures follow the canonical semantic
/// status mapping (`auth.failed` / `network.timeout` / `network.rate_limit` /
/// `service.unavailable`). See `docs/guides/http-status-error-mapping.md`.
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
    ///
    /// Default impl returns `CoreError::Analysis { AnalysisFailed }` with a
    /// "not implemented" message — adapters that don't support summarize
    /// can leave the default. Adapters that DO support it must override.
    /// Previously the default called `analyze()` which parses JSON
    /// suggestions and is incompatible with the plain-text contract here.
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
