# External gRPC Audit Completeness + Live Config Reload — Spec

**Date**: 2026-04-24
**Author**: Bundled follow-up spec (PR #486 deferrals + D13 V2c live config TODO)
**Base commit**: `5618558c` (origin/main post-PR-#486)
**Branch**: `feature/external-grpc-audit-liveconfig`
**Status**: Draft — pending deep review (3-loop quality gate)

---

## 1. Overview

This spec bundles three deferred follow-ups to the external gRPC subsystem shipped in PR #486 (D13 Task 13) into a single coherent PR. The common theme: **close the audit trail accuracy gaps and make runtime-tunable config actually tunable at runtime**.

Three bundled items:

1. **`x-request-id` response header** — end-to-end correlation ID. Accept incoming header when valid, else generate UUIDv4 server-side. Inject into response. Pass to audit as `command_id`.
2. **Per-response gRPC status introspection** — observe `grpc-status` trailer (unary + streaming) via `http_body::Body` wrapper, map to `AuditStatus::{Completed, Denied, Timeout, Failed}`. Replaces the current hard-coded `Completed` in `AuditLayer`.
3. **Live config reload for `streaming_enabled` + `LoadPolicy` thresholds** — atomic swap via `AtomicBool` + `ArcSwap`, driven by `ConfigManager::subscribe()` watch.

Scope: **external gRPC server only** (`crates/oneshim-web/src/grpc/external/`). Loopback server unchanged. Non-covered fields (port, bind_addr, auth_mode, JWT key material) continue to require server restart per existing behavior.

---

## 2. Motivation

### 2.1 Why bundle these three?

- **Same subsystem, small diff surface**: all three modify files under `grpc/external/`. Reviewer context is shared.
- **Interdependence**: the `x-request-id` value flows from `RequestIdLayer` → `AuditLayer` as `command_id`, and the trailer observation lives in the same layered pipeline. Implementing separately would double the plumbing edits to `AuditLayer::call`.
- **Reviewed together**: a single 3-loop quality gate (spec → plan → impl) is cheaper than three separate gates for tightly coupled work.

### 2.2 Current gaps (concrete code references)

| # | Gap | Current state | File:Line |
|---|-----|---------------|-----------|
| #2 | No request correlation | `command_id` always `None` in `record_completion` calls | `grpc/external/audit_layer.rs:139` |
| #3 | `Completed` hardcoded for all auth-passed requests | `let status = AuditStatus::Completed;` — `PermissionDenied`/`DeadlineExceeded`/`Failed` indistinguishable | `grpc/external/audit_layer.rs:127` |
| #4 | Spawn-time captured config frozen | `streaming_enabled: bool` (copied once at boot), `load_policy: Arc<LoadPolicy>` (constructed once) | `grpc/external/spawn_config.rs:73-75` |

### 2.3 Operational value

- **#2**: On-call debugging — given an error report from a consumer, find the exact server-side audit row in <1s via request ID.
- **#3**: Distinguish "client was denied" (security signal) vs "handler crashed" (reliability signal) vs "deadline exceeded" (latency signal) in audit query tooling.
- **#4**: Toggle `streaming_enabled: false` during incidents without server restart. Tune `LoadPolicy` thresholds during load spikes without process churn.

### 2.4 PR #486 explicit deferral references

From PR #486 commit log:
- `audit_layer.rs:125-126` comment: *"Follow-up (spec §8): parse grpc-status trailer to distinguish Denied / Timeout / Failed."*
- `tests/external_grpc_integration.rs:929` test comment: *"Note: the planned `x-request-id` response header carrying the audit command_id is a post-Task-13 follow-up (spec §8)."*
- memory `project_next_tasks.md`: *"D13 V2c live config reload: streaming_enabled currently captured at spawn-time"*

---

## 3. Goals and Non-Goals

### 3.1 Goals

- **G1** — Every external gRPC request (auth-passed) emits a Started + Completed audit pair where `command_id` = a globally unique request ID, available to the caller via `x-request-id` response header.
- **G2** — The Completed audit row reflects the actual gRPC status of the response (Ok / PermissionDenied / Cancelled / DeadlineExceeded / Failed / ...), mapped deterministically to `AuditStatus::{Completed, Denied, Timeout, Failed}`.
- **G3** — `streaming_enabled` and `LoadPolicy` thresholds can be toggled/adjusted via config file edit + `ConfigManager` reload, reflected in subsequent request decisions within **≤5 seconds** without server restart.
- **G4** — Coverage: unit tests for each new module (target ≥90% line coverage), integration tests for each new behavior (x-request-id round-trip, status mapping for Denied/Timeout, live reload toggle effect), zero flakes.
- **G5** — Zero performance regression on the happy path: unary request median latency Δ < +200µs relative to PR #486 baseline.

### 3.2 Non-Goals

- **NG1** — Loopback server changes. Loopback has no `AuditLayer`, no auth, no user-facing runtime knobs worth live-reloading today.
- **NG2** — Live reload of fields requiring rebind or verifier rebuild (`port`, `bind_address`, `auth_mode`, JWT public key path, TLS cert path). These remain restart-required.
- **NG3** — Distributed tracing (OpenTelemetry, W3C TraceContext). `x-request-id` is correlation-only; full tracing is a separate future project.
- **NG4** — Request-ID enforcement or rate-limiting by ID. Informational header only.
- **NG5** — New audit action types or new `AuditStatus` variants. Reuse the existing 4-variant enum exposed by PR #486 Task 5 (`Started`, `Completed`, `Denied`, `Timeout`, `Failed`).
- **NG6** — Config schema migration. Reuses existing `AppConfig.web.grpc_streaming_enabled` + `AppConfig.web.grpc_load_thresholds` fields; no user-facing config change.
- **NG7** — Per-field live reload granularity. The whole `LiveExternalConfig` atomic updates on any relevant change — readers don't observe partial updates.

---

## 4. Architecture

### 4.1 Layer stack change

```
BEFORE (post-PR #486):                       AFTER (this spec):

   ┌──────────────┐                            ┌──────────────┐
   │ tonic Server │                            │ tonic Server │
   └──────┬───────┘                            └──────┬───────┘
          │                                           │
   ┌──────▼───────┐                            ┌──────▼───────┐
   │  AuthLayer   │ ← outermost                │  AuthLayer   │ ← outermost
   └──────┬───────┘                            └──────┬───────┘
          │                                           │
   ┌──────▼───────┐                            ┌──────▼────────┐
   │  AuditLayer  │ ← hardcoded Completed      │RequestIdLayer │ ← NEW
   └──────┬───────┘                            └──────┬────────┘
          │                                           │
          │                                    ┌──────▼───────┐
          │                                    │  AuditLayer  │ ← reads RequestId,
          │                                    └──────┬───────┘   wraps body,
          │                                           │           deferred record
          │                                           │
   ┌──────▼────────────────────────┐           ┌──────▼────────────────────────┐
   │DashboardServiceServer         │           │DashboardServiceServer         │
   │  (streaming_enabled captured  │           │  (reads cfg.live on each call)│
   │   at service-build time)      │           │                               │
   └───────────────────────────────┘           └───────────────────────────────┘

                                                Background task (spawned in serve_external):
                                                ┌───────────────────────────────┐
                                                │  ConfigReloadTask             │ ← NEW
                                                │   watch::Receiver<AppConfig>  │
                                                │   → LiveExternalConfig swap   │
                                                └───────────────────────────────┘
```

Layer application order in `serve_external`:

```rust
Server::builder()
    .layer(auth_layer)        // outermost — first on ingress (PR #486 fix)
    .layer(request_id_layer)  // NEW — runs after auth, before audit
    .layer(audit_layer)       // innermost — reads RequestId extension
    .add_service(...)
```

tonic 0.14 layer semantics: first `.layer()` call is outermost on ingress (per PR #486 iter with empirical fix, documented in `reference_tonic_layer_order.md` memory). This ordering ensures:
- AuthLayer rejects/authenticates first (unauthorized requests never touch RequestId/Audit)
- RequestIdLayer issues/validates ID for auth-passed requests
- AuditLayer records with the issued ID

### 4.2 Component map

| Kind | Path | LoC est. | Role |
|------|------|----------|------|
| 🆕 New | `grpc/external/live_config.rs` | ~80 impl + ~100 test | `LiveExternalConfig` — `AtomicBool` + `ArcSwap<LoadPolicy>` wrapper |
| 🆕 New | `grpc/external/request_id_layer.rs` | ~150 impl + ~150 test | Tower Layer — ingress validate/generate, egress header inject |
| 🆕 New | `grpc/external/trailer_body.rs` | ~150 impl + ~150 test | `http_body::Body` wrapper — trailer observation via `poll_frame` |
| 🆕 New | `grpc/external/config_reload.rs` | ~100 impl + ~100 test | tokio task — `watch` subscription → `LiveExternalConfig` swap |
| ✏️ Mod | `grpc/external/audit_layer.rs` | +60/-30 | Deferred completion via `oneshot::Receiver`, `RequestId` extraction, status mapping |
| ✏️ Mod | `grpc/external/spawn_config.rs` | +10/-3 | `streaming_enabled` + `load_policy` collapsed into `live: Arc<LiveExternalConfig>`; new `config_rx: watch::Receiver<Arc<AppConfig>>` |
| ✏️ Mod | `grpc/external/mod.rs` | +30/-5 | `serve_external` inserts `RequestIdLayer`, spawns `ConfigReloadTask` |
| ✏️ Mod | `grpc/mod.rs` | +15/-10 | `DashboardServiceImpl::from_external_spawn_config` reads `cfg.live` instead of raw fields |
| ✏️ Mod | `src-tauri/src/app_runtime_launch.rs` | +20/-10 | `build_external_spawn_config` constructs `Arc<LiveExternalConfig>`, passes `config_manager.subscribe()` |

**Total**: ~1300 LoC (roughly 625 impl + 625 test) across 9 files.

### 4.3 Dependency graph (crate-boundary sanity)

```
oneshim-core
  ├ config::ConfigManager::subscribe() → watch::Receiver<Arc<AppConfig>>
  ├ config::sections::network::{LoadThresholds}
  └ ports::... (unchanged)

oneshim-web (this PR)
  └ grpc::external::
      ├ live_config (new)      — depends only on core: LoadPolicy, no new crate deps
      ├ request_id_layer (new) — depends on workspace: uuid, http, tower
      ├ trailer_body (new)     — depends on workspace: http, http-body, tokio (oneshot)
      ├ config_reload (new)    — depends on core: AppConfig, LoadThresholds
      ├ audit_layer (modified) — depends on new: request_id_layer::RequestId, trailer_body
      └ spawn_config (modified)— holds Arc<LiveExternalConfig>

src-tauri (bridge)
  └ app_runtime_launch — passes ConfigManager's Receiver to spawn_config
```

No Hexagonal Architecture violations (ADR-001 §1): adapter crate `oneshim-web` consumes core ports/types; no reverse dependency; no direct adapter-to-adapter coupling.

### 4.4 `uuid` dependency

Already in workspace root `Cargo.toml`:
```toml
uuid = { version = "1", features = ["v4", "serde"] }
```

`oneshim-web/Cargo.toml` may or may not already include it; if absent, add `uuid.workspace = true` under `[dependencies]`. No new version lockfile churn expected.

---

## 5. Components — Detailed API

### 5.1 `LiveExternalConfig`

**File**: `crates/oneshim-web/src/grpc/external/live_config.rs`

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use arc_swap::ArcSwap;

use crate::grpc::load_policy::LoadPolicy;

/// Runtime-tunable config slice for the external gRPC server.
///
/// Readers use `streaming_enabled()` / `load_policy()` on every request and
/// observe eventually-consistent snapshots (Relaxed memory ordering — no
/// happens-before guarantees are needed across config fields).
///
/// Writers are restricted to `ConfigReloadTask` via `pub(crate)` visibility.
pub(crate) struct LiveExternalConfig {
    streaming_enabled: AtomicBool,
    load_policy: ArcSwap<LoadPolicy>,
}

impl LiveExternalConfig {
    pub fn new(initial_streaming: bool, initial_policy: LoadPolicy) -> Self {
        Self {
            streaming_enabled: AtomicBool::new(initial_streaming),
            load_policy: ArcSwap::new(Arc::new(initial_policy)),
        }
    }

    /// Non-blocking, lock-free read. Called on every streaming RPC.
    pub fn streaming_enabled(&self) -> bool {
        self.streaming_enabled.load(Ordering::Relaxed)
    }

    /// Non-blocking read. Returns a cheap-to-clone Arc snapshot.
    pub fn load_policy(&self) -> Arc<LoadPolicy> {
        self.load_policy.load_full()
    }

    pub(crate) fn set_streaming_enabled(&self, v: bool) {
        self.streaming_enabled.store(v, Ordering::Relaxed);
    }

    pub(crate) fn set_load_policy(&self, p: LoadPolicy) {
        self.load_policy.store(Arc::new(p));
    }
}
```

**Invariants**:
- `Send + Sync` — via `AtomicBool` + `ArcSwap` (both `Sync`).
- Readers never block or wait.
- `Ordering::Relaxed` is safe: each field is independent; readers don't need cross-field happens-before.

### 5.2 `RequestIdLayer`

**File**: `crates/oneshim-web/src/grpc/external/request_id_layer.rs`

**Public surface**:
```rust
#[derive(Debug, Clone)]
pub(crate) struct RequestId(pub String);

#[derive(Clone, Default)]
pub(crate) struct RequestIdLayer;

impl<S: Clone> Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;
    fn layer(&self, inner: S) -> Self::Service { RequestIdService { inner } }
}

pub(crate) struct RequestIdService<S> { inner: S }
```

**Header name**: `x-request-id` (lowercase per HTTP/2 wire convention).

**Validation rules** for incoming value (validated on the **raw** string — no trim, no mutation):
- Non-empty
- Length in `1..=128` bytes
- All bytes in `0x21..=0x7E` (ASCII graphic — excludes space `0x20`, tab, control chars, high bytes)
- Value is `.to_str()` lossless (i.e., valid UTF-8 / ASCII)

**Rationale for no-trim**: the ID is correlation metadata. Silently trimming would produce a different ID than what the caller sent, breaking cross-system correlation. Whitespace-padded input is treated as malformed → new UUID generated.

**Generation fallback**: `uuid::Uuid::new_v4().to_string()` — 36 chars, lowercase hex + hyphens.

**Validation failure handling**: log `tracing::warn!(incoming = %raw, reason = %why, "external_grpc: invalid x-request-id, generating new")`, generate fresh UUIDv4, continue. Never reject the request.

**Ingress logic** (`call`):
```rust
fn call(&mut self, mut req: http::Request<B>) -> Self::Future {
    let incoming = req.headers().get("x-request-id").and_then(|h| h.to_str().ok());
    let request_id = match incoming {
        Some(raw) if is_valid(raw) => raw.to_string(),
        Some(raw) => {
            tracing::warn!(raw = %raw.chars().take(32).collect::<String>(), "invalid x-request-id");
            Uuid::new_v4().to_string()
        }
        None => Uuid::new_v4().to_string(),
    };
    req.extensions_mut().insert(RequestId(request_id.clone()));

    let mut inner = self.inner.clone();
    Box::pin(async move {
        let mut response = inner.call(req).await?;
        // Inject into response; overwrite any pre-existing value from handler.
        if let Ok(hv) = HeaderValue::from_str(&request_id) {
            response.headers_mut().insert("x-request-id", hv);
        }
        Ok(response)
    })
}

fn is_valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes().all(|b| (0x21..=0x7E).contains(&b))
}
```

The raw value is validated and — if valid — used verbatim as the `RequestId`. No trim, no normalization. The caller's exact string is preserved for cross-system correlation; whitespace-padded input (any `0x20`, `\t`, `\n`, ...) fails the `0x21..=0x7E` check and triggers UUID generation.

### 5.3 `TrailerCapturingBody<B>`

**File**: `crates/oneshim-web/src/grpc/external/trailer_body.rs`

```rust
use std::pin::Pin;
use std::task::{Context, Poll};
use http::HeaderMap;
use http_body::{Body, Frame};
use tokio::sync::oneshot;
use pin_project_lite::pin_project;

pin_project! {
    pub(crate) struct TrailerCapturingBody<B> {
        #[pin]
        inner: B,
        signal: Option<oneshot::Sender<Option<tonic::Code>>>,
        captured: Option<tonic::Code>,
    }

    impl<B> PinnedDrop for TrailerCapturingBody<B> {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            if let Some(tx) = this.signal.take() {
                // Best-effort; receiver may have been dropped (e.g., the deferred
                // audit task was cancelled). Ignore send errors.
                let _ = tx.send(*this.captured);
            }
        }
    }
}

