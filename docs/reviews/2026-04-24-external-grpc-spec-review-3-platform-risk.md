# Spec Review 3 — Platform & Risk Lens

**Reviewer role**: Platform & Risk lens (API correctness, concurrency, security, dependencies)
**Spec under review**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-spec.md` (commit e3f5ee68 at review time)
**Round**: Loop 1 / Round 1

---

## Critical

### C1: `TrailerCapturingBody` cannot observe "trailers-only" responses — and tonic emits those for every `Status::Err` from a handler

**Location**: spec §5.3 (`TrailerCapturingBody::poll_frame`), §8.2 table row "Body ends normally but no trailer"

**Issue**: tonic 0.14 has two distinct server response paths for non-streaming error cases:

1. Verified at `tonic-0.14.5/src/status.rs:605` — `Status::into_http<B>(self) -> http::Response<B>` constructs a response where `grpc-status` is injected into **`response.headers_mut()`** (initial HTTP/2 headers), and the body is `B::default()` (empty).
2. Verified at `tonic-0.14.5/src/server/grpc.rs:20` — `Err(status) => return status.into_http()` short-circuits every handler that returns `Err(Status::...)` *before* stream processing begins. The body will `poll_frame` → `Ready(None)` immediately, with **no trailer Frame** ever emitted.

This is the gRPC "trailers-only response" protocol case, and it's precisely how `Status::permission_denied("...")`, `Status::deadline_exceeded("...")`, `Status::unavailable("...")` in handler bodies are propagated. Spec §9.2 integration test `external_grpc_audit_denied_for_permission_denied` — "configure handler to return `Status::permission_denied`" — will **silently fail**: the handler path returns `Err(Status)` → tonic short-circuits to a trailers-only HTTP response → `TrailerCapturingBody::poll_frame` sees `Ready(None)` → `Drop` fires `tx.send(None)` → `map_code_to_audit_status(None)` → `AuditStatus::Completed`. The audit row records **Completed for a denied request**, which is the exact regression G2 is supposed to fix.

Spec §8.2 conflates this case with "body ended normally" and classifies both as `Completed` — but for an `Err(Status)` handler return, "Completed" is wrong and user-visible.

**Evidence**:
- `tonic-0.14.5/src/status.rs:605-613` — `into_http` puts grpc-status in headers, empty body
- `tonic-0.14.5/src/server/grpc.rs:20` — `Err(status) => return status.into_http()` short-circuit
- `tonic-0.14.5/src/client/grpc.rs:339` — `trailers_only_status = Status::from_header_map(response.headers())` — the client explicitly checks **response.headers** for grpc-status as a distinct trailers-only path

**Suggested fix**: The spec must add a second observation site in `RequestIdLayer` **or** the audit wrapper: after `inner.call(req).await?` returns, inspect `response.headers().get("grpc-status")` **before** wrapping the body. If present, extract the code, skip body wrapping (or wrap but pre-populate `captured`), and let the deferred task use that value.

Concretely, restructure §5.5 pseudocode:

```rust
let response = inner.call(req).await?;

// Trailers-only fast path — grpc-status is already in initial headers.
let header_status = response.headers()
    .get("grpc-status")
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.parse::<i32>().ok())
    .map(tonic::Code::from_i32);

