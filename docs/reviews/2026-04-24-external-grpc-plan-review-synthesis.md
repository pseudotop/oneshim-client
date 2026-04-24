# Plan Review Synthesis — Loop 2 Round 1

**Plan under review**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` (rev-1, commit `6bd654ff`)
**Reviews consolidated**:
- Review 1 (Architecture): 2C + 5I + 4M + 4Q
- Review 2 (Product/Test): 3C + 7I + 4M + 5Q
- Review 3 (Platform/Risk): 2C + 4I + 5M + 3Q

**Raw totals**: 7C + 16I + 13M + 12Q
**Consolidated (after dedup)**: **5 Critical, 13 Important, 11 Minor**

**Verdict**: **REWORK REQUIRED** — plan rev-1 has structural errors that prevent compilation/runtime success. Rev-2 must correct the audit-storage architecture model + signature-expansion semantics + test-helper scaffolding.

---

## Verified code reality (confirmed in rev-2 prep scan)

| Question | Answer | Evidence |
|---------|--------|----------|
| Where is `AuditLogPort` implemented for production? | `oneshim-automation::AuditLogAdapter` (wraps AuditLogger with VecDeque buffering + batched SqliteStorage persistence) | `crates/oneshim-automation/src/audit.rs:337` |
| What table stores audit entries? | **`audit_log`** (not `audit_entries`) | `crates/oneshim-storage/src/migration/v25.rs:6` |
| What is the primary-key column? | **`entry_id`** (not `id`) | Same file, L73 |
| Does SqliteStorage impl AuditLogPort directly? | **No** — exposes `save_audit_entry(&self, entry)` as a direct method | `crates/oneshim-storage/src/sqlite/mod.rs:255` |
| What Mutex wraps the connection? | **`std::sync::Mutex<Connection>`** (not `parking_lot::Mutex`) | Same file, L82 |
| What's the existing read-path pattern? | Synchronous `let Ok(conn) = self.conn.lock() else { ... }` — no `spawn_blocking` | Same file, L256 |
| Which crates currently have `impl AuditLogPort`? | 7 sites (1 prod + 6 test doubles): `oneshim-automation::AuditLogAdapter` (prod); test doubles: `NoopAudit` ×4 (`external_grpc_integration.rs:95`, `grpc/mod.rs:535`, `grpc/external/auth_layer.rs:331`, `grpc/external/spawn_config.rs:116`), `CapturingAudit` ×2 (`external_grpc_integration.rs:1447`, `grpc/external/audit_layer.rs:199`), `MockAuditLog` ×1 (`grpc/external/audit_bridge.rs:199`) | `grep -rn "impl AuditLogPort\|impl.*AuditLogPort"` |

**Impact on plan rev-1**:
- Plan Task 0.4's SQL, migration, and Mutex API are wrong in every detail
- Plan Task 0.3's NoopAudit/CapturingAudit update is incomplete (4+2+1 = 7 sites, plan touches 2-3)
- Plan Task 3.1's `fixture_bridge()` / `InnerEcho` / `PeerInfo::fixture()` helpers don't exist and plan doesn't create them

---

## Consolidated Critical issues

### CR1: Task 0.4 SQL + migration target the wrong table/column/Mutex
**Sources**: Arch-C1, Product-C1, Platform-C1, Platform-C2

**Defect**:
- Plan uses table `audit_entries` → actual is `audit_log`
- Plan uses column `id` → actual is `entry_id`
- Plan uses `parking_lot::Mutex` API (`let conn = conn.lock();`) → actual is `std::sync::Mutex` (fallible `lock()` returning `LockResult`)
- Plan wraps in `tokio::task::spawn_blocking` → existing read paths are synchronous

**Resolution**: Complete rewrite of Task 0.4. Concrete target:

```rust
// crates/oneshim-storage/src/sqlite/mod.rs — NEW method on SqliteStorage
pub fn entries_by_command_id(&self, command_id: &str, limit: usize) -> Vec<AuditEntry> {
    let Ok(conn) = self.conn.lock() else {
        tracing::warn!("audit: entries_by_command_id failed to acquire SQLite lock");
        return Vec::new();
    };
    let mut stmt = match conn.prepare(
        "SELECT entry_id, timestamp, session_id, command_id, action_type, status,
                details, execution_time_ms
         FROM audit_log
         WHERE command_id = ?1
         ORDER BY timestamp DESC
         LIMIT ?2"
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(err = %e, "audit: prepare failed");
            return Vec::new();
        }
    };
    let rows = stmt.query_map(
        rusqlite::params![command_id, limit as i64],
        /* use existing map_audit_row helper from save/read path */
    );
    match rows {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            tracing::warn!(err = %e, "audit: query_map failed");
            Vec::new()
        }
    }
}
```

Migration V32: `CREATE INDEX IF NOT EXISTS idx_audit_log_command_id ON audit_log (command_id) WHERE command_id IS NOT NULL;`

### CR2: Task 0.3 missing `AuditLogAdapter` impl + incomplete test-double inventory
**Sources**: Arch-C1 (part), Product-C1 (part)

**Defect**: Adding `entries_by_command_id` to `AuditLogPort` trait is only half the work. Plan must also:
1. Add impl on `oneshim-automation::AuditLogAdapter` that delegates to the backing SqliteStorage
2. Update **all 7** test-double impls (plan lists 2-3)

**Resolution**: Expand Task 0.3:
```rust
// crates/oneshim-automation/src/audit.rs — AuditLogAdapter impl
async fn entries_by_command_id(&self, command_id: &str, limit: usize) -> Vec<AuditEntry> {
    let storage = self.storage.clone();  // Arc<SqliteStorage>
    let cmd_id = command_id.to_string();
    tokio::task::spawn_blocking(move || storage.entries_by_command_id(&cmd_id, limit))
        .await
        .unwrap_or_default()
}
```

(The Adapter layer is async and storage is sync, so spawn_blocking is correct *here* — not in SqliteStorage itself.)

All 7 test-double impls: `vec![]` default body.

### CR3: Task 0.6 AuditBridge::record signature rewrite drops existing params
**Sources**: Arch-C2

**Defect**: Plan L766-789 shows a full-signature rewrite that omits the existing `result`, `request_size`, `response_size` parameters (and the `-> String` return). This breaks all existing callers during the intermediate commit.

**Resolution**: Change Task 0.6 semantics from "rewrite" to "ADD params":
- Read the current signature (existing 7 args) via `grep -n "pub async fn record\b"` on `audit_bridge.rs`
- Add exactly 2 new params at the end (`command_id: Option<String>`, and for record_completion only `grpc_status_code: Option<u32>`)
- Update existing callers to pass `None` for both new args — surgical, no field drops

### CR4: Test-support helpers (`fixture_bridge`, `InnerEcho`, `PeerInfo::fixture`) don't exist
**Sources**: Product-C2, Product-C3

**Defect**: Tasks 0.6, 3.1, 6.1 reference test helpers that don't exist. TDD flow fails at compile.

**Resolution**: Add **Task 0.0 "Test-support helpers"** before all others (or consolidate into Phase 0). Create a helper module `crates/oneshim-web/src/grpc/external/test_support.rs` (or extend existing) with:
- `fn fixture_auth_context() -> AuthContext`
- `fn fixture_peer_info() -> PeerInfo`
- `struct InnerEcho { ... }` with helpers `with_trailer_status(i32)` and `trailers_only_with_status(i32)` — returns `Response<BoxBody>` shaped appropriately
- `struct CapturingAudit { ... }` with `entries()` accessor that preserves real command_id + captures grpc_status_code from details JSON

Gate these under `#[cfg(any(test, feature = "test-support"))]`. Spec should reference this module from §9.1.