impl<B> TrailerCapturingBody<B> {
    pub fn new(inner: B, signal: oneshot::Sender<Option<tonic::Code>>) -> Self {
        Self { inner, signal: Some(signal), captured: None }
    }
}

impl<B> Body for TrailerCapturingBody<B>
where
    B: Body,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let result = this.inner.poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &result {
            if let Some(trailers) = frame.trailers_ref() {
                let code = parse_grpc_status(trailers);
                if this.captured.is_none() {
                    *this.captured = code;
                }
                // Fire immediately on first trailer observation so the deferred
                // audit task doesn't wait for body drop.
                if let Some(tx) = this.signal.take() {
                    let _ = tx.send(*this.captured);
                }
            }
        }
        result
    }

    fn is_end_stream(&self) -> bool { self.inner.is_end_stream() }
    fn size_hint(&self) -> http_body::SizeHint { self.inner.size_hint() }
}

fn parse_grpc_status(trailers: &HeaderMap) -> Option<tonic::Code> {
    trailers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok())
        .map(tonic::Code::from_i32)
}

pub(crate) fn map_code_to_audit_status(code: Option<tonic::Code>) -> oneshim_core::models::audit::AuditStatus {
    use oneshim_core::models::audit::AuditStatus;
    use tonic::Code::*;
    match code {
        None | Some(Ok) => AuditStatus::Completed,
        Some(PermissionDenied) | Some(Unauthenticated) => AuditStatus::Denied,
        Some(Cancelled) | Some(DeadlineExceeded) => AuditStatus::Timeout,
        _ => AuditStatus::Failed, // Unknown, InvalidArgument, NotFound, ... → Failed
    }
}
```

**Rationale for pin-projection**: `http_body::Body::poll_frame` takes `Pin<&mut Self>`. We need to project the `inner` field as `Pin<&mut B>`. `pin_project_lite` is preferred over `pin_project` (no proc-macro dependency, already in workspace if previously used).

**Drop semantics**: The `PinnedDrop` impl fires `signal` with `captured` if it was never sent. Two termination paths:
1. Trailer observed → fire in `poll_frame` with the captured code
2. Body dropped without trailer → fire in `Drop` with `None` (mapped to `Completed` downstream — the conservative default for non-observable cases)

**Why not fire on `is_end_stream` returning true without trailer?**: HTTP/2 streams can be cancelled mid-frame or aborted at the transport layer. `is_end_stream` may never be called. Drop is the reliable termination signal.

### 5.4 `ConfigReloadTask`

**File**: `crates/oneshim-web/src/grpc/external/config_reload.rs`

```rust
use std::sync::Arc;
use tokio::sync::watch;
use oneshim_core::config::AppConfig;

