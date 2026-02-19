//! 메모리 프로파일링 통합 테스트
//!
//! 주요 컴포넌트의 장시간 실행 시 메모리 누수를 검사합니다.
//!
//! 실행:
//! ```
//! cargo test -p oneshim-app --test memory_profile_test -- --nocapture --ignored
//! ```
//!
//! 또는 릴리즈 모드:
//! ```
//! cargo test -p oneshim-app --test memory_profile_test --release -- --nocapture --ignored
//! ```

use image::{DynamicImage, Rgba, RgbaImage};
use oneshim_core::models::event::{Event, UserEvent, UserEventType};
use oneshim_core::models::frame::FrameMetadata;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_vision::{delta, encoder, encoder::WebPQuality, thumbnail};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use uuid::Uuid;

/// 메모리 스냅샷
#[derive(Clone)]
struct MemorySnapshot {
    rss_bytes: u64,
    timestamp: Instant,
}

/// RSS 조회 (macOS)
fn get_rss() -> u64 {
    use std::process::Command;

    let pid = std::process::id();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .expect("ps 명령 실패");

    let rss_kb: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0);

    rss_kb * 1024
}

/// 테스트용 이미지 생성
fn create_test_image(width: u32, height: u32, seed: u8) -> DynamicImage {
    let mut img = RgbaImage::new(width, height);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let r = (x as u8).wrapping_add(seed).wrapping_mul(17);
        let g = (y as u8).wrapping_add(seed).wrapping_mul(31);
        let b = (x as u8).wrapping_add(y as u8).wrapping_add(seed);
        *pixel = Rgba([r, g, b, 255]);
    }
    DynamicImage::ImageRgba8(img)
}

/// 워밍업 후 안정 구간의 메모리 증가율 계산 (bytes/sec)
///
/// 처음 `warmup_ratio` 비율의 데이터를 건너뛰고 나머지로 계산
fn calculate_stable_growth_rate(snapshots: &[MemorySnapshot], warmup_ratio: f64) -> f64 {
    let warmup_count = (snapshots.len() as f64 * warmup_ratio).ceil() as usize;
    let stable_snapshots = &snapshots[warmup_count..];

    if stable_snapshots.len() < 2 {
        return 0.0;
    }

    // 선형 회귀로 증가율 계산
    let first_time = stable_snapshots[0].timestamp;
    let n = stable_snapshots.len() as f64;

    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_xx = 0.0;

    for s in stable_snapshots {
        let x = s.timestamp.duration_since(first_time).as_secs_f64();
        let y = s.rss_bytes as f64;
        sum_x += x;
        sum_y += y;
        sum_xy += x * y;
        sum_xx += x * x;
    }

    let denominator = n * sum_xx - sum_x * sum_x;
    if denominator.abs() < f64::EPSILON {
        return 0.0;
    }

    (n * sum_xy - sum_x * sum_y) / denominator
}

/// 안정 구간에서 메모리 변동폭 확인
///
/// 마지막 N개 스냅샷의 최대-최소 차이를 반환
fn calculate_memory_variance(snapshots: &[MemorySnapshot], last_n: usize) -> u64 {
    if snapshots.len() < last_n {
        return u64::MAX;
    }

    let tail = &snapshots[snapshots.len() - last_n..];
    let min = tail.iter().map(|s| s.rss_bytes).min().unwrap_or(0);
    let max = tail.iter().map(|s| s.rss_bytes).max().unwrap_or(0);

    max - min
}

/// 누수 검사 결과
struct LeakCheckResult {
    /// 워밍업 후 안정 구간 증가율 (bytes/sec)
    stable_growth_rate: f64,
    /// 마지막 구간 메모리 변동폭 (bytes)
    memory_variance: u64,
    /// 초기 RSS
    initial_rss: u64,
    /// 피크 RSS
    peak_rss: u64,
    /// 최종 RSS
    final_rss: u64,
    /// 누수 의심 여부
    leak_suspected: bool,
}

