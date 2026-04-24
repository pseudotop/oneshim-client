# Spec Verify Review 2 — Product & Test Lens

**Spec version**: rev-2 (commit `eb5e1958`)
**Round**: Loop 1 / Round 2 (verify)
**Prior review**: `2026-04-24-external-grpc-spec-review-2-product-test.md`

---

## Round-1 findings resolution status

### Resolved ✅

- **C1 (entries_by_command_id)** — Resolved via **D25** + §5.9. Spec adds `AuditLogPort::entries_by_command_id(cmd_id, limit) -> Vec<AuditEntry>`, SqliteStorage impl with concrete SELECT (indexed on `command_id`), and extends `GET /api/audit/export?command_id=X`. Two integration tests (§9.2 audit-query-surface block) exercise both the port method and the REST path. **Caveat**: see NV1 below — the REST surface doesn't currently exist.

- **C2 (grpc-status-code persistence)** — Resolved via **D26** + §5.5. `ExternalGrpcAuditDetails` gains `grpc_status_code: Option<u32>`, populated in the deferred task (§5.5 line 643: `observed.map(|c| c as i32 as u32)`). §9.2 explicit test `audit_denied_when_handler_returns_permission_denied` asserts `grpc_status_code = 7`. Security dashboards can now cut by raw code.

- **C3 (cancelled-stream test ambiguity)** — Resolved via **D28 header-first** + OQ6 split. §9.2 splits into two deterministic tests: `audit_timeout_when_handler_returns_cancelled` (Option B — handler returns `Err(Status::cancelled)` → trailers-only header path → `Code::Cancelled` → `Timeout`) and `audit_completed_when_client_drops_before_trailer` (Option A — documents fallback). Plus a unit-level test on `TrailerCapturingBody` for streaming DeadlineExceeded via trailer frame. OQ6 closed in §13.

- **I1 (G3 untestable)** — Resolved via **D33** + §9.2 `external_grpc_live_streaming_toggle_reflects_within_1s`. G3 now says "typically <10ms in production, CI assertion ≤1s". Explicit `start.elapsed() < Duration::from_secs(1)` assertion named in the test list.

- **I2 (operator cannot inspect live config)** — Resolved via **D29** + §5.11. `GET /api/external-grpc/live-config` returns `LiveConfigResponse { streaming_enabled, load_policy_snapshot: LoadPolicyView { cpu_*_pct, min_free_mem_gb, started_at_unix_ms, in_warmup } }`. Two integration tests named. 503 when external disabled.

- **I3 (REPLACE/EXTEND/NEW markers)** — Resolved in §9.2. Every entry now carries an explicit **NEW** / **REPLACE** / **EXTEND** tag. Line refs: REPLACE L933, EXTEND L1531, EXTEND L1594 — verified against current `tests/external_grpc_integration.rs` (grep confirms L934/L1531/L1594; spec cites L933 which is off-by-one from the `async fn` line but points to the attribute `#[tokio::test]` just above — acceptable).

- **I4 (auth-reject path correlation)** — Resolved via **D14 revised** + **U5** + §5.2/§5.5. `RequestIdLayer` moved outermost (above AuthLayer). `AuthLayer` reads `RequestId` from extensions for its Failed-path audit writes. §9.2 test `external_grpc_request_id_preserved_across_auth_reject` pins the behavior. Rationale for the ~30ns cost made explicit in §4.1.

- **I5 (long-stream snapshot docs)** — Resolved in §3.2 NG7 (explicit "eventually-consistent per-decision semantics" language) + §5.1 invariants bullet + §9.2 `live_reload_affects_long_running_stream` test.

- **I6 (docs criterion vague)** — Resolved in §14. Explicit "existing aspirational text at line 171 … was a lie pre-this-PR — must be rewritten as a correctness fix, not just an addition." Korean-sync requirement preserved.

### Incomplete / partial ⚠️

None of the Round-1 findings are outright partial — but see New Issues below for derivative concerns that rev-2 *introduced* during fix-up.

### New issues found in rev-2 edits

#### NV1 (Important): `GET /api/audit/export?command_id=X` — endpoint does not exist

**Evidence**: Spec §5.9 says "Extends the existing `GET /api/audit/export`" — but grepping `crates/oneshim-web/src/routes.rs` for `audit` returns only `/integration/audit` and `/audit` (both route to `handlers::integration::get_audit`, which returns **integration-specific** `IntegrationAuditLogResponse`, not raw `Vec<AuditEntry>`). No `export_audit` function exists. No `/api/audit/export` route exists.

The spec treats this endpoint as pre-existing when it is new. Three consequences:

1. §4.2 LoC estimate for `oneshim-web/src/handlers/audit.rs` shows "+20/-0" — but this is a net-new module (handler + `AppState` wiring + types), likely +80-100 LoC.
2. §9.2 `audit_export_rest_endpoint_filters_by_command_id` test assumes the endpoint exists; plan phase must create it.
3. §14 docs checklist mentions documenting the query surface under `docs/guides/external-grpc.md` — but this is an audit endpoint, not a gRPC endpoint. Docs home is unclear.

**Fix**: §4.2 should add a new row for `handlers/audit_export.rs` (new file, ~80 LoC). §5.9 language should say "add new endpoint" not "extend existing." Also consider whether `/api/audit/export` is in the `oneshim-web` contract-frozen manifest — if yes, OpenAPI spec (`docs/contracts/oneshim-web.v1.openapi.yaml`) needs an update too.

