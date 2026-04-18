# Phase 5-D8 Storage Test Backfill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add comprehensive inline tests to 6 untested `oneshim-storage` modules across 3 PRs in ≤ 16 working days, closing gap D8 from the 2026-04-16 feature analysis.

**Architecture:** Adapter-crate inline tests. Port trait impls + private helpers get function+branch coverage. Thin delegators get combined smoke tests. Concurrency handled as lock-contract regression (NOT race detection — `Arc<Mutex<Connection>>` already serializes).

**Tech Stack:** Rust 2021, `rusqlite` 0.38 (bundled), `tokio` 1 (test runtime), `tempfile::TempDir`, `chrono` 0.4. Test harness pattern: per-module-local `open_db() -> (TempDir, Arc<Mutex<Connection>>)`.

**Spec reference:** [`2026-04-18-phase5-d8-storage-test-backfill-spec.md`](2026-04-18-phase5-d8-storage-test-backfill-spec.md) at commit `48b14c3e`.

---

## File Structure

### PR1 — metrics.rs → directory module

**Created:**
- `crates/oneshim-storage/src/sqlite/metrics/mod.rs` (moved verbatim from `sqlite/metrics.rs` + `#[cfg(test)] mod tests;` at bottom)
- `crates/oneshim-storage/src/sqlite/metrics/tests.rs` (new — ~25 tests + local harness)

**Deleted:**
- `crates/oneshim-storage/src/sqlite/metrics.rs` (content migrated to `metrics/mod.rs`)

**Unchanged:**
- `crates/oneshim-storage/src/sqlite/mod.rs` — already has `mod metrics;`, import path stable after `git mv`.

### PR2 — tags.rs + device_identity.rs (inline)

**Modified:**
- `crates/oneshim-storage/src/sqlite/tags.rs` — append `#[cfg(test)] mod tests { ... }` block (~15 tests)
- `crates/oneshim-storage/src/sqlite/device_identity.rs` — append `#[cfg(test)] mod tests { ... }` block (~8 tests)

### PR3 — Delegator trio + optional underlying-impl gap tests

**Modified:**
- `crates/oneshim-storage/src/sqlite/coaching_storage_port_impl.rs` — append tests block (1 combined smoke)
- `crates/oneshim-storage/src/sqlite/session_context_store_impl.rs` — append tests block (1 combined smoke)
- `crates/oneshim-storage/src/sqlite/focus_storage_impl.rs` — append tests block (1 combined smoke)
- `crates/oneshim-storage/src/sqlite/tests.rs` — if audit reveals underlying-impl gaps, add a `// ── PR3 FocusStorage gap coverage ──` section with up to 10 tests (capped).

---

## PR1 — metrics.rs — 18 methods, ~27 tests, 7-day hard cap

### Task 0: Audit MetricsStorage contract coverage (Day 1 AM, ~1 hour)

**Files:**
- Read-only: `crates/oneshim-storage/src/sqlite/port_contract_tests.rs` lines 232–264
- Artifact: paste audit table into PR1 body draft (`.claude/phase5-d8-progress.md` as scratchpad)

- [ ] **Step 1: Read port_contract_tests.rs MetricsStorage section**

Run: `sed -n '232,264p' crates/oneshim-storage/src/sqlite/port_contract_tests.rs`

- [ ] **Step 2: Build the coverage table**

Produce exactly this table in the PR1 body draft:

```markdown
## Task 0: MetricsStorage port-contract coverage audit

| Method | Contract test | Status |
|---|---|---|
| `save_metrics` | `ms_save_and_get_metrics_roundtrip` | ✅ covered (happy path) |
| `get_metrics` | `ms_save_and_get_metrics_roundtrip` + `ms_get_metrics_empty_range_returns_empty` | ✅ covered (happy path + empty range) |
| `aggregate_hourly_metrics` | — | ❌ uncovered |
| `cleanup_old_metrics` | `ms_cleanup_old_metrics_returns_count` | ✅ covered (empty-cutoff return value) |
| `save_process_snapshot` | — | ❌ uncovered |
| `get_process_snapshots` | — | ❌ uncovered |
| `cleanup_old_process_snapshots` | — | ❌ uncovered |
| `start_idle_period` | — | ❌ uncovered |
| `end_idle_period` | — | ❌ uncovered |
| `get_ongoing_idle_period` | — | ❌ uncovered |
| `get_idle_periods` | — | ❌ uncovered |
| `cleanup_old_idle_periods` | — | ❌ uncovered |
| `upsert_session` | — | ❌ uncovered |
| `get_session` | — | ❌ uncovered |
| `end_session` | — | ❌ uncovered |
| `increment_session_counters` | — | ❌ uncovered |
| `list_session_stats` (sync helper) | — | ❌ uncovered |
| `list_hourly_metrics_since` (sync helper) | — | ❌ uncovered |

**PR1 scope (after audit):** 15 uncovered async methods + 2 uncovered sync helpers + edge-case additions for 3 contract-covered methods (NULL network, bulk 100+, UTC midnight).
```

- [ ] **Step 3: Commit audit artifact in phase5-d8-progress.md (not yet in the PR)**

```bash
# Append the audit table + a "Task 0 complete 2026-04-XX" line to
# .claude/phase5-d8-progress.md. Do NOT commit yet — the first actual
# commit is the Task 1 refactor. Progress tracker updates are local-only.
```

---

### Task 1: Promote metrics.rs to directory module (Day 1 PM, ~30 min)

**Files:**
- Move: `crates/oneshim-storage/src/sqlite/metrics.rs` → `crates/oneshim-storage/src/sqlite/metrics/mod.rs`
- Create: `crates/oneshim-storage/src/sqlite/metrics/tests.rs`
- No changes to `crates/oneshim-storage/src/sqlite/mod.rs`

- [ ] **Step 1: Perform the git mv**

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/features
mkdir -p crates/oneshim-storage/src/sqlite/metrics
git mv crates/oneshim-storage/src/sqlite/metrics.rs crates/oneshim-storage/src/sqlite/metrics/mod.rs
```

- [ ] **Step 2: Create the empty tests.rs**

```bash
cat > crates/oneshim-storage/src/sqlite/metrics/tests.rs <<'EOF'
//! Inline unit tests for `sqlite::metrics`.
//!
//! Test harness convention per the Phase 5-D8 spec
//! (`docs/reviews/2026-04-18-phase5-d8-storage-test-backfill-spec.md`):
//! each test module defines its own `open_db()` locally. Do NOT centralize
//! this helper into `test_utils.rs` — that is an explicit follow-up item.

// Populated by Tasks 2–10.
EOF
```

- [ ] **Step 3: Wire the mod tests declaration in metrics/mod.rs**

Append to the very bottom of `crates/oneshim-storage/src/sqlite/metrics/mod.rs` (after the final `}` of the `impl MetricsStorage for SqliteStorage` block):

```rust

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Verify zero behaviour delta**

Run:
```bash
cargo check -p oneshim-storage
cargo test -p oneshim-storage --no-run
```
Expected: both green, no errors, no new warnings.

- [ ] **Step 5: Run the existing workspace test suite**

Run: `cargo test -p oneshim-storage`
Expected: all existing tests still pass (regime_manager_state_store, vector_store_impl, port_contract_tests, migration, edge_intelligence, integration_state_store all green).

- [ ] **Step 6: Verify clippy + fmt**

Run:
```bash
cargo clippy -p oneshim-storage --all-targets
cargo fmt --check
```
Expected: no warnings, no diff.

- [ ] **Step 7: Commit refactor**

