use chrono::{DateTime, Utc};
use oneshim_core::models::event::{InputActivityEvent, KeyboardActivity, MouseActivity};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

pub struct InputActivityCollector {
    period_start: Mutex<DateTime<Utc>>,
    current_app: Mutex<String>,

    click_count: AtomicU32,
    scroll_count: AtomicU32,
    double_click_count: AtomicU32,
    right_click_count: AtomicU32,
    move_distance: AtomicU64, // f64 save
    total_keystrokes: AtomicU32,
    typing_bursts: AtomicU32,
    shortcut_count: AtomicU32,
    correction_count: AtomicU32,

    last_activity_ms: AtomicU64,
    burst_threshold_ms: u64,
}

impl InputActivityCollector {
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
            burst_threshold_ms: 2000, // 2 s
        }
    }

    pub fn set_current_app(&self, app_name: &str) {
        if let Ok(mut app) = self.current_app.lock() {
            *app = app_name.to_string();
        }
    }

    pub fn record_click(&self) {
        self.click_count.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }

    pub fn record_double_click(&self) {
        self.double_click_count.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }

    pub fn record_right_click(&self) {
        self.right_click_count.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }

    pub fn record_scroll(&self) {
        self.scroll_count.fetch_add(1, Ordering::Relaxed);
        self.record_activity();
    }

    pub fn record_mouse_move(&self, distance: f64) {
        let bits = self.move_distance.load(Ordering::Relaxed);
        let current = f64::from_bits(bits);
        let new_bits = (current + distance).to_bits();
        self.move_distance.store(new_bits, Ordering::Relaxed);
    }

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

    fn record_activity(&self) {
        let now_ms = Utc::now().timestamp_millis() as u64;
        let last_ms = self.last_activity_ms.swap(now_ms, Ordering::Relaxed);

        if last_ms > 0 && now_ms.saturating_sub(last_ms) > self.burst_threshold_ms {
            self.typing_bursts.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn estimate_from_idle_change(&self, prev_idle_secs: u64, curr_idle_secs: u64) {
        if curr_idle_secs < prev_idle_secs {
            let estimated_keystrokes = (prev_idle_secs - curr_idle_secs).min(10) as u32;
            self.total_keystrokes
                .fetch_add(estimated_keystrokes, Ordering::Relaxed);

            self.record_activity();
        }
    }

    pub fn take_snapshot(&self) -> InputActivityEvent {
        let now = Utc::now();

        let period_secs = {
            let mut start = self.period_start.lock().unwrap();
            let duration = (now - *start).num_seconds().max(1) as u32;
            *start = now;
            duration
        };

        let app_name = self
            .current_app
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default();

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
                last_position: None, // privacy-safe by default
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
        collector.estimate_from_idle_change(10, 0);

        let snapshot = collector.take_snapshot();
        assert!(snapshot.keyboard.total_keystrokes > 0);
    }
}
