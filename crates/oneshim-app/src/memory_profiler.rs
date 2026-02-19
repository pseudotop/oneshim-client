//! 메모리 프로파일링 유틸리티
//!
//! 장시간 실행 시 메모리 누수를 감지하기 위한 도구.
//! RSS (Resident Set Size) 추적 및 증가율 분석.
//!
//! 현재는 통합 테스트에서만 사용되며, 향후 런타임 모니터링에 통합 예정.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// 메모리 스냅샷
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// RSS (Resident Set Size) in bytes
    pub rss_bytes: u64,
    /// 힙 할당량 (추정)
    pub heap_bytes: u64,
    /// 측정 시각
    pub timestamp: Instant,
}

/// 메모리 추적기
#[derive(Debug)]
pub struct MemoryTracker {
    /// 초기 RSS
    initial_rss: AtomicU64,
    /// 최대 RSS
    peak_rss: AtomicU64,
    /// 스냅샷 이력
    snapshots: parking_lot::Mutex<Vec<MemorySnapshot>>,
    /// 시작 시각
    start_time: Instant,
}

impl Default for MemoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryTracker {
    /// 새 추적기 생성
    pub fn new() -> Self {
        let initial = get_current_rss().unwrap_or(0);
        Self {
            initial_rss: AtomicU64::new(initial),
            peak_rss: AtomicU64::new(initial),
            snapshots: parking_lot::Mutex::new(Vec::with_capacity(1000)),
            start_time: Instant::now(),
        }
    }

    /// 현재 메모리 스냅샷 기록
    pub fn record_snapshot(&self) -> Option<MemorySnapshot> {
        let rss = get_current_rss()?;
        let snapshot = MemorySnapshot {
            rss_bytes: rss,
            heap_bytes: 0, // 플랫폼별 구현 필요
            timestamp: Instant::now(),
        };

        // 피크 업데이트
        self.peak_rss.fetch_max(rss, Ordering::Relaxed);

        // 이력 저장
        self.snapshots.lock().push(snapshot.clone());

        Some(snapshot)
    }

    /// 메모리 증가율 분석
    pub fn analyze(&self) -> MemoryAnalysis {
        let snapshots = self.snapshots.lock();
        let initial = self.initial_rss.load(Ordering::Relaxed);
        let peak = self.peak_rss.load(Ordering::Relaxed);
        let current = snapshots.last().map(|s| s.rss_bytes).unwrap_or(initial);
        let elapsed = self.start_time.elapsed();

        // 선형 회귀로 증가율 계산
        let growth_rate = if snapshots.len() >= 2 {
            calculate_growth_rate(&snapshots)
        } else {
            0.0
        };

        MemoryAnalysis {
            initial_rss: initial,
            current_rss: current,
            peak_rss: peak,
            elapsed,
            growth_rate_bytes_per_sec: growth_rate,
            snapshot_count: snapshots.len(),
            leak_suspected: growth_rate > 1024.0, // 1KB/s 이상이면 누수 의심
        }
    }

    /// 분석 결과 로그 출력
    pub fn log_analysis(&self) {
        let analysis = self.analyze();

        info!(
            "메모리 분석: initial={:.2}MB, current={:.2}MB, peak={:.2}MB, growth={:.2}KB/s, elapsed={:.1}s",
            analysis.initial_rss as f64 / 1024.0 / 1024.0,
            analysis.current_rss as f64 / 1024.0 / 1024.0,
            analysis.peak_rss as f64 / 1024.0 / 1024.0,
            analysis.growth_rate_bytes_per_sec / 1024.0,
            analysis.elapsed.as_secs_f64()
        );

        if analysis.leak_suspected {
            warn!(
                "⚠️ 메모리 누수 의심: {:.2}KB/s 증가율",
                analysis.growth_rate_bytes_per_sec / 1024.0
            );
        }
    }
}

/// 메모리 분석 결과
#[derive(Debug, Clone)]
pub struct MemoryAnalysis {
    /// 초기 RSS
    pub initial_rss: u64,
    /// 현재 RSS
    pub current_rss: u64,
    /// 최대 RSS
    pub peak_rss: u64,
    /// 경과 시간
    pub elapsed: Duration,
    /// 메모리 증가율 (bytes/sec)
    pub growth_rate_bytes_per_sec: f64,
    /// 스냅샷 수
    pub snapshot_count: usize,
    /// 누수 의심 여부
    pub leak_suspected: bool,
}

