# External gRPC Audit + Live-Config — Plan Verify Round 2 / Platform & Risk Lens

**Reviewer role:** platform / runtime / dependencies / security
**Plan under review:** `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` rev-2 (commit `4bff975e`, ~3560 lines)
**Round-1 review:** `docs/reviews/2026-04-24-external-grpc-plan-review-3-platform-risk.md`
**Round:** Loop 2 Round 2
**Verdict:** **CONDITIONAL-PASS** — Round-1 Criticals resolved; 2 new Criticals found in Task 9.4 G3 test body (references non-existent `ConfigManager` APIs); 1 Important unresolved (I4 RAII guard).

---

## Round-1 findings status

### Resolved

**C1 — SQL schema (audit_entries / id):** RESOLVED. Rev-2 Task 0.4 consistently uses `audit_log` table (L686, L720, L854-856, L923) and `entry_id` column (L859, L886). Cross-check: zero remaining `audit_entries` references outside §"Evidence vs. rev-1" documentation bullets. Migration V32 filename renamed to `v32_audit_log_command_id_index.rs`. SELECT projection (L854-856) matches V25 schema exactly: `entry_id, timestamp, session_id, command_id, action_type, status, details, execution_time_ms`. Row-mapper at L885-894 populates `AuditEntry { entry_id, ... }` correctly. **Verified clean.**

**C2 — std::sync::Mutex API:** RESOLVED. Rev-2 L849 uses the exact pattern I requested: `let Ok(conn) = self.conn.lock() else { tracing::warn!(...); return Vec::new(); };`. Plan L688-690 explicitly documents the constraint ("fallible lock via `LockResult`"). `spawn_blocking` is correctly dropped — L840 header comment: "Synchronous to match the existing `save_audit_entry` pattern (no `spawn_blocking`)". `Arc::clone` discussion from I3 is now moot because the sync path doesn't need to clone `conn` into a closure. **Verified clean.**

**I1 — TrailerCapturingBody trailers-only note:** RESOLVED indirectly. Plan still uses the conditional `new_already_fired` vs `new` split (L2634) for trailers-only vs streaming. This is correct per `http-body 1.0.1` `Frame::trailers_ref` semantics (only trailer frames return `Some`). No code change needed; the test triplet still exercises the branches adequately. No explicit comment was added per my I1 suggestion, but it's not blocking.

**I2 — Phase 9 V32 collision preflight:** RESOLVED. Rev-2 Task 0.4 Step 1 (L700-706) adds the explicit preflight: `git show origin/feature/phase9-tracking-schedule:crates/oneshim-storage/src/migration/mod.rs | grep CURRENT_VERSION`. Fallback instruction present at L706: "If Phase 9 has bumped to 32+ by consume time, change this task to V33 + reserve V32 via empty stub." **Verified clean.**

**I3 — `Arc::clone` readability:** MOOT. Because Rev-2 drops `spawn_blocking`, there is no closure capture of `self.conn`, hence no `self.conn.clone()` call. Issue is architecturally eliminated.

### Partial

**I4 — Deferred audit RAII guard:** PARTIALLY RESOLVED. Plan Task 3.1 Step 3 (L2636) now says: "Increment/decrement `metrics.deferred_audit_in_flight` around spawn body". This is **language improvement** over rev-1 but still not explicit. My Round-1 ask was a concrete `DeferredGuard(Arc<Metrics>)` struct with `impl Drop { fn drop(&mut self) { self.0.deferred_audit_in_flight.fetch_sub(1, Relaxed); } }` — this guarantees decrement on panic/cancel. Rev-2 defers the ordering to the spec §5.5 pseudocode (L2639 "see spec §5.5 L559-688"). If the spec pseudocode has the `fetch_add` outside the spawn and a `fetch_sub` inside at the end (non-RAII), a panic inside the task leaks the gauge. **Recommendation:** Task 3.1 Step 3 should include a one-line code excerpt showing the `DeferredGuard` pattern, not a cross-reference. Non-blocking but keep on Loop 3 impl-review checklist.

### Missed

None.

---

## New Critical issues (Round 2)

### N-C1 — Task 9.4 G3 test calls `ConfigManager::new_in_memory(...)` — **does not exist**

**Impact:** `cargo check` failure on the integration test.

Verified in real source at `crates/oneshim-core/src/config_manager.rs`:

```
39: impl ConfigManager {
40:     pub fn new() -> Result<Self, CoreError> {
45:     pub fn with_path(config_path: PathBuf) -> Result<Self, CoreError> {
```

Only `new()` and `with_path(PathBuf)` constructors exist. The plan L3422 writes:
```rust
let cfg_mgr = Arc::new(ConfigManager::new_in_memory(Arc::new(cfg)));
```

This will fail `cargo check` at Task 9.4 time, blocking the G3 gate test, which is the convergence SLO guardrail.

**Required fix:** Either (a) add `new_in_memory(Arc<AppConfig>)` as a new test-support constructor gated behind `#[cfg(any(test, feature = "test-support"))]` (added in Task 0.0 inventory), or (b) change the G3 test to use `ConfigManager::with_path(tmp_path)` writing a tempfile, then `update_with(|c| { *c = initial_cfg; Ok(()) })`. Option (a) is cleaner. The plan's Task 0.0 inventory (L151) already flags this — "`spawn_server_with_config_manager` likely does NOT exist" — but misses `ConfigManager::new_in_memory` which is in the same category. Add `ConfigManager::new_in_memory` to the Task 0.0 helper list explicitly.

