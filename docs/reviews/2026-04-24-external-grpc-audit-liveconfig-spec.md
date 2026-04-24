# External gRPC Audit Completeness + Live Config Reload — Spec

**Date**: 2026-04-24
**Author**: Bundled follow-up spec (PR #486 deferrals + D13 V2c live config TODO)
**Base commit**: `5618558c` (origin/main post-PR-#486)
**Branch**: `feature/external-grpc-audit-liveconfig`
**Status**: Draft **rev-3** — Loop 1 Round 2 verify polish applied. Round-2 verdicts:
- Architecture: CONDITIONAL-PASS (rev-2) → PASS expected after rev-3 (I8 stale §5.7 deleted, I10 started_at_elapsed_ms, I9 Debug load_policy_snapshot)
- Product/Test: CONDITIONAL-PASS (rev-2) → PASS expected after rev-3 (NV1 /api/audit/export now documented as new, NV2 task_alive surfaced)
- Platform/Risk: PASS (rev-2)

5 Round-2 Important resolved + 4 Round-2 Minors polished. Awaiting Round-3 verify confirmation before transitioning to Loop 2 (plan phase).

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
- **G3** (revised per D33) — `streaming_enabled` and `LoadPolicy` thresholds can be toggled/adjusted via config file edit + `ConfigManager` reload, reflected in subsequent request decisions **within one tokio scheduler tick (typically <10ms)** without server restart. CI test asserts convergence ≤1 second (generous margin for scheduler jitter).
- **G4** — Coverage: unit tests for each new module (target ≥90% line coverage), integration tests for each new behavior (x-request-id round-trip, status mapping for Denied/Timeout, live reload toggle effect), zero flakes.
- **G5** — Zero performance regression on the happy path: unary request median latency Δ < +200µs relative to PR #486 baseline.

### 3.2 Non-Goals

- **NG1** (clarified per U1/D22) — Loopback server's own state is unchanged: no `AuditLayer` added, no live-reload for loopback's `DashboardServiceImpl`. Note that `AppConfig.web.grpc_streaming_enabled` is shared between loopback and external servers; the external server gains an **override** `ExternalGrpcConfig.streaming_enabled: Option<bool>` (D22) so operators can toggle external-only behavior without touching loopback. When `None`, external falls back to the shared `web.grpc_streaming_enabled` value as before.
- **NG2** — Live reload of fields requiring rebind or verifier rebuild (`port`, `bind_address`, `auth_mode`, JWT public key path, TLS cert path). These remain restart-required.
- **NG3** — Distributed tracing (OpenTelemetry, W3C TraceContext). `x-request-id` is correlation-only; full tracing is a separate future project.
- **NG4** — Request-ID enforcement or rate-limiting by ID. Informational header only.
- **NG5** — New audit action types or new `AuditStatus` variants. Reuse the existing 4-variant enum exposed by PR #486 Task 5 (`Started`, `Completed`, `Denied`, `Timeout`, `Failed`).
- **NG6** — Config schema migration. Reuses existing `AppConfig.web.grpc_streaming_enabled` + `AppConfig.web.grpc_load_thresholds` fields; no user-facing config change.
- **NG7** (revised per D21) — Per-field live reload granularity. The whole `LiveSnapshot` is atomic-swapped on any update; readers load a consistent snapshot via `live.snapshot()` and never observe torn cross-field reads. A long-running streaming RPC that calls `snapshot()` multiple times over its lifetime may see different snapshots (eventually-consistent per-decision semantics) — this is intentional: operators expect "flip switch, it takes effect." Explicit test `live_reload_affects_long_running_stream` pins this behavior (§9.2).

---

## 4. Architecture

### 4.1 Layer stack change

```
BEFORE (post-PR #486):                       AFTER (this spec, rev-2):

   ┌──────────────┐                            ┌──────────────┐
   │ tonic Server │                            │ tonic Server │
   └──────┬───────┘                            └──────┬───────┘
          │                                           │
          │                                    ┌──────▼────────┐
          │                                    │RequestIdLayer │ ← OUTERMOST (U5 — NEW)
          │                                    └──────┬────────┘   ingress validate/gen,
          │                                           │             insert RequestId ext,
          │                                           │             egress header inject
          │                                           │
   ┌──────▼───────┐                            ┌──────▼───────┐
   │  AuthLayer   │ ← outermost                │  AuthLayer   │ ← reads RequestId for
   └──────┬───────┘                            └──────┬───────┘   Failed-path command_id
          │                                           │
   ┌──────▼───────┐                            ┌──────▼───────┐
   │  AuditLayer  │ ← hardcoded Completed      │  AuditLayer  │ ← reads RequestId,
   └──────┬───────┘                            └──────┬───────┘   header-first status
          │                                           │           obs, wraps body
          │                                           │           for streaming only,
          │                                           │           deferred record
          │                                           │
   ┌──────▼────────────────────────┐           ┌──────▼────────────────────────┐
   │DashboardServiceServer         │           │DashboardServiceServer         │
   │  (streaming_enabled captured  │           │  (reads cfg.live snapshot     │
   │   at service-build time)      │           │   each call via StreamingSrc) │
   └───────────────────────────────┘           └───────────────────────────────┘

                                                Background task (spawned in
                                                build_external_spawn_config):
                                                ┌───────────────────────────────┐
                                                │  ConfigReloadTask             │ ← NEW
                                                │   watch::Receiver<AppConfig>  │
                                                │   → LiveSnapshot atomic swap  │
                                                │   (single ArcSwap, try_new)   │
                                                └───────────────────────────────┘
```

Layer application order in `serve_external` (tonic 0.14 applies `.layer()` FIFO-on-ingress, so the **first** `.layer()` is outermost):

```rust
Server::builder()
    .layer(request_id_layer)  // OUTERMOST (U5) — assigns ID BEFORE auth so auth
                              //                  rejection rows correlate with client's x-request-id
    .layer(auth_layer)        // second — validates JWT/mTLS; can read RequestId
    .layer(audit_layer)       // innermost — reads RequestId + header-first status
    .add_service(...)
```

**Rationale for RequestIdLayer outermost (U5)**: the correlation chain at the security boundary is where operators need it most. Auth-rejected audit rows get the same `command_id` as the request's `x-request-id` header, enabling end-to-end trace from client error report → server-side audit row. Cost: every unauth request pays ~30ns for UUID construction + extension insert. Negligible.

tonic 0.14 layer semantics: first `.layer()` call is outermost on ingress (per PR #486 empirical fix, documented in `reference_tonic_layer_order.md` memory).

### 4.2 Component map (revised — rev-2)

| Kind | Path | LoC est. | Role |
|------|------|----------|------|
| 🆕 New | `grpc/external/live_config.rs` | ~70 impl + ~90 test | `LiveSnapshot` + `LiveExternalConfig` — single `ArcSwap<LiveSnapshot>` (D21) |
| 🆕 New | `grpc/external/request_id_layer.rs` | ~160 impl + ~180 test | Tower Layer (outermost per U5/D14) — ingress validate/generate, egress header conditional-overwrite (D31) |
| 🆕 New | `grpc/external/trailer_body.rs` | ~170 impl + ~180 test | `http_body::Body` wrapper — trailer observation + `new_already_fired` ctor for trailers-only fast path (D28) |
| 🆕 New | `grpc/external/config_reload.rs` | ~130 impl + ~120 test | tokio task — `watch` subscription → atomic `LiveSnapshot` store. Uses `try_new_with_started_at` (D27) + partial apply (D23) |
| 🆕 New | `grpc/external/streaming_source.rs` (or in `grpc/mod.rs`) | ~40 impl + ~60 test | `enum StreamingSource { Fixed, Live }` (D24) — DashboardServiceImpl dual-mode |
| 🆕 New | `grpc/external/live_config_handler.rs` | ~50 impl + ~60 test | REST `GET /api/external-grpc/live-config` (D29) |
| ✏️ Mod | `grpc/external/audit_layer.rs` | +90/-30 | Header-first status observation (D28/CR1), deferred completion, `RequestId` extraction, status mapping |
| ✏️ Mod | `grpc/external/audit_bridge.rs` | +15/-0 | `record`/`record_completion` gain `command_id: Option<String>` arg (8th) + `grpc_status_code: Option<u32>` in `ExternalGrpcAuditDetails` (D26) |
| ✏️ Mod | `grpc/external/spawn_config.rs` | +15/-4 | `streaming_enabled` + `load_policy` collapsed into `live: Arc<LiveExternalConfig>`; new `config_rx: watch::Receiver<Arc<AppConfig>>`; manual `Debug` impl updated for renamed fields |
| ✏️ Mod | `grpc/external/auth_layer.rs` | +12/-2 | 4 Failed-path spawn blocks read `RequestId` from extensions for command_id (rather than None per U5) |
| ✏️ Mod | `grpc/external/mod.rs` | +35/-5 | `serve_external` inserts `RequestIdLayer` outermost; `pub(crate) mod` lines for 4-6 new files (I7) |
| ✏️ Mod | `grpc/mod.rs` | +50/-15 | `DashboardServiceImpl` holds `streaming_source: StreamingSource` (D24); both `from_spawn_config` + `from_external_spawn_config` updated |
| ✏️ Mod | `grpc/load_policy.rs` | +35/-5 | `try_new` / `try_new_with_started_at` / `started_at` accessor (D23/D27); `LoadPolicyError` enum |
| ✏️ Mod | `grpc/subscribe_metrics.rs`, `subscribe_events.rs` | +10/-5 each | Read `cfg.streaming_source.streaming_enabled()` + `.load_policy()` — no longer take raw `bool` and `Arc<LoadPolicy>` as parameters |
| ✏️ Mod | `oneshim-core/src/ports/audit_log.rs` | +8/-0 | Add `entries_by_command_id(cmd_id: &str, limit: usize)` trait method (D25) |
| ✏️ Mod | `oneshim-storage/src/sqlite/*` (audit impl) | +30/-0 | Implement `entries_by_command_id` — SELECT WHERE command_id = ? (D25) |
| 🆕 New | `oneshim-web/src/handlers/audit_export.rs` | ~80 impl + ~60 test | **New** `GET /api/audit/export` endpoint (D25 / NV1 fix) — rev-2 spec incorrectly assumed this was pre-existing |
| ✏️ Mod | `oneshim-web/src/routes.rs` | +1/-0 | Register new `/api/external-grpc/live-config` route (D29) |
| ✏️ Mod | `oneshim-core/src/config/sections/network.rs` | +8/-0 | Add `ExternalGrpcConfig.streaming_enabled: Option<bool>` (D22) |
| ✏️ Mod | `src-tauri/src/app_runtime_launch.rs` | +30/-10 | `build_external_spawn_config` gains `config_manager` param, constructs `Arc<LiveExternalConfig>` from initial `LiveSnapshot`, spawns `ConfigReloadTask` (D30) |

**Total**: ~1600 LoC (roughly 820 impl + 780 test) across 16-18 files. +300 LoC vs rev-1 due to user-decision expansions (U1 override field, U2 query surface — now confirmed NEW not extend per NV1, D29 REST endpoint, D24 StreamingSource enum, D26 raw code persistence).

**Module declarations** added to `grpc/external/mod.rs` (I7):
```rust
pub(crate) mod config_reload;
pub(crate) mod live_config;
pub(crate) mod live_config_handler;  // if separate file
pub(crate) mod request_id_layer;
pub(crate) mod streaming_source;     // if separate file, else inline in grpc/mod.rs
pub(crate) mod trailer_body;
```

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

### 5.1 `LiveExternalConfig` (revised — single `ArcSwap<LiveSnapshot>` per D21)

**File**: `crates/oneshim-web/src/grpc/external/live_config.rs`

```rust
use std::sync::Arc;
use arc_swap::ArcSwap;

use crate::grpc::load_policy::LoadPolicy;

/// Atomic snapshot of all runtime-tunable config fields. Readers load a
/// snapshot once per request-entry and see a consistent view of all fields.
///
/// Writers (ConfigReloadTask only) atomic-swap the whole snapshot. Updating
/// a single field requires constructing a new snapshot with the other
/// fields carried forward.
#[derive(Clone)]
pub(crate) struct LiveSnapshot {
    pub streaming_enabled: bool,
    pub load_policy: Arc<LoadPolicy>,
}

/// Runtime-tunable config holder for the external gRPC server.
///
/// Readers use `snapshot()` → returns `Arc<LiveSnapshot>` (cheap clone),
/// then access `.streaming_enabled` / `.load_policy` on that snapshot.
/// **The whole snapshot is atomic** — a reader never observes a torn read
/// across fields (a consequence of D21).
///
/// Writers are restricted to `ConfigReloadTask` via `pub(crate)` visibility.
pub(crate) struct LiveExternalConfig {
    current: ArcSwap<LiveSnapshot>,
}

impl LiveExternalConfig {
    pub fn new(initial: LiveSnapshot) -> Self {
        Self {
            current: ArcSwap::new(Arc::new(initial)),
        }
    }

    /// Non-blocking, lock-free read. Called on every request entry.
    /// Returns a consistent snapshot of all fields.
    pub fn snapshot(&self) -> Arc<LiveSnapshot> {
        self.current.load_full()
    }

    /// Atomic replace the full snapshot. Only called by ConfigReloadTask.
    pub(crate) fn store(&self, new: LiveSnapshot) {
        self.current.store(Arc::new(new));
    }
}
```

**Invariants**:
- `Send + Sync` — via `ArcSwap` (both `Sync`).
- Readers never block or wait; `ArcSwap::load_full` is lock-free.
- **Atomic across fields** (D21): unlike the rev-1 dual-atomic design, readers see a consistent cross-field view. A reload that changes both `streaming_enabled` and `load_policy` is observed as a single transition.
- Partial updates (e.g., `apply_config` accepts new `streaming_enabled` but rejects malformed `load_policy`) must construct a snapshot that carries forward the old `load_policy` — see §5.4.

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
        // Conditional overwrite (D31): if handler already set x-request-id AND
        // the value matches our validated ID, leave alone. Otherwise insert ours.
        // This preserves rare proxy-forward patterns where a gRPC handler mirrors
        // an upstream correlation ID, without breaking correlation for the 99% case.
        let should_insert = match response.headers().get("x-request-id") {
            Some(existing) => existing.to_str().map(|s| s != request_id).unwrap_or(true),
            None => true,
        };
        if should_insert {
            if let Ok(hv) = HeaderValue::from_str(&request_id) {
                response.headers_mut().insert("x-request-id", hv);
            }
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

**UUIDv4 compatibility** (addresses Platform I5): `uuid::Uuid::new_v4().to_string()` produces 36 lowercase hex chars + hyphens — all within `0x21..=0x7E` by construction. Generated IDs always satisfy `is_valid`; the inserted value round-trips cleanly through any downstream validator using the same rule.

**Note on layer position**: `RequestIdLayer` is now **outermost** (D14 revised, U5) — it runs *before* `AuthLayer`. This means auth-rejected requests get a `RequestId` populated in their `http::Request::extensions()` before `AuthLayer::call` executes. `AuthLayer`'s 4 Failed-path audit writes are updated to read `req.extensions().get::<RequestId>()` and pass the value as `command_id` in their `bridge.record(...)` calls (§5.5).

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

    /// Construct a wrapper where the status code is already known from
    /// initial response headers (trailers-only response path per D28).
    /// The caller has already fired the oneshot; this wrapper still
    /// implements `Body` faithfully but will not attempt to observe
    /// trailers (there won't be any).
    pub fn new_already_fired(inner: B, captured: Option<tonic::Code>) -> Self {
        Self { inner, signal: None, captured }
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

### 5.4 `ConfigReloadTask` (revised — `try_new` + partial apply + spawn in builder per D23)

**File**: `crates/oneshim-web/src/grpc/external/config_reload.rs`

```rust
use std::sync::Arc;
use tokio::sync::watch;
use oneshim_core::config::AppConfig;

use crate::grpc::load_policy::LoadPolicy;
use super::live_config::{LiveExternalConfig, LiveSnapshot};

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
                apply_config(&live, &config_rx.borrow_and_update());
                // borrow dropped at statement end; no .await held across.
            }
        }
    }
    // Optional metric: flip alive=false on clean exit (see §8.6).
}

fn apply_config(live: &LiveExternalConfig, cfg: &AppConfig) {
    // Start from current snapshot so partial-apply preserves other fields.
    let current = live.snapshot();

    // streaming_enabled: external override with fallback to shared web field (U1/D22).
    //   external_grpc.streaming_enabled: Some(v)  → v
    //   external_grpc.streaming_enabled: None     → web.grpc_streaming_enabled
    let new_streaming = cfg
        .external_grpc
        .streaming_enabled
        .unwrap_or(cfg.web.grpc_streaming_enabled);

    // load_policy: try_new is fallible; preserve started_at across reloads (U4/D27).
    let new_thresholds = cfg.web.grpc_load_thresholds.clone().unwrap_or_default();
    let old_started_at = current.load_policy.started_at();
    let new_load_policy = match LoadPolicy::try_new_with_started_at(new_thresholds, old_started_at) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            tracing::error!(
                err = %e,
                "external_grpc: invalid LoadThresholds in reloaded config; keeping previous load_policy"
            );
            // Partial apply: keep old policy, still update streaming_enabled.
            current.load_policy.clone()
        }
    };

    // Single atomic store of the whole snapshot — no torn reads.
    live.store(LiveSnapshot {
        streaming_enabled: new_streaming,
        load_policy: new_load_policy,
    });

    tracing::info!(
        streaming_enabled = new_streaming,
        "external_grpc: live config applied"
    );
}
```

**Spawn site (D23)**: in `build_external_spawn_config` (src-tauri/app_runtime_launch.rs), **not** inside `serve_external`. This matches the cert-watcher + expiry-monitor pattern: one task per server lifecycle, surviving supervisor respawns without duplicate-spawning.

```rust
// In build_external_spawn_config, after constructing `live` and `config_rx`:
let reload_handle = tokio::spawn(run_config_reload(
    live.clone(),
    config_rx,
    shutdown_rx.clone(),
));
// reload_handle is fire-and-forget; task exits on shutdown_rx or config_rx-dropped.
```

**`biased;` in select**: Prefer shutdown over config-changed. Under shutdown, we don't want to apply a final config change that might be stale.

**`LoadPolicy::try_new_with_started_at`** (new API per D23/D27):
- `LoadPolicy::try_new(thresholds) -> Result<Self, LoadPolicyError>` — fallible constructor, validates `cpu_low < cpu_medium < cpu_high <= 100.0`. Returns `Err(LoadPolicyError::InvalidThresholds { reason: String })` on violation.
- `LoadPolicy::try_new_with_started_at(thresholds, started_at: Instant) -> Result<Self, LoadPolicyError>` — same validation but carries an externally supplied `started_at` (reload preserves original warmup anchor).
- `LoadPolicy::new(thresholds) -> Self` — **retained as `try_new(...).expect(...)` wrapper** for boot-time callers (where config is known-valid via earlier validation); no source break.
- `LoadPolicy::started_at(&self) -> Instant` — accessor added so `apply_config` can preserve the value across reloads.

**Partial-apply semantics (D23)**: If `try_new` fails, the new `streaming_enabled` is still applied (from-boolean-field validation is trivial — it's a boolean); only the `load_policy` update is skipped. The `LiveSnapshot` atomic-swap ensures readers see the new `streaming_enabled` with preserved `load_policy`, or preserved both if unchanged. No torn reads.

### 5.5 `AuditLayer::call` — modified with **header-first grpc-status path** (D28)

**Key changes from rev-1**:

1. Read `RequestId` from extensions (injected by `RequestIdLayer`).
2. After `inner.call(req).await?`, **inspect response initial headers for `grpc-status` FIRST** (trailers-only response path — §6.1 case B).
3. If header-status present: fire oneshot synchronously; wrap body for msg_counter semantics only (no trailer observation needed).
4. If header-status absent: wrap body with `TrailerCapturingBody`; oneshot fires when body emits trailer or is dropped.
5. Spawn deferred audit task: awaits `rx`, maps status, calls `record_completion` with `command_id = Some(request_id)` and `grpc_status_code: Option<u32>` (D26).
6. Return response with wrapped body synchronously.

**Rationale for header-first (D28 / CR1 fix)**:
Verified at `tonic-0.14.5/src/status.rs:605-613` + `server/grpc.rs:20`: when a handler returns `Err(Status)`, tonic constructs a **trailers-only** HTTP response — `grpc-status` lives in **initial headers**, body is empty (`B::default()`), no trailer frame is emitted. Without this header-first path, `TrailerCapturingBody::poll_frame` would observe `Ready(None)` immediately, `Drop` would fire `None`, and `map_code_to_audit_status` would return `Completed` — recording every denied/failed handler-`Err(Status)` return as `Completed`. This is the exact bug G2 aims to fix.

**Crucial**: the deferred task holds captured clones (`bridge`, `metrics`, `ctx`, `operation`, `remote`, `request_id`, `msg_counter`, `start`). It does not borrow from the parent scope.

**Started record** is kept synchronous (before `inner.call`) as before, but `command_id` is now passed as `Some(request_id)` rather than `None`.

**Pseudocode** (elided boilerplate):
```rust
fn call(&mut self, mut req: http::Request<B>) -> Self::Future {
    let mut inner = self.inner.clone();
    let bridge = self.bridge.clone();
    let metrics = self.metrics.clone();

    // RequestIdLayer is outermost (U5); its extension is guaranteed present
    // for any request that reached AuditLayer.
    let request_id = req.extensions().get::<RequestId>().map(|r| r.0.clone());
    let auth_ctx = req.extensions().get::<AuthContext>().cloned();
    let peer = req.extensions().get::<PeerInfo>().cloned();
    let operation = req.uri().path().to_string();

    let msg_counter = Arc::new(AtomicU64::new(0));
    req.extensions_mut().insert(msg_counter.clone());

    Box::pin(async move {
        // Fallthrough: if auth_ctx/peer missing, skip audit (unit-test path).
        // AuthLayer now runs AFTER RequestIdLayer but BEFORE AuditLayer, so
        // in production these extensions are always present here.
        let Some(ctx) = auth_ctx else { return inner.call(req).await; };
        let Some(peer) = peer else { return inner.call(req).await; };
        let remote = peer.remote_addr.to_string();

        // Started — record synchronously before handler.
        let _ = bridge.record(
            &ctx, remote.clone(), &operation,
            "ok", AuditStatus::Started,
            Duration::ZERO, None, None,
            /* failure_reason */ None,
            /* command_id    */ request_id.clone(),
        ).await;

        let start = Instant::now();
        let response = inner.call(req).await?;

        // ── Header-first grpc-status observation (D28) ────────────────────
        // tonic emits "trailers-only" for handler Err(Status) returns:
        // grpc-status in initial headers, empty body, no trailer frame.
        let header_code = response
            .headers()
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i32>().ok())
            .map(tonic::Code::from_i32);

        let (tx, rx) = oneshot::channel::<Option<tonic::Code>>();
        let (parts, body) = response.into_parts();

        let wrapped = if let Some(code) = header_code {
            // Fire immediately — body won't emit trailer for trailers-only.
            let _ = tx.send(Some(code));
            // Still wrap for type-uniformity + msg_counter semantics (counter
            // stays 0 because body is empty; preserved for consistency).
            TrailerCapturingBody::new_already_fired(body, Some(code))
        } else {
            // Streaming or normal-trailers case: observe trailer via body wrap.
            TrailerCapturingBody::new(body, tx)
        };
        let response = http::Response::from_parts(parts, wrapped);

        // Deferred completion record.
        tokio::spawn(async move {
            let observed = rx.await.ok().flatten();
            let audit_status = map_code_to_audit_status(observed);
            let grpc_status_code: Option<u32> = observed.map(|c| c as i32 as u32);
            let duration = start.elapsed();
            let msg_count = msg_counter.load(Ordering::Relaxed);
            let msg_count_opt = (msg_count > 0).then_some(msg_count);

            let _ = bridge.record_completion(
                &ctx, remote, &operation, audit_status,
                duration, msg_count_opt,
                /* failure_reason    */ None,
                /* command_id        */ request_id,
                /* grpc_status_code  */ grpc_status_code,  // NEW per D26
            ).await;

            metrics.request_bump(
                "external",
                ctx.auth_type.as_str(),
                audit_status_label(audit_status),
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

**Unary vs streaming latency** (addresses Arch I4): For unary **Err(Status)** responses (trailers-only), the oneshot fires synchronously inside `call` before `Ok(response)` returns; the deferred task's `rx.await` resolves immediately — no shutdown-race window. For unary **Ok** responses, tonic writes data frame + trailer back-to-back; `TrailerCapturingBody::poll_frame` observes the trailer inline; deferred task resolves on first body poll. Only streaming RPCs have a long tail — consistent with their semantics.

**Type parameter note**: `AuditLayer` currently has generic `<S, B, RespBody>`. The return type becomes `http::Response<TrailerCapturingBody<RespBody>>`. Tonic 0.14's `Server::builder` accepts any service with `Response = http::Response<T> where T: http_body::Body + Send + 'static`. Compile-time assertion added in `trailer_body.rs` tests (§9.1.3):
```rust
const _: fn() = || {
    fn assert_body<T: http_body::Body + Send + 'static>() {}
    assert_body::<TrailerCapturingBody<tonic::body::Body>>();
};
```

**Prometheus cardinality note** (addresses Arch M3): The `request_bump` metric label goes from hardcoded `"ok"` (rev-1) to one of 4 values (`ok`/`denied`/`timeout`/`failed`). This is a bounded-cardinality change (4 labels × `auth_type` labels); dashboards aggregating by raw code rather than label should migrate to the new `grpc_status_code` field. Documented in §8.6.

### 5.6 `ExternalGrpcSpawnConfig` — modified (rev-2)

```rust
pub struct ExternalGrpcSpawnConfig {
    // ... existing fields unchanged (bind_addr, config, storage, system_monitor,
    //                                 event_tx, audit_port, cert_resolver,
    //                                 jwt_verifier, mtls_verifier, ip_ban,
    //                                 metrics, shutdown_rx, shutdown_tx,
    //                                 pii_sanitizer, ai_runtime_status_snapshot) ...

    // REMOVED:
    //   pub streaming_enabled: bool,
    //   pub load_policy: Arc<LoadPolicy>,

    // ADDED:
    pub live: Arc<LiveExternalConfig>,
    // Note: `config_rx` is NOT stored here (D30). The reload task is spawned
    // in build_external_spawn_config and owns its Receiver directly.
}
```

**Debug impl adjustment** (addresses Platform I2 + verify-round I9):
- Drop any `#[derive(Debug)]` and keep the existing **manual** `impl Debug` (already present per spawn_config.rs:82-103).
- Take a **single** `live.snapshot()` for all live-config Debug fields within one Debug print — avoids cross-field torn reads within a single `{:?}` output.
- Replace the rev-1 `.field("streaming_enabled", ...)` with two fields:
  - `.field("streaming_enabled_live", &snap.streaming_enabled)`
  - `.field("load_policy_snapshot_summary", &format_args!("cpu {:.0}/{:.0}/{:.0}, mem_gb {:.1}", snap.load_policy.thresholds().cpu_low_pct, snap.load_policy.thresholds().cpu_medium_pct, snap.load_policy.thresholds().cpu_high_pct, snap.load_policy.thresholds().min_free_mem_gb))`
- Existing redaction (cert/JWT material, bool-presence flags) preserved.
- **Racy across prints**: Debug values reflect the snapshot at print-time; consecutive `{:?}` prints during a reload may show different values. Documented; acceptable for diagnostic output (not a correctness surface).

**Test updates required** (addresses Arch I6):
- `spawn_config_clone_is_shallow` (`spawn_config.rs:242-250`) — add `assert!(Arc::ptr_eq(&cfg.live, &clone.live));` to the existing Arc-ptr-equality chain.
- `spawn_config_debug_redacts_sensitive_fields` (`spawn_config.rs:210-238`) — replace `streaming_enabled` substring check with `streaming_enabled_live`.

### 5.7 `build_external_spawn_config` — modified signature (rev-2)

**src-tauri/src/app_runtime_launch.rs** — gains 1 parameter + 3 constructor blocks:

```rust
async fn build_external_spawn_config(
    cfg: &oneshim_core::config::ExternalGrpcConfig,
    // ... existing 8 params unchanged ...
    config_manager: std::sync::Arc<oneshim_core::config_manager::ConfigManager>,  // NEW
    app_config_snapshot: std::sync::Arc<oneshim_core::config::AppConfig>,          // NEW (for initial values)
) -> anyhow::Result<ExternalGrpcSpawnConfig> {
    // ... existing construction of storage, verifiers, cert_resolver, etc. ...

    // Initial LiveSnapshot from current AppConfig.
    let initial_streaming = cfg
        .streaming_enabled
        .unwrap_or(app_config_snapshot.web.grpc_streaming_enabled);
    let initial_thresholds = app_config_snapshot.web.grpc_load_thresholds.clone().unwrap_or_default();
    // NOTE: boot-time validation — try_new is called here once; panic is acceptable
    //       at boot (config was validated by earlier ConfigManager::update_with).
    //       During ConfigReloadTask operation, try_new's Err is caught and logged.
    let initial_policy = LoadPolicy::try_new(initial_thresholds)
        .context("Invalid LoadThresholds at boot — check config.web.grpc_load_thresholds")?;

    let live = Arc::new(LiveExternalConfig::new(LiveSnapshot {
        streaming_enabled: initial_streaming,
        load_policy: Arc::new(initial_policy),
    }));

    // Spawn reload task (D30 — matches cert_watcher/expiry_monitor pattern).
    let config_rx = config_manager.subscribe();
    tokio::spawn(run_config_reload(
        live.clone(),
        config_rx,
        shutdown_rx.clone(),
    ));

    Ok(ExternalGrpcSpawnConfig {
        // ... existing ...
        live,
    })
}
```

**Call site at `app_runtime_launch.rs:897`** passes the new 2 args: `config_manager.clone()` + the current config snapshot (cloneable `Arc<AppConfig>` from `config_manager.current()`).

---

### 5.8 `StreamingSource` enum — DashboardServiceImpl dual-mode (D24)

**File**: `crates/oneshim-web/src/grpc/streaming_source.rs` (new) — or inlined in `grpc/mod.rs`.

```rust
use std::sync::Arc;
use crate::grpc::load_policy::LoadPolicy;
use crate::grpc::external::live_config::{LiveExternalConfig, LiveSnapshot};

/// Dual-mode source for streaming config fields that handlers read on every call.
///
/// Loopback server uses `Fixed` (values captured once at service-build time).
/// External server uses `Live` (values atomic-swapped by ConfigReloadTask).
#[derive(Clone)]
pub(crate) enum StreamingSource {
    /// Static values — used by loopback `from_spawn_config`.
    Fixed {
        streaming_enabled: bool,
        load_policy: Arc<LoadPolicy>,
    },
    /// Live-reloadable — used by external `from_external_spawn_config`.
    Live(Arc<LiveExternalConfig>),
}

impl StreamingSource {
    pub fn streaming_enabled(&self) -> bool {
        match self {
            Self::Fixed { streaming_enabled, .. } => *streaming_enabled,
            Self::Live(live) => live.snapshot().streaming_enabled,
        }
    }

    pub fn load_policy(&self) -> Arc<LoadPolicy> {
        match self {
            Self::Fixed { load_policy, .. } => load_policy.clone(),
            Self::Live(live) => live.snapshot().load_policy.clone(),
        }
    }
}
```

**DashboardServiceImpl field change** (`grpc/mod.rs`):
```rust
// REMOVED:
// pub(crate) load_policy: Arc<LoadPolicy>,
// pub(crate) streaming_enabled: bool,

// ADDED:
pub(crate) streaming_source: StreamingSource,
```

Both `DashboardServiceImpl::from_spawn_config` (loopback) and `from_external_spawn_config` (external) are updated to construct the appropriate variant:
- loopback: `StreamingSource::Fixed { streaming_enabled: cfg.streaming_enabled, load_policy: cfg.load_policy }`
- external: `StreamingSource::Live(cfg.live.clone())`

Handler call sites (`subscribe_metrics`, `subscribe_events`, etc.) read via `self.streaming_source.streaming_enabled()` / `.load_policy()` — single atomic snapshot load per call (D21 guarantee).

---

### 5.9 `AuditLogPort::entries_by_command_id` — new query surface (D25)

**File**: `crates/oneshim-core/src/ports/audit_log.rs` (modified)

```rust
#[async_trait]
pub trait AuditLogPort: Send + Sync {
    // ... existing methods ...

    /// Return audit entries whose `command_id` exactly matches the given value.
    /// Ordered by `timestamp DESC`. Empty vector if none match.
    ///
    /// # Errors
    /// Infallible (returns empty vec on storage error; error is logged).
    async fn entries_by_command_id(
        &self,
        command_id: &str,
        limit: usize,
    ) -> Vec<AuditEntry>;
}
```

**SqliteStorage impl** (`crates/oneshim-storage/src/sqlite/` — existing audit module):
```rust
async fn entries_by_command_id(&self, command_id: &str, limit: usize) -> Vec<AuditEntry> {
    tokio::task::block_in_place(|| {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, command_id, session_id, action_type, details, status,
                    execution_time_ms, timestamp
             FROM audit_entries
             WHERE command_id = ?1
             ORDER BY timestamp DESC
             LIMIT ?2"
        ).ok()?;
        stmt.query_map(rusqlite::params![command_id, limit as i64], map_row)
            .ok()?
            .filter_map(|r| r.ok())
            .collect()
    }).unwrap_or_default()
}
```

**REST handler** (**new file** `crates/oneshim-web/src/handlers/audit_export.rs` — addresses NV1 from verify Round-2):

Verify Round-2 (NV1) confirmed `GET /api/audit/export` does **not** currently exist — only the integration-specific `/integration/audit` route is registered (`routes.rs`). This spec introduces the endpoint as **net-new**:

```rust
// crates/oneshim-web/src/handlers/audit_export.rs (NEW FILE)
use std::sync::Arc;
use axum::{extract::{State, Query}, Json};
use serde::Deserialize;
use oneshim_core::models::audit::AuditEntry;

#[derive(Deserialize)]
pub struct AuditExportQuery {
    #[serde(default)]
    pub command_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,  // reserved for future filters
    pub limit: Option<usize>,
}

pub async fn export_audit(
    State(state): State<AppState>,
    Query(query): Query<AuditExportQuery>,
) -> Result<Json<Vec<AuditEntry>>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let entries = match &query.command_id {
        Some(cmd_id) if !cmd_id.is_empty() => {
            state.audit_port.entries_by_command_id(cmd_id, limit).await
        }
        _ => state.audit_port.recent_entries(limit).await,
    };
    Ok(Json(entries))
}
```

**Route registration** (`routes.rs`, adds 1 line):
```rust
.route("/api/audit/export", get(export_audit))
```

**DoS cap**: `limit.min(1000)` clamps response size at ~2MB (1000 rows × ~2KB JSON). Agent is loopback-only; cap is adequate.

**`NoopAudit`** test helper (in `spawn_config.rs` + `audit_layer.rs` tests) gains:
```rust
async fn entries_by_command_id(&self, _id: &str, _limit: usize) -> Vec<AuditEntry> {
    vec![]
}
```

**OpenAPI contract update** (per `docs/contracts/oneshim-web.v1.openapi.yaml`): new path `GET /api/audit/export` with `command_id` + `status` + `limit` query params, response `application/json: Vec<AuditEntry>`. Add to plan-phase tasks.

**Serde backward-compat** (OQ15 resolution): `#[serde(default)]` on `AuditExportQuery` fields + `#[serde(default)]` on `ExternalGrpcAuditDetails` struct ensure old audit rows (without `grpc_status_code`) deserialize as `None`.

---

### 5.10 `LoadPolicy` additions — `try_new` + `started_at` preservation (D23, D27)

**File**: `crates/oneshim-web/src/grpc/load_policy.rs` (modified)

```rust
#[derive(Debug, thiserror::Error)]
pub enum LoadPolicyError {
    #[error("invalid LoadThresholds: {reason}")]
    InvalidThresholds { reason: String },
}

impl LoadPolicy {
    /// Fallible constructor — validates threshold ordering.
    pub fn try_new(thresholds: LoadThresholds) -> Result<Self, LoadPolicyError> {
        Self::try_new_with_started_at(thresholds, Instant::now())
    }

    /// Fallible constructor with externally supplied `started_at`.
    /// Used by `ConfigReloadTask` to preserve the original warmup anchor
    /// across reloads (otherwise every reload forces a fresh 30s `Medium`).
    pub fn try_new_with_started_at(
        thresholds: LoadThresholds,
        started_at: Instant,
    ) -> Result<Self, LoadPolicyError> {
        if !(thresholds.cpu_low_pct < thresholds.cpu_medium_pct) {
            return Err(LoadPolicyError::InvalidThresholds {
                reason: format!(
                    "cpu_low_pct ({}) must be < cpu_medium_pct ({})",
                    thresholds.cpu_low_pct, thresholds.cpu_medium_pct
                ),
            });
        }
        if !(thresholds.cpu_medium_pct < thresholds.cpu_high_pct) {
            return Err(LoadPolicyError::InvalidThresholds {
                reason: format!(
                    "cpu_medium_pct ({}) must be < cpu_high_pct ({})",
                    thresholds.cpu_medium_pct, thresholds.cpu_high_pct
                ),
            });
        }
        if !(thresholds.cpu_high_pct <= 100.0) {
            return Err(LoadPolicyError::InvalidThresholds {
                reason: format!(
                    "cpu_high_pct ({}) must be <= 100.0",
                    thresholds.cpu_high_pct
                ),
            });
        }
        Ok(Self { thresholds, started_at })
    }

    /// Read accessor — needed by ConfigReloadTask for `started_at` preservation.
    pub fn started_at(&self) -> Instant {
        self.started_at
    }

    /// Backward-compat — existing `pub fn new(thresholds) -> Self` retained as:
    pub fn new(thresholds: LoadThresholds) -> Self {
        Self::try_new(thresholds).expect(
            "LoadPolicy::new: thresholds must be validated before construction; \
             use try_new for runtime-fallible construction"
        )
    }
}
```

`LoadPolicy::new` remains the boot-time entry point (config has already been validated at that stage; expect is appropriate). `try_new` / `try_new_with_started_at` are the reload-time entry points.

---

### 5.11 Live-config REST endpoint (D29)

**File**: `crates/oneshim-web/src/handlers/external_grpc_live_config.rs` (new)

```rust
use std::sync::Arc;
use axum::{extract::State, Json};
use serde::Serialize;
use crate::grpc::external::live_config::LiveExternalConfig;

#[derive(Serialize)]
pub struct LoadPolicyView {
    pub cpu_low_pct: f32,
    pub cpu_medium_pct: f32,
    pub cpu_high_pct: f32,
    pub min_free_mem_gb: f32,
    /// Milliseconds since this LoadPolicy's warmup anchor (monotonic).
    /// Operators compute wall-clock origin as `now - started_at_elapsed_ms`.
    /// Monotonic-clock avoids SystemTime drift/suspend-resume weirdness.
    pub started_at_elapsed_ms: u64,
    pub in_warmup: bool,
}

/// Task liveness surfaced to operators per D32 — addresses NV2 (silent
/// ConfigReloadTask panic is invisible otherwise).
#[derive(Serialize)]
pub struct LiveConfigResponse {
    pub streaming_enabled: bool,
    pub load_policy_snapshot: LoadPolicyView,
    pub config_reload_task_alive: bool,
}

pub async fn get_live_config(
    State(state): State<AppState>,
) -> Result<Json<LiveConfigResponse>, ApiError> {
    // Only available when external gRPC is enabled; return 503 otherwise.
    let Some(live) = &state.external_grpc_live else {
        return Err(ApiError::service_unavailable("external gRPC not enabled"));
    };
    let snap = live.snapshot();
    let policy = &snap.load_policy;
    // Decision I10 resolution: use monotonic elapsed rather than Unix epoch ms.
    // `Instant::elapsed()` is infallible; avoids `SystemTime` wall-clock hazards.
    let started_at_elapsed_ms = policy.started_at().elapsed().as_millis() as u64;
    Ok(Json(LiveConfigResponse {
        streaming_enabled: snap.streaming_enabled,
        load_policy_snapshot: LoadPolicyView {
            cpu_low_pct: policy.thresholds().cpu_low_pct,
            cpu_medium_pct: policy.thresholds().cpu_medium_pct,
            cpu_high_pct: policy.thresholds().cpu_high_pct,
            min_free_mem_gb: policy.thresholds().min_free_mem_gb,
            started_at_elapsed_ms,
            in_warmup: policy.is_in_warmup(),
        },
        // NV2 fix: surface config_reload_task_alive for operators.
        config_reload_task_alive: state.external_grpc_metrics
            .as_ref()
            .map(|m| m.config_reload_task_alive.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false),
    }))
}
```

**Route registration** (`crates/oneshim-web/src/routes.rs`):
```rust
.route("/api/external-grpc/live-config", get(get_live_config))
```

**AppState wiring**: `AppState` gains `external_grpc_live: Option<Arc<LiveExternalConfig>>` populated in `build_external_spawn_config` (stored alongside the spawn_config). `None` when external gRPC is disabled → handler returns 503.

**Test expectations** (§9.2 new):
- `live_config_endpoint_returns_current_snapshot` — integration: toggle config, call endpoint, verify response reflects new values
- `live_config_endpoint_503_when_external_disabled` — unit: bare AppState without external, call endpoint, expect 503

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

### 7.1 Consumed config fields (revised — rev-2)

| AppConfig path | Type | Default | Consumed by | Notes |
|---------------|------|---------|-------------|-------|
| **`external_grpc.streaming_enabled`** (**NEW**, D22) | `Option<bool>` | `None` → fall back to `web.grpc_streaming_enabled` | `LiveSnapshot.streaming_enabled` | External override so operators can disable external-only streaming without affecting loopback |
| `web.grpc_streaming_enabled` (existing) | `bool` | `true` | Loopback `DashboardServiceImpl` (NG1 — unchanged), **fallback** for external when `external_grpc.streaming_enabled: None` | Shared between loopback and external |
| `web.grpc_load_thresholds` | `Option<LoadThresholds>` | `None` → `LoadThresholds::default()` | `LiveSnapshot.load_policy` (via `LoadPolicy::try_new_with_started_at`) | Same field reused for external live-reload |

**One new field** (`external_grpc.streaming_enabled`). Schema migration: **none** — field is `Option<bool>` with `None` default; backward compatible (existing configs continue to work, defaulting to the shared fallback).

**User visibility**:
- Operator who doesn't touch `external_grpc.streaming_enabled` sees no change (shared field behavior preserved).
- Operator who sets `external_grpc.streaming_enabled = false` during an incident → external server disables streaming on next reload (~10ms) while loopback keeps running.
- Operator who edits `web.grpc_load_thresholds` → external server picks up new thresholds on next reload; warmup anchor preserved (D27 — no forced 30s Medium).

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

**In-scope metrics** (promoted from rev-1 "optional/deferred" per D32 / verify-round NV3):
- `external_grpc_config_reload_total: AtomicU64` — bumped on each successful `apply_config`
- `external_grpc_deferred_audit_in_flight: AtomicUsize` — gauge, increment on `tokio::spawn` in `AuditLayer::call`, decrement at task end
- `external_grpc_config_reload_task_alive: AtomicBool` — set true at task start, false on clean exit (panic leaves unchanged → observability for silent death)

These are surfaced via `ExternalMetrics` struct. Wire-level exposure (Prometheus/etc.) reuses existing telemetry plumbing.

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

Each entry is tagged **NEW** / **REPLACE** (existing test body fully rewritten) / **EXTEND** (existing asserts kept + new asserts added).

Request-ID behavior:
- **REPLACE** `external_grpc_request_id_header_returned` (line 933 in current file) — currently TODO-stub; rewrite to send incoming "req-xyz-123", assert response header = "req-xyz-123"
- **NEW** `external_grpc_request_id_generated_when_missing` — no incoming header; assert response header matches UUIDv4 regex `[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}`
- **NEW** `external_grpc_request_id_invalid_replaced` — send malformed ID (e.g., `"bad\x01id"`); assert response contains a UUID (and not the malformed input)
- **NEW** `external_grpc_request_id_preserved_across_auth_reject` — send incoming "req-abc-123" + invalid JWT; assert audit `command_id = "req-abc-123"` (validates U5/D14 — correlation preserved at security boundary)

Audit status mapping (D28 header-first verified):
- **EXTEND** `external_grpc_audit_completed_entry_written_after_ok_response` (line 1531) — existing asserts kept; additionally assert `command_id` matches incoming/generated request ID and `grpc_status_code = 0` in details JSON
- **NEW** `external_grpc_audit_denied_when_handler_returns_permission_denied` — test-fixture handler returns `Err(Status::permission_denied("..."))`; assert audit row has `status = Denied`, `grpc_status_code = 7`. This is the primary CR1-regression-catch test.
- **NEW** `external_grpc_audit_timeout_when_handler_returns_cancelled` — handler returns `Err(Status::cancelled("..."))`; assert `status = Timeout`, `grpc_status_code = 1`. Deterministic via trailers-only header path (Option B from closed OQ6). **Test uses a 0-message-fixture handler** (no data frames emitted) so the `Err` produces a trailers-only HTTP response exercising the §5.5 header-first branch (addresses NV6 — streaming-RPC test mechanism clarity).
- **NEW** `external_grpc_audit_failed_when_handler_returns_internal` — handler returns `Err(Status::internal("..."))`; assert `status = Failed`, `grpc_status_code = 13`.
- **NEW** `external_grpc_audit_completed_when_client_drops_before_trailer` — client drops stream mid-flight (realistic drop); assert audit = `Completed` with msg_count > 0. Documents the OQ6-Option-A fallback behavior.
- **EXTEND** `external_grpc_streaming_audit_records_message_count` (line 1594) — existing asserts kept; additionally verify msg_count correlation with status mapping.

Audit query surface (D25):
- **NEW** `audit_entries_by_command_id_returns_matching_rows` — insert 3 audit entries with same command_id + 2 with different; call `AuditLogPort::entries_by_command_id(cmd_id, 10)`; assert exactly 3 returned, newest first
- **NEW** `audit_export_rest_endpoint_filters_by_command_id` — call `GET /api/audit/export?command_id=X`; assert only matching rows returned

Live config reload:
- **NEW** `external_grpc_live_streaming_toggle_reflects_within_1s` — spawn server, reload config with `external_grpc.streaming_enabled = false`, next SubscribeMetrics returns `Unavailable`; assert `start.elapsed() < Duration::from_secs(1)` (CI convergence bound per D33)
- **NEW** `external_grpc_live_load_thresholds_applied_without_warmup_reset` — spawn server, wait 30s (out of warmup), reload with new thresholds, assert NOT in warmup on next request (D27 started_at preserved)
- **NEW** `external_grpc_live_reload_rejects_malformed_thresholds_and_continues` — reload with invalid thresholds (e.g., `cpu_low = 90, cpu_medium = 50`); assert ConfigReloadTask still running, `streaming_enabled` change (if any) still applied, `load_policy` unchanged
- **NEW** `external_grpc_live_reload_coalesces_rapid_updates` — fire 100 config updates in rapid succession; assert final `live.snapshot()` matches last update; task not panicked
- **NEW** `live_reload_affects_long_running_stream` — open SubscribeMetrics, reload with threshold-changing-to-shed config mid-stream, verify shed behavior on subsequent decisions
- **NEW** `external_grpc_config_reload_task_exits_on_shutdown` — spawn, trigger shutdown, assert task joins within 5s timeout

Live config inspection endpoint (D29):
- **NEW** `live_config_endpoint_returns_current_snapshot` — call `GET /api/external-grpc/live-config`, assert JSON response matches expected shape with current values
- **NEW** `live_config_endpoint_503_when_external_disabled` — call endpoint without external enabled; assert 503

Fallback field semantics (D22):
- **NEW** `loopback_streaming_enabled_is_not_live_reloaded` — toggle `external_grpc.streaming_enabled = false`; assert external is disabled AND loopback is still enabled (preserves NG1)
- **NEW** `external_streaming_falls_back_to_web_field_when_external_none` — set `external_grpc.streaming_enabled = None`; set `web.grpc_streaming_enabled = false`; external server honors false
- **NEW** `external_streaming_override_wins_over_web_field_when_some` — set `external_grpc.streaming_enabled = Some(true)`; set `web.grpc_streaming_enabled = false`; external server honors true (addresses NV4 — override-beats-parent semantics)

Expected count: **~18 new integration tests** (replacing/extending 2 existing) added to current 19 = total ~37.

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

### 9.5 Test organization (revised — rev-2)

| Crate:module | Test count (new) | Run command |
|-------------|------------------|-------------|
| `oneshim-web::grpc::external::live_config::tests` | 4 | `cargo test -p oneshim-web --features ... --lib live_config` |
| `oneshim-web::grpc::external::request_id_layer::tests` | 9 (+1 for conditional-overwrite per D31) | `--lib request_id_layer` |
| `oneshim-web::grpc::external::trailer_body::tests` | 10 (+1 for `new_already_fired` ctor per D28) | `--lib trailer_body` |
| `oneshim-web::grpc::external::config_reload::tests` | 7 (+2 for try_new rejection + started_at preservation) | `--lib config_reload` |
| `oneshim-web::grpc::external::audit_layer::tests` (augmented) | +9 (+2 for header-first grpc-status + grpc_status_code field) | `--lib audit_layer` |
| `oneshim-web::grpc::streaming_source::tests` (new) | 3 | `--lib streaming_source` |
| `oneshim-web::grpc::load_policy::tests` (augmented) | +4 (try_new success/error variants, started_at preservation) | `--lib load_policy` |
| `oneshim-core::ports::audit_log` / `oneshim-storage::sqlite` contract tests | +3 (entries_by_command_id) | `--lib` |
| `oneshim-web` integration | +18 (D25 + D28 + D29 + D22 + D27 tests, replace 2 existing) | `--test external_grpc_integration` |
| **Total** | **~66 test additions** (18 integration + 48 unit/contract) | — |

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
| ~~`external_grpc_config_reload_total` metric~~ | ~~Monitoring polish~~ | **Now in-scope per D32 / §8.6** (resolved, not deferred) |
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
| D14 (**revised**) | Layer ordering: `.layer(request_id).layer(auth).layer(audit)` — **RequestIdLayer outermost** | U5 synthesis: auth-rejected audit rows correlate with client's `x-request-id`. ~30ns UUID cost on unauth paths is negligible. Rev-1 had auth outermost — replaced. |
| D15 | No feature flag | Strict improvement + additive; no rollback concern |
| D16 | `command_id` in audit rows is now `Option<String>` with `Some(request_id)` for every request (including auth-rejected, per U5) | Reuses existing `command_id` field; no schema change |
| D17 | New files use `pub(crate)` visibility by default | ADR-001 §5 + workspace convention (iter-7 polish pass) |
| D18 | No new tokio runtime requirement | All new tasks run on the existing main runtime |
| D19 (**revised**) | Use `pin_project_lite` directly — already transitive in workspace | Verified: tokio/tower/hyper/http-body-util all depend on `pin-project-lite v0.2.17`. OQ2 closed. Zero new dependency. |
| D20 | Span-level tracing unchanged; add `request_id` structured field to key log events | Minimal observability improvement inside existing tracing setup |
| **D21** (new) | `LiveExternalConfig` uses a **single** `ArcSwap<LiveSnapshot>`, not dual `AtomicBool` + `ArcSwap<LoadPolicy>` | CR-arch3 / I1-platform: eliminates cross-field torn reads. Readers load whole snapshot once per request-entry. Writers atomic-swap the whole struct. |
| **D22** (new) | Add `ExternalGrpcConfig.streaming_enabled: Option<bool>` (overrides shared `web.grpc_streaming_enabled` when `Some`) | U1 / CR2-platform: loopback server unaffected by external-only live reload. Backward compat: `None` → legacy fall-through to `web.grpc_streaming_enabled`. |
| **D23** (new) | `LoadPolicy::try_new(thresholds) -> Result<Self, LoadPolicyError>` — fallible constructor. `LoadPolicy::new` retained as `try_new(...).expect(...)`. `apply_config` uses `try_new`; on `Err` logs error + keeps previous `load_policy` (partial apply, `streaming_enabled` still updates). | CR3 / CR-arch2: eliminates ConfigReloadTask panic path. D21 atomic store ensures readers see consistent partial-apply state. |
| **D24** (new) | `DashboardServiceImpl` dual-mode via `enum StreamingSource { Fixed(bool, Arc<LoadPolicy>), Live(Arc<LiveExternalConfig>) }` | CR4: loopback `from_spawn_config` constructs `Fixed`; external `from_external_spawn_config` constructs `Live`. Handlers call `self.streaming_source.streaming_enabled()` / `.load_policy()`. Avoids sibling-struct duplication. |
| **D25** (new) | Add `AuditLogPort::entries_by_command_id(cmd_id: &str, limit: usize) -> Vec<AuditEntry>` + SqliteStorage impl + REST `GET /api/audit/export?command_id=X` query param | CR5: operator correlation "<1s lookup" needs a first-class query surface, not raw sqlite3. +60 LoC port+impl, +3 integration tests. |
| **D26** (new) | `ExternalGrpcAuditDetails` gains `grpc_status_code: Option<u32>` field (`#[serde(skip_serializing_if = "Option::is_none")]`) | CR7: `Unauthenticated`/`PermissionDenied` conflation into `Denied` bucket is acceptable for audit-status granularity, but raw code enables security dashboards to disambiguate at query time. |
| **D27** (new) | `LoadPolicy::started_at()` accessor + `LoadPolicy::try_new_with_started_at(thresholds, started_at)` constructor. `apply_config` preserves `started_at` across reloads. | U4 / Arch-Q1: prevents 30s warmup reset on each reload — operators tuning thresholds during incident get immediate effect, not 30s of forced `Medium`. |
| **D28** (new) | `AuditLayer::call` inspects `response.headers().get("grpc-status")` BEFORE body wrap. If present → trailers-only path → fire oneshot synchronously with parsed code. Else → wrap body with `TrailerCapturingBody` as before. | CR1: handler `Err(Status)` returns go through trailers-only HTTP response (tonic `Status::into_http`); without header-first observation, all Denied/Failed/Timeout handler returns would audit as `Completed`. |
| **D29** (new) | Add REST endpoint `GET /api/external-grpc/live-config` returning `{ streaming_enabled: bool, load_policy_snapshot: LoadPolicyView }` | I2-product: operators verifying a reload took effect need an inspection endpoint, not just log grep. ~40 LoC handler + 2 tests. |
| **D30** (new) | `ConfigReloadTask` spawned inside `build_external_spawn_config`, NOT inside `serve_external` | Arch-I2: matches cert-watcher/expiry-monitor pattern; supervisor respawn of `serve_external` does not duplicate-spawn the reload task. |
| **D31** (new) | `RequestIdLayer` does **conditional overwrite** of `x-request-id` response header: if handler-set value matches validated value, leave alone; otherwise insert ours | Arch-I5: preserves rare proxy-forward patterns without breaking correlation for the 99% case. Replaces rev-1's unconditional overwrite. |
| **D32** (new) | `ExternalMetrics.deferred_audit_in_flight: AtomicUsize` gauge + `config_reload_total: AtomicU64` counter + `config_reload_task_alive: AtomicBool` | Platform-I3 + Q3: observability for unbounded per-request spawn + reload task liveness. Promoted from "optional deferred" to in-scope. |
| **D33** (new) | G3 SLO revised: "≤1s convergence at CI (tested), typically <10ms in production"; remove ≤5s | Platform-I4 + Product-I1: ≤5s was hand-wavy and inconsistent with synchronous `watch::send_replace`. CI test asserts ≤1s via `start.elapsed()` bound. |

---

## 13. Open Questions — Status after rev-2 (Loop 1 Round 1 synthesis applied)

- **OQ1** — **RESOLVED (D23)**: `LoadPolicy::new` verified to panic via `assert!` at `load_policy.rs:42-58`. `LoadPolicy::try_new` introduced; `apply_config` catches `Err` and preserves previous `load_policy`. See §5.4 + §5.10.
- **OQ2** — **RESOLVED (D19 revised)**: `pin_project_lite` confirmed transitive in workspace (via tokio/tower/hyper/http-body-util). Use directly, no new dependency.
- **OQ3** — **DEFERRED (non-blocking)**: Audit bridge impls are expected to be panic-safe by contract (AuditLogPort docstring). No `catch_unwind` wrapper needed around deferred task. If a real panic scenario surfaces in production, a `tokio::task::JoinHandle::catch_unwind` wrapper can be added without spec change.
- **OQ4** — **RESOLVED**: validate raw, no trim. See §5.2.
- **OQ5** — **RESOLVED (D20)**: tracing events for `record` and `record_completion` include structured field `request_id = %command_id` at `info!` level. Implementation plan makes this concrete in each module.
- **OQ6** — **RESOLVED (via D28 header-first + split tests)**: the `timeout_for_cancelled_stream` test is split into two per §9.2:
  - `external_grpc_audit_timeout_when_handler_returns_cancelled` (Option B — deterministic via trailers-only header path)
  - `external_grpc_audit_completed_when_client_drops_before_trailer` (Option A — documents fallback behavior)
  Unit-level test on `TrailerCapturingBody` with hand-crafted trailer also covers streaming DeadlineExceeded via the trailer path.
- **OQ7** — **RESOLVED**: `config_rx` is no longer stored on `ExternalGrpcSpawnConfig` (D30); the reload task receives the Receiver directly at spawn time in `build_external_spawn_config`. No Clone cascade concern.
- **OQ8** — **DEFERRED (instrumented)**: memory pressure from per-request oneshot allocations remains bounded by `max_concurrent_streams` + `max_connections`. `ExternalMetrics.deferred_audit_in_flight` gauge (D32) gives observability to quantify at production load.
- **OQ9** — **DEFERRED (D26 addresses)**: `map_code_to_audit_status` still coalesces `Internal`/`Unknown`/`DataLoss` to `Failed`. Raw `grpc_status_code: u32` in details JSON (D26) allows query-time re-splitting — no schema change or new `AuditStatus` variant needed.
- **OQ10** — **RESOLVED**: `config_rx.borrow_and_update()` returns `watch::Ref<'_, Arc<AppConfig>>`; the borrow is dropped at the end of `apply_config(&live, &config_rx.borrow_and_update());` statement (scope ends at `;`). No await held across; no deadlock. Documented in §5.4 code comment.

### New open questions surfaced in Loop 1 Round 1 (not yet classified as C/I)

- **OQ11** (rev-2 from review): `ExternalGrpcSpawnConfig` is `pub` and used directly by integration tests under `#[cfg(feature = "test-support")]`. Refactoring it is a ripple through those tests. Implementation plan must call this out in task ordering.
- **OQ12** (rev-2): `external_grpc.streaming_enabled: Option<bool>` serialization: confirm `#[serde(default, skip_serializing_if = "Option::is_none")]` attrs so existing config files don't grow a `"streaming_enabled": null` line on re-save. Minor config-hygiene detail.
- **OQ13** (rev-2 from Platform Q1): Should handler-panic case record `AuditStatus::Failed` with a distinct `failure_reason = "handler_panic"` marker separate from normal `Err(Status::internal)` returns? Existing `AuditBridge::record_completion` signature supports `failure_reason: Option<&str>` but spec §5.5 does not populate it. Follow-up: extend the deferred task to distinguish `Err(e)` from panic (currently caught via `?` — a handler panic would propagate to tonic's panic handler, bypassing our body-wrap). Deferred non-blocking.

These 3 new open questions are all flagged for Round-2 verify reviewers.

---

## 14. Success Criteria (Loop 3 exit)

- [ ] All 42 new tests green (0 fail, 0 ignore)
- [ ] `cargo check` + `cargo clippy -- -D warnings` + `cargo fmt --check` clean for:
  - `oneshim-web` with features `grpc-dashboard-external,external-grpc-tools,test-support`
  - `oneshim-app` with features `external-grpc-tools`
- [ ] `cargo test --workspace` zero regression (baseline: current main)
- [ ] Memory `reference_tonic_layer_order.md` updated if this PR introduces a new layer
- [ ] Docs: `docs/guides/external-grpc.md` + `.ko.md` updated (addresses Product-I6):
  - Auditing section **accurately** describes per-request status mapping (Completed/Denied/Timeout/Failed). **Existing aspirational text at line 171** (claiming `external_grpc_denied`/`external_grpc_timeout` were emitted) was a lie pre-this-PR — must be rewritten as a correctness fix, not just an addition.
  - `x-request-id` request/response header documented with validation rules + example
  - Live-reload section added with full watched-fields table (mirrors §7.1)
  - Live-config inspection endpoint (`GET /api/external-grpc/live-config`) documented per D29
  - Audit query surface (`GET /api/audit/export?command_id=X`) documented per D25
  - Korean companion doc synced section-for-section per `docs/DOCUMENTATION_POLICY.md`
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
