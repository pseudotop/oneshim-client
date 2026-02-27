use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::context::{ProcessInfo, UserContext, WindowInfo};
use crate::models::event::ProcessDetail;
use crate::models::system::SystemMetrics;

#[async_trait]
pub trait SystemMonitor: Send + Sync {
    async fn collect_metrics(&self) -> Result<SystemMetrics, CoreError>;
}

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

#[async_trait]
pub trait ActivityMonitor: Send + Sync {
    async fn collect_context(&self) -> Result<UserContext, CoreError>;
}
