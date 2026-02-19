//! 입력 활동 수집기.
//!
//! 마우스/키보드 패턴 집계 (내용 제외, 패턴만).
//! 유휴 시간 변화 기반 활동량 추정.

use chrono::{DateTime, Utc};
use oneshim_core::models::event::{InputActivityEvent, KeyboardActivity, MouseActivity};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

/// 입력 활동 수집기 — 마우스/키보드 패턴 집계
///
/// 실제 키 입력 내용은 수집하지 않음 (프라이버시).
/// 유휴 시간 변화 패턴으로 활동량 추정.
pub struct InputActivityCollector {
    /// 집계 시작 시각
    period_start: Mutex<DateTime<Utc>>,
    /// 현재 앱 이름
    current_app: Mutex<String>,

    // 마우스 활동 카운터 (Atomic)
    click_count: AtomicU32,
    scroll_count: AtomicU32,
    double_click_count: AtomicU32,
    right_click_count: AtomicU32,
    move_distance: AtomicU64, // f64 비트 저장

    // 키보드 활동 카운터
    total_keystrokes: AtomicU32,
    typing_bursts: AtomicU32,
    shortcut_count: AtomicU32,
    correction_count: AtomicU32,

    // 마지막 활동 시간 (버스트 감지용)
    last_activity_ms: AtomicU64,
    /// 버스트 감지 임계값 (ms) — 이 시간 내 연속 입력이면 버스트
    burst_threshold_ms: u64,
}

impl InputActivityCollector {
    /// 새 수집기 생성
    pub fn new() -> Self {
        Self {
            period_start: Mutex::new(Utc::now()),
            current_app: Mutex::new(String::new()),
            click_count: AtomicU32::new(0),
            scroll_count: AtomicU32::new(0),
            double_click_count: AtomicU32::new(0),
            right_click_count: AtomicU32::new(0),
            move_distance: AtomicU64::new(0),
            total_keystrokes: AtomicU32::new(0),
            typing_bursts: AtomicU32::new(0),
            shortcut_count: AtomicU32::new(0),
            correction_count: AtomicU32::new(0),
            last_activity_ms: AtomicU64::new(0),
            burst_threshold_ms: 2000, // 2초
        }
    }

    /// 현재 앱 업데이트
    pub fn set_current_app(&self, app_name: &str) {
        if let Ok(mut app) = self.current_app.lock() {
            *app = app_name.to_string();
        }
    }

    /// 클릭 이벤트 기록
    pub fn record_click(&self) {
        self.click_count.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }

    /// 더블클릭 이벤트 기록
    pub fn record_double_click(&self) {
        self.double_click_count.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }

    /// 우클릭 이벤트 기록
    pub fn record_right_click(&self) {
        self.right_click_count.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }

    /// 스크롤 이벤트 기록
    pub fn record_scroll(&self) {
        self.scroll_count.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }

    /// 마우스 이동 기록 (거리 누적)
    pub fn record_mouse_move(&self, distance: f64) {
        let bits = self.move_distance.load(Ordering::Relaxed);
        let current = f64::from_bits(bits);
        let new_bits = (current + distance).to_bits();
        self.move_distance.store(new_bits, Ordering::Relaxed);
    }

    /// 키 입력 기록 (내용 제외)
    pub fn record_keystroke(&self, is_shortcut: bool, is_correction: bool) {
        self.total_keystrokes.fetch_add(1, Ordering::Relaxed);

        if is_shortcut {
            self.shortcut_count.fetch_add(1, Ordering::Relaxed);
        }
        if is_correction {
            self.correction_count.fetch_add(1, Ordering::Relaxed);
        }

        self.record_activity();
    }

