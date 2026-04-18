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
