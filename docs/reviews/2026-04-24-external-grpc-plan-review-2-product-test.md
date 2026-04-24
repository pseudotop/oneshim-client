# Plan Review — Loop 2 Round 1 — Product & Test Lens

**Plan**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` (commit `6bd654ff`, 3204 lines)
**Spec**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-spec.md` (rev-4, commit `659bcebd`)
**Lens**: TDD flow adequacy, test coverage vs spec §9, acceptance criteria measurability, operator workflow, regression risk, doc/PR deliverables, REPLACE/EXTEND handling.

---

## Critical

### C1. Task 0.4 test fixture calls a non-existent `storage.log_entry(entry)` — test body is unrunnable

Task 0.4 (L526-586) tests `SqliteStorage::entries_by_command_id` by calling `storage.log_entry(entry).await;`. No such method exists on `AuditLogPort` (`crates/oneshim-core/src/ports/audit_log.rs:46-68` — real write methods are `log_event`, `log_start_if`, `log_complete_with_time`). Plan L589 says parenthetically "Adjust to use existing helpers like `audit_entry_fixture` if present" — none exists in `crates/oneshim-storage/src/sqlite/`.

Wrong `AuditEntry` field names too: plan writes `id: format!("id-{i}")` but actual is `entry_id: String` (`crates/oneshim-core/src/models/audit.rs:27`); plan writes `details: "{}".to_string()` but actual is `details: Option<String>`.

**Impact**: Implementer must synthesize an `insert_audit_row` helper (extra scope) or call port methods — plan specifies neither. The "test → failure → impl → pass" cycle requires the test to at least compile.

**Fix**: Replace test body with real port-method calls (e.g., `storage.log_start_if(AuditLevel::Basic, "cmd-X", "s", "test").await`) using struct's real field names. The task claims "COMPLETE (not pseudocode)" per spec §Global Conventions — that claim is currently false.

### C2. Task 0.6 / 3.1 / 6.1 rely on undefined test fixtures — `fixture_bridge()`, `InnerEcho::with_trailer_status`, `PeerInfo::fixture()`

Task 0.6 L766-788 asserts `entry.command_id == "req-abc-123"`, but the existing `CapturingAudit` in `tests/external_grpc_integration.rs:1495-1517` overwrites `command_id` with `action_type` in `log_event` (L1511). So the assertion can never pass against the current fixture. The plan says "use existing test fixture helper" (L770) with no pointer.

Task 3.1 similarly references `fixture_bridge()` (L2278), `fixture_metrics()` (L2279), `InnerEcho::with_trailer_status(0)` / `trailers_only_with_status(7)` (L2281, L2308), `AuthContext::fixture()` / `PeerInfo::fixture()` (L2284-2285). Task 6.1 calls `fixture_bridge()` (L2740) and `PassthroughInner` (L2742). None of these helpers exists in the codebase, and the plan gives no implementation, signature, or location.

**Impact**: Tasks 0.6 / 3.1 / 6.1 cannot be copy-pasted. The TDD "run test → failure → impl → pass" cycle requires compilation; it won't happen.

**Fix**: Add a Phase-0.x task: "Create `fixture_bridge()` + `InnerEcho` + `PeerInfo::fixture()` test-support helpers in `crates/oneshim-web/src/grpc/external/testing.rs`" with full body. Or stipulate reuse by pointing at concrete existing files and line ranges.

### C3. G3 ≤1s convergence test body is absent — Task 9.4 is handwaved

Spec G3 (L67) and §9.2 `external_grpc_live_streaming_toggle_reflects_within_1s` mandate a CI test asserting `start.elapsed() < Duration::from_secs(1)`. Plan Task 9.4 (L3106): "Live reload (6 tests) — two commits (3 each to keep under 100-line-diff rule)" — no test body, no trigger mechanism, no client call, no assertion code.

Unclear:
- How is the config change triggered? `ConfigManager::update_with(|c| c.external_grpc.streaming_enabled = Some(false))`? The real `update_with` writes to disk via `save_to_file` (`config_manager.rs:128`) — is that acceptable in a CI test?
- What's the client call observing the change? Spec says "next SubscribeMetrics returns `Unavailable`" but the tonic `Status::code() == Code::Unavailable` check isn't shown anywhere.
- When does the `start` stopwatch fire — before `update_with` or after?

