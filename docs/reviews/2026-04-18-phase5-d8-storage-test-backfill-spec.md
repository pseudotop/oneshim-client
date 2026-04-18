# Phase 5-D8: Storage Test Backfill — Design Spec

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
4. **Revalidation of already-tested ports.** `MetricsStorage` might be touched by `port_contract_tests.rs` — if so, we add the metrics-specific internal tests (time-window math, aggregation edge cases) but do NOT re-test the port surface.
5. **Schema migration.** v31 stays as the current head. No test here adds a column or changes existing schema.

---

## 4. PR structure

Three PRs, sequential but branchable from `main` once PR1 lands:

### PR1 — `metrics.rs` (estimate: 6 working days)

- **Refactor commit (1)**: `refactor(storage): promote metrics.rs to directory module`
  - `git mv crates/oneshim-storage/src/sqlite/metrics.rs crates/oneshim-storage/src/sqlite/metrics/mod.rs`
  - Create empty `crates/oneshim-storage/src/sqlite/metrics/tests.rs`
  - Wire `mod tests;` at the bottom of `metrics/mod.rs` (gated on `#[cfg(test)]`)
  - `cargo check` verifies zero behavior delta.
- **Test commits (multiple)**: add 25–35 tests covering the below.
- **Per-function target**: every `pub fn` / `pub async fn` in `MetricsStorage` impl + every `fn` private helper with a non-trivial branch.
- **Scenario seeds** (subject to confirmation against the actual `metrics.rs` API during the plan phase):
  - metric write/read roundtrip;
  - time-window aggregation (5 min / 1 h / 1 day bucket boundaries) — only if the production code bucketizes; skip if metrics are stored raw;
  - empty window (query returns zero rows, does not error);
  - UTC-midnight boundary if timestamps are bucketized by calendar day (verify `chrono` usage during plan);
  - bulk write + bulk read over 100+ samples (no pagination API assumed — just assert all N rows round-trip);
  - NULL CPU / Memory handling if columns are nullable;
  - retention cutoff / TTL behavior if the module implements it (verify during plan).
- **Concurrency**: 2 tests — parallel writes preserving count; writer + reader yielding consistent snapshot.
- **Err branches**: every `CoreError::Internal(...)` return exercised (e.g., poisoned mutex via forced panic on another thread, schema violation via direct INSERT of invalid data).

### PR2 — `tags.rs` + `device_identity.rs` (estimate: 4 working days)

- **`tags.rs`**: ~12–18 tests.
  - CRUD: create / list / rename / delete happy paths.
  - Constraints: duplicate name rejection (unique), case policy if present, empty name rejection.
  - Linkage: tag-resource association breaks correctly when tag deleted (if the schema has FK or explicit cleanup).
  - Concurrency: 4 threads racing to create same name — exactly 1 succeeds, 3 get Err.
- **`device_identity.rs`**: ~6–10 tests.
  - First-create path (no existing row) writes + reads identical.
  - Second-load path returns the same identity (persistence).
  - Corruption handling (if any defensive path) — simulate invalid row.
  - Determinism: identity generation is idempotent given same DB state.

### PR3 — Delegator trio + `port_contract_tests.rs` audit (estimate: 3 working days)

- **Blocking sub-task**: read `sqlite/port_contract_tests.rs` (337 LOC, 22 tests) and produce an audit table mapping each of `CoachingStoragePort`, `SessionContextStorePort`, `FocusStorage` to "covered / partial / uncovered."
- **If covered**: add no test for that delegator. Record the audit finding as a comment at the top of the delegator file and in the PR body.
- **If uncovered or partial**: add one smoke test per delegator. "Smoke test" here means a **single happy-path test** that (a) calls each `pub async fn` of the port once with a representative input, (b) asserts `Ok(_)`, (c) asserts the observable side effect via a separate read path (direct SQL query through the `open_db()` Connection, OR a sibling port method that has existing test coverage). No exhaustive contract duplication, no edge-case coverage — that belongs in `port_contract_tests.rs` as a separate initiative.
- Expected test count: 1–4 tests total for PR3 depending on audit outcome.
- **PR3 body must include the audit table** so future reviewers see why the delegators got minimal coverage.

---

## 5. Test harness conventions

Reuse the pattern established in `regime_manager_state_store::tests` and `vector_store_impl/tests.rs`:

```rust
fn open_db() -> (TempDir, Arc<Mutex<Connection>>) {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(dir.path().join("t.db")).unwrap();
    crate::migration::run_migrations(&conn).unwrap();
    (dir, Arc::new(Mutex::new(conn)))
}
```

