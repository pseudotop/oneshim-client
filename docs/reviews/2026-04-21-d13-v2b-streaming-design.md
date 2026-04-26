# D13 v2b — Dashboard gRPC Streaming RPCs

**Date**: 2026-04-21
**Status**: Design (spec) — awaiting user review before implementation
**Depends on**: D13 v1 (PR #455/#456/#462/#463), D13 v2a (PR #476 — merged 2026-04-21)
**Paired with**: [D13 v3 proto convention cleanup tracker](./2026-04-21-d13-v3-proto-convention-cleanup.md) (PR #477)
**Related**: [D13 v2 roadmap](./2026-04-21-d13-v2-roadmap.md) (PR #475) — high-level scope
**Target release**: v0.4.40 (post-stable-promotion) or v0.4.41

## Context

D13 v2a shipped 4 per-domain unary RPCs (`GetSessionStats`, `GetRecentFrames`, `GetProductivityMetrics`, `GetFocusStats`) on top of v1's `GetAgentInfo`/`HealthCheck`. Each returns an aggregated snapshot on demand.

v2b adds **server-streaming** RPCs for two realtime use cases:

- **Live metrics dashboards**: continuous CPU/memory/activity updates
- **Event streams**: frame captures, idle transitions, AI runtime status changes

v2b intentionally **does not** add TLS or dedicated gRPC auth tokens — those belong to v2c. v2b ships with localhost-only binding (inherited from v1/v2a) and reuses the existing `integration_auth_token` as an opt-out trust signal (see §4 Auth gate).

## Goals

1. Provide a typed gRPC streaming contract that external dashboards/CLIs can consume alongside (or instead of) the existing REST `/stream` SSE endpoint
2. Protect the user's PC from overzealous clients by classifying server load and enforcing reasonable rate limits (with an informative hint channel, not silent dropping)
3. Preserve the local-first architecture: SQLite is the source of truth; `event_tx` is a wake-up signal, not a data path
4. Maintain backward compatibility with v2a clients (no wire-breaking changes to existing RPCs)

## Non-goals

- gRPC-web proxy (browser clients) — deferred to v3
- Reflection service (`grpcurl` convenience) — deferred to v3
- Mutations (Write RPCs) — read-only by design, v3+
- Bidirectional streaming (dynamic interval renegotiation) — v3
- New `RealtimeEvent` variants (SessionStart/End, FocusStart/End) — separate ADR
- TLS, dedicated gRPC auth token, cert rotation — v2c

## High-level architecture

```
┌────────────┐   ┌──────────┐   ┌──────────────────────────────────┐
│ Monitor /  │   │ SQLite   │   │ DashboardServiceImpl             │
│ Capture    │→→→│ (source  │→→→│  - unary (v2a): query DB          │
│ loops      │   │  of      │   │  - stream (v2b):                  │
└────────────┘   │  truth)  │   │      event_tx tick → query DB →   │
                 │          │   │      classify load → enforce →    │
                 └──────────┘   │      emit {Data | Hint | Signal}  │
                      ↑         └──────────────────────────────────┘
                 event_tx
                 (wake-up signal, not data path)
```

v2b extends `DashboardServiceImpl`'s DI with three new dependencies:
- `Arc<dyn SystemMonitor>` — server load classification
- `broadcast::Sender<RealtimeEvent>` — subscription to existing wake-up channel
- `Option<String> integration_auth_token` — trust gate for opt-out (T2-extended)

All three wire up at `src-tauri/src/app_runtime_launch.rs` via a new `GrpcSpawnConfig` struct (replacing positional args).

---

## §1. Scope & feature additions

### New RPCs (2)

| RPC | Purpose | Cadence knob |
|---|---|---|
| `SubscribeMetrics` | CPU/memory/activity buckets — realtime (paced by the existing metrics monitor loop, typically ≈5s default) or interval-aggregated | `interval_secs` (0=realtime, N=N-second buckets) |
| `SubscribeEvents` | Frame / Idle / AiRuntimeStatus events | `event_types` filter + server rate limit |

### Architectural decisions

1. **Reuse existing `event_tx: broadcast::Sender<RealtimeEvent>`** (capacity 256). v2b adds one subscriber per active gRPC stream. No new broadcast channel. `RealtimeEvent::Metrics` is emitted by `Scheduler::spawn_metrics_loop` at the configured `metrics_interval` (default 5 seconds per `src-tauri/src/scheduler/mod.rs`), so "realtime" in SubscribeMetrics means "forwarded within one metrics tick after the monitor loop saves to SQLite" — not sub-second.

2. **In-band `oneof` stream responses** — each stream yields one of:
   - `SubscribeMetricsResponse { MetricBucket | ServerLoadHint }`
   - `SubscribeEventsResponse { DashboardEvent | ServerLoadHint | DroppedEventsSignal }`
   - No sidecar RPCs for hints, no metadata-based hints mid-stream

3. **Server-side enforcement on by default, opt-out gated by trust (T2-extended)**:
   - Opt-out (`respect_server_hints = false`) honored if either:
     - (a) Server bound to loopback (`127.0.0.1` / `::1`) — current default, always true pre-v2c
     - (b) Request carries `Authorization: Bearer <integration_auth_token>` matching configured token
   - Otherwise opt-out silently downgrades to `respect = true` + logs a warning

4. **Local-first data path**: `event_tx` is a wake-up signal; every emitted payload is freshly queried from SQLite. Same data consistency guarantees as v2a unary RPCs.

5. **Hardware-independent load classification (§3)** — `sysinfo`'s system-wide `cpu_usage` (%) + absolute memory headroom in GiB. CPU percentage is already normalized across core counts by the sysinfo crate, so no per-core math is needed. Three explicit CPU% boundaries (`cpu_low`, `cpu_medium`, `cpu_high`) are shipped as sensible defaults and config-overridable.

### Non-goals (repeated for emphasis)

Listed at the top of this doc. Anything not in the RPC table above or the decision list is out of scope for v2b.

### Prerequisites — RealtimeEvent emission wiring

Grep of the current workspace (`send(RealtimeEvent::{Frame|Idle|AiRuntimeStatus|Ping})` across `crates/**` and `src-tauri/**`) returns **zero production hits**. Only `RealtimeEvent::Metrics` is emitted today — by `Scheduler::spawn_metrics_loop`. The other variants exist as enum members and are fully SSE-serializable, but no production code path calls `event_tx.send(...)` with them. The SSE `/stream` handler forwards whatever is on `event_tx`, so today its clients receive only Metrics events (plus an initial `AiRuntimeStatus` pushed directly, not via event_tx).

**Implication for V2b.** SubscribeEvents as specified would subscribe to `event_tx` and see nothing. Before SubscribeEvents ships, the emission sites for the three in-scope event types must be wired:

| Event | Wire at | Expected shape |
|---|---|---|
| `Frame` | capture pipeline — after frame row insert in `oneshim-vision::processor` (or the scheduler loop that owns the insert, whichever commits the DB row first) | `event_tx.send(RealtimeEvent::Frame(FrameUpdate { id, timestamp, app_name, window_title, importance }))` right after the insert's `tx.commit()` |
| `Idle` | idle-monitor loop (today reflected in interruption records, but no event emission). The loop is in `oneshim-monitor` — add a transition edge: when `is_idle` flips, send `IdleUpdate { is_idle, idle_secs }` | Atomic to the idle-state flip |
| `AiRuntimeStatus` | `AppState.automation.ai_runtime_status` setter site (already exists; just add a `event_tx.send(RealtimeEvent::AiRuntimeStatus(status.clone()))` next to the set) | On every status mutation |

These wires are **prerequisites for SubscribeEvents**, folded into PR-B3's scope (bumping the PR-B3 LoC estimate).

Not in scope: `Ping` emission. `Ping` is effectively a keepalive marker — the SSE `/stream` handler already issues SSE-level pings via `KeepAlive::new().text("ping")`. V2b's gRPC streams use `DashboardEvent` payload or `ServerLoadHint` heartbeat for liveness; a separate `Ping` emission would be redundant.

---

## §2. Proto schema

### File

`api/proto/oneshim/dashboard/v1/dashboard.proto` (extends existing)

### Convention alignment

v2b's **new** messages adopt the canonical conventions observed in `api/proto/oneshim/client/v1/*.proto`:

- `google.protobuf.Timestamp` for timestamps
- `{ENUM_NAME}_UNSPECIFIED = 0;` prefix on enum variants
- `oneof payload` for heterogeneous stream items

v2a's already-shipped drifts (string RFC 3339 in `FrameMetadata.captured_at`, un-prefixed `HealthCheckResponse.Status`) are **not retrofitted here** to keep v2b scope bounded. See the [v3 cleanup tracker](./2026-04-21-d13-v3-proto-convention-cleanup.md).

### Type promotion

V2a's nested `productivity_metrics_response.MetricBucket` is promoted to top-level `MetricBucket`, shared between v2a unary response and v2b streaming response. Wire format unchanged (field numbers preserved).

### Service additions

```proto
service DashboardService {
  // ... existing v1 + v2a RPCs ...

  // V2b: server-streaming
  rpc SubscribeMetrics(SubscribeMetricsRequest) returns (stream SubscribeMetricsResponse);
  rpc SubscribeEvents(SubscribeEventsRequest) returns (stream SubscribeEventsResponse);
}
```

### Message definitions

```proto
import "google/protobuf/timestamp.proto";

// ── SubscribeMetrics ───────────────────────────────────────────────

message SubscribeMetricsRequest {
  // 0 = realtime (DB query on every event_tx::Metrics tick);
  // N>0 = emit an aggregated bucket every N seconds (tokio::interval).
  // Server clamps to [250ms floor, 60s ceiling] and applies load-based
  // enforcement (see §3).
  uint32 interval_secs = 1;

  // When false, client asks server to skip enforcement (only honored
  // if the connection is trusted — see §4 Auth gate).
  bool respect_server_hints = 2;
}

message SubscribeMetricsResponse {
  oneof payload {
    MetricBucket data = 1;
    ServerLoadHint hint = 2;
  }
}

// Promoted from v2a ProductivityMetricsResponse.MetricBucket to top-level.
message MetricBucket {
  google.protobuf.Timestamp start = 1;
  double cpu_avg_pct = 2;
  double memory_avg_mb = 3;
  uint32 active_keystrokes = 4;
  uint32 active_mouse_clicks = 5;
}

// ── SubscribeEvents ────────────────────────────────────────────────

message SubscribeEventsRequest {
  // "frame" | "idle" | "ai_runtime_status". Empty = all three (no filter).
  // Unknown types are silently ignored (forward-compat for v3 event types).
  repeated string event_types = 1;
  bool respect_server_hints = 2;
}

message SubscribeEventsResponse {
  oneof payload {
    DashboardEvent event = 1;
    ServerLoadHint hint = 2;
    DroppedEventsSignal dropped = 3;
  }
}

message DashboardEvent {
  // When the underlying event actually occurred (DB-backed when
  // possible, else RealtimeEvent wake-up timestamp).
  google.protobuf.Timestamp occurred_at = 1;

  oneof payload {
    FrameEvent frame = 2;
    IdleEvent idle = 3;
    AiRuntimeStatusEvent ai_runtime_status = 4;
  }
}

message FrameEvent {
  // DB primary key. Image bytes fetched separately via REST
  // GET /api/frames/{id}/image — gRPC stream stays text-payload-sized.
  int64 frame_id = 1;
  string app_name = 2;
  string window_title = 3;
  float importance = 4;
  string trigger_type = 5;  // "active_change" | "timer" | ...
}

message IdleEvent {
  bool is_idle = 1;
  uint64 idle_secs = 2;
}

message AiRuntimeStatusEvent {
  string ocr_source = 1;
  string llm_source = 2;
  string ocr_fallback_reason = 3;  // empty when no fallback
  string llm_fallback_reason = 4;
}

// ── Shared hint/signal messages ────────────────────────────────────

message ServerLoadHint {
  enum Level {
    LOAD_LEVEL_UNSPECIFIED = 0;
    LOAD_LEVEL_LOW = 1;
    LOAD_LEVEL_MEDIUM = 2;
    LOAD_LEVEL_HIGH = 3;
    LOAD_LEVEL_CRITICAL = 4;
  }

  Level load_level = 1;
  float cpu_pct = 2;
  float memory_pct = 3;
  // 0 = no suggestion. Relevant for SubscribeMetrics.
  uint32 suggested_interval_secs = 4;
  // 0 = no suggestion. Relevant for SubscribeEvents (events/sec cap).
  uint32 suggested_event_rate_limit = 5;
  // Human-readable, for logs/UIs. Example: "Server CPU 87% (HIGH)".
  string reason = 6;
  google.protobuf.Timestamp emitted_at = 7;
}

message DroppedEventsSignal {
  uint64 dropped_count = 1;
  google.protobuf.Timestamp since = 2;
  google.protobuf.Timestamp until = 3;
  // "rate_limit" | "channel_lag"
  string reason = 4;
  // Populated for reason="rate_limit". Empty for "channel_lag".
  repeated TypeCount by_type = 5;

  message TypeCount {
    string event_type = 1;
    uint64 count = 2;
  }
}
```

### Wire-level migration

- **V2a clients** see zero wire-level change (their RPCs untouched, MetricBucket wire format preserved via same field numbers)
- **V2b-only additions**: new RPCs + new message types. Existing proto consumers ignore unknown methods
- Proto regeneration (`scripts/regenerate-dashboard-protos.sh`) expected to grow the generated Rust file from ~400 LoC to ~900 LoC

---

## §3. Server-side components

### `DashboardServiceImpl` DI expansion

```rust
pub struct DashboardServiceImpl {
    started_at: Instant,                              // v1
    storage: Arc<dyn WebStorage>,                     // v2a
    // v2b additions:
    system_monitor: Arc<dyn SystemMonitor>,
    event_tx: broadcast::Sender<RealtimeEvent>,
    integration_auth_token: Option<String>,
    load_policy: Arc<LoadPolicy>,
}
```

### `LoadPolicy` — load classification

Classifies current server state using **system-wide CPU percentage + absolute memory headroom** (Option A from brainstorming — see v3 cleanup tracker for deferred Option B / C).

> **Note on normalization.** The brainstorming settled on "per-core load average", but the existing `SystemMonitor` trait (`crates/oneshim-core/src/ports/monitor.rs`) returns `SystemMetrics { cpu_usage: f32, … }` where `cpu_usage` is **already a system-wide percentage (0.0 – 100.0)** produced by the `sysinfo` crate's normalized aggregation. That gives us hardware-independent thresholds for free, without any per-core math. Adopting true load-average would require extending `SystemMetrics` and the `sysinfo` adapter — out of scope for v2b. Classifier inputs are therefore `cpu_usage` (%) and `free_mem_gb`.

```rust
pub struct LoadPolicy {
    thresholds: LoadThresholds,
    warmup_until: Instant,  // server start + 30s
}

pub struct LoadThresholds {
    pub min_free_mem_gb: f32,   // default 2.0
    pub cpu_low_pct: f32,       // default 50.0 (LOW → MEDIUM boundary)
    pub cpu_medium_pct: f32,    // default 70.0 (MEDIUM → HIGH boundary)
    pub cpu_high_pct: f32,      // default 90.0 (HIGH → CRITICAL boundary)
}
```

Three explicit CPU% boundaries (rather than one `max` + ratios) keeps the spec unambiguous; the ratio approach broke down because `sysinfo::cpu_usage` is bounded to 0.0–100.0 but the ratio 1.34 × 75 = 100.5% would never trigger. Validation requires `cpu_low < cpu_medium < cpu_high` and all ∈ (0, 100); `LoadPolicy::new` rejects other combinations.

**Classification** (`classify(&self, metrics: &SystemMetrics) -> LoadLevel`):

```
cpu_pct     = metrics.cpu_usage                              // sysinfo, already 0.0–100.0
free_mem_gb = (metrics.memory_total - metrics.memory_used) as f32 / 1_073_741_824.0  // bytes → GiB

if warmup_not_finished:
    return MEDIUM
else if cpu_pct < cpu_low_pct AND free_mem_gb > min_free_mem_gb * 1.5:
    return LOW
else if cpu_pct < cpu_medium_pct AND free_mem_gb > min_free_mem_gb:
    return MEDIUM
else if cpu_pct < cpu_high_pct AND free_mem_gb > min_free_mem_gb * 0.5:
    return HIGH
else:
    return CRITICAL
```

Defaults `(50, 70, 90)` are the traditional "calm / busy / overloaded" triples used by system-tray CPU indicators on macOS/Windows. Config override lets operators tighten or relax the whole ladder (e.g. `{cpu_low: 30, cpu_medium: 50, cpu_high: 70}` on a battery-constrained laptop).

The ladder uses `AND` at every level so that escalation is **protective**: a single degraded signal (either CPU or memory) is enough to escalate to the next level. CRITICAL triggers when `cpu_pct >= cpu_high_pct OR free_mem_gb <= min_free_mem_gb * 0.5` — the disjunctive fallthrough — which is the strict protective interpretation (protect before both signals collapse).

### Enforcement ladder

Outputs applied per-RPC in the stream loop. LOW uses a high per-type cap ("practically unlimited for any realistic client") rather than truly unlimited — this keeps a single constant shape across all four levels and prevents pathological misbehavior (a runaway producer would still be bounded to ~250 events/sec).

| Level | `SubscribeMetrics` interval clamp | `SubscribeEvents` Frame cap | Idle cap | AiStatus cap |
|---|---|---|---|---|
| LOW | `max(requested, 250ms)` | 250/s | 250/s | 250/s |
| MEDIUM | `max(requested, 1s)` | 10/s | 2/s | 1/s |
| HIGH | `max(requested, 5s)` | 2/s | 1/s | 1/s |
| CRITICAL | `max(requested, 30s)` | 0.5/s (1 per 2s) | 0.5/s | 0.5/s |

### Safety floors (always-on, level-independent)

- **Interval floor**: effective interval never below 250ms (prevents tight polling)
- **Interval ceiling**: effective interval never above 60s (hints delivered within the minute regardless)
- **Warm-up**: first 30s after server start, always classify as MEDIUM (sysinfo stabilization period)

### Warm-up × opt-out interaction

Opt-out (`respect_server_hints=false` + trusted connection) **bypasses the enforcement ladder** but **does not bypass the warm-up window's classification**. The emitted hint during warm-up still reads `LOAD_LEVEL_MEDIUM, reason="warmup"`; the difference is that an opt-out subscriber receives data at the client's requested cadence instead of the clamped MEDIUM cadence. Once warm-up ends, classification resumes naturally. This keeps the hint channel honest (never lying about the warm-up state) while still giving trusted clients their requested rate.

### Opt-out gate

Implementation of the T2-extended policy:

```rust
fn honor_opt_out(
    req_respect_hints: bool,
    remote_addr: SocketAddr,
    auth_header: Option<&str>,
    configured_token: Option<&str>,
) -> bool {
    // Client opted in → always enforce
    if req_respect_hints { return true; }

    // Trust (a): loopback binding
    if remote_addr.ip().is_loopback() { return false; }

    // Trust (b): matching configured token
    if let (Some(h), Some(t)) = (auth_header, configured_token) {
        if h == format!("Bearer {t}") { return false; }
    }

    // Default: enforce
    true
}
```

### Helper components

- **`HintEmitter`** — tracks last-emitted `LoadLevel` and last-emit timestamp per stream; emits on initial subscription, on level transitions, and every 30s as heartbeat
- **`DropAccumulator`** — per-type counters for rate-limit drops + a single counter for broadcast lag; flushes to `DroppedEventsSignal` every 30s (or when count exceeds 100)
- **`EventRateLimiter`** — per-type token buckets. **Lazy-refill model** (no background timer task): `try_acquire(event_type, level)` reads the atomic `last_refill_at: AtomicU64` (nanoseconds since boot via `Instant::now()` subtraction) and atomic `tokens: AtomicI64`, computes elapsed × refill_rate_per_sec(level), CAS-adds up to bucket capacity, then attempts to decrement one token. All atomic ops use `Relaxed`/`AcqRel` as appropriate — no `Mutex` on the hot path. Refill rate is re-read per call from `LoadPolicy` output (no cached stale rate). ~80 LoC; external crate (`governor`) avoided to keep dep surface minimal

### Shared `SystemMonitor` port

Reuses existing `Arc<dyn SystemMonitor>` from `oneshim-core::ports::monitor` — specifically the `SysInfoMonitor` adapter in `oneshim-monitor` that already collects CPU/memory via `sysinfo` crate. No new measurement loop introduced.

**Actual trait surface** (quoted from `crates/oneshim-core/src/ports/monitor.rs`):

```rust
#[async_trait]
pub trait SystemMonitor: Send + Sync {
    async fn collect_metrics(&self) -> Result<SystemMetrics, CoreError>;
}
```

The streaming RPCs call `system_monitor.collect_metrics().await?` to get a fresh `SystemMetrics` snapshot, then pass it to `LoadPolicy::classify(&metrics)`. `SystemMetrics` fields used: `cpu_usage: f32` (0.0–100.0 system-wide), `memory_total: u64`, `memory_used: u64`.

**`ServerLoadHint` proto field population** (so both hint consumers and spec readers know where the numbers come from):
- `cpu_pct` ← `metrics.cpu_usage` (direct copy)
- `memory_pct` ← `(metrics.memory_used as f64 / metrics.memory_total.max(1) as f64 * 100.0) as f32` (defensive `.max(1)` guards against uninitialized sysinfo returning 0/0)
- `suggested_interval_secs` ← enforcement floor for current level (Section §3 ladder) — 0 when level is LOW (no suggestion)
- `suggested_event_rate_limit` ← per-level Frame cap from the same ladder — 0 when level is LOW (no suggestion)
- `reason` ← `format!("{} (cpu={:.1}% mem={:.1}%)", level_tag, cpu_pct, memory_pct)` plus `"warmup"` prefix during warm-up
- `emitted_at` ← `Utc::now()` converted to `google.protobuf.Timestamp`

### Storage access pattern (important)

`WebStorage` methods are **synchronous** (they wrap `rusqlite` which has no async API). V2a's `DashboardServiceImpl` uses `tokio::task::spawn_blocking` to keep the async runtime free:

```rust
let storage = self.storage.clone();
let records = tokio::task::spawn_blocking(move || storage.list_hourly_metrics_since(&from))
    .await
    .map_err(|e| Status::internal(format!("spawn_blocking join: {e}")))?
    .map_err(|e| Status::internal(format!("list_hourly_metrics_since: {e}")))?;
```

V2b streaming follows the same pattern. Every DB access inside a stream iteration is wrapped in `spawn_blocking`.

### Required new WebStorage methods

V2a already has hourly-granularity aggregation (`list_hourly_metrics_since`). V2b streaming also needs sub-hour granularity for realtime / short-interval buckets. Two new synchronous methods to add to the `WebStorage` trait (PR-B1 scope):

```rust
/// Aggregate a single MetricBucket from raw system_metrics rows in
/// [from, to]. Returns zero-initialized bucket when the window is empty.
/// The MetricBucket fields map 1:1 to the proto message; the impl
/// averages cpu_usage / memory_used and sums keystrokes / clicks.
fn aggregate_metrics_window(
    &self,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<MetricBucketRecord, CoreError>;

/// Fetch the canonical DB row for an event signaled via RealtimeEvent,
/// so DashboardEvent carries DB-derived data (not raw event payload).
/// For frames: looks up `frames` table by id. For idle/ai_status:
/// reads the current latest value (these are "latest-state" events).
fn fetch_dashboard_event_source(
    &self,
    signal: &DashboardEventSignal,
) -> Result<DashboardEventRecord, CoreError>;
```

Where `DashboardEventSignal` is an internal enum `{ Frame(i64), Idle, AiRuntimeStatus }` mapping the RealtimeEvent kind into a minimal lookup key, and `MetricBucketRecord` / `DashboardEventRecord` are plain data records (not tied to proto).

If `aggregate_metrics_window` turns out to be performance-sensitive, a materialized-view or cached-aggregate optimization is tracked in Appendix B (v3).

### spawn entrypoint signature

Current V2a signature:
```rust
pub async fn serve_optional(port: u16, storage: Arc<dyn WebStorage>) { ... }
```

v2b replaces with struct:
```rust
pub struct GrpcSpawnConfig {
    pub port: u16,
    pub storage: Arc<dyn WebStorage>,
    pub system_monitor: Arc<dyn SystemMonitor>,
    pub event_tx: broadcast::Sender<RealtimeEvent>,
    pub integration_auth_token: Option<String>,
    pub load_policy: Arc<LoadPolicy>,
}

pub async fn serve_optional(config: GrpcSpawnConfig) { ... }
```

All fields `pub` + struct extensibility — future additions (v2c token, etc.) don't break callers.

---

## §4. Data flow

### `SubscribeMetrics` runtime sequence

```
1. Client → Server: SubscribeMetricsRequest { interval_secs, respect_server_hints }

2. Auth gate (§3 opt-out logic):
   - Respect=false + untrusted → force respect=true + warn-log
   - Respect=true or opt-out granted → proceed

3. Emit initial ServerLoadHint (current classify())

4. Stream loop — two modes (pseudocode; real calls use `spawn_blocking` for storage access):

   Realtime (interval_secs == 0):
     recv_result = broadcast::Receiver::recv().await
     match recv_result:
       Ok(RealtimeEvent::Metrics(_)) → wake, continue
       Ok(_other) → ignored (we only care about Metrics for wake-up)
       Err(RecvError::Lagged(_)) → no-op (Metrics fires on monitor tick, lag harmless here)
       Err(RecvError::Closed) → stream_end

     // Coalesce: drop additional queued wake-ups within a 10ms quiet window
     // before querying DB. Uses tokio::time::timeout on rx.recv() with 10ms.
     drain_wakeups_until_quiet(&mut rx, Duration::from_millis(10)).await;

     // Fresh classify using actual SystemMonitor trait
     metrics = system_monitor.collect_metrics().await?;  // SystemMetrics
     level = load_policy.classify(&metrics);
     hint_emitter.maybe_emit(level, &mut stream);

     // DB access via spawn_blocking (WebStorage is sync)
     storage = self.storage.clone();
     bucket_record = tokio::task::spawn_blocking(move || {
         storage.aggregate_metrics_window(now - Duration::from_secs(60), now)
     }).await??;

     // Enforce: skip emit if last emit was < effective_interval ago
     if now - last_emit >= effective_interval(level):
         yield Data(bucket_record.into_proto());
         last_emit = now;

   Interval (interval_secs > 0):
     effective_interval = enforcement(level).clamp(requested);  // see §3 table
     let mut ticker = tokio::time::interval(effective_interval);
     ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);  // avoid bursts after pause
     ticker.tick().await;

     metrics = system_monitor.collect_metrics().await?;
     level = load_policy.classify(&metrics);
     hint_emitter.maybe_emit(level, &mut stream);

     storage = self.storage.clone();
     bucket_record = tokio::task::spawn_blocking(move || {
         storage.aggregate_metrics_window(now - effective_interval, now)
     }).await??;
     yield Data(bucket_record.into_proto());

     // If level transition changes effective_interval,
     // recreate the ticker with the new period (next iteration).

5. Cleanup on stream drop:
   - async_stream generator Drop → broadcast::Receiver dropped
   - tokio task released
   - active_stream_counter decremented (§5 Fan-out)

**Per-stream state** (held in closure captured by `async_stream!`):
- `rx: broadcast::Receiver<RealtimeEvent>` (realtime mode only)
- `last_emit: Instant` (realtime mode only)
- `hint_emitter: HintEmitter` (per-stream level-tracking state)
- `effective_interval: Duration` (interval mode only, recomputed on level transition)
```

### `SubscribeEvents` runtime sequence

```
1. Client → Server: SubscribeEventsRequest { event_types, respect_server_hints }

2. Auth gate (same as above)

3. Emit initial ServerLoadHint

4. Stream loop (pseudocode):

   recv_result = broadcast::Receiver::recv().await
   match recv_result:
     Ok(event) →
       // V2b supports 3 variants. Metrics is owned by SubscribeMetrics.
       signal = match event {
         RealtimeEvent::Frame(f) => DashboardEventSignal::Frame(f.id),
         RealtimeEvent::Idle(_) => DashboardEventSignal::Idle,
         RealtimeEvent::AiRuntimeStatus(_) => DashboardEventSignal::AiRuntimeStatus,
         RealtimeEvent::Metrics(_) | RealtimeEvent::Ping => continue,  // handled elsewhere / ignored
       };
       event_type = signal.as_str();  // "frame" | "idle" | "ai_runtime_status"

       if event_types_filter non-empty AND event_type not in filter: continue

       // Classify + maybe emit hint
       metrics = system_monitor.collect_metrics().await?;
       level = load_policy.classify(&metrics);
       hint_emitter.maybe_emit(level, &mut stream);

       if rate_limiter.try_acquire(event_type, level):
         storage = self.storage.clone();
         record = tokio::task::spawn_blocking(move || {
             storage.fetch_dashboard_event_source(&signal)
         }).await??;
         yield Event(record.into_proto());
       else:
         drop_accumulator.record(event_type);

     Err(RecvError::Lagged(n)) →
       drop_accumulator.record_lag(n);

     Err(RecvError::Closed) →
       return;  // server shutdown

   // Tick-driven flush of drop_accumulator every 30s OR when count>=100
   if drop_accumulator.should_flush():
       yield Dropped(drop_accumulator.drain_signal());

5. Cleanup (same as SubscribeMetrics)

**Per-stream state**: `rx`, `rate_limiter`, `drop_accumulator`, `hint_emitter`, `event_types_filter: HashSet<String>`.
```

### Hint emission triggers (shared)

| Trigger | Rate |
|---|---|
| Stream opened | 1× (initial state) |
| `LoadLevel` transition | On transition (no rate limit) |
| Heartbeat | Every 30s even with same level |

30s heartbeat = "server alive + environment unchanged" signal. Client can treat no-hint-for-60s as stale stream.

### Configuration override propagation

```rust
// crates/oneshim-core/src/config/sections/network.rs
pub struct WebConfig {
    // ... existing ...
    #[serde(default)]
    pub grpc_load_thresholds: Option<LoadThresholds>,
    #[serde(default = "default_true")]
    pub grpc_streaming_enabled: bool,  // kill switch — see §7 rollback
}
```

`src-tauri/src/app_runtime_launch.rs` constructs `LoadPolicy` from config at spawn time:
```rust
let thresholds = config.web.grpc_load_thresholds.clone().unwrap_or_default();
let policy = Arc::new(LoadPolicy::new(thresholds));
```

---

## §5. Error handling & cancellation

### Client disconnection

| Scenario | Server behavior |
|---|---|
| Graceful (client closes stream) | `async_stream` generator Pending wake → Drop → `broadcast::Receiver` dropped, spawned tokio task released |
| Abrupt (TCP reset, network loss) | tonic propagates connection-level cancellation → same path, no extra code |
| Slow consumer | tonic's per-stream send buffer backpressures; `yield` blocks → natural pacing |

### Server shutdown

Existing `grpc::serve` shutdown path extended to drain streams:

```rust
tokio::select! {
    result = server.serve() => { ... }
    _ = shutdown_rx.recv() => {
        // Best effort: broadcast final hint to each active stream
        // (level=CRITICAL, reason="shutting_down") then close.
        // tokio cancellation propagation handles the rest.
    }
}
```

### Broadcast lag

```rust
match rx.recv().await {
    Ok(event) => { /* process */ }
    Err(RecvError::Lagged(n)) => {
        drop_accumulator.record_lag(n);
        // rx auto-recovers per broadcast channel semantics — continue loop
    }
    Err(RecvError::Closed) => {
        // event_tx dropped = server shutdown → stream_end
        return;
    }
}
```

### `SystemMonitor` failures

| Scenario | Response |
|---|---|
| `sysinfo` refresh fails (platform API error) | Use last good value; tolerate up to 5s stale |
| Stale > 5s | Lock to `MEDIUM` + hint with `reason="metrics_stale"` |
| Never initialized (bootup) | Warm-up window (§3) covers this — always `MEDIUM` for first 30s |

### Event ↔ DB race (Frame events specifically)

`RealtimeEvent::Frame(FrameUpdate { id, … })` is emitted by the capture pipeline **after** the row is persisted to `frames`, so `fetch_dashboard_event_source(Frame(id))` is expected to succeed. The stream still handles `CoreError::NotFound` defensively (log-and-skip, no stream abort) in case the ordering ever changes — e.g., the capture-save pipeline is refactored to send before commit. An integration test in PR-B3 mocks this ordering inversion by racing an event ahead of the DB insert and asserts the stream doesn't error out.

For `Idle` and `AiRuntimeStatus` events the lookup is "latest-state" (no specific row id), so the race window doesn't apply — the query returns whatever's current.

### Storage failures

Transient SQLite errors (WAL lock contention, busy) don't kill the stream:

```rust
let result = tokio::task::spawn_blocking({
    let storage = storage.clone();
    move || storage.aggregate_metrics_window(from, to)
})
.await
.map_err(|e| tonic::Status::internal(format!("spawn_blocking join: {e}")))?;

match result {
    Ok(record) => yield Data(record.into_proto()),
    Err(e) => {
        warn!(err.code = %e.code(), "dashboard aggregate query failed: {e}");
        // skip this tick, continue loop
        // on N consecutive failures (N=10), close stream with Status::Internal
    }
}
```

### Invalid requests

| Input | Response |
|---|---|
| `interval_secs = 999999` | Clamp to 60s ceiling + emit hint explaining |
| `event_types = ["unknown_future_type"]` | Silently ignored (forward-compat) |
| `event_types = []` | Treated as "all 3" (proto convention) |
| Malformed `Authorization` header | Opt-out rejected → enforce mode (stream not closed) |

### Fan-out (many concurrent subscribers)

10 subscribers, each requesting `interval_secs=0`, could cause 10× DB query per `event_tx::Metrics` tick.

**Mitigations (all in v2b scope)**:

1. **DB query coalescing** — within a 10ms window per stream, drain pending wake-ups before querying (see §4 realtime loop). Server-wide coalescing across subscribers is deferred to v3 (would require a shared query scheduler).

2. **Active stream cap** — `AppState.core.active_stream_counter: AtomicUsize`. Cap at 50 concurrent streams across both RPC types. Exceeding caps new subscriptions with `tonic::Status::ResourceExhausted`.

### Panic tolerance

Handler panics → tonic wraps in `Status::Internal` → client disconnected. Standard tonic behavior, no extra handling.

`async_stream!` internal panics are harder to test-catch — add `#[should_panic]` guards on the macro-boundary unit tests.

---

## §6. Testing strategy

### Test infrastructure

- **Time control**: `tokio::time::pause()` + `advance()` — no wall-clock `sleep`
- **Mock `SystemMonitor`**: atomics-backed mock for instant-switch load classification
- **In-memory storage**: `SqliteStorage::open_in_memory(30)` (same pattern as v2a tests)
- **`build_test_service()` helper** — DI factory with sensible mock defaults; per-test overrides

### Unit tests (in `crates/oneshim-web/src/grpc/` modules)

Approximate breakdown (~30 total):

```
load_policy_tests/        (8)
  classify_low_when_cpu_under_cpu_low_pct_and_mem_above_3gb
  classify_medium_between_cpu_low_and_cpu_medium_pct
  classify_high_between_cpu_medium_and_cpu_high_pct
  classify_critical_when_cpu_meets_cpu_high_pct
  classify_critical_when_free_mem_below_half_min
  enforced_interval_honors_floor_250ms
  enforced_interval_honors_ceiling_60s
  warmup_30s_forces_medium_regardless_of_metrics
  new_rejects_out_of_order_boundaries

hint_emitter_tests/        (4)
  emits_initial_hint_on_subscribe
  emits_on_level_transition
  emits_heartbeat_every_30s_unchanged_level
  no_duplicate_emit_in_burst_transitions

drop_accumulator_tests/    (4)
  accumulates_rate_limit_drops_per_type
  accumulates_broadcast_lag
  flush_emits_signal_with_time_window
  flush_resets_counters

rate_limiter_tests/        (4)
  frame_capped_at_10_per_sec_medium
  frame_capped_at_2_per_sec_high
  capacity_changes_on_level_transition
  drops_excess_into_accumulator

auth_gate_tests/           (5)
  opt_out_honored_on_loopback
  opt_out_honored_with_valid_bearer
  opt_out_rejected_external_no_token
  opt_out_rejected_external_wrong_token
  opt_out_rejected_malformed_auth_header
```

### Integration tests (in `crates/oneshim-web/tests/grpc_dashboard_integration.rs`)

Current: 10 (v1 + v2a). v2b adds ~12:

```
# SubscribeMetrics
grpc_dashboard_subscribe_metrics_realtime_emits_on_event_tx_tick
grpc_dashboard_subscribe_metrics_interval_5s_emits_every_5s
grpc_dashboard_subscribe_metrics_emits_initial_hint
grpc_dashboard_subscribe_metrics_enforces_clamp_under_high_load
grpc_dashboard_subscribe_metrics_honors_opt_out_on_localhost
grpc_dashboard_subscribe_metrics_closes_cleanly_on_client_drop

# SubscribeEvents
grpc_dashboard_subscribe_events_filters_by_type
grpc_dashboard_subscribe_events_emits_db_derived_payload
grpc_dashboard_subscribe_events_rate_limits_frames_under_high_load
grpc_dashboard_subscribe_events_emits_dropped_signal_with_breakdown
grpc_dashboard_subscribe_events_handles_broadcast_lag
grpc_dashboard_subscribe_events_empty_event_types_means_all
```

### Special scenario patterns

**Heartbeat timing** (30s cadence):
```rust
#[tokio::test(start_paused = true)]
async fn hint_heartbeat_fires_every_30s() {
    // ...setup...
    expect_hint(&mut stream).await;  // initial
    tokio::time::advance(Duration::from_secs(29)).await;
    assert!(poll_next_no_wait(&mut stream).is_none());
    tokio::time::advance(Duration::from_secs(2)).await;
    expect_hint(&mut stream).await;  // 30s heartbeat
}
```

**Warm-up window**:
```rust
#[tokio::test(start_paused = true)]
async fn warmup_forces_medium_regardless_of_actual_load() {
    let monitor = mock_monitor_with(0.1, 16.0);  // genuine LOW state
    // ...subscribe...
    assert_eq!(expect_hint(stream).await.load_level, Level::Medium);
    tokio::time::advance(Duration::from_secs(31)).await;
    assert_eq!(expect_hint(stream).await.load_level, Level::Low);
}
```

**Fan-out coalescing**:
```rust
#[tokio::test]
async fn concurrent_realtime_subscribers_coalesce_db_queries() {
    let spy = TrackingStorage::wrap(in_memory_storage().await);
    // ...10 parallel subscribers, 5 rapid-fire events...
    assert!(spy.metrics_query_count() < 5);  // coalesced
}
```

### CI impact

- `grpc_dashboard_integration`: 10 → ~22 tests
- Runtime: ~0.6s → ~1.5s (`time::pause` keeps it fast)
- No new cargo features, no new dependencies beyond `async-stream` (may already be pulled in by tonic)

---

## §6.1 Security & privacy posture

### PII boundaries for streaming payloads

V2b relies on the D5 iter-16 "PII filter at DB write-time" invariant: data that reaches SQLite has already passed through `PrivacyGateway` / `sanitize_title_with_level`. Every V2b streaming payload field sourced from SQLite inherits that sanitization for free.

Per-field PII risk analysis:

| Field | Source | Sanitization posture |
|---|---|---|
| `FrameEvent.app_name` | `frames.app_name` | Non-sensitive (process name only) |
| `FrameEvent.window_title` | `frames.window_title` | **Sanitized** at write-time via `sanitize_title_with_level` — same protection as the existing REST `/frames` + V2a `GetRecentFrames` wire already enjoys |
| `FrameEvent.trigger_type` | `frames.trigger_type` | Enum-like string, non-sensitive |
| `FrameEvent.importance` | Derived float, non-sensitive | N/A |
| `IdleEvent.{is_idle, idle_secs}` | Boolean + duration, non-sensitive | N/A |
| `AiRuntimeStatusEvent.{ocr_source, llm_source}` | Provider names ("ollama", "gpt-5"), non-sensitive | N/A |
| `AiRuntimeStatusEvent.{ocr_fallback_reason, llm_fallback_reason}` | Error messages — **could contain URLs/paths** | **Must be sanitized before emission**; reuse the same PII filter applied elsewhere when AI fallback reasons are displayed. Add a test that confirms no absolute paths leak through |
| `MetricBucket.*` | Aggregated numerics, non-sensitive | N/A |
| `ServerLoadHint.*` | Aggregated numerics + enum + reason string (server-authored, no user data) | N/A |
| `DroppedEventsSignal.*` | Counts + fixed reason strings, non-sensitive | N/A |

Because every field either maps to an already-sanitized DB column or to purely numeric/enum-shaped data, v2b does not introduce any new PII egress path. The only net-new sanitization need is `AiRuntimeStatusEvent.*_fallback_reason` — tracked as a PR-B3 acceptance criterion.

### Opt-out threat model

Opt-out is granted via (a) loopback binding or (b) matching `integration_auth_token`. Threats considered:

- **Same-host process reading loopback**: treated as trusted (OS-level process isolation is the user's PC's security boundary). Attacker with local code execution has larger attack surface than our rate limit anyway.
- **Remote attacker reading `integration_auth_token` from config on disk**: depends on config file permissions. Deployment guidance (to go in `docs/guides/grpc-client.md` during PR-B3) explicitly recommends chmod 0600 on the config directory; the config loader warns if permissions are looser. V2c's separate `grpc_auth_token` + TLS materially improves this by not reusing a token shared with the REST surface.
- **Token leakage via logs**: the token value is NEVER interpolated into log messages (only the hash or "present/absent" marker). Unit test asserts no token substring appears in tracing output on startup.

### Wire confidentiality

V2b binds loopback only (inherited from v1/v2a). Any remote bind will be rejected at `src-tauri/src/app_runtime_launch.rs` startup — v2c is required before the current config gate flips. Therefore v2b wire traffic never leaves the user's host, eliminating network-level confidentiality concerns within v2b scope.

---

## §7. Rollout & gating

### Feature flag

Existing `grpc-dashboard` feature extended — no new top-level feature. `async-stream = "0.3"` is already declared at the workspace root `Cargo.toml` (verified); `crates/oneshim-web` needs to add it to its `[dependencies]` under the `grpc-dashboard` feature gate. No new external dependencies otherwise.

### Config additions

Two new fields on `WebConfig` (both `#[serde(default)]` → existing configs unchanged):

```rust
pub struct WebConfig {
    // existing ...
    #[serde(default)]
    pub grpc_load_thresholds: Option<LoadThresholds>,
    #[serde(default = "default_true")]
    pub grpc_streaming_enabled: bool,  // kill switch
}
```

### V2a → v2b migration

- **Wire-level**: additive only. v2a clients 100% compatible (nested-vs-top-level proto message promotion doesn't change the wire representation of a `repeated` field — both serialize to the same length-delimited bytes at the owning field's tag)
- **Rust type path change**: `productivity_metrics_response::MetricBucket` → top-level `MetricBucket`. Server-side call sites (2-3 files) updated. External Rust clients need import adjustment — documented in `docs/guides/grpc-client.md`

### V2b → v2c transition plan (forward-compat)

V2b reuses `WebConfig.integration_auth_token` (existing REST token) as the T2-extended opt-out trust signal. v2c will introduce dedicated fields (`grpc_auth_token`, `grpc_auth_enabled`, `grpc_tls_enabled`, `grpc_allow_external`). V2b's opt-out check should be written so v2c's transition is purely additive:

```rust
fn configured_grpc_auth_token(config: &WebConfig) -> Option<&str> {
    // v2c will prefer config.grpc_auth_token when set; v2b only knows the
    // REST token. Wrapping this lookup in a helper means v2c adds one arm
    // without touching honor_opt_out's call site.
    config.grpc_auth_token.as_deref()  // v2c-only
        .or(config.integration_auth_token.as_deref())  // v2b fallback
}
```

In v2b, only the fallback arm exists. V2c introduces the preferred arm. No breaking change to `honor_opt_out`'s contract — signature stays `(respect_hints, remote_addr, auth_header, configured_token: Option<&str>)`.

### Rollback

In severity order:

1. **Runtime kill switch** — set `WebConfig.grpc_streaming_enabled = false`. `SubscribeMetrics`/`SubscribeEvents` return `tonic::Status::Unimplemented`. V2a still works.
2. **Feature flag off** — build with `--no-default-features` (drops whole `grpc-dashboard` surface)
3. **Git revert** — last resort, in the next RC

### Observability

Structured `tracing` logs:
```rust
info!(
    subscriber_id = %uuid,
    interval_secs = req.interval_secs,
    respect_hints = honor_opt_out,
    "dashboard SubscribeMetrics stream opened"
);

warn!(
    err.code = %e.code(),
    subscriber_id = %uuid,
    "dashboard aggregate DB query failed"
);
```

Key metrics (emitted as `tracing` spans; Prometheus/Grafana scrapes):

- `dashboard.active_streams` (gauge, labeled by RPC type)
- `dashboard.hint_emissions_total` (counter, labeled by level)
- `dashboard.drops_total` (counter, labeled by reason: `rate_limit` | `channel_lag`)
- `dashboard.db_query_duration` (histogram)

### Execution sequence (3 implementation PRs)

| PR | Scope | LoC | Effort |
|---|---|---|---|
| **PR-B1** | Proto changes + `MetricBucket` promotion + regenerate. **Add `aggregate_metrics_window` and `fetch_dashboard_event_source` to `WebStorage` trait** (sync fns + SQLite impls + trait-level tests). Server impl stubs (returns `Unimplemented`). V2a 10 integration tests pass unchanged. | ~550 | 0.75d |
| **PR-B2** | `SubscribeMetrics` implementation (realtime + interval modes + `LoadPolicy` + `HintEmitter` + safety floors + 6 integration tests + ~15 unit tests) | ~700 | 1d |
| **PR-B3** | **Emission-site wiring for `RealtimeEvent::{Frame, Idle, AiRuntimeStatus}`** (prerequisite — see §1 Prerequisites; ~3 call sites + 3 regression tests confirming SSE `/stream` now carries the new events). Then `SubscribeEvents` implementation (`EventRateLimiter` + `DropAccumulator` + broadcast lag handling + 6 integration tests + ~15 unit tests) + docs update (`grpc-client.md`) | ~650 | 1.25d |

Total: ~1900 LoC, ~3 dev days (PR-B1 bumped after storage-layer addition; PR-B3 bumped after adding prerequisite emission-site wiring for the three event types SubscribeEvents depends on).

PR order strictly sequential (B1 → B2 → B3) so each PR stays individually reviewable + testable.

### Release target

Post-v0.4.40 stable promotion. V2b lands in v0.4.41 RC series.

---

## Appendix A — Related work

- **PR #455** — D13 v1 proto foundation (introduced the service)
- **PR #456** — D13 v1 server implementation
- **PR #462** — D13 v1 integration test infrastructure (the pattern v2b extends)
- **PR #463** — `WebConfig.grpc_port` config field
- **PR #475** — D13 v2 roadmap (originating document for v2a/v2b/v2c scopes)
- **PR #476** — D13 v2a per-domain unary RPCs (merged 2026-04-21)
- **PR #477** — V3 proto convention cleanup tracker (companion doc)

## Appendix B — V3 backlog pointers

Items v2b knowingly defers to v3:

- Self-process CPU/memory in `LoadPolicy` classification (currently only system-wide signals)
- Hardware tier detection + tier-specific default thresholds
- Server-wide DB query scheduler (cross-subscriber coalescing): today each subscriber pays its own `aggregate_metrics_window` query; v3 can short-circuit within-tick duplicates at a shared scheduler layer
- `aggregate_metrics_window` materialized-view / cached-aggregate: when realtime subscriber count scales up, pre-computing rolling windows in SQLite (or an in-memory sliding window) avoids repeated `SUM`/`AVG` scans
- `EVENT_CHANNEL_CAPACITY = 256` tuning: current capacity suits the single SSE subscriber model. With 50 gRPC streams + bursty `Frame` events, `RecvError::Lagged` will fire more often. v2b copes via `DroppedEventsSignal`, but raising the capacity (e.g. 1024 or 4096) and/or moving to per-subscriber bounded channels is a straightforward v3 adjustment once real usage data is available
- Bidirectional streaming (dynamic interval renegotiation without disconnect)
- Write RPCs
- gRPC-web proxy + reflection service
- V2a proto drifts (enum prefix, string timestamp) — see dedicated cleanup tracker

## Appendix C — Open questions for implementation

These are acceptable to resolve during PR-B2/B3 review rather than blocking spec approval:

- Exact `HintEmitter` state machine for burst-transition suppression threshold (`0.5s`? `1s`?)
- Exact `DropAccumulator` flush interval (`30s` fixed vs adaptive to drop rate?)
- `active_stream_counter` cap (50 default) — may need tuning based on observed usage
- Whether to expose per-RPC subscriber-count metrics or aggregate them
