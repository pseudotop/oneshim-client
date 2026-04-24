# PR-B3 Spec Delta Brief — D13 V2b SubscribeEvents

**Date**: 2026-04-22
**Worktree branch**: `feature/d13-v2a-get-session-stats` (reading remote branches via `git show`)
**Status**: spec iter-5 (post-iter-4 code-reviewer: 2 Important — line number + stale test count — resolved)

**Source docs**:
- Design: `docs/reviews/2026-04-21-d13-v2b-streaming-design.md` (branch `feature/d13-v2b-streaming-design`)
- Plan: `docs/reviews/2026-04-21-d13-v2b-streaming-plan.md` L2128-2965 (same branch)

---

## Scope (from Design + Plan Tasks B3-1..B3-8, revised)

0. **B3-0 (precursor, first commit)**: Re-add `pii_sanitizer: Option<Arc<dyn PiiSanitizer>>` to `GrpcSpawnConfig` + wire passthrough in `src-tauri/src/app_runtime_launch.rs:786` (was dropped in PR-B2 per CRIT-15/IMP-17, required for B3-6 PII sanitisation)
1. **Emission wiring** (prerequisites): B3-1 Frame / B3-2 Idle / B3-3 AiRuntimeStatus (snapshot-only per §A.A2)
2. **Handler components**: `drop_accumulator.rs`, `rate_limiter.rs`, `subscribe_events.rs` under `crates/oneshim-web/src/grpc/`
3. **Integration tests (B3-7)**: 12 tests enumerated (iter-3 NI2 + iter-4 plan-phase handoff inlined):
   - (1) `subscribe_events_streams_frame_after_capture` — B3-1 end-to-end
   - (2) `subscribe_events_streams_idle_on_edge_only` — B3-2 edge detection (no mid-idle spam)
   - (3) `subscribe_events_filters_by_event_types` — `event_types` list honored
   - (4) `subscribe_events_drops_when_rate_limited` — DropAccumulator + DroppedEventsSignal
   - (5) `subscribe_events_emits_server_load_hint_on_high_cpu` — §3 enforcement ladder
   - (6) `subscribe_events_multiple_concurrent_clients_independent` — fairness / no cross-talk
   - (7) `subscribe_ai_runtime_status_sends_sentinel_when_none` (iter-2 new) — §A.A2 C2 resolution
   - (8) `subscribe_only_ai_runtime_status_keeps_stream_open` (iter-2 new, renamed from "_with_pings" per NC2) — §E lifecycle
   - (9) `subscribe_events_sanitizes_ai_runtime_status_fallback_reason` — PII test for `ocr_fallback_reason`/`llm_fallback_reason` (plan L2935)
   - (10) `subscribe_events_emits_idle_even_when_storage_fails` (iter-4 added) — U2 I2 invariant: storage write Err does NOT suppress Idle event emission
   - (11) `subscribe_events_skips_frame_emit_when_save_fails` (iter-4 added) — I3 Err branch: save_frame Err → no Frame event, warn log observable
   - (12) `grpc_server_configures_http2_keepalive` (iter-4 N1 B3-9 added) — verifies `Server::builder().http2_keepalive_interval(...)` is present; smoke or compile-time
