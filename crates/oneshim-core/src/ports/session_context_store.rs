use crate::error::CoreError;
use crate::models::storage_records::SuggestionRecord;
use crate::ports::storage::StorageService;

/// Query subset needed to assemble AI session system context.
pub trait SessionContextStorePort: StorageService + Send + Sync {
    fn list_suggestions(&self, limit: usize) -> Result<Vec<SuggestionRecord>, CoreError>;
}
