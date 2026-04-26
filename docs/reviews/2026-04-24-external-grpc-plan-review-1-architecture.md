# Plan Review 1 — Architecture Lens

**Reviewer role**: Architecture (task ordering, deps, hexagonal conformance, concurrency)
**Plan under review**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` (~3204 lines, 10 phases)
**Spec reference**: rev-4 (commit `659bcebd`)
**Round**: Loop 2 / Round 1

---

## Critical

### C1: Task 0.4 targets the wrong crate/impl for `entries_by_command_id`

**Location**: plan Task 0.4 (lines 484-661) and Task 0.3 (lines 386-481)

**Issue**: The plan instructs "In `crates/oneshim-storage/src/sqlite/` (the audit submodule), add inside `impl AuditLogPort for SqliteStorage`…" — but **`SqliteStorage` does not implement `AuditLogPort`**. Grep confirms the only non-test `impl AuditLogPort` lives in `crates/oneshim-automation/src/audit.rs:337` on `AuditLogAdapter` (which wraps `Arc<RwLock<AuditLogger>>`). `SqliteStorage` exposes only a non-trait `save_audit_entry()` helper (sqlite/mod.rs:255) used from a persistence callback. The audit table is also `audit_log` (from V25), NOT `audit_entries` as the plan asserts. See:

- `crates/oneshim-storage/src/migration/v25.rs:11`: `command_id TEXT NOT NULL,` in `audit_log`
- `crates/oneshim-storage/src/sqlite/mod.rs:255`: `save_audit_entry` inserts into `audit_log`
- No `AuditLogPort` implementation anywhere in `oneshim-storage`.

Plan's SQL (`FROM audit_entries WHERE command_id = ?1`) would fail at runtime; the migration file path (`migration/vNN_audit_command_id_index.rs`) never dispatches for `SqliteStorage`'s port impl because there is none.

**Evidence**: Plan line 512 — `WHERE command_id = ?1`; actual Rust table name in V25 migration is `audit_log`. Plan line 440 shows the NoopAudit stub path in `spawn_config.rs:116` (correct), but the real impl needed at Task 0.4 doesn't live where the plan claims.

**Suggested fix**: Retarget Task 0.4 to `AuditLogAdapter` (or add it as a method on `AuditLogger` first, then surface through the adapter's trait impl). Since `AuditLogger` is an in-memory `VecDeque`, "query by command_id" is either (a) a linear VecDeque scan, or (b) a call from the adapter into `SqliteStorage` via the persistence callback path. The plan needs a full re-architecture of Task 0.4:
1. Decide: is the lookup memory-side (AuditLogger's buffer) or disk-side (SqliteStorage's audit_log)? The spec §5.9 implies disk — so we need a **new** SqliteStorage method + a bridge wiring adapter → storage. This is a real dependency gap, not just a rename.
2. Fix SQL table name (`audit_log`), column list (see v25.rs — it's `entry_id, timestamp, session_id, command_id, action_type, status, details, execution_time_ms`).
3. Add the migration under correct V32 number (current is V31, plan says V32 — correct), but register in `mod.rs` CURRENT_VERSION bump (line 36).

This is critical because Task 0.4 blocks Task 7.2 (`/api/audit/export`) and Task 0.3 (test stubs reference `entries_by_command_id`), and it blocks the whole Phase 9.3 integration test block.

---

### C2: `AuditBridge::record` / `record_completion` signature expansion silently drops existing fields

**Location**: plan Task 0.6 (lines 748-875)

**Issue**: The current `record` takes **11 parameters** (ctx, remote_addr, operation, result, status, duration, request_size, response_size, failure_reason; the spec says it also computes details internally). The plan's new signature (lines 807-821) drops `result: &'static str`, `request_size: Option<u64>`, and `response_size: Option<u64>` and inserts `reason: &str` and `command_id: Option<String>`. Similarly, the plan's new `record_completion` (lines 823-837) drops the existing return `-> String` (which callers today use to get `command_id` back) — see existing impl at `audit_bridge.rs:48-170`:

```rust
pub async fn record(..., request_size: Option<u64>, response_size: Option<u64>, failure_reason: Option<&str>) -> String { ... }
pub async fn record_completion(..., response_message_count: Option<u64>, failure_reason: Option<&str>) -> String { ... }
```

The existing `ExternalGrpcAuditDetails` carries `request_size_bytes` and `response_size_bytes` fields that the plan's replacement signature stops populating. Callers in `audit_layer.rs` / `auth_layer.rs` that use the return value for follow-up logging/metrics would break silently (the plan says "update existing call sites" but doesn't enumerate them, and the diff risks a cascading cleanup at 3+ sites).

