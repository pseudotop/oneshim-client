# Plan Verify Round 2 — Architecture Lens

**Reviewer role**: Architecture (verify gate)
**Plan under verify**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` (rev-2, commit `4bff975e`, 3563 lines)
**Scope**: verify Round-1 resolutions + new regressions from rev-2 edits
**Round**: Loop 2 / Round 2

---

## Prior-finding resolution

### Critical (Round-1)

#### C1 — Task 0.4 wrong crate/impl (AuditLogPort on SqliteStorage)

**Resolved ✅** — Task 0.4 (L681-947) reframed as a `SqliteStorage` **direct sync method** (not port impl). Header states "*SqliteStorage does NOT impl AuditLogPort*" (L689). Codebase-reality table (L685-691) verifies `audit_log` table, `entry_id` column, `std::sync::Mutex<Connection>` fallible lock, `CURRENT_VERSION: u32 = 31`. SQL (L853-859) correct. Migration V32 dispatch (L736-742) mirrors existing `if current < N { v_mod::migrate(conn)?; }` pattern verified against `migration/mod.rs:113-181`. Row-mapper handles real column types (RFC3339 parse, `status: String → AuditStatus` match, `details: Option<String>` via `.ok()`, `execution_time_ms: Option<i64> → u64`). `AuditEntry` struct at `oneshim-core/models/audit.rs:26-34` aligns. No drift.

#### C2 — AuditBridge::record signature rewrite drops params

**Resolved ✅** — Task 0.6 (L1034-1169) explicitly "**Additive** param expansion" with warning "*⚠️ Do not drop any existing parameter.*" (L1055). Step 1 mandates `rg` of all call sites (L1049). New signatures at L1101-1132 preserve the existing 8 args and add `command_id: Option<String>` (both methods) + `grpc_status_code: Option<u32>` (completion only). Commit body (L1154-1168) reinforces "None" backfill policy.

**Minor caveat**: signatures dropped the pre-existing `-> String` return (command_id echo). Since `command_id` is now an input, the echo is redundant — safe but unacknowledged. One-line comment desirable, not blocking.

### Important (Round-1)

#### I1 — `TrailerCapturingBody::new_already_fired` contract

**Partial ⚠️** — The `new_already_fired` ctor is present (L1851) with `signal: None` pattern; test `new_already_fired_drop_is_safe` (L2050-2056) pins the no-panic-on-drop invariant. However, Round-1's specific ask — a test pinning whether a preset `captured: Some(Code::Ok)` is overwritten or preserved by a later trailer frame — is **still absent**. `first_trailer_wins_on_multiple` (L2059-2068) doesn't exercise the `new_already_fired` path. Synthesis I9 committed to an explanatory comment on `captures_permission_denied` but plan body doesn't show that comment being added.

I1 is partially resolved. Minor follow-up, non-blocking.

#### I2 — `shutdown_rx.clone()` timing vs. shutdown semantics

**Resolved ✅** — Task 4.2 Step 1 (L2807-2819) shows explicit `let shutdown_rx_for_reload = shutdown_rx.clone();` → spawn body → later struct-literal construction (L2822 "remove `streaming_enabled`/`load_policy` ... replace with `live`"). Matches cert_watcher/expiry_monitor precedent from `spawn_config.rs:61-63`.

#### I3 — Task 5.2 handler call-site enumeration

**Partial ⚠️** — Task 5.2 Step 1 (L2952-2956) added `grep -n "streaming_enabled\|load_policy" crates/oneshim-web/src/grpc/subscribe_metrics.rs crates/oneshim-web/src/grpc/subscribe_events.rs` — but this is narrower than Round-1's ask (Round-1 wanted `rg "subscribe_metrics\(|subscribe_events\(" crates/oneshim-web/src --type rust` to find all external callers). The integration-test consumers (`grpc_dashboard_integration.rs`) that synthesis I2 committed to also are *not* enumerated in Task 5.2 — they'd appear only if the caller grep ran. Step 3 says "In `DashboardServiceImpl::subscribe_metrics` + `subscribe_events` dispatch: pass `self.streaming_source.clone()` instead of the old pair" — OK for the dispatch method, but doesn't cover tests that directly invoke the handler.

I3 is partially resolved — medium-severity gap.

#### I4 — Task 0.5 future-unknown-fields deserialize test

**Missed ❌** — Synthesis I12 committed to adding `external_grpc_audit_details_deserialize_tolerates_future_unknown_fields`, but Task 0.5 (L951-1030) still shows only three tests (`accepts_grpc_status_code`, `none_field_skipped_in_serialization`, `deserialize_old_row_without_grpc_status_code`). The unknown-fields test is not present.

I4 is missed. Low-severity but factually not addressed.

#### I5 — Phase 9 REPLACE/EXTEND targets unnamed

**Missed ❌** — Plan File Structure (L122) still claims "REPLACE 2 existing, EXTEND 1 existing" but Task 9.1 (L3377-3393) still only names one (`external_grpc_request_id_header_returned`). The 2nd REPLACE target and the 1 EXTEND target remain unnamed. Synthesis I3 promised full replacement bodies + Task 9.0 update for `CapturingAudit` — neither landed in this section.

I5 is missed.

---

## New issues introduced by rev-2

### NI1 (Critical): **Task 9.0 referenced but does not exist**

Plan rev-2 has 3 forward-references to "Task 9.0":

1. Task 0.3 Step 6 (L603): "*CapturingAudit structural update is deferred to Task 9.0*"
2. Task 0.3 commit body (L667): "*Task 9.0 replaces it with a structural update*"
3. Task 9.1 helper note (L3469): "*If `spawn_server_with_config_manager` does not exist, add it in Task 9.0 (test-support expansion).*"

But Phase 9 starts directly at Task 9.1 (L3377) — there is no `### Task 9.0` heading anywhere in the plan (`grep '^### Task' plan.md` confirms: 0.0, 0.1-0.7, 1.1-1.4, 2.1, 3.1, 4.1-4.2, 5.1-5.2, 6.1, 7.1-7.2, 8.1, 9.1, 10.1-10.4 — no 9.0).

