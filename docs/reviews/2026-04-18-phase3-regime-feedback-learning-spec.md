# Phase 3 — FeedbackSignalSink + regime_id filter + RegimeManager persistence

_Date_: 2026-04-18
_Scope_: `client-rust` repository. Three remaining items from `docs/reviews/2026-04-16-feature-gaps-analysis.md` — narrower than the original 4-week projection because C1 retry queue, C2 coaching trigger, and C3b lifecycle tick were verified as already wired on current `main` (see `.claude/phase3-brief.md`).

- **X3** FeedbackSignalSink — route accept/reject/defer into `CoachingEngine` and `RegimeClassifier` (not just the outbound `ApiClient`).
- **C3a** `regime_id` vector filter — replace the silent-ignore warning in `search_filtered` + `search_quantized` with a real `WHERE` via the existing `activity_segments.regime_id` column.
- **C3c + X6** RegimeManager persistence — hydrate `RegimeManager` state on startup and persist on graceful shutdown so regimes survive restart.

_Non-goals_: C1 feedback retry (already wired), C2 coaching trigger (already wired), C3b deactivation rules refinement (`run_maintenance` already called per tick; rule tuning is deferred), Phase 4 provider/platform polish.

_Delivery_: single branch, phased. X3 ships first (narrowest blast radius, adds a port + one new call site), then C3a (schema-adjacent, single-file SQL change), finally C3c/X6 (touches migration + startup + shutdown).

---

## 1. Motivation

### 1.1 X3 — learning loop is still disconnected

Today, `commands/suggestions.rs::handle_suggestion_action` routes accept/reject/defer through `FeedbackSender`, which calls the server's `ApiClient::send_feedback`. On failure it enqueues onto `FeedbackRetryQueue`, which the scheduler drains. Nothing inside the client hears about these events — `CoachingEngine` never learns that a suggestion was accepted, `RegimeClassifier` never sees which regime's suggestions the user accepts vs rejects.

Consequence: the coaching feature ships proactive messages based on static templates + regime triggers, with no adaptation to user reactions. The original gap analysis captured this as the "learning signal path" remainder of C1.

### 1.2 C3a — `regime_id` vector filter silently ignored

`crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs:129` and `:388` contain:

```rust
if filters.regime_id.is_some() {
    tracing::warn!("regime_id filter not yet implemented, ignoring");
}
```

Callers pass `regime_id` expecting scoped results; they silently get cross-regime hits. For any downstream consumer that semantically relies on regime scoping (a per-regime similarity recommendation, a regime-aware suggestion), the data is wrong.

### 1.3 C3c + X6 — RegimeManager is ephemeral

`RegimeManager` is pure-in-memory. On startup, `agent_runtime/analysis_setup.rs:99` calls `RegimeManager::new(tm_config)` with an empty regime list. The process re-clusters from scratch; any user overrides (name, merges, deletes) vanish. The SQLite `regimes` table exists but is only touched by `sync_merger.rs` (cross-device sync metadata path), not by `RegimeManager` itself.

Consequence: every restart loses regime state. The "new regime discovered" notification fires on the same cluster each cold boot. Telemetry, coaching, and the vector filter from C3a all fail to maintain identity across restarts.

---

## 2. Design — X3 FeedbackSignalSink

### 2.1 Port trait in `oneshim-core`

```rust
// crates/oneshim-core/src/ports/feedback_signal_sink.rs

use async_trait::async_trait;
use crate::models::suggestion::SuggestionFeedback;
use crate::error::CoreError;

/// Cross-crate notification channel for user reactions to suggestions.
///
/// Implementations wrap `CoachingEngine`, `RegimeClassifier`, or any other
/// component that should adapt to accept/reject/defer signals.
///
/// # Failure semantics
///
/// Fire-and-forget from the caller's perspective. `FeedbackSender` MUST NOT
/// block user-path accept/reject on a sink error.
///
/// The `Result` return is ONLY for programmer bugs (mutex poisoning, invariant
/// violations). All expected failure classes — network, database, transient
/// unavailability — are the implementation's responsibility to log and swallow
/// internally; they MUST NOT escalate as `Err`.
///
/// # Latency budget
///
/// Implementations must return within ~10 ms. Any blocking work (database
/// writes, network calls, heavy computation) must be offloaded to
/// `tokio::spawn` INSIDE the impl so the inline path stays O(µs). The caller
/// awaits this future synchronously on the user-path accept/reject; breaking
/// this budget re-introduces the write-path wait we intentionally decoupled.
///
/// See ADR-017 for the rationale.
#[async_trait]
pub trait FeedbackSignalSink: Send + Sync {
    async fn record_user_reaction(
        &self,
        feedback: &SuggestionFeedback,
    ) -> Result<(), CoreError>;
}
```

