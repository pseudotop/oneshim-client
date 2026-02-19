//! 시스템 메트릭 모델.
//!
//! CPU, 메모리, 디스크, 네트워크 사용량 등 시스템 상태를 표현.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 시스템 메트릭 스냅샷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// 수집 시각
    pub timestamp: DateTime<Utc>,
    /// CPU 사용률 (0.0 ~ 100.0)
    pub cpu_usage: f32,
    /// 메모리 사용량 (바이트)
    pub memory_used: u64,
    /// 전체 메모리 (바이트)
    pub memory_total: u64,
    /// 디스크 사용량 (바이트)
    pub disk_used: u64,
    /// 전체 디스크 용량 (바이트)
    pub disk_total: u64,
    /// 네트워크 정보
    pub network: Option<NetworkInfo>,
}

/// 네트워크 상태 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// 업로드 속도 (bytes/sec)
    pub upload_speed: u64,
    /// 다운로드 속도 (bytes/sec)
    pub download_speed: u64,
    /// 연결 상태
    pub is_connected: bool,
}

/// 시스템 경고 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertInfo {
    /// 경고 유형
    pub alert_type: AlertType,
    /// 경고 메시지
    pub message: String,
    /// 심각도 (0.0 ~ 1.0)
    pub severity: f32,
    /// 발생 시각
    pub timestamp: DateTime<Utc>,
}

/// 시스템 경고 유형
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlertType {
    /// CPU 과부하
    HighCpu,
    /// 메모리 부족
    LowMemory,
    /// 디스크 공간 부족
    LowDisk,
    /// 네트워크 연결 끊김
    NetworkDisconnected,
}