```bash
git add crates/oneshim-storage/src/sqlite/metrics/mod.rs \
        crates/oneshim-storage/src/sqlite/metrics/tests.rs \
        crates/oneshim-storage/src/sqlite/metrics.rs
# Note: the deleted metrics.rs is staged as "deleted" by git mv; the
# created metrics/{mod.rs, tests.rs} are staged as "new". Confirm with
# git status before commit.

git commit -m "refactor(storage): promote metrics.rs to directory module

Prepare for Phase 5-D8 test backfill. Pure file relocation — no
behaviour change. mod tests; declaration added but tests.rs is a
placeholder that Tasks 2-10 populate.

Directory module precedent: sqlite/vector_store_impl/, sqlite/vector_index_impl/.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Add test harness + sample builders (Day 1 PM, ~30 min)

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/metrics/tests.rs`

- [ ] **Step 1: Append imports and the open_db helper**

Replace the placeholder comment in `metrics/tests.rs` with:

```rust
//! Inline unit tests for `sqlite::metrics`.
//!
//! Test harness convention per the Phase 5-D8 spec
//! (`docs/reviews/2026-04-18-phase5-d8-storage-test-backfill-spec.md`):
//! each test module defines its own `open_db()` locally. Do NOT centralize
//! this helper into `test_utils.rs` — that is an explicit follow-up item.

#![cfg(test)]

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, TimeZone, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::activity::{
    ProcessSnapshot, ProcessSnapshotEntry, SessionStats,
};
use oneshim_core::models::system::{NetworkInfo, SystemMetrics};
use oneshim_core::ports::storage::MetricsStorage;
use rusqlite::Connection;
use tempfile::TempDir;

use crate::sqlite::SqliteStorage;

// ── Harness ─────────────────────────────────────────────────────

/// Opens a fresh on-disk SQLite DB with all migrations applied.
/// `TempDir` is returned so it outlives the test; drop order matters
/// (connection must drop before tempdir).
fn open_db() -> (TempDir, Arc<Mutex<Connection>>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = Connection::open(dir.path().join("t.db")).expect("open sqlite");
    crate::migration::run_migrations(&conn).expect("run_migrations");
    (dir, Arc::new(Mutex::new(conn)))
}

/// Opens an in-memory SqliteStorage with the standard 30-day retention.
/// Used by tests that need the full `SqliteStorage` API (port methods).
fn open_storage() -> SqliteStorage {
    SqliteStorage::open_in_memory(30).expect("in-memory storage")
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
                memory_mb: 100 + (i as u64),
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
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p oneshim-storage --tests`
Expected: green. Any missing types (e.g., `ProcessSnapshotEntry` fields) become compiler errors — fix by reading the actual struct in `oneshim-core/src/models/activity.rs` and adjusting. **Do not guess field names.**

- [ ] **Step 3: Commit harness**

```bash
git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): add test harness and sample builders

Local open_db() / open_storage() helpers + sample_metrics /
sample_process_snapshot / sample_session builders. No tests yet;
Tasks 3-10 add them.

Per Phase 5-D8 spec: copy-local harness, no test_utils.rs promotion.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Tests for `aggregate_hourly_metrics` (Day 2 AM, ~2 hours, 3 tests, 1 commit)

**Files:** `crates/oneshim-storage/src/sqlite/metrics/tests.rs`

Tests write raw `system_metrics` rows, call `aggregate_hourly_metrics`, then verify the resulting `system_metrics_hourly` row. Because `open_db()` returns a raw Connection but `aggregate_hourly_metrics` is a `SqliteStorage` method, these tests use `open_storage()` and direct SQL via `storage.connection_arc()` for seeding.

- [ ] **Step 1: Write the happy-path aggregation test**

Append to `metrics/tests.rs` under a new `// ── aggregate_hourly_metrics ──────────────────` section header:

```rust

// ── aggregate_hourly_metrics ──────────────────────────────────

#[tokio::test]
async fn aggregate_hourly_metrics_rolls_up_samples_in_hour() {
    let storage = open_storage();
    let hour_start = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();

    // Seed 3 samples in the 10:00 hour with known CPU values.
    for (offset_min, cpu) in [(5, 20.0_f32), (20, 60.0_f32), (50, 40.0_f32)] {
        let ts = hour_start + Duration::minutes(offset_min);
        storage.save_metrics(&sample_metrics(ts, cpu)).await.unwrap();
    }

    storage.aggregate_hourly_metrics(hour_start).await.unwrap();

    let rows = storage
        .list_hourly_metrics_since("2026-04-15T10:00:00Z")
        .unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.hour, "2026-04-15T10:00:00Z");
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
```

- [ ] **Step 2: Write the empty-hour test**

```rust
#[tokio::test]
async fn aggregate_hourly_metrics_empty_hour_writes_no_row() {
    let storage = open_storage();
    let hour_start = Utc.with_ymd_and_hms(2026, 4, 15, 22, 0, 0).unwrap();

    // No samples seeded for this hour.
    storage.aggregate_hourly_metrics(hour_start).await.unwrap();

    let rows = storage
        .list_hourly_metrics_since("2026-04-15T22:00:00Z")
        .unwrap();
    assert!(
        rows.is_empty(),
        "empty hour must not produce an aggregate row"
    );
}
```

- [ ] **Step 3: Write the UTC-midnight boundary test**

```rust
#[tokio::test]
async fn aggregate_hourly_metrics_utc_midnight_boundary() {
    let storage = open_storage();
    // 23:58 of day 1 and 00:02 of day 2 — straddles UTC midnight.
    let day1_hour = Utc.with_ymd_and_hms(2026, 4, 15, 23, 0, 0).unwrap();
    let day2_hour = Utc.with_ymd_and_hms(2026, 4, 16, 0, 0, 0).unwrap();

    storage
        .save_metrics(&sample_metrics(day1_hour + Duration::minutes(58), 10.0))
        .await
        .unwrap();
    storage
        .save_metrics(&sample_metrics(day2_hour + Duration::minutes(2), 90.0))
        .await
        .unwrap();

    // Aggregate each hour separately. Each should see exactly 1 sample.
    storage.aggregate_hourly_metrics(day1_hour).await.unwrap();
    storage.aggregate_hourly_metrics(day2_hour).await.unwrap();

    let rows = storage
        .list_hourly_metrics_since("2026-04-15T23:00:00Z")
        .unwrap();
    assert_eq!(rows.len(), 2, "two distinct hour buckets expected");
    assert_eq!(rows[0].hour, "2026-04-15T23:00:00Z");
    assert_eq!(rows[0].sample_count, 1);
    assert_eq!(rows[1].hour, "2026-04-16T00:00:00Z");
    assert_eq!(rows[1].sample_count, 1);
}
```

- [ ] **Step 4: Run the 3 new tests**

Run: `cargo test -p oneshim-storage metrics::tests::aggregate_hourly -- --nocapture`
Expected: 3 PASS.

If any test FAILS, apply Section 10.1 bug-discovery policy: if the divergence is ≤ 20 LOC fix in `metrics/mod.rs`, fix in this PR and note in PR body. Larger fix → separate bugfix PR, mark the failing test `#[ignore = "blocked by bugfix PR #NNN"]`.

- [ ] **Step 5: Run full storage suite**

Run: `cargo test -p oneshim-storage`
Expected: all green (existing + 3 new).

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): aggregate_hourly_metrics — happy + empty + midnight

3 tests covering the uncovered hourly aggregation port method:
- happy path: 3 samples roll up into avg/max/count correctly
- empty hour: no samples => no hourly row written (matches debug! log)
- UTC midnight boundary: two distinct hours around 00:00 remain separate