**Impact**: G3 is the primary operator-visible SLO. A guess risks a test that passes for the wrong reason (e.g., stale snapshot). G3 is the only gate on D33's CI bound.

**Fix**: Phase 9 Task 9.4 must contain a complete code block for at least `external_grpc_live_streaming_toggle_reflects_within_1s`, at the same completeness level as Task 1.2's `request_id_layer` tests.

---

## Important

### I1. Phase 9 REPLACE/EXTEND tests delegate body-writing to implementer — and existing `CapturingAudit` is structurally incompatible

Plan L3090: "For the REPLACE `external_grpc_request_id_header_returned` (L933-ish), delete the TODO-stub body and write the real incoming-preserved assertion." The existing test at L933-966 is a JWT-smoke test exercising `get_agent_info` — not a stub. Replacing it requires knowing the tonic 0.14 API for reading `response.metadata()` (spec doesn't clarify whether `x-request-id` lives in `response.metadata()` or an HTTP trailer).

The EXTEND additions in spec §9.2 (`command_id matches incoming/generated request ID and grpc_status_code = 0 in details JSON`) require `CapturingAudit` itself to be rewritten — the existing `log_event` at L1495-1517 maps `command_id = action_type`, so the EXTEND assertion cannot pass. Plan Phase 9 allocates no task for updating `CapturingAudit`.

**Fix**: Add Task 9.0 "Rewrite `CapturingAudit` to preserve real `command_id` + surface `grpc_status_code`" with new body, OR fold into Task 0.3/0.6.

### I2. Task 4.2 AppState population is hand-waved — "after constructing live and metrics_arc"

Task 7.1 L2826: "Populate these fields from `build_external_spawn_config` in `src-tauri/src/app_runtime_launch.rs` after constructing `live` and `metrics_arc`." But Task 4.2 (L2494-2526) — where `live` and `metrics_arc` are constructed — shows only the `tokio::spawn(run_config_reload(...))` wiring, not the AppState write. The plan never shows where AppState is mutable at that call site, nor how `build_external_spawn_config` returns the `live` handle back to its caller (signature at L2488 returns only `ExternalGrpcSpawnConfig`).

**Impact**: AppState fields will not be populated until someone guesses the right integration point. This is the load-bearing piece connecting the live-config handler (D29) to its data. If broken silently, `/api/external-grpc/live-config` always returns 503 — the feature is invisibly disabled.

**Fix**: Task 4.2 or Task 7.1 must show the `.external_grpc_live = Some(live.clone())` write in its exact function context, with enough surrounding diff to be unambiguous.

### I3. G5 performance bench is explicitly deferred but spec says "reject if regression >500µs"

Spec §9.4 defines a G5 regression bench (`cargo bench --bench external_grpc_overhead`) with a ≤200µs p50 target and 500µs rejection. Plan Self-Review L3178: "bench deferred to manual PR validation." Plan Task 10.3 runs `cargo check/test/clippy/fmt` — no bench reference. Task 10.4 doesn't instruct the implementer to attach bench numbers.

**Impact**: G5 is an acceptance criterion. The 200µs target is load-bearing — `TrailerCapturingBody`'s `pin-project` + oneshot + deferred spawn are non-trivial additions to the hot path.

**Fix**: Add Task 10.3b "Produce bench numbers against `main` at `5618558c` baseline and include in PR description" with exact commands. OR explicitly demote G5 in plan Self-Review (and addendum the spec). Silent deferral leaves plan misaligned.

### I4. Task 5.2 signature change: caller inventory is missing

Task 5.2 (L2650-2719) changes `subscribe_metrics`/`subscribe_events` signatures from `(streaming_enabled: bool, load_policy: Arc<LoadPolicy>, ...)` to `(streaming_source: StreamingSource, ...)`. The plan only addresses the in-file dispatch (L2693). The sibling `grpc_dashboard_integration.rs` test file likely constructs `DashboardServiceImpl` manually with `streaming_enabled: true, load_policy: Arc::new(...)` — same shape as `external_grpc_integration.rs`'s `make_jwt_config` at L193-197. It will fail to compile after Task 5.1's field swap.

**Fix**: Task 5.1 Step 2 should `grep -rn "streaming_enabled:\|load_policy:" crates/oneshim-web/ src-tauri/` and list each hit. At minimum, explicitly name `grpc_dashboard_integration.rs`.

### I5. Task 0.6 call-site inventory missing — "Update all existing call sites" without an enumeration

Task 0.6 L840: "Update all existing call sites inside `audit_bridge.rs` AND in `audit_layer.rs`/`auth_layer.rs`". Never enumerated. Task 6.1 L2769 reveals `AuthLayer` has 4 Failed-path `bridge.record(..)` sites; `AuditLayer` has ≥1 caller. TDD Step 5 "no regression" passes only if all sites compile.

**Fix**: Task 0.6 Step 1 must add: `rg 'bridge\.(record|record_completion)\(' crates/oneshim-web/src/grpc/external/` and list each match (file:line); every listed site updated in Step 4.

### I6. OpenAPI yaml edit for `/api/external-grpc/live-config` missing

File-structure table (L119) lists `docs/contracts/oneshim-web.v1.openapi.yaml` as modified in Phase 7. Task 7.2 (L2989-3013) adds `/api/audit/export` yaml. Task 7.1 (L2805-2965) adds no yaml entry for `/api/external-grpc/live-config`. Contract-frozen enforcement per CLAUDE.md ("Contract-frozen via `docs/contracts/oneshim-web.v1.openapi.yaml`") may trip the workspace lint.

**Fix**: Task 7.1 Step 3 include yaml path block analogous to Task 7.2's (L2989). Check `http-interface-manifest.v1.json` for rebuild directive.

### I7. Task 10.4 PR description — no Phase 10 task ensures spec §14's 11 exit criteria

Spec §14 (L1579-1595) lists 11 concrete PR exit criteria. Task 10.4 L3161: "Create `.github/pr-description-draft.md` (local only, not committed)." Missing:
- Memory update `reference_tonic_layer_order.md` (§14 bullet 4)
- Phase 9 `merge-tree` check output capture (bullet 11) — Task 10.3 L3153-3157 runs but discards
- Doc-line-171 aspirational-text correction (bullet 5.a)

**Fix**: Task 10.4 include spec §14's checklist as PR skeleton + explicit commits for memory updates.

---

## Minor

### M1. Task 2.1 test `external_override_wins_over_web_field` uses `Arc::make_mut(c)` trick for zero-change detection

L2132: `config_tx.send_modify(|c| { Arc::make_mut(c); });  // mutate to trigger change`. This is a no-op modification to force a `watch::Sender::send_modify` to publish. It's semantically odd — `Arc::make_mut` on an `Arc<AppConfig>` with a single strong ref returns a `&mut AppConfig` without cloning (no change needed). If the test runs after the initial subscription is caught up (via `config_rx.changed()` in the task), this may not actually trigger a change event depending on watch semantics. Cleaner: `send_replace` with the same value or use `send_modify(|_| ())` — but `watch` might de-dup these. Test may be flaky.

**Fix**: Replace with an actual mutation like `send_modify(|c| { Arc::make_mut(c).external_grpc.streaming_enabled = Some(true); })`.

### M2. Task 1.3 `first_trailer_wins_on_multiple` test doesn't actually test "first wins"

L1765-1774: the test's `FixtureBody` only emits one trailer frame, so the "first trailer wins" invariant is never exercised. The comment "This is a smoke test; our FixtureBody only emits one trailer frame" acknowledges this. Either upgrade `FixtureBody` to emit two trailers, or rename the test to `single_trailer_captured`.

### M3. Plan Self-Review G4 total says "~49" but §Expected stats says "~49 new tests (32 unit + 17 integration)"; spec §9.5 totals 66 (48 unit + 18 integration)

Plan L22 claims "~49 new tests (32 unit + 17 integration)". Spec L1456 says "~66 test additions (18 integration + 48 unit/contract)". That's a 17-test delta. Tasks 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7 collectively add ~15 tests (I count 3+5+1+3+3+1+1 = 17 in Phase 0 alone, mostly invisible to the plan's count). The plan undercounts.

