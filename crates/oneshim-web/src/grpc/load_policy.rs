//! D13-v2b dashboard gRPC — per-stream load classifier + enforcement ladder.
//!
//! Inputs: system-wide `cpu_usage` (%) and `free_mem_gb` derived from
//! `SystemMetrics`. Outputs: a 4-level `LoadLevel` + per-level
//! enforcement clamps for `SubscribeMetrics` interval and (forward-compat)
//! `SubscribeEvents` rate limits. See spec §4.1.

use std::time::{Duration, Instant};

// `sections` is `mod sections;` (private) in oneshim-core::config; `pub use sections::*;`
// re-exports `LoadThresholds` at the config root.
use oneshim_core::config::LoadThresholds;
use oneshim_core::models::system::SystemMetrics;

/// Hard floor for `SubscribeMetrics` emission interval — prevents tight polling.
pub const INTERVAL_FLOOR: Duration = Duration::from_millis(250);
/// Hard ceiling for `SubscribeMetrics` emission interval — ensures hints land within a minute.
pub const INTERVAL_CEILING: Duration = Duration::from_secs(60);
/// Warm-up window after `LoadPolicy::new` — classification forced to `Medium` (sysinfo stabilisation).
pub const WARMUP: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Load classifier + enforcement ladder. Stateless except for the `started_at`
/// timestamp used to detect the 30s warm-up window.
pub struct LoadPolicy {
    thresholds: LoadThresholds,
    started_at: Instant,
}

impl LoadPolicy {
    /// Construct a new policy. Panics if thresholds are not strictly increasing
    /// in `cpu_low < cpu_medium < cpu_high <= 100.0` — callers must validate
    /// config before construction.
    pub fn new(thresholds: LoadThresholds) -> Self {
        assert!(
            thresholds.cpu_low_pct < thresholds.cpu_medium_pct,
            "LoadThresholds: cpu_low_pct ({}) must be < cpu_medium_pct ({})",
            thresholds.cpu_low_pct,
            thresholds.cpu_medium_pct,
        );
        assert!(
            thresholds.cpu_medium_pct < thresholds.cpu_high_pct,
            "LoadThresholds: cpu_medium_pct ({}) must be < cpu_high_pct ({})",
            thresholds.cpu_medium_pct,
            thresholds.cpu_high_pct,
        );
        assert!(
            thresholds.cpu_high_pct <= 100.0,
            "LoadThresholds: cpu_high_pct ({}) must be <= 100.0",
            thresholds.cpu_high_pct,
        );
        Self {
            thresholds,
            started_at: Instant::now(),
        }
    }

    pub fn thresholds(&self) -> &LoadThresholds {
        &self.thresholds
    }

    /// True iff less than `WARMUP` has elapsed since `::new()`.
    pub fn is_in_warmup(&self) -> bool {
        self.started_at.elapsed() < WARMUP
    }

    /// Classify a fresh `SystemMetrics` snapshot into a `LoadLevel`. See spec §4.1.
    pub fn classify(&self, metrics: &SystemMetrics) -> LoadLevel {
        if self.is_in_warmup() {
            return LoadLevel::Medium;
        }
        let cpu_pct = metrics.cpu_usage;
        let free_mem_gb =
            metrics.memory_total.saturating_sub(metrics.memory_used) as f32 / 1_073_741_824.0;
        let t = &self.thresholds;

        if cpu_pct < t.cpu_low_pct && free_mem_gb > t.min_free_mem_gb * 1.5 {
            LoadLevel::Low
        } else if cpu_pct < t.cpu_medium_pct && free_mem_gb > t.min_free_mem_gb {
            LoadLevel::Medium
        } else if cpu_pct < t.cpu_high_pct && free_mem_gb > t.min_free_mem_gb * 0.5 {
            LoadLevel::High
        } else {
            LoadLevel::Critical
        }
    }