### 2.2 Default implementation: `CompositeFeedbackSink`

Located in `src-tauri` (composition root) because it fans out into multiple adapter crates:

```rust
pub struct CompositeFeedbackSink {
    coaching: Option<Arc<oneshim_analysis::CoachingEngine>>,
    regime_classifier: Option<Arc<parking_lot::Mutex<oneshim_analysis::RegimeClassifier>>>,
}

#[async_trait]
impl FeedbackSignalSink for CompositeFeedbackSink {
    async fn record_user_reaction(
        &self,
        feedback: &SuggestionFeedback,
    ) -> Result<(), CoreError> {
        if let Some(ref c) = self.coaching {
            c.record_user_reaction(feedback).await;
        }
        if let Some(ref cls) = self.regime_classifier {
            let mut g = cls.lock();
            g.record_user_reaction(feedback);
        }
        Ok(())
    }
}
```

Both `Option` — each consumer may or may not be present depending on feature flags / runtime context. `Arc<Mutex<…>>` for `RegimeClassifier` because it needs interior mutability (the classifier updates its internal statistics).

### 2.3 New methods on `CoachingEngine` and `RegimeClassifier`

`CoachingEngine::record_user_reaction(feedback)` — records the feedback_type for the coaching-message-id. Implementation-side guidance only; this spec defines the method **signature** and specifies "must be side-effect-only, non-blocking except for a brief lock". Concrete learning algorithm (e.g., bayesian update of trigger priors) is left to a follow-up phase.

`RegimeClassifier::record_user_reaction(feedback)` — records per-regime acceptance rates. Same deferral: signature only in this phase, algorithm follows.

The aim of X3 is **wiring the channel**, not implementing learning. Downstream algorithm changes can land in follow-up PRs without touching the port.

### 2.4 Wiring in `FeedbackSender`

```rust
pub struct FeedbackSender {
    api_client: Arc<dyn ApiClient>,
    sink: Option<Arc<dyn FeedbackSignalSink>>,
}

impl FeedbackSender {
    pub fn new_with_sink(
        api_client: Arc<dyn ApiClient>,
        sink: Option<Arc<dyn FeedbackSignalSink>>,
    ) -> Self {
        Self { api_client, sink }
    }

    async fn send_feedback(&self, ...) -> Result<(), SuggestionError> {
        // 1. Fire-and-forget to the local sink.
        if let Some(ref s) = self.sink {
            if let Err(e) = s.record_user_reaction(&feedback).await {
                tracing::warn!(error = %e, "feedback sink error (non-fatal)");
            }
        }
        // 2. Forward to server (unchanged behaviour).
        self.api_client.send_feedback(&feedback).await
    }
}
```

Sink call happens **before** the server call so local learning adapts even when the server is unreachable (retry queue handles the delayed server-side acknowledgement).

`new` is preserved as a shim: `pub fn new(api) -> Self { Self::new_with_sink(api, None) }`. Every existing `FeedbackSender::new(api)` call site stays valid; the composition root wires a real sink.

### 2.5 Alternatives considered — X3

| Option | Verdict | Why |
|--------|---------|-----|
| Event bus (`tokio::sync::broadcast`) | Rejected | Adds a runtime task + sizing concern for no capability gain — only two consumers today and no per-event queuing needed. |
| Direct `Arc<CoachingEngine>` reference from `FeedbackSender` | Rejected | Violates hexagonal boundary — `oneshim-suggestion` would have to depend on `oneshim-analysis`. |
| Port method per consumer (`CoachingSink`, `RegimeSink`) | Rejected | Explodes the port surface with no caller that wants to pick one-not-the-other. `CompositeFeedbackSink` already handles Option<> per consumer. |
| Fire the sink AFTER server call | Considered but rejected | Server failure would prevent local learning. Local signal has independent value. |