impl LeakCheckResult {
    fn from_snapshots(snapshots: &[MemorySnapshot]) -> Self {
        let initial_rss = snapshots.first().map(|s| s.rss_bytes).unwrap_or(0);
        let peak_rss = snapshots.iter().map(|s| s.rss_bytes).max().unwrap_or(0);
        let final_rss = snapshots.last().map(|s| s.rss_bytes).unwrap_or(0);

        // 워밍업 30% 건너뛰기
        let stable_growth_rate = calculate_stable_growth_rate(snapshots, 0.3);
        // 마지막 5개 스냅샷의 변동폭
        let memory_variance = calculate_memory_variance(snapshots, 5);

        // 누수 판정 조건:
        // 1. 안정 구간 증가율 > 50KB/s AND
        // 2. 마지막 구간 변동폭 > 10MB (메모리가 계속 증가 중)
        let leak_suspected = stable_growth_rate > 50_000.0 && memory_variance > 10 * 1024 * 1024;

        Self {
            stable_growth_rate,
            memory_variance,
            initial_rss,
            peak_rss,
            final_rss,
            leak_suspected,
        }
    }

    fn print_summary(&self, test_name: &str, elapsed: Duration, iterations: u64) {
        println!("\n=== {} 결과 ===", test_name);
        println!(
            "초기 RSS: {:.2} MB",
            self.initial_rss as f64 / 1024.0 / 1024.0
        );
        println!("피크 RSS: {:.2} MB", self.peak_rss as f64 / 1024.0 / 1024.0);
        println!(
            "최종 RSS: {:.2} MB",
            self.final_rss as f64 / 1024.0 / 1024.0
        );
        println!(
            "메모리 증가: {:.2} MB ({:+.1}%)",
            (self.final_rss as i64 - self.initial_rss as i64) as f64 / 1024.0 / 1024.0,
            (self.final_rss as f64 - self.initial_rss as f64) / self.initial_rss as f64 * 100.0
        );
        println!(
            "안정 구간 증가율: {:.2} KB/s (워밍업 30% 제외)",
            self.stable_growth_rate / 1024.0
        );
        println!(
            "마지막 구간 변동폭: {:.2} MB",
            self.memory_variance as f64 / 1024.0 / 1024.0
        );
        println!("실행 시간: {:.2}s", elapsed.as_secs_f64());
        println!(
            "처리량: {:.1} iterations/s",
            iterations as f64 / elapsed.as_secs_f64()
        );

        if self.leak_suspected {
            println!("\n⚠️ 경고: 메모리 누수 의심");
            println!(
                "  - 안정 구간 증가율: {:.2} KB/s",
                self.stable_growth_rate / 1024.0
            );
            println!("  - 메모리가 안정화되지 않음");
        } else if self.stable_growth_rate > 10_000.0 {
            println!("\n🔶 주의: 높은 메모리 사용, 그러나 안정화됨");
        } else {
            println!("\n✅ 메모리 안정적 - 누수 없음");
        }
    }
}