**Evidence**: Lines 65, 75, 106, 111 in `audit_bridge.rs` (existing code) — multiple `request_size_bytes`/`response_size_bytes`/`ctx.command_id.clone()` return-value consumers.

**Suggested fix**: Explicitly enumerate the signature changes in Task 0.6 step-by-step:
- Keep `request_size` + `response_size` parameters (or document their removal as intentional with a separate commit).
- Add `command_id: Option<String>` as the 10th/11th param without dropping existing args.
- Clarify whether the `-> String` return value is preserved (callers depend on it).
- Add a "callers to update" checklist with grep-verified line numbers.
- Split this into two commits: (a) add optional new params (default None), (b) populate them in AuditLayer — otherwise Task 0.6 produces a broken intermediate compile state when Task 3.1 isn't yet merged.

---

## Important

### I1: `TrailerCapturingBody::new_already_fired` contract mismatch with spec §5.3

**Location**: plan Task 1.3 (lines 1549-1562)

**Issue**: The plan creates `new_already_fired(inner: B, captured: Option<tonic::Code>) -> Self` with `signal: None`. In `poll_frame`, the implementation at lines 1566-1589 still runs, including `if let Some(tx) = this.signal.take()` — which is None for the already-fired path. So far fine. But `captured` is **still overwritten** by trailer-observed code:

```rust
if this.captured.is_none() {
    *this.captured = code;
}
```

If `new_already_fired` sets `captured: Some(Code::PermissionDenied)` (header-first path), but the upstream tonic body includes a trailing grpc-status (protocol-violating duplicate), the overwrite is gated by `captured.is_none()` — OK. But Drop (PinnedDrop at 1537-1545) only fires if `signal` is Some; for `new_already_fired` construction, signal is None, so drop does nothing — matches the spec. However, there's no test covering the "wrap with `new_already_fired` + Drop" path for streaming bodies that happen to emit another trailer mid-stream. Consider documenting the invariant: **once `new_already_fired` is used, trailer observations are ignored** (captured is preset and signal was already consumed).

**Suggested fix**: Make it explicit in pseudocode / test (around 1754-1762) and add a unit test where `new_already_fired(body_with_trailer, Some(Code::Ok))` asserts that the preset status is NOT overwritten by a later trailer frame OR that it IS. Pick one and pin it.

### I2: `ConfigReloadTask` watch::Receiver clone timing vs. shutdown semantics

**Location**: plan Task 4.2 (lines 2486-2527)

**Issue**: The reload task spawn in `build_external_spawn_config` clones `shutdown_rx` BEFORE returning:

```rust
let shutdown_rx_for_reload = shutdown_rx.clone();
tokio::spawn(async move { run_config_reload(..., shutdown_rx_for_reload).await; });
```

But `shutdown_rx` itself is ALSO moved into the returned `ExternalGrpcSpawnConfig.shutdown_rx` (see existing struct at spawn_config.rs:57 — `pub shutdown_rx: watch::Receiver<bool>`). The plan doesn't show how `shutdown_rx` is cloned BEFORE being put into the struct (or cloned from the struct after). Current code at app_runtime_launch.rs (grep shows `load_policy` + `streaming_enabled` params) constructs shutdown_rx upstream — but the plan never demonstrates that the final assignment into the struct still has a valid Receiver. If the plan misses `shutdown_rx.clone()` before storing into the struct, the compile still passes (watch::Receiver is Clone), but **during shutdown the task exit and the tonic server exit become order-dependent** and can race: if reload task sees `changed()` first and drops its Receiver, the server's Receiver still works (channel stays open via `shutdown_tx`). This is probably fine — but not documented.

**Evidence**: Plan lines 2513-2525 don't trace through the order `shutdown_rx.clone() → spawn → return cfg{shutdown_rx}`. The existing `build_external_spawn_config` (app_runtime_launch.rs:1206) already clones shutdown_rx multiple times for cert_watcher + expiry_monitor (mentioned in spawn_config.rs:61-63 doc comment).

**Suggested fix**: Extend Task 4.2 step 1 with:
```rust
// Clone BEFORE moving into the struct.
let shutdown_rx_for_reload = shutdown_rx.clone();
// ... spawn block owns shutdown_rx_for_reload ...
// Caller (below) moves shutdown_rx into the returned cfg.
```
Or make the reference to the existing cert_watcher / expiry_monitor clone pattern explicit — "follow the same clone-before-return pattern used for cert_watcher_shutdown_rx at line NNN".

### I3: Phase 5 dispatch call-site update is under-specified for backward compat

**Location**: plan Task 5.2 (lines 2652-2718)