### CR5: Task 9.4 G3 convergence integration test body absent
**Sources**: Product-C3

**Defect**: Task 9.4 lists 6 tests but gives no code. The flagship `external_grpc_live_streaming_toggle_reflects_within_1s` (G3 gate) needs:
- Client call (`SubscribeMetrics` stream open → check initial Unavailable return)
- ConfigManager trigger mechanism (`ConfigManager::update_with` with in-memory transient config, NOT disk-write in CI)
- `start.elapsed()` assertion bound

**Resolution**: Task 9.4 needs full test code inline. Target skeleton:
```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_live_streaming_toggle_reflects_within_1s() {
    let cfg_mgr = ConfigManager::new_in_memory(initial_cfg());
    let (handle, port) = spawn_server_with_config_manager(cfg_mgr.clone()).await;

    // Initial: streaming enabled → SubscribeMetrics succeeds
    let channel = connect(port).await;
    let mut client = DashboardServiceClient::new(channel.clone());
    let resp = client.subscribe_metrics(req_with_valid_auth()).await.expect("initial call OK");
    drop(resp);

    // Toggle to disabled
    let start = std::time::Instant::now();
    cfg_mgr.update_with(|c| { c.external_grpc.streaming_enabled = Some(false); }).await;

    // Poll until next call returns Unavailable OR timeout
    loop {
        let result = client.subscribe_metrics(req_with_valid_auth()).await;
        if result.is_err() && result.unwrap_err().code() == tonic::Code::Unavailable {
            break;
        }
        if start.elapsed() > std::time::Duration::from_secs(1) {
            panic!("convergence > 1s; G3 CI-bound violated");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    handle.abort();
}
```

