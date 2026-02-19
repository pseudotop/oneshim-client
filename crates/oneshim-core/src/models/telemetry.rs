//! 텔레메트리 모델.
//!
//! 클라이언트 자체의 성능 지표, 세션 통계를 정의.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 단일 성능 지표
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    /// 지표 이름 (예: "sse_latency_ms", "batch_upload_size")
    pub name: String,
    /// 측정값
    pub value: f64,
    /// 측정 시각
    pub timestamp: DateTime<Utc>,
    /// 추가 레이블 (key-value)
    #[serde(default)]
    pub labels: std::collections::HashMap<String, String>,
}

/// 세션 통계 (주기적으로 서버에 보고)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// 세션 ID
    pub session_id: String,
    /// 통계 기간 시작
    pub period_start: DateTime<Utc>,
    /// 통계 기간 종료
    pub period_end: DateTime<Utc>,
    /// 수신한 제안 수
    pub suggestions_received: u32,
    /// 수락한 제안 수
    pub suggestions_accepted: u32,
    /// 거절한 제안 수
    pub suggestions_rejected: u32,
    /// 전송한 이벤트 배치 수
    pub batches_sent: u32,
    /// 전송 실패한 배치 수
    pub batches_failed: u32,
    /// 평균 SSE 지연 (밀리초)
    pub avg_sse_latency_ms: Option<f64>,
}
