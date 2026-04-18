//! RegimeStoragePort — persists RegimeManager state across process restart.

use crate::error::CoreError;
use crate::models::tiered_memory::Regime;
use async_trait::async_trait;

#[async_trait]
pub trait RegimeStoragePort: Send + Sync {
    /// Load all persisted regimes on startup. Empty Vec on first launch.
    ///
    /// Implementations MAY perform corrective side-effect writes — e.g.,
    /// quarantining a payload that failed to deserialise so user-curated
    /// state is preserved for later recovery (see
    /// `SqliteRegimeManagerStateStore`). Despite the name, `load_all` is
    /// therefore NOT guaranteed read-only; callers must treat it as a
    /// single-shot operation at startup. Concurrent `load_all` calls are
    /// not required to be safe.
    async fn load_all(&self) -> Result<Vec<Regime>, CoreError>;

    /// Persist the full regime set. Called on graceful shutdown and,
    /// in a future phase, periodically after lifecycle transitions
    /// (merge, delete, rename).
    async fn save_all(&self, regimes: &[Regime]) -> Result<(), CoreError>;
}