use crate::grpc::load_policy::LoadPolicy;
use super::live_config::LiveExternalConfig;

pub(crate) async fn run_config_reload(
    live: Arc<LiveExternalConfig>,
    mut config_rx: watch::Receiver<Arc<AppConfig>>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    tracing::debug!("external_grpc: config reload task started");
    loop {
        tokio::select! {
            biased;  // check shutdown first
            _ = shutdown_rx.changed() => {
                tracing::debug!("external_grpc: config reload task shutting down");
                break;
            }
            res = config_rx.changed() => {
                if res.is_err() {
                    tracing::warn!("external_grpc: ConfigManager sender dropped; exiting reload task");
                    break;
                }
                apply_config(&live, &*config_rx.borrow_and_update());
            }
        }
    }
}

fn apply_config(live: &LiveExternalConfig, cfg: &AppConfig) {
    let new_streaming = cfg.web.grpc_streaming_enabled;
    let new_thresholds = cfg.web.grpc_load_thresholds.clone().unwrap_or_default();
    let new_policy = LoadPolicy::new(new_thresholds);

    live.set_streaming_enabled(new_streaming);
    live.set_load_policy(new_policy);

    tracing::info!(
        streaming_enabled = new_streaming,
        "external_grpc: live config applied"
    );
}
```

**Spawn site**: `serve_external` (in `grpc/external/mod.rs`):

```rust
let reload_handle = tokio::spawn(run_config_reload(
    cfg.live.clone(),
    cfg.config_rx.clone(),
    cfg.shutdown_rx.clone(),
));
```

The `JoinHandle` is tracked alongside the cert watcher + expiry monitor handles (existing pattern); supervisor awaits it on shutdown.

**`biased;` in select**: Prefer shutdown over config-changed. Under shutdown, we don't want to apply a final config change that might be stale.

**`LoadPolicy::new()`**: Assumed to be infallible from `LoadThresholds` (verify during implementation — if fallible, wrap in `try_new` and log error without breaking the task).

### 5.5 `AuditLayer::call` — modified

**Key changes**:

1. Read `RequestId` from extensions (injected by `RequestIdLayer`).
2. After `inner.call(req).await?`, wrap the response body with `TrailerCapturingBody`.
3. Create `oneshot::channel::<Option<tonic::Code>>`. Pass `tx` to body; hold `rx` for deferred task.
4. Spawn deferred audit task: awaits `rx`, maps status, calls `record_completion` with `command_id = Some(request_id)`.
5. Return response with wrapped body synchronously.

**Crucial**: the deferred task holds captured clones (`bridge`, `metrics`, `ctx`, `operation`, `remote`, `request_id`, `msg_counter`, `start`). It does not borrow from the parent scope.

**Started record** is kept synchronous (before `inner.call`) as before, but `command_id` is now passed as `Some(request_id)` rather than `None`.

**Pseudocode** (elided boilerplate):
```rust
fn call(&mut self, mut req: http::Request<B>) -> Self::Future {
    let mut inner = self.inner.clone();
    let bridge = self.bridge.clone();
    let metrics = self.metrics.clone();

    let auth_ctx = req.extensions().get::<AuthContext>().cloned();
    let peer = req.extensions().get::<PeerInfo>().cloned();
    let request_id = req.extensions().get::<RequestId>().map(|r| r.0.clone());
    let operation = req.uri().path().to_string();

    let msg_counter = Arc::new(AtomicU64::new(0));
    req.extensions_mut().insert(msg_counter.clone());

    Box::pin(async move {
        let Some(ctx) = auth_ctx else { return inner.call(req).await; };
        let Some(peer) = peer else { return inner.call(req).await; };
        let remote = peer.remote_addr.to_string();

        let _ = bridge.record(
            &ctx, remote.clone(), &operation,
            "ok", AuditStatus::Started,
            Duration::ZERO, None, None, request_id.clone(),
        ).await;

        let start = Instant::now();
        let response = inner.call(req).await?;

        let (tx, rx) = oneshot::channel::<Option<tonic::Code>>();
        let (parts, body) = response.into_parts();
        let wrapped = TrailerCapturingBody::new(body, tx);
        let response = http::Response::from_parts(parts, wrapped);

        tokio::spawn(async move {
            let observed = rx.await.ok().flatten();
            let status = map_code_to_audit_status(observed);
            let duration = start.elapsed();
            let msg_count = msg_counter.load(Ordering::Relaxed);
            let msg_count_opt = (msg_count > 0).then_some(msg_count);

            let _ = bridge.record_completion(
                &ctx, remote, &operation, status,
                duration, msg_count_opt, request_id,
            ).await;

            metrics.request_bump(
                "external",
                ctx.auth_type.as_str(),
                audit_status_label(status),
            );
        });

        Ok(response)
    })
}

