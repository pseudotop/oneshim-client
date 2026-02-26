# Consumer Contract API + Feature Flag Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `cargo build` succeed without server dependencies, and define client-owned API contracts for server integration.

**Architecture:** Add `server` and `grpc` feature flags to `oneshim-app`. Extract `BatchSink` trait to `oneshim-core` so scheduler doesn't depend on `oneshim-network` concrete types. Replace server domain proto files with client-specific Consumer Contract protos. Remove unnecessary `oneshim-network` dependency from `oneshim-suggestion`.

**Tech Stack:** Rust feature flags, tonic/prost (gRPC), async-trait

**Design Doc:** `docs/plans/2026-02-26-consumer-contract-api-design.md`

---

## Task 1: Remove `oneshim-network` dependency from `oneshim-suggestion`

`oneshim-suggestion` lists `oneshim-network` as a dependency but never imports it. Removing it allows `oneshim-suggestion` to remain a non-optional crate.

**Files:**
- Modify: `crates/oneshim-suggestion/Cargo.toml`

**Step 1: Remove the dependency**

```toml
# Before (line 7):
oneshim-network = { workspace = true }

# After: delete that line entirely
```

**Step 2: Verify build**

Run: `cargo check -p oneshim-suggestion`
Expected: SUCCESS (no imports from oneshim_network exist)

**Step 3: Commit**

```bash
git add crates/oneshim-suggestion/Cargo.toml
git commit -m "refactor: remove unused oneshim-network dep from oneshim-suggestion"
```

---

## Task 2: Add `BatchSink` port trait to `oneshim-core`

Scheduler currently depends on `BatchUploader` (concrete type from `oneshim-network`). Extract a trait so scheduler uses `Option<Arc<dyn BatchSink>>` instead.

**Files:**
- Create: `crates/oneshim-core/src/ports/batch_sink.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

**Step 1: Create the trait**

Create `crates/oneshim-core/src/ports/batch_sink.rs`:

```rust
//! 배치 이벤트 전송 포트 — 서버 동기화 추상화

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::event::Event;

/// 이벤트를 배치로 서버에 전송하는 포트.
/// `oneshim-network::BatchUploader`가 구현체.
#[async_trait]
pub trait BatchSink: Send + Sync {
    /// 이벤트를 전송 큐에 추가
    async fn enqueue(&self, event: Event) -> Result<(), CoreError>;

    /// 큐에 쌓인 이벤트를 서버로 플러시. 전송된 건수 반환.
    async fn flush(&self) -> Result<usize, CoreError>;

    /// 현재 큐 크기
    fn queue_len(&self) -> usize;
}
```

**Step 2: Export from mod.rs**

Add to `crates/oneshim-core/src/ports/mod.rs`:

```rust
pub mod batch_sink;
```

**Step 3: Verify build**

Run: `cargo check -p oneshim-core`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/oneshim-core/src/ports/batch_sink.rs crates/oneshim-core/src/ports/mod.rs
git commit -m "feat: add BatchSink port trait for server sync abstraction"
```

---

## Task 3: Implement `BatchSink` for `BatchUploader`

**Files:**
- Modify: `crates/oneshim-network/src/batch_uploader.rs`

**Step 1: Add trait implementation**

Add at the bottom of `batch_uploader.rs`:

```rust
use oneshim_core::ports::batch_sink::BatchSink;

#[async_trait]
impl BatchSink for BatchUploader {
    async fn enqueue(&self, event: Event) -> Result<(), CoreError> {
        self.push(event);
        Ok(())
    }

    async fn flush(&self) -> Result<usize, CoreError> {
        BatchUploader::flush(self).await
    }

    fn queue_len(&self) -> usize {
        self.len()
    }
}
```

Note: Verify the exact method names (`push`, `flush`, `len`) match `BatchUploader`'s public API. Read the file first.

**Step 2: Verify build**

Run: `cargo check -p oneshim-network`
Expected: SUCCESS

**Step 3: Commit**

