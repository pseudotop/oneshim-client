# Spec Review 1 — Architecture Lens

**Reviewer role**: Architecture lens (Hexagonal boundaries, tower layer composition, concurrency correctness, API design)
**Spec under review**: `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-spec.md` (base commit `5618558c`)
**Round**: Loop 1 / Round 1

---

## Critical (C1, C2, ...)

### C1: `DashboardServiceImpl` must be modified — spec only says "`mod.rs` reads `cfg.live` instead of raw fields" but hides that the struct itself holds by-value fields

**Location**: spec §4.2 (Component map row "`grpc/mod.rs` +15/-10"), §5.6; real code `crates/oneshim-web/src/grpc/mod.rs:92-163,419-454`.

**Issue**: `DashboardServiceImpl` stores `load_policy: Arc<LoadPolicy>` (L99) and `streaming_enabled: bool` (L100) as owned fields, then **clones/copies them into the handler call site every request** (L429-430, L446-447). The spec treats this as a one-line change ("reads `cfg.live`"), but the real fix is structural: either (a) swap the two fields for `live: Arc<LiveExternalConfig>` and call `live.streaming_enabled()` / `live.load_policy()` at the call site — which breaks the loopback `from_spawn_config` path which has NO live config — or (b) keep separate field shapes for loopback vs external. Neither option is in the spec. Loopback `GrpcSpawnConfig` is unchanged (NG1), so this struct now needs to handle two modes. This is a real design decision, not a "modify" note.

