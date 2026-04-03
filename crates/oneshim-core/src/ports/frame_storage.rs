//! Port for persisting and managing captured frame images on disk.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

use crate::error::CoreError;

/// Port for persisting captured frame images to storage.
///
/// Implemented by `FrameFileStorage` in `oneshim-storage`.
/// Consumers receive `Arc<dyn FrameStoragePort>` via DI.
///
/// Diagnostic methods (`frames_dir`, `buffer_pool_stats`, `disk_status`)
/// remain on the concrete type — they are infrastructure-level concerns
/// that do not belong in the port contract.
#[async_trait]
pub trait FrameStoragePort: Send + Sync {
    /// Save a single frame image. Returns the relative path of the saved file.
    async fn save_frame(&self, timestamp: DateTime<Utc>, data: &[u8])
        -> Result<PathBuf, CoreError>;

    /// Save multiple frames in a batch. Returns per-frame results.
    async fn save_frames_batch(
        &self,
        frames: Vec<(DateTime<Utc>, Vec<u8>)>,
    ) -> Vec<Result<PathBuf, CoreError>>;

    /// Delete frames older than the configured retention period.
    /// Returns the number of deleted files.
    async fn enforce_retention(&self) -> Result<usize, CoreError>;

    /// Delete oldest frames to stay within storage size limits.
    /// Returns the number of deleted files.
    async fn enforce_storage_limit(&self) -> Result<usize, CoreError>;
}
