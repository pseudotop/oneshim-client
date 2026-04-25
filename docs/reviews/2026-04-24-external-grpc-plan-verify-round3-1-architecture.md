# Architecture-Lens Verify Review — Round 3 (Plan rev-3)

**Plan**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` (rev-3, commit `da8b2a26`, 3773 lines)
**Reviewer**: architecture-lens verify
**Prior verdicts**: R1 CONDITIONAL-PASS (2 Critical + 5 Important) → R2 CONDITIONAL-PASS (1 R2 Critical NI1 phantom + 2 R1 Importants I4, I5)

---

## R2-blocker resolution

### R2-NI1: Task 9.0 phantom — **RESOLVED** ✅

Rev-3 L3402 has `### Task 9.0 (NEW — rev-3 R2-C1 fix): CapturingAudit structural rewrite + spawn_server_with_config_manager helper`, correctly placed between `## Phase 9` (L3398) and `### Task 9.1` (L3566). It:

- **Step 1 (L3411-3511)**: Full inline template of the rewritten `CapturingAudit` struct (with `std::sync::Mutex<Vec<CapturedEntry>>` interior mutability per R1-C1 decision) + `CapturedEntry` with `command_id`, `grpc_status_code: Option<u32>`, `execution_time_ms`. `log_complete_with_time` correctly parses `details` JSON for both `result` (status) and `grpc_status_code` separately (addresses D26 raw-code visibility).
- **Step 2 (L3513-3515)**: Calls out the second CapturingAudit at `audit_layer.rs:~L199`. (See New Issue #1 below — concreteness concern.)
- **Step 3 (L3517-3533)**: Adds `spawn_server_with_config_manager` helper to `test_support.rs` with a 10-LoC sketch. (See New Issue #2 below.)
- **Step 4-5**: Compile verification + commit.

**Trait-method coverage** (verified against `crates/oneshim-core/src/ports/audit_log.rs`): the actual `AuditLogPort` trait declares exactly 11 methods (`pending_count`, `recent_entries`, `entries_by_status`, `entries_by_action_prefix`, `stats`, `has_pending_batch`, `log_event`, `log_start_if`, `log_complete_with_time`, `drain_batch`, `drain_all`, `record_session_event` default). The Task 0.3 trait extension adds a 12th: `entries_by_command_id`. The Task 9.0 template at L3440-3495 correctly implements **all 12** (`log_event`, `log_start_if`, `log_complete_with_time`, `pending_count`, `recent_entries`, `entries_by_status`, `entries_by_action_prefix`, `entries_by_command_id`, `stats`, `has_pending_batch`, `drain_batch`, `drain_all`, `record_session_event`). **Will compile.**

### R2-NI2: v32 module-name typo — **RESOLVED** ✅

`grep -n "v32_audit"` across rev-3 returns 7 hits, all spelled `v32_audit_log_command_id_index`. No stale `v32_audit_command_id_index` references remain. L11 explicitly documents the fix.

### R1-I4: unknown-fields test — **RESOLVED** ✅

L999-1013 adds `external_grpc_audit_details_deserialize_tolerates_future_unknown_fields` to Task 0.5 Step 1. Test deserializes JSON with `future_field_foo` + `future_field_baz.nested=true` and asserts clean parse. L1012 comment explicitly justifies no `#[serde(deny_unknown_fields)]`. Correctly addresses forward-compat rollback safety.

### R1-I5: Task 9.1 REPLACE/EXTEND targets — **RESOLVED** ✅

L3573-3578 enumerates by name:
- REPLACE #1: `external_grpc_request_id_header_returned` (~L933)
- REPLACE #2: `external_grpc_audit_denied_for_permission_denied`
- EXTEND #1: `external_grpc_audit_completed_entry_written_after_ok_response` (~L1531)
- 3 NEW tests named explicitly

Implementer now has unambiguous targets; no "find the right test to modify" guesswork.

---

## Prior-verified items — regression check

- **R1-C1** (Task 0.4 audit_log table + `std::sync::Mutex<Connection>` fallible lock): Preserved at L696, L937-939. No regression.
- **R1-C2** (Task 0.6 additive signature, not rewrite): Preserved at L1059 heading "**Additive** param expansion". L13 acknowledges in rev-history. No regression.
- **R2-C2/C3/C4** (ConfigManager real API — sync `update_with`, `with_path` ctor, no `Arc::make_mut`): Preserved at L3617-3643 + L3669-3671 real-API notes block. No regression.

---

## New issues (rev-3 edits)

### N1 (Minor): Step 2 "same rewrite" concreteness ⚠️

L3513-3515: "Apply the same rewrite to `grpc/external/audit_layer.rs` CapturingAudit (~L199) — separate in-module test helper — same shape. Copy the struct + impl block."

`grep` confirms both CapturingAudit locations exist (3 symbols in each file). However, the **in-module** CapturingAudit at `audit_layer.rs:~L199` may have a simpler signature than the integration-test one (it may not need all 12 trait methods if it is only used for local unit tests that exercise a subset). Prescribing a bare "copy the struct + impl block" could lead to an implementer over-engineering or mis-matching the imports (`oneshim_core::models::audit::...` path differs inside `crates/oneshim-web/src/` vs `crates/oneshim-web/tests/`).

**Impact**: Implementer compile-iterates a second time. Non-blocking.
**Suggested (post-merge)**: Add 2-line note: "Audit-layer in-module version must use `crate::grpc::external::...` module paths; import the same 4 types (`AuditLogPort`, `AuditEntry`, `AuditLevel`, `AuditStatus`) via absolute oneshim_core paths."

### N2 (Minor): `spawn_server_with_config_manager` sketch concreteness ⚠️

L3520-3531 provides a 10-LoC sketch but punts the body: "implementer inlines 10-15 LoC from existing spawn_server" and "Exact body mirrors existing `spawn_server` helper — grep to locate."

This is a deliberate design-by-reference — the implementer grep-locates `spawn_server` in test_support.rs and copies its body, substituting the cfg-pull step. Given:
- The real `spawn_server` already exists and is stable
- Task 4.2 (L166) requires `build_external_spawn_config` to accept `config_manager`
- The sketch shows the correct shape

…this is **acceptable** for a plan-level artifact. An implementer with the adjacent existing helper as reference will produce a correct copy.

**Impact**: Non-blocking. Follows rev-2 pattern of delegating mechanical work to implementer.

### N3 (Minor): Step 5 commit file list omits test_support migrations file ✅ (false alarm)

Step 5 L3545-3547 lists 3 files:
- `crates/oneshim-web/tests/external_grpc_integration.rs` (Step 1)
- `crates/oneshim-web/src/grpc/external/audit_layer.rs` (Step 2)
- `crates/oneshim-web/src/grpc/external/test_support.rs` (Step 3)

This matches Steps 1-3 exactly. No Cargo.toml, lib.rs, or module-wiring changes needed since `test_support.rs` is already registered (Task 0.0). Verified **correct**.

### N4 (Cosmetic): Expected-stats line should be updated

L32: "~49 new tests (32 unit + 17 integration)". Task 0.5 adds a 4th unknown-fields test (up from 3), and Task 9.0 Step 4 references existing 19 integration tests. No count update. **Non-blocking** — the numbers in L32 were already approximate.

---

## Phase 9 Task 9.0 architectural fit — verified

- CapturingAudit per-test isolation: `std::sync::Mutex` (sync, short-held) correctly chosen over `tokio::sync::Mutex` — the assertion callers are sync test bodies, and interior mutability is trivial under async trait methods per ADR-001 §2. ✅
- `AuditStatus` parsing at L3497-3510 uses the documented string contract (`"ok"/"denied"/"timeout"/"failed"` → variants). Matches the AuditBridge `record_completion` result field. ✅
- `grpc_status_code` extraction at L3472-3474 uses `.as_u64().map(|u| u as u32)` — no cast overflow risk since tonic `Code::from_i32` inputs are 0-16. ✅
- Task 9.0 is correctly sequenced **before** Task 9.1-9.6 and Task 0.3 (trait extension) runs earlier, so the 12th trait method `entries_by_command_id` exists when Task 9.0 implements it. ✅
- G3 gate test body at L3606-3667 uses `ConfigManager::with_path(PathBuf)` + sync `update_with` closure returning `Result<(), String>` exactly matching the verified real API at L3669-3671. No regression from rev-2's R2-C2/C3/C4 fixes. ✅

---

## Verdict: **PASS** ✅

All R2 blockers resolved. All prior-verified (R1-C1, R1-C2, R2-C2/C3/C4) items preserved without regression. The 2 new minor issues (N1, N2) are plan-level ambiguities that an implementer with grep + the adjacent existing helpers can resolve in seconds; they do not block compilation or Phase 9 test assertions.

Rev-3 is ready to proceed to Loop 3 (implementation). Task 9.0 correctly unblocks Phase 9's `command_id` + `grpc_status_code` assertions, and the G3 convergence test is grounded in the real `ConfigManager` API.

### Follow-ups for implementer (non-blocking)

1. For Task 9.0 Step 2: verify the second CapturingAudit's trait-bound scope before copy-pasting — it may be narrower.
2. For Task 9.0 Step 3: grep `spawn_server\b` in `test_support.rs` and mirror its body verbatim, swapping `AppConfig` seed source for `cfg_mgr.current()`.
3. Update L32 expected-stats count after final test tally.

**Word count**: ~1,020