### 2.6 Tests — X3

| # | Test | Asserts |
|---|------|---------|
| T-X3-1 | `sink_receives_accept_reject_defer` | Mock sink records each feedback_type exactly once per call. |
| T-X3-2 | `sink_error_does_not_fail_send_feedback` | Mock sink returns `Err(CoreError)`; `FeedbackSender::send_feedback` still returns `Ok(())` on server success. |
| T-X3-3 | `no_sink_configured_behaves_as_before` | `FeedbackSender::new(api)` (no sink) — verify `send_feedback` still works and no panic. |
| T-X3-4 | `sink_called_before_server` | A slow-Mock ApiClient blocks for 500ms; the sink is invoked synchronously before the server call begins. |
| T-X3-5 | `composite_sink_fans_out_to_both` | CompositeFeedbackSink with both coaching + regime_classifier Mocks — verify both `record_user_reaction` calls land. |

---

## 3. Design — C3a `regime_id` vector filter

### 3.1 Schema path via existing join

`embedding_vectors.segment_id` already references `activity_segments.id`; `activity_segments.regime_id` is nullable but indexed (`idx_segments_regime`). No migration needed — a correlated subquery (or explicit join) resolves the filter.

### 3.2 SQL replacement

In both `search_filtered` (line ~129) and `search_quantized` (line ~388) of `crates/oneshim-storage/src/sqlite/vector_store_impl/trait_impl.rs`, replace:

```rust
if filters.regime_id.is_some() {
    tracing::warn!("regime_id filter not yet implemented, ignoring");
}
```

With:

```rust
if let Some(ref regime_id) = filters.regime_id {
    conditions.push(format!(
        "segment_id IN (SELECT id FROM activity_segments WHERE regime_id = ?{})",
        param_values.len() + 1
    ));
    param_values.push(Box::new(regime_id.clone()));
}
```

Subquery over a correlated column with an index (`idx_segments_regime`) — SQLite plans this as a nested loop and the index turns it into O(hits) rather than O(rows) scan.

### 3.3 Alternatives considered — C3a

| Option | Verdict |
|--------|---------|
| Explicit JOIN | Equivalent plan but requires aliasing the columns in the SELECT list; the subquery is more localised. Same big-O. |
| Denormalise `regime_id` onto `embedding_vectors` | Rejected for this phase — requires a migration and a write-path update everywhere that inserts embeddings. Subquery is sufficient unless benchmarks show it's the hot path. |
| Post-query Rust filter | Rejected — wastes SQLite→Rust round-trip, defeats the purpose of passing `regime_id` through the port. |

### 3.4 Tests — C3a

| # | Test | Asserts |
|---|------|---------|
| T-C3a-1 | `search_filtered_excludes_other_regimes` | Seed 3 segments across 2 regimes; query with `regime_id = "r1"` returns only r1 embeddings. |
| T-C3a-2 | `search_quantized_excludes_other_regimes` | Same as above on the quantized path. |
| T-C3a-3 | `regime_id_none_preserves_existing_behaviour` | No regime filter → returns all matches regardless of regime_id. Regression guard for callers that pass `None`. |
| T-C3a-4 | `segment_without_regime_not_returned_under_filter` | A segment with `activity_segments.regime_id IS NULL` must not match any `regime_id = ?` query. |

---

## 4. Design — C3c + X6 RegimeManager persistence

### 4.1 Port trait in `oneshim-core`