Closes: Task 3 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Tests for process_snapshots group (Day 2 PM, ~2 hours, 4 tests, 1 commit)

Group: `save_process_snapshot`, `get_process_snapshots`, `cleanup_old_process_snapshots`.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: Write roundtrip + multi-timestamp ordering test**

Append:

```rust

// ── process_snapshots ──────────────────────────────────────────

#[tokio::test]
async fn save_and_get_process_snapshots_roundtrip_ordered_desc() {
    let storage = open_storage();
    let now = Utc::now();

    // Save 3 snapshots at different timestamps.
    storage
        .save_process_snapshot(&sample_process_snapshot(now - Duration::minutes(10), 2))
        .await
        .unwrap();
    storage
        .save_process_snapshot(&sample_process_snapshot(now - Duration::minutes(5), 3))
        .await
        .unwrap();
    storage
        .save_process_snapshot(&sample_process_snapshot(now, 4))
        .await
        .unwrap();

    let from = now - Duration::minutes(15);
    let to = now + Duration::minutes(1);
    let results = storage.get_process_snapshots(from, to, 100).await.unwrap();

    assert_eq!(results.len(), 3);
    // Query orders by timestamp DESC.
    assert_eq!(results[0].processes.len(), 4, "most recent first");
    assert_eq!(results[1].processes.len(), 3);
    assert_eq!(results[2].processes.len(), 2);
}
```

- [ ] **Step 2: Write empty-range test**

```rust
#[tokio::test]
async fn get_process_snapshots_empty_range_returns_empty() {
    let storage = open_storage();

    storage
        .save_process_snapshot(&sample_process_snapshot(Utc::now(), 1))
        .await
        .unwrap();

    // Query a far-future range.
    let future = Utc::now() + Duration::days(365);
    let far_future = future + Duration::days(1);
    let results = storage.get_process_snapshots(future, far_future, 100).await.unwrap();
    assert!(results.is_empty());
}
```

- [ ] **Step 3: Write cleanup-cutoff test**

```rust
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

    // Cutoff: 30 days ago. The 45-day-old snapshot should be deleted;
    // the 5-minute-old should remain.
    let cutoff = now - Duration::days(30);
    let deleted = storage
        .cleanup_old_process_snapshots(cutoff)
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let all = storage
        .get_process_snapshots(now - Duration::days(100), now + Duration::minutes(1), 100)
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].processes.len(), 2, "recent snapshot preserved");
}
```

- [ ] **Step 4: Write limit-respected test**

```rust
#[tokio::test]
async fn get_process_snapshots_respects_limit() {
    let storage = open_storage();
    let now = Utc::now();

    for i in 0..5 {
        storage
            .save_process_snapshot(&sample_process_snapshot(
                now - Duration::minutes(i),
                1,
            ))
            .await
            .unwrap();
    }

    let results = storage
        .get_process_snapshots(
            now - Duration::minutes(10),
            now + Duration::minutes(1),
            2,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
}
```

- [ ] **Step 5: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests
# Expect: all metrics tests green (3 from Task 3 + 4 new = 7).

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): process_snapshots save/get/cleanup + ordering

4 tests covering the 3 uncovered process_snapshots port methods:
- roundtrip with timestamp-DESC ordering
- empty range returns empty
- cleanup cutoff: deletes before, preserves after
- limit parameter respected

Closes: Task 4 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Tests for idle_periods group (Day 3 AM, ~3 hours, 6 tests, 1 commit)

Group: `start_idle_period`, `end_idle_period`, `get_ongoing_idle_period`, `get_idle_periods`, `cleanup_old_idle_periods`.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: Write start + get_ongoing roundtrip test**

```rust

// ── idle_periods ───────────────────────────────────────────────

#[tokio::test]
async fn start_idle_period_returns_id_and_get_ongoing_finds_it() {
    let storage = open_storage();
    let start = Utc::now();

    let id = storage.start_idle_period(start).await.unwrap();
    assert!(id > 0);

    let ongoing = storage.get_ongoing_idle_period().await.unwrap();
    assert!(ongoing.is_some());
    let (got_id, period) = ongoing.unwrap();
    assert_eq!(got_id, id);
    assert!(period.end_time.is_none());
    assert!(period.duration_secs.is_none());
}
```

- [ ] **Step 2: Write end_idle_period test**

```rust
#[tokio::test]
async fn end_idle_period_clears_ongoing_and_sets_duration() {
    let storage = open_storage();
    let start = Utc::now();

    let id = storage.start_idle_period(start).await.unwrap();
    let end = start + Duration::seconds(120);
    storage.end_idle_period(id, end).await.unwrap();

    let ongoing = storage.get_ongoing_idle_period().await.unwrap();
    assert!(ongoing.is_none(), "no ongoing period after end");

    let periods = storage
        .get_idle_periods(start - Duration::seconds(1), end + Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(periods.len(), 1);
    assert_eq!(periods[0].duration_secs, Some(120));
    assert_eq!(periods[0].end_time, Some(end));
}
```

- [ ] **Step 3: Write get_ongoing returns none on fresh db test**

```rust
#[tokio::test]
async fn get_ongoing_idle_period_none_when_no_periods() {
    let storage = open_storage();
    let result = storage.get_ongoing_idle_period().await.unwrap();
    assert!(result.is_none());
}
```

- [ ] **Step 4: Write get_ongoing prefers most recent (by id DESC)**

```rust
#[tokio::test]
async fn get_ongoing_idle_period_returns_most_recent_when_multiple_open() {
    let storage = open_storage();

    let id1 = storage.start_idle_period(Utc::now()).await.unwrap();
    let id2 = storage
        .start_idle_period(Utc::now() + Duration::seconds(10))
        .await
        .unwrap();
    assert!(id2 > id1);

    let ongoing = storage.get_ongoing_idle_period().await.unwrap();
    let (got_id, _) = ongoing.expect("expected some");
    assert_eq!(got_id, id2, "most recent (highest id) wins");
}
```

- [ ] **Step 5: Write get_idle_periods time-range filtering test**

```rust
#[tokio::test]
async fn get_idle_periods_filters_by_time_range() {
    let storage = open_storage();
    let now = Utc::now();

    // Period 1: far in the past.
    let id1 = storage
        .start_idle_period(now - Duration::days(10))
        .await
        .unwrap();
    storage
        .end_idle_period(id1, now - Duration::days(10) + Duration::seconds(30))
        .await
        .unwrap();

    // Period 2: recent.
    let id2 = storage
        .start_idle_period(now - Duration::minutes(5))
        .await
        .unwrap();
    storage
        .end_idle_period(id2, now - Duration::minutes(4))
        .await
        .unwrap();

    // Query a range covering only the recent period.
    let results = storage
        .get_idle_periods(now - Duration::hours(1), now + Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].duration_secs, Some(60));
}
```

- [ ] **Step 6: Write cleanup excludes active (end_time IS NULL)**

```rust
#[tokio::test]
async fn cleanup_old_idle_periods_preserves_active_even_if_start_is_old() {
    let storage = open_storage();
    let now = Utc::now();

    // An active period started 100 days ago — cleanup must NOT delete it.
    let _active_id = storage
        .start_idle_period(now - Duration::days(100))
        .await
        .unwrap();

    // An ended period from 50 days ago — cleanup SHOULD delete.
    let ended_id = storage
        .start_idle_period(now - Duration::days(50))
        .await
        .unwrap();
    storage
        .end_idle_period(ended_id, now - Duration::days(50) + Duration::seconds(60))
        .await
        .unwrap();

    let cutoff = now - Duration::days(30);
    let deleted = storage.cleanup_old_idle_periods(cutoff).await.unwrap();
    assert_eq!(deleted, 1, "only the ended old period deleted");

    let ongoing = storage.get_ongoing_idle_period().await.unwrap();
    assert!(ongoing.is_some(), "active period survived cleanup");
}
```