**Issue**: The plan says "In `DashboardServiceImpl::subscribe_metrics` + `subscribe_events` dispatch: pass `self.streaming_source.clone()` instead of the old pair". But `DashboardServiceImpl` at `crates/oneshim-web/src/grpc/mod.rs` is used by BOTH loopback and external servers. The plan changes the handler **signature** to take `StreamingSource` instead of `(bool, Arc<LoadPolicy>)` — but these handlers (`subscribe_metrics.rs`, `subscribe_events.rs`) are `pub(crate)` to the grpc module. If any external consumer or test helper imports them (grep needed), the change is breaking at the pub(crate) boundary. The plan doesn't include a "grep all call sites" step. Also, `StreamingSource::clone()` is cheap (enum of Arcs), but clones on EACH streaming subscription — acceptable for the low-frequency call, but undocumented.

**Suggested fix**: Add a Step 0.5 to Task 5.2:
```bash
rg "subscribe_metrics\(|subscribe_events\(" crates/oneshim-web/src --type rust
```
Enumerate each call site. Also add a note about clone semantics — each subscription triggers one Arc clone (cheap).

### I4: Task 0.5 backward-compat test covers only serialize, not deserialize with extra unknown fields

**Location**: plan Task 0.5 (lines 665-744)

**Issue**: Test `external_grpc_audit_details_deserialize_old_row_without_grpc_status_code` (line 696-702) covers the absent-field case but not the opposite — what happens when older code reads a row persisted by new code with `grpc_status_code: 7` present? Serde's default behavior is to error on unknown fields UNLESS `#[serde(deny_unknown_fields)]` is absent (it is absent here by default). So this probably "just works" — but no test pins the invariant. If a future refactor adds `deny_unknown_fields`, old callers break silently.

**Suggested fix**: Add one more test:
```rust
#[test]
fn external_grpc_audit_details_deserialize_tolerates_future_unknown_fields() {
    let json = r#"{"auth_type":"jwt","some_future_field":"xyz","grpc_status_code":7}"#;
    let d: ExternalGrpcAuditDetails = serde_json::from_str(json).expect("must tolerate extras");
    assert_eq!(d.grpc_status_code, Some(7));
}
```

### I5: Phase 9 integration test count (18 new) lacks existing test migration audit

**Location**: plan Task 9.1-9.6 (lines 3083-3117) and plan header line 22

**Issue**: Plan states "REPLACE 2 existing, EXTEND 1 existing" in line 120 File Structure table, but Phase 9 itself only mentions "REPLACE `external_grpc_request_id_header_returned` (L933-ish), delete the TODO-stub body" (line 3091). The other 2 REPLACE targets and the 1 EXTEND target are never named. If the existing 19 integration tests have assertions incompatible with the new Header-first grpc-status mapping (e.g., tests that check `AuditStatus::Completed` for what is now a PermissionDenied response), they will break. The plan claims "all 19 existing tests still PASS" at line 3058 — but cannot know this without enumerating.

**Evidence**: plan text says "Expected: all 19 existing tests still PASS" without prior verification; no grep list of tests that depend on `AuditStatus::Completed` / `streaming_enabled` field.

**Suggested fix**: Add a pre-phase step:
```bash
rg "AuditStatus::Completed|streaming_enabled|grpc-status" crates/oneshim-web/tests/external_grpc_integration.rs
```
Enumerate each match. Map each to "still valid" / "needs update". Task 9.1's REPLACE targets must be named up-front.

---

## Minor

### M1: Phase 9 coexistence guard command (line 79) uses `git merge-tree` with wrong argument order

**Location**: plan lines 75-80

**Issue**: `git merge-tree main feature/phase9-tracking-schedule feature/external-grpc-audit-liveconfig` — the modern `git merge-tree` (Git ≥ 2.38) expects `merge-tree [--write-tree] <branch1> <branch2>` (two branches for a virtual merge), not three. This command succeeds silently on older Git or with invalid 3rd arg but produces confusing output.

**Suggested fix**: Replace with:
```bash
git merge-tree --merge-base=main feature/phase9-tracking-schedule feature/external-grpc-audit-liveconfig
```

### M2: Task 1.1 `LiveSnapshot` field visibility — `pub(crate)` would compile-fail for Task 7.1 handler usage

**Location**: plan Task 1.1 (line 1005) and Task 7.1 (line 2863-2864)

**Issue**: `pub(crate) struct LiveSnapshot` (line 1005) makes the fields `streaming_enabled` and `load_policy` invisible to `/api/external-grpc/live-config` handler at `crates/oneshim-web/src/handlers/external_grpc_live_config.rs` ONLY if the handler is in a sibling module of `grpc/external/` — which it is (handlers/ vs grpc/external/). Since both are inside the same crate (`oneshim-web`), `pub(crate)` works. OK. But the test at line 2884-2894 (AppState construction) would fail because `AppState` is in `lib.rs` (different module) — also same crate — also OK. Double-check `LiveExternalConfig::snapshot()` returns `Arc<LiveSnapshot>` — the handler accesses `snap.streaming_enabled` + `snap.load_policy.thresholds()` which requires those fields pub(crate). Currently OK.

