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

- **Only migrate where it actually reduces latency or fixes a bug.** Config polling is cheap; migrating 7 loops just to say they subscribe is busywork.
- **Required in this phase**: the telemetry bootstrapper (X2) must use `subscribe` because toggling `telemetry.enabled` at runtime is the feature.
- **Optional in this phase**: one demonstrator loop converts to `subscribe` + diff to set the pattern for future work. We will convert `src-tauri/src/scheduler/loops/monitor.rs` where `prev_pii_level` already performs an ad-hoc diff — this exercise proves the idiom on real code.
- **Out of scope**: converting the other five loops; those are their own line items in Phase 3.

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
| T-X1-7 | `no_spurious_wakeup_when_content_identical` | Two `update()` calls with semantically equal configs still each generate a notification (watch semantics — documented behaviour; consumers do the diff). |

T-X1-7 documents expected behaviour rather than enforcing suppression: per-section diff is the consumer's job (§2.5).

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

Adds a fourth optional layer:
```rust
let (otel_layer, telemetry_handle) = telemetry::build_layer_and_handle(&initial_cfg);
tracing_subscriber::registry()
    .with(env_filter)
    .with(console_layer)
    .with(file_layer)
    .with(otel_layer)   // None when feature off or config.enabled = false at boot
    .init();
```

`otel_layer` is `Option<tracing_opentelemetry::OpenTelemetryLayer<…>>`. `tracing_subscriber::Registry` supports `Option<Layer>` natively (`None` is a no-op).

Managed state: `TelemetryHandle` is stored in Tauri managed state so scheduler loops (and Tauri commands if ever needed) can access it.

### 3.6 Runtime toggle strategy

We wrap the OTel layer in `tracing_subscriber::reload::Layer<Option<OtelLayer>, Registry>`. The wrapped type is always attached; at boot it is `Some(layer)` or `None` depending on config. On runtime config change we swap via the `reload::Handle`.

Boot:
- Feature OFF at compile time: no `Option<OtelLayer>` type exists; we skip the reload wrapper entirely. The `reload` dep is also feature-gated.
- Feature ON, config.enabled == false at boot: install `reload::Layer<Option<OtelLayer>>` with `None`. No exporter is built until the user opts in.
- Feature ON, config.enabled == true at boot: build the pipeline, install with `Some(layer)`.

Runtime (on ConfigChangeBus notification):
- `true → false`: `handle.modify(|opt| { if let Some(l) = opt.take() { /* drop layer */ } })`. Then call the stored `SdkTracerProvider::shutdown()` to flush. After this point spans still route to console+file; the OTel layer is a no-op.
- `false → true`: build a new pipeline, then `handle.modify(|opt| *opt = Some(new_layer))`.

Swapping is safe at any point after subscriber init; `reload::Handle` is `Send + Sync`. The cost is small: one extra `RwLock` indirection per span dispatch when the layer is attached, zero when the `Option` is `None`.

Why not restart-required: we want the UX of "toggle telemetry in Preferences and it takes effect now," and `reload::Layer` is the documented tool for this pattern. The previous reject-this-and-defer stance reconsidered.

### 3.7 Privacy

- `enabled: false` is the default in `TelemetryConfig::default()`. It stays false on upgrade because the existing field was already defaulted false.
- No span attribute or log record added by this phase carries PII. The existing `oneshim-vision::privacy::PiiFilterLevel` already redacts OCR output before it reaches any tracing call. New instrumentation added by consumers in later phases is responsible for the same discipline; we document it in `docs/guides/telemetry.md` (new, see §5).
- No user identifier shipped. `service.instance.id` is a per-install random UUID stored at first telemetry enable under `{data_dir}/telemetry_instance_id`. If the user opts out, the file is deleted so the next opt-in generates a fresh ID.

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
| T-X2-1 | `feature_off_init_is_noop` | default | `TelemetryHandle::init` with `enabled=true` still does not panic or allocate an exporter. |
| T-X2-2 | `feature_on_config_off_does_not_install_layer` | `telemetry` | `init` with `enabled=false` returns `Ok`; no network activity. |
| T-X2-3 | `feature_on_config_on_builds_pipeline` | `telemetry` | Pipeline builds with endpoint `http://127.0.0.1:4318`; `apply` is idempotent. |
| T-X2-4 | `apply_disables_when_toggled_off` | `telemetry` | After `apply(enabled=false)`, provider is shut down; subsequent `apply(enabled=true)` logs the restart-required warning. |
| T-X2-5 | `config_bus_delivers_telemetry_toggle` | `telemetry` | Integration: drive `ConfigManager::update_with` flipping `telemetry.enabled`, assert `TelemetryHandle::apply` is called with the new value. |
| T-X2-6 | `opt_in_default_is_false` | default | Fresh `AppConfig::default_config().telemetry.enabled == false`. |
| T-X2-7 | `env_endpoint_overrides_default_but_not_explicit_config` | `telemetry` | Precedence in §3.2 holds. |