- [ ] **Step 7: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests
# Expect 13 tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): idle_periods — start/end/get/cleanup coverage

6 tests covering the 5 uncovered idle_periods port methods plus the
'active period preserved on cleanup' invariant derived from the
'WHERE end_time IS NOT NULL' clause in cleanup_old_idle_periods.

Closes: Task 5 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Tests for sessions group (Day 3 PM, ~3 hours, 6 tests, 1 commit)

Group: `upsert_session`, `get_session`, `end_session`, `increment_session_counters`.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: Write upsert-then-get roundtrip**

```rust

// ── sessions ───────────────────────────────────────────────────

#[tokio::test]
async fn upsert_session_and_get_roundtrip() {
    let storage = open_storage();
    let stats = sample_session("sess-001", 10, 20);
    storage.upsert_session(&stats).await.unwrap();

    let got = storage.get_session("sess-001").await.unwrap();
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.session_id, "sess-001");
    assert_eq!(got.total_events, 10);
    assert_eq!(got.total_frames, 20);
    assert_eq!(got.total_idle_secs, 0);
    assert!(got.ended_at.is_none());
}
```

- [ ] **Step 2: Write upsert ON CONFLICT updates existing row**

```rust
#[tokio::test]
async fn upsert_session_on_conflict_updates_counters() {
    let storage = open_storage();

    storage
        .upsert_session(&sample_session("sess-conflict", 5, 5))
        .await
        .unwrap();

    // Upsert again with new counters + ended_at.
    let mut updated = sample_session("sess-conflict", 50, 100);
    updated.ended_at = Some(Utc::now());
    storage.upsert_session(&updated).await.unwrap();

    let got = storage.get_session("sess-conflict").await.unwrap().unwrap();
    assert_eq!(got.total_events, 50);
    assert_eq!(got.total_frames, 100);
    assert!(got.ended_at.is_some());
}
```

- [ ] **Step 3: Write get nonexistent returns None**

```rust
#[tokio::test]
async fn get_session_nonexistent_returns_none() {
    let storage = open_storage();
    let got = storage.get_session("does-not-exist").await.unwrap();
    assert!(got.is_none());
}
```

- [ ] **Step 4: Write end_session sets ended_at**

```rust
#[tokio::test]
async fn end_session_sets_ended_at_on_existing_row() {
    let storage = open_storage();
    storage
        .upsert_session(&sample_session("sess-end", 1, 1))
        .await
        .unwrap();

    let end_ts = Utc::now();
    storage.end_session("sess-end", end_ts).await.unwrap();

    let got = storage.get_session("sess-end").await.unwrap().unwrap();
    assert!(got.ended_at.is_some());
    // Allow minor RFC3339 rounding — compare to within 1 second.
    let diff = (got.ended_at.unwrap() - end_ts).num_milliseconds().abs();
    assert!(diff < 1000, "ended_at mismatch: {diff}ms");
}
```

- [ ] **Step 5: Write increment_session_counters accumulates**

```rust
#[tokio::test]
async fn increment_session_counters_accumulates() {
    let storage = open_storage();
    storage
        .upsert_session(&sample_session("sess-inc", 10, 5))
        .await
        .unwrap();

    storage
        .increment_session_counters("sess-inc", 3, 2, 30)
        .await
        .unwrap();
    storage
        .increment_session_counters("sess-inc", 7, 0, 15)
        .await
        .unwrap();

    let got = storage.get_session("sess-inc").await.unwrap().unwrap();
    assert_eq!(got.total_events, 10 + 3 + 7);
    assert_eq!(got.total_frames, 5 + 2);
    assert_eq!(got.total_idle_secs, 30 + 15);
}
```

- [ ] **Step 6: Write increment on nonexistent session is a no-op**

```rust
#[tokio::test]
async fn increment_session_counters_on_nonexistent_is_noop() {
    let storage = open_storage();
    // No upsert first — just call increment on a session_id that doesn't exist.
    let result = storage
        .increment_session_counters("does-not-exist", 1, 1, 1)
        .await;
    // SQLite UPDATE on 0 rows is not an error.
    assert!(result.is_ok());

    let got = storage.get_session("does-not-exist").await.unwrap();
    assert!(got.is_none(), "increment must not create the row");
}
```

- [ ] **Step 7: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests
# Expect 19 tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): sessions — upsert/get/end/increment coverage

6 tests covering the 4 uncovered sessions port methods:
- upsert roundtrip
- upsert ON CONFLICT updates counters + ended_at
- get returns None for missing session
- end_session sets ended_at
- increment accumulates across calls
- increment on nonexistent is no-op (SQLite UPDATE behaviour)

Closes: Task 6 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Tests for sync list helpers (Day 4 AM, ~1 hour, 3 tests, 1 commit)

Group: `list_session_stats` (sync), `list_hourly_metrics_since` (sync — already exercised by Task 3 but add one more).

**Files:** `metrics/tests.rs`

- [ ] **Step 1: Write list_session_stats ordering + limit**

```rust

// ── sync list helpers ──────────────────────────────────────────

#[tokio::test]
async fn list_session_stats_orders_by_started_at_desc_and_respects_limit() {
    let storage = open_storage();

    // Upsert 3 sessions with different started_at timestamps.
    for (i, started_delta_sec) in [100, 200, 300].iter().enumerate() {
        let mut s = sample_session(&format!("sess-{i}"), 0, 0);
        s.started_at = Utc::now() - Duration::seconds(*started_delta_sec);
        storage.upsert_session(&s).await.unwrap();
    }

    // limit = 2 returns the 2 most recent (i.e., smallest delta — sess-0 and sess-1).
    let results = storage.list_session_stats(2).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].session_id, "sess-0");
    assert_eq!(results[1].session_id, "sess-1");
}
```

- [ ] **Step 2: Write list_session_stats empty**

```rust
#[tokio::test]
async fn list_session_stats_empty_db_returns_empty_vec() {
    let storage = open_storage();
    let results = storage.list_session_stats(10).unwrap();
    assert!(results.is_empty());
}
```

- [ ] **Step 3: Write list_hourly_metrics_since filter**

```rust
#[tokio::test]
async fn list_hourly_metrics_since_filters_by_from_hour() {
    let storage = open_storage();

    // Seed 3 hours of raw samples, then aggregate each.
    for h in [9, 10, 11] {
        let hour_start = Utc.with_ymd_and_hms(2026, 4, 15, h, 0, 0).unwrap();
        storage
            .save_metrics(&sample_metrics(hour_start + Duration::minutes(30), 25.0))
            .await
            .unwrap();
        storage.aggregate_hourly_metrics(hour_start).await.unwrap();
    }

    // Filter: from 10:00 onward — should see 2 rows (10 and 11).
    let results = storage
        .list_hourly_metrics_since("2026-04-15T10:00:00Z")
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].hour, "2026-04-15T10:00:00Z");
    assert_eq!(results[1].hour, "2026-04-15T11:00:00Z");
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests
# Expect 22 tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): sync list helpers — session_stats + hourly since

3 tests covering the 2 uncovered sync helpers:
- list_session_stats: DESC ordering by started_at, LIMIT respected
- list_session_stats: empty DB returns empty vec
- list_hourly_metrics_since: from_hour filter correctness after
  aggregation of multiple hours

Closes: Task 7 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Lock-contract concurrency tests (Day 4 PM, ~2 hours, 2 tests, 1 commit)

Per spec Section 4 PR1 + Section 6 #3: these are NOT race tests. They assert the Mutex contract survives across threads.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: Parallel save_metrics preserves total count**

```rust