/// Vision 파이프라인 메모리 테스트
///
/// 델타 인코딩, 썸네일, WebP 인코딩을 반복 실행하며 메모리 추적
#[test]
#[ignore = "장시간 실행 테스트 - cargo test --ignored"]
fn test_vision_pipeline_memory() {
    const ITERATIONS: usize = 200;
    const SAMPLE_INTERVAL: usize = 5;

    println!("\n=== Vision 파이프라인 메모리 테스트 ===");
    println!("반복 횟수: {}", ITERATIONS);

    let mut snapshots = Vec::with_capacity(ITERATIONS / SAMPLE_INTERVAL + 2);
    let start = Instant::now();

    // 초기 메모리
    snapshots.push(MemorySnapshot {
        rss_bytes: get_rss(),
        timestamp: Instant::now(),
    });

    // 테스트 이미지 생성 (워밍업)
    let img1 = create_test_image(1920, 1080, 42);
    let img2 = create_test_image(1920, 1080, 43);

    // 워밍업 후 메모리 기록
    snapshots.push(MemorySnapshot {
        rss_bytes: get_rss(),
        timestamp: Instant::now(),
    });

    for i in 0..ITERATIONS {
        // 델타 계산
        let _delta = delta::compute_delta(&img1, &img2);

        // 썸네일 생성
        let thumb = thumbnail::fast_resize(&img2, 480, 270).unwrap();

        // WebP 인코딩
        let _encoded = encoder::encode_webp(&thumb, WebPQuality::Medium).unwrap();

        // 주기적 메모리 샘플링
        if i % SAMPLE_INTERVAL == 0 {
            snapshots.push(MemorySnapshot {
                rss_bytes: get_rss(),
                timestamp: Instant::now(),
            });
        }
    }

    // 최종 메모리
    snapshots.push(MemorySnapshot {
        rss_bytes: get_rss(),
        timestamp: Instant::now(),
    });

    let elapsed = start.elapsed();
    let result = LeakCheckResult::from_snapshots(&snapshots);
    result.print_summary("Vision 파이프라인", elapsed, ITERATIONS as u64);

    // 누수 검사 (안정 구간 기준)
    assert!(
        !result.leak_suspected,
        "메모리 누수 의심: 안정 구간 증가율 {:.2} KB/s, 변동폭 {:.2} MB",
        result.stable_growth_rate / 1024.0,
        result.memory_variance as f64 / 1024.0 / 1024.0
    );
}

/// Storage 작업 메모리 테스트
///
/// SQLite 이벤트/프레임 저장을 반복 실행하며 메모리 추적
#[test]
#[ignore = "장시간 실행 테스트 - cargo test --ignored"]
fn test_storage_memory() {
    const ITERATIONS: usize = 500;
    const BATCH_SIZE: usize = 10;
    const SAMPLE_INTERVAL: usize = 25;

    println!("\n=== Storage 메모리 테스트 ===");
    println!("반복 횟수: {} (배치 크기: {})", ITERATIONS, BATCH_SIZE);

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::open(&db_path, 30).unwrap();

    let mut snapshots = Vec::with_capacity(ITERATIONS / SAMPLE_INTERVAL + 2);
    let start = Instant::now();

    // 초기 메모리
    snapshots.push(MemorySnapshot {
        rss_bytes: get_rss(),
        timestamp: Instant::now(),
    });

    for i in 0..ITERATIONS {
        // 이벤트 배치 저장
        let events: Vec<Event> = (0..BATCH_SIZE)
            .map(|j| {
                Event::User(UserEvent {
                    event_id: Uuid::new_v4(),
                    event_type: UserEventType::WindowChange,
                    timestamp: chrono::Utc::now(),
                    app_name: format!("App{}", j % 5),
                    window_title: format!("Window {} - {}", i, j),
                })
            })
            .collect();

        let _ = storage.save_events_batch(&events);

        // 프레임 메타데이터 저장
        let metadata = FrameMetadata {
            timestamp: chrono::Utc::now(),
            trigger_type: "AppSwitch".to_string(),
            app_name: format!("App{}", i % 10),
            window_title: format!("Window {}", i),
            resolution: (1920, 1080),
            importance: 0.5,
        };
        let _ = storage.save_frame_metadata(&metadata, Some(&format!("frames/{}.webp", i)), None);

        // Focus 메트릭 업데이트
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let _ = storage.get_or_create_focus_metrics(&date);
        let _ = storage.increment_focus_metrics(&date, 1, 1, 0, 0, 0);

        // 주기적 메모리 샘플링
        if i % SAMPLE_INTERVAL == 0 {
            snapshots.push(MemorySnapshot {
                rss_bytes: get_rss(),
                timestamp: Instant::now(),
            });
        }
    }

    // 최종 메모리
    snapshots.push(MemorySnapshot {
        rss_bytes: get_rss(),
        timestamp: Instant::now(),
    });

    let elapsed = start.elapsed();
    let result = LeakCheckResult::from_snapshots(&snapshots);
    result.print_summary("Storage", elapsed, ITERATIONS as u64);

    println!(
        "저장된 데이터: {} 이벤트, {} 프레임",
        ITERATIONS * BATCH_SIZE,
        ITERATIONS
    );

    assert!(
        !result.leak_suspected,
        "메모리 누수 의심: 안정 구간 증가율 {:.2} KB/s, 변동폭 {:.2} MB",
        result.stable_growth_rate / 1024.0,
        result.memory_variance as f64 / 1024.0 / 1024.0
    );
}