**Impact**: G4's acceptance threshold is spec-level. An implementer trusting the plan's "49" may stop short of spec's "66" and fail a late-stage review.

### M4. Task 1.2 `accepts_valid_incoming_header` does not assert the extension was populated

L1322-1331 asserts `resp.headers().get(REQUEST_ID_HEADER)` but spec §5.2 D3 specifies the extension (`req.extensions_mut().insert(RequestId(request_id.clone()))`) must ALSO be populated for downstream layers (AuthLayer/AuditLayer) to read. The test verifies only the response-header path. If the implementer forgets `req.extensions_mut().insert(...)` but correctly sets the response header, this test passes — and `failed_audit_reads_request_id_from_extensions` in Task 6.1 fails for a reason unrelated to AuthLayer's logic.

**Fix**: Add a test `inner_service_sees_request_id_in_extensions` that wraps an inner service asserting `req.extensions().get::<RequestId>().is_some()`.

---

## Strengths

- **TDD rigor per task is strong where it's applied.** Tasks 0.1, 0.2, 0.5, 0.7, 1.1, 1.2, 1.3, 1.4, 2.1 show complete, copy-pasteable test bodies with concrete expected-output commentary. The `request_id_layer::tests` block (L1290-1444) is exemplary — 10 named tests, clear boundary coverage, `EchoService` test double defined inline.
- **Torn-read invariant test (Task 1.1 `snapshot_observes_consistent_cross_field_view`, L1075-1120)** proactively defends D21 with a thread-pair assertion; this is the right level of rigor for the core atomicity claim.
- **Spec §9.1 unit-test list is fully enumerated across Phase 0-2 tasks.** The plan's per-task test count closely matches spec §9.5's unit totals (4+10+10+7+9+3+4+3 = ~50, spec says 48 — close enough given optional tests).
- **Phase 0 "prerequisites" framing correctly isolates low-risk changes** (`thiserror` additions, struct field extensions, port trait additions) before cross-cutting rewrites in Phase 3+. This matches the spec's risk-ordering intent.
- **`map_code_to_audit_status` table-driven test (L1728-1753) covers all 16 `tonic::Code` variants + None** — this is the kind of exhaustive table that catches silent semantic drift and supports G2's "mapped deterministically" claim.
- **Task 0.5's three-test backward-compat trio (L675-702)** (Serde accept / skip-when-None / deserialize-old-row) is textbook API-evolution coverage and prevents prod-data corruption on deploy.

