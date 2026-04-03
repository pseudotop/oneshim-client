//! Runtime log snapshot provider port for bug reports.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::bug_report::RuntimeLogSnapshot;

/// Provides runtime log snapshots for bug reports.
#[async_trait]
pub trait RuntimeLogProvider: Send + Sync {
    /// Read the tail of the most recent log file.
    async fn snapshot(&self, line_limit: usize) -> Result<RuntimeLogSnapshot, CoreError>;
}
