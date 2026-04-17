# Phase 3 — Ralph loop progress

**Task**: X3 FeedbackSignalSink + C3a regime_id vector filter + C3c/X6 RegimeManager persistence.

**Branch**: `feat/phase3-regime-feedback-learning` (off `origin/main` @ 13ec2949).

**3-loop structure**:
1. Spec → deep review until no major issues
2. Plan → deep review until no Critical/Important issues
3. Implementation → deep review until none

## Loop status

| Loop | Status | Commit |
|------|--------|--------|
| 1. Spec v1 | done @ iter 1 | c4996a2d |
| 1. Spec v1 → v2 deep review | done @ iter 2 | 9e47f63e |
| 1. Spec fresh-eyes re-review | **next iter** | — |

## Iter-2 fixes applied

| # | Severity | Issue | Fix |
|---|----------|-------|-----|
| H | CRIT | Spec §4.6 pointed shutdown save at `lifecycle.rs` which has no hook registry — `std::process::exit(0)` after a 3s grace; save_all had no reachable call path | Rewritten to use the real shutdown orchestrator in `main.rs` RunEvent::Exit handler (line 335+). Uses `runtime_handle.block_on` + `tokio::time::timeout(4s)`. Stored via new Option fields on Tauri `State`. |
| D | CRIT | Parse-failure "log and wipe" would silently lose user-curated regime names on schema mismatch | Added `payload_backup` + `payload_backup_at` columns. On parse error, copy bad payload to backup column, log `error!`, return `Ok(vec![])`. Recovery via backup. T-C3c-4 rewritten accordingly. |
| C | IMP | Watchdog 3s arbitrarily tighter than telemetry 4s | Harmonised at 4s. SQLite contention justification. |
| F | IMP | T-C3a-1 perf budget in PR tests is CI-flaky | Dropped from PR. Moved to `benches/regime_filter_bench.rs` for manual RC validation. |
| G | IMP | `FeedbackSignalSink::record_user_reaction -> Result<(), CoreError>` return type semantics ambiguous | Narrowed: `Err` reserved for programmer bugs only; network/DB/transient failures are impl-internal, never escalated. Added explicit **~10ms latency budget** + `tokio::spawn` offload rule. ADR-017 cross-reference. |
| B | MIN | Manual restart-verify line in §8 duplicates T-C3c-6 | Removed from §8. |
| ADR# | MIN | Migration version was "N + 1" placeholder | Pinned to **v31** (confirmed CURRENT_VERSION=30). |

## Non-issues confirmed by reviewers

- `embedding_vectors.segment_id` text-to-text join to `activity_segments.id` works (no FK, but `idx_embedding_segment` + `idx_segments_regime` support the subquery).
- `CoachingEngine::evaluate` uses `&self` today — `record_user_reaction(&self)` signature consistent.
- `RegimeManager.regimes` is private — no bypass path; `hydrate_from(&mut self)` is safe.
- `regime_manager_state` table name clear — no conflict with sync_merger.
- `CompositeFeedbackSink` placement in src-tauri respects hexagonal boundary (src-tauri already depends on oneshim-analysis).
- `FeedbackSender::new` preservation — 3 test callers, all backward-compatible via `new_with_sink(api, None)` shim.

## Spec shape (v2, 502 lines)

Sections: Motivation → X3 design → C3a design → C3c/X6 design (with quarantine) → cross-item interaction → docs → rollout (7 commits) → acceptance → risks → out-of-scope.

## Next iteration

Fresh-eyes re-review confirming v2 fixes didn't introduce new issues. If clean → Loop 2 (plan via `superpowers:writing-plans`).

## Iter-3 fresh-eyes fixes applied

| # | Severity | Issue | Fix |
|---|----------|-------|-----|
| 1 | IMP | Commit 5 would have failed `cargo test` green rule — read AppState fields populated only in Commit 7 | §7 rewritten: Commit 5 adds field DECLARATIONS (None) + dormant save guard + hydrate_from; Commit 7 populates fields + lands T-C3c-6/7. |
| 2 | IMP | `RegimeStoragePort::load_all` trait doc silently permitted writes (quarantine) | §2.1/§4.1 trait doc now explicitly notes `load_all` MAY perform corrective writes; callers must treat as single-shot-at-startup. |

## Spec v3 artefact
- Path: `docs/reviews/2026-04-18-phase3-regime-feedback-learning-spec.md`
- Commit: `f925538d`
- Size: 516 lines

## Loop 2 — plan v1
- Path: `docs/reviews/2026-04-18-phase3-regime-feedback-learning-plan.md`
- Commit: `d50774c9`
- 13 tasks + 1650 lines