fn audit_status_label(s: AuditStatus) -> &'static str {
    match s {
        AuditStatus::Completed => "ok",
        AuditStatus::Denied => "denied",
        AuditStatus::Timeout => "timeout",
        AuditStatus::Failed => "failed",
        AuditStatus::Started => "started", // should never reach here
    }
}
```

**Type parameter note**: `AuditLayer` currently has generic `<S, B, RespBody>`. Replacing `RespBody` in the return type with `TrailerCapturingBody<RespBody>` changes the outer response body type. Consumers (tonic server builder) accept any `http_body::Body`, so this is safe. Confirm during implementation that the `AuditService::Response` type compiles as `http::Response<TrailerCapturingBody<RespBody>>`.

### 5.6 `ExternalGrpcSpawnConfig` — modified

```rust
pub struct ExternalGrpcSpawnConfig {
    // ... existing fields unchanged ...

    // REMOVED:
    //   pub streaming_enabled: bool,
    //   pub load_policy: Arc<LoadPolicy>,

    // ADDED:
    pub live: Arc<LiveExternalConfig>,
    /// Watch receiver for live config reload. The paired Sender is owned by
    /// ConfigManager; dropping the manager ends the reload task cleanly.
    pub config_rx: watch::Receiver<Arc<AppConfig>>,
}
```

**Debug impl adjustment**: replace `.field("streaming_enabled", ...)` with `.field("streaming_enabled_live", &self.live.streaming_enabled())`. `config_rx` debug impl is uninteresting; elide.

### 5.7 `build_external_spawn_config` — modified signature

**src-tauri/src/app_runtime_launch.rs** — add 2 parameters:

```rust
async fn build_external_spawn_config(
    // ... existing 9 params ...
    config_manager: Arc<ConfigManager>,  // NEW — for subscribe()
) -> anyhow::Result<ExternalGrpcSpawnConfig> {
    // ... existing construction ...

    let initial_streaming = /* passed-in value from caller; unchanged plumbing */;
    let initial_policy = /* passed-in value from caller; unchanged plumbing */;
    let live = Arc::new(LiveExternalConfig::new(initial_streaming, initial_policy));
    let config_rx = config_manager.subscribe();

    Ok(ExternalGrpcSpawnConfig {
        // ... existing ...
        live,
        config_rx,
    })
}
```

Call site at `L897` (`build_external_spawn_config(...)`) adds the `config_manager.clone()` arg. This is an addition only; no existing arg removed → diff is surgical.

---

## 6. Data Flow

### 6.1 Request/response pipeline (successful unary RPC)

```
Client ──GET /DashboardService/GetAgentInfo──►
  x-request-id: req-abc123
  authorization: Bearer <jwt>
                                       │
                                       ▼
                              ┌─────────────────┐
                              │   AuthLayer     │ validates JWT → inserts
                              │                 │ AuthContext + PeerInfo
                              └────────┬────────┘
                                       │
                                       ▼
                              ┌─────────────────┐
                              │ RequestIdLayer  │ extracts req-abc123, validates,
                              │                 │ inserts RequestId("req-abc123")
                              └────────┬────────┘
                                       │
                                       ▼
                              ┌─────────────────┐
                              │   AuditLayer    │ reads RequestId,
                              │                 │ records Started (command_id=req-abc123),
                              │                 │ wraps response body
                              └────────┬────────┘
                                       │
                                       ▼
                              ┌─────────────────┐
                              │ Handler (impl)  │ returns Response<Body>
                              └────────┬────────┘
                                       │
                              [body = TrailerCapturingBody<Body>]
                                       │
                                       ▼
                              (tonic writes frames → trailer grpc-status:0)
                                       │
                              [TrailerCapturingBody.poll_frame observes trailer,
                               captures Ok, fires oneshot::send(Some(Ok))]
                                       │
                                       ▼
                            Deferred audit task:
                              rx.await = Some(Some(Ok))
                              status = Completed
                              bridge.record_completion(command_id=req-abc123, ...)
                                       │
