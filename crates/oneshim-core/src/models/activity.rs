//! 활동 및 유휴 감지 모델.
//!
//! 사용자 유휴 상태와 세션 통계를 표현.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 유휴 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdleState {
    /// 활성 (사용자 입력 있음)
    Active,
    /// 유휴 (임계값 초과)
    Idle,
    /// 잠금 상태
    Locked,
}

/// 유휴 감지 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleInfo {
    /// 현재 유휴 상태
    pub state: IdleState,
    /// 마지막 입력 이후 경과 시간 (초)
    pub idle_secs: u64,
    /// 측정 시각
    pub timestamp: DateTime<Utc>,
}

/// 유휴 기간 기록
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlePeriod {
    /// 유휴 시작 시각
    pub start_time: DateTime<Utc>,
    /// 유휴 종료 시각 (None이면 진행 중)
    pub end_time: Option<DateTime<Utc>>,
    /// 유휴 지속 시간 (초)
    pub duration_secs: Option<u64>,
}

impl IdlePeriod {
    /// 새 유휴 기간 시작
    pub fn start_now() -> Self {
        Self {
            start_time: Utc::now(),
            end_time: None,
            duration_secs: None,
        }
    }

    /// 유휴 기간 종료
    pub fn end_now(&mut self) {
        let now = Utc::now();
        self.end_time = Some(now);
        self.duration_secs = Some((now - self.start_time).num_seconds() as u64);
    }

    /// 진행 중인지 확인
    pub fn is_ongoing(&self) -> bool {
        self.end_time.is_none()
    }
}

/// 세션 통계
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    /// 세션 ID
    pub session_id: String,
    /// 세션 시작 시각
    pub started_at: DateTime<Utc>,
    /// 세션 종료 시각 (None이면 진행 중)
    pub ended_at: Option<DateTime<Utc>>,
    /// 총 이벤트 수
    pub total_events: u64,
    /// 총 프레임 수
    pub total_frames: u64,
    /// 총 유휴 시간 (초)
    pub total_idle_secs: u64,
}

impl SessionStats {
    /// 새 세션 시작
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            started_at: Utc::now(),
            ended_at: None,
            total_events: 0,
            total_frames: 0,
            total_idle_secs: 0,
        }
    }

    /// 이벤트 카운트 증가
    pub fn increment_events(&mut self) {
        self.total_events += 1;
    }

    /// 프레임 카운트 증가
    pub fn increment_frames(&mut self) {
        self.total_frames += 1;
    }

    /// 유휴 시간 추가
    pub fn add_idle_secs(&mut self, secs: u64) {
        self.total_idle_secs += secs;
    }

    /// 세션 종료
    pub fn end_now(&mut self) {
        self.ended_at = Some(Utc::now());
    }
}

/// 프로세스 스냅샷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    /// 스냅샷 시각
    pub timestamp: DateTime<Utc>,
    /// 프로세스 목록 (JSON 직렬화용)
    pub processes: Vec<ProcessSnapshotEntry>,
}

/// 프로세스 스냅샷 항목
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSnapshotEntry {
    /// 프로세스 ID
    pub pid: u32,
    /// 프로세스 이름
    pub name: String,
    /// CPU 사용률 (%)
    pub cpu_usage: f32,
    /// 메모리 사용량 (바이트)
    pub memory_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_period_lifecycle() {
        let mut period = IdlePeriod::start_now();
        assert!(period.is_ongoing());
        assert!(period.end_time.is_none());

        std::thread::sleep(std::time::Duration::from_millis(10));
        period.end_now();

        assert!(!period.is_ongoing());
        assert!(period.end_time.is_some());
        assert!(period.duration_secs.is_some());
    }

    #[test]
    fn session_stats_counters() {
        let mut stats = SessionStats::new("test-session".to_string());
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.total_frames, 0);

        stats.increment_events();
        stats.increment_events();
        stats.increment_frames();
        stats.add_idle_secs(30);

        assert_eq!(stats.total_events, 2);
        assert_eq!(stats.total_frames, 1);
        assert_eq!(stats.total_idle_secs, 30);
    }
}
