# SQLite Performance Tuning — Design Spec

> Created: 2026-03-21
> Status: Proposed
> Scope: oneshim-storage (sqlite.rs, migration.rs), src-tauri (scheduler)
> Reference: ADR-003 (Directory Module Pattern for Large Source Files)

## 1. Current State

### Code-Level Findings

| Area | Location | Finding |
|------|----------|---------|
| WAL mode | `sqlite.rs` PRAGMA setup | Enabled (`journal_mode=WAL`), `synchronous=NORMAL` |
| Page cache | `sqlite.rs` PRAGMA setup | `cache_size=-8000` (8MB) |
| Temp store | `sqlite.rs` PRAGMA setup | `temp_store=MEMORY` |
| Busy timeout | `sqlite.rs` PRAGMA setup | `busy_timeout=5000` (5s) |
| Batch inserts | `sqlite.rs` | Uses transactions for batch operations |
| Compound indexes | `migration.rs` | Present on high-traffic tables |
| WAL checkpoint | `scheduler/loops.rs` (sync/aggregate loops) | No explicit checkpoint in scheduler — only in IVF build (build.rs:176,295) |

## 2. Architecture

### A. PRAGMA Optimization

Review and tune existing PRAGMA settings:
- Verify `mmap_size` is set for read-heavy workloads
- Consider `page_size=4096` alignment with OS page size
- Evaluate `auto_vacuum=INCREMENTAL` for long-running sessions

### B. Index Audit

- Identify unused indexes via `EXPLAIN QUERY PLAN`
- Add covering indexes for frequent query patterns
- Review partial indexes for time-bounded queries

### C. Batch Insert Optimization

- Use `INSERT OR REPLACE` with prepared statements
- Batch sizes: 100-500 rows per transaction
- Avoid individual `INSERT` in loops

### D. Memory-Mapped I/O

- Enable `PRAGMA mmap_size` for read-heavy tables
- Benchmark with and without mmap on macOS/Windows/Linux

### E. Connection Pool Tuning

- Single writer, multiple readers (WAL mode advantage)
- Evaluate `rusqlite::Connection` pool vs. single connection with `block_in_place`

### F. Vacuum Strategy

- `PRAGMA auto_vacuum=INCREMENTAL` + periodic `PRAGMA incremental_vacuum`
- Schedule vacuum during idle periods (see Section H)

### G. Query Plan Analysis

- Add `EXPLAIN QUERY PLAN` logging in debug builds
- Identify full table scans on tables with >10K rows
- Add missing indexes based on query plan analysis

### H. WAL Checkpoint in Scheduler

- Currently no WAL checkpoint in scheduler loops
- Add `PRAGMA wal_checkpoint(PASSIVE)` to sync loop (every 10 seconds) — non-blocking
- Idle callback identified: `IdleState::Active -> Idle` transition is optimal VACUUM insertion point
- Add `wal_autocheckpoint=1000` to PRAGMA setup (default already, but make explicit)

### I. Edge Intelligence Retention Gap

**Gap (round 3):** V6 tables `work_sessions`, `interruptions`, `focus_metrics` have
**no retention policy** — data accumulates indefinitely. Other tables (events, frames,
metrics) have 30-day retention.

**Fix:** Add retention enforcement in the aggregation loop:
- `work_sessions`: DELETE WHERE `ended_at < cutoff AND state = 'completed'` (30 days)
- `interruptions`: DELETE WHERE `interrupted_at < cutoff` (30 days)
- `focus_metrics`: DELETE WHERE `date < cutoff` (90 days — daily aggregates, longer lifecycle)

**Write frequency confirmed:** ~7 writes/sec peak, single Mutex sufficient. No connection pool needed.

## 3. Testing Strategy

- Benchmark before/after each PRAGMA change
- Use `criterion` for micro-benchmarks on insert/query paths
- Regression test: ensure no data loss after PRAGMA changes
- Test WAL checkpoint under concurrent read/write load

## 4. Performance Budget

| Operation | Current | Target |
|-----------|---------|--------|
| Single event insert | <1ms | <0.5ms |
| Batch insert (100) | <10ms | <5ms |
| Time-range query | <5ms | <2ms |
| WAL checkpoint | N/A | <50ms (passive) |

## 5. Risks

- `mmap_size` can cause issues on 32-bit systems (not applicable — 64-bit only)
- Aggressive `cache_size` may increase memory pressure on low-end devices
- WAL checkpoint during heavy writes could cause brief latency spikes (mitigated by PASSIVE mode)
