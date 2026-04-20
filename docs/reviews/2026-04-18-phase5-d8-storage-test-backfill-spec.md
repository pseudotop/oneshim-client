# Phase 5-D8: Storage Test Backfill — Design Spec

> **Status: SHIPPED.** Spec preserved as execution history. CoreError assertion examples below use pre-[ADR-019](../architecture/ADR-019-error-code-infrastructure.md) tuple-variant syntax (`matches!(err, Err(CoreError::Internal(_)))`); post-ADR-019 use struct-variant form `matches!(err, Err(CoreError::Internal { .. }))` or (preferred for wire-code-specific assertions) `err.code() == "internal.generic"`.

**Date**: 2026-04-18
**Target branch series**: `feat/phase5-d8-storage-tests` (spec) → 3 impl PRs
**Scope owner**: client-rust
**Origin gap**: `docs/reviews/2026-04-16-feature-gaps-analysis.md` — D8
**Status**: Design phase (pre-plan)

---

## 1. Context

The 2026-04-16 feature gap analysis flagged 6 storage modules in `crates/oneshim-storage/` as lacking inline tests. Post-Phase-3 (PR #436, merge commit `041eeb4c`) the storage surface grew by one more module (`regime_manager_state_store.rs`, already test-covered). The 6 original gaps remain:

| Module | LOC | Nature |
|---|---|---|
| `sqlite/coaching_storage_port_impl.rs` | 20 | Thin port impl — delegates to `coaching_storage.rs` |
| `sqlite/device_identity.rs` | 75 | Standalone device-ID utility |
| `sqlite/focus_storage_impl.rs` | 82 | Port impl with some logic |
| `sqlite/metrics.rs` | **670** | Full `MetricsStorage` implementation |
| `sqlite/session_context_store_impl.rs` | 11 | Thin port impl — delegates to existing internals |
| `sqlite/tags.rs` | 280 | Tags CRUD |

This spec defines the plan for closing that gap in ~2 calendar weeks across 3 PRs.

---

## 2. Scope (C-mixed approach)

**In scope:**
- Add inline `#[cfg(test)] mod tests` to every target module **where tests carry weight**.
- For thin delegators (`coaching_storage_port_impl`, `session_context_store_impl`, `focus_storage_impl`), first **audit** `sqlite/port_contract_tests.rs` to confirm existing contract coverage, then add only what the contract tests miss (usually a smoke test per port — "impl does not panic and propagates underlying storage behavior").
- For real-logic modules (`metrics.rs`, `tags.rs`, `device_identity.rs`), land full test suites per the D-done-criteria below.
- Promote `metrics.rs` to a directory module (`sqlite/metrics/mod.rs` + `sqlite/metrics/tests.rs`) as a minimal split — no functional concerns extracted.

**Out of scope:**
- Line-coverage instrumentation / CI coverage gates. Target is function+branch coverage, measured manually by function count.
- Port trait doc / Err-semantics additions — flagged as sub-tasks inside each PR if the existing port doc is silent, but not their own deliverable.
- Refactoring production code beyond the `metrics.rs` directory promotion.
- New production features / behavior changes — this is pure test backfill.
- `edge_intelligence/*.rs`, `vector_*_impl/*.rs`, `integration_state_store/*.rs`, `migration/v01..v18.rs` modules (already covered by sibling `tests.rs` files).
- `error.rs` and `lib.rs` (non-testable — pure re-exports / error type declarations).

---

## 3. Non-goals — what "done" is NOT

1. **100% line coverage.** We target function+branch coverage measured by "every `pub fn` / `pub async fn` has ≥ 1 happy-path test and every `Err` return path has ≥ 1 test." Not a `%` number.
2. **Property tests / fuzz.** Out of scope. If a specific invariant is naturally expressed as a property (e.g., "read-after-write roundtrip for any valid Tag"), a normal unit test with a small enumerated set is sufficient.
3. **Integration tests across crates.** Storage-only. No testing of `src-tauri` wiring, `oneshim-analysis` consumers, etc.
4. **Revalidation of already-tested port contracts.** `port_contract_tests.rs` already covers 3 `MetricsStorage` methods (`ms_save_and_get_metrics_roundtrip`, `ms_get_metrics_empty_range_returns_empty`, `ms_cleanup_old_metrics_returns_count` at `port_contract_tests.rs:232-264`) and 3 `FocusStorage` methods (`start_work_session`/`end_work_session`, `get_or_create_focus_metrics`, `increment_focus_metrics` at `port_contract_tests.rs:299-337`). PR1 and PR3 audits explicitly enumerate the delta — we add module-internal coverage for methods NOT already contract-tested, plus aggregation / edge-case scenarios. We do NOT re-test a method whose behaviour is already asserted at the port surface.
5. **Schema migration.** v31 stays as the current head. No test here adds a column or changes existing schema.

---

## 4. PR structure

Three PRs, sequential but branchable from `main` once PR1 lands:

### PR1 — `metrics.rs` (estimate: 6 working days + 1 day slack)

- **Task 0 (blocking) — Audit `port_contract_tests.rs` MetricsStorage coverage.** Produce a table mapping every `MetricsStorage` method (there are ~18 methods across the trait impl + helpers) to "contract-covered / uncovered / partial." The 3 known contract-covered methods (`save_metrics`, `get_metrics` / `get_metrics` empty-range variant, `cleanup_old_metrics`) get NO additional roundtrip test in PR1. The audit output is pasted into the PR body.
- **Refactor commit (1)**: `refactor(storage): promote metrics.rs to directory module`
  - `git mv crates/oneshim-storage/src/sqlite/metrics.rs crates/oneshim-storage/src/sqlite/metrics/mod.rs`
  - Create `crates/oneshim-storage/src/sqlite/metrics/tests.rs` (initially just `//! Inline unit tests — populated in subsequent commits.`)
  - Wire `mod tests;` at the bottom of `metrics/mod.rs` (gated on `#[cfg(test)]`)
  - `cargo check` + `cargo test -p oneshim-storage` verify zero behavior delta.
- **Test commits (bundled)**: add the module-internal test delta. Estimated 18–25 tests after subtracting the 3 contract-covered methods. Commits SHOULD be bundled in groups of 3+ to amortize lefthook clippy cost (~16 min cold per commit) — see Section 10 Schedule Risk.
- **Per-function target**: every `pub fn` / `pub async fn` in `MetricsStorage` impl + every `fn` private helper with a non-trivial branch, MINUS methods already asserted at the port surface per Task 0 audit.
- **Scenario seeds (confirmed from reading `metrics.rs` as of commit 041eeb4c):**
  - Bucketized aggregation via `aggregate_hourly_metrics` (confirmed at `metrics.rs:~213`) — hour-boundary aggregation, empty hour, multiple hours rolled.
  - TTL cutoff via `cleanup_old_metrics` (confirmed at `metrics.rs:~265`) — contract-covered for return count; module-internal test: assert that rows older than cutoff are actually deleted (not just counted).
  - NULL `NetworkInfo` handling (`NetworkInfo` is `Option<…>` in the type — confirmed in signature) — write + read a sample with `network = None`, assert no panic and column comes back NULL.
  - `system_metrics_hourly` table roundtrip (confirmed table exists) — scenario: raw write into `system_metrics`, run `aggregate_hourly_metrics`, assert hourly row.
  - UTC-midnight boundary for hourly aggregation if the bucket key is `DATE_TRUNC('hour')` or equivalent — verify in impl and write a single test at the boundary.
  - Bulk write + bulk read over 100+ samples — no pagination API expected; test is "all N rows round-trip in time-range query order."
  - Contract-covered scenarios (save/get/cleanup roundtrip + empty-range) are SKIPPED per Task 0.
- **Concurrency (2 lock-contract regression tests, not race tests)**: since `with_conn` serializes via `Arc<Mutex<Connection>>`, these tests assert the Mutex contract survives across threads — equivalent to "module works under `tokio::spawn`" not "module detects data races." Tests:
  1. Spawn 4 `tokio::task`s writing metrics concurrently (different sample inputs); after `join_all`, assert total row count = sum of inputs.
  2. Spawn 1 writer task + 1 reader task; after both complete, assert reader observed a consistent snapshot (either empty or full, never partial).
  Use `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]` for these two tests. Helper `open_db()` returns the same `Arc<Mutex<Connection>>` that is cloned into each task.
- **Err branches (reachability-aware, per Section 6 #2 + Section 7):**
  - Schema violation via direct `INSERT` of invalid data (CHECK-constraint path) — 1 test per distinct CHECK constraint in the module's tables.
  - Mutex poisoning — OPTIONAL: attempt one test using `tokio::task::spawn_blocking` + `panic!` inside a lock acquisition; if the test hangs or requires unsafe-feeling infrastructure, document as `// TODO: untested Err — mutex poisoning reachability` comment + PR body note, and move on. This is NOT a Done-criteria blocker.
  - Unreachable Err paths (closed connection, etc.) get a `// TODO: untested Err — <reason>` comment and PR body acknowledgment. This IS an accepted Done state.

### PR2 — `tags.rs` + `device_identity.rs` (estimate: 4 working days + 1 day slack)

**Grouping rationale:** each is too small to justify its own PR yet large enough to merit isolated review. `tags.rs` (280 LOC, 10 pub methods, schema-enforced `UNIQUE(name)` at `migration/v01_v08.rs:189`) and `device_identity.rs` (75 LOC, 2 pub methods, singleton-row semantics) share no testing pattern — they are bundled purely for PR-count economy.

- **`tags.rs`**: ~12–18 tests.
  - CRUD: create / list / `update_tag(name, color)` / delete happy paths (note: the public API is `update_tag`, not "rename" — matches `tags.rs` line inventory).
  - Constraints: duplicate name rejection (UNIQUE), case policy — verify actual behaviour during plan phase (likely case-sensitive per SQLite default), empty name rejection.
  - Linkage: tag-resource association (e.g., `frame_tags` join table) breaks correctly when tag deleted — verify schema + ON DELETE CASCADE during plan.
  - Concurrency (lock-contract regression, NOT race): 4 `std::thread::spawn` threads sharing the `Arc<SqliteStorage>` / `Arc<Mutex<Connection>>`, each calling `create_tag("same-name", ...)` without a barrier. After `join()` all: assert exactly 1 `Ok(..)` and 3 `Err(..)` with UNIQUE-constraint error. Uses sync threads (not `tokio::spawn`) because `create_tag` is sync. Test name: `concurrent_create_same_name_enforces_uniqueness`.
- **`device_identity.rs`**: ~6–10 tests.
  - First-create path (no existing row) writes + reads identical — covers the singleton-init branch.
  - Second-load path returns the same identity (persistence) — open DB, call `ensure_device_identity` twice, assert identity unchanged.
  - `reset_device_identity` produces a new identity that differs from the old one — exercises the reset branch.
  - Corruption handling: if the module has a defensive "row exists but is invalid" path, simulate via direct INSERT of malformed row. If no such path exists, document in PR body.
  - Determinism: identity generation is idempotent given same DB state (N calls to `ensure_device_identity` after init return the identical identity).

### PR3 — Delegator trio + `port_contract_tests.rs` audit (estimate: 3 working days + 1 day slack)

**Definition — "thin delegator":** a module whose every `pub fn` / `pub async fn` has a body consisting of a single delegation expression (optionally wrapped in `.map_err(Into::into)` or similar one-step conversion) to an underlying `SqliteStorage::<method>_sync` (or similar) implementation. Multi-line function signatures with many parameters still qualify if the body is structurally one delegation call — the "single delegation" criterion is semantic (what the function does), not syntactic (how many lines it occupies). The underlying implementations are covered by their own sibling tests OR by `port_contract_tests.rs`. Under this definition:
- `coaching_storage_port_impl.rs` (20 LOC, 2 forwards) — thin delegator ✓
- `session_context_store_impl.rs` (11 LOC, 1 forward) — thin delegator ✓
- `focus_storage_impl.rs` (82 LOC, 12 forwards — 9 truly 1-line, 3 multi-line due to parameter count but single-delegation bodies) — thin delegator ✓ (verify structurally during plan)

**Done criteria variant (thin delegators only):** Section 6 Done #1 is satisfied by ONE combined smoke test per delegator that invokes every `pub fn`/`pub async fn` of the port in sequence and asserts each returns `Ok(_)` + at least one side effect is observable via a direct SQL read. Per-method happy-path tests are NOT required for thin delegators. The underlying `_sync` implementations' coverage is assessed by the audit (below).

- **Task 0 (blocking) — Port coverage audit.** Read `sqlite/port_contract_tests.rs` (337 LOC, 22 tests) and produce an audit table:
  - `CoachingStoragePort` — which of the 2 methods covered? Partial coverage means "some methods have port-level tests; module-internal gap for others."
  - `SessionContextStorePort` — which of the 1 method covered?
  - `FocusStorage` — known partial: 3 of 12 methods covered per reviewer finding (`start_work_session`/`end_work_session`, `get_or_create_focus_metrics`, `increment_focus_metrics`). The remaining 9 methods lack port-surface coverage.
- **Mapping audit output to work:**
  - **Delegator-level (always):** 1 combined smoke test per delegator per the Done-criteria variant above — 3 smoke tests total, one per port, regardless of audit result.
  - **Underlying-impl gaps (FocusStorage the current suspect):** if the audit shows an underlying `SqliteStorage::_sync` method has neither port-contract tests nor sibling tests.rs coverage, ADD tests for it in PR3 under `sqlite/tests.rs` (or a new dedicated file if the count justifies it). Scope-control: cap underlying-impl additions at 10 tests total for PR3. If the audit reveals > 10 method gaps, list the overflow as follow-up in Section 11 — do NOT expand PR3 beyond the 3-day + 1-day-slack budget.
- Expected test count: **3 delegator smoke tests + 0–10 underlying-impl gap tests = 3–13 tests.**
- **PR3 body MUST include the audit table** so future reviewers see the coverage rationale + any deferred gaps.

---

## 5. Test harness conventions

**Actual state of harness code as of commit `041eeb4c` (verify by reading):**
- `crates/oneshim-storage/src/sqlite/test_utils.rs` is 13 lines — exposes only `pub(crate) fn make_user_event() -> Event`. **It does NOT contain a DB factory, `open_db`, or a `SqliteStorage` constructor.** The spec's earlier draft incorrectly implied it did.
- The actual precedent harness functions live test-mod-local in their respective files:
  - `crates/oneshim-storage/src/regime_manager_state_store.rs:~106 fn open_db() -> (TempDir, Arc<Mutex<Connection>>)`
  - `crates/oneshim-storage/src/sqlite/vector_store_impl/tests.rs:~11 fn setup_db() -> …`
  - `crates/oneshim-storage/src/sqlite/port_contract_tests.rs:~20 fn storage() -> …` (uses `SqliteStorage::open_in_memory(30)`)

**Convention for Phase 5-D8: copy-local, do NOT centralize.** Each new test module adds its own `open_db()` (or `setup_db()`, `storage()`) helper local to the `#[cfg(test)] mod tests { }` block, following the regime-manager-state-store precedent. This keeps PR diffs self-contained and avoids any "promote to `test_utils.rs`" refactor creeping into the middle of a test-backfill PR.

```rust
// Local to each test module — copy verbatim.
fn open_db() -> (TempDir, Arc<Mutex<Connection>>) {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(dir.path().join("t.db")).unwrap();
    crate::migration::run_migrations(&conn).unwrap();
    (dir, Arc::new(Mutex::new(conn)))
}
```

**If a test needs `SqliteStorage` (the full struct, not a raw Connection)**, use `SqliteStorage::open_in_memory(30)` as `port_contract_tests.rs` does. **If a test needs both `SqliteStorage` (for port method calls) AND `Arc<Mutex<Connection>>` (for concurrent-thread cloning in lock-contract tests)**, call `SqliteStorage::open_in_memory(30)` first, then obtain the `Arc<Mutex<Connection>>` via `storage.connection_arc()` — this is the bridge used by `port_contract_tests.rs:~271`. If neither raw Connection nor `open_in_memory` is sufficient (e.g., a test needs a specific on-disk path), derive the helper inline in the test module.

**Schema migration cost per test:** each `open_db()` / `open_in_memory(30)` call runs the full v1→v31 migration chain. This is idempotent and fast (existing 22 contract tests do the same with no measurable suite-runtime impact). 40+ additional tests are not expected to materially change suite runtime; if `cargo test -p oneshim-storage` suite time doubles after PR1, consider a `once_cell`-backed shared-DB harness as a follow-up (captured in Section 11).

**Promoting `open_db` and `sample_X` builders into `test_utils.rs` is OUT OF SCOPE for Phase 5-D8.** It is captured as a follow-up in Section 11. Doing it mid-phase adds refactor scope to every PR and breaks the "test-only" contract of this phase.

- `TempDir` cleanup on drop; no explicit teardown.
- No global fixtures (async or otherwise). Module-local `fn sample_foo(id: &str) -> Foo { ... }` builders per the `regime_manager_state_store::tests::sample_regime` precedent.
- `#[tokio::test]` for async port methods; `#[test]` for sync helpers.
- **`#[tokio::test(flavor = "multi_thread", worker_threads = N)]`** for concurrency lock-contract regression tests (PR1 metrics, PR2 tags). Default single-thread flavor is fine for all other tests.

---

## 6. Done criteria (D — per-function + concurrency, with explicit carve-outs)

For each target module, "done" means **all six** hold:

1. **Happy-path coverage — two variants by module type:**
   - **Real-logic modules (`metrics.rs`, `tags.rs`, `device_identity.rs`):** every `pub fn` / `pub async fn` in the module has at least one happy-path test, MINUS any method whose behaviour is already asserted at the port surface in `port_contract_tests.rs` (documented via the Task 0 audit in PR1/PR3). The reduced set is "uncovered at the port surface."
   - **Thin delegators (`coaching_storage_port_impl.rs`, `session_context_store_impl.rs`, `focus_storage_impl.rs` — each a 1-line forward to an underlying `_sync` impl):** one combined smoke test per delegator module that invokes every `pub fn` / `pub async fn` of the port and asserts `Ok(_)` + at least one observable side effect. Per-method individual tests are NOT required for thin delegators.
2. **Reachable Err paths covered.** Every `Err` return path reachable via the techniques in Section 7 (CHECK violation via raw INSERT, invalid payload injection, mutex poisoning where tractable) has at least one test asserting the variant (e.g., `matches!(result, Err(CoreError::Internal(_)))`) AND, where the error message is stable, asserting a substring. **Unreachable Err paths** (typically closed-connection branches under `std::sync::Mutex`, or patterns requiring unsafe-feeling test infra) are documented as `// TODO: untested Err — <reason>` inline comments + a bullet in the PR body. Unreachability does NOT block Done.
3. **Lock-contract regression tests** for `metrics.rs` and `tags.rs` per the specific test designs in Section 4 PR1 / PR2. These are NOT race-detection tests — they assert the module behaves correctly when invoked from multiple threads sharing the `Arc<Mutex<Connection>>`.
4. **Workspace green.** `cargo test -p oneshim-storage` passes. `cargo clippy --workspace --all-targets` produces zero warnings. `cargo fmt --check` clean.
5. **Test module organization:** following `port_contract_tests.rs` layout precedent.
   - Helpers / sample builders first.
   - Optional `// ── group name ──` separators grouping tests by area (e.g., `// ── happy path ──`, `// ── Err branches ──`, `// ── lock-contract ──`).
   - Within each group: `#[tokio::test]` async tests, then `#[test]` sync tests, then `#[tokio::test(flavor = "multi_thread", ...)]` concurrency tests.
   - Test name convention: `<subject>_<expected_behaviour>` (e.g., `save_metrics_roundtrip`, `save_metrics_rejects_negative_cpu`).
6. **Flaky-test policy.** The lock-contract regression tests at #3 must be deterministic — use explicit `join_all` / thread `join()` barriers, NOT `thread::sleep` or wall-clock timing. If a lock-contract test flakes during PR review, it is rejected — no `#[ignore]` escape hatch. Statistical assertions (N runs) are not permitted.

---

## 7. Err-branch & concurrency patterns

**Err triggers, in order of preference — use only what actually reaches an `Err` branch in the target module:**

- **CHECK / UNIQUE constraint violation** — PREFERRED when applicable. Force-insert values that violate existing table constraints (e.g., negative count where `CHECK count >= 0`, duplicate key where `UNIQUE(name)` applies) using `conn.execute(..)` directly. Then call the port method and assert the variant. This is the simplest, most deterministic Err-trigger and should be the default.
- **Invalid JSON / payload in a TEXT column** — WHEN the module deserialises a persisted JSON blob. Direct `INSERT` via raw Connection to seed the bad row, then call the port method that reads it. Precedent: `regime_manager_state_store::tests::malformed_payload_quarantines_and_starts_fresh` at `regime_manager_state_store.rs:~165`.
- **Mutex poisoning** — OPTIONAL, attempt ONCE per module then escape if hard. Worked example:
  ```rust
  // Inside a test, after `open_db()` yields conn: Arc<Mutex<Connection>>.
  // Poison by panicking while holding the lock in a separate thread.
  let c = conn.clone();
  let _ = std::thread::spawn(move || {
      let _guard = c.lock().unwrap();
      panic!("intentional panic to poison");
  }).join(); // join() returns Err from the panicked thread; that's expected.
  // The lock is now poisoned. Calling the port method should propagate
  // the poison as CoreError::Internal(_).
  let result = storage.some_port_method(...).await;
  // ASSERTION NOTE: different call sites emit different poison-error
  // strings. `with_conn` (sqlite/mod.rs ~line 143) emits "SQLite lock
  // poisoned: {e}"; direct `conn.lock()` paths in metrics.rs emit
  // "Failed to acquire lock: {e}". Match on the variant only, not a
  // substring, to keep the test robust across call sites:
  assert!(matches!(result, Err(CoreError::Internal(_))));
  ```
  **No prior test in `oneshim-storage` uses this technique.** Expect the first attempt to take 1–2 hours. Prefer picking a `with_conn`-backed method (e.g., `save_metrics`) over direct-lock paths for the first attempt — behaviour is more uniformly documented. If the port method does not translate poison into `CoreError::Internal` (or if the test hangs), downgrade to "unreachable" per the escape hatch below.
- **Unreachable paths (escape hatch, permitted):** closed-connection branches, paths requiring `drop(Arc<Mutex<Connection>>)` without unsafe, and similar are NOT attempted. Document them as:
  ```rust
  // TODO: untested Err — closed-connection path unreachable under Arc<Mutex>.
  // SqliteStorage holds one Arc reference for its lifetime; we cannot force
  // connection closure from a test without unsafe code.
  ```
  Plus one bullet in the PR body listing which Err branch was skipped and why. This satisfies Section 6 Done #2.

**Concurrency assertions** (for `metrics.rs`, `tags.rs`):
- Do not race for race-detection — `Arc<Mutex>` already serializes. Race tests assert **final state invariants**: total count, uniqueness, no torn writes.
- Thread count: 4 or 8 depending on the invariant being tested.
- Each thread runs `spawn(move || { let c = conn.clone(); /* run port op */ })`.
- Join all handles, then make assertions.
- Name the test with the invariant: `concurrent_create_preserves_uniqueness`, `parallel_writes_preserve_total_count`, `reader_during_writer_sees_consistent_snapshot`.

---

## 8. ADR / Architecture compliance

| ADR / guardrail | Compliance |
|---|---|
| ADR-001 Hexagonal | Tests live in adapter crate; ports in `oneshim-core` untouched; no cross-adapter imports. |
| ADR-003 Directory Module | `metrics.rs` is promoted to a directory module (`sqlite/metrics/{mod.rs, tests.rs}`) — minimal split, matching `vector_store_impl/` precedent. Remaining modules stay inline (feedback_file_split_policy: 500-line split is over-engineering). |
| ADR-017 FeedbackSignalSink | Not touched. |
| ADR-018 RegimeManager persistence | Not touched. `run_migrations` is called in test setup; v31 is the head. |
| ADR-007 `parking_lot::Mutex` never across `.await` | Tests use `std::sync::Mutex<Connection>` only (rusqlite constraint); no lock-across-await risk. |
| Concurrency guardrail ("bounded collections") | N/A for tests. |
| Port instance sharing guardrail | N/A for tests. |

**No ADR revisions required.** No new ADR needed — this is adapter-crate work within existing patterns.

---

## 9. Review discipline

Each PR follows the **3-loop + 4th-pass fresh-eyes** pattern established in Phase 3 (ref: `feedback_3loop_quality_gate.md` + `feedback_multi_pass_review.md`):

1. **Spec** — this doc + per-PR-specific acceptance notes if needed.
2. **Plan** — task list per PR (expected 5–15 tasks; smaller than Phase 3's 13 because scope is narrower). Written during the first PR's setup; PR2 / PR3 get their own smaller plans or extend this doc.
3. **Impl** — commits per task, tests green per task.
4. **4th-pass deep review** — fresh-eyes reviewer on full diff before merge. Critical must be fixed; Important must be fixed or explicitly accepted in-PR with reasoning; Minor follow-up OK.

Lefthook cost (`feedback_lefthook_clippy_cost.md`): ~16 min cold clippy hook. Bundle commits to avoid 6+ cold runs per PR. Spec → draft → batch-commit model preferred over incremental push.

---

## 10. Timeline & acceptance

| PR | Target duration | Slack | Hard cap |
|---|---|---|---|
| PR1 `metrics.rs` | 6 working days | +1 day | 7 days |
| PR2 `tags.rs` + `device_identity.rs` | 4 working days | +1 day | 5 days |
| PR3 delegator trio + audit | 3 working days | +1 day | 4 days |

Total: 13 working days nominal, 16 working days hard cap (~3 calendar weeks worst case).

### 10.1 Schedule risk

- **Lefthook clippy cost** (`feedback_lefthook_clippy_cost.md`): pre-commit hook runs `cargo clippy --workspace` in ~16 min cold / ~1s warm. PR1's test-driven workflow naturally produces 5–10 distinct commits (refactor, Task 0 audit notes, multiple test batches, Err-branch additions, concurrency tests). Mitigation: **bundle test commits in groups of 3+ before pushing** to amortize the hook cost. A worst-case 3 cold runs × 16 min ≈ 48 min/day overhead was factored into the 1-day slack per PR.
- **Task 0 audit in PR1** may reveal more than 3 `MetricsStorage` methods already contract-covered — good (reduces test count) but also may reveal port-contract gaps in methods we assumed were uncovered. If the audit reveals scope drift > 20% of the 18–25 test estimate, PR1 absorbs it into the slack day; beyond that it becomes a plan revision.
- **PR3 underlying-impl overflow cap**: if the FocusStorage audit reveals > 10 uncovered `_sync` methods, overflow is deferred to a follow-up issue. Hard cap of 10 new tests in `sqlite/tests.rs` under PR3.
- **Mutex-poisoning tractability (Section 7)**: first attempt budgeted at 1–2 hours; failure → downgrade to unreachable, no budget impact.
- **Review turnaround**: 3-loop + 4th-pass deep review per PR adds 0.5–1 calendar day per PR beyond implementation. Calendar-week estimate (~3 weeks hard-cap) assumes review is not self-blocking.
- **Bug-discovery policy during test authoring.** If a new test uncovers a latent bug in the module under test (e.g., off-by-one in bucket boundary, incorrect NULL handling):
  - **Fix ≤ 20 LOC in-PR.** Test lands green alongside the 1-commit fix. PR title/body explicitly calls out the bugfix.
  - **Fix > 20 LOC → separate bugfix PR.** The failing test lands in D8 marked `#[ignore = "blocked by bugfix PR #NNN"]` with a link; the bugfix PR removes the `#[ignore]` when it lands. D8 PR does NOT wait for the bugfix PR to merge first.
  - This prevents D8 scope from bleeding into production refactors but also prevents D8 from rubber-stamping known bugs.

**Phase 5-D8 is complete when:**
- All 3 PRs merged to `main`.
- `cargo test -p oneshim-storage` green on `main`.
- The `project_feature_gaps_next_session.md` D8 row is marked ✅ with linked PRs.
- `docs/STATUS.md` test counts are updated. Expected Rust test delta range:
  - **Low bound ~39 tests** (PR1: 18 after contract-coverage subtraction + PR2: 18 tags-min + device-min + PR3: 3 delegator smoke + 0 underlying-impl gaps).
  - **High bound ~66 tests** (PR1: 25 + PR2: 28 tags-max + device-max + PR3: 3 + 10 capped underlying-impl gaps).
  - If PR1's Task 0 audit reveals substantially more MetricsStorage overlap than currently known (> 6 methods), PR1's lower bound drops below 18 — but overall phase low bound stays above 30 tests via PR2 + PR3.

---

## 11. Follow-ups / Out-of-scope (captured for later)

- **Line-coverage instrumentation** (tarpaulin / llvm-cov) + CI gate. Requires infra work; not worth the friction for a 2-week effort.
- **Functional concern split of `metrics.rs`** (`metrics/write.rs`, `metrics/query.rs`, `metrics/aggregation.rs`) — only if the module grows further or SOLID issues emerge. Current split is `mod.rs` + `tests.rs` only.
- **Extending `port_contract_tests.rs`** to exhaustively cover every port — a separate initiative; D8 only audits, doesn't extend.
- **Promoting the shared `open_db()` / `sample_X` harness builders into `test_utils.rs`** as `pub(crate)` items — explicitly deferred per Section 5. Do this as a follow-up PR after all 3 D8 PRs merge; scope: ~4 helper functions, 1 commit, no behaviour change.
- **Fixing stale `CLAUDE.md` schema version text** — the worktree CLAUDE.md currently lists schema `V1-V22` under `oneshim-storage`; real head is v31. Out-of-scope for D8 but worth a 1-line follow-up alongside the next phase's CLAUDE.md update.
- **PR3 underlying-impl overflow**: any FocusStorage (or other port) `_sync` methods that remain uncovered after PR3's 10-test cap is spent are deferred to a follow-up issue, tracked in `project_feature_gaps_next_session.md`.

---

## 12. References

- Gap analysis: [`docs/reviews/2026-04-16-feature-gaps-analysis.md`](2026-04-16-feature-gaps-analysis.md) (D8 row).
- Phase 3 spec / plan (for review discipline precedent): [`2026-04-18-phase3-regime-feedback-learning-spec.md`](2026-04-18-phase3-regime-feedback-learning-spec.md), [`2026-04-18-phase3-regime-feedback-learning-plan.md`](2026-04-18-phase3-regime-feedback-learning-plan.md).
- Existing test harness precedent: `crates/oneshim-storage/src/regime_manager_state_store.rs` (inline), `crates/oneshim-storage/src/sqlite/vector_store_impl/tests.rs` (directory-module).
- File-split policy: `feedback_file_split_policy.md` (memory).
- ADR-003: [`docs/architecture/ADR-003-directory-module-pattern.md`](../architecture/ADR-003-directory-module-pattern.md).
