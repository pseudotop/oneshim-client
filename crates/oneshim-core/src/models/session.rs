//! 세션 모델.
//!
//! 서버와의 연결 세션 및 건강 상태를 표현.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 서버 연결 세션 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// 세션 고유 ID
    pub session_id: String,
    /// 사용자 ID
    pub user_id: String,
    /// 세션 시작 시각
    pub started_at: DateTime<Utc>,
    /// 마지막 활동 시각
    pub last_active_at: DateTime<Utc>,
    /// 연결 건강 상태
    pub health: ConnectionHealth,
}

/// 서버 연결 건강 상태
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionHealth {
    /// 연결 상태
    pub status: ConnectionStatus,
    /// 마지막 하트비트 시각
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// 연속 실패 횟수
    pub consecutive_failures: u32,
    /// 평균 응답 시간 (밀리초)
    pub avg_latency_ms: Option<f64>,
}

/// 연결 상태
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectionStatus {
    /// 연결됨
    Connected,
    /// 연결 중
    Connecting,
    /// 연결 끊김 (재연결 예정)
    Disconnected,
    /// 재연결 시도 중
    Reconnecting,
    /// 연결 실패 (수동 재시도 필요)
    Failed,
}
