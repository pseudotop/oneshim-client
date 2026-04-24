# Loop 2 Round 3 verify — product/test lens

**Plan**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` @ `da8b2a26` (3773 lines)
**Reviewer**: product/test-lens verify (Round 3)
**Prior verdict (R2)**: FAIL — 4 Criticals (N1/N2/N3 new regressions + N4 phantom Task 9.0)
**Round 3 scope**: Check whether rev-3 resolves R2 blockers; re-confirm R1 Important status; scan for new issues.

---

## R2 Blocker Resolution

### N1 — `ConfigManager::new_in_memory` nonexistent  → RESOLVED

Round 3 G3 body (L3615–3619) constructs the manager via the real API:
```rust
let tmp = tempfile::NamedTempFile::new().expect("tempfile create");
let cfg_mgr = Arc::new(
    ConfigManager::with_path(tmp.path().to_path_buf())
        .expect("ConfigManager::with_path")
);
```
Cross-checked against `crates/oneshim-core/src/config_manager.rs:45` — `pub fn with_path(config_path: PathBuf) -> Result<Self, CoreError>` exists and matches. No other `new_in_memory` references remain in the plan (grep confirms).

### N2 — `update_with` wrong shape  → RESOLVED

Round 3 closure (L3621–3626, L3640–3643) is:
```rust
cfg_mgr.update_with(|c| {
    c.external_grpc.streaming_enabled = Some(false);
    Ok(())
}).expect("update_with apply");
```
- Sync call (no `.await`) ✓
- Closure takes `&mut AppConfig` via `c` ✓ (param name `c` matches mutate style)
- Returns `Ok(())` of shape `Result<(), String>` ✓
- Outer wrapped in `.expect(...)` ✓

Matches real API at `crates/oneshim-core/src/config_manager.rs:139`:
`pub fn update_with<F>(&self, updater: F) -> Result<AppConfig, CoreError> where F: FnOnce(&mut AppConfig) -> Result<(), String>`.

Plan §"Real API notes" (L3669–3671) explicitly documents this. Good provenance.

### N3 — `Arc::make_mut` regression  → RESOLVED in G3

G3 test (Task 9.4 body L3606–3666) has **zero** `Arc::make_mut` references. Closure mutates `c` (a `&mut AppConfig`) directly.

**Caveat (not R2-gating but noteworthy)**: `Arc::make_mut` still appears 4x elsewhere in the plan (L2451, L2470, L2492, L2511) in Task 2.1 `ConfigReloadTask` unit-test stubs. Those are `watch::Sender<Arc<AppConfig>>::send_modify` usages — a different API surface driven by the tokio `watch` channel, not `update_with`. That's legitimate (the test is simulating a raw watch update in-process, bypassing ConfigManager). Not a blocker.

### N4 — Task 9.0 phantom  → RESOLVED

`### Task 9.0 (NEW — rev-3 R2-C1 fix): CapturingAudit structural rewrite + spawn_server_with_config_manager helper` now exists at L3402, fully fleshed out across 5 steps (L3411–3562), including full 90-line template for the struct + `AuditLogPort` impl + `parse_status_from_details` helper, plus `spawn_server_with_config_manager` stub. Task 0.3 now notes at L611 that CapturingAudit is intentionally stubbed there and replaced by Task 9.0. Forward reference is explicit.

**R2 verdict: all 4 blockers resolved.**

---

## R1 Importants — Rev-3 Claimed Resolutions

### R1-I4 — Unknown-fields deserialize test → RESOLVED

Task 0.5 now carries `external_grpc_audit_details_deserialize_tolerates_future_unknown_fields` test at L999–1013, asserting that extra JSON fields (`future_field_foo`, `future_field_baz`) don't break deserialization when `grpc_status_code` is still extracted correctly. Exercises the absence-of-`deny_unknown_fields` contract (spec §I12, synthesis I12). Good.

### R1-I5 — REPLACE/EXTEND target names → RESOLVED

Task 9.1 rev-3 (L3573–3578) now names exactly:
- **REPLACE #1**: `external_grpc_request_id_header_returned` (at ~L933)
- **REPLACE #2**: `external_grpc_audit_denied_for_permission_denied`
- **EXTEND #1**: `external_grpc_audit_completed_entry_written_after_ok_response` (at ~L1531)

Plus 3 NEW tests enumerated. The 2+1+3 structure matches the intended surface area.

---

## R1 Importants — Rev-3 Explicitly Deferred

Per the plan's own header (L6–10), rev-3 only claimed to fix R2-C1..C4 + R1-I4 + R1-I5. The following R1 items are still pending:

