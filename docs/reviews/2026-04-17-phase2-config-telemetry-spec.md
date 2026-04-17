# Phase 2 — Config Change Bus + Telemetry Exporter Wiring

_Date_: 2026-04-17
_Scope_: `client-rust` repository. Two cross-domain items from `docs/reviews/2026-04-16-feature-gaps-analysis.md`:
- **X1** ConfigChangeBus — broadcast runtime config changes to every subscriber
- **X2** Telemetry exporter wiring — connect `TelemetryConfig` to an OTLP exporter, gated by feature flag and runtime opt-in.

_Non-goals_: server-side observability (already wired), replacing every scheduler loop that polls config (only migrate what benefits), bolt-on metrics crates (we use `tracing` and OpenTelemetry exclusively).

_Delivery_: single doc, phased delivery. X1 ships first because X2 depends on X1 to react to runtime toggles of `telemetry.enabled`.

---

## 1. Motivation

### 1.1 Problem — X1

`ConfigManager` (`crates/oneshim-core/src/config_manager.rs`) owns `Arc<RwLock<AppConfig>>`. Mutation goes through three APIs:

| API | Effect |
|-----|--------|
| `update(AppConfig)` | Replace whole config + persist to disk |
| `update_with(FnOnce(&mut AppConfig))` | Read-modify-write under write lock + persist |
| `reload()` | Re-read from disk + replace in-memory snapshot |

All three mutate silently. Consumers must poll `get()` to detect change. Today, the scheduler loops (`intelligence`, `events`, `monitor`, `sync`, `system`, plus a few helpers in the same directory) do exactly that:

```rust
let current_config = config_manager.as_ref().map(|cm| cm.get()).unwrap_or_default();
```

Consequences:
- **Latency**: a toggle in the settings UI only takes effect on the next tick of each loop (1–30 s depending on loop).
- **Cached-section drift**: `oneshim-vision::privacy` and `oneshim-analysis::regime_manager` copy sub-sections into their own state at init; changes never reach them.
- **No "react to a specific field" primitive**: every consumer reimplements dirty-check against a remembered snapshot.

### 1.2 Problem — X2

`TelemetryConfig` (`crates/oneshim-core/src/config/sections/storage.rs:59`) is a struct with four bool fields (`enabled`, `crash_reports`, `usage_analytics`, `performance_metrics`). It has zero consumers. The `tracing_subscriber` stack initialised in `src-tauri/src/main.rs:114-147` never reads it.

The server-side already terminates OTLP (OTel Collector → Tempo/Prometheus/Loki; see top-level `CLAUDE.md` production-infra section). The missing piece is a client-side exporter.

### 1.3 Why now