### N-C2 — Task 9.4 G3 test calls `update_with(...).await` — **`update_with` is synchronous, not async**

**Impact:** `cargo check` failure; extra compile error beyond N-C1.

Verified at `config_manager.rs` L139-141:
```
pub fn update_with<F>(&self, updater: F) -> Result<AppConfig, CoreError>
where
    F: FnOnce(&mut AppConfig) -> Result<(), String>,
```

Note two things:
1. **Not async** — no `async fn`, no `Future` return. The `.await` at plan L3437 is invalid.
2. **Closure signature is `FnOnce(&mut AppConfig) -> Result<(), String>`** — the updater receives `&mut AppConfig` directly, not `&mut Arc<AppConfig>`. Plan L3436 `Arc::make_mut(c).external_grpc.streaming_enabled = Some(false);` is wrong — `c` is `&mut AppConfig`, not `&mut Arc<AppConfig>`. `Arc::make_mut` does not apply. And the closure must return `Result<(), String>` not `()`.

**Required fix:** Change L3435-3437 to:
```rust
cfg_mgr.update_with(|c| {
    c.external_grpc.streaming_enabled = Some(false);
    Ok(())
}).expect("update_with must succeed in G3 test");
```

No `.await`. No `Arc::make_mut`. Return `Ok(())`.

Note: plan L2426, L2445, L2467, L2486 show `config_tx.send_modify(|c| { Arc::make_mut(c); })` — that path is on a `watch::Sender<Arc<AppConfig>>` where `send_modify` does give `&mut Arc<AppConfig>`. That's a different API (`tokio::sync::watch::Sender::send_modify`) and is correct for the `LiveConfig` reload loop. The bug is specifically in Task 9.4 where the test is calling `ConfigManager::update_with`, not `watch::Sender::send_modify` — the plan author conflated the two APIs.

---

## New Important issues

### N-I1 — Task 0.4 Step 6 timestamp parsing is CORRECT (false alarm deflected)

Plan L870-872:
```rust
let ts_str: String = row.get("timestamp")?;
let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
```

Cross-checked `migration/v25.rs:9` — column is `timestamp TEXT NOT NULL` (not INTEGER epoch). And `sqlite/mod.rs:262` confirms write path: `let timestamp_str = entry.timestamp.to_rfc3339();`. RFC3339 round-trip is correct. **No issue.**

### N-I2 — Migration V32 partial index `WHERE command_id IS NOT NULL` — verified supported

SQLite has supported partial indexes since v3.8.0 (2013). `rusqlite 0.38` bundles SQLite ≥ 3.45. Plan L719-720 `CREATE INDEX IF NOT EXISTS idx_audit_log_command_id ON audit_log (command_id) WHERE command_id IS NOT NULL;` is valid. One subtle note: V25 schema declares `command_id TEXT NOT NULL` (v25.rs:11). Since the column is `NOT NULL`, the `WHERE command_id IS NOT NULL` predicate is **tautological** — every row matches. The partial-index qualifier is harmless but unnecessary. Consider dropping the `WHERE` clause for clarity, or add a comment explaining it's defensive against future migrations that might relax the `NOT NULL` constraint. Non-blocking.

### N-I3 — Task 0.0 `InnerEcho` snippet is incomplete (self-acknowledged)

Plan L194 notes: "Body impl + Service impl — ~40 LoC; see trailer_body::tests::FixtureBody for pattern." The snippet at L177-192 shows only the `Clone`-derived struct with three constructors — no `Service` impl, no `Body` impl. The plan acknowledges this is a scaffold.

**Risk:** Loop 3 impl-writer may underestimate the 40 LoC of boilerplate (associated types, `poll_frame`, `poll_ready`, `call`). I recommend the plan embed a pointer to the actual `FixtureBody` pattern in `plan L1654+` (the test fixture in trailer_body tests) so the impl-writer can copy-paste the `Service` boilerplate. Non-blocking but improves plan quality.

---

## Verdict

**CONDITIONAL-PASS** — Round-1 Criticals C1/C2 cleanly resolved; both Importants I1/I2 addressed. But the G3 convergence test body (critical for spec G3 CI gate) has **two compile-blocking errors**:
- `ConfigManager::new_in_memory` does not exist
- `update_with(...).await` — `update_with` is sync, not async; also closure signature mismatch

Both are in ~5 lines of the G3 test body. Both are 1-line fixes if the plan author accepts the options in N-C1 / N-C2. Rev-3 of the plan (or impl-time adjustment with spec acknowledgment) should:

1. Add `ConfigManager::new_in_memory` to Task 0.0 inventory (or replace G3 test with `with_path` + tempfile).
2. Rewrite G3 test L3435-3437 as sync `update_with(|c| { c.external_grpc.streaming_enabled = Some(false); Ok(()) })` (no await, no Arc::make_mut).

Optionally tighten:
3. Make `DeferredGuard` RAII pattern explicit in Task 3.1 Step 3.
4. Drop tautological `WHERE command_id IS NOT NULL` from V32 index or add justification comment.
5. Point Task 0.0 InnerEcho stub to `trailer_body::tests::FixtureBody` code explicitly.

If these 2 compile-blocking fixes land in rev-3, the platform/risk lens can PASS this plan for Loop 3 implementation.

---

**Word count:** ~1320.
**Blocking findings:** N-C1 + N-C2 (both in G3 test body, ~5 lines of fix).
**Recommended gate:** CONDITIONAL-PASS — fix the 2 G3 test API bugs, then proceed to Loop 3.
