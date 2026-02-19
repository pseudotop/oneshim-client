//! 시스템 리소스 모니터링.
//!
//! `SystemMonitor` 포트 구현. sysinfo 기반 CPU/메모리/디스크 수집.

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::models::system::{NetworkInfo, SystemMetrics};
use oneshim_core::ports::monitor::SystemMonitor;
use std::sync::Mutex;
use sysinfo::{Disks, Networks, System};
use tracing::debug;

/// sysinfo 기반 시스템 모니터 — `SystemMonitor` 포트 구현
pub struct SysInfoMonitor {
    sys: Mutex<System>,
    disks: Mutex<Disks>,
    networks: Mutex<Networks>,
}

impl SysInfoMonitor {
    /// 새 시스템 모니터 생성
    pub fn new() -> Self {
        Self {
            sys: Mutex::new(System::new_all()),
            disks: Mutex::new(Disks::new_with_refreshed_list()),
            networks: Mutex::new(Networks::new_with_refreshed_list()),
        }
    }
}

impl Default for SysInfoMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SystemMonitor for SysInfoMonitor {
    async fn collect_metrics(&self) -> Result<SystemMetrics, CoreError> {
        // CPU 갱신
        {
            let mut sys = self
                .sys
                .lock()
                .map_err(|e| CoreError::Internal(format!("시스템 잠금 실패: {e}")))?;
            sys.refresh_cpu_usage();
            sys.refresh_memory();
        }

        // 디스크 갱신
        {
            let mut disks = self
                .disks
                .lock()
                .map_err(|e| CoreError::Internal(format!("디스크 잠금 실패: {e}")))?;
            disks.refresh();
        }

        // 네트워크 갱신
        {
            let mut networks = self
                .networks
                .lock()
                .map_err(|e| CoreError::Internal(format!("네트워크 잠금 실패: {e}")))?;
            networks.refresh();
        }

        // 메트릭 수집
        let sys = self
            .sys
            .lock()
            .map_err(|e| CoreError::Internal(format!("시스템 잠금 실패: {e}")))?;

        let cpu_usage = sys.global_cpu_usage();
        let memory_used = sys.used_memory();
        let memory_total = sys.total_memory();

        // 디스크 합계
        let disks = self
            .disks
            .lock()
            .map_err(|e| CoreError::Internal(format!("디스크 잠금 실패: {e}")))?;
        let (disk_used, disk_total) = disks.list().iter().fold((0u64, 0u64), |(used, total), d| {
            (
                used + d.total_space() - d.available_space(),
                total + d.total_space(),
            )
        });

        // 네트워크 합계
        let networks = self
            .networks
            .lock()
            .map_err(|e| CoreError::Internal(format!("네트워크 잠금 실패: {e}")))?;
        let (upload_speed, download_speed) = networks
            .list()
            .iter()
            .fold((0u64, 0u64), |(up, down), (_name, data)| {
                (up + data.transmitted(), down + data.received())
            });

        let network = Some(NetworkInfo {
            upload_speed,
            download_speed,
            is_connected: download_speed > 0 || upload_speed > 0,
        });

        let metrics = SystemMetrics {
            timestamp: chrono::Utc::now(),
            cpu_usage,
            memory_used,
            memory_total,
            disk_used,
            disk_total,
            network,
        };

        debug!(
            "시스템 메트릭: CPU {:.1}%, 메모리 {}/{}MB",
            metrics.cpu_usage,
            metrics.memory_used / 1_048_576,
            metrics.memory_total / 1_048_576
        );

        Ok(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collect_metrics() {
        let monitor = SysInfoMonitor::new();
        let metrics = monitor.collect_metrics().await.unwrap();

        assert!(metrics.cpu_usage >= 0.0);
        assert!(metrics.memory_total > 0);
        assert!(metrics.memory_used <= metrics.memory_total);
    }
}
