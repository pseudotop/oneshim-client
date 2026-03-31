//! Intent planner port — contract for translating natural-language
//! intent hints into structured automation intents.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::intent::AutomationIntent;

#[async_trait]
pub trait IntentPlanner: Send + Sync {
    async fn plan(&self, intent_hint: &str) -> Result<AutomationIntent, CoreError>;
}
