//! System and process monitoring ports — defines contracts for collecting
//! CPU/memory metrics, active window info, and user activity context.
//! Implemented by `SysInfoMonitor`, `ProcessTracker`, and `ActivityTracker`
//! in `oneshim-monitor`.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::context::{ProcessInfo, UserContext, WindowInfo};
use crate::models::event::ProcessDetail;
use crate::models::system::SystemMetrics;

/// Collects CPU, memory, disk, and network metrics.
///
/// # Errors
/// Returns `CoreError::Internal` if the platform API fails to report metrics.
#[async_trait]
pub trait SystemMonitor: Send + Sync {
    async fn collect_metrics(&self) -> Result<SystemMetrics, CoreError>;
}

/// Active window detection and process enumeration.
///
/// # Errors
/// Returns `CoreError::Internal` on platform API failure,
/// `CoreError::PermissionDenied` if accessibility permissions are missing.
#[async_trait]
pub trait ProcessMonitor: Send + Sync {
    async fn get_active_window(&self) -> Result<Option<WindowInfo>, CoreError>;

    async fn get_top_processes(&self, limit: usize) -> Result<Vec<ProcessInfo>, CoreError>;

    async fn get_detailed_processes(
        &self,
        foreground_pid: Option<u32>,
        top_n: usize,
    ) -> Result<Vec<ProcessDetail>, CoreError>;
}

/// Collects composite user activity context (window, mouse, keyboard, idle).
///
/// # Errors
/// Returns `CoreError::Internal` on platform API failure.
#[async_trait]
pub trait ActivityMonitor: Send + Sync {
    async fn collect_context(&self) -> Result<UserContext, CoreError>;
}
