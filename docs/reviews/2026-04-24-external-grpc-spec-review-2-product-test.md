# Spec Review 2 — Product & Test Lens

**Reviewer role**: Product & Test lens (user-visible behavior, acceptance criteria, test coverage, observability)
**Spec under review**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-spec.md` (commit `e3f5ee68`)
**Round**: Loop 1 / Round 1

---

## Critical

### C1: Audit rows are not queryable by `x-request-id` — key product promise is unreachable

**Location**: §2.3 "On-call debugging — given an error report from a consumer, find the exact server-side audit row in <1s via request ID" + §6.1 data flow.

**Issue**: The headline operational value (G1 + §2.3) rests on an operator being able to look up an audit row by request ID. But the `AuditLogPort` trait (`crates/oneshim-core/src/ports/audit_log.rs`) exposes only `recent_entries`, `entries_by_status`, `entries_by_action_prefix` — **there is no `entries_by_command_id`**. Storage writes `command_id` via SQL (`crates/oneshim-storage/src/sqlite/mod.rs:267`) but no surface returns rows filtered by it. Operators would have to shell into SQLite directly, defeating the "<1s" promise.

**Evidence**:
```
$ grep entries_by_ crates/oneshim-core/src/ports/audit_log.rs
  async fn entries_by_status(...)
  async fn entries_by_action_prefix(...)