**Default limit / DoS surface**: §5.9 does cap at `limit.min(1000)` — good. But 1000 rows × ~2KB JSON = 2MB response; if called with no throttle, a malicious internal caller (already auth'd) could slow the agent. Minor — the dashboard is loopback-only. Not blocking.

#### NV2 (Important): D32 `config_reload_task_alive` has no inspection surface

**Evidence**: D32 declares `ExternalMetrics.config_reload_task_alive: AtomicBool`. §5.11 `LiveConfigResponse` / `LoadPolicyView` **does not include it**. `ExternalMetrics` module docstring at `crates/oneshim-web/src/grpc/external/metrics.rs:1-3` says "values are exported via the existing telemetry adapter as a follow-up" — so there's no `/metrics` endpoint today. This means if `ConfigReloadTask` silently panics mid-reload, the `config_reload_task_alive = false` fact is **invisible to operators**.

This is exactly the "3am page gap" that I2 was about — closed for current values via D29, but the *task liveness* signal gets the same gap.

**Fix**: Add `task_alive: bool` field to `LiveConfigResponse`. Trivial LoC addition. Or mention in §14 that operators verify task liveness by observing that reload changes reflect in `GET /live-config` within 1s — but spec should be explicit either way.

#### NV3 (Minor): §8.6 contradicts D32 scope

**Evidence**: §8.6 line 1294 says "Optional metric (deferred unless requested): add `external_grpc_config_reload_total` counter in `ExternalMetrics`. **Not blocking for this PR**." But D32 line 1514 declares the same counter as **in scope** ("Promoted from 'optional deferred' to in-scope"). §11 "Out of Scope" line 1473 also lists it as deferred.

**Fix**: remove stale line from §8.6 + §11. D32 is the single source of truth. Not blocking but confusing for the plan-phase reviewer.

#### NV4 (Minor): `external_grpc.streaming_enabled: Option<bool>` fallback discoverability

**Evidence**: OQ12 flags the serde null-line concern — good. But the spec's §7.1 user-visibility bullet only covers the *two* explicit cases (operator sets `Some(false)` vs. leaves `None`). It doesn't say what happens when an operator flips `web.grpc_streaming_enabled = false` while `external_grpc.streaming_enabled = Some(true)` (external override wins → external keeps streaming even though the shared field is off). The integration test `external_streaming_falls_back_to_web_field_when_external_none` covers the `None` case but not the override-beats-parent case.

**Fix**: Add a matrix row to §7.1 (or a §9.2 test `external_streaming_override_wins_over_web_field_when_some`). Low cost (+1 test, +3 lines of docs). Not blocking.

#### NV5 (Minor): `grpc_status_code` discoverability for operators

**Evidence**: D26 persists the field inside `ExternalGrpcAuditDetails` JSON. §14 docs checklist does not explicitly say to document this field as queryable. An operator running `GET /api/audit/export?command_id=X` gets the row but has to know the `details` JSON contains a `grpc_status_code` key to write dashboards against it.

**Fix**: §14 docs add a bullet: "Document `ExternalGrpcAuditDetails` JSON schema including the new `grpc_status_code` field in the Auditing section." Minor polish.

#### NV6 (Minor): Streaming-RPC cancelled test mechanism still slightly underspecified

**Evidence**: §9.2 `audit_timeout_when_handler_returns_cancelled` says "handler returns `Err(Status::cancelled(...))`; deterministic via trailers-only header path." Fine for unary. For streaming RPCs (SubscribeMetrics), `Status::cancelled` can be returned either (a) before any body frame is emitted (trailers-only-header path) or (b) after several frames via a stream-abort error (trailer-frame path). The test doesn't say which one. Both should work via rev-2's dual-path design, but ambiguity remains.

**Fix**: Add clarifying sentence: "Test uses a 0-message fixture (no data frames) so the `Err` produces trailers-only headers, exercising the §5.5 header-first branch." Near-trivial.

---

## Verdict

**CONDITIONAL-PASS**.

Rev-2 addresses all 6 Round-1 product/test findings with concrete, testable, user-visible fixes. The synthesis was applied faithfully: D25 (C1), D26 (C2), D28 + split tests (C3), D33 + ≤1s test (I1), D29 + §5.11 (I2), REPLACE/EXTEND/NEW tags (I3), D14-revised + §5.2/§5.5 (I4), §3.2 NG7 language + long-stream test (I5), §14 rewrite-directive (I6).

The conditions: two new Important issues introduced during fix-up need to be pinned before plan phase:

1. **NV1** — spec assumes `/api/audit/export` is pre-existing; it is not. §4.2 LoC + §5.9 language + docs checklist need adjusting. Plan phase must include "create new audit export handler" as a task, not "extend existing."
2. **NV2** — D32's `config_reload_task_alive` has no inspection surface. Either add it to `LiveConfigResponse` (§5.11) or document the log-grep / indirect-verification path in §14. Operator cannot detect task panic otherwise.

Plus three Minors (NV3-NV6) that are polish — not blocking.

**Recommendation**: fix NV1 + NV2 in rev-3 before transitioning to writing-plans. The plan phase will otherwise encode a wrong LoC estimate and an invisible failure mode.

No Critical issues remain. Integration-test coverage for the new behaviors is explicitly laid out, timing bounds are CI-verifiable, and the operator story (correlation lookup, live-config inspection, status disambiguation) is coherent.