let (tx, rx) = oneshot::channel::<Option<tonic::Code>>();
if let Some(code) = header_status {
    // Fire immediately; body poll won't see trailers for trailers-only responses.
    let _ = tx.send(Some(code));
    // Still wrap so msg_counter semantics are preserved (trivially 0 here).
    let wrapped = TrailerCapturingBody::new_already_fired(body);
    ...
} else {
    let wrapped = TrailerCapturingBody::new(body, tx);
    ...
}
```

This must also be documented in §8.2 as a first-class case, and §9.2 must add a specific test `external_grpc_audit_denied_observes_header_status` that sends a handler returning `Status::permission_denied` and asserts the audit row status is `Denied`, not `Completed`.

---

### C2: Spec mis-scopes the config field — `grpc_streaming_enabled` lives on `AppConfig.web`, but the field is shared with the loopback streaming server

**Location**: spec §7.1, NG1 ("Loopback server unchanged")

**Issue**: Verified at `crates/oneshim-core/src/config/sections/network.rs:106-109`: `WebConfig::grpc_streaming_enabled` is a **single boolean** shared between the loopback gRPC server (port 10091) and the external gRPC server. There is no `external.streaming_enabled` distinct from a loopback one. When the spec's `ConfigReloadTask` observes `cfg.web.grpc_streaming_enabled = false` and swaps the `LiveExternalConfig.streaming_enabled` atomic, it affects **only the external server** (because loopback has no `LiveExternalConfig`), but the user who toggles the field to disable streaming in incident response will silently also be altering behavior that loopback handlers are *supposed* to check (but don't, post-reload, because they still read `cfg.web.grpc_streaming_enabled` at service-build time).

The upshot: during an incident, an operator toggles `grpc_streaming_enabled: false` → external server honors it, loopback server does not (unchanged per NG1) → inconsistent behavior that is hard to reason about and debug.

Spec §7.1 claims "No new config fields. No schema migration. Users who never edited these fields see no behavior change; users who edit them see changes take effect on next `ConfigManager` reload." The second half is wrong for loopback users.

**Evidence**: `crates/oneshim-core/src/config/sections/network.rs:108-109` — field is under `WebConfig`, no separate `ExternalGrpcConfig.streaming_enabled` in the codebase.

**Suggested fix**: Either:
- (a) Add a new `external_grpc.streaming_enabled: Option<bool>` field that falls back to `web.grpc_streaming_enabled` when `None`, and wire the live reload to the new field only. Document in §7.1.
- (b) Accept the dual-subsystem impact and update NG1 + §7.1 + §10 explicitly: state that `grpc_streaming_enabled` is shared between loopback and external; live reload affects external immediately, loopback remains at its boot-time value until restart. Add an integration test `loopback_streaming_enabled_is_not_live_reloaded` that pins this behavior.

Option (a) is the safer operator story. Option (b) is cheaper but must be documented loudly.

---

### C3: `LoadPolicy::new` **panics** on invalid thresholds — `ConfigReloadTask` will crash the tokio task on a malformed live config

**Location**: spec §5.4 `apply_config`, §8.3 ("LoadPolicy::new fails (if fallible)")

**Issue**: Verified at `crates/oneshim-web/src/grpc/load_policy.rs:41-63`: `LoadPolicy::new` has three `assert!` calls. If a user edits config.json and writes e.g. `{"grpc_load_thresholds": {"cpu_low_pct": 80.0, "cpu_medium_pct": 70.0}}` (or any combination violating `low < medium < high <= 100`), `LoadPolicy::new` will **panic, not return Err**. In the reload task's `apply_config`, the panic propagates out of the `tokio::select!` loop, aborting the spawned task (since it's not `catch_unwind`-wrapped). The existing behavior continues to serve requests (the previous `Arc<LoadPolicy>` stays live), but subsequent config edits are silently ignored — the task is dead.

Spec §8.3 already calls this "OQ1" (open question), and the table row "`LoadPolicy::new` fails (if fallible) → log error, skip this cycle, continue" is written as if it were the contract. It is not — the actual API panics.

**Evidence**:
- `crates/oneshim-web/src/grpc/load_policy.rs:41` — `/// Panics if thresholds are not strictly increasing...`
- Same file L42-58: three `assert!()` calls

**Suggested fix**: Either:
- (a) Add a `LoadPolicy::try_new(thresholds) -> Result<Self, LoadPolicyError>` constructor that returns `Err` for invalid threshold ordering, and have `apply_config` use it with `tracing::error!` + early-return on Err.
- (b) Add explicit validation in `apply_config` before calling `LoadPolicy::new`, matching the same invariants.
- (c) Wrap the panicking body in `std::panic::catch_unwind` (ugly, not recommended).

Option (a) is cleanest and makes the spec §8.3 claim actually true. Spec must mention the new `try_new` API and test case `config_reload_rejects_malformed_thresholds_and_continues`.

Also, silent acceptance of `grpc_streaming_enabled = false` without verifying load_policy construction would set the external server into a hybrid state (streaming disabled but load_policy unchanged) — spec should clarify whether partial apply is acceptable or whether the reload is atomic.

---

## Important

### I1: `ConfigReloadTask` performs two independent atomic stores — a request reading between them sees a partially-applied config

**Location**: spec §5.4 `apply_config`, §3 NG7 ("readers don't observe partial updates")

**Issue**: `apply_config` does:
```rust
live.set_streaming_enabled(new_streaming);   // store 1 — AtomicBool Relaxed
live.set_load_policy(new_policy);            // store 2 — ArcSwap
```

