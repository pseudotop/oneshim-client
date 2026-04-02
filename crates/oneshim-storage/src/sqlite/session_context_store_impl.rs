use oneshim_core::error::CoreError;
use oneshim_core::models::storage_records::SuggestionRecord;
use oneshim_core::ports::session_context_store::SessionContextStorePort;

use super::SqliteStorage;

impl SessionContextStorePort for SqliteStorage {
    fn list_suggestions(&self, limit: usize) -> Result<Vec<SuggestionRecord>, CoreError> {
        SqliteStorage::list_suggestions(self, limit).map_err(CoreError::from)
    }
}
