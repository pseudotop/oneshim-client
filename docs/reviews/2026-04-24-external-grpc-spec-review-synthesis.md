# Spec Review Synthesis — Loop 1 Round 1

**Spec under review**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-spec.md` (commit `e3f5ee68`)
**Reviews consolidated**:
- Review 1 (Architecture): `2026-04-24-external-grpc-spec-review-1-architecture.md`
- Review 2 (Product/Test): `2026-04-24-external-grpc-spec-review-2-product-test.md`
- Review 3 (Platform/Risk): `2026-04-24-external-grpc-spec-review-3-platform-risk.md`

**Raw finding totals**: 9C + 18I + 14M + 16Q across 3 reviews
**Consolidated (after deduplication)**: **7 Critical, 15 Important, 11 Minor, 12 Open Questions**

---

## Consolidated Critical issues (must fix in spec before plan phase)

### CR1: Trailers-only response path breaks G2 for handler-returned errors ⚠️ **highest-impact**
**Source**: Platform C1
**Impact**: `TrailerCapturingBody::poll_frame` never observes `grpc-status` when a handler returns `Err(Status)` because tonic 0.14 puts grpc-status in **initial headers** (verified `tonic-0.14.5/src/status.rs:605`, `server/grpc.rs:20`). Result: every `Status::permission_denied`/`timeout`/etc. handler path audits as `Completed` — exactly the bug G2 aims to fix.
**Blocks**: G2 (per-response status mapping) + §9.2 `external_grpc_audit_denied_for_permission_denied` test
**Resolution** (inferred from reviewer's concrete code suggestion): spec §5.3 + §5.5 gain a **header-first** observation path: after `inner.call(req).await?`, inspect `response.headers().get("grpc-status")`. If present → the response is trailers-only → pre-populate oneshot with the parsed code and skip trailer-observation (wrap body for msg_counter semantics only). Otherwise, existing body-wrapping path handles streaming + normal-trailer cases.

### CR2: `grpc_streaming_enabled` is on `AppConfig.web`, shared with loopback — NG1 violated
**Source**: Platform C2
**Impact**: Spec NG1 says "Loopback server unchanged" and §7.1 says "users who edit these fields see changes take effect." But the field is under `WebConfig` (verified `config/sections/network.rs:106-109`), so enabling live reload for external creates a hybrid state where external honors the toggle and loopback does not (frozen at boot).
**Resolution required** (decision): **U1 below**

### CR3: `LoadPolicy::new` panics on invalid thresholds — `ConfigReloadTask` silently dies
**Source**: Architecture C2 + Platform C3 (duplicates)
**Impact**: Verified `crates/oneshim-web/src/grpc/load_policy.rs:42-58` uses `assert!` trio. A malformed `grpc_load_thresholds` in config → panic in spawned tokio task → task aborts silently → subsequent reloads ignored. Supervisor not aware.
**Resolution** (inferred): add `LoadPolicy::try_new(thresholds) -> Result<Self, LoadPolicyError>` alongside existing `new` (which can become `try_new(...).expect("validated")` for backward-compat at boot). `apply_config` uses `try_new`, logs error on failure, skips only the `load_policy` update (preserves `streaming_enabled` independent update). Add test `config_reload_rejects_malformed_thresholds_and_continues`.

### CR4: `DashboardServiceImpl` dual-mode (loopback/external) design not in spec
**Source**: Architecture C1
**Impact**: `DashboardServiceImpl` owns `streaming_enabled: bool` + `load_policy: Arc<LoadPolicy>` at `grpc/mod.rs:99-100` and is shared between `from_spawn_config` (loopback) and `from_external_spawn_config` (external). Spec §5.6 understates change as "reads `cfg.live`" but loopback has no `LiveExternalConfig` per NG1. Spec silent on which fields persist + how handlers dispatch.
**Resolution** (inferred from the two options presented): **Option A — `StreamingSource` enum** replaces the two owned fields with `enum StreamingSource { Fixed(bool, Arc<LoadPolicy>), Live(Arc<LiveExternalConfig>) }`. Handlers call `self.streaming_source.streaming_enabled()` / `.load_policy()`. Avoids struct duplication. `from_spawn_config` constructs `Fixed`, `from_external_spawn_config` constructs `Live`.

### CR5: No `entries_by_command_id` query surface — product promise unreachable
**Source**: Product C1
**Impact**: §2.3 promises "find audit row in <1s via request ID" but `AuditLogPort` exposes only `entries_by_status` / `entries_by_action_prefix`. Operators would need raw sqlite3 access.
**Resolution required** (decision): **U2 below**

### CR6: Flagship Timeout test design (OQ6) actually tests opposite of intent
**Source**: Product C3 (overlaps with Platform M3)
**Impact**: OQ6 was deferred. Spec §9.2 test `external_grpc_audit_timeout_for_cancelled_stream` using Option A (realistic client drop) → `poll_frame Err → Drop fires None → Completed` per §8.2. Option B (handler returns `Status::cancelled`) triggers the trailers-only path (CR1) — which doesn't work until CR1 is fixed.
**Resolution** (inferred — depends on CR1 fix): After CR1 lands (header-first observation), Option B becomes testable: handler returns `Status::cancelled`, header path captures `Code::Cancelled` → maps to Timeout. Split the test:
  - `audit_timeout_when_handler_returns_cancelled` — deterministic via handler-side Status return (header-first path)
  - `audit_completed_when_client_drops_before_trailer` — documents fallback (body-drop → None → Completed)
  - Plus a **unit-level** test on `TrailerCapturingBody` with hand-crafted body emitting `grpc-status: 4` trailer (covers streaming Timeout via trailer path for completeness).
Close OQ6 now.

### CR7: `Unauthenticated`/`PermissionDenied` coalesce without raw code persistence
**Source**: Product C2
**Impact**: §5.3 maps both to `Denied`. Security dashboards see one bucket that could be "no permission", "token expired at handler", or "handler decided so" — no way to disambiguate. Raw `grpc-status` integer is not persisted.
**Resolution required** (decision): **U3 below**

---

## Key User Decisions Required

The fixer cannot resolve these without picking a direction. Default recommendations noted per preceding convention ("go with recommended" from prior sections).

### U1: streaming_enabled field scope (CR2 resolution)
**Options**:
- **(a) Add distinct `external_grpc.streaming_enabled: Option<bool>`** — falls back to `web.grpc_streaming_enabled` when None. Live reload wires to the new field. Preserves NG1 fully.
- **(b) Accept shared field, document + test pinning** — less code; explicit note that live reload affects external only; add test `loopback_streaming_enabled_is_not_live_reloaded`.

**Recommendation**: **(a)** — cleaner scope isolation, defensive for future, ~15 LoC config addition + serde attr for backward compat. The dual-subsystem-impact in (b) is an operational footgun.

**Inference decision**: **Going with (a)**. Rationale: matches existing `external_grpc.*` field-under-own-section convention already established in PR #484.

### U2: `entries_by_command_id` query surface (CR5 resolution)
**Options**:
- **(a) Add `AuditLogPort::entries_by_command_id(cmd_id: &str, limit: usize)` + SQL + REST `?command_id=` param** — in-scope, ~60 LoC total.
- **(b) Descope product promise** — §2.3 rewritten from "<1s lookup" to "via raw `sqlite3 ... WHERE command_id = ?`"; add explicit snippet to docs.

**Recommendation**: **(a)** — the whole operational value of x-request-id rests on correlation lookup. Descoping to raw sqlite is hostile to SRE workflows (exposes DB shape; no API stability).

**Inference decision**: **Going with (a)**. Adds AuditLogPort method + SqliteStorage impl + REST handler + 3-4 tests. Adds ~80 LoC to the PR.

### U3: grpc_status_code persistence (CR7 resolution)
**Options**:
- **(a) Add `grpc_status_code: Option<u32>`** to `ExternalGrpcAuditDetails` struct; `record_completion` populates from observed status.
- **(b) Justify conflation in spec** without persisting; security dashboards use status buckets only.

**Recommendation**: **(a)** — the field is cheap (1 field in audit details JSON), enables security dashboards to cut by raw code, future-proofs against more conflations. Table-driven test `map_code_to_audit_status` already planned — just extends it.

**Inference decision**: **Going with (a)**.

### U4: LoadPolicy warmup-reset on reload (Architecture Q1)
**Context**: `LoadPolicy::new` sets `started_at = Instant::now()` — every reload resets the 30s warmup, forcing `LoadLevel::Medium` regardless of actual load. Defeats operator intent ("tune thresholds during incident").
**Options**:
- **(a) Preserve warmup** — new `LoadPolicy::with_started_at(thresholds, Instant)` method; `apply_config` on reload captures old `started_at` and threads through.
- **(b) Accept reset** — documented as "new policy begins fresh warmup"; operator must wait 30s for new thresholds to take effect.

**Recommendation**: **(a)** — preserving warmup is what operators actually want. Adds a single constructor method + 1 test.

**Inference decision**: **Going with (a)**.

### U5: RequestIdLayer vs AuthLayer outer order (Product I4)
**Context**: Current spec D14: auth → request_id → audit. Auth-rejected rows get `command_id = None` from AuthLayer's own spawn-Failed path; operator correlating by client-side `x-request-id` finds no matching audit row.
**Options**:
- **(a) Move RequestIdLayer outermost** — `request_id → auth → audit`. AuthLayer reads `RequestId` extension for its Failed-row `command_id`. Cost: every unauth request pays 1 UUID construction (~30ns).
- **(b) Keep order, document correlation gap** — spec explicitly says "auth-rejected audit rows have different command_id than request header."

**Recommendation**: **(a)** — the correlation chain at the security boundary is exactly where operators need it most. Cost is negligible.

**Inference decision**: **Going with (a)**. This changes D14 and requires updating AuthLayer's 4 Failed-path spawn blocks to read `RequestId` from extensions.

---

## Other issues (auto-resolved inline without user decision)

### Arch I1 / Platform M1: `pin_project_lite` is workspace-transitive (verified)
→ Close OQ2. Update D19 to "use `pin_project_lite` (already transitive via tokio/tower/hyper)."

### Arch I2: `ConfigReloadTask` spawn placement
→ Change §5.4 spec: spawn in `build_external_spawn_config` matching cert watcher pattern, not `serve_external`. Prevents restart-leak.

### Arch I3: Body trait compatibility proof
→ Add §5.5.x compile-time-check test: `fn assert_body<T: http_body::Body + Send + 'static>(); assert_body::<TrailerCapturingBody<tonic::body::Body>>();`.