    /// 활동 기록 (버스트 감지)
    fn record_activity(&self) {
        let now_ms = Utc::now().timestamp_millis() as u64;
        let last_ms = self.last_activity_ms.swap(now_ms, Ordering::Relaxed);

        // 이전 활동과의 간격이 임계값 이상이면 새 버스트 시작
        if last_ms > 0 && now_ms.saturating_sub(last_ms) > self.burst_threshold_ms {
            self.typing_bursts.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 유휴 시간 변화로 활동량 추정
    ///
    /// 유휴 시간이 짧아지면 활동 중, 길어지면 유휴 상태.
    /// 이 메서드는 정확한 입력 이벤트 대신 근사 추정에 사용.
    pub fn estimate_from_idle_change(&self, prev_idle_secs: u64, curr_idle_secs: u64) {
        // 유휴 시간이 리셋되면 (짧아지면) 활동이 있었음
        if curr_idle_secs < prev_idle_secs {
            // 예상 키 입력 수 추정 (단순 휴리스틱)
            let estimated_keystrokes = (prev_idle_secs - curr_idle_secs).min(10) as u32;
            self.total_keystrokes
                .fetch_add(estimated_keystrokes, Ordering::Relaxed);

            // 활동 기록
            self.record_activity();
        }
    }

    /// 집계 기간 스냅샷 생성 및 카운터 리셋
    ///
    /// 호출 시점까지의 활동을 `InputActivityEvent`로 반환하고 카운터 초기화.
    pub fn take_snapshot(&self) -> InputActivityEvent {
        let now = Utc::now();

        // 기간 계산
        let period_secs = {
            let mut start = self.period_start.lock().unwrap();
            let duration = (now - *start).num_seconds().max(1) as u32;
            *start = now;
            duration
        };

        // 앱 이름
        let app_name = self
            .current_app
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default();

        // 카운터 수집 및 리셋
        let clicks = self.click_count.swap(0, Ordering::Relaxed);
        let scrolls = self.scroll_count.swap(0, Ordering::Relaxed);
        let double_clicks = self.double_click_count.swap(0, Ordering::Relaxed);
        let right_clicks = self.right_click_count.swap(0, Ordering::Relaxed);
        let move_bits = self.move_distance.swap(0, Ordering::Relaxed);
        let move_dist = f64::from_bits(move_bits);

        let keystrokes = self.total_keystrokes.swap(0, Ordering::Relaxed);
        let bursts = self.typing_bursts.swap(0, Ordering::Relaxed);
        let shortcuts = self.shortcut_count.swap(0, Ordering::Relaxed);
        let corrections = self.correction_count.swap(0, Ordering::Relaxed);

        // 분당 키 입력 수 계산
        let keystrokes_per_min = if period_secs > 0 {
            (keystrokes as f64 / period_secs as f64 * 60.0) as u32
        } else {
            0
        };

        InputActivityEvent {
            timestamp: now,
            period_secs,
            mouse: MouseActivity {
                click_count: clicks,
                move_distance: move_dist,
                scroll_count: scrolls,
                last_position: None, // 프라이버시: 위치는 선택적
                double_click_count: double_clicks,
                right_click_count: right_clicks,
            },
            keyboard: KeyboardActivity {
                keystrokes_per_min,
                total_keystrokes: keystrokes,
                typing_bursts: bursts,
                shortcut_count: shortcuts,
                correction_count: corrections,
            },
            app_name,
        }
    }
}

impl Default for InputActivityCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_collector_has_zero_counts() {
        let collector = InputActivityCollector::new();
        let snapshot = collector.take_snapshot();

        assert_eq!(snapshot.mouse.click_count, 0);
        assert_eq!(snapshot.keyboard.total_keystrokes, 0);
    }

    #[test]
    fn records_clicks() {
        let collector = InputActivityCollector::new();
        collector.record_click();
        collector.record_click();
        collector.record_double_click();

        let snapshot = collector.take_snapshot();
        assert_eq!(snapshot.mouse.click_count, 2);
        assert_eq!(snapshot.mouse.double_click_count, 1);
    }

    #[test]
    fn records_keystrokes() {
        let collector = InputActivityCollector::new();
        collector.record_keystroke(false, false);
        collector.record_keystroke(true, false); // shortcut
        collector.record_keystroke(false, true); // correction

        let snapshot = collector.take_snapshot();
        assert_eq!(snapshot.keyboard.total_keystrokes, 3);
        assert_eq!(snapshot.keyboard.shortcut_count, 1);
        assert_eq!(snapshot.keyboard.correction_count, 1);
    }

    #[test]
    fn snapshot_resets_counters() {
        let collector = InputActivityCollector::new();
        collector.record_click();
        collector.record_keystroke(false, false);

        let _ = collector.take_snapshot();
        let second = collector.take_snapshot();

        assert_eq!(second.mouse.click_count, 0);
        assert_eq!(second.keyboard.total_keystrokes, 0);
    }

    #[test]
    fn estimates_from_idle_change() {
        let collector = InputActivityCollector::new();
        // 유휴 시간이 10초에서 0초로 줄어듦 = 활동 있음
        collector.estimate_from_idle_change(10, 0);

        let snapshot = collector.take_snapshot();
        assert!(snapshot.keyboard.total_keystrokes > 0);
    }
}
