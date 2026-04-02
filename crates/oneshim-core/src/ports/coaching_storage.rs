use crate::error::CoreError;
use crate::models::coaching::CoachingEventRow;

/// Focused persistence contract for scheduler coaching events.
pub trait CoachingStoragePort: Send + Sync {
    fn insert_coaching_event(&self, event: &CoachingEventRow) -> Result<(), CoreError>;

    fn update_coaching_event_personalized(
        &self,
        event_id: &str,
        personalized_text: &str,
    ) -> Result<(), CoreError>;
}
