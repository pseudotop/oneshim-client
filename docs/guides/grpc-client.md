[English](./grpc-client.md) | [한국어](./grpc-client.ko.md)

# Rust gRPC Client Guide

> **Written**: 2026-02-04
> **Phase**: 36 (gRPC Client)
> **Related docs**: [oneshim-network crate](../crates/oneshim-network.md)
> **Governance**: [gRPC Governance Guide](./grpc-governance.md)
> **Compatibility Matrix**: [gRPC Compatibility Matrix](./grpc-compatibility-matrix.md)

## Overview

The ONESHIM Rust client provides a **tonic + prost** based gRPC client. Through Feature Flags, gRPC and REST can be selectively used, and on gRPC failure it automatically falls back to REST.

## Quick Start

### 1. Enable Feature Flag

```bash
# Build with gRPC support
cargo build -p oneshim-app --features grpc

# Or build oneshim-network only
cargo build -p oneshim-network --features grpc
```

### 2. Basic Usage

```rust
use oneshim_network::grpc::{GrpcConfig, UnifiedClient};
use oneshim_network::TokenManager;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create token manager
    let token_manager = Arc::new(TokenManager::new(
        "http://localhost:8000",
        "user@example.com",
        "password",
    ));

    // gRPC configuration
    let config = GrpcConfig {
        use_grpc_auth: true,
        use_grpc_context: true,
        grpc_endpoint: "http://localhost:50052".to_string(),
        rest_endpoint: "http://localhost:8000".to_string(),
        ..Default::default()
    };

    // Create UnifiedClient (gRPC + REST integrated)
    let client = UnifiedClient::new(config, token_manager);

    // Login
    let login_response = client.login("user@example.com", "password", None).await?;
    println!("Login successful: {}", login_response.user_id);

    Ok(())
}
```

## Configuration

### GrpcConfig

```rust
/// gRPC client configuration
#[derive(Debug, Clone)]
pub struct GrpcConfig {
    /// Whether to use gRPC for auth (Login, Logout, RefreshToken, ValidateToken)
    pub use_grpc_auth: bool,

    /// Whether to use gRPC for context (UploadBatch, SubscribeSuggestions, etc.)
    pub use_grpc_context: bool,

    /// gRPC server endpoint
    pub grpc_endpoint: String,

    /// REST API endpoint (for fallback)
    pub rest_endpoint: String,

    /// Connection timeout (seconds)
    pub connect_timeout_secs: u64,

    /// Request timeout (seconds)
    pub request_timeout_secs: u64,

    /// Whether to use TLS
    pub use_tls: bool,

    /// Whether to use mTLS (client cert authentication)
    pub mtls_enabled: bool,

    /// TLS domain name used for certificate validation (SNI)
    pub tls_domain_name: Option<String>,

    /// Optional CA cert PEM path
    pub tls_ca_cert_path: Option<String>,

    /// Client cert PEM path (required when mtls_enabled=true)
    pub tls_client_cert_path: Option<String>,

    /// Client key PEM path (required when mtls_enabled=true)
    pub tls_client_key_path: Option<String>,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            use_grpc_auth: false,      // Default: use REST
            use_grpc_context: false,   // Default: use REST
            grpc_endpoint: "http://127.0.0.1:50052".to_string(),
            rest_endpoint: "http://127.0.0.1:8000".to_string(),
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
            use_tls: false,
            mtls_enabled: false,
            tls_domain_name: None,
            tls_ca_cert_path: None,
            tls_client_cert_path: None,
            tls_client_key_path: None,
        }
    }
}
```

### mTLS Configuration Example

```json
{
  "grpc": {
    "use_grpc_auth": true,
    "use_grpc_context": true,
    "grpc_endpoint": "https://grpc.example.com:50051",
    "grpc_fallback_ports": [50052, 50053],
    "connect_timeout_secs": 10,
    "request_timeout_secs": 30,
    "use_tls": true,
    "mtls_enabled": true,
    "tls_domain_name": "grpc.example.com",
    "tls_ca_cert_path": "/etc/oneshim/ca.pem",
    "tls_client_cert_path": "/etc/oneshim/client.pem",
    "tls_client_key_path": "/etc/oneshim/client.key"
  }
}
```

### Environment Variable Configuration

```bash
# Enable gRPC
export GRPC_ENABLED=true
export GRPC_ENDPOINT=http://localhost:50052

# REST fallback endpoint
export REST_ENDPOINT=http://localhost:8000

# Industrial ASCII output (disable emojis)
export NO_EMOJI=1
```

## Client Modules