```bash
git add crates/oneshim-network/src/batch_uploader.rs
git commit -m "feat: implement BatchSink trait for BatchUploader"
```

---

## Task 4: Update `scheduler.rs` for optional server components

Change `batch_uploader: Arc<BatchUploader>` and `api_client: Arc<dyn ApiClient>` to `Option` types using traits from `oneshim-core`.

**Files:**
- Modify: `crates/oneshim-app/src/scheduler.rs`

**Step 1: Update imports**

```rust
// Remove:
use oneshim_network::batch_uploader::BatchUploader;

// Add:
use oneshim_core::ports::batch_sink::BatchSink;
```

**Step 2: Update struct fields (lines 194-195)**

```rust
// Before:
    batch_uploader: Arc<BatchUploader>,
    api_client: Arc<dyn ApiClient>,

// After:
    batch_sink: Option<Arc<dyn BatchSink>>,
    api_client: Option<Arc<dyn ApiClient>>,
```

**Step 3: Update constructor (lines 214-215)**

```rust
// Before:
        batch_uploader: Arc<BatchUploader>,
        api_client: Arc<dyn ApiClient>,

// After:
        batch_sink: Option<Arc<dyn BatchSink>>,
        api_client: Option<Arc<dyn ApiClient>>,
```

And the assignment in `Self { ... }`:

```rust
// Before:
            batch_uploader,
            api_client,

// After:
            batch_sink,
            api_client,
```

**Step 4: Update `spawn_sync_loop` (line 578)**

```rust
// Before:
        let uploader4 = self.batch_uploader.clone();

// After:
        let uploader4 = self.batch_sink.clone();
```

And the flush call (lines 589-599):

```rust
// Before:
                        if egress4.is_enabled() {
                            match uploader4.flush().await {

// After:
                        if egress4.is_enabled() {
                            if let Some(ref sink) = uploader4 {
                                match sink.flush().await {
                                    Ok(count) => {
                                        if count > 0 {
                                            debug!("batch: {count}items sent");
                                        }
                                    }
                                    Err(e) => {
                                        warn!("batch failure: {e}");
                                    }
                                }
                            }
```

**Step 5: Update `spawn_heartbeat_loop` (line 631)**

```rust
// Before:
        let api = self.api_client.clone();
        // ...
                        if let Err(e) = api.send_heartbeat(&sid).await {

// After:
        let api = self.api_client.clone();
        // ...
        tokio::spawn(async move {
            let api = match api {
                Some(a) => a,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };
            // ... rest unchanged, using `api` directly
                        if let Err(e) = api.send_heartbeat(&sid).await {
```

**Step 6: Verify build**

