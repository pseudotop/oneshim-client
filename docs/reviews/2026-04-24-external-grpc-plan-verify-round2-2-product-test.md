# Plan Verify — Loop 2 Round 2 — Product & Test Lens

**Plan**: rev-2, commit `4bff975e`, 3563 lines.
**Round-1 findings**: 3 Critical + 7 Important.

---

## Prior findings disposition

### Critical

#### C1. Task 0.4 test unrunnable (wrong fields + nonexistent `log_entry`) — **RESOLVED**

Task 0.4 rewritten (L681-947). Test body (L748-821) uses correct fields: `entry_id`, `details: None`, `execution_time_ms: Some(10)`, writes via `storage.save_audit_entry(&entry)` (the real method). Also corrects the table (`audit_log`), column (`entry_id`), and Mutex API (`std::sync::Mutex` fallible `lock()`). The `AuditStatus::Completed` match-arm set is reasonable. Migration V32 with `CURRENT_VERSION` bump shown; Phase 9 collision check is in Step 1.

#### C2. Undefined helpers (`fixture_bridge`, `InnerEcho`, `PeerInfo::fixture`) — **PARTIAL**

Task 0.0 (L130-226) is new per CR4, with a genuine inventory → scaffold → compile-check cycle. `InnerEcho` has a 10-line skeleton inline (L177-192) and the commit covers the rest by reference ("Body impl + Service impl — ~40 LoC; see trailer_body::tests::FixtureBody for pattern"). **But** `fixture_bridge`, `fixture_metrics`, `PeerInfo::fixture`, `PassthroughInner`, `spawn_server_with_config_manager` each get only a naming reference — no bodies, no signatures. The Step 3 rubric says "Similar minimal impls for remaining helpers. For `fixture_bridge` / `fixture_metrics` — return `(AuditBridge, MockRecorder)` tuples that capture calls for assertion." That's still pseudocode for ~5 of the 8 helpers. Implementer must synthesize — but at least the scaffolding task explicitly exists now and feature-flags the module. Downgrades C2 from blocker to Important.

#### C3. G3 ≤1s test body absent — **RESOLVED with new bugs (see new issues N1-N3 below)**

Task 9.4 G3 test body now inline (L3412-3461). Contains `#[tokio::test]`, `Instant::now()`, polling loop, 1s timeout panic, abort. Structure is correct. However, the inline code uses APIs that don't exist on the real `ConfigManager` — see N1-N3.

### Important

- **I1** (REPLACE/EXTEND body handling): Task 9.1 (L3377-3393) still says "implement the 4 tests listed in spec §9.2" — same "transcribe from spec" offloading as before. One commit for 4 tests, no inline bodies. Related CapturingAudit structural update deferred to the phantom "Task 9.0" (see N4). **NOT addressed.**
- **I2** (Task 4.2 AppState population): Task 7.1 Step 1 (L3111-3120) still says "Populate these fields from `build_external_spawn_config` in `src-tauri/src/app_runtime_launch.rs` after constructing `live` and `metrics_arc`" — no exact line, no diff context, no show of `.external_grpc_live = Some(live.clone())` write at the call site. Task 4.2 (L2773-2861) never writes to AppState. **NOT addressed.**
- **I3** (G5 bench deferral/gating): Self-Review L3537 still says "bench deferred to manual PR validation" with no Task 10.3b. **NOT addressed.**
- **I4** (Task 5.2 grpc_dashboard_integration.rs consumer audit): Task 5.2 Step 1 (L2952-2958) greps only the two handler files; doesn't mention `grpc_dashboard_integration.rs`. Task 5.1 has no global grep. **NOT addressed.**
- **I5** (Task 0.6 call-site enumeration): Task 0.6 Step 1 (L1041-1055) now explicitly runs `rg 'bridge\.(record|record_completion)\('` and says "list every caller file:line" verbatim. **RESOLVED.**
- **I6** (Task 7.1 OpenAPI yaml): Task 7.1 (L3099-3259) adds NO OpenAPI entry for `/api/external-grpc/live-config`. File-structure table L121 still lists both paths as modified, but only Task 7.2 (L3283-3307) includes yaml. **NOT addressed.**
- **I7** (Task 10.4 PR description §14 checklist): Task 10.4 (L3518-3524) is 6 bullets; no spec §14 11-criteria verbatim, no mention of memory update, no merge-tree output capture, no line-171 correction sub-task. **NOT addressed.**

**Important scorecard**: 1/7 resolved. Six still pending.

---

## New issues introduced or surfaced in rev-2

### N1. (Critical) Task 9.4 G3 test uses `ConfigManager::new_in_memory` which does not exist

L3422: `let cfg_mgr = Arc::new(ConfigManager::new_in_memory(Arc::new(cfg)));`.

Grep of `crates/oneshim-core/src/config_manager.rs` shows only one public constructor: `ConfigManager::new()` (L40, returns `Result<Self, CoreError>`), which loads from the default disk path. There is no `new_in_memory`, no `new_with_path`, no in-memory variant. The helper `spawn_server_with_config_manager` (L3424) can't work around this because the real `update_with` (L139) calls `Self::save_to_file(&self.inner.config_path, &new_cfg)` unconditionally — an in-memory ConfigManager is not a no-op extension.

**Fix**: either
(a) Task 0.0 adds a `ConfigManager::new_with_path(path)` (plus an `#[cfg(test)]` constructor that writes to a `tempfile::NamedTempFile`), OR
(b) introduce a port-layer `ConfigWatcher` trait the test mocks, OR
(c) use `tempfile::tempdir()` + real file-backed `ConfigManager` + accept disk writes in CI.

