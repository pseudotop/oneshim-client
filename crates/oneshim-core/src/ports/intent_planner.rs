//! Intent planner port — contract for translating natural-language
//! intent hints into structured automation intents.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::intent::AutomationIntent;

/// IntentPlanner adapters emit `CoreError::Analysis`
/// (wire: `provider.analysis_failed`) for LLM-side planning failures
/// (empty output, non-parseable plan). HTTP-layer failures follow the
/// canonical semantic status mapping. See
/// `docs/guides/http-status-error-mapping.md`.
///
/// The caller surfaces "IntentPlanner is not configured" as
/// `CoreError::Config { ConfigCode::Missing }` (iter-100).
#[async_trait]
pub trait IntentPlanner: Send + Sync {
    async fn plan(&self, intent_hint: &str) -> Result<AutomationIntent, CoreError>;
}