### Arch I4: Deferred spawn regression for unary
→ Optimize §5.5: header-first path (CR1 fix) naturally handles unary `Err(Status)` synchronously. Body-wrap path still used for streaming. Unary `Ok` path: trailer arrives inline in poll_frame before `Ok(response)` returns — oneshot fires synchronously, deferred task resolves immediately (no shutdown race window). Document this in §6.1.

### Arch I5: RequestIdLayer overwrite-vs-append
→ D5 clarified: **conditional overwrite** — if handler already set `x-request-id` matching the validated value, leave alone; otherwise insert. Preserves rare proxy-forward cases without breaking correlation.

### Arch I6 / Platform I2: spawn_config test updates + manual Debug impl
→ §5.6 explicit bullets: (a) update `spawn_config_clone_is_shallow` for new `live: Arc<...>` field; (b) keep manual Debug impl (drop derive), elide `config_rx` as `&"<watch>"`, continue redacting cert/JWT material.

### Arch I7: `mod.rs` declarations
→ §4.2 explicit list of 4 `pub(crate) mod` lines added.

### Arch C3 / Platform I1: two-store tear (NG7 claim false)
→ Replace dual `AtomicBool` + `ArcSwap<LoadPolicy>` with **single `ArcSwap<LiveSnapshot>`** where `LiveSnapshot { streaming_enabled: bool, load_policy: Arc<LoadPolicy> }`. Single-load atomicity, simpler reasoning. Update §5.1 + §5.4.