---

## Questions

1. **Spec §9.1.4 lists 5 config_reload unit tests; Task 2.1 shows 6. Which extra?** The plan adds `biased_shutdown_preempts_config_change` and `exits_on_config_sender_drop` but omits spec §9.1.4's `load_policy_constructed_from_thresholds` (verify via public accessor). Intentional?

2. **Does the `config_rx.borrow_and_update()` call at Task 2.1 L2018 hold a `watch::Ref` across potentially-awaiting code?** The plan's comment says "Ref dropped at end of statement; no await held across borrow", but `apply_config(&live, &config_rx.borrow_and_update())` — is `apply_config` sync? (It appears synchronous at L2028.) Worth confirming no future hot-loop refactor breaks this contract.

3. **How is the integration test for `external_grpc_live_load_thresholds_applied_without_warmup_reset` (spec §9.2 L1409) expected to "wait 30s"?** In CI this would exceed the 1-second convergence bound elsewhere and likely be flaky. Is there a time-mocking facility in use (`tokio::time::pause`)? Task 9.4 is silent.

4. **Task 3.1 Step 3 L2345 says "Full code per spec §5.5 rev-4 (see … spec file L559-688)".** Does the implementer literally open the spec file in a second window and transcribe 130 lines? This offloads the most critical code rewrite to the implementer's transcription skills — is this acceptable given the rest of the plan shows code inline?

5. **Task 4.1 L2402-2405 renames Debug fields to `streaming_enabled_live` / `load_policy_snapshot_summary`** — these are text strings in assertion output. Is downstream log-analysis tooling (loki/grafana queries) updated? If operators grep for "streaming_enabled = " in old logs, the new format breaks dashboards silently.

---

*End of Loop 2 Round 1 Product-Test review. ~2430 words.*