**Suggested fix**: No action, but document in Task 1.1: "Fields are `pub(crate)` for cross-module readers within the same crate".

### M3: Task 4.2's anyhow::Context usage conflict with `try_new` error type

**Location**: plan Task 4.2 (line 2502)

**Issue**: `let initial_policy = LoadPolicy::try_new(initial_thresholds).context("Invalid LoadThresholds at boot ...")?` — `context` is an extension trait on `Result<T, E>` from anyhow. `LoadPolicy::try_new` returns `Result<LoadPolicy, LoadPolicyError>`. For `.context(...)` to work, `LoadPolicyError` must impl `std::error::Error + Send + Sync + 'static` — which `thiserror::Error` provides. Check: plan's definition at line 288 uses `#[derive(Debug, thiserror::Error)]` — OK. But the caller `build_external_spawn_config` returns `anyhow::Result<ExternalGrpcSpawnConfig>` — assumed but not verified.

**Suggested fix**: Verify return type at `build_external_spawn_config` is `anyhow::Result<T>`. If not, wrap differently.

### M4: `AuditEntry.details: Option<String>` vs plan test assertions using `.into_string()`

**Location**: plan Task 0.4 (line 590) / Task 0.6 (line 783)

**Issue**: Spot-check of model: `AuditEntry.details: Option<String>` (models/audit.rs:33). Tests at line 585 use `storage.log_entry(AuditEntry { ..., details: "{}".to_string(), ... }).await;` — this assigns `String` to `Option<String>`, which is a **type mismatch** at compile. Task 0.6 test at line 785 also uses `serde_json::from_str(&entry.details)` without `.as_ref().unwrap()`.

**Suggested fix**: Change to `details: Some("{}".to_string())` in all fixture construction and `.as_ref().map(|s| serde_json::from_str(s)).transpose().unwrap()` in test parsing.

---

## Strengths

- **S1: Phase 0 correctly separates port additions from impl**. Task 0.3 adds the trait method + test stubs (NoopAudit, CapturingAudit) in the same commit so no subsequent phase fails to compile. This guards against the "forgotten impl" footgun.
- **S2: Task 0.6 2-sub-commit split (add params → populate)** is explicitly mentioned as a safeguard against broken intermediate state.
- **S3: `LiveSnapshot` single-ArcSwap design** (Task 1.1) correctly rejects the dual-atomic torn-read hazard; the thread-pair torn-read detector test (line 1076-1120) pins the invariant.
- **S4: `ConfigReloadTask` shutdown-biased select** (line 2005) correctly preempts config changes during shutdown. Good hygiene.
- **S5: `serve_external` layer ordering** references memory `reference_tonic_layer_order.md` — this is an explicit acknowledgment of an empirically-discovered fact (tonic 0.14 FIFO-on-ingress).
- **S6: StreamingSource dual-mode (Fixed/Live)** is an elegant ADR-003 compliant abstraction that avoids polluting the loopback path with live-reload machinery.

## Questions raised

1. **Q1**: What happens to in-flight streaming subscriptions when `live.store(new)` flips `streaming_enabled: true → false`? The plan's live.snapshot() reads per-call at subscription start (line 2687) but doesn't specify whether active streams are torn down. Spec §5.8 should define this but plan doesn't test it.
2. **Q2**: `config_reload_task_alive` starts `false` and is set `true` on task entry. There's a window (maybe milliseconds) where the task has spawned but not yet flipped the flag, during which `GET /api/external-grpc/live-config` returns `config_reload_task_alive: false`. Does this constitute a user-visible bug? Phase 9.5 tests should cover this timing.
3. **Q3**: Task 7.1's `AppState` modification (line 2818-2824) adds `Option<Arc<LiveExternalConfig>>`. The `AppState` is likely already Clone + Send + Sync. Verify `LiveExternalConfig` has no `impl !Send` / `!Sync` surprise (it should be fine with `ArcSwap<T: Send + Sync>` + `Arc<LoadPolicy>`).
4. **Q4**: The plan never explicitly lists `LiveSnapshot` in the public re-export hierarchy for tests. If `LiveSnapshot` stays `pub(crate)` inside `grpc/external/live_config.rs` but tests in `tests/external_grpc_integration.rs` (integration binary, outside the crate) want to assert live state, they cannot. The `/api/external-grpc/live-config` REST endpoint is the only integration-test observation channel — good. But this requires the tests to go through HTTP rather than directly asserting on `live.snapshot()`. Make this explicit in Phase 9 test design.