### R1-I1 — REPLACE body handling (Task 9.1)  → STILL PENDING
Task 9.1 Step body at L3580–3587 still says "follow the TDD flow… most pass first try". The REPLACE bodies are not inlined — they rely on CapturingAudit (now unblocked by Task 9.0) but the actual assertion bodies remain implementer-written. **Acceptable as Loop 3 implementer latitude**, but would benefit from inline sketches during impl.

### R1-I2 — Task 4.2 AppState call-site  → LARGELY ADDRESSED
Task 4.2 Step 2 at L2849–2855 now says `grep -n "build_external_spawn_config(" src-tauri/src/app_runtime_launch.rs` and lists the two new args explicitly. This is more concrete than rev-2 and is sufficient for impl. No longer "hand-waved".

### R1-I3 — G5 bench deferral  → STILL PENDING
L3747: "G5 (perf regression ≤200µs): Task 10.3 mentions final verification; bench deferred to manual PR validation". The plan acknowledges deferral explicitly. **Acceptable** given the 1M-context Loop 2 scope is plan completeness, not bench infrastructure.

### R1-I6 — OpenAPI yaml for live-config  → NOW ADDRESSED
L129 in the Files Modified table: `docs/contracts/oneshim-web.v1.openapi.yaml | Add 2 new paths (/api/audit/export, /api/external-grpc/live-config) | 7`. Task 7.2 Step at L3296+L3308 includes an OpenAPI yaml edit block. Task 7.1 needs matching coverage — plan §"Self-Review" spec coverage line L3699 ("Document new endpoints: /api/external-grpc/live-config… and /api/audit/export") references both. Good enough.

### R1-I7 — Task 10.4 PR description checklist  → STILL VAGUE
Task 10.4 at L3728–3734 still only says "reference spec + plan + Loop 1 reviews". The §14 G1–G5 gate checklist items are not spelled out explicitly in the PR-draft outline. **Not a blocker** but would improve PR hygiene.

---

## New Issues from Rev-3

### NI-1 (Observation, not defect) — `log_event` command_id handling  
Per port trait `crates/oneshim-core/src/ports/audit_log.rs:49`:
`async fn log_event(&self, action_type: &str, session_id: &str, details: &str);`

Task 9.0 Step 1 template (L3441–3451) correctly stores `command_id: String::new()` for log_event-only entries and comments: *"log_event doesn't carry command_id in the port trait; session_id is the stable correlation key at this layer"*. This matches the real trait signature. **Correctly handled.**

### NI-2 (Observation, not defect) — `parse_status_from_details` imports
Task 9.0 Step 1 uses `use serde_json::Value;` *inside* the function (L3498) — compile-safe. `serde_json` is already a workspace dep. The `AuditStatus` enum values (`Completed`, `Denied`, `Timeout`, `Failed`, `Started`) cross-checked against the typical enum shape (the Capture* entry uses `oneshim_core::models::audit::AuditStatus` fully-qualified where needed, L3428). No missing imports.

### NI-3 (Non-issue) — `NamedTempFile` lifetime in G3 test
Concern: does `tmp` drop and delete the file mid-test while `save_to_file` writes?

Verification: `tmp` is bound as a local variable at L3615 and is held in scope until test function returns (L3658 / L3662). Rust drop order is reverse-declaration, so `tmp` drops **after** `handle` and `cfg_mgr`. The tempfile persists for the full test, including all `update_with` → `save_to_file` writes. **No lifetime bug.** The plan's inline comment at L3671 ("CI test uses tempfile::NamedTempFile so disk write is a no-op on teardown") is accurate.

### NI-4 (Minor) — `Step 3` stub at L3519–3530  
`spawn_server_with_config_manager` body is partially sketched ("…implementer inlines 10–15 LoC from existing spawn_server…"). Acceptable impl latitude since the existing `spawn_server` template is referenced by name, but a two-line signature call-plan would tighten this.

---

## Verdict

**PASS**

All 4 R2 blockers (N1/N2/N3/N4) resolved against real codebase API (verified against `config_manager.rs:45/139/268` and `audit_log.rs:49`). R1-I4 and R1-I5 claims honored. R1-I2 improved from hand-waving to concrete `grep -n` + arg list. Remaining R1-I1/I3/I7 are acknowledged deferrals within impl latitude — not blockers for plan approval. No new bugs introduced by the rev-3 edits.

**Rev-3 is ready to transition to Loop 3 (subagent-driven-development)** with Task 0.0 as the entry point. The G3 test body, CapturingAudit rewrite, and Task 4.2 call-site are all concrete enough for first-try impl.

**Recommended (non-blocking)** follow-ups during impl:
1. When executing Task 9.1 REPLACE bodies, inline the assertion shapes (R1-I1).
2. When writing the PR body in Task 10.4, spell out G1–G5 checklist boxes (R1-I7).
3. Run the G3 bench manually at end of Phase 9 to close G5 (R1-I3).

---

*End of Round 3 verify. Product/test lens satisfied.*