A request running on another thread can observe the new `streaming_enabled` and the **old** `load_policy` (or vice versa) between these two statements. Spec NG7 claims "readers don't observe partial updates" — that claim is false under the current design. Whether it matters depends on whether a request-path code reads both fields in one decision. Grep of the codebase at `grpc/external/spawn_config.rs` currently holds `streaming_enabled: bool` and `load_policy: Arc<LoadPolicy>` as independent fields, and `DashboardServiceImpl` reads both for `subscribe_metrics` (classify throttle interval based on `load_policy` *only if* `streaming_enabled = true`). So the practical impact today: for ~nanoseconds, a newly-authorized stream could use old thresholds with new `streaming_enabled`, or vice versa. Mostly benign but should be acknowledged.

**Suggested fix**: Either:
- Replace the two stores with a single `ArcSwap<LiveSnapshot>` where `LiveSnapshot { streaming_enabled: bool, load_policy: Arc<LoadPolicy> }`, giving readers a single-load atomic snapshot. This is cleaner and matches the cert_resolver precedent more literally.
- Or: weaken NG7 to "readers never observe torn fields (AtomicBool/ArcSwap guarantee per-field atomicity) but may observe cross-field inconsistency for a few nanoseconds during reload" and justify why that's acceptable.

### I2: `ExternalGrpcSpawnConfig::config_rx: watch::Receiver<Arc<AppConfig>>` — verify tonic layer cloning semantics

**Location**: spec §5.6 (struct field), OQ7

**Issue**: Spec OQ7 acknowledges but doesn't resolve: `ExternalGrpcSpawnConfig` will be moved into `serve_external`, which consumes it. The spec adds `config_rx` for the reload task spawn in `serve_external` itself — the receiver is *consumed there*, not inside tonic's layer clones. This is fine for the `watch::Receiver` itself (cheap Clone, non-blocking). **However**, the spec also says `cfg.live.clone()` is passed into the reload task — confirm that `cfg.live: Arc<LiveExternalConfig>` is also cloned for use inside handlers (`DashboardServiceImpl::from_external_spawn_config`). Since `tonic` Server consumes the layer chain and clones services per-request, `Arc<LiveExternalConfig>` clones are free — this is fine.

But §5.6 does not address one real issue: the `Debug` derive on `ExternalGrpcSpawnConfig` currently formats all fields. `watch::Receiver` has no public `Debug` impl that doesn't panic under certain watch states (it does `Debug` safely, but verify). Spec §5.6 says "elide" — make this explicit with `#[derive(Debug)]` dropped or a manual impl, per ADR-001 codebase convention.

**Suggested fix**: Spec §5.6 must state that the `Debug` impl for `ExternalGrpcSpawnConfig` transitions from derive to manual, listing only stable-Debug fields, and explicitly `.field("config_rx", &"<watch>")` stub.

### I3: Deferred `tokio::spawn` per request — unbounded task creation; consider SystemTaskHandle tracking

**Location**: spec §5.5 (deferred audit task spawn)

**Issue**: Every request spawns an unawaited `tokio::spawn(async move { ... })` for the deferred audit. These tasks hold `Arc<AuditBridge>`, `Arc<ExternalMetrics>`, `AuthContext` (with `command_id: String` + `client_id: String`), `operation: String`, `remote: String`, `Arc<AtomicU64>` — roughly 200-400 bytes of captures per task plus the oneshot + channel state. The existing `max_concurrent_streams = 50` bounds streaming-RPC concurrency but not unary-RPC. At the theoretical max of `max_connections * max_concurrent_streams`, plus any post-inner.call backlog, there's no explicit cap.

Spec §8.6 notes an optional `external_grpc_config_reload_total` metric but does not propose any metric on deferred-task count or backlog. Under high RPS + a slow audit backend (`record_completion` awaits DB I/O), the task queue can grow.

**Suggested fix**: Either:
- (a) Add an `Arc<AtomicUsize> deferred_audit_in_flight` counter into `ExternalMetrics`, increment on spawn, decrement at task end. Add a `deferred_audit_gauge` metric; spec §8.6 should promote this to non-optional.
- (b) Add a bounded `mpsc` channel: spawn a single audit worker per server that consumes completion events from a bounded channel (e.g., capacity 1024), and the per-request completion sends to the channel with `try_send`. If full → drop + log a warn + bump drop_counter. This is the more defensive design for a security-relevant code path.

At minimum, §5.5 must acknowledge the unbounded spawn and justify why `max_concurrent_streams * max_connections` is an acceptable implicit ceiling.

### I4: `ConfigReloadTask` will silently miss rapid toggles due to `watch` coalescing — spec should document

**Location**: spec §5.4, §3 G3 ("≤5 seconds")