Response headers:                      │
  x-request-id: req-abc123     ◄───────┘ (injected by RequestIdLayer after inner.call)
```

### 6.2 Streaming RPC with deadline exceeded

```
Client ──SubscribeMetrics(deadline=5s)──►
   → AuthLayer → RequestIdLayer → AuditLayer → handler

Handler returns Response<StreamBody> with initial ServerLoadHint emitted.
AuditLayer records Started synchronously, wraps body, returns response.
Deferred task awaits oneshot.

... streaming for 5s ...

Client timer fires → client closes stream.
tonic server-side sees RST_STREAM → stops polling body.
Body frames flushed + grpc-status: 4 (DeadlineExceeded) trailer sent.

TrailerCapturingBody.poll_frame sees trailer,
  captured = Some(DeadlineExceeded),
  tx.send(Some(DeadlineExceeded)).

Deferred task:
  observed = Some(DeadlineExceeded)
  status = Timeout
  duration = elapsed from inner.call start to now (includes full 5s stream)
  msg_count = N (from CountingStream)
  record_completion(command_id=<uuid>, status=Timeout, duration=5s, msg_count=N, ...)
```

### 6.3 Request with missing or invalid x-request-id

```
Client ──GET /.../GetAgentInfo── (NO x-request-id header) ──►
  → AuthLayer (OK)
  → RequestIdLayer:
      incoming = None → generate UUIDv4 = "f47ac10b-58cc-4372-a567-0e02b2c3d479"
      req.extensions.insert(RequestId("f47ac10b-..."))
  → AuditLayer records Started with command_id = Some("f47ac10b-...")
  → ... handler ... → response
  ← RequestIdLayer: response.headers.insert("x-request-id", "f47ac10b-...")
Client sees: x-request-id: f47ac10b-58cc-4372-a567-0e02b2c3d479

---

Client ──... x-request-id: \x00malformed── (invalid char) ──►
  → AuthLayer (OK)
  → RequestIdLayer:
      validate("\x00malformed") → fail (non-graphic)
      tracing::warn!("invalid x-request-id, generating new")
      generate UUIDv4 = "new-uuid-..."
      req.extensions.insert(RequestId("new-uuid-..."))
  → ... → response.headers: x-request-id: new-uuid-...
```

### 6.4 Live config reload data flow

```
User edits ~/.config/oneshim/config.json:
  "web": { "grpc_streaming_enabled": false, ... }

ConfigManager.reload() or watched file change:
  self.sender.send(Arc::new(new_cfg))

         │
         ▼
  watch::Receiver<Arc<AppConfig>> — all subscribers see the new version
         │
         ▼
  ConfigReloadTask (spawned in serve_external):
    config_rx.changed() fires
    apply_config(&live, &new_cfg):
      live.set_streaming_enabled(false)
      live.set_load_policy(LoadPolicy::new(new_cfg.web.grpc_load_thresholds.unwrap_or_default()))

         │
         ▼
  Next SubscribeMetrics RPC arrives:
    handler reads cfg.live.streaming_enabled() → false
    returns Status::unavailable("streaming disabled")

Convergence time: config change → live-config swap ≤ watch-notify latency
                  (typically <10ms, bounded by tokio scheduler).
                  G3 target of ≤5s is conservative.
```

### 6.5 Shutdown coordination

```
Supervisor fires shutdown_tx.send(true) on exit / signal:
  → shutdown_rx.changed() fires in all 4 long-lived tasks:
      1. cert watcher          — exits file-watch loop
      2. cert expiry monitor   — exits sleep loop
      3. ConfigReloadTask (NEW)— exits select loop (biased: shutdown first)
      4. tonic server (serve_with_incoming) — graceful shutdown
  → All JoinHandles awaited by supervisor before final cleanup.