This is a direct product of rev-2's edit: synthesis Section 14 (L264) ordered "Task 9.0 (NEW): Update CapturingAudit helper" but the fix never landed as a heading. Task 0.3 and Task 9.1 were both patched to reference it, creating three dangling pointers.

**Impact**: The defer-to-Task-9.0 clause in Task 0.3 Step 6 is load-bearing — if `CapturingAudit` (used at `external_grpc_integration.rs:1447` + `audit_layer.rs:199`) gets only a `vec![]` stub in Phase 0 and the structural rewrite never happens, the integration tests lose `grpc_status_code` capture (spec §5.5 D26). Implementation will either stall at Phase 9 ("where's Task 9.0?") or diverge from spec.

**Fix required**: Add `### Task 9.0: CapturingAudit structural update + spawn_server_with_config_manager helper` section between Phase 9 heading (L3373) and Task 9.1 (L3377), with the full replacement body that preserves real `command_id` from the `record`/`record_completion` args and captures `grpc_status_code` by parsing the details JSON.

### NI2 (Important): **Task 0.4 dispatch reference (Step 2) points to still-generic pattern — no line cite**

Task 0.4 Step 2 (L726-729) says "Register in `crates/oneshim-storage/src/migration/mod.rs`: ... Add the migration call in the version-dispatch match arm" but doesn't cite the insertion line. Step 3 (L731-743) expands this with the full dispatch snippet `if current < 32 { v32_audit_log_command_id_index::migrate(conn)?; }` — this is syntactically correct per the existing pattern (verified at `migration/mod.rs:113-181`), but the filename/module naming drifts between Steps 2 and 3: Step 2 says `mod v32_audit_command_id_index;` (L727), Step 3 says `mod v32_audit_log_command_id_index;` (L734) — the trailing hyphen-segment differs (`audit_command_id_index` vs. `audit_log_command_id_index`). Commit command at L921-923 uses `v32_audit_log_command_id_index.rs` (matching Step 3). Step 2's module name is a typo that an implementer might paste.