---

## Consolidated Important issues (13)

### I1: Task 0.6 missing call-site enumeration (Product-I5)
**Fix**: Task 0.6 Step 1 runs `rg 'bridge\.(record|record_completion)\(' crates/oneshim-web/src/grpc/external/ --line-number` and lists every match; Step 4 updates each to pass `None` for new args before commit.

### I2: Task 5.2 missing `grpc_dashboard_integration.rs` consumer check (Product-I4)
**Fix**: Task 5.1 Step 2 adds `grep -rn "streaming_enabled:\|load_policy:" crates/ src-tauri/` and lists each hit as an explicit "Files" entry.

### I3: Task 9.1 REPLACE bodies undefined; CapturingAudit structural update missing (Product-I1)
**Fix**: Add Task 9.0 "Update CapturingAudit helper to preserve command_id and capture grpc_status_code" with full rewrite. Task 9.1 shows the full replacement body for `external_grpc_request_id_header_returned`.

### I4: Task 4.2 AppState population site hand-waved (Product-I2)
**Fix**: Task 4.2 Step 3 shows the exact insertion point in `app_runtime_launch.rs` around the call site (~L897), including surrounding context, the `AppState.external_grpc_live = Some(live.clone())` write, and `AppState.external_grpc_metrics = Some(metrics_arc.clone())` write.

### I5: Task 10.3 G5 perf bench deferred without a gating mechanism (Product-I3)
**Fix**: Either add Task 10.3b "Run bench and attach numbers to PR" with exact command, OR explicitly demote G5 to non-gating in plan Self-Review (with a note to update the spec).

### I6: Phase 9 migration version collision risk (Platform-I2)
**Fix**: Add `scripts/check_migration_versions.sh` run in plan §"Phase 9 coexistence guard" that greps migration version on `feature/phase9-tracking-schedule`. Reserve V32 via early-land or bump to V33 based on Phase 9 state at consume time.

### I7: Task 7.1 OpenAPI yaml entry missing (Product-I6)
**Fix**: Task 7.1 Step 3 adds the yaml snippet for `/api/external-grpc/live-config` alongside the route registration, analogous to Task 7.2's audit export yaml.

### I8: Deferred audit gauge RAII guard not specified (Platform-I4)
**Fix**: Task 3.1 Step 3 shows the `DeferredGuard(Arc<ExternalMetrics>)` struct + Drop impl that decrements the gauge — survives panic/cancellation.

### I9: `new_already_fired` production case unpinned for mid-stream trailers (Arch-I1)
**Fix**: Task 1.3 Step 4 adds explanatory comment to `captures_permission_denied` that it exercises `poll_frame` path; production uses header-first fast path. Prevents future "redundant test" cleanup.

### I10: Task 10.4 PR description deliverable — plan §14 criteria checklist missing (Product-I7)
**Fix**: Task 10.4 includes the 11 spec-§14 criteria verbatim as a PR-description skeleton with checkboxes. Add separate sub-tasks for:
- `reference_tonic_layer_order.md` memory update (bullet 4)
- Phase 9 merge-tree check logged (bullet 11)
- Doc line-171 correction (bullet 5.a)

### I11: `shutdown_rx.clone()` timing in `build_external_spawn_config` (Arch-I2)
**Fix**: Task 4.2 shows exactly where `shutdown_rx` is cloned relative to its source — must be the same Sender used by cert-watcher + expiry-monitor spawn sites.

### I12: Task 0.5 serde backward-compat coverage gap (Arch-I4)
**Fix**: Task 0.5 adds test `deserialize_tolerates_future_unknown_fields` — confirms deserializer doesn't error on JSON containing unknown keys (for forward-compat on schema evolution).

### I13: Task 1.2 extension-populated assertion missing (Product-M4 → I-class per impact)
**Fix**: Task 1.2 adds test `inner_service_sees_request_id_in_extensions` — wraps inner service with an assertion that `req.extensions().get::<RequestId>().is_some()`.

---

## Minor issues (11) — batch polish

