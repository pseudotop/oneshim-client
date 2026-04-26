# Spec Verify Review 3 — Product & Test Lens

**Spec version**: rev-3 (commit `bee9b308`)
**Round**: Loop 1 / Round 3 (verify)
**Prior review**: `2026-04-24-external-grpc-spec-review-verify-2-product-test.md` (CONDITIONAL-PASS)

---

## Round-2 findings resolution status

### Resolved (product/test dimension)

**NV1 (Important — `/api/audit/export` endpoint is new, not extend)** — **Resolved**.
- §4.2 row at L162: `🆕 New | oneshim-web/src/handlers/audit_export.rs | ~80 impl + ~60 test | New GET /api/audit/export endpoint (D25 / NV1 fix) — rev-2 spec incorrectly assumed this was pre-existing`. LoC realigned from "+20/-0" in rev-2 to a dedicated 🆕 row at ~80+60.
- §5.9 L879-881 now explicitly says "**new file** `crates/oneshim-web/src/handlers/audit_export.rs`" and cites "Verify Round-2 (NV1) confirmed `GET /api/audit/export` does **not** currently exist — only the integration-specific `/integration/audit` route is registered (`routes.rs`). This spec introduces the endpoint as **net-new**."
- §5.9 L916 shows route registration `.route("/api/audit/export", get(export_audit))`.
- §5.9 L928 explicitly names the OpenAPI contract file: "**OpenAPI contract update** (per `docs/contracts/oneshim-web.v1.openapi.yaml`): new path `GET /api/audit/export` with `command_id` + `status` + `limit` query params, response `application/json: Vec<AuditEntry>`. Add to plan-phase tasks." Actionable file path supplied.
- §14 L1588 docs checklist retains "Audit query surface (`GET /api/audit/export?command_id=X`) documented per D25".
- Grep for dangling `extend.*existing.*audit` or plain `handlers/audit.rs`: **zero hits**. No stale rev-2 language remains.

**NV2 (Important — `config_reload_task_alive` inspection gap)** — **Resolved**.
- §5.11 L1027-1034: `LiveConfigResponse` struct now includes `pub config_reload_task_alive: bool` with docstring "Task liveness surfaced to operators per D32 — addresses NV2 (silent ConfigReloadTask panic is invisible otherwise)".
- §5.11 L1058-1062: handler body populates the field from `state.external_grpc_metrics.as_ref().map(|m| m.config_reload_task_alive.load(Relaxed)).unwrap_or(false)`. Semantics are sensible — when no metrics registered (external disabled), field defaults to `false`, which conservatively signals "not alive".
- The silent-panic observability hole that the rev-2 comment "panic leaves unchanged → observability for silent death" (L1322) had left invisible is now reachable via a first-class REST field.

**NV3 (Minor — §8.6/§11/D32 contradiction)** — **Resolved**.
- §8.6 L1316-1323 now reads "**In-scope metrics** (promoted from rev-1 'optional/deferred' per D32 / verify-round NV3)" — the three counters are inside the in-scope list, not an "Optional metric (deferred unless requested)" block.
- §11 L1503 row is struck through: "~~`external_grpc_config_reload_total` metric~~ | ~~Monitoring polish~~ | **Now in-scope per D32 / §8.6 (resolved, not deferred)**".
- D32 L1544 remains the source of truth. No contradiction.

**NV4 (Minor — override-beats-parent test missing)** — **Resolved**.
- §9.2 L1418: `external_streaming_override_wins_over_web_field_when_some` — "set `external_grpc.streaming_enabled = Some(true)`; set `web.grpc_streaming_enabled = false`; external server honors true (addresses NV4 — override-beats-parent semantics)". Complements the existing `external_streaming_falls_back_to_web_field_when_external_none` for the `None` case. Integration-test matrix now covers both legs of OQ12.

**NV6 (Minor — cancelled-stream test mechanism)** — **Resolved**.
- §9.2 L1394 now includes the explicit clause "**Test uses a 0-message-fixture handler** (no data frames emitted) so the `Err` produces a trailers-only HTTP response exercising the §5.5 header-first branch (addresses NV6 — streaming-RPC test mechanism clarity)." Unambiguous — reviewer knows the test exercises the Option B (header-first) branch, not a trailer-frame branch.

### Partially addressed