### Product I1: G3 convergence test
→ Add integration test `live_reload_convergence_under_1s` with `start.elapsed() < Duration::from_secs(1)` assertion. Keep SLO wording at ≤5s but enforce ≤1s in CI.

### Product I2: live-config inspection for operators
→ Add in-scope `GET /api/external-grpc/live-config` REST endpoint returning current `{ streaming_enabled, load_policy_snapshot }`. ~40 LoC + 2 tests. Promotes operator confidence; closes "is my reload working?" gap.

### Product I3: integration test replace/extend/duplicate semantics
→ §9.2 each new test explicitly says what it does to existing tests:
- `external_grpc_request_id_header_returned` (L933) → **REPLACE** body
- `external_grpc_audit_completed_entry_written_after_ok_response` (L1531) → **EXTEND** (assert command_id match + keep existing asserts)
- `external_grpc_streaming_audit_records_message_count` (L1594) → **EXTEND** similarly

### Product I5: long-stream snapshot docs
→ §5.1 adds: "Long-running streams may call `.streaming_enabled()` / `.load_policy()` multiple times and see eventually-consistent snapshots across reloads. This is intentional — decisions are per-call, not per-stream." Add test `live_reload_affects_long_running_stream`.

### Product I6: docs §14 rewording
→ §14 doc criterion rewritten: correct existing aspirational Denied/Timeout claim; add accurate live-reload section; sync `.ko.md` per `DOCUMENTATION_POLICY.md`.

### Platform I3: unbounded deferred spawn
→ Add minimum: `ExternalMetrics.deferred_audit_in_flight: AtomicUsize` gauge. Increment on spawn, decrement at task end. Documented in §5.5 + §8.6 (move from "optional deferred" to in-scope). Bounded-channel alternative deferred to a follow-up if high-RPS shows pressure.

### Platform I4: coalescing + G3 SLO
→ Rewrite G3: "After `ConfigManager` emits a change event, `LiveExternalConfig` reflects the new value within one tokio scheduler tick, typically <10ms." Remove ≤5s. Add coalescing doc note + test `config_reload_coalesces_rapid_successive_updates`.