**Evidence**: spec §4.2 row 7: *"`grpc/mod.rs` +15/-10 — `DashboardServiceImpl::from_external_spawn_config` reads `cfg.live` instead of raw fields"* — understates the delta. Actual cross-module coupling at `subscribe_metrics::subscribe_metrics(…, streaming_enabled: bool, …)` (crates/oneshim-web/src/grpc/subscribe_metrics.rs:61,83) means the `bool` is read *per-request* at the handler entry, not at build time as §4.1 claims (*"BEFORE (post-PR #486)… streaming_enabled captured at service-build time"* is misleading — it's captured at call-site start, which is per-RPC).

**Suggested fix**:
- Add an explicit §5.6.x subsection: *"`DashboardServiceImpl` field layout split"*. Two concrete options:
  - Option A: Replace `streaming_enabled: bool` + `load_policy: Arc<LoadPolicy>` with `streaming_source: StreamingSource` enum with `Fixed(bool, Arc<LoadPolicy>)` (loopback) and `Live(Arc<LiveExternalConfig>)` (external). Handlers call `self.streaming_source.streaming_enabled()` / `.load_policy()`.
  - Option B: Keep loopback unchanged; external path constructs a separate sibling struct `ExternalDashboardServiceImpl` that wraps `Arc<LiveExternalConfig>` and delegates all non-streaming handlers to an inner struct. Bigger churn but cleaner.
- Pick one in the spec. Document that the loopback `from_spawn_config` path reads from a `Fixed` variant and that the spawn_config still carries `streaming_enabled: bool` / `load_policy: Arc<LoadPolicy>` there — they only move under `external::` for live reload.

---

### C2: `LoadPolicy::new` panics on invalid thresholds — `ConfigReloadTask` will tear down the server

**Location**: spec §5.4 (`apply_config`), §8.3 row "`LoadPolicy::new` fails (if fallible)"; `crates/oneshim-web/src/grpc/load_policy.rs:41-63`.

**Issue**: `LoadPolicy::new` is **not fallible** — it **panics** via three `assert!`s if `cpu_low_pct < cpu_medium_pct < cpu_high_pct <= 100.0` does not hold. The spec OQ1 and §8.3 hedge on "*if fallible, wrap in `try_new`*", but the code is already panicking. A user-edited `config.json` with e.g. `cpu_low_pct: 99.0, cpu_medium_pct: 50.0` will cause the reload task to panic inside the spawned tokio task, which then (per `tokio::task::JoinHandle` semantics) silently aborts. No further reloads happen. The supervisor doesn't know (the panic is in a detached `tokio::spawn`, not in `serve_external`).

**Evidence**: load_policy.rs:42 `assert!(thresholds.cpu_low_pct < thresholds.cpu_medium_pct, …)`. Spec §5.4 calls `LoadPolicy::new(new_thresholds)` directly with no try-pattern. OQ1 treats this as "verify during implementation" but it's verifiable now.

**Suggested fix**:
- Introduce `LoadPolicy::try_new(LoadThresholds) -> Result<Self, LoadPolicyError>` (a minor refactor, kept backward-compat if `new` becomes `try_new(…).expect("validated at build time")`).
- In `apply_config`, on `Err` log `error!(err = %e, "external_grpc: invalid LoadThresholds in reloaded config; keeping previous policy")` and **skip** `set_load_policy` while still applying `streaming_enabled` (independent field). Document this partial-apply in D7 / D11.
- Alternative if `try_new` is out of scope: **validate in `ConfigManager::update_with`** at the config layer so invalid values never reach here. This is cleaner but touches `oneshim-core`.

---

### C3: Per-field "Relaxed" ordering argument is correct for `LiveExternalConfig` in isolation, but `apply_config` writes two fields non-atomically — a reader can observe `streaming=new, policy=old`

**Location**: spec §5.1 ("*readers don't need cross-field happens-before*"), §5.4 `apply_config`.

**Issue**: The spec explicitly claims *"NG7 — Per-field live reload granularity. The whole `LiveExternalConfig` atomic updates on any relevant change — readers don't observe partial updates."* This is **false**. `apply_config` calls `set_streaming_enabled` and `set_load_policy` as two independent stores. A subscribe_metrics RPC arriving between the two sees new streaming and old thresholds (or vice versa). This may be acceptable — but the spec asserts the opposite.

Separately: is there any cross-field invariant? In the current code base, no — `streaming_enabled` is checked in the handler top-level gate (subscribe_metrics.rs:83); `LoadPolicy` is consulted later (classify + hint emission). A transient torn read affects one request duration at most. Low severity but the spec promises otherwise.

**Evidence**: spec §3.2 NG7; §5.1 docstring. §5.4 `apply_config` performs 2 stores with no ordering.

**Suggested fix**:
- Weaken NG7 to: *"Per-field writes are independent; a reader may briefly observe new `streaming_enabled` with old `load_policy` during a reload transition. This is acceptable because the two fields have no cross-invariant."*
- If true atomicity is wanted, wrap both in a single `ArcSwap<ExternalConfigSnapshot { streaming: bool, policy: Arc<LoadPolicy> }>`. Simpler than two atomics; single store. Recommend this.

---

## Important (I1, I2, ...)

### I1: `TrailerCapturingBody` introduces new crate dependencies not listed in the dependency graph

**Location**: spec §4.3 (dependency graph), §5.3.

**Issue**: §5.3 uses `http_body::{Body, Frame}`, `pin_project_lite::pin_project`, and `tokio::sync::oneshot`. Neither `http-body` nor `pin-project-lite` is in the workspace root `Cargo.toml` (verified by `grep pin[_-]project|http[_-]body Cargo.toml crates/oneshim-web/Cargo.toml` — no hits). These would be **new direct deps** on `oneshim-web`. The spec §4.3 dependency graph says "*depends on workspace: http, http-body, tokio (oneshot)*" but doesn't note this is a new dep — it's listed as if already workspace-resolved.

**Suggested fix**: add a subsection under §4.4 ("`uuid` dependency") titled "`http-body` + `pin-project-lite` dependencies" stating: *"`oneshim-web/Cargo.toml` gains two direct dependencies — `http-body = "1"` (provided transitively via `tonic`/`hyper`, but add explicit version to freeze) and `pin-project-lite = "0.2"` (proc-macro-free pinned projection). Lockfile diff expected but no new transitive bloat."* Confirm `http-body 1.x` is the version tonic 0.14 re-exports.

---

### I2: `ConfigReloadTask` is spawned via bare `tokio::spawn` — not tracked by supervisor

**Location**: spec §5.4 spawn-site snippet; §12 D11; compared to `crates/oneshim-web/src/grpc/external/mod.rs:142-149,222-265` (supervisor).

**Issue**: The spec claims (§5.4) *"The `JoinHandle` is tracked alongside the cert watcher + expiry monitor handles (existing pattern); supervisor awaits it on shutdown."* But the existing supervisor code **does not** track `spawn_cert_watcher` / `spawn_expiry_monitor` — they are fire-and-forget via `tokio::spawn` inside `build_external_spawn_config` at `app_runtime_launch.rs:1259-1266`, and rely on `shutdown_rx.changed()` to exit. The supervisor's "known JoinHandle set" doesn't exist — the only handle is `spawn_with_supervisor`'s own top-level one. The spec's claim is factually wrong.

This is not itself a bug (the design is "shutdown via watch channel"), but the spec's description misleads plan/impl into building a tracking mechanism that doesn't exist. Worse, if the reload task is spawned inside `serve_external` (per §5.4 snippet), it will be respawned on every supervisor restart — could leak if the previous one didn't observe shutdown yet.

**Suggested fix**:
- Amend §5.4 to: *"`ConfigReloadTask` is spawned fire-and-forget in `serve_external` identically to `spawn_cert_watcher` / `spawn_expiry_monitor`. It exits when `shutdown_rx.changed()` resolves (the shutdown sender's Arc drop closes the watch channel)."*
- Address the restart-leak: **spawn `ConfigReloadTask` in `build_external_spawn_config` instead of `serve_external`** (matching cert watcher / expiry monitor placement). This way, a supervisor respawn of `serve_external` does not create a duplicate reload task. Add to §5.4.

---

### I3: Body-wrapping changes `AuditService::Response` type — the spec elides the full generic cascade

**Location**: spec §5.5 last paragraph ("Type parameter note"); `crates/oneshim-web/src/grpc/external/audit_layer.rs:55-72`.

**Issue**: Current `AuditService<S>` has `type Response = http::Response<RespBody>;` where `S: Service<http::Request<B>, Response = http::Response<RespBody>, …>`. After wrapping, `AuditService::Response` becomes `http::Response<TrailerCapturingBody<RespBody>>`. Tonic's `Server::builder().layer(L).add_service(svc)` accepts any layer that produces a service implementing `Service<http::Request<…>, Response = http::Response<impl http_body::Body>>`. This should work — but the spec doesn't prove it. Concrete proof requires `TrailerCapturingBody<B>` to impl `http_body::Body + Send + 'static` where `B: Body + Send + 'static` (same bounds as `tonic_prost::body::BoxBody` or similar), and the `Body::Data` / `Body::Error` types to pass the `MakeService` bounds check.

tonic 0.14 uses `axum::body::Body` internally; final response body is boxed. If tonic boxes before our wrapper, our trailer capture is layered outside the box — fine. If tonic boxes after, the wrap is correct at layer-service boundary.

**Suggested fix**: add a concrete prove-out subsection (§5.5.x) "Body trait compatibility" that:
1. Cites the `http_body::Body` super-trait bounds required by tonic 0.14's `Server::builder`.
2. States that `TrailerCapturingBody<B>`'s `Body::Data = B::Data`, `Body::Error = B::Error` preserves all consumer bounds (this is already in §5.3 code).
3. Commits to a compile-time-check test in `trailer_body.rs` tests: `fn assert_body<T: http_body::Body + Send + 'static>() {}; assert_body::<TrailerCapturingBody<tonic::body::Body>>();`.

---

### I4: Deferred audit task lifetime — detached `tokio::spawn` can lose the in-flight Completed record on supervisor-shutdown races

**Location**: spec §5.5 deferred task; §8.2 "receiver drops during deferred task" row.

**Issue**: `AuditService::call` returns the response synchronously while the deferred task awaits `rx.await`. The spec acknowledges (§8.2) that if the runtime is dropped mid-request, the deferred task is cancelled and the audit row is lost. Consistent with "existing behavior (panic catching already drops pending audit records)" — true, but the surface is larger now because **every successful unary RPC** is now deferred. Pre-this-spec, only streaming RPCs had a long tail; unary would have already recorded Completed synchronously.

This is a regression in audit durability for unary RPCs under shutdown pressure. Not a show-stopper, but worth calling out.

**Suggested fix**:
- For **unary** RPCs (no `CountingStream`, no long body), optimize the path: if `is_end_stream` + trailer both observable inline in `poll_frame` before body returns (normal case: tonic writes data frame + trailer back-to-back), the oneshot fires synchronously before the outer `Ok(response)` return. The deferred task's `rx.await` resolves immediately. Shutdown races affect streaming only. **Document this in §6.1 data flow.**
- Alternatively: **keep unary recordings synchronous**. Only streaming RPCs (detected by route `path()` via `SubscribeMetrics` / `SubscribeEvents`) use the deferred task. Simpler mental model, half the regression surface. Document in D12.

---

### I5: `RequestIdLayer` overwrites handler-supplied `x-request-id` — breaks gRPC servers that mirror trailer metadata

**Location**: spec §5.2 ingress logic ("*overwrite any pre-existing value from handler*"), D5.

**Issue**: gRPC convention is that trailer/header metadata set by `tonic::Status` or via `Response::metadata_mut()` are authoritative. tonic puts metadata in HTTP/2 trailers for gRPC; headers in HTTP/2 HEADERS frames pre-body. A handler that explicitly inserts `x-request-id` in response headers (not metadata) should be rare, but libraries that proxy another gRPC service may forward upstream correlation IDs. Overwriting is a behavior change that the spec treats as "single source of truth" but could surprise users.

**Suggested fix**: Add counter-insert, not overwrite: if `x-request-id` already exists AND matches the ingress-validated value, leave alone; otherwise insert. In most cases this is identical; the edge case (proxy forwarding) is preserved. Alternative: emit both via `append` (HTTP/2 allows multiple values with same header name). Record decision in D5 with rationale.

---

### I6: `ExternalGrpcSpawnConfig` now carries `watch::Receiver` + `Arc<LiveExternalConfig>` — Clone behavior test is stale

**Location**: `crates/oneshim-web/src/grpc/external/spawn_config.rs:242-250` (`spawn_config_clone_is_shallow` test); spec §5.6, §10.1.

**Issue**: The existing test asserts `Arc::ptr_eq` on three fields. After the spec, `live: Arc<LiveExternalConfig>` should be added to that assertion (shallow). `config_rx: watch::Receiver<Arc<AppConfig>>` does NOT satisfy `Arc::ptr_eq` — it's not an Arc wrapper; it's a struct with internal Arc. The spec §5.6 doesn't mention updating this test. Also, the `Debug` impl test (`spawn_config_debug_redacts_sensitive_fields`) needs updating for the new `streaming_enabled_live` field name (spec §5.6 rename).

**Suggested fix**: add explicit bullets in §5.6: (a) update `spawn_config_clone_is_shallow` test to include `Arc::ptr_eq(&cfg.live, &clone.live)`; (b) `config_rx` is intentionally not Arc-compared; (c) update `spawn_config_debug_redacts_sensitive_fields` for the renamed Debug field.

---

### I7: Module declaration in `grpc/external/mod.rs` is incomplete — spec names 4 new files but no `pub(crate) mod X;` lines are specified

**Location**: spec §4.2 (new files listed), §5.*; `crates/oneshim-web/src/grpc/external/mod.rs:4-16` (existing `pub mod X;` / `pub(crate) mod X;` block).

**Issue**: The spec adds 4 new files under `grpc/external/` but never tells plan/impl that each must be declared in `mod.rs`. Current pattern mixes `pub mod` (audit_bridge, auth_layer) and `pub(crate) mod` (audit_layer). D17 says *"New files use `pub(crate)` visibility by default"* — so the 4 new mods should all be `pub(crate) mod`. This is the kind of omission that slips into impl as a wrong-visibility copy of `pub mod audit_bridge`.

**Suggested fix**: add to §4.2 a "mod declarations" sub-list with the exact 4 lines:
```
pub(crate) mod config_reload;
pub(crate) mod live_config;
pub(crate) mod request_id_layer;
pub(crate) mod trailer_body;
```
Add to `mod.rs` after `pub(crate) mod audit_layer;` (L6).

---

## Minor (M1, M2, ...)

### M1: `AuthType` JSON label "jwt+mtls" uses `+` which is URL-unsafe

Spec §5.5 new audit rows flow through existing `AuditBridge` (audit_bridge.rs:66). Pre-existing. Out of scope but worth noting that downstream log parsers already cope.

### M2: Hard-coded header name `"x-request-id"` — consider constant

§5.2, §5.3 use the literal. Define `const REQUEST_ID_HEADER: &str = "x-request-id";` at module top. Referenced from §6.1 diagram, integration tests.

### M3: D12 + §5.5 pseudocode uses `metrics.request_bump(…, audit_status_label(status))` — current audit_layer.rs L143 uses hard-coded `"ok"`. The spec change implicitly converts the metric label from `"ok"` to four possible labels. This is a breaking Prometheus cardinality increase that dashboards may depend on. Call out in §8.6 as a metric schema change.

### M4: §5.4 `apply_config` clones `cfg.web.grpc_load_thresholds` (`.clone().unwrap_or_default()`). Since `LoadThresholds` is `Copy`? No — it has `Clone` (sections/network.rs:139) but no `Copy`. Small heap alloc per reload. Acceptable; just a note that `apply_config` allocates. No action.

### M5: §9.2 test `external_grpc_live_streaming_toggle_reflects_immediately` — convergence is `<10ms` per §6.4 but G3 target is `≤5s`. Test should use a generous 1s poll to avoid flakes; document the gap between convergence-in-practice and spec-guarantee.

### M6: §5.2 logs `raw.chars().take(32)` on invalid header — but the invalidation check was on *bytes*, not chars. `raw.chars()` on bytes containing `\x00` produces U+0000 which is fine; but if header contains non-UTF-8, `.to_str()` would have already returned None and we'd be in the None branch. Consistent, just note.

---

## Strengths

- **D11 (biased shutdown order)**: correctly prioritizes clean exit. Good defensive design.
- **§6.1-§6.4 data-flow diagrams**: explicit, traceable, and cover the three "interesting" paths (happy, timeout, invalid-ID) — rare in specs.
- **OQ4**: the explicit no-trim decision with the "cross-system correlation" rationale is exactly the right tradeoff, well articulated.
- **§10.2 Phase-9 coexistence matrix**: concrete file-by-file conflict analysis. Matches the memory `feedback_ci_workflow_assumption_verification.md` discipline.
- **§12 Decisions-Locked table**: D1-D20 are specific enough to enable a hard gate on plan deviations.
- **§4.4** uuid dep note: shows author verified `Cargo.toml` — unusual rigor.
- Deferred audit task **holds captured clones** (§5.5 "Crucial"): good borrow-discipline call-out, will prevent a class of impl bugs.
- `RequestId` type wrapping `String` (§5.2): gives strong static typing at extension-read sites; better than raw `String` in extensions.

---

## Questions raised

- **Q1**: Is `LoadPolicy`'s `started_at: Instant` intentional to reset warmup on each reload? §5.4 calls `LoadPolicy::new(…)` creating a **fresh** 30s warmup window on every config reload. This means every policy tweak causes 30s of `LoadLevel::Medium` regardless of actual load. Probably not desired — users tuning thresholds during an incident *want* immediate effect. Recommend: `LoadPolicy::with_started_at(thresholds, Instant)` so reloads preserve the original `started_at`. Raise with product.

- **Q2**: Does the spec intend `AuthLayer` to also start recording `request_id`? The layer stack is `auth → request_id → audit`, so AuthLayer runs *before* `RequestIdLayer`. AuthLayer still records `Failed` on rejection (audit_layer.rs:7-11 docstring), but has no `RequestId` in extensions yet. Should auth-rejected rows get a `command_id = None` (current) or should `RequestIdLayer` move outermost (breaks the "unauth requests never touch RequestIdLayer" claim in D14)? Needs explicit decision.

- **Q3**: `config_rx.borrow_and_update()` (OQ10) — the spec's concern is valid. `borrow_and_update` returns `Ref<'_, T>` (RAII-lifetime). Calling `LoadPolicy::new(…)` inside the `apply_config` helper while holding a borrow is fine because `new` doesn't touch the receiver. But holding the borrow across an `.await` would be a compile error — `Ref` is not `Send`. Current §5.4 code calls `apply_config(&live, &*config_rx.borrow_and_update())` which drops the borrow at `;`. Safe. Document: *"the `Ref` is dropped at the end of the `apply_config` call — no `.await` held across."*

- **Q4**: No mention of `request_id` propagation into tonic `Status` messages on handler errors. If a handler returns `Status::internal("oops")`, the caller sees `grpc-status: 13, grpc-message: "oops"` but no server request-id for the error. The response header x-request-id still flows (RequestIdLayer inject is after `inner.call`), so correlation works — confirm this path is tested (§9.2 seems to cover Denied but not Internal-error case).

---

*Word count ~2330. End of Review 1 (Architecture lens).*
