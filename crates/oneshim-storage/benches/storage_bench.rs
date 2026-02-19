//! oneshim-storage 성능 벤치마크
//!
//! 실행: cargo bench -p oneshim-storage
//!
//! 벤치마크 대상:
//! - 이벤트 배치 저장
//! - 프레임 메타데이터 저장
//! - Focus 메트릭 조회/업데이트
//! - 태그 CRUD 연산

// 벤치마크 코드에서 criterion 패턴 관련 clippy 경고 허용
#![allow(clippy::redundant_closure, clippy::unit_arg)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oneshim_core::models::event::{ContextEvent, Event, UserEvent, UserEventType};
use oneshim_core::models::frame::FrameMetadata;
use oneshim_storage::sqlite::SqliteStorage;
use tempfile::TempDir;
use uuid::Uuid;

/// 테스트용 UserEvent 생성
fn create_test_user_event(i: usize) -> Event {
    Event::User(UserEvent {
        event_id: Uuid::new_v4(),
        event_type: UserEventType::WindowChange,
        timestamp: chrono::Utc::now(),
        app_name: format!("TestApp{}", i % 10),
        window_title: format!("Test Window {}", i),
    })
}

/// 테스트용 ContextEvent 생성
fn create_test_context_event(i: usize) -> Event {
    Event::Context(ContextEvent {
        app_name: format!("App{}", i % 5),
        window_title: format!("Window {}", i),
        prev_app_name: Some(format!("PrevApp{}", (i + 1) % 5)),
        timestamp: chrono::Utc::now(),
    })
}

/// 임시 SQLite 스토리지 생성
fn create_temp_storage() -> (SqliteStorage, TempDir) {
    let temp_dir = TempDir::new().expect("임시 디렉토리 생성 실패");
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::open(&db_path, 30).expect("스토리지 생성 실패");
    (storage, temp_dir)
}

/// 이벤트 배치 저장 벤치마크
fn bench_event_batch_save(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_batch_save");

    let batch_sizes = [10, 50, 100, 500];

    for batch_size in batch_sizes {
        group.throughput(Throughput::Elements(batch_size as u64));

        let events: Vec<Event> = (0..batch_size).map(create_test_user_event).collect();

        group.bench_with_input(
            BenchmarkId::new("user_events", batch_size),
            &events,
            |b, events| {
                b.iter_with_setup(
                    || create_temp_storage(),
                    |(storage, _temp): (SqliteStorage, TempDir)| {
                        black_box(storage.save_events_batch(events).unwrap());
                    },
                );
            },
        );

        let context_events: Vec<Event> = (0..batch_size).map(create_test_context_event).collect();

        group.bench_with_input(
            BenchmarkId::new("context_events", batch_size),
            &context_events,
            |b, events| {
                b.iter_with_setup(
                    || create_temp_storage(),
                    |(storage, _temp): (SqliteStorage, TempDir)| {
                        black_box(storage.save_events_batch(events).unwrap());
                    },
                );
            },
        );
    }

    group.finish();
}

/// 프레임 메타데이터 저장 벤치마크
fn bench_frame_metadata_save(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_metadata_save");

    let counts = [10, 50, 100];

    for count in counts {
        group.throughput(Throughput::Elements(count as u64));

        group.bench_with_input(BenchmarkId::new("frames", count), &count, |b, &count| {
            b.iter_with_setup(
                || {
                    let (storage, temp) = create_temp_storage();
                    let frames: Vec<_> = (0..count)
                        .map(|i| {
                            let metadata = FrameMetadata {
                                timestamp: chrono::Utc::now(),
                                trigger_type: "AppSwitch".to_string(),
                                app_name: format!("App{}", i % 5),
                                window_title: format!("Window {}", i),
                                resolution: (1920, 1080),
                                importance: 0.5 + (i % 5) as f32 * 0.1,
                            };
                            let path = format!("frames/2026-01-31/{:03}.webp", i);
                            (metadata, path)
                        })
                        .collect();
                    (storage, temp, frames)
                },
                |(storage, _temp, frames): (
                    SqliteStorage,
                    TempDir,
                    Vec<(FrameMetadata, String)>,
                )| {
                    for (metadata, path) in &frames {
                        black_box(
                            storage
                                .save_frame_metadata(metadata, Some(path.as_str()), None)
                                .unwrap(),
                        );
                    }
                },
            );
        });
    }

    group.finish();
}

