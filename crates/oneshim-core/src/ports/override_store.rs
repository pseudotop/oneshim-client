//! Port for persisting user regime overrides.
//!
//! Implementations reside in the storage adapter layer (e.g., SQLite).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::CoreError;
use crate::models::recalibration::RegimeOverride;

/// Async port for CRUD operations on regime overrides.
///
/// # Errors
/// `CoreError::Storage` (wire: `storage.failed`) for SQLite operations
/// (iter-47 mass fix pattern). `delete_override` with an unknown ID is
/// treated as rowcount=0 by current adapters and returns `Ok(())`
/// rather than a distinct NotFound. Empty range results from
/// `list_overrides` are `Ok(Vec::new())`.
#[async_trait]
pub trait OverrideStore: Send + Sync {
    /// Persist a new user override.
    async fn save_override(&self, entry: &RegimeOverride) -> Result<(), CoreError>;

    /// List overrides whose `created_at` falls within `[from, to]`.
    async fn list_overrides(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<RegimeOverride>, CoreError>;

    /// Delete an override by its ID.
    async fn delete_override(&self, override_id: &str) -> Result<(), CoreError>;
}