impl MemoryAnalysis {
    /// 증가량 (bytes)
    pub fn growth_bytes(&self) -> i64 {
        self.current_rss as i64 - self.initial_rss as i64
    }

    /// 증가율 (%)
    pub fn growth_percent(&self) -> f64 {
        if self.initial_rss == 0 {
            return 0.0;
        }
        (self.current_rss as f64 - self.initial_rss as f64) / self.initial_rss as f64 * 100.0
    }
}

/// 선형 회귀로 메모리 증가율 계산
fn calculate_growth_rate(snapshots: &[MemorySnapshot]) -> f64 {
    if snapshots.len() < 2 {
        return 0.0;
    }

    let first_time = snapshots[0].timestamp;
    let n = snapshots.len() as f64;

    // 시간(초)과 RSS 데이터
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_xx = 0.0;

    for s in snapshots {
        let x = s.timestamp.duration_since(first_time).as_secs_f64();
        let y = s.rss_bytes as f64;
        sum_x += x;
        sum_y += y;
        sum_xy += x * y;
        sum_xx += x * x;
    }

    // 기울기 계산 (최소제곱법)
    let denominator = n * sum_xx - sum_x * sum_x;
    if denominator.abs() < f64::EPSILON {
        return 0.0;
    }

    (n * sum_xy - sum_x * sum_y) / denominator
}

/// 현재 프로세스의 RSS 조회 (macOS)
#[cfg(target_os = "macos")]
pub fn get_current_rss() -> Option<u64> {
    use std::process::Command;

    let pid = std::process::id();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;

    let rss_kb: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;

    Some(rss_kb * 1024) // KB → bytes
}

/// 현재 프로세스의 RSS 조회 (Linux)
#[cfg(target_os = "linux")]
pub fn get_current_rss() -> Option<u64> {
    use std::fs;

    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let rss_kb: u64 = parts[1].parse().ok()?;
                return Some(rss_kb * 1024);
            }
        }
    }
    None
}

/// 현재 프로세스의 RSS 조회 (Windows)
#[cfg(target_os = "windows")]
pub fn get_current_rss() -> Option<u64> {
    // Windows에서는 GetProcessMemoryInfo 사용 필요
    // 간단한 구현을 위해 sysinfo 크레이트 활용 권장
    None
}

/// 현재 프로세스의 RSS 조회 (기타 플랫폼)
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn get_current_rss() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_memory_tracker_basic() {
        let tracker = MemoryTracker::new();

        // 스냅샷 기록
        let snap1 = tracker.record_snapshot();
        assert!(snap1.is_some());

        thread::sleep(Duration::from_millis(10));

        let snap2 = tracker.record_snapshot();
        assert!(snap2.is_some());

        // 분석
        let analysis = tracker.analyze();
        assert_eq!(analysis.snapshot_count, 2);
        assert!(analysis.initial_rss > 0);
    }

    #[test]
    fn test_get_current_rss() {
        let rss = get_current_rss();
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert!(rss.is_some(), "RSS 조회 실패");
            assert!(rss.unwrap() > 0, "RSS가 0");
        }
    }

    #[test]
    fn test_growth_rate_calculation() {
        let base = Instant::now();
        let snapshots = vec![
            MemorySnapshot {
                rss_bytes: 100_000_000,
                heap_bytes: 0,
                timestamp: base,
            },
            MemorySnapshot {
                rss_bytes: 101_000_000,
                heap_bytes: 0,
                timestamp: base + Duration::from_secs(1),
            },
            MemorySnapshot {
                rss_bytes: 102_000_000,
                heap_bytes: 0,
                timestamp: base + Duration::from_secs(2),
            },
        ];

        let rate = calculate_growth_rate(&snapshots);
        // 1MB/s 증가율 예상
        assert!((rate - 1_000_000.0).abs() < 10_000.0, "rate: {}", rate);
    }
}
