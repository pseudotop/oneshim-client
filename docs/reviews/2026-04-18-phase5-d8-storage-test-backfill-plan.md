# Phase 5-D8 Storage Test Backfill Implementation Plan

> **Status: SHIPPED** (all 3 PRs: PR1 +17 tests, PR2 +3 tests, PR3 +7 tests). Plan preserved as execution history. CoreError assertion examples may use pre-[ADR-019](../architecture/ADR-019-error-code-infrastructure.md) tuple-variant syntax; current canonical form is `matches!(err, Err(CoreError::Internal { .. }))` or `err.code() == "internal.generic"`.

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

## PR1 — metrics.rs — 18 methods, ~15–16 active tests (audit-gated), 7-day hard cap

**⚠ AUDIT-CONTINGENT SCOPE:** The task counts below are PRE-AUDIT MAXIMUMS. Task 0 produces a dual-file coverage audit; Tasks 3–10 execute ONLY for methods the audit marks as having a "Residual gap for PR1." Duplicates of `sqlite/tests.rs` coverage are SKIPPED, not rewritten.

**Bug-discovery policy (applies to every Task 3–10):** if a new test reveals a real bug in production code (mismatch between expected and actual behaviour), apply the spec Section 10.1 protocol:
- Fix ≤ 20 LOC: land in this PR, add PR body callout.
- Fix > 20 LOC: file a separate bugfix PR, mark the failing test `#[ignore = "blocked by bugfix PR #NNN"]`, link from this PR.