```rust
// crates/oneshim-core/src/ports/regime_storage.rs

use async_trait::async_trait;
use crate::models::tiered_memory::regime::Regime;
use crate::error::CoreError;

#[async_trait]
pub trait RegimeStoragePort: Send + Sync {
    /// Load all persisted regimes on startup. Empty Vec on first launch.
    ///
    /// Implementations MAY perform corrective side-effect writes — e.g.,
    /// quarantining a payload that failed to deserialise so user-curated
    /// state is preserved for later recovery (see `SqliteRegimeManagerStateStore`).
    /// Despite the name, `load_all` is therefore NOT guaranteed read-only;
    /// callers must treat it as a single-shot operation at startup. Concurrent
    /// `load_all` calls are not required to be safe.
    async fn load_all(&self) -> Result<Vec<Regime>, CoreError>;

    /// Persist the full regime set. Called on graceful shutdown and,
    /// in a future phase, periodically after lifecycle transitions
    /// (merge, delete, rename).
    async fn save_all(&self, regimes: &[Regime]) -> Result<(), CoreError>;
}
```

Full-replace semantics (`save_all`) keeps the port simple — the RegimeManager's full state fits in memory and a few hundred regimes × ~500 bytes JSON = negligible write cost. If that ever becomes a hotspot, a diff-based API is a backward-compatible follow-up.

### 4.2 Persistence format: JSON blob in existing `regimes` table, single row

Two reasons:

1. **The existing `regimes` table columns are partial and were designed for the sync path, not RegimeManager's actual state.** Adding the missing columns (centroid, RegimeStatus enum, name override) would require migration + write-path update in sync_merger to keep it in sync. Scope creep.
2. **A JSON blob is robust across RegimeManager schema evolution** — new fields auto-serialise with serde defaults.

Proposal: store the RegimeManager state as a single row in a **new** dedicated table `regime_manager_state`, plus a quarantine column for failed-to-parse payloads so user-curated state is NEVER silently wiped on a schema-mismatch:

```sql
CREATE TABLE IF NOT EXISTS regime_manager_state (
    id INTEGER PRIMARY KEY CHECK (id = 0),
    payload TEXT NOT NULL,
    payload_backup TEXT,                 -- set only when a corrupt payload is quarantined
    payload_backup_at TEXT,              -- timestamp of quarantine
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Singleton row (id=0, enforced by CHECK). `payload` is `serde_json::to_string(&regimes)`. `updated_at` for diagnostics.

**Parse-failure handling**: on `load_all`, if `serde_json::from_str(&payload)` fails, the implementation:
1. Copies the unparseable payload to `payload_backup` with `payload_backup_at = datetime('now')`.
2. Logs at `error!` (not `warn!`) with the parse error and a hint that user-curated state has been quarantined.
3. Returns `Ok(Vec::new())` so the app starts with a fresh RegimeManager rather than crashing.

The backup preserves the data for later recovery (manual migration script, support request, or a future auto-heal pass). Wiping silently — the original draft — would throw away months of user-curated regime names on any future schema break.

Does **not** touch the existing `regimes` table — sync_merger's use of that table is preserved unchanged.

### 4.3 Migration

New migration file `crates/oneshim-storage/src/migration/v31_regime_manager_state.rs`. Current `CURRENT_VERSION` is 30 (verified in `crates/oneshim-storage/src/migration/mod.rs:33`); v31 is the next free slot. Only creates the table; no data migration needed because RegimeManager has no pre-existing state to import.

### 4.4 Implementation

`crates/oneshim-storage/src/regime_manager_state_store.rs` — new file implementing `RegimeStoragePort`:

```rust
pub struct SqliteRegimeManagerStateStore {
    conn: Arc<Mutex<Connection>>,
}