4. **Docs (B3-8)**: `docs/guides/grpc-client.md` V2b section + `IdleTracker` cold-start edge behavior documentation (U7)
5. **FrameUpdate consumer migration** (iter-2 C1): 4 sites in single commit (see U3)
6. **Effort**: ~780 LoC / 1.6d (iter-4 revised up: B3-9 keepalive config ~4 LoC + 3 added tests #10-#12 + minor doc updates)

---

## Drift vs Landed Code

### D1. `pii_sanitizer` MISSING from `GrpcSpawnConfig` (CONFIRMED)
- `crates/oneshim-web/src/grpc/spawn_config.rs` at PR-B2 tip has no `pii_sanitizer` field
- Plan L1598 specified it; dropped per PR-B2 CRIT-15/IMP-17
- PR-B3 must re-add + wire `AppState.diagnostics.pii_sanitizer.clone()` in `src-tauri/src/app_runtime_launch.rs:786`

### D2. `frames` schema matches plan
`crates/oneshim-storage/src/migration/v01_v08.rs:36-48` — all fields present and non-null (`app_name`, `window_title` NOT NULL)

### D3. `RealtimeEvent` enum matches
`crates/oneshim-api-contracts/src/stream.rs` has 5 variants (`Metrics/Frame/Idle/AiRuntimeStatus/Ping`)

### D4. Production `event_tx.send` = ZERO
Only `RealtimeEvent::Metrics` at `src-tauri/src/scheduler/loops/system.rs:49`. Plan's assumption holds.

### D5. Frame insert site IS NOT in `oneshim-vision::processor` (plan WRONG)
- **Actual site**: `src-tauri/src/scheduler/loops/helpers.rs:181-188` (post-commit after `sqlite.save_frame_metadata_with_bounds`)
- `save_frame_metadata_with_bounds` signature (verified):
  - `SchedulerStorage` trait (`src-tauri/src/scheduler/config.rs:15-22`) → `Result<i64, CoreError>`
  - `SqliteStorage` impl (`crates/oneshim-storage/src/sqlite/frames.rs:59-101`) → `Result<i64, StorageError>`, uses `conn.last_insert_rowid()`
- **Call-site pattern issue** (per iter-1 I3): current helpers.rs:181 uses `if let Err(e) = sqlite.save_frame_metadata_with_bounds(...)` — **discards `Ok(i64)`**. Emission must refactor to `match ... { Ok(id) => { emit Frame(FrameUpdate { id, ... }); }, Err(e) => warn!(...) }`. No port signature change needed — only call-site pattern change.

### D6. Idle edge detection IS NOT in `oneshim-monitor/activity.rs` (plan WRONG)
- **Actual site**: `src-tauri/src/scheduler/loops/helpers.rs:208-248 handle_idle_tick()`
- Edge detection at lines 219 (Active→Idle), 227 (Idle→Active)
- `IdleTracker` (`crates/oneshim-monitor/src/idle.rs:5-68`) has no `event_tx`, uses `IdleState` enum (not `bool`)
- Plan's `ActivityTracker::new_with_event_tx_primed` + `observe_idle(bool)` **DOES NOT EXIST**

### D7. `ai_runtime_status` mutation sites (EXPANDED FINDING — CRITICAL)
**All sites are BUILD-TIME only — ZERO live-runtime mutations:**
- `src-tauri/src/automation_controller_builder.rs:136, 156` — initial build
- `crates/oneshim-web/src/runtime_bindings.rs:40` — builder field
- `crates/oneshim-web/src/lib.rs:153` — `with_ai_runtime_status` builder method
- `crates/oneshim-web/src/lib.rs:277` — `apply_bindings` (also build-time)
- `crates/oneshim-web/src/lib.rs:632` — **test-only** setup

**No OCR/LLM fallback runtime detection** exists in `oneshim-automation`/`oneshim-analysis`/`oneshim-network` that mutates this field.

→ **Plan B3-3 premise is invalid**: the event has no real source. Resolution: see CRITICAL ISSUES §A.

### D8. `PiiSanitizer` port exists
`crates/oneshim-core/src/ports/pii_sanitizer.rs:15-19` — trait with `sanitize_text(&self, text: &str, level: PiiFilterLevel) -> String`. Plumbed via `AppState.diagnostics.pii_sanitizer`.

### D9. `DashboardStreamingStorage` + proto messages landed
- Sub-trait at `crates/oneshim-core/src/ports/web_storage.rs:407-437` (PR-B2 branch). Correct signature (verified):
  ```rust
  fn fetch_dashboard_event_source(
      &self,
      signal: &DashboardEventSignal,
  ) -> Result<DashboardEventRecord, CoreError>;
  ```
  (Not `(id: i64) -> DashboardEventRecord::Frame` as spec iter-1 stated. Per iter-1 I1 correction.)
- Proto messages `SubscribeEvents{Request,Response}`, `DashboardEvent`, `FrameEvent`, `IdleEvent`, `AiRuntimeStatusEvent` all defined on PR-B2 branch `api/proto/oneshim/dashboard/v1/dashboard.proto` L42, L165-207
- **Consequence with §B resolution**: `fetch_dashboard_event_source` has **zero call sites in PR-B3 handler implementation**. Method remains reserved for post-restart gap-fill queries (out of scope). `FrameUpdate.trigger_type` makes the Frame path pure payload transform.

### D10. `FrameUpdate` contract vs `FrameEvent` proto mismatch
- `FrameUpdate` (contract): `id, timestamp, app_name, window_title, importance` (5 fields)
- `FrameEvent` (proto): `frame_id, app_name, window_title, importance, trigger_type` (5 fields, different shape)
- Missing in `FrameUpdate`: `trigger_type`
- Missing in `FrameEvent`: `timestamp` (carried via enclosing `DashboardEvent.occurred_at`)

---

## Resolved Unknowns (spec iter-1)

### U1. `pii_sanitizer` re-add location → **PR-B3 first commit**
Re-add to `GrpcSpawnConfig` + wire passthrough in `app_runtime_launch.rs:786` as the first PR-B3 commit. Not a precursor on PR-B2 (keep PR-B2 frozen for merge).

### U2. Idle wiring layer → **Caller-side emit in `handle_idle_tick`**, emit AFTER storage, BEFORE notif
Thread `event_tx: &Option<broadcast::Sender<RealtimeEvent>>` into `handle_idle_tick`. `IdleTracker` stays pure (no `event_tx` awareness).

**Ordering** (per iter-1 I2):
1. `sqlite.start_idle_period(Utc::now()).await` at L220 — storage first. If storage fails, `warn!` and still emit event (informational).
2. `event_tx.send(RealtimeEvent::Idle(IdleUpdate { is_idle: true, idle_secs }))` — emit AFTER storage branch (success or failure). Independent of storage success — subscribers should see edge even if DB write failed.
3. Symmetric for Active→Idle→Active at L227-237.
4. Emit is placed BEFORE `notif.check_idle` at L248 — subscribers not coupled to notification throttle state.

Matches existing scheduler-loop-helper pattern.

### U3. `FrameEvent.trigger_type` source → **Extend `FrameUpdate` with `trigger_type`** (REVISED 2026-04-22)
`trigger_type` is already in-memory at emission site: `frame.metadata.trigger_type` (`src-tauri/src/scheduler/loops/helpers.rs:132` already debug-logs it). `FrameMetadata` (`crates/oneshim-core/src/models/frame.rs:50`) carries `trigger_type: String`.

**Resolution**: Add `pub trigger_type: String` to `FrameUpdate` (`crates/oneshim-api-contracts/src/stream.rs:35-41`). Emission site clones `frame.metadata.trigger_type`. Handler reads `FrameUpdate.trigger_type` directly, populates proto `FrameEvent.trigger_type`.

**Consumer migration checklist** (iter-1 C1 — 4 sites, single commit, option (b) migrate-all):
1. **Emit site**: `src-tauri/src/scheduler/loops/helpers.rs:181-188` — new `trigger_type: frame.metadata.trigger_type.clone()` field in `FrameUpdate { ... }`
2. **Rust test fixture** `crates/oneshim-web/src/services/stream_assembler.rs:53` — add `trigger_type: "test".to_string()` (or realistic value) to test `FrameUpdate { ... }` literal
3. **Rust test fixture** `crates/oneshim-web/src/handlers/stream.rs:93` — same addition
4. **TS consumer** `crates/oneshim-web/frontend/src/hooks/useSSE.ts:13-19` — `FrameUpdate` interface adds `trigger_type: string`; downstream consumers of this type auto-inherit

**Consequence**: `DashboardStreamingStorage::fetch_dashboard_event_source` DB call is NOT needed for `trigger_type` nor for `occurred_at` (FrameUpdate already has `timestamp`). Handler is pure payload transform. Design L180 "DB-backed when possible" satisfied because `FrameUpdate.timestamp == frame.metadata.timestamp == DB.frames.timestamp` (all point to capture time, no ms-level drift).

### U4. `AiRuntimeStatus.occurred_at` → **Emit-time `Utc::now()`** (conditional on §A)
Design L180 states "else RealtimeEvent wake-up timestamp". Accept emit-time timestamp. No contract change.

### U5. `AiRuntimeStatus` emission strategy → **Snapshot-on-subscribe + None-state sentinel** (§A A2 finalised)
On subscribe with `event_types=["ai_runtime_status"]` (explicit) or empty filter (all three event types):
- Read `AppState.automation.ai_runtime_status: Option<AiRuntimeStatus>` once at subscribe-handler entry (post-auth, post-policy clamp)
- `Some(status)` → emit one `AiRuntimeStatusEvent { ocr_source, llm_source, ocr_fallback_reason, llm_fallback_reason }` (sanitised per §A.sec)
- `None` → emit one `AiRuntimeStatusEvent { ocr_source: "unknown", llm_source: "unknown", ocr_fallback_reason: "", llm_fallback_reason: "" }` (iter-1 C2 sentinel). Preserves snapshot semantics + simple client handling.
- Zero further AiRuntimeStatus emission for the lifetime of the stream.
- Stream stays open for Frame/Idle events (if in filter) + `ServerLoadHint`/`Ping` keepalives. If filter was `["ai_runtime_status"]` ONLY, stream stays open with `Ping` keepalives (iter-1 I4, snapshot-only lifecycle). Client may close at will.

No `AiRuntimeStateHolder::new_with_event_tx` (plan L2262 aspirational, does not exist).

### §A.sec (iter-2 addition + iter-3 minor-2): PII sanitisation of snapshot payload
`ocr_fallback_reason` and `llm_fallback_reason` may contain internal diagnostic strings with indirect PII (file paths, error messages embedding user data). Apply `PiiSanitizer::sanitize_text(reason, PiiFilterLevel::Standard)` before building the proto message.

**iter-3 minor-2 — `None` sanitiser fallback**: When `GrpcSpawnConfig.pii_sanitizer: Option<Arc<dyn PiiSanitizer>>` is `None` (test builds that don't wire the port), the handler passes `*_fallback_reason` fields through **unchanged** (no sanitisation). Acceptable for test builds; production wiring in `app_runtime_launch.rs:786` always provides a sanitiser. The integration test `subscribe_events_sanitizes_ai_runtime_status_fallback_reason` (test #9) must inject a test `PiiSanitizer` impl to exercise the sanitisation path.

This matches what plan B3-7 PII sanitisation test expects (plan L2935).

### U6. PR-B3 branch base → **`origin/main` after PR-B2 merge**
Per memory `project_next_tasks.md` entry conditions. Plan L2116 "Branch off PR-B2 tip" is superseded.

### U7. `IdleTracker` cold-start edge → **Accept** (document)
Fresh tracker starts `previous_state = Active`. If user already idle at app start, first `check_idle` triggers Active→Idle edge — semantically correct from observer POV. Document in user-facing docs.

---

## RESOLVED ISSUES (iter-1 findings, iter-2 decisions applied)

### §A. AiRuntimeStatus has NO live-mutation source → **A2 snapshot-on-subscribe + None sentinel** (iter-1 §A + iter-2 C2 + iter-3 NC1)

Plan B3-3 premise ("event on runtime status change") is invalid — status is set once at server build via `with_ai_runtime_status` / `apply_bindings`; no detection logic produces the signal.

**Resolution (A2 + C2 sentinel)**:
- On subscribe with `event_types` containing `"ai_runtime_status"` (or empty = all): read `AppState.automation.ai_runtime_status: Option<AiRuntimeStatus>` once at handler entry.
- `Some(status)`: build `AiRuntimeStatusEvent` proto directly from `status` fields; apply §A.sec PII sanitisation to `*_fallback_reason` fields before wrap into `DashboardEvent` + push to outbound stream.
- `None`: build sentinel `AiRuntimeStatusEvent` proto directly with `ocr_source="unknown"`, `llm_source="unknown"`, `ocr_fallback_reason=""`, `llm_fallback_reason=""`.
- Zero further AiRuntimeStatus emission for stream lifetime.
- Rate limiter does NOT apply to snapshot. `DropAccumulator` NOT invoked.
- Approx ~40 LoC handler side; stream lifecycle handled per §E (transport keepalive).

**iter-3 NC1 — proto-layer construction**: sentinel is constructed **directly at the proto `AiRuntimeStatusEvent` layer**. The Rust `oneshim_api_contracts::stream::AiRuntimeStatus` contract struct is **not** instantiated in the `None` branch (this avoids ambiguity: `AiRuntimeStatus { ocr_fallback_reason: Some("".to_string()) }` would conflate "explicit empty fallback" with "sentinel"). Handler code is `AiRuntimeStatusEvent { ocr_source: "unknown".to_string(), ... }` directly — bypass the contract struct for the `None` case. `Some(status)` case uses the contract normally.

**Defer**: fallback-detection infrastructure (A3) reserved for v2c, out of PR-B3 scope.

### §B. Extend `FrameUpdate` with `trigger_type` (supersedes DB-lookup)

Emission site (`helpers.rs:181`) already has `frame.metadata.trigger_type` in-memory — no DB round-trip needed. Add `pub trigger_type: String` (non-optional, no `Default`) to `FrameUpdate` struct. Handler becomes pure payload transform (zero DB reads for Frame events). See U3 for 4-site migration checklist.

### §C. Plan's B3-1 / B3-2 / B3-3 wiring files are WRONG (D5 + D6 + D7)

Plan steers implementers to:
- B3-1: `crates/oneshim-vision/src/processor.rs` — **actual**: `src-tauri/src/scheduler/loops/helpers.rs:181-188` (D5)
- B3-2: `crates/oneshim-monitor/src/activity.rs` — **actual**: `src-tauri/src/scheduler/loops/helpers.rs:208-248` (D6)
- B3-3: `AiRuntimeStateHolder::new_with_event_tx` — **does not exist**; snapshot-on-subscribe doesn't need holder struct (D7)

**Action**: Plan iter-1 must correct all three. Otherwise implementers waste time on wrong files.

### §E. (iter-2 I4 + iter-3 NC2 + iter-4 N1) Stream lifecycle when `event_types=["ai_runtime_status"]` only

After snapshot emission, keep the server-streaming RPC open. Do NOT close with OK. Rationale: (a) client-side cost of re-subscribing is high; (b) consistent behavior with filters that include frame/idle (those also stay open).

**iter-3 NC2 — keepalive mechanism (wire level)**: `SubscribeEventsResponse.payload` oneof is `{DashboardEvent event, ServerLoadHint hint, DroppedEventsSignal dropped}` — **no `Ping` variant at the wire level** (verified on PR-B2 tip `api/proto/oneshim/dashboard/v1/dashboard.proto`).

**iter-4 N1 — tonic server keepalive default is OFF**: `tonic 0.14.5 tonic::transport::Server::builder()` defaults `http2_keepalive_interval: Option<Duration>` to `None`. PR-B2's `Server::builder()` call at `crates/oneshim-web/src/grpc/mod.rs:314` (iter-5 line correction; iter-4 erroneously cited :419) does NOT configure keepalive. Without HTTP/2 PING frames, a `event_types=["ai_runtime_status"]`-only (snapshot-only) stream sits fully idle and will be torn down by intermediate routers/NATs/load-balancers (typically 1–5 minute idle timeouts). The iter-2 renamed test `subscribe_only_ai_runtime_status_keeps_stream_open` (test #8) would pass on loopback (no intermediate timeout) but fail in production — masking the bug.

**Resolution (iter-4 N1)**: Add explicit HTTP/2 keepalive configuration to the `Server::builder()` chain. New scope item:

### B3-9 (iter-4 added): Configure tonic HTTP/2 keepalive

**Location**: `crates/oneshim-web/src/grpc/mod.rs:314` (iter-5 line correction; iter-4 erroneously cited :419) (wherever `tonic::transport::Server::builder()` is called for the dashboard service)

**Change**:
```rust
tonic::transport::Server::builder()
    .http2_keepalive_interval(Some(Duration::from_secs(30)))
    .http2_keepalive_timeout(Some(Duration::from_secs(10)))
    // ... existing chain ...
```

**Values**: 30s interval / 10s timeout — conservative defaults aligned with common LB idle budgets (AWS ELB: 350s, GCP: 600s, Cloudflare: 100s). 30s/10s ensures PING success is detectable well before any likely timeout.

**Effort**: ~4 LoC + one integration test to verify keepalive is configured (assert via reflection or smoke test). +0.05d.

**Test #12 (iter-4 added)**: `grpc_server_configures_http2_keepalive` — smoke-level test that `Server::builder()` path emits HTTP/2 PING frames at ~30s cadence. Can be cheap: start server, open stream, observe a PING within ~35s window. If too flaky for CI, reduce to a compile-time check that keepalive config methods are called (via spawn helper unit test).

No proto change needed. `RealtimeEvent::Ping` (internal broadcast-channel variant) is not exposed to the gRPC wire.

---

## Entry Conditions

| # | Condition | Status |
|---|-----------|--------|
| 1 | PR #479 merged | CI green 2026-04-22 (Test ✅ 27m47s); awaiting holistic-review follow-up decisions |
| 2 | Parent submodule bumped | blocked by 1 |
| 3 | New worktree for PR-B3 off latest main | currently on features worktree |

(Iter-1 M2: former entry condition #4 — `pii_sanitizer` re-add — was a work item, moved to Scope as B3-0 precursor.)

---

## Critical Files for Implementation (corrected for drift)

- `crates/oneshim-web/src/grpc/spawn_config.rs` — D1 re-add `pii_sanitizer` field
- `src-tauri/src/app_runtime_launch.rs:786` — D1 wire passthrough
- `src-tauri/src/scheduler/loops/helpers.rs:181-188` — B3-1 Frame emission (D5, NOT processor.rs)
- `src-tauri/src/scheduler/loops/helpers.rs:208-248` — B3-2 Idle emission (D6, NOT activity.rs)
- `crates/oneshim-web/src/lib.rs:153, 277` — B3-3 AiRuntimeStatus (CONDITIONAL on §A)
- `crates/oneshim-web/src/grpc/drop_accumulator.rs` — NEW (B3-4)
- `crates/oneshim-web/src/grpc/rate_limiter.rs` — NEW (B3-5)
- `crates/oneshim-web/src/grpc/subscribe_events.rs` — NEW (B3-6)
- `docs/guides/grpc-client.md` — NEW section (B3-8)

---

## Decisions (spec iter-2 finalised 2026-04-22)

### From spec iter-1 (scope)
1. **§A**: **A2** — snapshot-on-subscribe for AiRuntimeStatus. Send current static config ONCE; zero further emission. ~40 LoC.
2. **§B**: **Extend `FrameUpdate` with `trigger_type: String`** (non-optional). Handler is pure payload transform; zero DB reads.
3. **§C**: Plan iter-1 must correct B3-1/B3-2/B3-3 wiring file references per D5/D6/D7.

### From spec iter-2 (code-reviewer findings)
4. **C1 FrameUpdate consumer migration**: **Option (b)** — migrate all 4 consumer sites in a single commit (emit + stream_assembler test + handlers/stream test + useSSE.ts TS). Makes `trigger_type` a required non-Optional field. Cleaner than `#[serde(default)]` compat shim.
5. **C2 AiRuntimeStatus None sentinel**: **Option (a)** — when `AppState.automation.ai_runtime_status` is `None`, emit `AiRuntimeStatusEvent { ocr_source: "unknown", llm_source: "unknown", ocr_fallback_reason: "", llm_fallback_reason: "" }`. Preserves snapshot semantics; simple client handling.
6. **I3 save_frame return value**: **Call-site refactor** — change `if let Err(e) = ...` to `match sqlite.save_frame_metadata_with_bounds(...) { Ok(frame_id) => { ... emit Frame(FrameUpdate { id: frame_id, ... }); }, Err(e) => warn!(...) }`. No port signature change. **iter-3 NI1 ordering**: The subsequent `sqlite.increment_session_counters(session_id, 0, 1, 0).await` at `helpers.rs:190` runs **unconditionally, OUTSIDE the match block** — behavior preserved from PR-B2. Do NOT move it into the `Ok` branch (that would silently change session-counter semantics on save failure; out of scope for PR-B3).
7. **I4 snapshot-only stream lifecycle**: **Option (a)** — keep stream open after snapshot emission with `Ping` keepalives. Rate limiter does NOT apply to the initial snapshot. `DropAccumulator` NOT invoked for snapshot emission (it's terminal, not rate-limited).

### Other iter-2 resolutions
- **I1** (fetch_dashboard_event_source sig correction): see D9 (updated). Method has zero call sites in PR-B3 handler.
- **I2** (Idle emission ordering): see U2 (updated) — AFTER storage branch, BEFORE notif.
- **M1** (PII invariants): see §D below.
- **M2** (entry condition cleanup): entry condition #4 moved to Scope B3-0.
- **M3** (test count update): Scope §3 updated — 12 integration tests total (iter-5 corrected: 6 plan original + 2 iter-2 + 3 iter-4 added = 11 non-PII, + 1 PII = 12).
- **§A.sec** (PII sanitisation of snapshot): apply `PiiSanitizer::sanitize_text(..., Standard)` to `ocr_fallback_reason` + `llm_fallback_reason` in the snapshot payload.

---

## §D. Privacy Invariants (iter-2 M1 addition)

**Frame emission path is already-sanitised at source**:
- `frame.metadata.app_name` / `frame.metadata.window_title` are sanitised at capture boundary via `oneshim_vision::privacy::sanitize_title_with_level` (`oneshim-vision/src/processor.rs:100`) and `scheduler::config::sanitize_title` (`src-tauri/src/scheduler/config.rs:275-291`) — **before** reaching `helpers.rs:181` emission site.
- `frame.metadata.ocr_text` is sanitised at `helpers.rs:179` before SQLite persist.
- `frame.metadata.trigger_type` is an internal enum string (e.g. `"active_change"`, `"timer"`) with no user content — no sanitisation needed.

**Consequence**: B3-1 Frame emission does **not** apply PII sanitisation at the emission site or handler. All fields are safe to copy verbatim into `FrameUpdate` then `FrameEvent`.

**Idle emission path is inherently PII-free**: `IdleUpdate { is_idle: bool, idle_secs: u64 }` — only booleans + numerics. No sanitisation needed.

**AiRuntimeStatus snapshot path requires sanitisation** (per §A.sec): `ocr_source` / `llm_source` are internal strings (e.g. `"remote"`, `"subprocess_cli"`) — safe. But `ocr_fallback_reason` / `llm_fallback_reason` may contain diagnostic strings embedding user PII (file paths, request URLs, error messages quoting user data). Apply `PiiSanitizer::sanitize_text(reason, PiiFilterLevel::Standard)` at the handler before emission.

**Test**: B3-7 PII-sanitisation test targets the AiRuntimeStatus `*_fallback_reason` path (plan L2935), not the Frame path.