**Git push cadence:** push after each Task that produces a commit (Tasks 3–10 each include an explicit `git push -u origin feat/phase5-d8-storage-tests` after their final commit — see each task's last Step). Tasks 0/1/2 are low-commit-churn (audit scratchpad, pure refactor, harness) and can defer push to the end of Task 3. If your machine crashes mid-PR, you lose at most one task's work:
```bash
# Standard post-commit push (shown inline in each task's Final Step):
git push -u origin feat/phase5-d8-storage-tests
```

### Task 0: Dual-file coverage audit (Day 1 AM, ~2 hours)

**Files (read-only):**
- `crates/oneshim-storage/src/sqlite/port_contract_tests.rs` — 337 LOC, 22 tests; contract-level port coverage
- `crates/oneshim-storage/src/sqlite/tests.rs` — 864 LOC; legacy **sibling** tests that already cover MANY of the target methods
- Artifact: paste audit table into `.claude/phase5-d8-progress.md` as scratchpad; final version goes into the PR1 body

**⚠ CRITICAL ASSUMPTION:** The original Phase 5-D8 gap was phrased as "files without inline `#[cfg(test)] mod tests`" — but many of those methods ARE tested via `sqlite/tests.rs`. The audit MUST check both files. Any test duplicating coverage in `sqlite/tests.rs` is dropped from PR1 scope and the decision recorded in the audit table.

- [ ] **Step 1: Read port_contract_tests.rs MetricsStorage section**

Run: `sed -n '232,264p' crates/oneshim-storage/src/sqlite/port_contract_tests.rs`

- [ ] **Step 2: Read all MetricsStorage-related tests in sqlite/tests.rs**

Run: `grep -nE '^(#\[test\]|#\[tokio::test\]|^fn |^async fn )' crates/oneshim-storage/src/sqlite/tests.rs | head -60`

Expected (confirmed 2026-04-18 at commit 041eeb4c): lines 139 (`concurrent_save_and_get`), 164 (`make_system_metrics`), 181 (`make_process_snapshot`), 201 (`save_and_get_metrics`), 214 (`cleanup_old_metrics`), 225 (`save_and_get_process_snapshot`), 241 (`idle_period_lifecycle`), 267 (`session_stats_lifecycle`), 304 (`session_not_found`), 311–424 (tags group, 7 tests), 505–577 (device_identity group, 5 tests).

- [ ] **Step 3: Produce the dual-file coverage table**

Produce exactly this table. Mark ✅ / ❌ / ⚠ based on the two source files:

```markdown
## Task 0: MetricsStorage dual-file coverage audit

| Method | port_contract_tests.rs | sqlite/tests.rs | Residual gap for PR1 |
|---|---|---|---|
| `save_metrics` | ✅ `ms_save_and_get_metrics_roundtrip` | ✅ `save_and_get_metrics` L201 | Edge cases only: NULL `NetworkInfo`, bulk 100+. |
| `get_metrics` | ✅ happy + empty range | ✅ `save_and_get_metrics` roundtrip | Edge cases only: same as above. |
| `aggregate_hourly_metrics` | ❌ | ❌ | **FULL**: happy / empty-hour / UTC-midnight boundary. |
| `cleanup_old_metrics` | ✅ empty-cutoff return value | ✅ `cleanup_old_metrics` L214 | Edge only: non-empty cutoff with boundary. |
| `save_process_snapshot` | ❌ | ✅ `save_and_get_process_snapshot` L225 | **NONE** — covered. Drop Task 4 save/get happy tests. |
| `get_process_snapshots` | ❌ | ✅ same | **NONE** — covered happy path; keep "invalid JSON silently defaults" as Task 9 (genuine Err-branch gap). |
| `cleanup_old_process_snapshots` | ❌ | ❌ | **1 test** — cutoff behaviour. |
| `start_idle_period` | ❌ | ✅ `idle_period_lifecycle` L241 | **NONE** — covered. |
| `end_idle_period` | ❌ | ✅ same | **NONE** — covered. |
| `get_ongoing_idle_period` | ❌ | ✅ same (covers Some case) | **1 test** — None case (fresh DB). |
| `get_idle_periods` | ❌ | ✅ same | **NONE** — covered. |
| `cleanup_old_idle_periods` | ❌ | ❌ | **1 test** — preserves active periods (`end_time IS NOT NULL` filter). |
| `upsert_session` | ❌ | ✅ `session_stats_lifecycle` L267 | **NONE** — covered (incl. ON CONFLICT update implicit in lifecycle test). Verify with `grep` before dropping. |
| `get_session` | ❌ | ✅ same + `session_not_found` L304 | **NONE** — covered. |
| `end_session` | ❌ | ✅ lifecycle | **NONE** — covered. |
| `increment_session_counters` | ❌ | ✅ lifecycle | Edge only: increment on nonexistent session is no-op (verify lifecycle covers this; if not, **1 test**). |
| `list_session_stats` (sync) | ❌ | ❌ | **FULL**: DESC ordering, LIMIT, empty. |
| `list_hourly_metrics_since` (sync) | ❌ | ❌ (only used transitively via aggregate tests in this plan) | **1 test**: `from_hour` filter across multiple aggregated hours. |

**PR1 scope (after audit):**
- **Task 3 retain** — aggregate_hourly_metrics: 3 tests (full gap).
- **Task 4 shrink dramatically** — process_snapshots: drop save/get happy; keep cleanup_old_process_snapshots + invalid-JSON deser (moved to Task 9). **Task 4 = 1 test (cleanup only) or 0 tests if cleanup is deemed low-value.**
- **Task 5 shrink dramatically** — idle_periods: drop lifecycle-duplicated tests; keep get_ongoing=None + cleanup preserves active. **Task 5 = 2 tests.**
- **Task 6 shrink to edge-only** — sessions: drop lifecycle; keep increment-on-nonexistent IF lifecycle doesn't cover it. **Task 6 = 0 or 1 test.**
- **Task 7 retain** — sync helpers: 3 tests (full gap).
- **Task 8 retain with scope note** — concurrency: verify existing `concurrent_save_and_get` at `sqlite/tests.rs:139` and reframe new tests to cover `save_process_snapshot` OR different invariant than existing. **Task 8 = 1–2 tests (depending on existing overlap).**
- **Task 9 retain** — Err branches: 1 active + 1 `#[ignore]` + 2 documented skips.
- **Task 10 retain (scope-aligned)** — contract-covered edge cases: NULL network, bulk 100+, cleanup boundary. **Task 10 = 3 tests.**

**Revised PR1 test count (expected): 15–16 active tests + 1 `#[ignore]` + 2 documented skips.** The original 27-test estimate is reduced because `sqlite/tests.rs` already covers most of the happy-path surface; the remaining gaps total 15–16 tests per the per-task breakdown in the Task 11 front-matter.
```

- [ ] **Step 4: Record audit in .claude/phase5-d8-progress.md**

```bash
# Append the audit table + a "Task 0 complete 2026-04-XX" line to
# .claude/phase5-d8-progress.md. Do NOT commit yet — the first actual
# commit is the Task 1 refactor. Progress tracker updates are local-only.
```

- [ ] **Step 5: Gate downstream tasks on the audit output**

For every test in Tasks 3–10 below, cross-check against the audit table's "Residual gap for PR1" column. **If a proposed test's target is marked "NONE — covered" in the audit, SKIP that test and note in PR1 body.** This overrides the plan's pre-audit test counts — the audit is authoritative.

- [ ] **Step 6: Zero-gap escape hatch**

**If the audit shows EVERY MetricsStorage method is either port-contract-covered or sqlite/tests.rs-covered with NO residual gaps AND no edge cases missing:**
- Tasks 3–10 are SKIPPED entirely.
- Proceed directly to Task 11 with the Task 1 directory-module refactor as the only substantive change.
- Task 11 PR body is replaced with a concise refactor-only description citing Task 0's audit table as proof of existing coverage.
- Per spec Section 10 acceptance, this still closes D8 because the coverage already exists — D8 was originally scoped as "modules without inline tests", which the refactor addresses by promoting `metrics.rs` to a directory module ready for future inline tests.

The same escape hatch applies to PR2 (Tasks 12-13) and PR3 (Tasks 15-17).

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
                memory_bytes: (100 + i as u64) * 1_048_576, // ~100+ MiB per entry
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

**Audit gate:** Task 0 marks this as **FULL gap** — execute all 3 tests.

**Bug-discovery reminder:** Section 10.1 protocol applies if any test fails due to a production divergence.

**Files:** `crates/oneshim-storage/src/sqlite/metrics/tests.rs`

Tests write raw `system_metrics` rows, call `aggregate_hourly_metrics`, then verify the resulting `system_metrics_hourly` row. Because `open_db()` returns a raw Connection but `aggregate_hourly_metrics` is a `SqliteStorage` method, these tests use `open_storage()` and direct SQL via `storage.connection_arc()` for seeding.

**Date choice:** tests use today-relative hour boundaries via `Utc::now()` + `with_minute(0).with_second(0).with_nanosecond(0)` to avoid hardcoded calendar dates (per `feedback_time_relative_test_dates.md`).

- [ ] **Step 1: Write the happy-path aggregation test**

Append to `metrics/tests.rs` under a new `// ── aggregate_hourly_metrics ──────────────────` section header:

```rust

// ── aggregate_hourly_metrics ──────────────────────────────────

/// Helper: round `Utc::now()` to the start of its own hour (minute=0,
/// second=0, nanos=0). Used so tests never rely on hardcoded dates.
fn current_hour_start() -> DateTime<Utc> {
    Utc::now()
        .with_minute(0)
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .expect("truncation to hour should always succeed")
}

#[tokio::test]
async fn aggregate_hourly_metrics_rolls_up_samples_in_hour() {
    let storage = open_storage();
    let hour_start = current_hour_start();
    let hour_key = hour_start.format("%Y-%m-%dT%H:00:00Z").to_string();

    // Seed 3 samples in the current hour with known CPU values.
    for (offset_min, cpu) in [(5, 20.0_f32), (20, 60.0_f32), (50, 40.0_f32)] {
        let ts = hour_start + Duration::minutes(offset_min);
        storage.save_metrics(&sample_metrics(ts, cpu)).await.unwrap();
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
```

- [ ] **Step 2: Write the empty-hour test**

```rust
#[tokio::test]
async fn aggregate_hourly_metrics_empty_hour_writes_no_row() {
    let storage = open_storage();
    // Pick 6 hours in the future so no sample could possibly be there.
    let hour_start = current_hour_start() + Duration::hours(6);
    let hour_key = hour_start.format("%Y-%m-%dT%H:00:00Z").to_string();

    // No samples seeded for this hour.
    storage.aggregate_hourly_metrics(hour_start).await.unwrap();

    let rows = storage.list_hourly_metrics_since(&hour_key).unwrap();
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
    // Hour labels are midnight-crossing — verify structurally, not by hardcoded string.
    assert!(rows[1].hour.ends_with("T00:00:00Z"));
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

git push -u origin feat/phase5-d8-storage-tests
```

---

### Task 4: Tests for process_snapshots group (Day 2 PM, ~0.5 hour after audit, 1 test max, 1 commit)

**⚠ AUDIT-GATED:** Per Task 0 dual-file audit — `save_process_snapshot` and `get_process_snapshots` are ALREADY covered by `sqlite/tests.rs:225` (`save_and_get_process_snapshot`). `cleanup_old_process_snapshots` is the only genuine gap.

**Scope after audit:**
- SKIP the "roundtrip + DESC order" test (duplicate of `sqlite/tests.rs:225`).
- SKIP the "empty range" test (no direct duplicate but trivial; the coverage already exists).
- SKIP the "limit respected" test (trivial, same reason).
- **RETAIN** the cleanup-cutoff test ONLY (genuine gap — no `cleanup_old_process_snapshots` test exists anywhere).

The test specs below marked "[DROP per audit]" are kept in the plan for traceability — the implementer skips them and documents the skip in the PR1 body.

**Bug-discovery reminder:** Section 10.1 protocol applies.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: [DROP per audit — reference only, DO NOT EXECUTE]** Roundtrip + multi-timestamp ordering test

SKIP — this duplicates `sqlite/tests.rs:225 save_and_get_process_snapshot`. Document in PR1 body:

```markdown
- PR1 Task 4 Step 1: SKIPPED. Duplicate of `sqlite/tests.rs:225`.
```

The original spec (for reference, not for execution):

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

- [ ] **Step 2: [DROP per audit — reference only, DO NOT EXECUTE]** Empty-range test

SKIPPED. `save_and_get_process_snapshot` at `sqlite/tests.rs:225` + port-contract patterns cover similar "no results" scenarios. The reference code below is retained for traceability only:

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

- [ ] **Step 3: [RETAIN per audit — genuine gap]** Write cleanup-cutoff test

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

- [ ] **Step 4: [DROP per audit — reference only, DO NOT EXECUTE]** Limit-respected test

SKIPPED. Trivial coverage; the sqlite/tests.rs lifecycle test implicitly depends on LIMIT semantics. Reference code:

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
# Expect: 3 (Task 3) + 1 (Task 4 Step 3 only) = 4 metrics tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): process_snapshots cleanup cutoff (1 test, audit-gated)

Task 0 audit showed save_process_snapshot and get_process_snapshots
are already covered by sqlite/tests.rs:225. cleanup_old_process_snapshots
is the only residual gap — this commit adds that single test.

Closes: Task 4 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"

git push -u origin feat/phase5-d8-storage-tests
```

---

### Task 5: Tests for idle_periods group (Day 3 AM, ~1 hour after audit, 2 tests max, 1 commit)

**⚠ AUDIT-GATED:** Per Task 0 dual-file audit — `idle_period_lifecycle` at `sqlite/tests.rs:241` ALREADY covers `start_idle_period`, `end_idle_period`, `get_ongoing_idle_period` (Some case), `get_idle_periods`. Genuine gaps:
- `get_ongoing_idle_period` None case (lifecycle test covers Some path but not None path — confirm by reading L241 contents before implementing).
- `cleanup_old_idle_periods` WHERE-end_time-IS-NOT-NULL filter — no dedicated cleanup test for idle_periods exists.

**Scope after audit:**
- SKIP Step 1 (start + get_ongoing roundtrip — duplicate of lifecycle).
- SKIP Step 2 (end_idle_period roundtrip — duplicate of lifecycle).
- **RETAIN** Step 3 (get_ongoing None case) — verify against L241 first; if lifecycle already has a pre-start None assertion, drop.
- SKIP Step 4 (most-recent wins) — requires no dedicated test if lifecycle exercises the single-period case; new coverage here adds marginal value.
- SKIP Step 5 (time-range filter — duplicate of lifecycle's range query).
- **RETAIN** Step 6 (cleanup preserves active) — new coverage for an untested filter invariant.

Final expected: **2 tests max** — get_ongoing None, cleanup preserves active.

**Bug-discovery reminder:** Section 10.1 protocol applies.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: [DROP per audit — reference only, DO NOT EXECUTE]** Start + get_ongoing roundtrip

SKIPPED. `sqlite/tests.rs:241 idle_period_lifecycle` already exercises start → get_ongoing=Some → end → get_idle_periods. Reference code:

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

- [ ] **Step 2: [DROP per audit — reference only, DO NOT EXECUTE]** end_idle_period behaviour

SKIPPED. Same `idle_period_lifecycle` coverage. Reference code:

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

- [ ] **Step 3: [RETAIN per audit — genuine gap]** get_ongoing returns None on fresh DB

```rust
#[tokio::test]
async fn get_ongoing_idle_period_none_when_no_periods() {
    let storage = open_storage();
    let result = storage.get_ongoing_idle_period().await.unwrap();
    assert!(result.is_none());
}
```

- [ ] **Step 4: [DROP per audit — reference only, DO NOT EXECUTE]** Most-recent wins

SKIPPED. Lifecycle test covers the single-period case; multi-ongoing-period is an unlikely invariant to regress. Reference code:

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

- [ ] **Step 5: [DROP per audit — reference only, DO NOT EXECUTE]** Time-range filter

SKIPPED. Lifecycle exercises range queries. Reference code:

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

- [ ] **Step 6: [RETAIN per audit — genuine gap]** Cleanup preserves active (end_time IS NULL)

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
# Expect 3 (Task 3) + 1 (Task 4) + 2 (Task 5 Steps 3 and 6) = 6 metrics tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): idle_periods — None + cleanup-preserves-active (2, audit-gated)

Task 0 audit showed idle_period_lifecycle at sqlite/tests.rs:241
covers start/end/get_ongoing=Some/get_idle_periods. Genuine gaps:
- get_ongoing returns None on fresh DB (never started)
- cleanup_old_idle_periods preserves rows with end_time IS NULL

Closes: Task 5 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"

git push -u origin feat/phase5-d8-storage-tests
```

---

### Task 6: Tests for sessions group (Day 3 PM, ~0.5–1 hour after audit, 1 test, 1 commit)

**⚠ AUDIT-GATED:** Per Task 0 dual-file audit — `session_stats_lifecycle` at `sqlite/tests.rs:267` + `session_not_found` at `sqlite/tests.rs:304` ALREADY cover `upsert_session`, `get_session`, `end_session`, and `increment_session_counters` on an EXISTING session (verified at line 289). The increment-on-nonexistent path is NOT covered.

**Scope after audit:**
- SKIP Steps 1–5 — all duplicate of lifecycle.
- **RETAIN** Step 6 — increment on nonexistent session is no-op. This is a confirmed gap per the audit; decision is upfront, not deferred to impl time.

Final expected: **1 test**.

**Bug-discovery reminder:** Section 10.1 protocol applies.

**Files:** `metrics/tests.rs`

- [ ] **Step 1: [DROP per audit — reference only, DO NOT EXECUTE]** Upsert-then-get roundtrip

SKIPPED. `sqlite/tests.rs:267 session_stats_lifecycle` covers this. Reference code:

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

- [ ] **Step 2: [DROP per audit — reference only, DO NOT EXECUTE]** Upsert ON CONFLICT updates existing

SKIPPED. Covered by session_stats_lifecycle (implicit in the upsert-then-update pattern). Reference code:

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

- [ ] **Step 3: [DROP per audit — reference only, DO NOT EXECUTE]** Get nonexistent returns None

SKIPPED. `sqlite/tests.rs:304 session_not_found` is exactly this test. Reference code:

```rust
#[tokio::test]
async fn get_session_nonexistent_returns_none() {
    let storage = open_storage();
    let got = storage.get_session("does-not-exist").await.unwrap();
    assert!(got.is_none());
}
```

- [ ] **Step 4: [DROP per audit — reference only, DO NOT EXECUTE]** end_session sets ended_at

SKIPPED. Covered by session_stats_lifecycle. Reference code:

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

- [ ] **Step 5: [DROP per audit — reference only, DO NOT EXECUTE]** Increment accumulates on existing session

SKIPPED. session_stats_lifecycle at `sqlite/tests.rs:267` exercises accumulation on an existing session. Reference code:

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

- [ ] **Step 6: [RETAIN per audit — genuine gap]** Increment on nonexistent session is a no-op

Per reviewer iter-2 M-P2: `session_stats_lifecycle` increments on an EXISTING session at `sqlite/tests.rs:289`; the nonexistent-row case is NOT covered. Decide upfront: RETAIN this test.

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
# Expect 3 + 1 + 2 + 1 = 7 metrics tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): sessions — increment-on-nonexistent no-op (1, audit-gated)

Task 0 audit showed session_stats_lifecycle + session_not_found at
sqlite/tests.rs:267, 304 cover upsert / get / end / increment on
existing session. Genuine gap: increment_session_counters on a
nonexistent session must be a no-op (SQLite UPDATE 0 rows).

Closes: Task 6 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"

git push -u origin feat/phase5-d8-storage-tests
```

---

### Task 7: Tests for sync list helpers (Day 4 AM, ~1 hour, 3 tests, 1 commit)

**⚠ AUDIT-GATED (retained — full gap):** Per Task 0 — neither file covers `list_session_stats` or the `from_hour` filter of `list_hourly_metrics_since`. Execute all 3 tests.

**Bug-discovery reminder:** Section 10.1 protocol applies.

Group: `list_session_stats` (sync), `list_hourly_metrics_since` (sync — already exercised by Task 3 but add one more).

**Date choice:** use `Utc::now()` + offsets (today-relative) — no hardcoded calendar dates.

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
    // Anchor to the hour AT LEAST 3 hours in the past (so all 3 seeded hours
    // are in the past relative to Utc::now() and the filter is exercised).
    let anchor = current_hour_start() - Duration::hours(3);

    // Seed 3 hours of raw samples, then aggregate each.
    for h in 0..3 {
        let hour_start = anchor + Duration::hours(h);
        storage
            .save_metrics(&sample_metrics(hour_start + Duration::minutes(30), 25.0))
            .await
            .unwrap();
        storage.aggregate_hourly_metrics(hour_start).await.unwrap();
    }

    // Filter: from hour+1 onward — should see 2 rows (h+1 and h+2).
    let from_key = (anchor + Duration::hours(1))
        .format("%Y-%m-%dT%H:00:00Z")
        .to_string();
    let results = storage.list_hourly_metrics_since(&from_key).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].hour, from_key);
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests
# Expect 3 + 1 + 2 + 1 + 3 = 10 metrics tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): sync list helpers — session_stats + hourly since

3 tests covering the 2 uncovered sync helpers (genuine gap per audit):
- list_session_stats: DESC ordering by started_at, LIMIT respected
- list_session_stats: empty DB returns empty vec
- list_hourly_metrics_since: from_hour filter correctness after
  aggregation of multiple hours

Closes: Task 7 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"

git push -u origin feat/phase5-d8-storage-tests
```

---

### Task 8: Lock-contract concurrency tests (Day 4 PM, ~2 hours, 1–2 tests, 1 commit)

**⚠ AUDIT-GATED:** `sqlite/tests.rs:139` already contains `concurrent_save_and_get`. Read it before writing the new tests. Audit decision:
- If `concurrent_save_and_get` exercises the "N writers + 1 reader consistent snapshot" invariant → SKIP Task 8 Step 2 (duplicate).
- If it only covers "N writers preserve total count" → SKIP Task 8 Step 1 (duplicate). Retain Step 2.
- If it's a different invariant (e.g., single writer sequential) → retain BOTH Steps 1 and 2 but reference `concurrent_save_and_get` in the test-module-level comment.

Final expected: **1–2 tests** depending on existing overlap. Document the decision in the PR1 body.

Per spec Section 4 PR1 + Section 6 #3: these are NOT race tests. They assert the Mutex contract survives across threads.

**Bug-discovery reminder:** Section 10.1 protocol applies.

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
# Expect: 1–2 PASS per audit outcome.

cargo test -p oneshim-storage metrics::tests
# Expect: 10 + (1 or 2) = 11 or 12 metrics tests green.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): lock-contract regression tests (1-2, audit-gated)

NOT race tests — Arc<Mutex<Connection>> already serializes. These
tests assert the Mutex contract survives across multiple async tasks
on a multi-thread tokio runtime (these 2 tests are the only ones
that use flavor = multi_thread in the metrics module).

- concurrent_save_metrics_preserves_count: 4 tasks × 5 samples = 20
- concurrent_writer_plus_reader_yield_consistent_snapshot: reader
  sees 0..=50 rows monotonically while writer runs

Closes: Task 8 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"

git push -u origin feat/phase5-d8-storage-tests
```

---

### Task 9: Err-branch tests (Day 5, ~4 hours, 1 active + 1 `#[ignore]` + 2 TODO docs, 1 commit)

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

- [ ] **Step 2: Document closed-DB Err as unreachable**

No test code. Add an inline TODO comment in `metrics/tests.rs` (just before Step 1's test) capturing the documented-unreachable decision per spec Section 7 escape hatch:

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

- [ ] **Step 4: Document RFC3339 fallback as unreachable**

No test code. Add an inline TODO comment in `metrics/tests.rs` capturing the documented-unreachable decision per spec Section 7 escape hatch:

```rust
// TODO: untested Err — get_metrics RFC3339 parse fallback.
// The fallback branch (`unwrap_or_else(|_| Utc::now())`) IS reachable
// in production if a row has a malformed timestamp column, but cannot
// be deterministically exercised from a test because SQL string-ordering
// on 'not-a-date' vs RFC3339 range bounds is undefined for our purposes.
// Also it is a SILENT FALLBACK (returns a value, not Err), so even a
// successful trigger would not exercise an Err branch.
```

- [ ] **Step 5: Run + commit**

```bash
cargo test -p oneshim-storage metrics::tests
# Expect: (11 or 12) + 1 active Err test = 12 or 13 green.
# Plus 1 #[ignore] (mutex poisoning) + 2 documented-unreachable TODO comments.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): Err-branch coverage + unreachable-Err documentation

- get_process_snapshots silently-swallowed JSON parse (by design, 1 active)
- Mutex poisoning attempt (#[ignore], OPTIONAL per spec Section 7)
- Inline TODOs for 2 unreachable Err paths (closed connection,
  get_metrics RFC3339 parse fallback) per Section 6 #2 escape hatch

Closes: Task 9 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"

git push -u origin feat/phase5-d8-storage-tests
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
# Expect: (12 or 13) + 3 = 15 or 16 active tests + 1 #[ignore] = 16 or 17 test items.

git add crates/oneshim-storage/src/sqlite/metrics/tests.rs
git commit -m "test(metrics): contract-covered edge cases (3 tests)

Edge cases for the 3 MetricsStorage methods already contract-covered
(save/get/cleanup) but whose happy-path-only contract tests miss:
- NULL network field round-trips as (0, 0, is_connected=false)
- Bulk 100-sample write + read preserves all rows
- Cleanup cutoff: deletes strictly-older rows, preserves strictly-newer

Closes: Task 10 from Phase 5-D8 PR1 plan.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"

git push -u origin feat/phase5-d8-storage-tests
```

---

### Task 11: Final verify + push + open PR (Day 6 PM, ~2 hours)

**Files:** no new edits

**Test count:** variable based on Task 0 audit outcome. Expected range **15–16 active tests + 1 `#[ignore]` + 2 documented-unreachable TODO comments**. The old "27 tests" estimate is pre-audit; the post-audit target composition:
- Task 3: 3 active (retained — full gap)
- Task 4: 1 active (cleanup_old_process_snapshots — only residual gap)
- Task 5: 2 active (get_ongoing None + cleanup preserves active)
- Task 6: 1 active (increment on nonexistent — audit-confirmed gap)
- Task 7: 3 active (retained — full gap)
- Task 8: 1–2 active (audit-dependent; reviewer confirmed no overlap with existing `concurrent_save_and_get` which tests `save_event`, not `save_metrics`)
- Task 9: 1 active + 1 `#[ignore]` + 2 TODO docs
- Task 10: 3 active (retained — edge cases for contract-covered methods)

Total: 15–16 active tests + 1 `#[ignore]` = 16–17 test items.

- [ ] **Step 1: Full workspace test suite**

Run: `cargo test --workspace`
Expected: all green (new metrics::tests + pre-existing tests all green).

- [ ] **Step 2: Full workspace clippy**

Run: `cargo clippy --workspace --all-targets`
Expected: zero warnings.

- [ ] **Step 3: Fmt check**

Run: `cargo fmt --check`
Expected: no diff.

- [ ] **Step 4: Verify the audit-driven coverage matrix**

Walk through the Task 0 dual-file audit table. For each row with "Residual gap for PR1" that is NOT "NONE — covered":
- Find the corresponding test name in `metrics/tests.rs`.
- Record the mapping in the PR1 body.

For each row where the residual gap column IS "NONE — covered":
- Record in the PR1 body that this was deliberately skipped because of `sqlite/tests.rs` / `port_contract_tests.rs` coverage.

Expected count: **15–16 active tests + 1 `#[ignore]` + 2 documented-unreachable TODO comments**, matching the Task 11 front-matter.

- [ ] **Step 5: Push branch**

```bash
git push -u origin feat/phase5-d8-storage-tests
```

- [ ] **Step 6: Open PR1**

```bash
gh pr create \
  --base main \
  --head feat/phase5-d8-storage-tests \
  --title "test(storage): Phase 5-D8 PR1 — metrics.rs audit-gated coverage" \
  --body "$(cat <<'EOF'
## Summary

Phase 5-D8 PR1 — closes the genuine MetricsStorage test-coverage gaps per
[the D8 analysis](docs/reviews/2026-04-16-feature-gaps-analysis.md)
and [the Phase 5-D8 spec](docs/reviews/2026-04-18-phase5-d8-storage-test-backfill-spec.md).

**Important scoping note:** Task 0 audit revealed `sqlite/tests.rs` already covers most MetricsStorage method surfaces. This PR adds tests ONLY for genuine gaps (~15–16 tests), not the pre-audit 27-test estimate. The directory-module refactor (`metrics.rs` → `metrics/{mod.rs, tests.rs}`) provides structural value alongside the tests.

## Changes

- **Refactor:** `sqlite/metrics.rs` promoted to directory module (`sqlite/metrics/{mod.rs, tests.rs}`) per ADR-003. Pure file relocation — zero behaviour delta.
- **Tests:** 15–16 active tests + 1 `#[ignore]` (optional mutex-poison) + 2 documented-unreachable TODO comments. Exact count depends on audit outcome. Per-task breakdown:
  - Task 3 — `aggregate_hourly_metrics`: 3 tests (happy / empty / midnight)
  - Task 4 — `cleanup_old_process_snapshots`: 1 test (cutoff behaviour)
  - Task 5 — `idle_periods` edge cases: 2 tests (get_ongoing None, cleanup preserves active)
  - Task 6 — `sessions` edge case: 1 test (increment on nonexistent session is no-op)
  - Task 7 — sync list helpers: 3 tests (list_session_stats DESC + LIMIT + empty; list_hourly_metrics_since filter)
  - Task 8 — lock-contract regression: 1–2 tests (multi-thread tokio, no overlap with existing `concurrent_save_and_get` which tests `save_event`)
  - Task 9 — Err branches + unreachable: 1 active test (invalid JSON silent fallback) + 1 `#[ignore]` + 2 TODO docs (closed-DB, RFC3339 fallback)
  - Task 10 — contract-covered edge cases: 3 tests (NULL network, bulk 100, non-empty cleanup boundary)

## Task 0 dual-file audit summary

| Method | port_contract_tests.rs | sqlite/tests.rs | This PR adds |
|---|---|---|---|
| save_metrics | ✅ happy | ✅ roundtrip | NULL network + bulk 100 (Task 10) |
| get_metrics | ✅ happy + empty range | ✅ via roundtrip | bulk 100 (Task 10) |
| aggregate_hourly_metrics | ❌ | ❌ | 3 tests (Task 3) |
| cleanup_old_metrics | ✅ empty cutoff | ✅ | non-empty cutoff edge (Task 10) |
| save_process_snapshot | ❌ | ✅ at L225 | nothing — fully covered |
| get_process_snapshots | ❌ | ✅ at L225 | invalid-JSON silent fallback (Task 9) |
| cleanup_old_process_snapshots | ❌ | ❌ | 1 test (Task 4) |
| start/end/get_ongoing/get_idle_periods | ❌ | ✅ lifecycle at L241 | get_ongoing None + cleanup-preserves-active (Task 5) |
| cleanup_old_idle_periods | ❌ | ❌ | covered by Task 5 cleanup-preserves-active |
| upsert/get/end/increment sessions | ❌ | ✅ lifecycle L267 + not_found L304 | increment on nonexistent no-op (Task 6) |
| list_session_stats | ❌ | ❌ | 2 tests (Task 7) |
| list_hourly_metrics_since | ❌ | ❌ | 1 test (Task 7) |

**Methods with no new PR tests are NOT regressions** — their existing coverage was verified during the audit and is cited above.

## Unreachable Err branches (documented)

- `save_metrics` closed-connection: Connection held inside SqliteStorage
  for lifetime; would require Connection-trait injection.
- `get_metrics` RFC3339 parse fallback: string-ordering in SQL range
  filter is non-deterministic on malformed timestamps.

## Verification

- `cargo test --workspace`: green (+15–16 active tests + 1 `#[ignore]`).
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

## PR2 — tags.rs + device_identity.rs — audit-dependent, 5-day hard cap

**⚠ MAJOR SCOPE CAVEAT:** `sqlite/tests.rs` already contains:
- **tags tests** at lines 311–424 (7 tests): `create_and_get_tags`, `delete_tag`, `get_tag_by_id`, `update_tag`, `frame_tag_operations`, `duplicate_tag_name_fails`, `add_tag_to_frame_idempotent`.
- **device_identity tests** at lines 505–577 (5 tests): `ensure_device_identity_generates_uuid_on_first_call`, `ensure_device_identity_returns_same_id_on_second_call`, `ensure_device_identity_persists_across_reopens`, `reset_device_identity_generates_new_uuid`, `reset_device_identity_allows_re_ensure`.

**PR2 may be nearly or entirely redundant.** Before writing tests, Task 12 MUST audit these existing tests and identify GENUINE gaps (edge cases, error paths, concurrency invariants, methods with no test). The original spec Section 4 PR2 assumed no such coverage; that assumption is false.

**If the audit finds no genuine gaps, PR2 becomes a no-op or converts into a documentation-only PR** that records "PR2 scope closed by existing `sqlite/tests.rs` coverage — 0 new tests."

### Task 12: Audit + (conditional) tests block for tags.rs (Days 8–9, 0–15 tests)

**Files:** read-only `crates/oneshim-storage/src/sqlite/tests.rs:311-424`; conditionally modify `crates/oneshim-storage/src/sqlite/tags.rs`.

- [ ] **Step 0 (BLOCKING): Audit tags coverage in sqlite/tests.rs**

Run: `sed -n '311,424p' crates/oneshim-storage/src/sqlite/tests.rs`

Produce this audit table:

```markdown
| tags.rs method | sqlite/tests.rs test | Residual gap for PR2 |
|---|---|---|
| create_tag | create_and_get_tags + duplicate_tag_name_fails | ? — check whether empty-name or edge cases missing |
| delete_tag | delete_tag | ? |
| get_tag_by_id | get_tag_by_id | likely NONE |
| update_tag | update_tag | ? — check whether only name or also color change |
| add_tag_to_frame | frame_tag_operations + add_tag_to_frame_idempotent | likely NONE |
| remove_tag_from_frame | frame_tag_operations | likely NONE |
| get_tags_for_frame | frame_tag_operations | likely NONE |
| get_frames_by_tag | frame_tag_operations | likely NONE |
| (any other pub fn?) | ? | ? |
```

Fill the `?` cells by reading `tags.rs` and comparing against the existing tests. For any method with "likely NONE", verify by reading the actual test body.

- [ ] **Step 1: Confirm tags.rs public API**

Run: `grep -n "pub fn\|pub async fn" crates/oneshim-storage/src/sqlite/tags.rs`

Any method NOT listed in the audit table gets a gap-fill test. Concurrency (`UNIQUE(name)` race with 4 threads) is a GENUINE gap — the existing `duplicate_tag_name_fails` is sequential.

- [ ] **Step 2: Write ONLY the gap-fill tests using the PR1 harness pattern**

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

### Task 13: Audit + (conditional) tests block for device_identity.rs (Day 10, 0–3 tests)

**⚠ AUDIT-HEAVY:** `sqlite/tests.rs:505-577` already has 5 tests. The original spec's 8 scenarios overlap heavily:
- first-create / second-load / persist-across-reopen / reset-new / reset-allows-re-ensure ALL covered.
- Genuine potential gaps: singleton CHECK violation on direct-insert (not tested), corruption recovery (not tested if the module has one).

**Files:** read-only `sqlite/tests.rs:505-577`; conditionally modify `crates/oneshim-storage/src/sqlite/device_identity.rs`.

- [ ] **Step 0 (BLOCKING): Audit device_identity coverage**

Run: `sed -n '505,577p' crates/oneshim-storage/src/sqlite/tests.rs` + `cat crates/oneshim-storage/src/sqlite/device_identity.rs`

Produce audit table:

```markdown
| device_identity.rs path | sqlite/tests.rs test | Residual gap for PR2 |
|---|---|---|
| first-create on empty DB | ensure_device_identity_generates_uuid_on_first_call | NONE |
| second-load same id | ensure_device_identity_returns_same_id_on_second_call | NONE |
| persist across open-close-open | ensure_device_identity_persists_across_reopens | NONE |
| reset → new id differs | reset_device_identity_generates_new_uuid | NONE |
| reset → re-ensure works | reset_device_identity_allows_re_ensure | NONE |
| singleton CHECK(id=1) enforced | ? | verify if a direct-insert test exists |
| corruption / malformed row | ? | verify if module has a defensive path |
```

- [ ] **Step 1: Identify public methods**

Run: `grep -n "pub fn\|pub async fn" crates/oneshim-storage/src/sqlite/device_identity.rs`

- [ ] **Step 2: Write ONLY the 0–3 gap-fill tests**

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

### After Loop-2 iter-1 deep review (2026-04-18)

Critical and Important fixes landed:

- **C1 FIXED:** `ProcessSnapshotEntry.memory_bytes` (was `memory_mb`) — verified against `oneshim-core/src/models/activity.rs:96`.
- **C2 FIXED:** Task 0 audit expanded to BOTH `port_contract_tests.rs` AND `sqlite/tests.rs`. Tasks 4–6 reframed as audit-gated with explicit skip conditions. PR2 Tasks 12–13 similarly gated. Expected PR1 test count reduced from 27 to 15–16.
- **I1 FIXED:** Hardcoded 2026-04-15 dates in Tasks 3/7 replaced with today-relative helpers (`current_hour_start()`, `Utc::now() - Duration::hours(N)`).
- **I2 FIXED:** Test count numbers updated in Task 11 Step 4 and the final verify to reflect audit-driven variability.
- **I3 FIXED:** Bug-discovery policy reminder added as a PR1-front-matter note covering all Tasks 3–10; each task also has a per-task reminder.
- **I4 FIXED:** Task 9 Step 2 (closed-DB) and Step 4 (RFC3339 fallback) both trimmed to documentation-only entries — no contradictory test code.

### Residual consistency checks

1. **Spec coverage:**
   - Spec Section 4 PR1 Task 0 audit → Plan Task 0 ✅ (expanded)
   - Spec Section 4 PR1 refactor → Plan Task 1 ✅
   - Spec Section 6 Done criteria "real-logic modules" → downgraded expectations after audit confirmation
   - Spec Section 4 PR1 concurrency (2 tests) → Plan Task 8 ⚠ audit-gated (1–2 depending on existing overlap)
   - Spec Section 4 PR1 Err branches → Plan Task 9 ✅
   - Spec Section 4 PR2 tags+device → Plan Tasks 12–13 ⚠ both audit-gated and likely reduced to near-zero
   - Spec Section 4 PR3 audit + smoke + underlying-impl cap → Plan Tasks 15–17 ✅
   - Spec Section 9 review discipline → Checkpoint after each PR ✅
   - Spec Section 10.1 schedule risk → 7/5/4-day hard caps + bug-discovery policy in PR1 front-matter + per-task reminders.

2. **Placeholder scan:** `// TODO: untested Err` comments in tests are part of the spec's escape-hatch mechanism (Section 6 #2), not plan placeholders. No "TBD" / unfilled sections remain.

3. **Type consistency:**
   - `HourlyMetricsRecord`: used in `list_hourly_metrics_since` (confirmed at `sqlite/mod.rs` — imported via `super::HourlyMetricsRecord`).
   - `SystemMetrics`, `ProcessSnapshot`, `ProcessSnapshotEntry` (with **`memory_bytes: u64`**), `SessionStats`, `IdlePeriod`, `NetworkInfo`: all from `oneshim-core::models::*`; field names verified against the actual struct definitions.
   - Method names (`save_metrics`, `get_metrics`, etc.) — verified against `metrics.rs` lines 117–670.
   - `chrono::TimeZone::with_ymd_and_hms` and `DateTime::with_minute/with_second/with_nanosecond` — standard chrono 0.4 APIs, confirmed by existing use in `metrics.rs:214-218`.

4. **Spec-driven scope sanity:** the original spec counted 6 target modules as "untested." The Loop-2 review revealed this was an inline-vs-sibling distinction, not a coverage gap — most methods ARE tested via `sqlite/tests.rs`. Rather than discarding D8, the plan is retained with audit-driven scope: add inline tests ONLY for methods that have genuine coverage gaps. If the audits show no gaps, the phase becomes documentation-only. The spec's scope retains PR1 as a directory-module refactor (value: ADR-003 alignment + enables future test-organization) even if PR1 adds very few or zero new tests.

---

## Execution Handoff

**Plan saved to `docs/reviews/2026-04-18-phase5-d8-storage-test-backfill-plan.md`.**

Per the Ralph Loop mandate, this plan will enter Loop 2 deep review before any implementation begins. Loop 2 iterates until Critical + Important = 0, then implementation (Loop 3) begins using whichever execution mode fits the session.