**NV5 (Minor — `grpc_status_code` field discoverability in docs)** — **Partial**.
- §14 L1583-1589 docs checklist lists "Auditing section **accurately** describes per-request status mapping (Completed/Denied/Timeout/Failed)" and the query surface, but **does not** explicitly add a bullet for documenting `ExternalGrpcAuditDetails` JSON schema including the new `grpc_status_code` field. Operators reading the guide will learn the status buckets (Completed/Denied/etc.) but may not know the raw int code is also queryable out of the JSON details blob. The spec body mentions the field is serialized with `#[serde(skip_serializing_if = "Option::is_none")]` (L1538 / §5.9 L930), so it will appear in JSON output when present — but discoverability in docs is still implicit.
- **Not blocking**. §14 is a checklist; planners can add the bullet when executing Docs. Minor polish residue only.

---

## New issues found in rev-3 edits

### NV7 (Minor): `AppState.external_grpc_metrics` field addition is not explicitly documented

**Evidence**: §5.11 handler body at L1059 reads `state.external_grpc_metrics.as_ref().map(|m| m.config_reload_task_alive.load(Relaxed))`. §5.11 L1072 wiring paragraph documents only `AppState.external_grpc_live: Option<Arc<LiveExternalConfig>>` — it does **not** explicitly list a companion `external_grpc_metrics: Option<Arc<ExternalMetrics>>` field that the compiler will require. Also §4.2 `app_runtime_launch.rs` row (L165) does not call out this additional field.

**Impact**: Plan-phase executor would otherwise receive a compile error ("no field `external_grpc_metrics` on AppState") and have to infer the wiring. Not catastrophic — it's a single-line `Option<Arc<ExternalMetrics>>` addition paralleling `external_grpc_live` — but should be explicit before plan transition.

**Fix (trivial)**: add a sentence to §5.11 L1072 along the lines of "`AppState` also gains `external_grpc_metrics: Option<Arc<ExternalMetrics>>` populated in `build_external_spawn_config` from the existing `ExternalMetrics` instance threaded through `AuditLayer`." Alternatively extend the §4.2 `app_runtime_launch.rs` row delta from "+30/-10" to reflect both fields.

### NV8 (Minor): §4.2 `routes.rs` row undercounts added routes

**Evidence**: §4.2 L163: `✏️ Mod | oneshim-web/src/routes.rs | +1/-0 | Register new /api/external-grpc/live-config route (D29)`. But §5.9 L916 also adds `.route("/api/audit/export", get(export_audit))` to the same file. The row should read "+2/-0 | Register two new routes: `/api/external-grpc/live-config` (D29) + `/api/audit/export` (D25)".

**Impact**: Cosmetic only — LoC estimate off by 1 line. Will surface during plan phase as a minor discrepancy. Not blocking.

### Regressions / drift from Round-2 fixes

None. The NV1/NV2 edits are localized (new §5.9 handler module + 4-line field addition in §5.11). No cascading breakage:
- Grep confirms no remaining "extend existing" language for the audit endpoint.
- `LiveConfigResponse` Serialize derive handles the new bool field trivially.
- The integration test list in §9.2 still cleanly numbers to ~18 new tests / total ~37 (L1420) — consistent with the +1 override test (NV4) already counted.
- Endpoint paths (`/api/audit/export`, `/api/external-grpc/live-config`) consistent throughout spec, route table, test names, docs checklist.

---

## Verdict

**PASS**.

Rev-3 resolves both Round-2 Important findings (NV1, NV2) with actionable, specific edits (LoC row additions, file-path callouts, explicit "net-new" language, observable surface). Three of four Round-2 Minors (NV3, NV4, NV6) are fully resolved; NV5 is partially addressed (the functionality is spec'd; the docs checklist bullet is still implicit but non-blocking).

Two net-new Minors (NV7 — `AppState.external_grpc_metrics` field addition undocumented; NV8 — `routes.rs` LoC undercount) surfaced and are both ≤5-line text fixes. Neither threatens the plan phase's ability to proceed — the executor can trivially add the field — but rev-4 (or an in-flight plan-phase correction) should pick them up for completeness.

No Critical or Important issues remain. Operator observability story is coherent (live-config + task_alive + deferred_audit_in_flight + config_reload_total all reachable via D32 + §5.11 surface). Integration-test coverage for the rev-2 gaps (override-beats-parent, cancelled-via-trailers-only) is explicitly laid out with deterministic mechanisms. Audit endpoint is correctly flagged as net-new with a concrete contract-update target file.

**Recommendation**: Proceed to Loop 2 (plan phase). Plan author should:
1. Add `external_grpc_metrics: Option<Arc<ExternalMetrics>>` to `AppState` (NV7) — 1 line.
2. Track `routes.rs` as +2 routes, not +1 (NV8) — cosmetic.
3. Optionally add the `ExternalGrpcAuditDetails` JSON-schema bullet to §14 docs checklist (NV5 polish) when executing Docs.

These are all sub-5-line polish items — not gate-blocking.