/// Focus 메트릭 조회/업데이트 벤치마크
fn bench_focus_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("focus_metrics");

    // get_or_create 벤치마크
    group.bench_function("get_or_create", |b| {
        b.iter_with_setup(
            || create_temp_storage(),
            |(storage, _temp): (SqliteStorage, TempDir)| {
                black_box(storage.get_or_create_today_focus_metrics().unwrap());
            },
        );
    });

    // increment 벤치마크 (연속 업데이트)
    group.bench_function("increment_100x", |b| {
        b.iter_with_setup(
            || {
                let (storage, temp) = create_temp_storage();
                storage.get_or_create_today_focus_metrics().unwrap();
                (storage, temp)
            },
            |(storage, _temp): (SqliteStorage, TempDir)| {
                for _ in 0..100 {
                    black_box(
                        storage
                            .increment_focus_metrics(
                                &chrono::Utc::now().format("%Y-%m-%d").to_string(),
                                1, // total_active_secs
                                1, // deep_work_secs
                                0, // communication_secs
                                0, // context_switches
                                0, // interruption_count
                            )
                            .unwrap(),
                    );
                }
            },
        );
    });

    // 최근 메트릭 조회 벤치마크
    group.bench_function("get_recent_7days", |b| {
        b.iter_with_setup(
            || {
                let (storage, temp) = create_temp_storage();
                // 7일치 데이터 생성
                for i in 0..7 {
                    let date = (chrono::Utc::now() - chrono::Duration::days(i))
                        .format("%Y-%m-%d")
                        .to_string();
                    storage.get_or_create_focus_metrics(&date).unwrap();
                }
                (storage, temp)
            },
            |(storage, _temp): (SqliteStorage, TempDir)| {
                black_box(storage.get_recent_focus_metrics(7).unwrap());
            },
        );
    });

    group.finish();
}

/// 태그 CRUD 벤치마크
fn bench_tags(c: &mut Criterion) {
    let mut group = c.benchmark_group("tags");

    // 태그 생성
    group.bench_function("create_10", |b| {
        b.iter_with_setup(
            || create_temp_storage(),
            |(storage, _temp): (SqliteStorage, TempDir)| {
                for i in 0..10 {
                    black_box(storage.create_tag(&format!("Tag{}", i), "#FF5733").unwrap());
                }
            },
        );
    });

    // 모든 태그 조회
    group.bench_function("get_all_50tags", |b| {
        b.iter_with_setup(
            || {
                let (storage, temp) = create_temp_storage();
                for i in 0..50 {
                    storage.create_tag(&format!("Tag{}", i), "#FF5733").unwrap();
                }
                (storage, temp)
            },
            |(storage, _temp): (SqliteStorage, TempDir)| {
                black_box(storage.get_all_tags().unwrap());
            },
        );
    });

    // 프레임에 태그 추가
    group.bench_function("add_tag_to_frame", |b| {
        b.iter_with_setup(
            || {
                let (storage, temp) = create_temp_storage();
                // 프레임 생성
                let metadata = FrameMetadata {
                    timestamp: chrono::Utc::now(),
                    trigger_type: "AppSwitch".to_string(),
                    app_name: "TestApp".to_string(),
                    window_title: "Window".to_string(),
                    resolution: (1920, 1080),
                    importance: 0.8,
                };
                let frame_id = storage
                    .save_frame_metadata(&metadata, Some("frames/test.webp"), None)
                    .unwrap();
                // 태그 생성
                let tag = storage.create_tag("TestTag", "#FF5733").unwrap();
                (storage, temp, frame_id, tag.id)
            },
            |(storage, _temp, frame_id, tag_id): (SqliteStorage, TempDir, i64, i64)| {
                // 태그 제거 후 다시 추가 (중복 방지)
                let _ = storage.remove_tag_from_frame(frame_id, tag_id);
                black_box(storage.add_tag_to_frame(frame_id, tag_id).unwrap());
            },
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_event_batch_save,
    bench_frame_metadata_save,
    bench_focus_metrics,
    bench_tags
);
criterion_main!(benches);
