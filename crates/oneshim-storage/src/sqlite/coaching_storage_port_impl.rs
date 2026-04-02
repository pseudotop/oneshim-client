use oneshim_core::error::CoreError;
use oneshim_core::models::coaching::CoachingEventRow;
use oneshim_core::ports::coaching_storage::CoachingStoragePort;

use super::SqliteStorage;

impl CoachingStoragePort for SqliteStorage {
    fn insert_coaching_event(&self, event: &CoachingEventRow) -> Result<(), CoreError> {
        SqliteStorage::insert_coaching_event(self, event).map_err(CoreError::from)
    }

    fn update_coaching_event_personalized(
        &self,
        event_id: &str,
        personalized_text: &str,
    ) -> Result<(), CoreError> {
        SqliteStorage::update_coaching_event_personalized(self, event_id, personalized_text)
            .map_err(CoreError::from)
    }
}
