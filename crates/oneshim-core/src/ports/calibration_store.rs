//! Read/write ports for calibration entries used by regime detection and noise filtering.

use chrono::{DateTime, Utc};

use crate::error::CoreError;
use crate::models::tiered_memory::CalibrationEntry;

/// Synchronous write port for calibration data.
///
/// Called with batched entries from CalibrationBuffer. Implementations
/// typically write to SQLite in a single transaction.
pub trait CalibrationWriter: Send + Sync {
    /// Persist a batch of calibration entries atomically.
    fn log_batch(&self, entries: &[CalibrationEntry]) -> Result<(), CoreError>;

    /// Flag all entries in the given time range as noise. Returns the number
    /// of rows updated.
    fn flag_noise_range(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<u64, CoreError>;
}

/// Asynchronous read port for calibration data.
///
/// Used by RegimeDetector for batch analysis over historical calibration
/// entries.
#[async_trait::async_trait]
pub trait CalibrationReader: Send + Sync {
    /// Retrieve calibration entries within the given time range.
    /// When `exclude_noise` is true, entries flagged as noise are omitted.
    async fn get_entries(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        exclude_noise: bool,
    ) -> Result<Vec<CalibrationEntry>, CoreError>;

    /// Delete entries older than `max_days` or exceeding `max_rows`, whichever
    /// removes more. Returns the number of rows deleted.
    async fn enforce_retention(&self, max_days: u32, max_rows: u64) -> Result<u64, CoreError>;

    /// List segment IDs with their start/end times for the given range.
    ///
    /// Used by the constrained re-clustering pipeline to map override
    /// `segment_id` values back to feature vector indices.
    async fn list_segment_time_ranges(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<(String, DateTime<Utc>, DateTime<Utc>)>, CoreError> {
        // Default: empty — implementations that have segment storage override this.
        let _ = (from, to);
        Ok(vec![])
    }
}