#[async_trait]
impl RegimeStoragePort for SqliteRegimeManagerStateStore {
    async fn load_all(&self) -> Result<Vec<Regime>, CoreError> {
        let conn = self.conn.lock();
        let payload: Option<String> = conn
            .query_row(
                "SELECT payload FROM regime_manager_state WHERE id = 0",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        match payload {
            Some(json) => match serde_json::from_str::<Vec<Regime>>(&json) {
                Ok(regimes) => Ok(regimes),
                Err(e) => {
                    // Quarantine — never silently wipe user-curated state.
                    tracing::error!(
                        error = %e,
                        "regime_manager_state payload failed to parse; quarantining to payload_backup and starting fresh. Recovery via manual inspection of the backup column."
                    );
                    let _ = conn.execute(
                        "UPDATE regime_manager_state
                            SET payload_backup = payload,
                                payload_backup_at = datetime('now'),
                                payload = '[]',
                                updated_at = datetime('now')
                          WHERE id = 0",
                        [],
                    );
                    Ok(Vec::new())
                }
            },
            None => Ok(Vec::new()),
        }
    }

    async fn save_all(&self, regimes: &[Regime]) -> Result<(), CoreError> {
        let json = serde_json::to_string(regimes)
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO regime_manager_state (id, payload, updated_at) VALUES (0, ?1, datetime('now'))",
            rusqlite::params![json],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
}
```

### 4.5 Startup hydration

In `agent_runtime/analysis_setup.rs` around line 99, replace:

```rust
regime_manager: oneshim_analysis::RegimeManager::new(tm_config),
```

With:

```rust
regime_manager: {
    let mut mgr = oneshim_analysis::RegimeManager::new(tm_config);
    // Load persisted state (best-effort — empty on first boot or parse error).
    match regime_storage.load_all().await {
        Ok(regimes) if !regimes.is_empty() => {
            mgr.hydrate_from(regimes);
            info!(count = mgr.all_regimes().len(), "regime manager hydrated from storage");
        }
        Ok(_) => info!("regime manager: no persisted state, starting fresh"),
        Err(e) => warn!(error = %e, "regime manager hydrate failed; starting fresh"),
    }
    mgr
},
```

New method on `RegimeManager`:

```rust
pub fn hydrate_from(&mut self, regimes: Vec<Regime>) {
    self.regimes = regimes;
}
```

Small, single-purpose — alternative would be `RegimeManager::with_regimes()` ctor but that clones the whole config; hydrate is cheaper.

### 4.6 Shutdown persistence

The actual shutdown orchestrator is the `RunEvent::Exit` handler in `src-tauri/src/main.rs` around line 335 (it already persists the suggestion queue, terminates AI sessions, and runs the WAL checkpoint). `src-tauri/src/lifecycle.rs` is just the watch-channel coordinator that carries the shutdown signal — it has no hook registry. We add the regime save directly into the `RunEvent::Exit` block alongside the existing persistence steps:

```rust
// Inside the existing `RunEvent::Exit => { ... }` block in main.rs, AFTER
// the suggestion-queue persist and AI-session shutdown, BEFORE the WAL
// checkpoint:
if let Some(ref regime_storage) = state.regime_storage {
    if let Some(ref regime_manager) = state.regime_manager_snapshot {
        let regimes = regime_manager.all_regimes().to_vec();
        let save_future = regime_storage.save_all(&regimes);
        let runtime_handle = state.runtime_handle.clone();
        let outcome = runtime_handle.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(4),
                save_future,
            )
            .await
        });
        match outcome {
            Ok(Ok(())) => info!(count = regimes.len(), "regime state persisted"),
            Ok(Err(e)) => warn!(error = %e, "regime state save failed"),
            Err(_) => warn!("regime state save exceeded 4s; proceeding with shutdown"),
        }
    }
}
```

Watchdog is **4 s**, matching the telemetry OTel shutdown deadline — not arbitrarily tighter. SQLite `INSERT OR REPLACE` on a singleton row under WAL contention can momentarily exceed 3 s if `run_maintenance` or the analysis pipeline is mid-transaction when the signal arrives. 4 s covers worst-case; past the deadline we log and continue. Shutdown MUST NOT be blocked.

`state.regime_storage` and `state.regime_manager_snapshot` are new `Option<Arc<dyn RegimeStoragePort>>` / `Option<Arc<RegimeManager>>` fields on the existing Tauri `State` struct in `src-tauri/src/runtime_state.rs`. They are populated in Commit 7 from the composition root.

Exit from `RunEvent::Exit` is NOT via `std::process::exit(0)` — the handler returns and Tauri finishes the normal exit path. The save is therefore synchronous relative to the handler (via `runtime_handle.block_on`). `lifecycle.rs`'s `FORCE_EXIT_GRACE_SECS=3` hard-kill timer is orthogonal — it only fires if the Tauri event loop itself hangs.

### 4.7 Mid-life persistence (optional)

Out of scope for this phase. A future enhancement could auto-save after every `run_maintenance` tick. Current `all_regimes()` count is bounded (≤ `max_active + archive_days`), so mid-life save is cheap, but **only ship the shutdown path now** to keep the blast radius contained. Add an item to Phase 3 follow-ups for the periodic save.

### 4.8 Tests — C3c + X6

| # | Test | Asserts |
|---|------|---------|
| T-C3c-1 | `empty_on_first_load` | Fresh SQLite DB → `load_all()` returns `Ok(vec![])`. |
| T-C3c-2 | `save_then_load_roundtrip` | Save 3 regimes with distinct statuses → `load_all()` returns identical set. |
| T-C3c-3 | `save_replaces_previous` | Save 3 → save 1 → `load_all()` returns 1 (upsert semantics). |
| T-C3c-4 | `malformed_payload_quarantines_and_starts_fresh` | Hand-write bad JSON into the row → `load_all()` returns `Ok(vec![])` (NOT Err), and the bad payload is copied to `payload_backup` with `payload_backup_at` set. Never panics. Never silently wipes the original. |
| T-C3c-5 | `hydrate_from_replaces_in_memory_state` | `RegimeManager` with existing regimes → `hydrate_from(new_set)` → `all_regimes()` equals `new_set`. |
| T-C3c-6 | `shutdown_save_within_watchdog` | Integration: create RegimeManager with 5 regimes, trigger shutdown, verify save completes and load after restart matches. |
| T-C3c-7 | `shutdown_save_timeout_does_not_panic` | Mock store that blocks for 5 s → shutdown proceeds within 3 s + small overhead; warn log emitted. |

---

## 5. Cross-item interaction

- **X3 ↔ C3a**: independent. X3 writes a learning signal; C3a filters vector queries. Unrelated.
- **X3 ↔ C3c**: independent. But the eventual downstream learning algorithm inside `CoachingEngine::record_user_reaction` and `RegimeClassifier::record_user_reaction` may want to persist adaptation state. That is deferred — the port signatures lock in the *shape* but the persistence of any learning counters they add is a Phase 4+ concern.
- **C3a ↔ C3c**: loosely coupled. Once `RegimeManager` persists across restart, its `regime_id` values stay stable — which is what makes the C3a filter meaningful across sessions. Without persistence, a restart produces fresh regime IDs and any cached `regime_id` filter value becomes stale. This is an argument for landing C3c before users can save regime IDs, but **both can land in the same PR in any internal order** because neither's tests depend on the other.

---

## 6. Documentation deliverables

- `docs/architecture/ADR-017-feedback-signal-sink.md` — records the port-based fanout and the fire-and-forget failure semantic. Korean companion.
- `docs/architecture/ADR-018-regime-manager-persistence.md` — records the JSON-blob-in-dedicated-table choice and why the existing `regimes` table was not reused. Korean companion.
- No new user-facing guide. The learning signal and regime persistence are both invisible to users beyond "regimes survive restart" which is a default-improvement, not a feature toggle.
- `docs/STATUS.md` — bump test totals after implementation.

ADR numbers: verified against `docs/architecture/ADR-*.md` — ADR-016 is the next taken slot (from Phase 2); 017 and 018 are free.

---

## 7. Rollout

Single feature branch `feat/phase3-regime-feedback-learning`:

1. **Commit 1** — X3 port + CoachingEngine/RegimeClassifier method stubs + FeedbackSender sink param + T-X3-1..5.
2. **Commit 2** — ADR-017 (includes the ~10 ms latency-budget rationale that §2.1's trait doc references).
3. **Commit 3** — C3a SQL filter + T-C3a-1..4.
4. **Commit 4** — RegimeStoragePort + v31 migration (with comment noting the deliberately-nullable `payload_backup_at` divergence from the usual `NOT NULL DEFAULT (datetime('now'))`) + SqliteRegimeManagerStateStore + T-C3c-1..5.
5. **Commit 5** — AppState field declarations (`regime_storage: Option<Arc<dyn RegimeStoragePort>>`, `regime_manager_snapshot: Option<Arc<RegimeManager>>`, initialised to `None`) + `RunEvent::Exit` save-guard block + `RegimeManager::hydrate_from`. **No new tests here** — the save path is dormant until Commit 7 populates the fields. Commit keeps `cargo check/test/clippy` green because the guard is a no-op when both fields are `None`.
6. **Commit 6** — ADR-018.
7. **Commit 7** — Composition-root wiring: build `CompositeFeedbackSink`, pass via `FeedbackSender::new_with_sink`; construct `SqliteRegimeManagerStateStore`, call `load_all` during startup with `hydrate_from`, populate the Commit-5 AppState fields so `RunEvent::Exit`'s save guard is active. **T-C3c-6 and T-C3c-7 land here** because they exercise the end-to-end populated path.

Each commit must keep `cargo check --workspace`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` green. Commit 5's test coverage gap is deliberate: the dormant guard is trivially covered by compile-time + Commit 7's integration tests.