## Loop 3 — implementation progress

| Task | Status | Commit |
|------|--------|--------|
| 1. FeedbackSignalSink port | ✅ done | `0fb45b6a` |
| 2. record_user_reaction stubs | ✅ done | `a336ba68` |
| 3. FeedbackSender::new_with_sink + T-X3-4 ordering test | ✅ done | `df19afc2` |
| 4. CompositeFeedbackSink + T-X3-1/2/5 tests | ✅ done | `51a2f615` |
| 5. ADR-017 (EN+KO) | ⏳ in progress | — |
| 6. C3a regime_id vector filter subquery + T-C3a-1..4 | — | — |
| 7. RegimeStoragePort | — | — |
| 8. v31 migration regime_manager_state | — | — |
| 9. SqliteRegimeManagerStateStore + T-C3c-1..4 | — | — |
| 10. RegimeManager::hydrate_from + T-C3c-5 | — | — |
| 11. AppState fields + dormant save guard | — | — |
| 5. ADR-017 (EN+KO) | ✅ done | `b7a6e909` |
| 6. C3a regime_id vector filter subquery + T-C3a-1..4 | ✅ done | `eeea83f2` |
| 7. RegimeStoragePort | ✅ done | `bdfa1bf8` |
| 8. v31 migration regime_manager_state | ✅ done | `e573f8cc` |
| 9. SqliteRegimeManagerStateStore + T-C3c-1..4 | ✅ done | `0a539bdb` |
| 10. RegimeManager::hydrate_from + T-C3c-5 | ✅ done | `ea5129e6` |
| 11. AppState fields + dormant save guard | ✅ done | `6bdfdc72` |
| 12. ADR-018 (EN+KO) | ✅ done | `ef140520` |
| 13a. T-C3c-6/7 storage tests | ✅ done | `8aa4ef26` |
| 13b. Composition-root wiring (hydrate + CompositeFeedbackSink) | ✅ done | `d8c345fa` + `f1385943` |

All tests green per task. Lefthook cargo-fmt + cargo-clippy pass each commit.
Workspace: **3,401 tests passing, 0 failed**. Clippy clean.

## Phase 3 — shipped end-to-end

All 13 tasks complete. Acceptance criteria:
- ✅ `FeedbackSignalSink` port trait with ~10ms latency budget + Err-for-
  programmer-bugs-only semantics (ADR-017).
- ✅ `CompositeFeedbackSink` fans accept/reject to `Arc<CoachingEngine>`
  + `Arc<Mutex<RegimeClassifier>>`; sink fires BEFORE server call in
  `FeedbackSender::send_feedback` (T-X3-4 ordering).
- ✅ C3a vector filter: `search_filtered`/`search_quantized` honour
  `regime_id` via subquery over `activity_segments.regime_id` using
  existing `idx_segments_regime` index.
- ✅ `RegimeStoragePort` + `SqliteRegimeManagerStateStore` with v31
  migration. Parse-failure quarantines to `payload_backup` column
  rather than silently wiping.
- ✅ `RegimeManager::hydrate_from` + AppState dormant fields + 4s
  shutdown save guard via the Phase-2 std::thread::spawn watchdog
  pattern.
- ✅ Composition-root wiring in `app_runtime_launch.rs` — shares the
  same `Arc<parking_lot::Mutex<_>>` across scheduler + AppState +
  feedback sink. "Regimes survive restart" is end-to-end functional.

## Tests landed (15)

- **T-X3-1/2/5** (`src-tauri/src/feedback_sink/mod.rs`) — accept/reject/defer
  each land once, no-consumer still Ok, both consumers reached
- **T-X3-3** — existing `feedback::tests::{accept,reject,defer}_feedback`
  serve as regression guard (new-shim path)
- **T-X3-4** (`crates/oneshim-suggestion/src/feedback.rs`) —
  `sink_fires_before_api_client` ordering test
- **T-C3a-1..4** (`crates/oneshim-storage/src/sqlite/vector_store_impl/tests.rs`)
  — regime scoping, quantized path, None passthrough, NULL regime exclusion
- **T-C3c-1..4** (`crates/oneshim-storage/src/regime_manager_state_store.rs`)
  — empty on first load, save-load roundtrip, save replaces previous,
  quarantine-on-parse-failure
- **T-C3c-5** (`crates/oneshim-analysis/src/regime_manager.rs`) —
  hydrate_from replaces in-memory state
- **T-C3c-6** — survives-restart via two sequential store instances
- **T-C3c-7** — watchdog-under-slow-save
