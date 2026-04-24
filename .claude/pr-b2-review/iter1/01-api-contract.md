# PR-B2 Spec Review — Dimension 1: API Contract & Wire Semantics (iter-1)

**Reviewer**: API contract dimension
**Date**: 2026-04-22
**Overall verdict**: IMPORTANT issues

The spec's "no proto changes" claim is structurally correct (PR-B1 shipped the frozen surface), but multiple fields in the spec's Section 3 wire table and Section 4 generator/hint pseudocode drift from the actually generated proto (`crates/oneshim-web/src/proto/generated/oneshim.dashboard.v1.rs`) in ways that would produce compile errors or silently wrong wire output during implementation. Status-code usage is mostly appropriate but `Status::unimplemented` for a runtime kill-switch warrants a call-out. The client-compat claim (v2a unaffected by `MetricBucket` promotion) holds up under inspection of `grpc/mod.rs:252`.

---

## Critical findings

### C1. Wire contract in Section 3 uses bare enum variant names that diverge from the actually shipped proto

**Problem**: Section 3 of the spec (lines 87-97) quotes the enum as:

```proto
enum Level { LOAD_LEVEL_UNSPECIFIED=0; LOW=1; MEDIUM=2; HIGH=3; CRITICAL=4; }
```

But the actual `dashboard.proto` ships:

```proto
enum Level {
  LOAD_LEVEL_UNSPECIFIED = 0;
  LOAD_LEVEL_LOW = 1;
  LOAD_LEVEL_MEDIUM = 2;
  LOAD_LEVEL_HIGH = 3;
  LOAD_LEVEL_CRITICAL = 4;
}
```

**Evidence**:
- Spec: `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md:88`
- Proto: `api/proto/oneshim/dashboard/v1/dashboard.proto:212-218`
- Generated Rust: `crates/oneshim-web/src/proto/generated/oneshim.dashboard.v1.rs:313-320` — variants are `LoadLevelUnspecified, LoadLevelLow, LoadLevelMedium, LoadLevelHigh, LoadLevelCritical`

**Impact**: Implementor reading the spec would write `Level::Low` / `Level::High` and hit immediate compile errors (`no variant Low on Level`). A spec reviewer consuming only this doc would misunderstand the wire contract and might design client tools that depend on the shorter names.

**Recommended fix**: Replace Section 3's proto block with the verbatim `LOAD_LEVEL_*` prefix form, and update downstream mentions (§4.2 "protobuf enum mapped" comment). Add a note: "Generated Rust variants are `LoadLevelLow`, etc."

---

### C2. `HintEmitter::build_hint` pseudocode invokes a non-existent `LoadPolicy::enforced_frame_rate` method

**Problem**: Section 4.2 pseudocode (line 174) calls:

```
suggested_event_rate_limit = policy.enforced_frame_rate(level) as u32  // set for parity
```

But `LoadPolicy` as specified in §4.1 only defines `classify()` and `enforced_metrics_interval()`. There is no `enforced_frame_rate` method on `LoadPolicy` in either this spec or the upstream design's `LoadPolicy` definition (which is plain `{thresholds, warmup_until}`). The upstream design §3 ladder table lists per-type Frame/Idle/AiStatus caps, but these belong to PR-B3's `EventRateLimiter`, not `LoadPolicy`.

**Evidence**:
- Spec: `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md:174`
- §4.1: `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md:140-147` — only `classify` + `enforced_metrics_interval` defined
- Upstream design §3 (enforcement ladder): lives outside `LoadPolicy`, applied per-RPC in the stream loop

**Impact**: `suggested_event_rate_limit` is declared in PR-B2's scope (§3 wire table). If implementor follows this pseudocode, they either add a stub `enforced_frame_rate` in PR-B2 that arbitrarily mirrors the PR-B3 ladder (expanding PR-B2 scope silently), or they short-circuit with `0` and diverge from the spec.

The cleanest fix mirrors the upstream design §3 `ServerLoadHint` field population: emit literal `0` for PR-B2 (per proto comment "0 = no suggestion"), **explicitly declaring PR-B3 as the one that populates a real value**. The "parity" wording is misleading — parity is already achieved by emitting the field at all; populating it in PR-B2 is out of scope.

