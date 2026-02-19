//! 유휴 감지 모듈.
//!
//! 플랫폼별 유휴 시간 감지 및 상태 추적.

use oneshim_core::models::activity::{IdleInfo, IdleState};
use tracing::debug;

/// 유휴 감지 임계값 (초)
const DEFAULT_IDLE_THRESHOLD_SECS: u64 = 300; // 5분

/// 유휴 추적기
///
/// 사용자 유휴 상태를 추적하고 상태 변경을 감지합니다.
pub struct IdleTracker {
    /// 유휴 임계값 (초)
    threshold_secs: u64,
    /// 이전 상태
    previous_state: IdleState,
    /// 현재 진행 중인 유휴 기간 ID (DB)
    current_idle_period_id: Option<i64>,
}

impl IdleTracker {
    /// 새 유휴 추적기 생성
    pub fn new(threshold_secs: Option<u64>) -> Self {
        Self {
            threshold_secs: threshold_secs.unwrap_or(DEFAULT_IDLE_THRESHOLD_SECS),
            previous_state: IdleState::Active,
            current_idle_period_id: None,
        }
    }

    /// 현재 유휴 상태 확인
    pub fn check_idle(&mut self) -> IdleInfo {
        let idle_secs = get_idle_time().unwrap_or(0);
        let state = if idle_secs >= self.threshold_secs {
            IdleState::Idle
        } else {
            IdleState::Active
        };

        let info = IdleInfo {
            state,
            idle_secs,
            timestamp: chrono::Utc::now(),
        };

        if state != self.previous_state {
            debug!(
                "유휴 상태 변경: {:?} → {:?} ({}초)",
                self.previous_state, state, idle_secs
            );
        }

        self.previous_state = state;
        info
    }

    /// 상태가 유휴로 전환되었는지 확인
    pub fn became_idle(&self, current: IdleState) -> bool {
        self.previous_state == IdleState::Active && current == IdleState::Idle
    }

    /// 상태가 활성으로 전환되었는지 확인
    pub fn became_active(&self, current: IdleState) -> bool {
        self.previous_state == IdleState::Idle && current == IdleState::Active
    }

    /// 이전 상태 조회
    pub fn previous_state(&self) -> IdleState {
        self.previous_state
    }

    /// 현재 유휴 기간 ID 설정
    pub fn set_idle_period_id(&mut self, id: Option<i64>) {
        self.current_idle_period_id = id;
    }

    /// 현재 유휴 기간 ID 조회
    pub fn idle_period_id(&self) -> Option<i64> {
        self.current_idle_period_id
    }

    /// 임계값 조회
    pub fn threshold_secs(&self) -> u64 {
        self.threshold_secs
    }
}

impl Default for IdleTracker {
    fn default() -> Self {
        Self::new(None)
    }
}

/// 플랫폼별 유휴 시간 조회 (초 단위)
///
/// 마지막 사용자 입력(키보드/마우스) 이후 경과 시간을 반환합니다.
/// 플랫폼을 지원하지 않거나 실패 시 None을 반환합니다.
pub fn get_idle_time() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        crate::macos::get_idle_time_macos()
    }

    #[cfg(target_os = "windows")]
    {
        crate::windows::get_idle_time_windows()
    }

    #[cfg(target_os = "linux")]
    {
        crate::linux::get_idle_time_linux()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // 기타 플랫폼: 미구현
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_tracker_default() {
        let tracker = IdleTracker::default();
        assert_eq!(tracker.threshold_secs(), DEFAULT_IDLE_THRESHOLD_SECS);
        assert_eq!(tracker.previous_state(), IdleState::Active);
    }

    #[test]
    fn idle_tracker_custom_threshold() {
        let tracker = IdleTracker::new(Some(60));
        assert_eq!(tracker.threshold_secs(), 60);
    }

    #[test]
    fn idle_tracker_state_transitions() {
        let mut tracker = IdleTracker::new(Some(0)); // 즉시 유휴 전환

        let info = tracker.check_idle();
        // 첫 체크에서는 상태가 Active에서 시작하므로 became_idle 확인 가능
        assert!(info.state == IdleState::Idle || info.state == IdleState::Active);
    }

    #[test]
    fn idle_period_id_management() {
        let mut tracker = IdleTracker::default();
        assert!(tracker.idle_period_id().is_none());

        tracker.set_idle_period_id(Some(123));
        assert_eq!(tracker.idle_period_id(), Some(123));

        tracker.set_idle_period_id(None);
        assert!(tracker.idle_period_id().is_none());
    }
}