**Issue**: `tokio::sync::watch::Receiver` is latest-wins (confirmed in `config_manager.rs:107-111` doc comment and upstream tokio docs). If a user saves config.json twice in rapid succession (e.g., via a rollout script), the reload task wakes up once and sees only the final value. Intermediate transitions are coalesced. For `streaming_enabled` toggles (boolean), this is fine. For `LoadThresholds`, if an operator first pushes buggy thresholds and quickly rolls back, the reload task might never see the buggy value — which is actually desirable.

However, the spec §3 G3 promises "≤5 seconds" convergence. With `ConfigManager::reload()` which is synchronous + fires `send_replace`, the actual observability is microseconds. The 5s number is hand-wavy and inconsistent with the synchronous design. Either tighten it (≤100ms is defensible) or remove the SLO.

**Suggested fix**: Spec §3 G3 rewrite: "After `ConfigManager` emits a change event, `LiveExternalConfig` reflects the new value within one tokio scheduler tick, typically <10ms. Coalescing of rapid successive updates is acceptable (watch semantics — latest-wins)." Add a test `config_reload_coalesces_rapid_successive_updates` that fires 100 `update_with` calls and asserts the final `live.streaming_enabled()` matches the last value, and the reload task did not panic.

### I5: `RequestIdLayer::is_valid` — byte-range check does not exclude horizontal tab, but spec claims it does

**Location**: spec §5.2 validation rules + `is_valid` implementation

**Issue**: Spec §5.2 reads: "All bytes in `0x21..=0x7E` (ASCII graphic — excludes space `0x20`, tab, control chars, high bytes)". This is correct — tab (0x09) < 0x21, so it's excluded. But the narrative "Whitespace-padded input (any `0x20`, `\t`, `\n`, ...) fails the `0x21..=0x7E` check and triggers UUID generation" is correct only for the exact bytes listed. However, `\r` (0x0D) and `\n` (0x0A) are also < 0x21 and excluded — this is good for audit log injection prevention. Verify `HeaderValue::from_str(&request_id)` at §5.2 cannot fail for any string that passes `is_valid`:

Checked `http` crate: `HeaderValue::from_str` requires the input to be "visible ASCII characters (32-127)" plus tab (9). Since `is_valid` gates on `0x21..=0x7E` (33-126), a validated value is a strict subset of what `HeaderValue::from_str` accepts. So §8.1 row "Response `HeaderValue::from_str` fails (never, by construction)" is correct. Good.

However, there's a subtle issue: UUIDs generated via `uuid::Uuid::new_v4().to_string()` produce lowercase hex + hyphens (`'-'` = 0x2D ∈ `0x21..=0x7E`, `'a'-'f'` ∈ `0x21..=0x7E`, `'0'-'9'` ∈ `0x21..=0x7E`). No issue. But spec §5.2 should explicitly state "UUIDv4 output (36 chars `[0-9a-f-]`) satisfies the validation predicate" so a reader is not left wondering whether a generated ID would be rejected by a downstream validator using the same rules.

**Suggested fix**: Add a sentence to §5.2: "UUIDv4 output (lowercase hex + hyphens, 36 chars) satisfies the `is_valid` predicate by construction; no special-casing needed."

---

## Minor

### M1: `pin_project_lite` is already ubiquitous in the workspace — D19 should be promoted from conditional to decided

**Location**: spec D19, OQ2

**Issue**: `cargo tree -p oneshim-web --features grpc-dashboard-external` shows `pin-project-lite v0.2.17` transitively from tokio, tower, http-body-util, hyper, etc. It is already part of the compiled artifact. D19 says "prefer pin_project_lite if already adopted; verify during implementation." The verification is already done: it's there. Spec should state affirmatively "use `pin_project_lite`" as D19 without the conditional, and OQ2 can be closed.

**Suggested fix**: Close OQ2. Update D19: "`pin_project_lite` is a transitive workspace dependency (tokio/tower/hyper pin it) — use directly; do not add `pin_project` proc-macro dep."

### M2: `tonic::Code::from_i32` signature — spec is correct, verified

**Location**: spec §5.3 `parse_grpc_status`

**Issue**: Spec uses `tonic::Code::from_i32(i)`. Verified at `tonic-0.14.5/src/status.rs:823`: `pub const fn from_i32(i: i32) -> Code` — **infallible** (returns `Code` directly, no `Result`). Unknown values map to `Code::Unknown` per the match table. The spec code is correct.

**Note**: `impl From<i32> for Code` is also present at L907-909, so `tonic::Code::from(i)` works equivalently. Spec can choose either; current `Code::from_i32` is fine.

No change needed. Listed for reviewer confidence.

### M3: Test `external_grpc_audit_timeout_for_cancelled_stream` is non-trivial; OQ6 should be resolved in plan phase