### N2. (Critical) Task 9.4 G3 test uses `cfg_mgr.update_with(|c| { Arc::make_mut(c)... }).await`

Two independent defects on one line (L3435-3437):

1. **`.await` on a non-Future**: `update_with` returns `Result<AppConfig, CoreError>`, not a Future. This won't compile.
2. **`Arc::make_mut(c)`**: the real closure signature is `FnOnce(&mut AppConfig) -> Result<(), String>`. `c` is `&mut AppConfig`, not `&mut Arc<AppConfig>`. `Arc::make_mut(c)` won't typecheck. The closure body also doesn't return `Result<(), String>` — another compile error.

**Fix**:
```rust
cfg_mgr.update_with(|c| {
    c.external_grpc.streaming_enabled = Some(false);
    Ok(())
})?;
```
(note: sync, returns `Result`, closure body returns `Ok(())`).

### N3. (Important) Round-1 M1's `Arc::make_mut(c)` trick resurfaces in Task 9.4 after being "fixed" in Task 2.1

Synthesis M5 said "Task 2.1 `Arc::make_mut(c)` no-op trick → proper mutation." Task 2.1 was presumably fixed — but the same defective pattern was then introduced into Task 9.4's brand-new G3 test body. Author mis-applied the pattern rather than understanding why it was wrong. This is a process signal: the CR5 G3 fixer did not read the synthesis M5 note.

### N4. (Critical) Three dangling "Task 9.0" references, but Task 9.0 no longer exists

The synthesis §"Fixer plan" item #14 proposed a new **Task 9.0**: "Update CapturingAudit helper (preserve command_id, capture grpc_status_code)." Rev-2 also added a new **Task 0.0** (test-support helpers) per item #2. Both items were executed, but the plan body references "Task 9.0" in three locations that were never reconciled:

- L603: "Special case — `CapturingAudit` structural update is **deferred to Task 9.0**"
- L667: commit message says "CapturingAudit gets stub here for compile correctness; **Task 9.0** replaces it"
- L3469: "If `spawn_server_with_config_manager` does not exist, **add it in Task 9.0**"

No task labeled "Task 9.0" exists in the plan. Phase 9 begins at L3375 with Task 9.1. So (a) the structural CapturingAudit rewrite has no owner; (b) `spawn_server_with_config_manager` has no owner; (c) the fix work for the G3 test's helper gap points to a nonexistent task. The L3469 reference even lands inside Task 9.4's own body — the implementer reading top-to-bottom will hit "add it in Task 9.0" and have nowhere to go.

Given that Task 9.4's G3 test body also needs `spawn_server_with_config_manager`, this defect compounds with N1/N2 — the G3 test is structurally undeliverable without a Task 9.0 that doesn't exist.

**Fix**: either renumber Task 0.0 → Task 9.0, OR explicitly fold CapturingAudit rewrite and `spawn_server_with_config_manager` into Task 0.0 and update all three dangling references to point to "Task 0.0."

### N5. (Important) Task 0.3 `CapturingAudit` stub + "Task 9.0 replaces it" breaks Task 6.1 compile order

Task 0.3 Step 6 (L586-603) adds a `vec![]` stub for `CapturingAudit::entries_by_command_id`. "Task 9.0 replaces it with a structural update preserving real `command_id`." Task 6.1 (from rev-1 — not shown here, but referenced at plan L2740+) asserts on `CapturingAudit` entries including `command_id == "req-abc-123"`. If the CapturingAudit rewrite landing is deferred to the phantom Task 9.0, Task 6.1's assertion cannot pass at the time Task 6.1 is committed. This creates out-of-order TDD failure.

---

## Verdict

**FAIL** for rev-2.

Summary:
- C1 resolved; C3 replaced by 3 new compile errors (N1/N2/N4) in the same test body it was supposed to fix.
- C2 downgraded to Important but partial — most helper bodies still pseudocode.
- 6 of 7 prior Importants untouched (I1/I2/I3/I4/I6/I7); only I5 resolved.
- 4 new issues, 3 Critical (N1/N2/N4) and 1 Critical-adjacent (N5).

The G3 test body — the single most load-bearing artifact in this plan, the only acceptance gate on the CI-enforced 1s SLO — won't compile. The fix introduced pattern regressions that the Round-1 review already flagged (M5 `Arc::make_mut` misuse). Three Task 9.0 references are dangling; the CapturingAudit rewrite has no owner; the `spawn_server_with_config_manager` helper has no construction path.

Rev-3 must:
1. **Fix Task 9.4 G3 test API usage** (N1, N2): replace `new_in_memory` + `Arc::make_mut(c).await` with real `ConfigManager` API (sync `update_with(|c| { c.field = ...; Ok(()) })` + tempdir-backed path).
2. **Resolve the Task 9.0 ghost** (N4): renumber or fold into Task 0.0; update all three call sites.
3. **Address surviving Importants** (I1/I2/I3/I4/I6/I7) — especially I2 (Task 4.2/7.1 AppState population site — this gate invisibly disables the entire live-config feature if missed) and I6 (OpenAPI yaml — enforced by workspace lint per CLAUDE.md).
4. **Complete C2 scaffolding**: inline the ~40 LoC each for `fixture_bridge`, `fixture_metrics`, `PeerInfo::fixture`, `PassthroughInner`, and `spawn_server_with_config_manager` in Task 0.0.

---

*End of Loop 2 Round 2 Product-Test verify. ~1470 words.*