Run: `cargo check -p oneshim-app`
Expected: May fail (main.rs still passes concrete types — that's OK, next task fixes it)

**Step 7: Commit**

```bash
git add crates/oneshim-app/src/scheduler.rs
git commit -m "refactor: scheduler accepts optional server components via traits"
```

---

## Task 5: Add feature flags to `oneshim-app/Cargo.toml`

**Files:**
- Modify: `crates/oneshim-app/Cargo.toml`

**Step 1: Make oneshim-network optional, add features**

```toml
# Change line 21 from:
oneshim-network = { workspace = true, features = ["grpc"] }

# To:
oneshim-network = { workspace = true, optional = true }

# Add features section after [dependencies]:
[features]
default = []
server = ["dep:oneshim-network"]
grpc = ["server", "oneshim-network/grpc"]
```

**Step 2: Verify standalone build**

Run: `cargo check -p oneshim-app`
Expected: FAIL (main.rs still imports oneshim_network unconditionally — next task fixes it)

**Step 3: Commit**

```bash
git add crates/oneshim-app/Cargo.toml
git commit -m "feat: add server/grpc feature flags, make oneshim-network optional"
```

---

## Task 6: Gate `main.rs` server code with `#[cfg(feature = "server")]`

This is the largest task. Gate all server-dependent imports, initialization, and runtime code.

**Files:**
- Modify: `crates/oneshim-app/src/main.rs`

**Step 1: Gate imports (lines 36-40)**

```rust
// Before:
use oneshim_network::auth::TokenManager;
use oneshim_network::batch_uploader::BatchUploader;
use oneshim_network::grpc::{GrpcConfig, UnifiedClient};
use oneshim_network::http_client::HttpApiClient;
use oneshim_network::sse_client::SseStreamClient;

// After:
#[cfg(feature = "server")]
use oneshim_network::auth::TokenManager;
#[cfg(feature = "server")]
use oneshim_network::batch_uploader::BatchUploader;
#[cfg(feature = "grpc")]
use oneshim_network::grpc::{GrpcConfig, UnifiedClient};
#[cfg(feature = "server")]
use oneshim_network::http_client::HttpApiClient;
#[cfg(feature = "server")]
use oneshim_network::sse_client::SseStreamClient;
```

**Step 2: Gate TokenManager + GrpcConfig + UnifiedClient initialization (lines 329-340)**

```rust
#[cfg(feature = "server")]
let (token_manager, api_client, sse_client, batch_uploader) = {
    let token_manager = Arc::new(TokenManager::new(&config.server.base_url));

    info!(
        "network configuration: grpc_auth={}, grpc_context={}, endpoint={}",
        config.grpc.use_grpc_auth, config.grpc.use_grpc_context, config.grpc.grpc_endpoint
    );

    #[cfg(feature = "grpc")]
    {
        let grpc_config = GrpcConfig::from_core_with_rest(&config.grpc, &config.server.base_url);
        let unified_client = Arc::new(UnifiedClient::new(
            grpc_config.clone(),
            token_manager.clone(),
        )?);

        if platform_connected_mode {
            // ... login flow (lines 342-366) ...
        }
    }

    #[cfg(not(feature = "grpc"))]
    {
        if platform_connected_mode {
            let email = std::env::var("ONESHIM_EMAIL")
                .unwrap_or_else(|_| "user@example.com".to_string());
            let password = std::env::var("ONESHIM_PASSWORD").unwrap_or_default();
            if let Err(e) = token_manager.login(&email, &password).await {
                warn!("login failure: {e}");
            }
        }
    }

    let api_client = Arc::new(HttpApiClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.request_timeout(),
    )?);

    let sse_client = Arc::new(SseStreamClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.server.sse_max_retry_secs,
    ));

    let batch_uploader = Arc::new(BatchUploader::new(
        api_client.clone(),
        session_id.clone(),
        100,
        3,
    ));

    (token_manager, api_client, sse_client, batch_uploader)
};
```

**Step 3: Update Scheduler construction**

```rust
let sched = Scheduler::new(
    // ... common params unchanged ...
    #[cfg(feature = "server")]
    Some(batch_uploader.clone() as Arc<dyn oneshim_core::ports::batch_sink::BatchSink>),
    #[cfg(not(feature = "server"))]
    None,
    #[cfg(feature = "server")]
    Some(api_client.clone() as Arc<dyn ApiClient>),
    #[cfg(not(feature = "server"))]
    None,
);
```

Note: `#[cfg]` on function call arguments is not supported. Instead, create local variables:

```rust
#[cfg(feature = "server")]
let batch_sink_opt: Option<Arc<dyn oneshim_core::ports::batch_sink::BatchSink>> =
    Some(batch_uploader.clone());
#[cfg(not(feature = "server"))]
let batch_sink_opt: Option<Arc<dyn oneshim_core::ports::batch_sink::BatchSink>> = None;

#[cfg(feature = "server")]
let api_client_opt: Option<Arc<dyn ApiClient>> = Some(api_client.clone());
#[cfg(not(feature = "server"))]
let api_client_opt: Option<Arc<dyn ApiClient>> = None;

let sched = Scheduler::new(
    // ... common params ...
    batch_sink_opt,
    api_client_opt,
);
```

**Step 4: Gate SuggestionReceiver (lines 439-444, 632-651)**

```rust
#[cfg(feature = "server")]
let receiver = SuggestionReceiver::new(
    sse_client.clone(),
    Some(notifier.clone()),
    suggestion_queue.clone(),
    suggestion_tx,
);

// ...

#[cfg(feature = "server")]
if platform_connected_mode {
    let sid = session_id.clone();
    tokio::spawn(async move {
        if let Err(e) = receiver.run(&sid).await {
            error!("suggestion received error: {e}");
        }
    });

    let bus = event_bus.clone();
    tokio::spawn(async move {
        while let Some(suggestion) = suggestion_rx.recv().await {
            // ... suggestion processing ...
        }
    });
}
```

**Step 5: Verify standalone build**

Run: `cargo check -p oneshim-app`
Expected: SUCCESS (standalone mode, no server deps)

**Step 6: Verify server build**

Run: `cargo check -p oneshim-app --features server`
Expected: SUCCESS (REST/SSE enabled)

**Step 7: Commit**

```bash
git add crates/oneshim-app/src/main.rs
git commit -m "feat: gate server code behind #[cfg(feature = \"server\")]"
```

---

## Task 7: Create Consumer Contract proto files

Replace server domain protos with client-specific API contracts.

**Files:**
- Create: `api/proto/oneshim/client/v1/auth.proto`
- Create: `api/proto/oneshim/client/v1/session.proto`
- Create: `api/proto/oneshim/client/v1/context.proto`
- Create: `api/proto/oneshim/client/v1/suggestion.proto`
- Create: `api/proto/oneshim/client/v1/health.proto`

**Step 1: Create directory**

```bash
mkdir -p api/proto/oneshim/client/v1
```

**Step 2: Create `auth.proto`**

```protobuf
syntax = "proto3";
package oneshim.client.v1;

// Client authentication contract.
// Minimal surface: token acquisition and refresh only.

service ClientAuth {
  rpc GetToken(GetTokenRequest) returns (TokenResponse);
  rpc RefreshToken(RefreshTokenRequest) returns (TokenResponse);
}

message GetTokenRequest {
  string identifier = 1;  // email or API key
  string credential = 2;  // password or secret
  string organization_id = 3;
}

message TokenResponse {
  string access_token = 1;
  string refresh_token = 2;
  int64 expires_in_secs = 3;
  string user_id = 4;
}

message RefreshTokenRequest {
  string refresh_token = 1;
}
```

**Step 3: Create `session.proto`**

```protobuf
syntax = "proto3";
package oneshim.client.v1;

import "google/protobuf/empty.proto";

service ClientSession {
  rpc CreateSession(CreateSessionRequest) returns (CreateSessionResponse);
  rpc EndSession(EndSessionRequest) returns (google.protobuf.Empty);
  rpc Heartbeat(HeartbeatRequest) returns (google.protobuf.Empty);
}

message CreateSessionRequest {
  string client_id = 1;
  map<string, string> metadata = 2;
}

message CreateSessionResponse {
  string session_id = 1;
  string user_id = 2;
  string client_id = 3;
  repeated string capabilities = 4;
}

message EndSessionRequest {
  string session_id = 1;
}

message HeartbeatRequest {
  string session_id = 1;
}
```

**Step 4: Create `context.proto`**

```protobuf
syntax = "proto3";
package oneshim.client.v1;

import "google/protobuf/timestamp.proto";

service ClientContext {
  rpc UploadBatch(UploadBatchRequest) returns (UploadBatchResponse);
}

message UploadBatchRequest {
  string session_id = 1;
  repeated ClientEvent events = 2;
  repeated FrameMetadata frames = 3;
}

message ClientEvent {
  string event_id = 1;
  string event_type = 2;
  google.protobuf.Timestamp timestamp = 3;
  map<string, string> payload = 4;
}

message FrameMetadata {
  string frame_id = 1;
  google.protobuf.Timestamp captured_at = 2;
  string app_name = 3;
  string window_title = 4;
  double importance = 5;
  bytes thumbnail = 6;
}

message UploadBatchResponse {
  int32 accepted_count = 1;
}
```

**Step 5: Create `suggestion.proto`**

```protobuf
syntax = "proto3";
package oneshim.client.v1;

import "google/protobuf/empty.proto";

service ClientSuggestion {
  rpc Subscribe(SubscribeRequest) returns (stream SuggestionEvent);
  rpc SendFeedback(SendFeedbackRequest) returns (google.protobuf.Empty);
}

message SubscribeRequest {
  string session_id = 1;
}

message SuggestionEvent {
  string suggestion_id = 1;
  string content = 2;
  Priority priority = 3;
  double confidence_score = 4;
  string category = 5;
}

enum Priority {
  PRIORITY_UNSPECIFIED = 0;
  LOW = 1;
  MEDIUM = 2;
  HIGH = 3;
  CRITICAL = 4;
}

message SendFeedbackRequest {
  string suggestion_id = 1;
  FeedbackAction action = 2;
  string comment = 3;
}

enum FeedbackAction {
  FEEDBACK_ACTION_UNSPECIFIED = 0;
  ACCEPTED = 1;
  REJECTED = 2;
  DEFERRED = 3;
}
```

**Step 6: Create `health.proto`**

```protobuf
syntax = "proto3";
package oneshim.client.v1;

service ClientHealth {
  rpc Ping(PingRequest) returns (PingResponse);
}

message PingRequest {}

message PingResponse {
  string server_version = 1;
  bool healthy = 2;
}
```

**Step 7: Commit**

```bash
git add api/proto/oneshim/client/v1/
git commit -m "feat: add Consumer Contract proto definitions (5 services)"
```

---

## Task 8: Update build.rs and proto module for new proto paths

**Files:**
- Modify: `crates/oneshim-network/build.rs`
- Modify: `crates/oneshim-network/src/proto/mod.rs`
- Delete: `crates/oneshim-network/src/proto/generated/oneshim.v1.*.rs`

**Step 1: Delete old generated files**

```bash
rm crates/oneshim-network/src/proto/generated/oneshim.v1.auth.rs
rm crates/oneshim-network/src/proto/generated/oneshim.v1.common.rs
rm crates/oneshim-network/src/proto/generated/oneshim.v1.user_context.rs
rm crates/oneshim-network/src/proto/generated/oneshim.v1.monitoring.rs
```

**Step 2: Update `build.rs`**

Replace the proto file paths section. The new proto root is `api/proto` and files are under `oneshim/client/v1/`.

```rust
// Replace the proto_files vector with:
let proto_files = vec![
    proto_root.join("oneshim/client/v1/auth.proto"),
    proto_root.join("oneshim/client/v1/session.proto"),
    proto_root.join("oneshim/client/v1/context.proto"),
    proto_root.join("oneshim/client/v1/suggestion.proto"),
    proto_root.join("oneshim/client/v1/health.proto"),
];
```

Note: Read the full `build.rs` to understand the tonic-prost-build vs tonic-build configuration. The key fix is also changing `tonic_prost::ProstCodec` to `tonic::codec::ProstCodec` if needed — check the builder config.

**Step 3: Update `proto/mod.rs`**

Replace with modules matching the new generated file names:

```rust
#![allow(clippy::all)]
#![allow(warnings)]

pub mod auth {
    include!("generated/oneshim.client.v1.rs");
}
```

Note: tonic-prost-build may generate a single file per package or multiple. Check the actual output directory after running `cargo build --features grpc` to determine the correct `include!()` paths.

If generated as one file per package:
```rust
pub mod auth {
    include!("generated/oneshim.client.v1.auth.rs");
}
pub mod session {
    include!("generated/oneshim.client.v1.session.rs");
}
pub mod context {
    include!("generated/oneshim.client.v1.context.rs");
}
pub mod suggestion {
    include!("generated/oneshim.client.v1.suggestion.rs");
}
pub mod health {
    include!("generated/oneshim.client.v1.health.rs");
}
```

If generated as a single file (all share `package oneshim.client.v1`):
```rust
pub mod client {
    include!("generated/oneshim.client.v1.rs");
}
```

**Step 4: Update gRPC client implementations**

Update `crates/oneshim-network/src/grpc/auth_client.rs`, `session_client.rs`, `context_client.rs` to use new proto types from `crate::proto::*` matching the new module structure.

This requires reading each gRPC client file and updating the proto type imports. Example:

```rust
// Before:
use crate::proto::auth::*;

// After (depends on module structure):
use crate::proto::auth::*;  // or crate::proto::client::*
```

**Step 5: Verify gRPC build**

Run: `cargo check -p oneshim-network --features grpc`
Expected: SUCCESS (new proto generated, clients updated)

**Step 6: Commit**

```bash
git add -A crates/oneshim-network/src/proto/ crates/oneshim-network/build.rs
git commit -m "feat: replace server domain protos with Consumer Contract definitions"
```

---

## Task 9: Update CI workflow

**Files:**
- Modify: `.github/workflows/ci.yml` (or `rust-ci.yml`)

**Step 1: Add feature matrix**

```yaml
jobs:
  check:
    strategy:
      matrix:
        features: ["", "server", "grpc"]
    steps:
      - uses: actions/checkout@v6
      - name: Cargo check
        run: |
          if [ -z "${{ matrix.features }}" ]; then
            cargo check --workspace
          else
            cargo check --workspace --features ${{ matrix.features }}
          fi
      - name: Cargo test
        run: |
          if [ -z "${{ matrix.features }}" ]; then
            cargo test --workspace
          else
            cargo test --workspace --features ${{ matrix.features }}
          fi
```

**Step 2: Update release workflow**

In `.github/workflows/release.yml`, the release builds should use `--features grpc` (full feature set):

```yaml
      - name: Build release binary
        run: cargo build --release -p oneshim-app --features grpc
```

**Step 3: Commit**

```bash
git add .github/workflows/
git commit -m "ci: add feature matrix for standalone/server/grpc builds"
```

---

## Task 10: Verify all feature combinations

**Step 1: Standalone (default)**

```bash
cargo check -p oneshim-app
cargo test -p oneshim-app
```

Expected: SUCCESS — no server dependencies compiled

**Step 2: Server (REST/SSE)**

```bash
cargo check -p oneshim-app --features server
cargo test -p oneshim-app --features server
```

Expected: SUCCESS — HTTP/SSE clients compiled, gRPC excluded

**Step 3: gRPC (full)**

```bash
cargo check -p oneshim-app --features grpc
cargo test -p oneshim-app --features grpc
```

Expected: SUCCESS — all features compiled including proto generation

**Step 4: Workspace-wide**

```bash
cargo check --workspace
cargo test --workspace
```

Expected: SUCCESS — default features only (standalone)

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat: Consumer Contract API + feature flag layering complete"
```

---

## Summary: File Change Map

| File | Action | Task |
|------|--------|------|
| `crates/oneshim-suggestion/Cargo.toml` | Remove `oneshim-network` dep | 1 |
| `crates/oneshim-core/src/ports/batch_sink.rs` | Create `BatchSink` trait | 2 |
| `crates/oneshim-core/src/ports/mod.rs` | Export `batch_sink` | 2 |
| `crates/oneshim-network/src/batch_uploader.rs` | Implement `BatchSink` | 3 |
| `crates/oneshim-app/src/scheduler.rs` | Optional server params | 4 |
| `crates/oneshim-app/Cargo.toml` | Feature flags | 5 |
| `crates/oneshim-app/src/main.rs` | `#[cfg(feature)]` gates | 6 |
| `api/proto/oneshim/client/v1/*.proto` | 5 consumer contract protos | 7 |
| `crates/oneshim-network/build.rs` | New proto paths | 8 |
| `crates/oneshim-network/src/proto/` | Replace generated code | 8 |
| `crates/oneshim-network/src/grpc/*.rs` | Update proto imports | 8 |
| `.github/workflows/*.yml` | Feature matrix | 9 |