// ── lock-contract regression ───────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_save_metrics_preserves_count() {
    let storage = Arc::new(open_storage());

    // Spawn 4 concurrent writer tasks, each inserting 5 samples.
    let mut handles = Vec::new();
    for task_id in 0..4 {
        let s = storage.clone();
        let base = Utc::now() + Duration::seconds(task_id as i64 * 10);
        handles.push(tokio::spawn(async move {
            for i in 0..5 {
                let ts = base + Duration::milliseconds(i as i64);
                s.save_metrics(&sample_metrics(ts, 10.0 + task_id as f32))
                    .await
                    .unwrap();
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Total = 4 × 5 = 20 rows.
    let results = storage
        .get_metrics(
            Utc::now() - Duration::minutes(1),
            Utc::now() + Duration::minutes(10),
            1_000,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 20);
}
```

- [ ] **Step 2: Writer + reader yield consistent snapshot**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_writer_plus_reader_yield_consistent_snapshot() {
    let storage = Arc::new(open_storage());
    let ts_start = Utc::now();

    let writer = {
        let s = storage.clone();
        tokio::spawn(async move {
            for i in 0..50 {
                s.save_metrics(&sample_metrics(
                    ts_start + Duration::milliseconds(i),
                    20.0,
                ))
                .await
                .unwrap();
            }
        })
    };

    let reader = {
        let s = storage.clone();
        tokio::spawn(async move {
            // Read repeatedly while writer runs.
            for _ in 0..10 {
                let results = s
                    .get_metrics(
                        ts_start - Duration::seconds(1),
                        ts_start + Duration::seconds(10),
                        1_000,
                    )
                    .await
                    .unwrap();
                // Invariant: count is monotonic (Arc<Mutex> serializes);
                // any snapshot must be 0..=50.
                assert!(results.len() <= 50);
            }
        })
    };

    writer.await.unwrap();
    reader.await.unwrap();

    // Final count is exactly 50.
    let final_count = storage
        .get_metrics(
            ts_start - Duration::seconds(1),
            ts_start + Duration::seconds(10),
            1_000,
        )
        .await
        .unwrap()
        .len();
    assert_eq!(final_count, 50);
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests::concurrent
# Expect: 2 PASS.

cargo test -p oneshim-storage metrics::tests
# Expect: 24 tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): lock-contract regression tests (2)

NOT race tests — Arc<Mutex<Connection>> already serializes. These
tests assert the Mutex contract survives across multiple async tasks
on a multi-thread tokio runtime (2 of 24 tests use
flavor = multi_thread).

- concurrent_save_metrics_preserves_count: 4 tasks × 5 samples = 20
- concurrent_writer_plus_reader_yield_consistent_snapshot: reader
  sees 0..=50 rows monotonically while writer runs

Closes: Task 8 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Err-branch tests (Day 5, ~4 hours, 3 tests, 1 commit)

Per spec Section 7: try CHECK/UNIQUE violations first; mutex-poisoning is OPTIONAL; unreachable paths get `// TODO` comments.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: Direct-insert invalid process_snapshots JSON triggers filter_map skip**

`save_process_snapshot` serializes via `serde_json::to_string` (success path only — serialization of a `Vec<ProcessSnapshotEntry>` cannot fail from a well-formed struct). BUT `get_process_snapshots` uses `serde_json::from_str(&data).unwrap_or_default()` — a silently-swallowed deserialization failure. This test verifies the "unreachable Err" claim: invalid JSON becomes empty `processes` in the returned snapshot but does NOT return `Err`.

```rust

// ── Err branches + unreachable-Err documentation ───────────────

#[tokio::test]
async fn get_process_snapshots_invalid_json_in_column_silently_defaults_to_empty() {
    let storage = open_storage();
    let arc = storage.connection_arc();

    // Direct-insert a row with malformed JSON in snapshot_data.
    {
        let conn = arc.lock().unwrap();
        conn.execute(
            "INSERT INTO process_snapshots (timestamp, snapshot_data) VALUES (?1, ?2)",
            rusqlite::params![Utc::now().to_rfc3339(), "{not:valid json"],
        )
        .unwrap();
    }

    // get_process_snapshots uses `serde_json::from_str(&data).unwrap_or_default()`
    // so the bad row is returned with empty processes — not an Err.
    let results = storage
        .get_process_snapshots(
            Utc::now() - Duration::minutes(1),
            Utc::now() + Duration::minutes(1),
            10,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].processes.is_empty(),
        "malformed JSON silently becomes empty processes — this is by design per the \
         unwrap_or_default() call in the module"
    );
}
```

- [ ] **Step 2: save_metrics on a closed DB returns Err(CoreError::Internal(_))**

This is the closest we get to exercising the `map_err(|e| StorageError::Internal(...))` branch without mutex poisoning. The `with_conn` path errors out cleanly.

Actually — this is hard to exercise because `SqliteStorage` holds the Connection for its lifetime. Per spec, document as unreachable.

```rust
// TODO: untested Err — save_metrics StorageError::Internal on SQLite
// failure. The Connection is held inside SqliteStorage for its lifetime;
// there is no path to force a closed-connection error without unsafe code.
// Equivalent coverage would require injecting a mock Connection, which
// is out of scope for D8 (that's port-contract-test territory).
//
// If this ever needs coverage, the approach is: extract a `with_conn`
// trait with a failure-injecting test double. Captured in Section 11
// follow-ups of the spec.
```

- [ ] **Step 3: Mutex-poisoning attempt on save_metrics (OPTIONAL, budget 1-2h)**

Try once. If it works, ship it. If it hangs or doesn't trigger, downgrade to unreachable per spec Section 7.

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "mutex poisoning via spawn_blocking + panic is spec-OPTIONAL; \
            un-ignore if the technique is proven to work against with_conn"]
async fn save_metrics_mutex_poison_returns_err_variant_only() {
    let storage = open_storage();
    let conn_arc = storage.connection_arc();

    // Poison the lock from a blocking task.
    let c = conn_arc.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let _guard = c.lock().unwrap();
        panic!("intentional poison");
    })
    .await;

    // The subsequent save_metrics should propagate as CoreError::Internal.
    // Variant-only match — error string varies per call site (see spec Section 7).
    let result = storage.save_metrics(&sample_metrics(Utc::now(), 10.0)).await;
    assert!(
        matches!(result, Err(CoreError::Internal(_))),
        "expected Err(CoreError::Internal(_)), got {result:?}"
    );
}
```

- [ ] **Step 4: Direct-insert invalid RFC3339 timestamp into system_metrics — parse fallback to Utc::now() in get_metrics**

Another "not-actually-Err" path — `DateTime::parse_from_rfc3339(&ts_str).map(...).unwrap_or_else(|_| Utc::now())` in `get_metrics`. Document as non-Err swallow.

```rust
#[tokio::test]
async fn get_metrics_invalid_rfc3339_timestamp_silently_substitutes_now() {
    let storage = open_storage();
    let arc = storage.connection_arc();

    // Direct-insert a row with a malformed timestamp.
    {
        let conn = arc.lock().unwrap();
        conn.execute(
            "INSERT INTO system_metrics
             (timestamp, cpu_usage, memory_used, memory_total,
              disk_used, disk_total, network_upload, network_download)
             VALUES ('not-a-date', 50.0, 1, 2, 3, 4, 0, 0)",
            [],
        )
        .unwrap();
    }

    // get_metrics's DESC timestamp filter: query a wide range.
    let results = storage
        .get_metrics(
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(1),
            10,
        )
        .await
        .unwrap();
    // The bad timestamp was substituted for Utc::now() at read time, so the row
    // may or may not be within the range depending on filter SQL (which uses the
    // raw string, not the parsed timestamp). The assertion here is that we do
    // NOT return Err even though the row is corrupt.
    // Whether the row appears in results depends on whether 'not-a-date' >= from_str
    // and <= to_str in string comparison terms. Documenting; no hard assertion.
    let _ = results;
}
```

Actually — this test is unreliable because the SQL filter uses string comparison. Let me drop it from the plan and rely on Step 1's process_snapshots JSON test + Step 3's poisoning attempt. **The plan-phase reviewer should flag this — if so, acknowledge and remove Step 4.**

- [ ] **Step 4 (revised): Skip — document the decision**

Do NOT implement the invalid-timestamp test. Add to PR body:

> `get_metrics`'s `unwrap_or_else(|_| Utc::now())` fallback is unreachable from a test (the SQL filter operates on the raw timestamp column, so a malformed row with 'not-a-date' may or may not appear depending on string-ordering which is non-deterministic). Documented as:

```rust
// TODO: untested Err — get_metrics RFC3339 parse fallback.
// The fallback branch (`unwrap_or_else(|_| Utc::now())`) IS reachable
// in production if a row has a malformed timestamp column, but cannot
// be deterministically exercised from a test because SQL string-ordering
// on 'not-a-date' vs RFC3339 range bounds is undefined for our purposes.
```

- [ ] **Step 5: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests
# Expect: 26 tests green (24 active + 1 #[ignore] + 1 skipped-documented).
# Mutex poisoning test: if it passes, un-ignore in a follow-up commit;
# if it hangs in CI, leave ignored.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): Err-branch coverage + unreachable-Err documentation

- get_process_snapshots silently-swallowed JSON parse (by design)
- Mutex poisoning attempt (#[ignore], OPTIONAL per spec Section 7)
- Inline TODOs for 2 unreachable Err paths (closed connection,
  get_metrics RFC3339 parse fallback) per Section 6 #2 escape hatch

Closes: Task 9 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: Contract-covered edge cases (Day 6 AM, ~2 hours, 3 tests, 1 commit)

Three contract-covered methods (save_metrics, get_metrics, cleanup_old_metrics) already have happy-path tests — but edge cases are uncovered.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: save_metrics with NULL network field**

```rust

// ── contract-covered edge cases ────────────────────────────────

#[tokio::test]
async fn save_metrics_null_network_roundtrips_as_zeros() {
    let storage = open_storage();
    let ts = Utc::now();
    storage
        .save_metrics(&sample_metrics_no_network(ts, 15.0))
        .await
        .unwrap();

    let results = storage
        .get_metrics(ts - Duration::seconds(1), ts + Duration::seconds(1), 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    // save_metrics writes (0, 0) for null network; get_metrics reconstructs
    // a NetworkInfo { upload=0, download=0, is_connected=false }.
    let got = &results[0];
    assert!(got.network.is_some());
    let net = got.network.as_ref().unwrap();
    assert_eq!(net.upload_speed, 0);
    assert_eq!(net.download_speed, 0);
    assert!(!net.is_connected);
}
```

- [ ] **Step 2: Bulk write + bulk read preserves all rows**

```rust
#[tokio::test]
async fn save_and_get_metrics_bulk_100_samples_round_trips_all() {
    let storage = open_storage();
    let base = Utc::now();

    for i in 0..100 {
        storage
            .save_metrics(&sample_metrics(base + Duration::milliseconds(i), i as f32))
            .await
            .unwrap();
    }

    let results = storage
        .get_metrics(
            base - Duration::seconds(1),
            base + Duration::seconds(10),
            200,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 100);
}
```

- [ ] **Step 3: cleanup with cutoff deletes exactly the expected subset**

```rust
#[tokio::test]
async fn cleanup_old_metrics_deletes_before_cutoff_preserves_after() {
    let storage = open_storage();
    let now = Utc::now();

    // 3 old, 2 recent.
    for i in 0..3 {
        storage
            .save_metrics(&sample_metrics(
                now - Duration::days(45) + Duration::seconds(i),
                10.0,
            ))
            .await
            .unwrap();
    }
    for i in 0..2 {
        storage
            .save_metrics(&sample_metrics(
                now - Duration::minutes(5) + Duration::seconds(i),
                20.0,
            ))
            .await
            .unwrap();
    }

    let cutoff = now - Duration::days(30);
    let deleted = storage.cleanup_old_metrics(cutoff).await.unwrap();
    assert_eq!(deleted, 3, "3 old rows deleted");

    let remaining = storage
        .get_metrics(now - Duration::days(100), now + Duration::days(1), 100)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 2);
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests
# Expect: 29 tests green (27 active + 1 ignored + 1 skipped-documented).

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): contract-covered edge cases (3 tests)

Edge cases for the 3 MetricsStorage methods already contract-covered
(save/get/cleanup) but whose happy-path-only contract tests miss:
- NULL network field round-trips as (0, 0, is_connected=false)
- Bulk 100-sample write + read preserves all rows
- Cleanup cutoff: deletes strictly-older rows, preserves strictly-newer

Closes: Task 10 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Final verify + push + open PR (Day 6 PM, ~2 hours)

**Files:** no new edits

- [ ] **Step 1: Full workspace test suite**

Run: `cargo test --workspace`
Expected: all green (including 27 new metrics::tests, 1 #[ignore], other crates unaffected).

- [ ] **Step 2: Full workspace clippy**

Run: `cargo clippy --workspace --all-targets`
Expected: zero warnings.

- [ ] **Step 3: Fmt check**

Run: `cargo fmt --check`
Expected: no diff.

- [ ] **Step 4: Verify per-function coverage against Task 0 audit**

Walk through the Task 0 table. For each method marked ❌ uncovered, find the test name. Missing any?

Expected mapping:
- aggregate_hourly_metrics → Task 3 (3 tests)
- save/get/cleanup_process_snapshot → Task 4 (4 tests)
- 5 idle_period methods → Task 5 (6 tests)
- 4 session methods → Task 6 (6 tests)
- 2 sync helpers → Task 7 (3 tests)
- Concurrency → Task 8 (2 tests)
- Err + unreachable → Task 9 (1 active + 1 #[ignore] + 2 documented skips)
- Contract-covered edge → Task 10 (3 tests)

Total: 27 active tests + 1 #[ignore] = 28 test items.

- [ ] **Step 5: Push branch**

```bash
git push -u origin feat/phase5-d8-storage-tests
```

- [ ] **Step 6: Open PR1**

```bash
gh pr create \
  --base main \
  --head feat/phase5-d8-storage-tests \
  --title "test(storage): Phase 5-D8 PR1 — metrics.rs coverage (27 tests)" \
  --body "$(cat <<'EOF'
## Summary

Phase 5-D8 PR1 — closes the MetricsStorage test-coverage gap per
[the D8 analysis](docs/reviews/2026-04-16-feature-gaps-analysis.md)
and [the Phase 5-D8 spec](docs/reviews/2026-04-18-phase5-d8-storage-test-backfill-spec.md).

## Changes

- **Refactor:** `sqlite/metrics.rs` promoted to directory module
  (`sqlite/metrics/{mod.rs, tests.rs}`) per ADR-003. Pure file
  relocation — zero behaviour delta.
- **Tests:** 27 new tests + 1 `#[ignore]` (optional mutex-poison).
  Coverage breakdown:
  - `aggregate_hourly_metrics`: 3 (happy / empty / midnight)
  - `process_snapshots` group: 4
  - `idle_periods` group: 6
  - `sessions` group: 6
  - sync helpers: 3
  - lock-contract regression (multi-thread tokio): 2
  - Err branches + documented unreachable: 1 + 2 TODOs
  - Contract-covered edge cases: 3

## Task 0 audit (MetricsStorage port-contract coverage)

| Method | Before PR1 | After PR1 |
|---|---|---|
| save_metrics | contract (happy) | + NULL network, bulk 100 |
| get_metrics | contract (happy + empty) | + bulk, NULL network |
| aggregate_hourly_metrics | ❌ | ✅ (3 tests) |
| cleanup_old_metrics | contract (empty cutoff) | + non-empty cutoff with boundary |
| save_process_snapshot | ❌ | ✅ |
| get_process_snapshots | ❌ | ✅ (roundtrip + empty range + JSON silent fallback) |
| cleanup_old_process_snapshots | ❌ | ✅ |
| start/end/get_ongoing/get/cleanup_idle_periods | ❌ (all 5) | ✅ (6 tests, inc. "cleanup preserves active") |
| upsert/get/end/increment sessions | ❌ (all 4) | ✅ (6 tests, inc. increment-nonexistent no-op) |
| list_session_stats | ❌ | ✅ (DESC order + LIMIT + empty) |
| list_hourly_metrics_since | ❌ | ✅ (from_hour filter) |

## Unreachable Err branches (documented)

- `save_metrics` closed-connection: Connection held inside SqliteStorage
  for lifetime; would require Connection-trait injection.
- `get_metrics` RFC3339 parse fallback: string-ordering in SQL range
  filter is non-deterministic on malformed timestamps.

## Verification

- `cargo test --workspace`: green (+27 tests).
- `cargo clippy --workspace --all-targets`: zero warnings.
- `cargo fmt --check`: clean.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 7: Record the PR number in the progress tracker**

Append to `.claude/phase5-d8-progress.md` the PR URL and number. **Do not commit the progress tracker to this PR** — it's `.claude/`-gitignored.

---

### PR1 Review checkpoint — Loop 3 iter N (Day 7, review turnaround)

Per spec Section 9 and the Ralph Loop mandate for Loop 3:

- Dispatch `superpowers:code-reviewer` agent with PR1 diff + this plan + the spec.
- Apply any Critical / Important findings.
- If 0 Critical + 0 Important, proceed to squash merge.
- If findings exist, return to the relevant Task (3–10) and fix; re-run Step 1–4 of Task 11; push; re-review.

**PR1 merge gate:** 0 Critical + 0 Important on the final deep-review pass.

---

## PR2 — tags.rs + device_identity.rs — ~23 tests, 5-day hard cap

### Task 12: Add tests block to tags.rs (Days 8–9, ~2 days, ~15 tests)

**Files:** `crates/oneshim-storage/src/sqlite/tags.rs` (append tests block).

- [ ] **Step 1: Read tags.rs to confirm public API surface**

Run: `grep -n "pub fn\|pub async fn" crates/oneshim-storage/src/sqlite/tags.rs`
Expected: ~10 methods (create_tag, list_tags, update_tag, delete_tag, get_tag_by_name, assign_tag, unassign_tag, list_tags_for_frame, list_frames_for_tag, clear_frame_tags or similar — confirm).

- [ ] **Step 2: Write the 15 tests using the same structure as PR1 metrics/tests.rs**

Same patterns:
- local `open_db()` / `open_storage()` helpers
- CRUD happy path for each public method (4–6 tests)
- UNIQUE(name) constraint violation on duplicate create (1 test, sync threads)
- Empty-name rejection if the impl has one (1 test, verify during impl)
- Tag-frame linkage: assign, then list_tags_for_frame sees the tag (1 test)
- Tag-frame linkage: delete tag, assert `frame_tags` cascade or manual cleanup (1 test)
- `update_tag` changes color + name, preserved across list (1–2 tests)
- Concurrency lock-contract: 4 `std::thread::spawn` creating same name — 1 Ok, 3 Err (1 test)
- Edge cases per impl reading (empty list, case sensitivity, etc.) (1–3 tests)

For each test: write → run → fix if bug discovered per Section 10.1 → commit in groups of 3–5.

- [ ] **Step 3: Verify + commit batches**

```bash
cargo test -p oneshim-storage tags::tests
cargo clippy --workspace --all-targets
git commit -m "test(tags): CRUD + unique constraint + linkage coverage (batch 1)"
# ...repeat for remaining batches.
```

### Task 13: Add tests block to device_identity.rs (Day 10, ~1 day, ~8 tests)

**Files:** `crates/oneshim-storage/src/sqlite/device_identity.rs`.

- [ ] **Step 1: Read device_identity.rs to identify methods + schema**

Run: `cat crates/oneshim-storage/src/sqlite/device_identity.rs`
Expected: ~2 public methods + a singleton-row table schema.

- [ ] **Step 2: Write 8 tests following the structure of metrics/tests.rs**

Scenarios per spec Section 4 PR2:
- First create (no row): writes row, returns identity (1 test)
- Second call: same identity (idempotency, 1 test)
- reset: new identity differs from prior (1 test)
- reset + read: new identity persists (1 test)
- Idempotent reread after reset: stable (1 test)
- Singleton constraint: direct-insert second row violates CHECK id=1 (1 test)
- Corruption: direct-insert malformed identity bytes; recovery behaviour (1 test, verify during impl)
- Cross-session persistence: open-close-open same db path → same identity (1 test)

- [ ] **Step 3: Verify + commit**

```bash
cargo test -p oneshim-storage device_identity::tests
git commit -m "test(device_identity): singleton + reset + corruption coverage"
```

### Task 14: PR2 final verify + open (Day 11)

Same structure as PR1 Task 11. Branch: keep on `feat/phase5-d8-storage-tests` OR open new branch off main after PR1 merges — decide based on PR1's merge timing.

**PR2 merge gate:** 0 Critical + 0 Important on deep review.

---

## PR3 — Delegator trio + port audit — 3–13 tests, 4-day hard cap

### Task 15: Audit port_contract_tests.rs (Day 12 AM, ~2 hours)

**Files:** read-only `crates/oneshim-storage/src/sqlite/port_contract_tests.rs` + each delegator impl file.

- [ ] **Step 1: Produce the full audit table**

Format (paste into PR3 body draft):

```markdown
## Task 15 — Port-coverage audit

### CoachingStoragePort (2 methods)

| Method | Contract test | Status |
|---|---|---|
| save_coaching_event | ? | ? |
| list_coaching_events | ? | ? |

### SessionContextStorePort (1 method)

| Method | Contract test | Status |
|---|---|---|
| save_session_context | ? | ? |

### FocusStorage (12 methods) — 3 known contract-covered

| Method | Contract test | Status |
|---|---|---|
| start_work_session | fs_start_and_end_work_session_roundtrip | ✅ |
| end_work_session | fs_start_and_end_work_session_roundtrip | ✅ |
| get_or_create_focus_metrics | fs_get_or_create_focus_metrics_fresh_db | ✅ |
| increment_focus_metrics | fs_increment_focus_metrics_accumulates | ✅ |
| add_deep_work_secs | ? | ? |
| record_interruption | ? | ? |
| increment_work_session_interruption | ? | ? |
| record_interruption_resume | ? | ? |
| update_focus_metrics | ? | ? |
| save_rule_suggestion | ? | ? |
| mark_suggestion_shown_by_id | ? | ? |
| get_pending_interruption | ? | ? |
```

- [ ] **Step 2: Fill the `?` cells**

Read `port_contract_tests.rs` top to bottom; for each delegator-port method, mark `✅ covered` / `❌ uncovered` / `⚠ partial (covers happy path only)`.

- [ ] **Step 3: Decide PR3 scope from the audit**

Rules:
- 3 delegator smoke tests regardless of audit (always).
- For each ❌ in the underlying `_sync` impls, add 1 happy-path test in `sqlite/tests.rs` — **cap at 10 total underlying-impl gap tests for PR3**.
- Overflow > 10 → defer to a follow-up issue + mention in spec Section 11 update.

### Task 16: Write 3 delegator smoke tests (Day 12 PM, ~3 hours)

**Files:**
- `crates/oneshim-storage/src/sqlite/coaching_storage_port_impl.rs` (append tests)
- `crates/oneshim-storage/src/sqlite/session_context_store_impl.rs` (append tests)
- `crates/oneshim-storage/src/sqlite/focus_storage_impl.rs` (append tests)

- [ ] **Step 1: coaching_storage_port_impl smoke test**

Pattern: one `#[tokio::test]` that invokes every `pub async fn` of `CoachingStoragePort` in sequence, asserts each returns `Ok`, then verifies via a direct SQL read (using `storage.connection_arc()`) or via the non-port sibling function (e.g., `coaching_storage::list_events_sync`).

- [ ] **Step 2: session_context_store_impl smoke test**

Same pattern for `SessionContextStorePort` — single method per the spec; smoke test invokes it once and asserts Ok.

- [ ] **Step 3: focus_storage_impl smoke test**

Invokes ALL 12 `FocusStorage` methods sequentially, using realistic arguments, and asserts Ok on each. Final state verification via a sibling sync method.

Write each smoke test as one `#[test]` or `#[tokio::test]` per the port's async-ness.

- [ ] **Step 4: Commit the 3 smoke tests**

```bash
git add crates/oneshim-storage/src/sqlite/coaching_storage_port_impl.rs \
        crates/oneshim-storage/src/sqlite/session_context_store_impl.rs \
        crates/oneshim-storage/src/sqlite/focus_storage_impl.rs
git commit -m "test(storage): Phase 5-D8 PR3 — delegator smoke tests (3)

One combined smoke test per delegator port per the spec's
thin-delegator Done-criteria variant. Validates that the trait
impl does not panic and propagates the underlying storage behaviour.
Does NOT duplicate port-contract-test coverage.

Closes: Task 16 from Phase 5-D8 PR3 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 17: Underlying-impl gap tests (Day 13, up to 10 tests, 1–3 commits)

**Files:** `crates/oneshim-storage/src/sqlite/tests.rs` — add a `// ── PR3 FocusStorage gap coverage ──` section header.

- [ ] **Step 1: Pick up to 10 uncovered `_sync` methods from Task 15 audit**

For each: write one happy-path test. Scenario = "direct call via `SqliteStorage::<method>_sync`, verify side effect via direct SQL or sibling method."

- [ ] **Step 2: Commit**

```bash
git commit -m "test(storage): Phase 5-D8 PR3 — FocusStorage gap coverage (N tests)

Closes underlying-impl gaps identified in Task 15 audit. Capped at
10 tests per spec Section 10.1 schedule risk."
```

### Task 18: PR3 final verify + open (Day 14)

Same structure as PR1 Task 11. Include the full audit table in the PR body. **PR3 merge gate:** 0 Critical + 0 Important on deep review.

---

## Per-PR Dependencies

- **PR1 blocks PR2** only if PR2 wants to use the directory-module split as a pattern. Otherwise PR2 can branch off `main` after PR1 merges OR develop in parallel on `feat/phase5-d8-storage-tests` with PR1 commits.
- **PR2 blocks PR3** — not strictly; PR3 is primarily audit-driven and touches unrelated files. BUT if PR3's underlying-impl gap tests go into `sqlite/tests.rs`, they should be based off `main` after PR1+PR2 merge to avoid conflicts.
- **Recommended order:** PR1 → merge → PR2 (off new branch from main) → merge → PR3 (off new branch from main). Sequential maintains clean review discipline.

## 3-Loop Review Checkpoints

Per `feedback_3loop_quality_gate.md` (already applied to the spec):

1. **Spec (Loop 1)** — ✅ COMPLETE. Exit at commit `48b14c3e`.
2. **Plan (Loop 2)** — THIS document. Exit criterion: 0 Critical + 0 Important in plan deep review. Fresh-eyes reviewer dispatched by the Ralph Loop runner.
3. **Impl (Loop 3)** — per PR:
   - Dispatch fresh-eyes reviewer after PR push.
   - Categorize findings.
   - Fix Critical + Important; acknowledge Minor in PR body or defer.
   - Re-review if fixes substantial.
   - Merge when clean.

The Ralph Loop runs each loop automatically per user mandate. Loop 3 runs N iterations per PR.

## Self-review (internal)

1. **Spec coverage check:**
   - Spec Section 4 PR1 Task 0 audit → Plan Task 0 ✅
   - Spec Section 4 PR1 refactor → Plan Task 1 ✅
   - Spec Section 4 PR1 18–25 test-count estimate → Plan 27 active tests (above upper bound by 2; acceptable — per-function target was aspirational, actual count driven by method signature).
   - Spec Section 4 PR1 concurrency (2 tests) → Plan Task 8 ✅
   - Spec Section 4 PR1 Err branches → Plan Task 9 ✅
   - Spec Section 4 PR2 tags+device → Plan Tasks 12–13 ✅
   - Spec Section 4 PR3 audit + smoke + underlying-impl cap → Plan Tasks 15–17 ✅
   - Spec Section 6 Done criteria (all 6) → implicitly enforced by Task 11 / 14 / 18 final verify.
   - Spec Section 9 review discipline → Checkpoint after each PR ✅
   - Spec Section 10.1 schedule risk → reflected in 7/5/4-day hard caps + bug-discovery policy in Task 3.

2. **Placeholder scan:** no "TBD", "TODO" in tasks; inline code `// TODO` comments in tests are the documented-unreachable-Err pattern from the spec, not plan placeholders.

3. **Type consistency:**
   - `HourlyMetricsRecord`: used in `list_hourly_metrics_since` (confirmed at `sqlite/mod.rs` — imported via `super::HourlyMetricsRecord`).
   - `SystemMetrics`, `ProcessSnapshot`, `ProcessSnapshotEntry`, `SessionStats`, `IdlePeriod`, `NetworkInfo`: all from `oneshim-core::models::*`; builder field names must match — verified during Task 2 Step 2.
   - Method names (`save_metrics`, `get_metrics`, etc.) — verified against `metrics.rs` lines 117–670.

---

## Execution Handoff

**Plan saved to `docs/reviews/2026-04-18-phase5-d8-storage-test-backfill-plan.md`.**

Per the Ralph Loop mandate, this plan will enter Loop 2 deep review before any implementation begins. Loop 2 iterates until Critical + Important = 0, then implementation (Loop 3) begins using whichever execution mode fits the session.