- `TempDir` cleanup on drop; no explicit teardown.
- If a test needs `SqliteStorage` (the full struct) rather than a raw Connection, use the existing constructor from `test_utils.rs`. If `test_utils.rs` does not yet expose what a test needs, add the helper **in that test's PR** rather than leaving the test awkward.
- No global fixtures. Module-local `fn sample_foo(id: &str) -> Foo { ... }` builders as in `regime_manager_state_store::tests::sample_regime`.
- `#[tokio::test]` for async port methods; `#[test]` for sync helpers.

---

## 6. Done criteria (D — per-function + concurrency)

For each target module, "done" means **all five** hold:

1. Every `pub fn` / `pub async fn` in the module has at least one happy-path test.
2. Every `Err` return path in the impl has at least one test asserting the variant (e.g., `matches!(result, Err(CoreError::Internal(_)))`) AND, where message content is stable, asserting a substring.
3. `metrics.rs`, `tags.rs`: at least one concurrency-contract test per the patterns in section 3 of the brainstorm (barrier-free naive race, assert final-state invariant).
4. `cargo test -p oneshim-storage` passes green. `cargo clippy --workspace --all-targets` produces zero warnings. `cargo fmt --check` clean.
5. Test module order follows existing convention: helpers first, then happy-path tests (`#[tokio::test]` group), then edge/error-path tests, then concurrency tests.

---

## 7. Err-branch & concurrency patterns

**Err triggers** (pick the minimal set that actually exercises each `Err` branch in the module; don't force every pattern):
- **Invalid JSON / payload in a TEXT column**: direct `INSERT` via raw Connection to seed the bad row, then call the port method that reads it. (Used by `regime_manager_state_store::tests::malformed_payload_quarantines_and_starts_fresh` precedent.)
- **CHECK constraint violation**: force-insert values that violate existing table constraints (e.g., negative count where `CHECK count >= 0`) to exercise the `rusqlite` error path.
- **Mutex poisoning**: `std::thread::spawn` + `panic!` while holding the Connection lock, then call the port method and assert `Err(CoreError::Internal(msg))` where `msg` contains `"poisoned"`.
- **Not attempted**: dropping or closing the Connection mid-op. `rusqlite` holds the Connection inside the `std::sync::Mutex`; we don't have a clean way to simulate a closed handle without unsafe code. If a module has a closed-connection `Err` branch that isn't reachable via mutex poisoning, document the gap in the PR body and move on.

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

| PR | Target start | Target end | Duration |
|---|---|---|---|
| PR1 `metrics.rs` | Day 1 | Day 6 | ~6d |
| PR2 `tags.rs` + `device_identity.rs` | Day 7 | Day 10 | ~4d |
| PR3 delegator trio + audit | Day 11 | Day 13 | ~3d |

Total: ~13 working days (~2 calendar weeks w/ review turnaround).

**Phase 5-D8 is complete when:**
- All 3 PRs merged to `main`.
- `cargo test -p oneshim-storage` green on `main`.
- The `project_feature_gaps_next_session.md` D8 row is marked ✅ with linked PRs.
- `docs/STATUS.md` test counts are updated (Rust test delta should be ~40–60 additional tests).

---

## 11. Follow-ups / Out-of-scope (captured for later)

- **Line-coverage instrumentation** (tarpaulin / llvm-cov) + CI gate. Requires infra work; not worth the friction for a 2-week effort.
- **Functional concern split of `metrics.rs`** (`metrics/write.rs`, `metrics/query.rs`, `metrics/aggregation.rs`) — only if the module grows further or SOLID issues emerge. Current split is `mod.rs` + `tests.rs` only.
- **Extending `port_contract_tests.rs`** to exhaustively cover every port — a separate initiative; D8 only audits, doesn't extend.
- **Unifying `test_utils.rs` harness** across the crate — if any PR adds a one-off helper, consider promoting to `test_utils.rs` in the final polish PR of Phase 5.

---

## 12. References

- Gap analysis: [`docs/reviews/2026-04-16-feature-gaps-analysis.md`](2026-04-16-feature-gaps-analysis.md) (D8 row).
- Phase 3 spec / plan (for review discipline precedent): [`2026-04-18-phase3-regime-feedback-learning-spec.md`](2026-04-18-phase3-regime-feedback-learning-spec.md), [`2026-04-18-phase3-regime-feedback-learning-plan.md`](2026-04-18-phase3-regime-feedback-learning-plan.md).
- Existing test harness precedent: `crates/oneshim-storage/src/regime_manager_state_store.rs` (inline), `crates/oneshim-storage/src/sqlite/vector_store_impl/tests.rs` (directory-module).
- File-split policy: `feedback_file_split_policy.md` (memory).
- ADR-003: [`docs/architecture/ADR-003-directory-module-pattern.md`](../architecture/ADR-003-directory-module-pattern.md).
