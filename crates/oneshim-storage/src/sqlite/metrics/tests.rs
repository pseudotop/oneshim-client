//! Inline unit tests for `sqlite::metrics`.
//!
//! Test harness convention per the Phase 5-D8 spec
//! (`docs/reviews/2026-04-18-phase5-d8-storage-test-backfill-spec.md`):
//! each test module defines its own `open_db()` locally. Do NOT centralize
//! this helper into `test_utils.rs` — that is an explicit follow-up item.

// Helpers + imports below are populated progressively across Tasks 2-10
// of the Phase 5-D8 PR1 plan. Some are unused on any given task commit
// until the corresponding tests land; allow the warnings so incremental
// commits pass the lefthook clippy hook. These attributes are removed
// in Task 11 (final verify) once all tests are in place.
#![allow(dead_code, unused_imports)]

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Timelike, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::activity::{ProcessSnapshot, ProcessSnapshotEntry, SessionStats};
use oneshim_core::models::system::{NetworkInfo, SystemMetrics};
use oneshim_core::ports::storage::MetricsStorage;
use rusqlite::Connection;
use tempfile::TempDir;

use crate::sqlite::SqliteStorage;

// ── Harness ─────────────────────────────────────────────────────

/// Opens a fresh on-disk SQLite DB with all migrations applied.
/// Returned `TempDir` must outlive the test; drop order matters
/// (connection must drop before tempdir).
#[allow(dead_code)]
fn open_db() -> (TempDir, Arc<Mutex<Connection>>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = Connection::open(dir.path().join("t.db")).expect("open sqlite");
    crate::migration::run_migrations(&conn).expect("run_migrations");
    (dir, Arc::new(Mutex::new(conn)))
}

/// Opens an in-memory `SqliteStorage` with the standard 30-day retention.
/// Used by tests that need the full `SqliteStorage` API (port methods).
fn open_storage() -> SqliteStorage {
    SqliteStorage::open_in_memory(30).expect("in-memory storage")
}

/// Truncate `Utc::now()` to the start of its own hour.
/// Used so tests never rely on hardcoded calendar dates.
fn current_hour_start() -> DateTime<Utc> {
    Utc::now()
        .with_minute(0)
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .expect("truncation to hour should always succeed")
}

// ── Sample builders ─────────────────────────────────────────────

fn sample_metrics(ts: DateTime<Utc>, cpu: f32) -> SystemMetrics {
    SystemMetrics {
        timestamp: ts,
        cpu_usage: cpu,
        memory_used: 8_000_000_000,
        memory_total: 16_000_000_000,
        disk_used: 100_000_000_000,
        disk_total: 500_000_000_000,
        network: Some(NetworkInfo {
            upload_speed: 1_000,
            download_speed: 2_000,
            is_connected: true,
        }),
        typing_wpm: 0.0,
    }
}

fn sample_metrics_no_network(ts: DateTime<Utc>, cpu: f32) -> SystemMetrics {
    let mut m = sample_metrics(ts, cpu);
    m.network = None;
    m
}

fn sample_process_snapshot(ts: DateTime<Utc>, count: usize) -> ProcessSnapshot {
    ProcessSnapshot {
        timestamp: ts,
        processes: (0..count)
            .map(|i| ProcessSnapshotEntry {
                pid: i as u32,
                name: format!("proc-{i}"),
                cpu_usage: 1.0 + (i as f32),
                memory_bytes: (100 + i as u64) * 1_048_576,
            })
            .collect(),
    }
}

fn sample_session(id: &str, events: u64, frames: u64) -> SessionStats {
    SessionStats {
        session_id: id.to_string(),
        started_at: Utc::now(),
        ended_at: None,
        total_events: events,
        total_frames: frames,
        total_idle_secs: 0,
    }
}

// ── aggregate_hourly_metrics ──────────────────────────────────