- Phase 1 quick wins (#425/#426/#427) are merged. The plumbing floor is stable enough for wide-reaching additive work.
- `C1/C2/C3` in the feature-gap doc all benefit from a bus being in place first (each has its own per-loop reactions to config).
- Observability blind-spots in the Rust client are becoming painful during release triage — we can see what the *server* did but not what the *client* tried to send.

---

## 2. Design — X1 (ConfigChangeBus)

### 2.1 Mechanism choice

**Chosen**: `tokio::sync::watch::channel<Arc<AppConfig>>` embedded in `ConfigManager`.

Why:
- **Latest-wins semantics match configuration** — a subscriber that wakes late should see the current config, not a historical replay.
- **No queue bound to pick** — `broadcast` requires a capacity and drops older values on overflow, forcing subscribers to handle `Lagged`.
- **Subscribers cheap to add/drop** — each `subscribe()` just clones a `Receiver`.
- **Sender owns the channel** — stored inside `ConfigManager`; subscriber channel never outlives the manager.

`Arc<AppConfig>` (not raw `AppConfig`) avoids cloning the ~900-line config tree on every write and lets receivers share structure.

### 2.2 Alternatives considered

| Option | Verdict | Reason |
|--------|---------|--------|
| `tokio::sync::broadcast` | Rejected | Buffered multi-producer; we have a single writer (`ConfigManager`). `Lagged` errors leak complexity into every consumer. |
| Per-section channel (one `watch` per section) | Rejected | Explodes API surface. `AppConfig` has 16 top-level sections. If consumers need sub-section change detection, they cheap-compare the field they care about (see §2.5). |
| Event bus crate (`tokio::sync::mpsc` + router) | Rejected | Over-engineered for a latest-wins broadcast. Adds a task and a queue for no additional capability. |
| `arc-swap::ArcSwap<AppConfig>` + polling | Rejected | Avoids lock contention but gives no wake-up — consumers still poll. Doesn't solve X1's reactivity requirement. |

### 2.3 Public API additions to `ConfigManager`

```rust
impl ConfigManager {
    /// Subscribe to whole-config change notifications.
    ///
    /// The initial value is the current config. `changed().await` returns
    /// after the next `update`/`update_with`/`reload`. Dropping the receiver
    /// does not affect other subscribers.
    pub fn subscribe(&self) -> watch::Receiver<Arc<AppConfig>>;

    /// Returns a cheap pointer-equality snapshot. Equivalent to
    /// `subscribe().borrow().clone()` without registering a subscriber.
    pub fn snapshot(&self) -> Arc<AppConfig>;
}
```

No existing method changes signature. `get() -> AppConfig` stays, implemented as `(*self.snapshot()).clone()`.

### 2.4 Internal wiring

```rust
pub struct ConfigManager {
    // Source of truth for current config. `sender.borrow()` gives cheap read access.
    sender: watch::Sender<Arc<AppConfig>>,
    // Serialises concurrent writers (update/update_with/reload) so their
    // read-modify-write sequences don't interleave. Held briefly, never across await.
    writer_lock: parking_lot::Mutex<()>,
    config_path: PathBuf,
}
```

The previous `Arc<RwLock<AppConfig>>` goes away — `watch::Sender` owns the current value and exposes it via `borrow()`. The only reason we still keep a lock is to linearise writers across the (non-atomic) compute-new-value → persist → send sequence.

Every mutation path (`update`, `update_with`, `reload`) performs:
1. Acquire `writer_lock` (drops at function exit).
2. Read current `Arc<AppConfig>` via `sender.borrow().clone()`.
3. Compute new `AppConfig` (applying the updater or loading from disk).
4. Persist to disk (for `update`/`update_with`; `reload` already read from disk in step 3).
5. `sender.send_replace(Arc::new(new))` — returns the previous value; ignored. Broadcast happens atomically inside `watch`.

Readers (via `subscribe()` or `snapshot()`) never block on `writer_lock`; they only interact with `watch`'s internal synchronisation. Two concurrent writers serialise via `writer_lock` and therefore each observes a consistent snapshot before persisting.

### 2.5 Consumer pattern

The recommended idiom for a loop that reacts to config changes:

```rust
let mut rx = config_manager.subscribe();
let mut prev_section: SectionConfig = rx.borrow_and_update().section.clone();
loop {
    tokio::select! {
        _ = rx.changed() => {
            let new_section = rx.borrow_and_update().section.clone();
            if new_section != prev_section {
                apply(&new_section);
                prev_section = new_section;
            }
        }
        _ = interval.tick() => { /* existing work */ }
    }
}
```

For loops that only need "read latest on next tick" behaviour, `config_manager.snapshot()` replaces `config_manager.get()` at zero migration cost.

### 2.6 Migration policy (what consumers convert now)

- **Only migrate where it actually reduces latency or fixes a bug.** Config polling is cheap; mechanical migrations for their own sake are busywork.
- **Required in this phase**: the telemetry bootstrapper (X2) must use `subscribe` because toggling `telemetry.enabled` at runtime is the feature.
- **Not in this phase** — even as a demonstrator. The obvious candidate was `src-tauri/src/scheduler/loops/monitor.rs::prev_pii_level`, but review found it is co-updated atomically with `prev_full_text_consent` inside `helpers.rs::audit_consent_and_pii_changes` and emits an audit-log entry on every transition. `watch` coalesces rapid updates (latest-wins), so a subscribe-and-diff rewrite would silently drop intermediate audit transitions — a regression in a compliance-relevant path. The other scheduler loops identified so far have the same shape (on-tick diff + audit/side-effect per transition), so there is no zero-risk demonstrator in this phase.
- **Out of scope**: converting scheduler loops at large. Each conversion is its own Phase 3 line item, and the audit-coalescing concern is explicit in the review checklist.
- **Docs**: `ADR-016-config-change-bus.md` (see §5) records the audit-coalescing hazard and the "diff in consumer, do not assume every update fires" guidance.

### 2.7 Error & lifecycle semantics

- `watch::Sender::send` returns `Err` only when all receivers are dropped. We ignore it (fire-and-forget) and log at `trace` if desired for debugging.
- `ConfigManager` owns the sender; the sender lives as long as the manager. The manager is stored in Tauri-managed state, so its lifetime matches the process.
- No explicit shutdown for the bus. Receivers drop when their owning tasks end.
- `subscribe()` is safe to call from any thread; `watch::Receiver` is `Send + Sync + Clone`.

### 2.8 Testing (X1)

Each test lives alongside `config_manager.rs` in a `#[cfg(test)]` module.

| # | Test | Asserts |
|---|------|---------|
| T-X1-1 | `subscribe_sees_initial_value` | `borrow()` on fresh receiver equals the persisted config. |
| T-X1-2 | `update_notifies_subscribers` | After `update()`, `changed().await` returns and the new value is visible. |
| T-X1-3 | `update_with_notifies_subscribers` | Same as above, via `update_with`. |
| T-X1-4 | `reload_notifies_subscribers` | After the file is rewritten on disk and `reload()` is called, subscribers see the new value. |
| T-X1-5 | `dropped_receiver_does_not_block_sender` | Drop one of two receivers mid-test; subsequent `update()` still notifies the survivor. |
| T-X1-6 | `snapshot_matches_latest_update` | `snapshot()` returns the most recent `Arc` pointer-equal to what `subscribe().borrow()` returns. |
| T-X1-7 | `no_spurious_wakeup_when_content_identical` | Two `update()` calls with equal configs still each fire a notification. **Doc-only test** that pins `watch` semantics for future readers — consumers must diff themselves (audit-coalescing hazard — see §2.6). |
| T-X1-8 | `deserialises_legacy_config_json_without_new_telemetry_fields` | A JSON payload missing `otlp_endpoint`/`sample_rate`/`service_name` parses, and `get()` returns defaults for those fields. Protects serde-defaults contract for existing users on first boot after upgrade. |
| T-X1-9 | `update_with_does_not_reenter` | Calling `get()` or `snapshot()` from inside a running `update_with` closure returns a usable (pre-swap) snapshot without deadlock. Pins the "writer_lock is not held across `watch` reads" invariant from §2.4. |
| T-X1-10 | `receiver_changed_returns_err_after_manager_dropped` | Drop the `ConfigManager` while a subscriber task is still `awaiting .changed()`; the await resolves to `Err`, and the subscriber task exits cleanly (no panic, no hang). Exercises the telemetry bootstrap task's exit path (§4). |

---

## 3. Design — X2 (Telemetry exporter wiring)

### 3.1 Scope

Connect `tracing` events and spans to an OTLP endpoint when the user has opted in. Gated at compile time by a new `telemetry` feature on the `oneshim-app` binary crate (i.e., `src-tauri/Cargo.toml`), and at runtime by `config.telemetry.enabled`.

This is spans + events (logs) only. We do **not** add dedicated metric instruments (counters, gauges, histograms) in this phase. Once the span pipeline is healthy and shipping, a follow-up can introduce metrics by adding the `opentelemetry-sdk/metrics` feature and a handful of instruments. Keeping metrics out of scope here is deliberate: spans alone cover the triage need (who, what, when, how long) without multiplying our OTel surface area.

### 3.2 `TelemetryConfig` extension

Current shape (unchanged fields):
```rust
pub struct TelemetryConfig {
    pub enabled: bool,            // master switch; default false (opt-in)
    pub crash_reports: bool,      // reserved, not wired in this phase
    pub usage_analytics: bool,    // reserved, not wired in this phase
    pub performance_metrics: bool // reserved, not wired in this phase
}
```

Added (with serde defaults so existing config files deserialise without edit):
```rust
pub struct TelemetryConfig {
    // existing fields …
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: Option<String>,  // None = env var / default
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,               // 0.0–1.0; default 1.0 (honour tracing's filter already)
    #[serde(default = "default_service_name")]
    pub service_name: String,           // default "oneshim-client"
}

fn default_otlp_endpoint() -> Option<String> { None }
fn default_sample_rate() -> f64 { 1.0 }
fn default_service_name() -> String { "oneshim-client".into() }
```

Default endpoint resolution precedence (highest wins):
1. `config.telemetry.otlp_endpoint` if `Some`.
2. Env var `OTEL_EXPORTER_OTLP_ENDPOINT` if set (OpenTelemetry spec).
3. `http://localhost:4318` (OTLP/HTTP default — Caddy on the server VM terminates this publicly at `otel.oneshim.thengd.com` but we do not bake that URL in).

The reserved bools stay — removing them changes the JSON shape and the backoffice may already surface toggles. This phase wires `enabled` only; the other three remain for their own follow-ups (already captured as D8 and ADR work).

### 3.3 Feature flag `telemetry`

Defined in `src-tauri/Cargo.toml`:
```toml
[features]
default = []                                     # telemetry OFF by default
telemetry = [
    "dep:opentelemetry",
    "dep:opentelemetry_sdk",
    "dep:opentelemetry-otlp",
    "dep:tracing-opentelemetry",
]

[dependencies]
opentelemetry        = { version = "0.27",  optional = true, default-features = false, features = ["trace"] }
opentelemetry_sdk    = { version = "0.27",  optional = true, default-features = false, features = ["rt-tokio", "trace"] }
opentelemetry-otlp   = { version = "0.27",  optional = true, default-features = false, features = ["http-proto", "reqwest-client", "trace"] }
tracing-opentelemetry = { version = "0.28", optional = true }
```

(Exact versions pinned when we run `cargo add`; the Cargo.toml entry we land must resolve against the workspace reqwest 0.13 / tokio 1 constraints. If the HTTP variant can't be reconciled with reqwest 0.13, we fall back to `tonic-client` for OTLP/gRPC. That outcome is explicitly part of the plan's risk list, not the design.)

Why `http-proto` (not `grpc`): the workspace already pins `reqwest 0.13` and `tonic 0.14`. OTLP/HTTP re-uses the HTTP path; OTLP/gRPC would pull a second tonic surface and increase binary size. The Caddy reverse proxy on the server supports HTTP already.

Why feature-gated: default builds (CI, most developer machines, user machines with telemetry off) should not pay for the OTel transitive deps. Binary size audit (feature off vs on) is an acceptance criterion.

### 3.4 Bootstrapper

Location: `src-tauri/src/telemetry/` (new module). One file for now; split per ADR-003 only if it crosses 500 LOC.

```
src-tauri/src/telemetry/
├── mod.rs           # public: init(), subscribe_config_toggle()
└── otlp.rs          # build_layer(), shutdown_provider()    (behind `#[cfg(feature = "telemetry")]`)
```

`mod.rs` (always compiled — feature-off path reduces to empty no-op functions):
```rust
pub struct TelemetryHandle {
    #[cfg(feature = "telemetry")]
    inner: parking_lot::Mutex<TelemetryInner>,
}

#[cfg(feature = "telemetry")]
struct TelemetryInner {
    // Handle to swap the Option<OtelLayer> baked into the subscriber.
    reload_handle: tracing_subscriber::reload::Handle<
        Option<otlp::OtelLayer>,
        tracing_subscriber::Registry,
    >,
    // Current pipeline, if any. Held here so we can shutdown on toggle-off.
    active: Option<otlp::OtlpPipeline>,
    // Captured once at init (from AppConfig.telemetry).
    last_cfg: TelemetryConfig,
}

impl TelemetryHandle {
    /// Produces a handle together with the layer to attach to the subscriber.
    /// When feature is off, returns a handle and a unit placeholder layer.
    pub fn new_with_layer(initial_cfg: &TelemetryConfig)
        -> (Self, TelemetryLayer);

    /// Apply a runtime toggle. Idempotent: re-applying the same cfg is a no-op.
    pub fn apply(&self, cfg: &TelemetryConfig) -> anyhow::Result<()>;
}
```

`TelemetryLayer` is a zero-sized alias when the feature is off, and the wrapped `reload::Layer<Option<OtelLayer>, Registry>` when on. This lets `main.rs` write a single `.with(telemetry_layer)` regardless of feature state.

`otlp.rs` (behind feature):
```rust
pub(super) type OtelLayer = tracing_opentelemetry::OpenTelemetryLayer<
    tracing_subscriber::Registry,
    opentelemetry_sdk::trace::Tracer,
>;

pub(super) struct OtlpPipeline {
    provider: opentelemetry_sdk::trace::SdkTracerProvider,
}

pub(super) fn build(cfg: &TelemetryConfig) -> anyhow::Result<(OtlpPipeline, OtelLayer)>;
pub(super) fn shutdown(pipeline: OtlpPipeline);
pub(super) fn resolve_endpoint(cfg: &TelemetryConfig) -> String; // §3.2 precedence
```

### 3.5 Integration with the tracing subscriber

`main.rs`'s current subscriber composition:
```
tracing_subscriber::registry()
    .with(env_filter)
    .with(console_layer)
    .with(file_layer)
    .init();
```

Adds a fourth layer — a `reload::Layer` wrapping `Option<OtelLayer>`. The concrete construction is in §3.6 (it is the same snippet; we avoid duplicating it here).

Managed state: `TelemetryHandle` is stored in Tauri managed state so the bus-driven toggle task (§4) and any future Tauri commands can access it.

### 3.6 Runtime toggle strategy

We wrap `Option<OtelLayer>` in `tracing_subscriber::reload::Layer` and attach it to the stack at init. The `Option` is the swap unit — not the entire composite subscriber. Concrete construction in `main.rs`:

```rust
use tracing_subscriber::{layer::SubscriberExt, reload, util::SubscriberInitExt};

// `telemetry` feature OFF — no OTel dep compiles; bootstrap is a no-op.
#[cfg(not(feature = "telemetry"))]
let (telemetry_layer, telemetry_handle) = telemetry::noop_layer_and_handle();

#[cfg(feature = "telemetry")]
let (telemetry_layer, telemetry_handle) = {
    let initial: Option<otlp::OtelLayer> = match initial_cfg.enabled {
        true  => Some(otlp::build(&initial_cfg)?.1),
        false => None,
    };
    // `reload::Layer::new(L)` returns `(Layer, Handle<L, _>)` where `_` is
    // the subscriber type inferred at the `.with()` site. The `S` parameter
    // is therefore the layered subscriber produced by all preceding `.with()`
    // calls — NOT `Registry` alone. We let inference fill it in and store the
    // handle as `reload::Handle<Option<otlp::OtelLayer>, _>`; we never name
    // the `S` parameter explicitly.
    let (layer, handle) = reload::Layer::new(initial);
    (layer, telemetry::Handle::new(handle))
};

tracing_subscriber::registry()
    .with(env_filter)
    .with(console_layer)
    .with(file_layer)
    .with(telemetry_layer)   // Option<OtelLayer> wrapped in reload::Layer
    .init();
```

Boot:
- Feature OFF at compile time: `telemetry_layer` is a unit-typed no-op, `telemetry_handle` is a zero-sized struct.
- Feature ON, `config.enabled == false` at boot: wrapper holds `None`. No exporter is built until the user opts in.
- Feature ON, `config.enabled == true` at boot: wrapper holds `Some(layer)`; pipeline is live.

Runtime (on ConfigChangeBus notification):
- `true → false`: `handle.modify(|opt| { *opt = None; })`. Then call `SdkTracerProvider::shutdown()` on the stored provider to flush. After this point spans still route to console+file; the OTel layer is a no-op.
- `false → true`: build a new pipeline, then `handle.modify(|opt| { *opt = Some(new_layer); })`.

Swapping is safe at any point after subscriber init; `reload::Handle` is `Send + Sync`. Cost: one `RwLock` read per span dispatch when attached; an `Option::None` short-circuit when detached — effectively free.

Why not restart-required: we want the UX of "toggle telemetry in Preferences and it takes effect now," and `reload::Layer` is the documented tool for this pattern.

### 3.7 Privacy

- `enabled: false` is the default in `TelemetryConfig::default()`. It stays false on upgrade because the existing field was already defaulted false.
- No span attribute or log record added by this phase carries PII. The existing `oneshim-vision::privacy::PiiFilterLevel` already redacts OCR output before it reaches any tracing call. New instrumentation added by consumers in later phases is responsible for the same discipline; we document it in `docs/guides/telemetry.md` (new, see §5).
- No user identifier shipped. `service.instance.id` is a per-install random UUID generated lazily at opt-in.

`telemetry_instance_id` file lifecycle:

| State transition | File action | Who does it |
|------------------|-------------|-------------|
| First opt-in (feature on, `enabled` flips `false → true`) | Create `{data_dir}/telemetry_instance_id` containing a fresh UUIDv4, `0600` perms (owner-only read/write on Unix; `CREATE_NEW` on Windows) | `TelemetryHandle::apply` when transitioning to `Some(layer)` |
| Boot with feature on + `enabled=true` + file exists | Read UUID, attach as `service.instance.id` resource attribute | `TelemetryHandle::new_with_layer` |
| Boot with feature on + `enabled=true` + file missing | Create as above; treat as first opt-in | same |
| Boot with `enabled=false` + file exists | **Ignore the file.** Do not read, do not delete. Preserves the UUID across accidental toggles. | no-op |
| Opt-out (`enabled` flips `true → false`) | Leave file in place. | no-op |
| User-requested "forget my telemetry identity" (Tauri command `telemetry.reset_instance_id`) | Delete file. Next opt-in regenerates. | dedicated command — wired in Phase 3 UX work, not this phase |
| Feature off at compile time | File never created nor read. If it exists from a previous feature-on install, it is inert. | no-op |

Rationale: delete-on-opt-out is tempting for "clean slate" but in practice causes UUID churn when a user toggles briefly for debugging. Opt-out stops exports immediately; identity erasure is a separate explicit action.

### 3.8 Error handling

- OTel init failure (bad URL, network off): log `warn!`, proceed without the layer. App MUST NOT fail to boot because of telemetry.
- Exporter runtime failures: `opentelemetry_sdk::trace::BatchSpanProcessor` drops on queue overflow. We set the queue bound to 2048 and the export timeout to 10 s (OTel defaults); no custom retry. The Caddy-fronted collector has its own buffering.
- Shutdown failure: `shutdown_provider()` logs and swallows; app exit proceeds.

### 3.9 Followups explicitly deferred

- Metrics (counters/gauges/histograms) via OTel meters.
- Wiring `crash_reports`, `usage_analytics`, `performance_metrics` to their own pipelines.
- Distributed-trace context propagation through the reqwest client stack (requires `tracing::Instrument::in_current_span` discipline across network call sites — its own review cycle).
- Back-pressure-aware exporter (honour `OTEL_EXPORTER_OTLP_TIMEOUT` and compressed payloads).

### 3.10 Testing (X2)

| # | Test | Feature | Asserts |
|---|------|---------|---------|
| T-X2-1 | `feature_off_init_is_noop` | default | `TelemetryHandle` construction with `enabled=true` does not panic and allocates no exporter. |
| T-X2-2 | `feature_on_config_off_installs_empty_reload_wrapper` | `telemetry` | With `enabled=false` at boot the wrapper holds `None`; no network activity. |
| T-X2-3 | `feature_on_config_on_builds_pipeline` | `telemetry` | Pipeline builds against a local mock OTLP collector (see §3.10 note) and `apply` with the same config is idempotent. |
| T-X2-4 | `apply_disables_and_reenables_live` | `telemetry` | `apply(enabled=false)` swaps the wrapper to `None` and shuts down the provider; subsequent `apply(enabled=true)` rebuilds the pipeline and swaps the wrapper back to `Some(_)` with no restart. |
| T-X2-5 | `config_bus_delivers_telemetry_toggle` | `telemetry` | Integration: flip `telemetry.enabled` via `ConfigManager::update_with`; assert `TelemetryHandle::apply` is called with the new value within one async tick. |
| T-X2-6 | `opt_in_default_is_false` | default | Fresh `AppConfig::default_config().telemetry.enabled == false`. |
| T-X2-7 | `env_endpoint_overrides_default_but_not_explicit_config` | `telemetry` | Precedence in §3.2 holds. |
| T-X2-8 | `shutdown_completes_when_collector_unreachable` | `telemetry` | Boot with `enabled=true`, endpoint pointing at a closed TCP port; emit 5 spans; call `apply(enabled=false)` (which triggers `shutdown`) and assert it completes within 5 s without hanging or panicking. Guards against the OTel batch processor blocking on an unreachable exporter. |
| T-X2-9 | `instance_id_file_lifecycle_matches_state_table` | `telemetry` | Drives the §3.7 state table: first opt-in creates the file with `0600` perms (Unix), opt-out leaves it, second opt-in reuses the same UUID, `reset_instance_id` deletes and regenerates. |
| T-X2-10 | `mock_collector_receives_span` | `telemetry` | End-to-end spine test: start mock Axum collector on `127.0.0.1:0`, configure endpoint, emit a single span, assert the collector sees an OTLP POST body containing the span name within 15 s. Replaces the prior manual-verification acceptance criterion. |

T-X2-3 and T-X2-10 use a mock OTLP collector — a minimal Axum route on `127.0.0.1:<random>` that answers `200 OK` — so CI does not depend on external hosts. The mock lives under `src-tauri/tests/mock_otlp.rs` and is only compiled with `--features telemetry`.

---

## 4. Cross-item interaction

The only coupling is §3.6: the telemetry module is the first subscriber to `ConfigChangeBus`. Concretely, in `src-tauri/src/main.rs` after Tauri state is built:

```rust
let handle_for_task = telemetry_handle.clone();
let mut rx = config_manager.subscribe();
tokio::spawn(async move {
    let mut prev = rx.borrow_and_update().telemetry.clone();
    while rx.changed().await.is_ok() {
        let current = rx.borrow_and_update().telemetry.clone();
        if current != prev {
            if let Err(e) = handle_for_task.apply(&current) {
                warn!(error=%e, "telemetry apply failed");
            }
            prev = current;
        }
    }
});
```

This task lives for the process lifetime. Dropping the `ConfigManager` (never, in practice) would close the channel, `changed()` returns `Err`, loop exits cleanly.

---

## 5. Documentation deliverables

- `docs/guides/telemetry.md` — new. End-user view: what is collected, how to enable, how to point it at a custom collector. Korean companion (`.ko.md`) per `docs/DOCUMENTATION_POLICY.md`.
- `docs/architecture/ADR-016-config-change-bus.md` — new. Records the watch-channel + subscribe API decision, the audit-coalescing hazard, and the non-migration policy for existing loops. ADR-005 through ADR-015 are already taken; next free slot is 16 (verified against `docs/architecture/ADR-*.md`).
- `docs/STATUS.md` — bump test totals and feature-gate line when implementation lands (not a spec deliverable — implementation deliverable).
- Per-crate `CLAUDE.md` additions where relevant (`oneshim-core` for the new API surface, `src-tauri` for the telemetry module location).

---

## 6. Rollout

This ships on a single feature branch `feat/phase2-config-telemetry`:

0. **Spike (pre-Commit-4)** — disposable, not committed. Run `cargo add opentelemetry opentelemetry_sdk opentelemetry-otlp tracing-opentelemetry --dry-run` and then a throwaway `cargo check --features telemetry` in a scratch branch to confirm reqwest-0.13 / tokio-1 compat and record the concrete resolved minor versions. Outcome is captured in the plan, not the spec. If HTTP transport cannot resolve, the plan switches to `grpc-tonic` before Commit 5.
1. **Commit 1** — X1 core: `watch` channel + `subscribe()` + `snapshot()` + T-X1-1..10. All existing `ConfigManager` tests must still pass unchanged.
2. **Commit 2** — ADR-016.
3. **Commit 3** — X2 config extension: `otlp_endpoint`, `sample_rate`, `service_name` with serde defaults; T-X1-8 already in Commit 1 exercises the backward-compat deserialisation. T-X2-6 here.
4. **Commit 4** — `telemetry` feature + deps + empty module skeleton + no-op feature-off path (T-X2-1 lands here).
5. **Commit 5** — X2 OTLP pipeline + reload-wrapped layer attach + `instance_id` lifecycle + T-X2-2..4, T-X2-7, T-X2-8, T-X2-9.
6. **Commit 6** — Bus-driven telemetry toggle task in `main.rs` + T-X2-5 + T-X2-10 (mock-collector spine test).
7. **Commit 7** — User doc (`docs/guides/telemetry.md` + `.ko.md`).

No X1 consumer migration commits in this phase (see §2.6). The telemetry bootstrap task in Commit 6 is the only `subscribe()` consumer in this PR.

Each commit must keep `cargo check --workspace`, `cargo test --workspace`, and `cargo clippy --workspace -- -D warnings` green. The `--features telemetry` variant runs as a **path-gated** CI matrix cell: it fires only when `src-tauri/**`, `crates/oneshim-core/src/config/sections/storage.rs`, or `docs/guides/telemetry.md` changed in the PR; the nightly/main schedule runs it unconditionally. This limits the compile-time doubling to PRs that could plausibly break telemetry (§8).

---

## 7. Acceptance criteria

All machine-checkable:

- `cargo check --workspace` on default features: green on macOS / Linux / Windows CI.
- `cargo check --workspace --features telemetry` on `src-tauri`: green on the same matrix (path-gated per §6).
- `cargo test --workspace` on default features: green.
- `cargo test -p oneshim-app --features telemetry`: green, including T-X2-3, T-X2-4, T-X2-5, T-X2-8, T-X2-9, T-X2-10.
- `cargo clippy --workspace --all-targets -- -D warnings` and the `--features telemetry` variant: both green (includes Rust 1.95 lints).
- T-X2-10 (mock-collector spine test) passes: a span emitted with telemetry enabled reaches the mock collector's HTTP handler within 15 s. This replaces the previous manual `docker run` verification.
- T-X2-4 passes: runtime toggle off→on→off cycles cleanly within one async tick each, file+console logging unaffected.
- Binary size delta vs `main` (measured on `cargo build --release -p oneshim-app`, stripped):
  - Default build: **≤ +20 KB** (subscribe API plus serde extensions).
  - `--features telemetry` build: **≤ +5 MB target**. First measurement lands in `docs/STATUS.md` under a new "Telemetry feature size" row. If the first measurement exceeds 5 MB, the plan documents the actual delta and we decide reconcile-vs-accept in Loop 3 review rather than re-gating the spec.
- Fresh install: `AppConfig::default_config().telemetry.enabled == false`.
- `telemetry_instance_id` file permissions match the §3.7 state table (verified by T-X2-9).

---

## 8. Risks & mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|-----------|
| OTel crate version churn breaks on CI | Medium | Pin minor versions; matrix job runs only on `telemetry` feature and is path-gated so default build is unaffected if we need to temporarily revert. |
| OTLP/HTTP can't be reconciled with workspace reqwest 0.13 | Medium | **Commit-0 spike** (§6) forces this verification before any OTel code lands. Fallback to `opentelemetry-otlp` `grpc-tonic` (tonic 0.14 already in the tree). Spike outcome captured in the plan, not the spec. |
| Config bus introduces subtle race (subscriber sees old snapshot after update completes) | Low | `writer_lock` serialises writers; `send_replace` broadcasts atomically inside `watch`. Tests T-X1-2..4 exercise the happy path; T-X1-9 pins no-reentrancy. Documented that `snapshot()` and `subscribe()` are latest-wins. |
| Consumer silently coalesces audit-critical transitions | **High if migrated** | Not migrating any consumer in this phase (§2.6). The audit-coalescing hazard is recorded in ADR-016 so Phase 3 migrations review it before touching code. |
| Accidental PII in new span attributes | Medium | PR checklist item + `docs/guides/telemetry.md` guidance. (The `clippy::missing_docs_in_private_items` idea was aspirational and is dropped to avoid a bogus lint gate.) |
| Path-gated CI cell lets telemetry regressions slip through unrelated PRs | Low | Scheduled main run + RC release-gate unconditionally compile `--features telemetry`. Regressions caught within 24 h at worst. |
| `SdkTracerProvider::shutdown` hangs when collector unreachable | Low | Explicitly tested by T-X2-8 with a 5 s watchdog. Provider is dropped inside `tokio::task::spawn_blocking` if needed to isolate from the async runtime. |

---

## 9. Out of scope (and why, pointed at the right issue)

- Converting scheduler loops to `subscribe()` — each loop needs individual review for the audit-coalescing hazard (§2.6). Captured as a Phase 3 follow-up line per loop.
- Metrics (counters/gauges/histograms) — additive on top of OTel plumbing once spans are healthy. Separate phase.
- Server-side OTel endpoint TLS setup on `otel.oneshim.thengd.com` — server-repo work; Caddy already terminates HTTP(S).
- C1/C2/C3 from the feature-gap doc — Phase 3 (this design intentionally avoids them).
- Sentry-style crash reports (`crash_reports` bool) — separate exporter (panic handler + mini-dump); not this phase.
- Tauri command to trigger `reset_instance_id` — wiring exists in the state table (§3.7) but the UI surface is Phase 3 settings-page work.

---

## 10. Alternatives considered (summary)

See §2.2 for X1's channel-type alternatives and §3.6 for X2's layer-swap alternatives. Two more surfaced during design and were rejected:

- **Embed the telemetry module in `oneshim-core`** instead of `src-tauri`. Rejected: OTel pulls reqwest + tokio-rt features that would infect every library crate's MSRV and binary footprint. Telemetry is a binary concern.
- **Use the existing `tracing` subscriber's JSON output + a separate sidecar collector process** rather than OTel. Rejected: adds a process-management problem; OTel Collector already exists server-side and accepts OTLP directly.