**Recommended fix**: Rewrite §4.2 `build_hint` as:

```
suggested_event_rate_limit = 0  // "0 = no suggestion" per proto; PR-B3 populates.
```

Add to §2 Non-goals: "populate `suggested_event_rate_limit` with a nonzero value — PR-B3 scope".

---

## Important findings

### I1. `Status::unimplemented` for the kill-switch is semantically wrong — should be `Status::unavailable` or `Status::failed_precondition`

**Problem**: §3 line 109 and §4.6 line 280 specify that `grpc_streaming_enabled = false` → `Status::unimplemented`. gRPC's canonical `UNIMPLEMENTED` means "this method is not implemented by the server" — typically set at service registration (the tonic-generated router emits UNIMPLEMENTED for unknown methods). Using it for a runtime toggle conflates "not compiled in" with "temporarily disabled".

Clients' reasonable reaction to `UNIMPLEMENTED` is to stop calling the RPC permanently and log an incompatibility warning. A kill-switch reactivation wouldn't be noticed; clients would need a manual restart or cache flush.

`FAILED_PRECONDITION` (operation not allowed in current state) or `UNAVAILABLE` (server not ready to handle this request, retry with backoff) are semantically correct.

**Evidence**:
- Spec: §3 line 109, §4.6 line 280, test at line 429
- The stub at `grpc/mod.rs:323` correctly returns `Status::unimplemented("SubscribeMetrics stub lands in PR-B2")` — the RPC genuinely has no implementation in that case. Reusing the same code for "implementation exists but disabled" loses that distinction.

**Impact**: Client confusion if the kill-switch is ever toggled in production. Also muddies stub semantics — once PR-B2 merges, `Status::unimplemented` should ONLY be reachable for `SubscribeEvents` (PR-B3 stub).

**Recommended fix**: Change to `Status::unavailable("gRPC streaming disabled by config")` or `Status::failed_precondition("grpc_streaming_enabled=false")`. Update §3, §4.6, and the test at line 429 (`rejects_when_streaming_disabled`).

---

### I2. Spec's "no proto changes in PR-B2" claim is true, but the wire-breaking change window is already open from PR-B1

**Problem**: §3 header says "already-landed proto — no changes in PR-B2" (line 60). True per the letter — PR-B2 doesn't touch `.proto`. But the `MetricBucket.start` field-type flip from `string` to `Timestamp` at tag 1 (documented in the proto itself at dashboard.proto:131-132: "Wire format number preserved for the repeated field; start field type flips from string to Timestamp (wire-breaking for that specific field)") is a wire-breaking change for any v2a client that was deployed before PR-B1 and parsed `start` as a string.

The spec's claim that "v2a clients unaffected" (line 47, goal 6) applies only to integration tests (rebuilt from the same proto tree). Real external consumers (if any exist) that imported the old `ProductivityMetricsResponse.MetricBucket` with `string start` will break on wire decode — `Timestamp` is length-delimited (wire type 2) just like `string`, but the payload is an embedded message, not UTF-8 bytes. A v2a-only client would silently decode garbage.

**Evidence**:
- Proto: `api/proto/oneshim/dashboard/v1/dashboard.proto:131-132` explicitly acknowledges the break
- Spec §3 line 60 + goal 6 line 47 don't mention it
- Upstream design §7 "Wire-level: additive only" is similarly silent

**Impact**: PR-B2 doesn't introduce this problem — PR-B1 did. But PR-B2's spec takes a posture ("v2a clients unaffected") that's only true modulo "v2a clients built from the post-PR-B1 proto tree". External CLI tools pinned to pre-PR-B1 protos would break. If no such tools exist (likely — dashboard gRPC is localhost-only and there's no published client SDK yet), it's fine. But the spec should at least acknowledge the boundary.

**Recommended fix**: Add to §3 (below the wire contract table):

> **Wire-compat note**: `MetricBucket.start` flipped from `string` (RFC 3339) to `Timestamp` in PR-B1 (wire types both length-delimited but payload encoding differs). Pre-PR-B1 clients that deployed against the v2a-original proto tree must regenerate. Dashboard gRPC is localhost-only + no published SDK → external impact expected to be zero.

---

### I3. `Status::resource_exhausted` for active-stream cap is correct — but the spec nowhere specifies the cap emission integration point