# no by_command_id
```

Docs guide §Auditing (`docs/guides/external-grpc.md:166-196`) only shows `entries_by_status` / `entries_by_action_prefix` / raw sqlite3 as query surfaces.

**Suggested fix**:
- Either add `AuditLogPort::entries_by_command_id(cmd_id: &str, limit: usize)` in this PR and expose via the existing `/api/audit/export` handler (new `?command_id=` query param), **or**
- Scope the product promise down: §2.3 should say "via raw sqlite3 query" not "<1s via request ID", and §14 doc criteria must add an explicit sqlite3 snippet (`SELECT ... WHERE command_id = ?`) to the Auditing doc block.
- Pick one; don't ship with the promise implicit and the query surface missing.

---

### C2: Status mapping table deliberately coalesces `Unauthenticated` into `Denied` without audit-query impact analysis

**Location**: §5.3 `map_code_to_audit_status` + §8.5 + D16.

**Issue**: `Some(PermissionDenied) | Some(Unauthenticated) => AuditStatus::Denied`. But AuthLayer **already writes an `AuditStatus::Failed` row for auth rejections before the request enters AuditLayer** (confirmed in §6.2 conceptual flow and `docs/guides/external-grpc.md:163-165`). That means the new `Denied` bucket will only ever contain handler-originated `PermissionDenied` (e.g., handler decides caller lacks a specific permission). But mixing handler-originated `PermissionDenied` with `Unauthenticated` — which a handler would only emit if JWT decoding fails inside the handler (never, by design) — means a security team running a Denied-rate dashboard now sees 100% handler-level auth failures conflated into one bucket that could be any of "no permission", "token expired at the handler layer", or "handler decided so".

The operator doesn't get to distinguish: `Denied_rate_spike → which kind?`

**Evidence**: §8.5 is silent on this. §5.3 code block:
```rust
Some(PermissionDenied) | Some(Unauthenticated) => AuditStatus::Denied,
```
Current handlers in `crates/oneshim-web/src/grpc/external/` only return `Ok | Unavailable | ResourceExhausted | Cancelled | Internal`. So bucket is probably always PermissionDenied today — but spec doesn't say that.

**Suggested fix**:
- Add an explicit rationale row to the status mapping table (§5.3) justifying the conflation, with a note that either (a) handlers never return `Unauthenticated` in practice so the conflation is benign, or (b) raw `grpc-status` integer must be persisted in the audit `details` JSON (it already is — via `failure_reason` — but spec doesn't require handler to populate it).
- Add `"grpc_status_code": 7` as a required field in `ExternalGrpcAuditDetails` and populate it in `record_completion`. Security dashboards can then cut by raw code. Cost: +1 field, +10 LoC, already table-tested by §9.1.3 `map_code_to_audit_status` table-driven test.

---

### C3: `external_grpc_audit_timeout_for_cancelled_stream` test design unverified and high-risk-flaky

**Location**: §9.2 test bullet + OQ6.

**Issue**: This is the headline test for Goal G2 (status mapping for Timeout) but the spec explicitly flags uncertainty in OQ6: "how to simulate client cancellation cleanly? Option A vs Option B, choose during plan phase."

Reviewing `tests/external_grpc_integration.rs:1594` (current `external_grpc_streaming_audit_records_message_count`), the existing streaming test drops the client stream and asserts a Completed row appears. But **current code writes `AuditStatus::Completed` for every body-drop case** (audit_layer.rs:127 hardcoded), so the existing test proves nothing about Timeout mapping.

For the new test to reliably assert Timeout, the server body must emit `grpc-status: 4` (DeadlineExceeded) or `grpc-status: 1` (Cancelled) in the trailer. But:
- Client-side `drop(stream)` → tonic sees `RST_STREAM` → server body polling returns `Err` → trailer may never be emitted → `TrailerCapturingBody::Drop` fires `None` → **mapped to Completed, NOT Timeout**.
- This is the exact case in §8.2 table row 1 ("`inner.poll_frame` returns `Err` → `None` → Completed").

So the "realistic" Option A from OQ6 would **test the opposite of what spec claims**. Only Option B (server returns `Status::cancelled` directly via the handler) exercises the Timeout branch, but that's not the real production scenario.

**Evidence**: §8.2 table + §6.2 conceptual flow ("DeadlineExceeded trailer sent") is not validated against tonic's actual behavior under client-side drop. The spec assumes the trailer gets written but provides no test that can verify this without mocking inner trailer emission.

**Suggested fix**:
- Split the test into two: (a) `audit_timeout_when_handler_returns_cancelled` — exercises `map_code_to_audit_status(Cancelled)` via a handler that returns `Err(Status::cancelled(...))` with a 1-frame stream, **deterministic**; (b) `audit_completed_when_client_drops_before_trailer` — documents the fallback behavior (drop → Completed).
- Add to §9.2 an explicit unit test at the `TrailerCapturingBody` layer using a hand-crafted body that **does** emit `grpc-status: 4` trailer; this covers the Timeout mapping deterministically.
- Close OQ6 in the spec **now** (it's blocking — changing the test design changes which code paths are covered).

---

## Important

### I1: G3's "≤5s convergence" acceptance criterion has no CI-verifiable test

**Location**: §3.1 G3 + §9.2 `external_grpc_live_streaming_toggle_reflects_immediately` + §9.4.

**Issue**: G3 promises ≤5s convergence. §9.2 integration test has no timing assertion — just "reload config ... next request returns Unavailable." That passes whether it took 1ms or 10s in CI. §6.4 notes convergence is "<10ms" — but no test enforces a bound.

The ≤5s number is defensive but the spec never ties it to an assertion. If the `ConfigReloadTask` future regresses (e.g., mis-ordered `biased` branches, adds a sleep, `apply_config` blocks on an async call), no CI signal fires.

**Suggested fix**: Add an integration test that asserts `start.elapsed() < Duration::from_secs(1)` (10× safety margin on the 100ms expected case) between `ConfigManager::reload()` returning and the first `SubscribeMetrics` returning `Unavailable`. Keep the ≤5s *contract* but enforce ≤1s in test to catch regressions early.

---

### I2: Operator cannot inspect current live-config value — no way to verify reload took effect

**Location**: §8.6 + §11 "`external_grpc_config_reload_total` metric" deferred.

**Issue**: Imagine an on-call runbook: "set `grpc_streaming_enabled=false` to mitigate." Operator edits config, runs reload, then… how do they confirm the server actually applied it? Options today:
1. Check agent logs for the `tracing::info!("external_grpc: live config applied")` line (§5.4). But agent logs may not be exposed to SRE.
2. Make a test SubscribeMetrics call and see if it returns Unavailable. Functional but roundabout.
3. Query a REST endpoint like `GET /api/external-grpc/live-config` — **spec does not propose one**.

This is a 3am-page gap.

**Evidence**: `crates/oneshim-web/src/routes.rs` has no `live-config` endpoint (grep shows only `/integration/*`, `/automation/*`, `/audit`). §8.6 calls the reload-count metric "optional, deferred" but has no in-scope mechanism for current-value inspection.

**Suggested fix**: Add to scope a `GET /api/external-grpc/live-config` REST handler returning `{ streaming_enabled: bool, load_policy_snapshot: {...} }`. Costs ~40 LoC + 2 tests. Or document explicitly in §14 that operators verify via agent logs grep command (and include the grep line in the runbook). Don't leave it unspecified.

---

### I3: Test count (42) includes heavy unit coverage but integration coverage is thin for behavior changes

**Location**: §9.5.

**Issue**: Of the 42 new tests, only 9 are integration tests. The spec changes 3 user-visible behaviors (request-ID round-trip, status mapping granularity, live reload). Integration tests map:
- Request-ID round-trip: 3 tests (happy, missing, invalid) — OK
- Status mapping: 3 tests (ok, denied, timeout) — **missing: failed for `Internal`/`Unknown`**, **missing: composite streaming RPC with 200+ msgs then failure**
- Live reload: 3 tests — OK

Existing `external_grpc_request_id_header_returned` (line 933) and `external_grpc_audit_completed_entry_written_after_ok_response` (line 1531) **already exist but don't assert the new behaviors**. Spec §9.2 proposes new names; will the existing 2 be deleted, updated, or duplicated? Unclear.

**Suggested fix**: Spec §9.2 should add a dedicated "Replaces existing test X" / "Modifies existing test Y" subsection. Specifically:
- `external_grpc_request_id_header_returned` (line 933) should be **replaced** (the TODO comment in its body becomes the new test body).
- `external_grpc_audit_completed_entry_written_after_ok_response` (line 1531) should be **extended** to also assert `command_id` matches the request header — not duplicated.
- Add: `external_grpc_audit_failed_for_internal_error` (covers Failed mapping).
- Add: `external_grpc_streaming_status_on_handler_ok_after_N_messages` (covers the msg_count+status correlation in Completed row).

Net: +2 integration tests, -0 duplicates, closes gap.

---

### I4: Auth-reject path audit row loses command_id correlation

**Location**: §6.1 + §10.1 + NG5.

**Issue**: `AuthLayer` rejects bad JWTs before `RequestIdLayer` runs (per §4.1 layer order: Auth outermost). So auth-rejected requests get an audit row **with `command_id` generated by AuthLayer's own logic, not the request's `x-request-id`**. That means:
- An operator who receives a client error report with `x-request-id: req-xyz-123` and looks up the audit row will find **no matching row** if the request was auth-rejected.
- The entire correlation chain breaks at the security-most-relevant boundary.

**Evidence**: §4.1 diagram explicitly places AuthLayer before RequestIdLayer. `crates/oneshim-web/src/grpc/external/audit_layer.rs` and the spec's D14 rationale ("unauth requests never touch RequestId/Audit") confirm this.

**Suggested fix**: Two options:
1. Move RequestIdLayer outermost (before Auth), so AuthLayer can read `RequestId` extension for its Failed-row `command_id`. Documented cost: unauth requests incur 1 UUID construction (~30 ns). Cheap.
2. Keep order, document clearly in §8 that auth-rejected rows have a **different `command_id` than the client's `x-request-id` header** — and update §5.3 tests + §14 docs to explicitly call this out so operators don't chase missing rows.

Option 1 is the honest choice. Spec currently implies seamless correlation without this caveat.

---

### I5: Live reload of `LoadPolicy` is atomic per-reload but decisions mid-request may be split across old and new policies

**Location**: §5.1 + §3.2 NG7.

**Issue**: NG7 says "readers don't observe partial updates" — true for a single `load_policy()` call. But a streaming RPC may call `live.load_policy()` **multiple times over its lifetime** (once per decision point, e.g., initial admission, periodic shed-check). If the config reloads mid-stream, decision N uses policy v1 and decision N+1 uses policy v2. The individual decisions are atomic but the stream's *behavior* is non-atomic.

This is likely **intentional** (it's how the operator expects "flip the switch and it takes effect"), but no test confirms it or documents it. If a reviewer assumes "one RPC sees one policy", they'd mis-design the handler.

**Evidence**: §5.1 comment says "observe eventually-consistent snapshots" — good. But no integration test in §9.2 verifies a long-running stream correctly picks up a threshold change mid-flight. `external_grpc_live_load_thresholds_applied` tests "next request" only.

**Suggested fix**: Add a test: `live_reload_affects_long_running_stream` — open SubscribeMetrics, reload with new thresholds that would trigger shed, verify shed happens within N decision cycles. And add a sentence to §5.1 clarifying that long-running streams see eventually-consistent per-decision snapshots (an acknowledged design choice).

---

### I6: Docs criteria (§14) mention updating external-grpc.md but not the scope of changes

**Location**: §14 success criteria.

**Issue**: §14 says "Auditing section mentions `x-request-id`, new `AuditStatus` granularity" + "Live-reload section added with supported-fields table." But:
- Current doc (line 162) says "writes Completed after the handler returns" — the new behavior changes this to "writes Completed/Denied/Timeout/Failed per grpc-status trailer". This phrasing must be rewritten, not just "mentioned".
- The 4-status distinction was **claimed** in the current doc (line 171: "external_grpc_denied", "external_grpc_timeout") but actually wasn't emitted before this PR. So the current doc is already lying. The PR closes the lie — spec should call this out as a doc correctness fix, not just an addition.
- Korean companion doc (`external-grpc.ko.md`) must be synced. §14 says "+ .ko.md" but no per-section parity requirement.

**Suggested fix**: Rewrite §14 doc criterion as: "Auditing section accurately describes per-request status mapping (Completed/Denied/Timeout/Failed) with examples; existing text claiming Denied/Timeout emission must be corrected (was aspirational; now true). Live-reload section added with full field table (§7). Korean companion doc synced section-for-section (per `docs/DOCUMENTATION_POLICY.md`)."

---

## Minor

### M1: `CapturingAudit` mock fidelity mismatch with spec's new signature

**Location**: §9.1.5 + `tests/external_grpc_integration.rs:1474-1516`.

The existing `CapturingAudit` infers status from `action_type` (`external_grpc_completed` etc.). With the new deferred-task pattern, `log_event` is called with the mapped action_type and `log_complete_with_time` with the details JSON. Current mock writes **both** as separate entries (lines 1482 + 1507). The tests count Started+Completed = 2 entries but after this PR it might be 4 (Started via log_complete, Started via log_event, Completed via log_complete, Completed via log_event). §9.1.5 unit tests don't clarify whether the mock is rewritten.

**Suggested fix**: Spec §9.1.5 should add a line: "Existing `CapturingAudit` helper (tests/external_grpc_integration.rs:1434) needs update: dedupe Started rows by command_id+action_type to avoid double-counting." Or commit to one entry per record_completion call (drop the log_event) — that would simplify both audit bridge and tests.

### M2: UUIDv4 validation rule accepts UUIDs but is not UUID-specific

**Location**: §5.2 D3.

The 0x21..=0x7E+length≤128 rule accepts UUIDs, ULIDs, snowflakes — per D3 intent. But it also accepts `"admin"`, `"test"`, `"1"`. No user-visible harm, but it means an operator searching audit logs by "all req-IDs starting with `req-`" will find caller-supplied non-UUIDs mixed with generated UUIDs. Spec silent on operator mental model.

**Suggested fix**: §8.5 should add: "Operators should not assume `command_id` in audit rows is a UUID; it may be any caller-supplied string matching the validation rule. Use response header + audit query together for correlation; don't assume structure."

### M3: `biased;` select comment in §5.4 says "prefer clean exit over final stale config" but never tested

Spec §9.1.4 has `biased_prefers_shutdown_over_config_change` — good. But timing: if config and shutdown fire the same tick, tokio's scheduling still makes this partially non-deterministic. §9.1.4 should specify: "send config change FIRST (future pending), then send shutdown, then poll the task — shutdown must win." Clarifies the test's determinism.

### M4: `ExternalGrpcSpawnConfig` "internal struct" claim in §10.1 but it's `pub`

§10.1 says "`ExternalGrpcSpawnConfig` is pub but the type isn't exported outside `oneshim-web`." Checking `tests/external_grpc_integration.rs:37` — the test imports it directly: `use oneshim_web::grpc::external::spawn_config::ExternalGrpcSpawnConfig;`. So it IS exported (at least to tests). Add a note that `#[cfg(feature = "test-support")]`-gated tests use it and must update.

---

## Strengths

- **Goals are measurable and numbered** (G1-G5). Rare in specs of this size.
- **Status mapping table** (§5.3) is concrete and exhaustive (16 tonic::Code variants covered by table-driven test §9.1.3).
- **Phase 9 coexistence (§10.2)** pre-empts a likely merge conflict with a verifiable check. Excellent forward-planning.
- **§8 failure table** is unusually thorough — every failure mode has a row, action, and audit visibility column.
- **OQ list (§13)** is honest about open questions and closes OQ4 inline.
- **Decisions table (§12)** is genuinely useful — 20 decisions with rationale means reviewers don't re-litigate.
- **§9.4 perf regression budget** (≤200µs median) is a concrete G5 number most specs skip.
- **Memory consistency**: spec references `reference_tonic_layer_order.md` memory and applies its lesson (D14) — exactly the reuse pattern we want.

---

## Questions raised

- **Q1** (C1 follow-up): Is adding `entries_by_command_id` to `AuditLogPort` in scope, or should correlation rely on `/api/audit/export` + client-side grep? (Affects D16 semantics.)
- **Q2** (C2 follow-up): Does the spec team accept conflating `Unauthenticated` into `Denied` without persisting raw grpc-status, or do we add a `grpc_status_code: u32` field to `ExternalGrpcAuditDetails`?
- **Q3** (I4 follow-up): Should `RequestIdLayer` move outermost (before AuthLayer) so auth-rejected rows have matching `command_id`? Affects D14 rationale.
- **Q4** (C3 follow-up): OQ6 must be closed in this loop, not deferred. Which option (A client-drop, B handler-returns-cancelled) does the plan adopt?
- **Q5** (I2 follow-up): Is in-scope a `GET /api/external-grpc/live-config` REST endpoint, or is operator verification strictly log-grep?
- **Q6** (I6 follow-up): Does §14 doc criterion include correcting the existing doc's aspirational Denied/Timeout claim?
- **Q7** (M1 follow-up): Does the existing `CapturingAudit` mock need rewrite, or does the spec commit to a single-write audit bridge (drop `log_event` double-write)?
- **Q8** (I3 follow-up): Will existing tests `external_grpc_request_id_header_returned` and `external_grpc_audit_completed_entry_written_after_ok_response` be replaced, extended, or duplicated?
