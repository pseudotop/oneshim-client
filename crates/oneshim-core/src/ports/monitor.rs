//! 시스템 모니터링 포트.
//!
//! 구현: `oneshim-monitor` crate (sysinfo + 플랫폼별 FFI)

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::context::{ProcessInfo, UserContext, WindowInfo};
use crate::models::event::ProcessDetail;
use crate::models::system::SystemMetrics;

/// 시스템 리소스 모니터링 (CPU, 메모리, 디스크)
#[async_trait]
pub trait SystemMonitor: Send + Sync {
    /// 현재 시스템 메트릭 수집
    async fn collect_metrics(&self) -> Result<SystemMetrics, CoreError>;
}

/// 프로세스/활성 창 모니터링
#[async_trait]
pub trait ProcessMonitor: Send + Sync {
    /// 현재 활성 창 정보 조회
    async fn get_active_window(&self) -> Result<Option<WindowInfo>, CoreError>;

    /// 실행 중인 프로세스 목록 조회 (CPU 사용률 상위 N개)
    async fn get_top_processes(&self, limit: usize) -> Result<Vec<ProcessInfo>, CoreError>;

    /// 상세 프로세스 정보 조회 (서버 전송용)
    ///
    /// Foreground 프로세스 + CPU 상위 N개를 중복 제거하여 반환.
    /// 창 개수, 실행 시간, 실행 경로 등 추가 정보 포함.
    async fn get_detailed_processes(
        &self,
        foreground_pid: Option<u32>,
        top_n: usize,
    ) -> Result<Vec<ProcessDetail>, CoreError>;
}

/// 사용자 활동 모니터링 (컨텍스트 수집 오케스트레이터)
#[async_trait]
pub trait ActivityMonitor: Send + Sync {
    /// 현재 사용자 컨텍스트 전체 수집
    async fn collect_context(&self) -> Result<UserContext, CoreError>;
}