#[tokio::test]
async fn aggregate_hourly_metrics_rolls_up_samples_in_hour() {
    let storage = open_storage();
    let hour_start = current_hour_start();
    let hour_key = hour_start.format("%Y-%m-%dT%H:00:00Z").to_string();

    // Seed 3 samples in the current hour with known CPU values.
    for (offset_min, cpu) in [(5, 20.0_f32), (20, 60.0_f32), (50, 40.0_f32)] {
        let ts = hour_start + Duration::minutes(offset_min);
        storage
            .save_metrics(&sample_metrics(ts, cpu))
            .await
            .unwrap();
    }

    storage.aggregate_hourly_metrics(hour_start).await.unwrap();

    let rows = storage.list_hourly_metrics_since(&hour_key).unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.hour, hour_key);
    assert!(
        (r.cpu_avg - 40.0_f64).abs() < 0.1,
        "cpu_avg should be ~40.0, got {}",
        r.cpu_avg
    );
    assert!(
        (r.cpu_max - 60.0_f64).abs() < 0.1,
        "cpu_max should be 60.0, got {}",
        r.cpu_max
    );
    assert_eq!(r.sample_count, 3);
}

#[tokio::test]
async fn aggregate_hourly_metrics_empty_hour_writes_no_row() {
    let storage = open_storage();
    // Pick 6 hours in the future so no sample could possibly be there.
    let hour_start = current_hour_start() + Duration::hours(6);
    let hour_key = hour_start.format("%Y-%m-%dT%H:00:00Z").to_string();

    storage.aggregate_hourly_metrics(hour_start).await.unwrap();

    let rows = storage.list_hourly_metrics_since(&hour_key).unwrap();
    assert!(
        rows.is_empty(),
        "empty hour must not produce an aggregate row"
    );
}

#[tokio::test]
async fn aggregate_hourly_metrics_utc_midnight_boundary() {
    let storage = open_storage();
    // Anchor to "today's UTC midnight" regardless of when the test runs.
    let day_midnight: DateTime<Utc> = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    // 23:00 bucket of yesterday and 00:00 bucket of today (straddles midnight).
    let day1_hour = day_midnight - Duration::hours(1);
    let day2_hour = day_midnight;
    let day1_key = day1_hour.format("%Y-%m-%dT%H:00:00Z").to_string();

    storage
        .save_metrics(&sample_metrics(day1_hour + Duration::minutes(58), 10.0))
        .await
        .unwrap();
    storage
        .save_metrics(&sample_metrics(day2_hour + Duration::minutes(2), 90.0))
        .await
        .unwrap();

    storage.aggregate_hourly_metrics(day1_hour).await.unwrap();
    storage.aggregate_hourly_metrics(day2_hour).await.unwrap();

    let rows = storage.list_hourly_metrics_since(&day1_key).unwrap();
    assert_eq!(rows.len(), 2, "two distinct hour buckets expected");
    assert_eq!(rows[0].sample_count, 1);
    assert_eq!(rows[1].sample_count, 1);
    assert!(rows[1].hour.ends_with("T00:00:00Z"));
}

// ── process_snapshots ──────────────────────────────────────────
// save_process_snapshot + get_process_snapshots are covered by
// sqlite/tests.rs:225 `save_and_get_process_snapshot`. Only the
// cleanup-cutoff path is a residual gap (per Task 0 audit).

#[tokio::test]
async fn cleanup_old_process_snapshots_deletes_before_cutoff_only() {
    let storage = open_storage();
    let now = Utc::now();
    let old = now - Duration::days(45);
    let recent = now - Duration::minutes(5);

    storage
        .save_process_snapshot(&sample_process_snapshot(old, 1))
        .await
        .unwrap();
    storage
        .save_process_snapshot(&sample_process_snapshot(recent, 2))
        .await
        .unwrap();

    let cutoff = now - Duration::days(30);
    let deleted = storage.cleanup_old_process_snapshots(cutoff).await.unwrap();
    assert_eq!(deleted, 1, "1 row older than cutoff deleted");

    let remaining = storage
        .get_process_snapshots(now - Duration::days(100), now + Duration::minutes(1), 100)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].processes.len(), 2, "recent snapshot preserved");
}