### Platform I5: UUID validation compatibility
→ §5.2 adds one-liner: "UUIDv4 output (lowercase hex + hyphens, 36 chars) satisfies `is_valid` by construction."

### Minor items (spec polish, not blocking)
- M2 (audit log label constant): define `const REQUEST_ID_HEADER: &str = "x-request-id";` at module top.
- M3 (Prometheus cardinality note): call out audit_status label change in §8.6.
- Platform M2: `tonic::Code::from_i32` is infallible (verified) — no change needed.
- Platform M4: line-number drift check deferred to plan phase.

---

## Open Questions still pending (non-blocking)

### Q-NEW-1 (Platform Q2): `AuditBridge::record_completion` signature
Spec §5.5 pseudocode argument order doesn't exactly match current signature `(ctx, remote, operation, status, duration, msg_count_opt, failure_reason)`. Is `command_id` **added as 8th param** or **replaces `failure_reason`** or **part of `ctx`**?
→ **Decision (inferred)**: add as 8th param (`command_id: Option<String>`). `failure_reason` is for grpc error message text (separate concern). Spec §5.5 + §4.2 component map should show `audit_bridge.rs +10/-0` (added param + propagation into `ExternalGrpcAuditDetails`).

### Q-NEW-2 (Platform Q3): observability when ConfigReloadTask exits
→ Add `ExternalMetrics.config_reload_task_alive: AtomicBool` (set true at task start, false at clean exit, unchanged on panic — so operators can detect panic vs clean exit separately via `external_grpc_config_reload_total` + `alive`).

### Q-NEW-3 (Arch Q2): Auth-reject audit command_id
→ Resolved by **U5** (move RequestIdLayer outermost).

### Q-NEW-4 (Arch Q3 / Platform Q4): misc micro-Qs
→ `config_rx.borrow_and_update()` borrow lifetime (Arch Q3) — document in §5.4 code comment: "`Ref` dropped at end of `apply_config(…)` statement — no await held across." (Already safe.)
→ `max_concurrent_streams` not live-reloaded (Platform Q4) — add one-sentence justification in §7.1: "Stream cap changes mid-flight are complex (existing streams vs new streams); deferred."

---

## Fixer applied — list of spec edits

The fixer will apply the following 5 user decisions (U1-U5) + the 14 auto-resolved items in one consolidated spec revision (rev-2). Each edit traces back to a review finding.

**Decisions table changes** (§12):
- D14 → revised: "Layer ordering `request_id → auth → audit` (CR-resolves I4-product)"
- D19 → revised: "Use pin_project_lite (already transitive); OQ2 closed"
- D21 (new): "Single ArcSwap<LiveSnapshot> for atomic cross-field read"
- D22 (new): "streaming_enabled = external_grpc.streaming_enabled: Option<bool>, falls back to web.grpc_streaming_enabled"
- D23 (new): "LoadPolicy::try_new infallible constructor, apply_config uses it"
- D24 (new): "DashboardServiceImpl dual-mode via StreamingSource enum"
- D25 (new): "entries_by_command_id added to AuditLogPort + SqliteStorage + REST handler"
- D26 (new): "grpc_status_code u32 persisted in ExternalGrpcAuditDetails"
- D27 (new): "LoadPolicy preserves started_at across reloads via with_started_at"
- D28 (new): "Header-first grpc-status observation before body wrapping (trailers-only path)"
- D29 (new): "GET /api/external-grpc/live-config REST endpoint for operator inspection"

**LoC impact revision** (§4.2):
- `audit_bridge.rs`: +10 (command_id param + grpc_status_code field)
- `config_reload.rs`: +20 (try_new + partial-apply logic)
- `live_config.rs`: simplified to `ArcSwap<LiveSnapshot>` + LoC slightly lower (~60)
- `load_policy.rs`: +25 (try_new + with_started_at)
- New: `handlers/external_grpc_live_config.rs` (+40)
- `routes.rs`: +2 (new route)
- AuditLogPort + SqliteStorage: +60 (entries_by_command_id)
- `grpc/mod.rs`: +30 (StreamingSource enum + dispatch)
- Net total: ~1500 LoC (up from 1300). Acceptable for scope.

**Test count revision** (§9.5):
- Unit tests: +4 (try_new, with_started_at, entries_by_command_id, live-config-handler)
- Integration tests: +3 (audit_by_command_id_query, audit_includes_grpc_status_code, live_config_rest_endpoint)
- Total: ~49 (up from 42).

---

*End of synthesis. Fixer phase next — apply all above as spec rev-2, commit, then trigger verify round.*