T-X2-3 uses a mock OTLP collector (a minimal Axum route on 127.0.0.1 that returns 200) rather than a real server, so CI does not depend on external hosts.

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
- `docs/architecture/ADR-005-config-change-bus.md` — new. Records the watch-channel + subscribe API decision and the non-migration policy for existing loops. Numbered sequentially after ADR-004.
- `docs/STATUS.md` — bump test totals and feature-gate line when implementation lands (not a spec deliverable — implementation deliverable).
- Per-crate `CLAUDE.md` additions where relevant (`oneshim-core` for the new API surface, `src-tauri` for the telemetry module location).

---

## 6. Rollout

This ships on a single feature branch `feat/phase2-config-telemetry`:

1. **Commit 1** — X1 core: `watch` channel + `subscribe()` + `snapshot()` + T-X1-1..7.
2. **Commit 2** — X1 demonstrator: `monitor.rs` converts `prev_pii_level` ad-hoc diff to subscribe-and-diff. No behaviour change.
3. **Commit 3** — ADR-005.
4. **Commit 4** — X2 config extension: `otlp_endpoint`, `sample_rate`, `service_name` with serde defaults; T-X2-6 lands here.
5. **Commit 5** — `telemetry` feature + deps + empty module skeleton (no-op init).
6. **Commit 6** — X2 OTLP pipeline + tracing layer attach + T-X2-1..4, T-X2-7.
7. **Commit 7** — Bus-driven telemetry toggle task in `main.rs` + T-X2-5.
8. **Commit 8** — User doc + Korean companion.

Each commit must keep `cargo check --workspace`, `cargo test --workspace`, and `cargo clippy --workspace -- -D warnings` green. The `--features telemetry` variant also runs in CI (new matrix cell).

---

## 7. Acceptance criteria

- `cargo check --workspace` + `--features telemetry` on `src-tauri` both green on macOS / Linux / Windows CI.
- `cargo test --workspace` + the feature-flagged tests green.
- `cargo clippy --workspace --all-targets -- -D warnings` + `--features telemetry` green (incl. Rust 1.95 new lints).
- With `config.telemetry.enabled = true` and a local OTLP/HTTP collector at `127.0.0.1:4318`, spans from `oneshim-network::batch_uploader::upload_batch` arrive at the collector within 15 s (manual verification via `docker run otel/opentelemetry-collector-contrib` with a debug exporter).
- Runtime toggle off (via a Tauri command that calls `config_manager.update_with`) stops new exports within 5 s; file+console logging unaffected.
- Binary size delta vs `main` (measured on `cargo build --release -p oneshim-app`): default build ≤ +20 KB (pure code additions — subscribe API plus serde extensions; no OTel pulled in); `--features telemetry` build ≤ +2 MB. Actual numbers land in `docs/STATUS.md` after first measurement.
- No new clippy warning with default features or with `telemetry`.
- Fresh install: `TelemetryConfig.enabled == false` after boot.

---

## 8. Risks & mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|-----------|
| OTel crate version churn breaks on CI | Medium | Pin minor versions; matrix job runs only on `telemetry` feature so default build is unaffected if we need to temporarily revert. |
| OTLP/HTTP can't be reconciled with workspace reqwest 0.13 | Medium | Fallback to `opentelemetry-otlp` `grpc-tonic` feature; we already have tonic 0.14 in the tree. Pivot decision captured in the plan; not a spec deliverable. |
| Config bus introduces subtle race (subscriber sees old snapshot after update completes) | Low | Write-lock held during swap; `send` called after; tests T-X1-2..4 exercise happy path. Documented that `snapshot()` and `subscribe()` are latest-wins. |
| Telemetry restart-required on enable surprises users | Low | In-app warning toast + doc entry. Follow-up issue opens immediately to implement `reload::Layer` swap. |
| Accidental PII in new span attributes | Medium | PR checklist item + `docs/guides/telemetry.md` guidance + `#[deny(clippy::missing_docs_in_private_items)]` on new span attribute adders (aspirational — not part of this phase). |

---

## 9. Out of scope (and why, pointed at the right issue)

- Converting all seven scheduler loops to `subscribe()` — pure mechanical migration, separate PR per loop post-phase. Captured as a follow-up line in Phase 3.
- Metrics (counters/gauges/histograms) — additive on top of OTel plumbing once spans are healthy. Separate phase.
- Server-side OTel endpoint TLS setup on `otel.oneshim.thengd.com` — server-repo work; Caddy already terminates HTTP(S).
- C1/C2/C3 from the feature-gap doc — Phase 3 (this design intentionally avoids them).
- Sentry-style crash reports (`crash_reports` bool) — separate exporter (panic handler + mini-dump); not this phase.

---

## 10. Alternatives considered (summary)

See §2.2 for X1's channel-type alternatives and §3.6 for X2's layer-swap alternatives. Two more surfaced during design and were rejected:

- **Embed the telemetry module in `oneshim-core`** instead of `src-tauri`. Rejected: OTel pulls reqwest + tokio-rt features that would infect every library crate's MSRV and binary footprint. Telemetry is a binary concern.
- **Use the existing `tracing` subscriber's JSON output + a separate sidecar collector process** rather than OTel. Rejected: adds a process-management problem; OTel Collector already exists server-side and accepts OTLP directly.