**Fix required**: Edit Step 2 to use `mod v32_audit_log_command_id_index;` consistently.

### NI3 (Minor): **Task 0.3 Step 7 test `log_start_if` signature verified OK**

I verified Task 0.3 Step 7 tests (L609-632). `AuditLogger::new(100, 10)` matches the real `pub fn new(max_buffer_size: usize, batch_size: usize) -> Self` at `audit.rs:38`. `log_start_if(AuditLevel::Basic, "cmd-X", "s1", "act1")` matches the real `pub fn log_start_if(level: AuditLevel, command_id, session_id, action_type)` at `audit.rs:125`. The `AuditLogAdapter::new(logger)` ctor at L628 matches `audit.rs:326`. No compile issues.

### NI4 (Minor): **Task 0.4 Step 6 row-mapper imports are plan-only**

The row-mapper inside Task 0.4 Step 6 (L869-895) uses `use oneshim_core::models::audit::{AuditEntry, AuditStatus};` at L848 inside the method body — that's fine Rust, but the `save_audit_entry` existing method at `sqlite/mod.rs:255` uses `format!("{:?}", entry.status)` to serialize status (Debug format, L261). The plan's `match status_str.as_str() { "Completed" => AuditStatus::Completed, … }` reverses this, but falls back to `_ => AuditStatus::Completed` (L882) on any unknown string. That's lenient but silently loses `AuditStatus::Started/Denied/Timeout/Failed` if the Debug derive changes in the future. A slightly safer `_ => { tracing::warn!("unknown audit status: {status_str}"); AuditStatus::Completed }` pattern would be preferable.

**Fix**: Minor polish — add warn log for unknown status strings.

---

## Additional consistency spot-checks

- **Task 0.0 → later tasks compatibility**: ✅ OK. Task 0.0 creates `crates/oneshim-web/src/grpc/external/test_support.rs` gated behind `#[cfg(any(test, feature = "test-support"))]`. Task 0.6 test at L1062 references `fixture_bridge()` without import; assumed to be a re-exported helper from the new test_support module. Compile ordering is intact.
- **Task 2.1 `watch::Ref` drop**: L2312-2313 (`apply_config(&live, &config_rx.borrow_and_update())` + "*Ref dropped at end of statement*" comment) — correctly addresses synthesis M10.
- **Task 4.2 LoadPolicy::try_new anyhow::Context**: L2795-2796 — `LoadPolicyError` is `thiserror::Error`, so `.context()` works; Synthesis M3 resolved.

---

## Verdict

**CONDITIONAL-PASS**

Rev-2 resolves all 2 Round-1 Criticals (C1, C2) plus 1 of 5 Round-1 Importants (I2) fully. I1 and I3 are partially resolved. I4 and I5 are missed.

Rev-2 also introduces **1 new Critical (NI1: Task 9.0 phantom reference)** which must be fixed before implementation — three call sites in the plan assume it exists, but it doesn't. This is a drop-in fix (add the `### Task 9.0` section at L3375 area per synthesis §14 step 14) that could be merged in rev-3 alongside the I4/I5 gaps.

**Blockers for PASS**:
1. Add Task 9.0 section (NI1 — the most structurally load-bearing)
2. Name the 2 REPLACE + 1 EXTEND targets in Task 9.1 (I5)
3. Add unknown-fields serde test (I4)
4. Fix Task 0.4 Step 2 typo `v32_audit_command_id_index` → `v32_audit_log_command_id_index` (NI2)

**Non-blockers (defer to rev-3 if needed)**:
- I1 comment on `captures_permission_denied` (already captured in synthesis I9)
- I3 broader Task 5.2 call-site grep (synthesis I2)
- NI3/NI4 polish items

The rev-2 plan is substantively sound — architecture, hexagonal-conformance, concurrency, migration dispatch, `shutdown_rx` flow are all correctly specified. The failures are gaps in completeness, not errors in direction.

---

*End of Round-2 architecture verify. Await rev-3 or hand off to final synthesis.*
