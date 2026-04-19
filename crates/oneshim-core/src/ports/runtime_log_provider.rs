//! Runtime log snapshot provider port for bug reports.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::bug_report::RuntimeLogSnapshot;

/// Provides runtime log snapshots for bug reports.
///
/// # Errors
/// - `CoreError::Io` (wire: `internal.io`) — log directory read failure,
///   file open failure, permission denied (propagated via `?` from
///   `std::io::Error`).
/// - `CoreError::Internal` (wire: `internal.generic`) — UTF-8 decode
///   failure on corrupt log lines (adapters typically lossy-decode
///   instead; check the implementation).
/// - Absence of any log file on first launch is `Ok(RuntimeLogSnapshot::default())` /
///   empty snapshot rather than Err.
#[async_trait]
pub trait RuntimeLogProvider: Send + Sync {
    /// Read the tail of the most recent log file.
    async fn snapshot(&self, line_limit: usize) -> Result<RuntimeLogSnapshot, CoreError>;
}