Addressed via targeted edits in rev-2:
- **M1 (Arch)**: Git merge-tree arg ordering — invert to match git docs convention
- **M2 (Arch)**: `pub(crate)` visibility notes for new types
- **M3 (Arch)**: `anyhow::Context` compat check in Task 4.2
- **M4 (Arch)**: AuditEntry `details: Option<String>` fixture updates
- **M5 (Product)**: Task 2.1 `Arc::make_mut(c)` no-op trick → proper mutation
- **M6 (Product)**: Task 1.3 `first_trailer_wins_on_multiple` rename or upgrade fixture
- **M7 (Product)**: Plan test count (49) vs spec (66) reconcile — update plan header
- **M8 (Platform)**: `http-body` + `pin-project-lite` direct-dep add in oneshim-web/Cargo.toml (not just verify-transitive)
- **M9 (Platform)**: `Arc::clone(&self.conn)` turbofish for readability
- **M10 (Platform)**: `watch::Ref` drop-across-await lint hint
- **M11 (Platform)**: HeaderValue double-validation rationale comment

---

## Open questions (non-blocking, flagged for Round-2)

- **OQ-P1**: Does `TrailerCapturingBody::new_already_fired` have a compile-time proof against `tonic::body::Body`? Plan shows only a generic assertion.
- **OQ-P2**: Runtime-shutdown dropping deferred audit row — explicit test not in plan. Accept per spec §8.2.
- **OQ-P3**: Mutex contention between save path and read path — pre-existing characteristic, document in Task 0.4 commit message.
- **OQ-P4**: How is `load_policy_snapshot_summary` Debug format versioned for downstream log parsers?
- **OQ-P5**: `external_grpc_live_load_thresholds_applied_without_warmup_reset` uses `tokio::time::pause`? Plan silent on 30s wait strategy.

---

## Fixer plan — plan rev-2 structure

**Section-by-section edit plan** (applied in one commit unless test-infra requires two):

1. **File Structure table (plan L~100)**: Add `test_support.rs` (new helper module) as an item. Update Phase 0 file count.
2. **Task 0.0 (NEW)**: "Test-support helpers" — creates `fixture_auth_context()`, `fixture_peer_info()`, `InnerEcho`, `CapturingAudit` helper. Full body inline.
3. **Task 0.3**: Clarify — add method to port trait, impl in `AuditLogAdapter` (in oneshim-automation), update 7 test-double impls (enumerated with file:line).
4. **Task 0.4**: Complete rewrite — target `SqliteStorage::entries_by_command_id` direct method (not impl AuditLogPort); use `audit_log` table, `entry_id` column, `std::sync::Mutex`, synchronous pattern matching existing `save_audit_entry`.
5. **Task 0.5**: Add `deserialize_tolerates_future_unknown_fields` test.
6. **Task 0.6**: Semantics change — "ADD 2 params" not "rewrite". Step 1 enumerates call sites via `rg`.
7. **Task 1.2**: Add `inner_service_sees_request_id_in_extensions` test.
8. **Task 1.3**: Add explanatory comment on `captures_permission_denied`. Fix `first_trailer_wins_on_multiple` to actually test multiple trailers.
9. **Task 2.1**: Replace `Arc::make_mut(c)` no-op with proper field mutation.
10. **Task 3.1**: Full code inline (don't defer to spec L559-688). Add `DeferredGuard` RAII pattern explicitly.
11. **Task 4.2**: AppState population site — exact code at `app_runtime_launch.rs:~L897`.
12. **Task 5.1/5.2**: Global grep before signature change; enumerate `grpc_dashboard_integration.rs` consumers.
13. **Task 7.1**: Add OpenAPI yaml snippet for `/api/external-grpc/live-config`.
14. **Task 9.0 (NEW)**: Update CapturingAudit helper (preserve command_id, capture grpc_status_code).
15. **Task 9.1-9.4**: Complete test bodies inline. G3 test gets full code per CR5 resolution.
16. **Task 10.3b (NEW)**: G5 bench requirement OR Self-Review demotes to non-gating.
17. **Task 10.4**: Verbatim spec §14 checklist as PR description skeleton.
18. **Phase 9 coexistence guard**: Add `check_migration_versions.sh` step.
19. **Global Conventions section**: Note test-support helper module location.
20. **Plan Self-Review**: Update test count reconciliation (49→66); mark G5 gating decision.

---

## Expected plan rev-2 stats

- Line growth: +300-500 lines (new Task 0.0 + Task 9.0 + expanded Task 3.1 + 9.4 inline bodies)
- Task count: 30 → 32 (+2 new tasks)
- Phase count: unchanged (10)

---

*End of synthesis. Fixer phase next — apply 20 edits, commit as rev-2.*
