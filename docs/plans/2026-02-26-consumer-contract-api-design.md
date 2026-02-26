# Consumer Contract API + Feature Flag Layering

**Date**: 2026-02-26
**Status**: Approved

## Problem

- `oneshim-app`이 `features = ["grpc"]`를 항상 활성화 → proto 깨지면 전체 빌드 실패
- 서버 도메인 proto(14개)를 공개하면 내부 서비스 계약 노출
- 오픈소스 사용자가 `cargo build` 시 즉시 실패

## Decision

### 1. Feature Flag 계층화

```toml
# oneshim-app/Cargo.toml
[features]
default = []                                    # standalone
server = ["dep:oneshim-network", "dep:oneshim-suggestion"]  # REST/SSE
grpc = ["server", "oneshim-network/grpc"]       # gRPC
```

| Command | Result | Target |
|---------|--------|--------|
| `cargo build` | standalone agent | open-source users |
| `cargo build --features server` | REST/SSE server sync | self-hosted |
| `cargo build --features grpc` | full gRPC | ONESHIM platform |

### 2. Consumer Contract API

Client-specific API contracts instead of server domain protos.

```
api/proto/oneshim/client/v1/
├── auth.proto        # GetToken, RefreshToken
├── session.proto     # Create, End, Heartbeat
├── context.proto     # UploadBatch
├── suggestion.proto  # Subscribe (streaming), SendFeedback
└── health.proto      # Ping
```

- Client owns its own proto definitions (5 files, minimal surface)
- Server fulfills these contracts
- Server internal domain protos (14 domains) remain private

### 3. Code Changes

| File | Change |
|------|--------|
| `api/proto/oneshim/client/v1/*.proto` | New: 5 client contract files |
| `crates/oneshim-network/src/proto/generated/` | Replace: server domain → client contract |
| `crates/oneshim-network/build.rs` | Update: proto path to `client/v1/` |
| `crates/oneshim-app/Cargo.toml` | Add: `server`/`grpc` features, optional deps |
| `crates/oneshim-app/src/main.rs` | Add: `#[cfg(feature = "server")]` gates |

### 4. main.rs Pattern

```rust
// Always (standalone)
let system_monitor = Arc::new(SysInfoMonitor::new());
let sqlite_storage = Arc::new(SqliteStorage::open(...)?);

// Server integration (compiled only with feature = "server")
#[cfg(feature = "server")]
if platform_connected_mode {
    let token_manager = Arc::new(TokenManager::new(...));
    let api_client = Arc::new(HttpApiClient::new(...)?);
}
```

### 5. Maintenance Workflow

```
Server API change:
  monorepo api/proto/ change
    → client-rust/api/proto/oneshim/client/v1/ update (subset only)
    → cargo build --features grpc to regenerate + verify
    → submodule commit → parent pointer update

Independent client development:
  cargo build → succeeds without server
```

### 6. CI Matrix

```yaml
strategy:
  matrix:
    features: ["", "server", "grpc"]
```