---

## 8. Acceptance criteria

- `cargo check --workspace` green.
- `cargo test --workspace` green with all 16 new tests (T-X3-1..5, T-C3a-1..4, T-C3c-1..7) passing.
- `cargo clippy --workspace --all-targets -- -D warnings -A clippy::empty_docs -A clippy::derivable_impls -A clippy::type_complexity` green.
- `cargo fmt --check` green.
- `.github/workflows/ci.yml` passes on the PR.
- Integration-level survives-restart guarantee is covered by **T-C3c-6** (two sequential `SqliteRegimeManagerStateStore` constructions against the same `TempDir` SQLite file, asserting the second load returns the first save's regime set). No separate manual RC verification required.

---

## 9. Risks & mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|-----------|
| JSON blob grows unbounded as regimes accumulate | Low | `RegimeManager::run_maintenance` already archives / deletes per rule. If size becomes an issue, add a `max_persisted` cap in a follow-up — port is unchanged. |
| Subquery in `search_filtered` regresses latency on large embedding tables | Medium | `idx_segments_regime` is already in place. Timing assertions in `cargo test` are flaky under CI scheduling, so the PR tests do NOT gate on latency. A dedicated `benches/regime_filter_bench.rs` (invoked manually during RC validation) records the baseline. If the bench shows the subquery is the hot path, denormalise `regime_id` onto `embedding_vectors` via migration — separate follow-up PR. |
| `FeedbackSignalSink::record_user_reaction` calls the async methods which may take a lock — blocks the user-path accept/reject UI | Low | Sink call is awaited but the implementations grab a `parking_lot::Mutex` and return immediately. T-X3-4 asserts no unbounded wait. |
| Shutdown save collides with another write (e.g., sync_merger) | Low | `sync_merger` writes to the separate `regimes` table; our new `regime_manager_state` table is disjoint. No lock interaction. |
| Schema evolution of `Regime` breaks existing JSON payload | Medium | serde's `#[serde(default)]` handles additive fields. For removed or renamed fields, §4.4 quarantines the bad payload to `payload_backup` with a timestamp, logs `error!`, and starts fresh — T-C3c-4 asserts this. User-curated state is never silently wiped; recovery is possible via the backup column. |
| Re-hydrated regimes conflict with fresh `update_from_detection` events | Low | `update_from_detection` matches by centroid distance + merge; imported regimes participate in the same loop. Identity is preserved across restart. |

---

## 10. Out of scope

- Concrete learning algorithms inside `CoachingEngine::record_user_reaction` / `RegimeClassifier::record_user_reaction`. This spec lands the channel; the algorithms are a follow-up.
- Periodic mid-life regime persistence (save after every `run_maintenance` tick). Shutdown-only in this phase.
- Migrating the existing `regimes` table to carry RegimeManager's full state. We introduce a new dedicated table instead to avoid touching the sync path.
- Cross-device sync of the new `regime_manager_state` table. Sync is per-table per-policy; this table is local-only until a future HLC-tagging pass.
- Denormalising `regime_id` onto `embedding_vectors` — conditional on the perf risk materialising.
- UI surface for "forget my learned preferences" — CoachingEngine/RegimeClassifier reset is a UX slice.