```

No new shutdown coordination; `ConfigReloadTask` simply joins the existing 3-task pattern.

---

## 7. Configuration

### 7.1 Consumed config fields

| AppConfig path | Type | Default | Consumed by |
|---------------|------|---------|-------------|
| `web.grpc_streaming_enabled` | `bool` | `true` (existing) | `LiveExternalConfig.streaming_enabled` |
| `web.grpc_load_thresholds` | `Option<LoadThresholds>` | `None` → `LoadThresholds::default()` | `LiveExternalConfig.load_policy` (via `LoadPolicy::new`) |

No new config fields. No schema migration. Users who never edited these fields see no behavior change; users who edit them see changes take effect on next `ConfigManager` reload.

### 7.2 Not watched (restart-required)

Unchanged restart-required fields (for reference):
- `external_grpc.enabled`
- `external_grpc.bind_address`, `external_grpc.port`
- `external_grpc.auth_mode`
- `external_grpc.jwt_public_key_path`, `jwt_expected_issuer`, `jwt_expected_audience`
- `external_grpc.mtls_fingerprint_allowlist_path`
- `external_grpc.tls_cert_path`, `tls_key_path` (cert *content* reloads via `HotReloadCertResolver`, path change needs restart)
- `external_grpc.max_concurrent_streams`, `max_connections`

A future spec may extend live reload to additional fields, but that's out of scope here (NG2).

---

## 8. Error Handling

### 8.1 RequestIdLayer failure modes

| Condition | Action | Audit visibility |
|-----------|--------|------------------|
| Incoming header is invalid UTF-8 | `tracing::warn`, generate fresh UUID | Started+Completed pair with generated ID |
| Incoming header too long (>128) | `tracing::warn` (log first 32 chars only), generate fresh UUID | Same |
| Incoming header has non-graphic chars | Same | Same |
| Response `HeaderValue::from_str` fails (never, by construction) | Skip injection, log at `error!` level (this indicates a bug) | Audit row still written with validated ID |
| UUID generation panics | Never (uuid crate guarantees infallible for v4) | — |

**Design choice**: Never reject a request due to bad request ID. The header is informational; rejecting would be a regression vs current pass-through behavior.

### 8.2 TrailerCapturingBody failure modes

| Condition | Action | Audit row |
|-----------|--------|-----------|
| `inner.poll_frame` returns `Err` | Error propagated to tonic; body is dropped; our `Drop` fires `tx.send(None)` | Completed (None→Completed mapping — body drop without trailer is the default) |
| `poll_frame` returns `Ready(None)` without trailer | Body ended normally but no trailer; `Drop` fires `tx.send(None)` | Completed |
| Multiple trailer frames (protocol violation) | First trailer captured, subsequent ignored (signal already taken) | Completed/Denied/Timeout/Failed per first trailer |
| grpc-status header value is non-numeric | `parse::<i32>()` fails, captured stays `None` | Completed (conservative default) |
| `oneshot::Receiver` dropped before `tx.send` | `send` returns Err (receiver gone); ignored | No audit row (deferred task already cancelled) |

**Edge case — receiver drops during deferred task**: if the tokio runtime cancels the spawned audit task (e.g., runtime shutdown mid-request), the oneshot receiver is dropped. The body's Drop still attempts `tx.send(captured)`, which returns Err silently. No audit row written — but this matches existing behavior (panic catching already drops pending audit records).

### 8.3 ConfigReloadTask failure modes

| Condition | Action |
|-----------|--------|
| `config_rx.changed().await` returns `Err` (sender dropped) | Log `warn`, exit loop cleanly |
| `shutdown_rx.changed()` fires | Log `debug`, exit loop cleanly |
| `LoadPolicy::new` fails (if fallible) | Log `error!(err = %e, "load_policy construction failed; keeping previous")`, skip this reload cycle, continue |
| `apply_config` panics (should never, but defensive) | Panic propagates → task aborts; no auto-restart. Supervisor sees detached JoinHandle fail; logs warn |

**Consequence of task exit**: The last-applied config continues to serve requests. New config changes are not picked up until server restart. Monitoring can detect this via a metric (see §8.6) or tracing logs.

### 8.4 AuditLayer interaction failures

| Condition | Action |
|-----------|--------|
| `bridge.record(...)` returns Err | `let _ = ...` pattern (existing) — audit failure is non-fatal for the request |
| Spawned task panics | task aborts; no audit record for that request; parent request unaffected |
| `record_completion` pre-empted by process termination | Pending audit may be lost (existing behavior — ADR note in audit_log port doc) |

### 8.5 x-request-id collision risk

UUIDv4 collision probability: ~5×10^-11 over 1e9 requests/day (cf. NIST). Acceptable.

Malicious caller supplies same x-request-id repeatedly: each is treated independently (no dedup, no rate limit by ID) — this is correlation metadata, not identity. Security teams querying audit logs by ID may see multiple rows with the same ID — each with its own start/end timestamp and auth_type, which is still useful.

### 8.6 Observability additions (optional, recommend)

New tracing log event on successful reload (INFO level): already included in §5.4.

**Optional metric** (deferred unless requested): add `external_grpc_config_reload_total` counter in `ExternalMetrics` (bumped on each successful `apply_config`). Not blocking for this PR.

---

## 9. Testing Strategy

### 9.1 Unit tests (per module)

#### 9.1.1 `live_config`

- `streaming_enabled_round_trip` — init false, read false; set true, read true
- `load_policy_swap_returns_new_snapshot` — Arc identity after `set_load_policy`
- `concurrent_read_write_no_data_race` — 4 tokio tasks × 1000 reads while writer toggles; expect no panic, final value observable
- `send_sync_bounds` — compile-time assertion via `fn assert<T: Send + Sync>() {}; assert::<LiveExternalConfig>()`

#### 9.1.2 `request_id_layer`

- `accepts_valid_incoming_header` — "test-req-123" preserved in extension + response header
- `generates_uuid_when_missing` — empty headers → extension and response contain UUIDv4 format (36 chars, 4 hyphens)
- `rejects_invalid_characters` — "req\x00bad" → fresh UUID generated, tracing::warn captured
- `rejects_too_long` — 200-char input → fresh UUID
- `rejects_empty_or_whitespace` — "" or "   " → fresh UUID
- `preserves_request_id_across_call` — extension reaches inner service; response injected with same value
- `overwrites_handler_supplied_x_request_id` — handler sets response header to "wrong"; layer overwrites with validated/issued value
- `valid_boundary_128_chars` — exactly 128 graphic chars → accepted

#### 9.1.3 `trailer_body`

- `captures_ok_trailer` — body with data + `grpc-status: 0` trailer → oneshot delivers `Some(Ok)`
- `captures_permission_denied` — `grpc-status: 7` → `Some(PermissionDenied)`
- `captures_deadline_exceeded` — `grpc-status: 4` → `Some(DeadlineExceeded)`
- `drop_without_trailer_sends_none` — body ends, no trailer → oneshot delivers `None`
- `drop_mid_stream_sends_none` — body not fully polled, dropped → oneshot delivers `None`
- `parse_grpc_status_invalid_value` — "abc" → `captured = None`
- `first_trailer_wins_if_multiple` — protocol-violating multiple trailers → first captured
- `receiver_dropped_before_send_is_safe` — drop rx before body poll → no panic
- `map_code_to_audit_status` — table-driven over all 16 tonic::Code variants

#### 9.1.4 `config_reload`

- `applies_initial_config_on_change` — config_rx emits new AppConfig → live reflects new streaming + policy
- `exits_on_shutdown` — shutdown_rx fires → task completes within 100ms
- `exits_when_config_sender_dropped` — drop ConfigManager → task exits with warn log
- `biased_prefers_shutdown_over_config_change` — both signaled same tick → shutdown wins, no config applied after shutdown
- `load_policy_constructed_from_thresholds` — apply with custom LoadThresholds → `live.load_policy()` returns matching policy (verify via public accessor)

#### 9.1.5 `audit_layer` (modifications)

- `record_started_includes_request_id` — inject RequestId extension → bridge.record receives command_id=Some(id)
- `deferred_completion_maps_ok_to_completed` — inner returns Ok response with `grpc-status: 0` → Completed recorded with request_id
- `deferred_completion_maps_7_to_denied` — `grpc-status: 7` → Denied
- `deferred_completion_maps_4_to_timeout` — `grpc-status: 4` → Timeout
- `deferred_completion_maps_unknown_to_failed` — `grpc-status: 2` → Failed
- `body_drop_without_trailer_records_completed` — abrupt drop → Completed (conservative)
- `missing_request_id_extension_records_none` — (defensive) no RequestIdLayer upstream → command_id=None

### 9.2 Integration tests (`tests/external_grpc_integration.rs`)

All gated on `feature = "grpc-dashboard-external, external-grpc-tools, test-support"`.

- `external_grpc_request_id_incoming_preserved_in_response` — client sends "req-123", asserts response header = "req-123"
- `external_grpc_request_id_generated_when_missing` — no header sent, asserts response header matches UUID regex
- `external_grpc_request_id_invalid_replaced` — sends malformed ID, asserts response has UUID (different from sent)
- `external_grpc_audit_completed_for_ok_response` — make request, read audit via CapturingAudit, assert Completed + command_id populated
- `external_grpc_audit_denied_for_permission_denied` — configure handler to return `Status::permission_denied`, assert audit = Denied
- `external_grpc_audit_timeout_for_cancelled_stream` — open SubscribeMetrics, client cancels mid-stream, assert audit = Timeout
- `external_grpc_live_streaming_toggle_reflects_immediately` — spawn server, reload config with streaming_enabled=false, next SubscribeMetrics returns `Unavailable`
- `external_grpc_live_load_thresholds_applied` — reload with new LoadThresholds, next request's enforcement reflects (e.g., cpu_low_pct threshold change → shed behavior)
- `external_grpc_config_reload_task_exits_on_shutdown` — spawn, trigger shutdown, assert task joins within timeout

Expected count: ~9 new integration tests added to the current 19 = total ~28.

### 9.3 Property-based / fuzz tests (optional)

If quick to add:
- `RequestIdLayer::is_valid` — quickcheck over arbitrary byte strings: no panic, deterministic classification
- `parse_grpc_status` — quickcheck over arbitrary HeaderMap: no panic

Not required for G4; consider during implementation if `quickcheck` is already a dev-dep.

### 9.4 Performance test (regression guard for G5)

Not a gating test (too flaky for CI), but a benchmark script:
- `cargo bench --bench external_grpc_overhead` (new benchmark) — single-threaded client hitting a simple unary endpoint through the full layer stack
- Baseline: PR #486 HEAD (prior to this PR)
- Target: ≤200µs median increase at p50

Document result in PR description; reject if regression >500µs.

### 9.5 Test organization

| Crate:module | Test count (new) | Run command |
|-------------|------------------|-------------|
| `oneshim-web::grpc::external::live_config::tests` | 4 | `cargo test -p oneshim-web --features ... --lib live_config` |
| `oneshim-web::grpc::external::request_id_layer::tests` | 8 | `--lib request_id_layer` |
| `oneshim-web::grpc::external::trailer_body::tests` | 9 | `--lib trailer_body` |
| `oneshim-web::grpc::external::config_reload::tests` | 5 | `--lib config_reload` |
| `oneshim-web::grpc::external::audit_layer::tests` (augmented) | +7 | `--lib audit_layer` |
| `oneshim-web` integration | +9 | `--test external_grpc_integration` |
| **Total** | **~42** | — |

---

## 10. Migration and Phase 9 Coexistence

### 10.1 Breaking changes (internal only, no public API)

- `ExternalGrpcSpawnConfig::streaming_enabled: bool` → `ExternalGrpcSpawnConfig::live.streaming_enabled()`
- `ExternalGrpcSpawnConfig::load_policy: Arc<LoadPolicy>` → `ExternalGrpcSpawnConfig::live.load_policy()`
- `build_external_spawn_config` gains 1 param (`config_manager`)

All call sites within `crates/oneshim-web` + `src-tauri` are updated in this PR. No external consumers (none outside workspace).

`ExternalGrpcSpawnConfig` is `pub` but the type isn't exported outside `oneshim-web`. Documenting as "internal struct" in the module docstring is sufficient.

### 10.2 Phase 9 coexistence (critical — other worktree has active WIP)

| File | This spec touches | Phase 9 branch (`feature/phase9-tracking-schedule`) touches | Expected rebase difficulty |
|------|-------------------|-----------------------------------------------------------|----------------------------|
| `src-tauri/src/app_runtime_launch.rs` | L813 (streaming_enabled site), L897 (build call site), L1206-1325 (build_external_spawn_config body) | L1091 (AudioRuntimeState consent wiring) | Trivial — different line ranges, no shared identifiers |
| `crates/oneshim-web/Cargo.toml` | Maybe +`uuid.workspace=true` if missing | `+chrono-tz` | Trivial — different lines in `[dependencies]` |
| `crates/oneshim-web/src/routes.rs` | untouched | modified (tracking_schedule routes) | No conflict |
| All files in `crates/oneshim-web/src/grpc/external/` | heavy mod | untouched | No conflict |
| All files in `crates/oneshim-core/src/config/sections/` | untouched | modified (+tracking_schedule.rs) | No conflict |
| All files in `src-tauri/src/scheduler/loops/` | untouched | modified | No conflict |

**Rebase strategy**: If Phase 9 lands first, this PR rebases trivially. If this PR lands first, Phase 9 rebases trivially. Order doesn't matter.

**Verification**: explicit test in CI — `git merge-tree main feature/phase9-tracking-schedule feature/external-grpc-audit-liveconfig` should show no conflicts. (Run manually during plan review.)

### 10.3 Rollout

- Single PR, merged via squash (convention per recent history).
- No feature flag — behavior change (accurate audit status) is a strict improvement; config reload is additive.
- `RequestIdLayer` is compiled-in always; zero allocation in the no-incoming-header + no-logger case is ~1 UUID construction (cheap).
- No schema migration. No database change.

---

## 11. Out of Scope / Deferred

Explicitly **not** covered, tracked for future PRs:

| Item | Reason for deferral | Future owner |
|------|--------------------|--------------| 
| W3C TraceContext / OpenTelemetry | Separate observability initiative | TBD |
| Per-response grpc-status observation in loopback server | NG1 — loopback has no AuditLayer | Future loopback-audit spec |
| Live reload of `max_concurrent_streams` | StreamCounter cap change mid-flight is complex | Separate spec |
| Live reload of `auth_mode` / JWT / TLS paths | Requires verifier rebuild + in-flight connection handling | Separate spec |
| Request-ID propagation into downstream internal calls (trace across services) | OpenTelemetry scope | TBD |
| `external_grpc_config_reload_total` metric | Monitoring polish | Follow-up issue |
| TCP connection stress test (T15 deferred from PR #486) | Dedicated CI with elevated ulimit | Separate stress-test-suite spec |
| Subscribe handlers observing `shutdown_rx` to emit `Unavailable` on graceful shutdown (T19 narrow) | Handler-side shutdown awareness is its own scope | Separate streaming-shutdown spec |

---

## 12. Decisions Locked

| ID | Decision | Rationale |
|----|----------|-----------|
| D1 | Bundle scope: external gRPC only | Loopback has no AuditLayer; adding one would be a separate initiative |
| D2 | x-request-id: incoming accepted (when valid) OR UUIDv4 generated | Industry convention (AWS, GCP, GitHub); supports distributed correlation |
| D3 | Validation: ASCII graphic 0x21..0x7E, length 1..=128 | Safe superset of UUIDs + common correlation ID formats (ULID, snowflake, etc.) |
| D4 | Invalid x-request-id → log warn, generate fresh; never reject | Informational header; rejection would be a regression |
| D5 | Response header overwrites handler-supplied value | Single source of truth; prevents handler from accidentally breaking correlation |
| D6 | Full trailer observation via `http_body::Body` wrapper (not Result-only) | Required for accurate streaming-RPC audit status |
| D7 | Drop without trailer → `None` → mapped to `Completed` (conservative) | Matches current PR #486 behavior for body-drop cases |
| D8 | Live reload fields: `streaming_enabled` + `LoadPolicy` thresholds | Highest operational value; avoid verifier-rebuild hazard |
| D9 | `LiveExternalConfig` uses `AtomicBool` + `ArcSwap`, not `RwLock` | Lock-free reads on hot path; proven pattern (HotReloadCertResolver) |
| D10 | `pub(crate)` for `LiveExternalConfig::set_*` | Only `ConfigReloadTask` writes; lock-down via visibility |
| D11 | `ConfigReloadTask` uses `biased; shutdown → config_changed` order | Prefer clean exit over final stale config application |
| D12 | `AuditLayer` spawns deferred task for `record_completion` | Streaming-RPC final status arrives after `inner.call` returns; can't block |
| D13 | `AuthLayer` Started-success spawn remains removed (PR #486 Task 7) | AuditLayer owns Started+Completed pair; no regression |
| D14 | Layer ordering: `.layer(auth).layer(request_id).layer(audit)` (auth outermost) | PR #486's empirical FIFO-on-ingress convention; unauth requests never touch RequestId/Audit |
| D15 | No feature flag | Strict improvement + additive; no rollback concern |
| D16 | `command_id` in audit rows is now `Option<String>` with `Some(request_id)` when RequestIdLayer ran | Reuses existing `command_id` field; no schema change |
| D17 | New files use `pub(crate)` visibility by default | ADR-001 §5 + workspace convention (iter-7 polish pass) |
| D18 | No new tokio runtime requirement | All new tasks run on the existing main runtime |
| D19 | `pin_project_lite` preferred over `pin_project` proc-macro | Smaller compile-time footprint if already adopted; verify during implementation |
| D20 | Span-level tracing unchanged; add `request_id` structured field to key log events | Minimal observability improvement inside existing tracing setup |

---

## 13. Open Questions — Deep-Review Target

Items for spec-loop review rounds (Critical/Important candidates):

- **OQ1**: `LoadPolicy::new(LoadThresholds)` — is this infallible or can it return `Err`? If fallible, error-handling path in `ConfigReloadTask` needs explicit design (log + skip cycle, or log + fall back to previous).
- **OQ2**: `pin_project_lite` vs `pin_project` — verify current workspace adoption; adopt whichever is already used elsewhere in `oneshim-web` to avoid new dep.
- **OQ3**: Spawned deferred audit task — runtime panic from inside `record_completion` (e.g., storage unreachable) aborts the spawn. Do we need a panic catcher around it for belt-and-suspenders? (Existing `Arc<dyn AuditLogPort>` impls already panic-safe by contract.)
- **OQ4**: ~~Incoming `x-request-id` with trailing/leading whitespace — validate raw (reject) or trim?~~ **Answered inline** (§5.2): validate raw with no trim; whitespace-padded values are treated as malformed → fresh UUID. Listed here only to surface the decision for reviewer confirmation.
- **OQ5**: Should `AuditLayer` log `request_id` as a structured tracing field on every `record_completion` for Loki/Grafana filtering? Spec currently says yes (D20); confirm coverage in logging events.
- **OQ6**: Test for `external_grpc_audit_timeout_for_cancelled_stream` — how to simulate client cancellation cleanly? Option A: client-side drop of the stream handle. Option B: server returns `Status::cancelled` directly. B is simpler but A is more realistic. Choose during plan phase.
- **OQ7**: `ExternalGrpcSpawnConfig` gains `config_rx: watch::Receiver<Arc<AppConfig>>` — does this create an unexpected Clone blowup if the struct is cloned inside tonic (`Server::builder().layer(...)` takes owned values)? `watch::Receiver` is `Clone` and cheap; should be fine. Verify at implementation.
- **OQ8**: Single oneshot channel per request — memory pressure at high RPS? oneshot is cheap (~80 bytes) and scales linearly with in-flight requests, which are already bounded by `max_concurrent_streams` + `max_connections`. No concern at target load, but benchmark in §9.4.
- **OQ9**: Should `map_code_to_audit_status` map `Internal`/`Unknown`/`DataLoss` distinctly from `Failed`? Current design coalesces them all to `Failed`. Distinguishing "known internal error" from "unclassified" might be valuable. Propose deferral: audit query layer can re-split by raw `grpc-status` stored in `details` JSON.
- **OQ10**: `config_rx.borrow_and_update()` — does this clone the `Arc<AppConfig>` or borrow? If borrow, ensure the borrow is dropped before calling `LoadPolicy::new` to avoid holding the read lock across the clone. (Probably fine — borrow drops at expression end — but confirm.)

---

## 14. Success Criteria (Loop 3 exit)

- [ ] All 42 new tests green (0 fail, 0 ignore)
- [ ] `cargo check` + `cargo clippy -- -D warnings` + `cargo fmt --check` clean for:
  - `oneshim-web` with features `grpc-dashboard-external,external-grpc-tools,test-support`
  - `oneshim-app` with features `external-grpc-tools`
- [ ] `cargo test --workspace` zero regression (baseline: current main)
- [ ] Memory `reference_tonic_layer_order.md` updated if this PR introduces a new layer
- [ ] Docs: `docs/guides/external-grpc.md` + `.ko.md` updated — Auditing section mentions `x-request-id`, new `AuditStatus` granularity (Denied/Timeout/Failed); Live-reload section added with supported-fields table
- [ ] PR description references this spec + commit-squashed body summarizes the 3 bundled items
- [ ] Phase 9 worktree merge-tree check passes (`git merge-tree main phase9-tracking-schedule feature/external-grpc-audit-liveconfig` shows no conflict)

---

## 15. References

- **PR #486** (`5618558c`) — source of deferrals (commit body lists follow-ups)
- `reference_tonic_layer_order.md` — empirical FIFO-on-ingress convention for tonic 0.14
- `tests/external_grpc_integration.rs:929` — x-request-id TODO comment
- `crates/oneshim-web/src/grpc/external/audit_layer.rs:125-127` — hardcoded Completed TODO
- memory `project_next_tasks.md` — D13 V2c live config reload TODO
- memory `feedback_3loop_quality_gate.md` — the 3-loop review flow this spec will go through
- `crates/oneshim-web/src/grpc/external/cert_resolver.rs` — HotReloadCertResolver (pattern precedent for ArcSwap use)
- `crates/oneshim-core/src/config_manager.rs:113` — `ConfigManager::subscribe()` (live reload infrastructure)
- ADR-001 (Rust Client Architecture) — Hexagonal conformance
- ADR-003 (Directory Module Pattern) — new-file placement convention
- ADR-019 (Error Code Infrastructure) — typed error codes (not modified but respected)

---

*End of spec. Next step: deep review rounds (3-loop quality gate Loop 1). When ≥1 reviewer finds no Critical/Important issues across all 3 review lenses, transition to writing-plans.*