**Problem**: §6 claims "Enforced via `Status::resource_exhausted`" for the 50-stream cap, and the acceptance criteria expect a test (line 441) asserting the 51st stream fails. But §4.6's pseudocode for `subscribe_metrics` never checks the counter — the counter check is implicit in "subscribe entry, decrement on generator drop" in §6 prose, with no code site shown. An implementor writing §4.6's generator literally would not include the cap check, and the test would fail (or they'd bolt it on ad-hoc outside the pseudocode).

**Evidence**:
- Spec: §4.6 (line 278-335) — no `active_stream_counter` reference
- §6 line 386 — asserts the counter is "added" in PR-B2, but no integration point
- Tests at line 440 — relies on the counter existing

**Impact**: Ambiguity — does the counter live inside `DashboardServiceImpl` or `GrpcSpawnConfig`? Is the increment at the first `async_stream!` yield or at the outer `async fn subscribe_metrics` entry? The two are meaningfully different for the "drop decrements" invariant: if incremented inside the generator, a `ResourceExhausted` check wraps it and the decrement happens on generator Drop (RAII guard). If at the outer fn entry, you need a guard struct threaded through.

**Recommended fix**: Add a §4.6.1 sub-section sketching the counter via an RAII guard:

```rust
struct StreamCounterGuard(Arc<AtomicUsize>);
impl StreamCounterGuard {
    fn try_acquire(cnt: Arc<AtomicUsize>, cap: usize) -> Result<Self, Status> {
        let prev = cnt.fetch_add(1, Ordering::AcqRel);
        if prev >= cap {
            cnt.fetch_sub(1, Ordering::AcqRel);  // unwind
            return Err(Status::resource_exhausted(
                format!("active gRPC streams cap ({cap}) reached")
            ));
        }
        Ok(Self(cnt))
    }
}
impl Drop for StreamCounterGuard { fn drop(&mut self) { self.0.fetch_sub(1, Ordering::AcqRel); } }
```

The guard must be moved into the `async_stream!` closure so it decrements on graceful OR abrupt stream termination.

---

### I4. §3 clamping table has an internally inconsistent row and omits the canonical `interval_secs ∈ {1,…,60}` case

**Problem**: §3 table line 104:

| `interval_secs` in (0, 1) requested as zero-second — not representable (uint32) | N/A — uint32 starts at 0 |

This row is confused. uint32 has no fractional values, so "(0, 1) requested as zero-second" is nonsensical (there's no value between 0 and 1). The comment "N/A — uint32 starts at 0" is also technically wrong — uint32 starts at 0 in the type system sense but `interval_secs=0` has a distinct semantic meaning per the first row (realtime). The row appears to be a leftover from a brainstorm where fractional seconds were considered.

Additionally, the table has no row for the canonical `interval_secs ∈ {1, …, 60}` case (just the edges).

**Evidence**: Spec `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md:104`

**Impact**: Spec reviewer confusion, and if a later reviewer tries to resolve the ambiguity they might propose an `interval_ms` field (scope creep).

**Recommended fix**: Delete the confused row. Add a row: `interval_secs ∈ {1,…,60}` → `Duration::from_secs(n)` (subject to load-policy ladder floor per §4.1).

---

### I5. Window timestamps drift when level transitions mid-wait (spec's Section 4.6 invariant is unenforceable)

**Problem**: §4.6 semantic choice #1 (line 339): "Simpler invariant: every data payload has `window_start = now - effective_interval`."

But §4.6 pseudocode line 305 computes `effective_interval` **before** the `recv()` wait (realtime) or `sleep()` (interval). The `window_start = Utc::now() - chrono::Duration::from_std(effective_interval)` at line 326-327 uses the `effective_interval` computed before the wait, then does `Utc::now()` AFTER the wait. Under heavy level transitions (MEDIUM→CRITICAL mid-sleep), a pre-transition MEDIUM `effective_interval` (1s) might be used to label a window where the actual wait was 30s — the bucket's aggregated data covers the 30s window but the `start` field claims it's 1s wide.

The bucket's semantic meaning is "the window this bucket aggregates over." If `storage.aggregate_metrics_window(window_start, window_end)` at line 328 queries rows between the computed pair, the DB query width and the wait duration can diverge.

**Evidence**: Spec §4.6 line 305 (compute) vs line 326-328 (use after wait)

**Impact**: Minor data fidelity issue — buckets report wrong window widths during level transitions. Worst case: the `cpu_avg_pct` / `memory_avg_mb` are averaged over a 30s real window but labeled as a 1s window. Dashboards would show anomalous spikes.

**Recommended fix**: Either:
1. Capture `window_end = Utc::now()` BEFORE the wait, and `window_start = window_end - effective_interval`. Then after the wait, use the captured pair (the bucket aggregates a pre-wait window, which is the "cadence-driven sample" semantics).
2. Capture `wait_start = Utc::now()` before wait, `wait_end = Utc::now()` after wait, use `[wait_start, wait_end]` as the window. More accurate but shifts `MetricBucket.start` semantics from "now - effective_interval" to "actual wait start".

Spec should pick one and fix §4.6 text.

---

### I6. Generator's DB-failure `continue` in interval mode produces a 2×interval silent gap

**Problem**: §4.6 line 331 "Transient DB — skip tick" — `continue`s to the next loop iteration. In `interval_secs > 0` mode, the loop head re-enters `tokio::time::sleep(effective_interval)` — so a failure mid-tick becomes an extra full-interval delay (e.g. CRITICAL 30s DB hiccup → 30s sleep → another 30s wait = 60s silent gap between data payloads).

In realtime mode, `continue` skips back to `rx.recv()` but we've already consumed the wake-up; the next data arrives on the next Metrics tick (~5s at monitor cadence).

More subtly: the `hint_emitter.maybe_emit()` call at line 301 ran successfully BEFORE the DB call. The next iteration runs it again. In the realtime path, this is fine (hints only emit on transition). In interval mode, if a level transition happened during the sleep AND the DB failed, the emitter would have emitted at the top of iteration N, and iteration N+1 would not re-emit (same level) — correct. So no double-emit bug. But the prose doesn't explain this.

**Evidence**:
- Spec: §4.6 line 331 (`warn!(...); continue`)
- Spec line 343: "consecutive failures do NOT get counted" — acknowledges this explicitly but doesn't address the silent gap

**Impact**: UX: client observes 60s+ data gaps under DB stress. Spec's acceptance criterion 10 (line 468) says "10 existing integration tests pass" but doesn't test this scenario.

**Recommended fix**: Document the gap explicitly in §4.6: "In interval mode, a DB failure causes up to 2×effective_interval gap between data payloads. Clients should use the 30s heartbeat hint to distinguish a silent stream from a failed server." Or alternatively retry-with-short-backoff (100ms) before `continue` in interval mode.

---

### I7. `honor_opt_out` IPv6 loopback handling underspecified for test expectations

**Problem**: §7 unit tests include "opt_out_honored_on_loopback (v4 + v6)" at line 415. The implementation uses `addr.ip().is_loopback()` (line 196), which for IPv6 returns true only for `::1` exactly, not for `::ffff:127.0.0.1` (IPv4-mapped IPv6, which Linux uses when bound to `[::]:port` and a v4 client connects).

**Evidence**:
- Spec §4.3 line 196: `addr.ip().is_loopback()`
- Tests line 415: "opt_out_honored_on_loopback (v4 + v6)"
- `std::net::Ipv6Addr::is_loopback` returns true only for `::1`. IPv4-mapped loopback (`::ffff:127.0.0.1`) returns false

**Impact**: On Linux dual-stack binds (common), the test might pass (if the listener is pinned to `127.0.0.1` the incoming addr is `127.0.0.1`), but some kernel configs / socket options promote v4 connects to v4-mapped-v6. The spec says "dashboard gRPC is localhost-only" — the bind is `127.0.0.1` per `grpc/mod.rs:348` — so in practice v4-mapped-v6 won't show up. But if §4.3's `honor_opt_out` is ever reused by a future v2c that dual-binds, it silently breaks.

**Recommended fix**: Either:
1. Add to `honor_opt_out`: accept canonical loopback plus v4-mapped-v6 loopback:
   ```rust
   fn is_loopback_canonical(addr: IpAddr) -> bool {
       addr.is_loopback() || matches!(addr, IpAddr::V6(v6)
           if v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()))
   }
   ```
2. Document scope: "loopback check accepts `127.0.0.1` and `::1` only. Dual-stack + v4-mapped-v6 is out of scope for v2b (bind is v4 loopback)."

Spec currently has neither.

---

## Minor findings

### M1. `SubscribeMetricsResponse.payload` is `Option<Payload>` in generated Rust but spec §3 implies non-optional

**Problem**: Generated type is `pub payload: ::core::option::Option<subscribe_metrics_response::Payload>` (proto oneof → Option in prost). Spec's pseudocode treats it as if it's always `Some(...)` when yielded. Good implementors know proto oneof → Option in prost, but the spec never flags this.

**Evidence**: `crates/oneshim-web/src/proto/generated/oneshim.dashboard.v1.rs:189-190`

**Impact**: Minor implementation-level footgun. Implementors constructing responses must wrap in `Some(Payload::Data(...))`.

**Recommended fix**: Add a line to §3 or §4.6: "Generated `payload` field is `Option<Payload>`; always yield `Some(...)`."

---

### M2. §4.4 `GrpcSpawnConfig` lists 8 fields but upstream design §3 shipped with 6

**Problem**: Upstream design §3 enumerates: `port, storage, system_monitor, event_tx, integration_auth_token, load_policy` — 6 fields. Spec §4.4 adds `streaming_enabled` (kill switch) and `pii_sanitizer` (PR-B3 forward-compat). Both are defensible additions but the spec doesn't cross-reference the delta.

**Evidence**:
- Spec §4.4: `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md:216-225`
- Upstream design §3 `GrpcSpawnConfig` struct

**Impact**: Reviewer cross-checking against upstream sees "drift" without explanation.

**Recommended fix**: Add a one-line note at §4.4: "Delta from upstream design: added `streaming_enabled` (kill switch per §7 rollback) and `pii_sanitizer` (PR-B3 forward-compat field; unused in PR-B2)."

---

### M3. §3 `suggested_event_rate_limit` comment is misleading ("set for parity" vs proto "0 = no suggestion")

**Problem**: Line 93: `uint32 suggested_event_rate_limit = 5;   // unused by SubscribeMetrics, set for parity`. The proto comment (dashboard.proto:223) says "0 = no suggestion. Relevant for SubscribeEvents (events/sec cap)." So it's not "unused by SubscribeMetrics set for parity" — it's a cross-RPC shared-hint design where SubscribeMetrics emits 0 (no suggestion). The "set for parity" phrasing suggests you'd populate it with something.

**Evidence**:
- Spec line 93
- Proto `dashboard.proto:222-223`
- Upstream design §3 hint field population: "`suggested_event_rate_limit` ← per-level Frame cap from the same ladder — 0 when level is LOW (no suggestion)"

**Impact**: Confusing prose; tied into C2.

**Recommended fix**: Rewrite the inline comment to: `// SubscribeMetrics always emits 0 here ("no suggestion" per proto); SubscribeEvents populates in PR-B3.`

---

### M4. "hint is first yield" invariant is claimed twice but implicitly satisfied, not explicit in pseudocode

**Problem**: §2 goal 1 (line 40): "First yield is always a `ServerLoadHint`." §4.6 semantic choice #1 (line 339) reaffirms. But the generator pseudocode (line 294-334) has no pre-loop hint emission — it relies on `hint_emitter.maybe_emit()` inside the loop to emit `Some(...)` on the first call because `last_level = None`. Step 2 of the loop is "Maybe emit hint". Step 4 is "Wait for next tick". So yes, first iteration emits a hint before the first wait → the first payload on the stream IS a hint. Correct semantics, but the prose-vs-pseudocode mismatch is jarring.

**Evidence**:
- Spec §2 line 40, §4.6 line 339 — claim
- Spec §4.6 line 293-334 — implementation

**Impact**: Reviewer confusion.

**Recommended fix**: Add a comment in the generator pseudocode after step 2: `// First iter: last_level=None → guaranteed Some(...); satisfies §2 goal 1.`

---

### M5. §10 stale-assumption audit omits a row for `SubscribeMetricsRequest.interval_secs` clamp verification

**Problem**: §10 verifies trait surfaces and paths but doesn't verify that `SubscribeMetricsRequest` has no server-side max beyond the spec's 60s ceiling. The proto's field is `uint32` (max 4,294,967,295). §3 clamp row 1 says >60 → 60s, which handles it — but §10 could add a verification row.

**Evidence**: §10 table at line 492-505; no interval_secs row

**Impact**: Minor — the spec handles it in §3, just not cross-referenced in §10.

**Recommended fix**: Add one row: "`SubscribeMetricsRequest.interval_secs` wire max is `u32::MAX`; server clamps to 60s per §3 — verify clamp test exists."

---

### M6. §7 test counts are internally inconsistent (28 vs 29)

**Problem**: Line 393 says "28 new tests". Line 445 says "Revised total: 29 new tests". Acceptance criteria line 462 says 9 + 4 + 6 = 19 unit tests (internally consistent with the breakdown at lines 399-422 if you count the stream-cap and log tests separately).

**Evidence**: Spec lines 393, 445, 462

**Impact**: Trivial — reviewer questions the count.

**Recommended fix**: Pick one number consistently (29 seems right if we count the 2 safety tests as additions). Update line 393 to match.

---

## Verification checklist

| Claim in spec | Verdict |
|---|---|
| `MetricBucket` fields match generated proto | ✓ — all 5 fields present with matching types in generated Rust |
| `SubscribeMetricsRequest.interval_secs` is `uint32` | ✓ |
| `SubscribeMetricsRequest.respect_server_hints` is `bool` | ✓ |
| `SubscribeMetricsResponse` is a `oneof { data, hint }` | ✓ |
| `ServerLoadHint.load_level` enum values (0-4) | ✗ — spec uses bare `LOW`/`MEDIUM`/`HIGH`/`CRITICAL`, proto uses `LOAD_LEVEL_LOW` etc. (see C1) |
| `ServerLoadHint.cpu_pct` / `memory_pct` are `float` | ✓ — both f32 in generated |
| `ServerLoadHint.suggested_interval_secs` is `uint32` | ✓ |
| `ServerLoadHint.suggested_event_rate_limit` is `uint32` | ✓ — but comment is misleading (see M3) |
| `ServerLoadHint.reason` is `string` | ✓ |
| `ServerLoadHint.emitted_at` is `Timestamp` | ✓ |
| `to_proto_ts` helper visible via `pub(super)` | ✓ — `grpc/mod.rs:59-64` |
| `subscribe_metrics` stub returns `Status::unimplemented` | ✓ — `grpc/mod.rs:319-326` |
| "No proto changes in PR-B2" | ✓ — but see I2 re: PR-B1's already-landed wire break |
| v2a clients unaffected by `MetricBucket` promotion | ✓ — `grpc/mod.rs:252` still constructs from top-level `MetricBucket`; generated code shares the type |
| `SubscribeMetricsResponse.payload` is `Option<Payload>` in Rust | ✓ in generated; spec doesn't flag (see M1) |
| `Status::resource_exhausted` appropriate for 50-stream cap | ✓ conceptually — but integration point missing (see I3) |
| `Status::unimplemented` appropriate for runtime kill-switch | ✗ — should be `Unavailable` or `FailedPrecondition` (see I1) |
| `Status::internal` for `spawn_blocking` JoinError | ✓ — matches v2a pattern at upstream design §3 |
| `LoadPolicy::enforced_frame_rate` exists in §4.1 | ✗ — method not defined; spec §4.2 calls a ghost method (see C2) |
| `GrpcSpawnConfig` field count matches upstream | ✗ — spec adds 2 fields without cross-reference (see M2) |
| Hint as first stream yield | ✓ — generator semantics preserve this invariant (see M4) |
| IPv4 + IPv6 loopback both trusted | unclear — `is_loopback()` handles `::1` but not v4-mapped-v6 (see I7) |
| `interval_secs` clamping table is complete | ✗ — spec has a nonsensical row at line 104 (see I4) |
| Test count consistency (28 vs 29) | ✗ — minor (see M6) |
| Dependency audit in §10 covers all critical symbols | ✓ — 12 rows; most marked "verify" are legitimate plan-phase tasks |

**Summary counts**: Critical 2, Important 7, Minor 6.