/// 복합 시나리오 메모리 테스트
///
/// Vision + Storage를 동시에 실행하며 메모리 추적
#[test]
#[ignore = "장시간 실행 테스트 - cargo test --ignored"]
fn test_combined_memory() {
    const DURATION_SECS: u64 = 30;

    println!("\n=== 복합 시나리오 메모리 테스트 ===");
    println!("실행 시간: {}초", DURATION_SECS);

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::open(&db_path, 30).unwrap();

    let img1 = create_test_image(1920, 1080, 42);
    let img2 = create_test_image(1920, 1080, 43);

    let mut snapshots = Vec::new();
    let start = Instant::now();
    let iteration_count = AtomicU64::new(0);

    // 초기 메모리
    snapshots.push(MemorySnapshot {
        rss_bytes: get_rss(),
        timestamp: Instant::now(),
    });

    // 지정 시간 동안 반복
    let mut last_sample = Instant::now();
    while start.elapsed() < Duration::from_secs(DURATION_SECS) {
        let iter = iteration_count.fetch_add(1, Ordering::Relaxed);

        // Vision 작업
        let _delta = delta::compute_delta(&img1, &img2);
        let thumb = thumbnail::fast_resize(&img2, 480, 270).unwrap();
        let _encoded = encoder::encode_webp(&thumb, WebPQuality::Medium).unwrap();

        // Storage 작업
        let events: Vec<Event> = (0..5)
            .map(|j| {
                Event::User(UserEvent {
                    event_id: Uuid::new_v4(),
                    event_type: UserEventType::WindowChange,
                    timestamp: chrono::Utc::now(),
                    app_name: format!("App{}", j),
                    window_title: format!("Window {}", iter),
                })
            })
            .collect();
        let _ = storage.save_events_batch(&events);

        // 메모리 샘플링 (1초마다)
        if last_sample.elapsed() >= Duration::from_secs(1) {
            snapshots.push(MemorySnapshot {
                rss_bytes: get_rss(),
                timestamp: Instant::now(),
            });
            last_sample = Instant::now();
        }

        // 실제 사용 시나리오 시뮬레이션 (약간의 딜레이)
        std::thread::sleep(Duration::from_millis(10));
    }

    // 최종 메모리
    snapshots.push(MemorySnapshot {
        rss_bytes: get_rss(),
        timestamp: Instant::now(),
    });

    let elapsed = start.elapsed();
    let total_iterations = iteration_count.load(Ordering::Relaxed);
    let result = LeakCheckResult::from_snapshots(&snapshots);
    result.print_summary("복합 시나리오", elapsed, total_iterations);

    // 메모리 추이 출력
    println!("\n--- 메모리 추이 (5초 간격) ---");
    for (i, snap) in snapshots.iter().enumerate() {
        if i % 5 == 0 || i == snapshots.len() - 1 {
            println!(
                "  {:3}s: {:.2} MB",
                snap.timestamp.duration_since(start).as_secs(),
                snap.rss_bytes as f64 / 1024.0 / 1024.0
            );
        }
    }

    assert!(
        !result.leak_suspected,
        "메모리 누수 의심: 안정 구간 증가율 {:.2} KB/s, 변동폭 {:.2} MB",
        result.stable_growth_rate / 1024.0,
        result.memory_variance as f64 / 1024.0 / 1024.0
    );
}
