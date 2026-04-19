//! Persistence port for coaching event records emitted by the scheduler.

use crate::error::CoreError;
use crate::models::coaching::CoachingEventRow;

/// Focused persistence contract for scheduler coaching events.
///
/// # Errors
/// `CoreError::Storage` (wire: `storage.failed`) for SQLite operations
/// (iter-47 mass fix pattern). Event not found during `update_coaching_event_personalized`
/// is a rowcount=0 condition — the impl may return Ok(()) or a specific
/// NotFound; check adapter for exact semantic.
pub trait CoachingStoragePort: Send + Sync {
    fn insert_coaching_event(&self, event: &CoachingEventRow) -> Result<(), CoreError>;

    fn update_coaching_event_personalized(
        &self,
        event_id: &str,
        personalized_text: &str,
    ) -> Result<(), CoreError>;
}