### GrpcAuthClient — Authentication Service

```rust
use oneshim_network::grpc::GrpcAuthClient;

let auth_client = GrpcAuthClient::new(&config).await?;

// Login
let response = auth_client.login(
    "user@example.com",
    "password123",
    Some("org-id"),
).await?;

// Token refresh
let response = auth_client.refresh_token(&refresh_token).await?;

// Token validation
let validation = auth_client.validate_token(&access_token).await?;

// Logout
auth_client.logout(&session_id).await?;
```

### GrpcSessionClient — Session Service

```rust
use oneshim_network::grpc::GrpcSessionClient;

let session_client = GrpcSessionClient::new(&config, token_manager).await?;

// Create session
let session = session_client.create_session(
    "client-123",
    DeviceInfo { os: "macOS".into(), ..Default::default() },
).await?;

// Heartbeat
session_client.heartbeat(&session.id, ClientStatus::Active).await?;

// End session
session_client.end_session(&session.id).await?;
```

### GrpcContextClient — Context Service

```rust
use oneshim_network::grpc::GrpcContextClient;

let context_client = GrpcContextClient::new(&config, token_manager).await?;

// Batch upload
let response = context_client.upload_batch(
    ContextBatchUploadRequest {
        session_id: session.id.clone(),
        events: vec![event1, event2],
        frames: vec![frame1],
        client_stats: Some(stats),
    },
).await?;

// Suggestion feedback
context_client.send_feedback(
    SuggestionFeedback {
        suggestion_id: "sugg-123".into(),
        accepted: true,
        ..Default::default()
    },
).await?;

// List suggestions
let suggestions = context_client.list_suggestions(
    ListSuggestionsRequest {
        session_id: session.id.clone(),
        suggestion_type: Some(SuggestionType::WorkGuidance),
        limit: 10,
        ..Default::default()
    },
).await?;
```

### Server Streaming — Real-time Suggestion Subscription

```rust
use oneshim_network::grpc::GrpcContextClient;
use futures::StreamExt;

// Subscribe to suggestion stream
let mut stream = context_client.subscribe_suggestions(
    &session_id,
    &client_id,
).await?;

// Suggestion reception loop
while let Some(result) = stream.next().await {
    match result {
        Ok(suggestion) => {
            println!("Suggestion received: {:?}", suggestion);
            // Process suggestion
            handle_suggestion(suggestion).await;
        }
        Err(e) => {
            eprintln!("Stream error: {}", e);
            // Reconnection logic
            break;
        }
    }
}
```

### GrpcHealthClient — Server Health Check

```rust
use oneshim_network::grpc::{GrpcHealthClient, ServingStatus};

// Connect Health Check client
let mut health = GrpcHealthClient::connect(config).await?;

// Check overall server status
let status = health.check("").await?;
match status {
    ServingStatus::Serving => println!("Server healthy"),
    ServingStatus::NotServing => println!("Server stopped"),
    _ => println!("Status unknown"),
}

// Quick health check (convenience method)
if health.is_healthy().await {
    println!("Server ready");
}

// Check specific service status
let auth_status = health.check("oneshim.v1.auth.AuthenticationService").await?;
println!("Auth service: {}", auth_status);

// Check all ONESHIM service statuses
let statuses = health.check_oneshim_services().await;
for s in statuses {
    println!("{}: {}", s.service, s.status);
}
// Example output:
// <server>: SERVING
// oneshim.v1.auth.AuthenticationService: SERVING
// oneshim.v1.auth.SessionService: SERVING
// oneshim.v1.user_context.UserContextService: SERVING
```

### Health Check Before Connection Pattern

```rust
use oneshim_network::grpc::{GrpcHealthClient, GrpcConfig, UnifiedClient};

async fn create_client_with_health_check(
    config: GrpcConfig,
    token_manager: Arc<TokenManager>,
) -> Result<UnifiedClient, CoreError> {
    // 1. Check server status via Health Check
    match GrpcHealthClient::connect(config.clone()).await {
        Ok(mut health) => {
            if health.is_healthy().await {
                info!("gRPC server healthy, using gRPC");
            } else {
                warn!("gRPC server NOT_SERVING, falling back to REST");
            }
        }
        Err(e) => {
            warn!("gRPC connection unavailable ({}), using REST", e);
        }
    }

    // 2. Create UnifiedClient (auto-fallback supported)
    Ok(UnifiedClient::new(config, token_manager))
}
```

## UnifiedClient — Integrated Client

### REST Fallback Mechanism

`UnifiedClient` automatically falls back to REST API on gRPC failure.