    /// Effective `SubscribeMetrics` interval for the requested value at the
    /// given level. `requested_secs = 0` means realtime — floor to `INTERVAL_FLOOR`.
    pub fn enforced_metrics_interval(&self, level: LoadLevel, requested_secs: u32) -> Duration {
        let requested = if requested_secs == 0 {
            INTERVAL_FLOOR
        } else {
            Duration::from_secs(requested_secs as u64)
        };
        let level_floor = match level {
            LoadLevel::Low => Duration::from_millis(250),
            LoadLevel::Medium => Duration::from_secs(1),
            LoadLevel::High => Duration::from_secs(5),
            LoadLevel::Critical => Duration::from_secs(30),
        };
        requested.max(level_floor).min(INTERVAL_CEILING)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_metrics(cpu: f32, used_gib: u64, total_gib: u64) -> SystemMetrics {
        SystemMetrics {
            timestamp: chrono::Utc::now(),
            cpu_usage: cpu,
            memory_used: used_gib * 1_073_741_824,
            memory_total: total_gib * 1_073_741_824,
            disk_used: 0,
            disk_total: 0,
            network: None,
            typing_wpm: 0.0,
        }
    }

    /// Build a policy and rewind `started_at` so `classify` runs the real branch
    /// rather than the warm-up `Medium` shortcut.
    fn mk_policy_past_warmup() -> LoadPolicy {
        let mut p = LoadPolicy::new(LoadThresholds::default());
        p.started_at = Instant::now() - Duration::from_secs(3600);
        p
    }

    #[test]
    fn classify_low_when_cpu_under_50_and_mem_above_3gb() {
        let p = mk_policy_past_warmup();
        // cpu < 50 AND free = 16-8 = 8 GiB > 2.0 * 1.5 = 3.0
        let m = mk_metrics(30.0, 8, 16);
        assert_eq!(p.classify(&m), LoadLevel::Low);
    }

    #[test]
    fn classify_medium_between_low_and_medium_pct() {
        let p = mk_policy_past_warmup();
        // cpu between 50 and 70; free = 6 GiB > 2.0
        let m = mk_metrics(60.0, 10, 16);
        assert_eq!(p.classify(&m), LoadLevel::Medium);
    }

    #[test]
    fn classify_high_between_medium_and_high_pct() {
        let p = mk_policy_past_warmup();
        // cpu between 70 and 90; free = 2 GiB > 1.0 (half of min)
        let m = mk_metrics(80.0, 14, 16);
        assert_eq!(p.classify(&m), LoadLevel::High);
    }

    #[test]
    fn classify_critical_at_cpu_high_pct_or_above() {
        let p = mk_policy_past_warmup();
        let m = mk_metrics(95.0, 14, 16);
        assert_eq!(p.classify(&m), LoadLevel::Critical);
    }

    #[test]
    fn classify_critical_when_free_mem_below_half_min() {
        let p = mk_policy_past_warmup();
        // cpu low but free = 1 GiB < half of min (1.0)
        let m = mk_metrics(30.0, 15, 16);
        assert_eq!(p.classify(&m), LoadLevel::Critical);
    }

    #[test]
    fn warmup_30s_forces_medium_regardless_of_metrics() {
        let p = LoadPolicy::new(LoadThresholds::default());
        let m_low = mk_metrics(10.0, 1, 16);
        let m_high = mk_metrics(99.0, 15, 16);
        assert_eq!(p.classify(&m_low), LoadLevel::Medium);
        assert_eq!(p.classify(&m_high), LoadLevel::Medium);
    }

    #[test]
    fn enforced_interval_honors_floor_250ms() {
        let p = mk_policy_past_warmup();
        assert_eq!(
            p.enforced_metrics_interval(LoadLevel::Low, 0),
            Duration::from_millis(250),
        );
    }

    #[test]
    fn enforced_interval_honors_ceiling_60s() {
        let p = mk_policy_past_warmup();
        assert_eq!(
            p.enforced_metrics_interval(LoadLevel::Critical, 999_999),
            Duration::from_secs(60),
        );
    }

    #[test]
    fn enforced_interval_picks_larger_of_request_and_level_floor() {
        let p = mk_policy_past_warmup();
        // High level floor = 5s; request 2s → 5s wins.
        assert_eq!(
            p.enforced_metrics_interval(LoadLevel::High, 2),
            Duration::from_secs(5),
        );
        // Medium level floor = 1s; request 3s → 3s wins.
        assert_eq!(
            p.enforced_metrics_interval(LoadLevel::Medium, 3),
            Duration::from_secs(3),
        );
    }
}