**Location**: spec §9.2, OQ6

**Issue**: Simulating client-side cancellation in integration tests requires dropping the tonic client stream handle mid-stream. This is doable (spec gives option A) but the details matter for audit determinism:
- If the client drops, the server sees RST_STREAM; the handler's `abort_handle`/stream `drop` may or may not emit a grpc-status trailer depending on how the handler was coded.
- Option B (server-side `Status::cancelled`) is the trailers-only case — see C1.

**Suggested fix**: Plan phase must specify Option A with a concrete mechanism (e.g., `drop(response_stream)` after N messages on the client side, then assert audit row). A pending open question should not survive into implementation.

### M4: Spec §2.4 reference says "`tests/external_grpc_integration.rs:929`" — worktree drift check

**Location**: spec §2.4

**Issue**: Line numbers in spec quotes can drift. Per memory `feedback_cross_worktree_line_drift.md`, pre-impl re-verify is required. Not a correctness issue for the spec itself, just reminds the plan phase to re-check.

**Suggested fix**: In plan phase, add a one-liner verification script run: `grep -n "post-Task-13 follow-up" crates/oneshim-web/tests/external_grpc_integration.rs` and confirm citation.

---

## Strengths

- **S1**: Spec correctly identifies the tonic 0.14 layer FIFO-on-ingress convention (§4.1) — cross-referenced with memory `reference_tonic_layer_order.md`. The layer ordering is right.
- **S2**: `ArcSwap` + `AtomicBool` design mirrors the `HotReloadCertResolver` precedent exactly (§5.1 lineage acknowledged). Lock-free hot path is correct.
- **S3**: UUID crate already workspace-level (verified at `Cargo.toml:114`), no dependency churn. `oneshim-web/Cargo.toml:35` already includes `uuid`.
- **S4**: Constants `arc-swap v1` + `http-body v1.0.1` are workspace-transitive, no new version lockfile pressure (verified via `cargo tree`).
- **S5**: Spec proactively closes several sub-decisions via D1-D20, reducing plan-phase debate surface.
- **S6**: Phase-9 coexistence matrix (§10.2) is thoughtful and file-level specific. Unlikely to cause merge pain either way.
- **S7**: `command_id` column reuse (D16) avoids audit schema migration — correct choice.
- **S8**: Validation rule `0x21..=0x7E` is a safe superset for UUIDs + ULIDs + snowflakes + common correlation IDs. Correctly rejects log-injection bytes (`\r`, `\n`, `\0`).

---

## Questions raised

- **Q1**: Should `AuditStatus::Failed` be sub-classified to distinguish "handler returned `Err(Status)`" (normal error) from "handler panicked" (unexpected crash) in the audit row? The spec maps both to `Failed`, but the latter is a reliability signal deserving its own wire label. Could be deferred to a follow-up metric, but call it out.
- **Q2**: `AuditBridge::record_completion` current signature takes `failure_reason: Option<&str>` (verified at `audit_bridge.rs:120`). Spec §5.5 pseudocode does **not** pass `failure_reason`, only `command_id`. The spec lists args as `(&ctx, remote, &operation, status, duration, msg_count_opt, request_id)` — 7 args — matching the current signature's first 7 slots, but swaps the last two. Verify: does spec intend `record_completion(..., msg_count_opt, failure_reason=None, command_id=request_id)` (8 args total), or does `record_completion` need a signature change to accept `command_id`? If the latter, spec §5.5 must explicitly call out the signature change to `record_completion` and update §4.2 "Component map" to add `+2/-0` on `audit_bridge.rs`.
- **Q3**: What is the observability contract when `ConfigReloadTask` exits due to sender drop (§8.3)? If the ConfigManager is dropped at shutdown, this is normal. If it's dropped unexpectedly (bug), the external server continues with stale config forever. Should this trigger a metric or a health check signal? Propose `external_grpc_config_reload_task_alive` gauge.
- **Q4**: `max_concurrent_streams` in `WebConfig::grpc_max_concurrent_streams` is **also** not live-reloaded per §7.2 (restart-required). This is fine but noteworthy: an operator tuning streaming behavior live has `streaming_enabled` (kill switch) + `LoadPolicy` thresholds, but cannot raise the concurrent-stream cap without a restart. Is this the right set of live-reloadable knobs? Worth a one-sentence justification in §7.1.

---

*End of review. Expected deliverables for plan phase: address C1-C3 (Critical); adopt I1-I5 as plan-level subtasks; resolve Q1-Q4 with ADR-flavored one-liners.*