```rust
use oneshim_network::grpc::UnifiedClient;

let client = UnifiedClient::new(config, token_manager);

// gRPC tried first, REST fallback on failure
let response = client.upload_batch(request).await?;

// Internal behavior:
// 1. use_grpc_context == true → Try gRPC
// 2. gRPC fails (connection error, timeout, etc.) → Call REST API
// 3. REST also fails → Return error
```

### Fallback Scenarios

| Situation | gRPC | REST | Result |
|-----------|------|------|--------|
| Normal | ✅ | - | gRPC response |
| gRPC connection failure | ❌ | ✅ | REST response |
| Both fail | ❌ | ❌ | Error returned |
| Industrial environment (HTTP/2 blocked) | ❌ | ✅ | REST response |

### Features Without REST Support

Some features do not support REST fallback:

```rust
// Batch upload — frame data not supported via REST
let response = client.upload_batch(request).await;
// On gRPC failure: only events sent via REST, frames logged as warning

// Suggestion list — REST not supported
let suggestions = client.list_suggestions(request).await;
// On gRPC failure: empty list returned + warning log
```

## Error Handling

### CoreError Mapping

```rust
use oneshim_core::error::CoreError;

match client.login(email, password, org_id).await {
    Ok(response) => {
        // Success
    }
    Err(CoreError::Network(msg)) => {
        // Network connection error
        eprintln!("Network error: {}", msg);
    }
    Err(CoreError::RateLimit { retry_after }) => {
        // 429 Too Many Requests
        if let Some(duration) = retry_after {
            tokio::time::sleep(duration).await;
        }
    }
    Err(CoreError::ServiceUnavailable) => {
        // 503 Service Unavailable
        // Retry with backoff
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

### gRPC Status → CoreError Mapping

| gRPC Status | CoreError |
|-------------|-----------|
| `UNAVAILABLE` | `ServiceUnavailable` |
| `DEADLINE_EXCEEDED` | `Network("timeout")` |
| `UNAUTHENTICATED` | `Unauthorized` |
| `PERMISSION_DENIED` | `Forbidden` |
| `NOT_FOUND` | `NotFound` |
| `RESOURCE_EXHAUSTED` | `RateLimit` |

## Retry Logic

### Automatic Retry

`UnifiedClient` performs automatic retries for specific errors:

```rust
// Internal retry logic
async fn execute_with_retry<F, T>(&self, operation: F) -> Result<T, CoreError>
where
    F: Fn() -> Pin<Box<dyn Future<Output = Result<T, CoreError>>>>,
{
    let mut delay = Duration::from_secs(1);
    let max_delay = Duration::from_secs(30);
    let max_retries = 3;

    for attempt in 0..max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(CoreError::Network(_)) |
            Err(CoreError::RateLimit { .. }) |
            Err(CoreError::ServiceUnavailable) => {
                if attempt < max_retries - 1 {
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, max_delay);
                }
            }
            Err(e) => return Err(e),
        }
    }

    Err(CoreError::Network("Max retries exceeded".into()))
}
```

## Build and Test

### Feature Flag Build

```bash
# Build with gRPC
cargo build --features grpc

# Build without gRPC (REST only)
cargo build

# Run tests
cargo test --features grpc
```

### Proto Code Regeneration

```bash
# From the api directory
cd api
./scripts/generate.sh

# Rust code is auto-generated in build.rs
cargo build --features grpc
```

### Mock Server Testing

```bash
# Run server-side mock server
uv run python scripts/run_grpc_server.py

# Client tests
cargo test --features grpc -- --test-threads=1
```

## Industrial Environment Support

### HTTP/2 Blocked Environments

Some industrial environments may block HTTP/2:

```rust
// Enable automatic REST fallback
let config = GrpcConfig {
    use_grpc_auth: true,
    use_grpc_context: true,
    ..Default::default()
};

let client = UnifiedClient::new(config, token_manager);

// Automatically uses REST when HTTP/2 is blocked
let response = client.upload_batch(request).await?;
```

### ASCII Output Mode

```bash
# Disable emojis (industrial terminal compatibility)
NO_EMOJI=1 cargo run -p oneshim-app --features grpc
```

## References

- Proto definitions — `api/proto/oneshim/v1/` (see server repository)
- [Server API Specification](../migration/04-server-api.md) — REST + gRPC endpoints
- [Migration Phases](../migration/05-migration-phases.md) — Phase 36
- [tonic documentation](https://github.com/hyperium/tonic)
- [prost documentation](https://github.com/tokio-rs/prost)

---

_Last updated: 2026-02-04_
