# P3 Cross-Device Sync Phase 3b: Remote + LAN Transport — Design Spec

> Created: 2026-03-19
> Status: Draft
> Parent spec: [P3 Cross-Device Sync](2026-03-19-p3-cross-device-sync-design.md)
> Depends on: Phase 3a-2 (SyncEngine, ChangeExtractor, ChangeMerger, FileSyncTransport)

## 1. Goal

Implement two new `SyncTransport` adapters — `RemoteSyncTransport` (REST-based
cloud relay) and `LanSyncTransport` (mDNS + direct peer-to-peer over the local
network) — so users can sync their ONESHIM activity data without relying on a
shared filesystem. Both transports implement the existing `SyncTransport` trait
from `oneshim-core::ports::sync_transport` and are selected via
`SyncConfig::transport`. The `SyncEngine` remains unchanged.

### 1.1 Non-Goals

- Server-side sync relay implementation (the server endpoint is treated as an
  opaque REST API; this spec covers the client transport adapter only).
- NAT traversal or relay for cross-network LAN sync (same-subnet only).
- Multi-user sync (same single-user model as the parent spec).
- Changes to the `SyncEngine` orchestrator, `ChangeExtractor`, or `ChangeMerger`.

## 2. Design Decisions

| Item | Decision | Rationale |
|------|----------|-----------|
| Remote protocol | REST (JSON over HTTPS) | Matches existing `reqwest`-based HTTP patterns in `oneshim-network`; gRPC option deferred to avoid mandatory proto changes |
| LAN discovery | mDNS/DNS-SD (`_oneshim-sync._tcp.local.`) | Zero-config, works on all three platforms (Bonjour/Avahi/built-in) |
| LAN data transport | HTTPS (Axum) per-device | Reuses existing Axum dependency; TLS ensures confidentiality even on untrusted networks |
| LAN trust model | TOFU (Trust On First Use) + passphrase verification | No PKI needed; passphrase proves same-user ownership; cert fingerprint pinned after first handshake |
| mDNS crate | `mdns-sd` (0.12+) | Pure Rust, async-friendly, actively maintained, supports macOS/Windows/Linux |
| TLS cert generation | `rcgen` (0.14+) | Lightweight, self-signed cert creation in pure Rust |
| Retry strategy | Exponential backoff with jitter (reuse `resilience` module) | Consistent with all other network adapters in `oneshim-network` |
| Encryption layer | AES-256-GCM (same as `FileSyncTransport`) | Unified encryption across all transports; passphrase KDF shared |
| Auth for Remote | Bearer token OR API key (configurable) | Flexible — supports both self-hosted and managed endpoints |

## 3. Architecture

### 3.1 Transport Selection Flow

```
SyncConfig::transport
       │
       ├── File   → FileSyncTransport   (oneshim-storage, existing)
       ├── Remote → RemoteSyncTransport  (oneshim-network, this spec)
       └── Lan    → LanSyncTransport     (oneshim-network, this spec)
              │
              ├── LanDiscovery       (mDNS service advertisement + browse)
              ├── LanPeerServer      (Axum HTTPS listener for inbound pulls)
              └── LanPeerClient      (reqwest HTTPS for outbound push/pull)
```

### 3.2 Dependency Graph (Hexagonal Architecture)

```
oneshim-core  <──  oneshim-network
                   ├── RemoteSyncTransport  (impl SyncTransport)
                   └── LanSyncTransport     (impl SyncTransport)
                       ├── LanDiscovery     (mDNS)
                       ├── LanPeerServer    (Axum)
                       └── LanPeerClient    (reqwest)

oneshim-core  <──  oneshim-storage
                   └── FileSyncTransport    (impl SyncTransport, existing)

oneshim-core  <──  src-tauri
                   └── SyncEngine           (orchestrator, DI wiring)
```

No direct dependency between `oneshim-storage` and `oneshim-network`. The
`SyncEngine` in `src-tauri` holds an `Arc<dyn SyncTransport>` and is wired to
the concrete transport at startup based on `SyncConfig::transport`.

### 3.3 Crate Placement

```
crates/oneshim-network/
└── src/
    ├── sync/
    │   ├── mod.rs              # re-exports
    │   ├── remote_transport.rs # RemoteSyncTransport
    │   ├── lan_transport.rs    # LanSyncTransport (orchestrator)
    │   ├── lan_discovery.rs    # mDNS advertisement + browse
    │   ├── lan_server.rs       # Axum HTTPS pull/push server
    │   ├── lan_tls.rs          # Self-signed cert generation + TOFU pin store
    │   └── lan_crypto.rs       # Shared passphrase challenge-response
    └── lib.rs                  # + pub mod sync;
```

## 4. RemoteSyncTransport

### 4.1 Overview

A stateless HTTP client adapter that pushes/pulls encrypted `ChangeSet` payloads
to/from a configurable REST endpoint. The endpoint URL comes from
`SyncConfig::remote_endpoint`. Authentication is via Bearer token or API key
header.

### 4.2 REST API Contract

The remote sync endpoint MUST implement these three routes. This spec defines
the client's expectations; the server implementation is out of scope.

#### 4.2.1 Push Changes

```
POST /sync/push
Authorization: Bearer <token> | X-Api-Key: <key>
Content-Type: application/octet-stream

Body: AES-256-GCM encrypted ChangeSet (same format as FileSyncTransport)

Response:
  200 OK                — changeset accepted
  401 Unauthorized      — bad token/key
  409 Conflict          — stale watermark, re-pull needed
  429 Too Many Requests — rate limited (Retry-After header)
  503 Service Unavailable
```

#### 4.2.2 Pull Changes

```
GET /sync/pull?since_wall_ms=<u64>&since_counter=<u32>&device_id=<string>
Authorization: Bearer <token> | X-Api-Key: <key>

Response:
  200 OK
  Content-Type: application/octet-stream
  Body: AES-256-GCM encrypted ChangeSet (or empty body if no changes)

  204 No Content — no changes since the given watermark
  401 Unauthorized
  429 Too Many Requests
```

#### 4.2.3 Discover Peers

```
GET /sync/peers
Authorization: Bearer <token> | X-Api-Key: <key>

Response:
  200 OK
  Content-Type: application/json
  Body: [
    {
      "device_id": "abc-123",
      "device_name": "Work MacBook",
      "last_sync_at": "2026-03-19T12:00:00Z",
      "watermark": { "wall_ms": 1710859200000, "counter": 42, "device_id": "abc-123" }
    }
  ]

  401 Unauthorized
```

### 4.3 Config Extensions

New fields in `SyncConfig` (in `crates/oneshim-core/src/config/sections/sync.rs`):

```rust
/// Remote sync endpoint URL. Required when transport == Remote.
/// Example: "https://sync.example.com/api/v1"
#[serde(default)]
pub remote_endpoint: Option<String>,

/// Authentication mode for the remote endpoint.
#[serde(default)]
pub remote_auth: RemoteSyncAuth,
```

```rust
/// Authentication mode for remote sync transport.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteSyncAuth {
    /// Bearer token authentication (e.g., JWT from ONESHIM server login).
    #[default]
    BearerToken,
    /// Static API key (self-hosted scenarios).
    ApiKey,
}
```

The actual token/key value is stored in the OS keychain via the existing
`SecretStore` port (from `oneshim-core::ports::secret_store`), NOT in the
config file. The secret key name is `oneshim_sync_remote_token`.

### 4.4 Struct Definition

```rust
/// Remote sync transport — push/pull changesets via REST to a cloud endpoint.
pub struct RemoteSyncTransport {
    /// reqwest HTTP client (TLS-enabled).
    client: reqwest::Client,
    /// Base URL of the sync endpoint (e.g., "https://sync.example.com/api/v1").
    endpoint: String,
    /// Local device ID.
    local_device_id: String,
    /// Passphrase for AES-256-GCM encryption of changeset payloads.
    passphrase: String,
    /// Auth header name + value resolver.
    auth: RemoteSyncAuthProvider,
    /// Retry policy.
    retry_policy: RetryBackoffPolicy,
}
```

### 4.5 Retry and Error Handling

Reuse the existing `resilience` module from `oneshim-network`:

- **Retryable errors**: `CoreError::Network`, `CoreError::RequestTimeout`,
  `CoreError::ServiceUnavailable`, `CoreError::RateLimit`.
- **Non-retryable errors**: `CoreError::Auth` (401), `CoreError::Validation` (400).
- **Max retries**: 3 (same as `HttpApiClient`).
- **Backoff**: Exponential with jitter via `jittered_backoff_delay()`.
  Base delay 1s, max delay 30s.
- **Rate limit**: Honor `Retry-After` header via `extract_retry_after()`.

On all errors, the `SyncEngine` sync cycle continues to the next iteration
rather than crashing. The error is logged at `warn!` level.

### 4.6 Push Flow

```
SyncEngine::push(changeset)
  │
  ├── Serialize ChangeSet to JSON
  ├── Encrypt with AES-256-GCM (same as FileSyncTransport::encrypt)
  ├── Compress with zstd (optional, if payload > 1 KB)
  │
  ├── POST /sync/push
  │   ├── Headers: Authorization, Content-Type: application/octet-stream
  │   ├── Body: encrypted bytes
  │   └── On 409 Conflict → return Ok(()) — SyncEngine will re-pull
  │
  └── Retry on retryable errors (max 3 attempts)
```

### 4.7 Pull Flow

```
SyncEngine::pull(since_hlc)
  │
  ├── GET /sync/pull?since_wall_ms=X&since_counter=Y&device_id=Z
  │   ├── Headers: Authorization
  │   └── On 204 → return Ok(None)
  │
  ├── Decompress if compressed
  ├── Decrypt with AES-256-GCM (same as FileSyncTransport::decrypt)
  ├── Deserialize to ChangeSet
  │
  └── Return Ok(Some(changeset))
```

### 4.8 Peer Discovery

For remote transport, peer discovery delegates to the sync endpoint:

```rust
async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError> {
    let response = self.client
        .get(format!("{}/sync/peers", self.endpoint))
        .header(/* auth */)
        .send()
        .await?;
    // Parse JSON array of PeerInfo
}
```

## 5. LanSyncTransport

### 5.1 Overview

A peer-to-peer transport that discovers other ONESHIM devices on the local
network via mDNS and exchanges encrypted changesets directly over HTTPS.
Each device runs a lightweight Axum HTTPS server (LanPeerServer) that serves
its local changesets to peers, and uses reqwest (LanPeerClient) to push
changesets to discovered peers.

### 5.2 Component Architecture

```
Device A (LAN)                              Device B (LAN)
┌──────────────────────────┐              ┌──────────────────────────┐
│  LanSyncTransport        │              │  LanSyncTransport        │
│  ┌────────────────────┐  │              │  ┌────────────────────┐  │
│  │ LanDiscovery       │  │  mDNS        │  │ LanDiscovery       │  │
│  │ (mdns-sd)          │──┼──broadcast───┼──│ (mdns-sd)          │  │
│  └────────────────────┘  │              │  └────────────────────┘  │
│  ┌────────────────────┐  │   HTTPS      │  ┌────────────────────┐  │
│  │ LanPeerServer      │  │◄─────────────┼──│ LanPeerClient      │  │
│  │ (Axum, port N)     │  │              │  │ (reqwest)           │  │
│  └────────────────────┘  │              │  └────────────────────┘  │
│  ┌────────────────────┐  │   HTTPS      │  ┌────────────────────┐  │
│  │ LanPeerClient      │──┼──────────────┼─►│ LanPeerServer      │  │
│  │ (reqwest)           │  │              │  │ (Axum, port M)     │  │
│  └────────────────────┘  │              │  └────────────────────┘  │
└──────────────────────────┘              └──────────────────────────┘
```

### 5.3 mDNS Service Discovery

#### 5.3.1 Service Type

```
_oneshim-sync._tcp.local.
```

#### 5.3.2 TXT Record Fields

| Key | Value | Purpose |
|-----|-------|---------|
| `device_id` | UUID v4 | Stable device identifier |
| `device_name` | UTF-8 string | Human-readable name from SyncConfig |
| `port` | Integer | HTTPS port the LanPeerServer is listening on |
| `version` | `1` | Protocol version for forward compatibility |
| `fingerprint` | Hex SHA-256 | TLS certificate fingerprint for TOFU verification |

#### 5.3.3 Discovery Lifecycle

```
App startup
  │
  ├── Generate/load self-signed TLS cert (see 5.5)
  ├── Start LanPeerServer on ephemeral port
  ├── Register mDNS service:
  │     _oneshim-sync._tcp.local.
  │     hostname: {device_name}.local.
  │     port: {server_port}
  │     TXT: device_id, device_name, port, version, fingerprint
  │
  ├── Browse for peers:
  │     mdns_sd::ServiceDaemon::browse("_oneshim-sync._tcp.local.")
  │     → on ServiceEvent::ServiceResolved:
  │         store PeerInfo { device_id, device_name, addr, port, fingerprint }
  │     → on ServiceEvent::ServiceRemoved:
  │         remove PeerInfo
  │
  └── On app shutdown:
        Unregister mDNS service
        Stop LanPeerServer
```

#### 5.3.4 Crate Selection: `mdns-sd`

```toml
# crates/oneshim-network/Cargo.toml
mdns-sd = "0.12"  # Pure Rust, async, macOS + Windows + Linux
```

**Platform behavior**:
- **macOS**: Uses Bonjour (native mDNS, always available).
- **Windows**: Uses built-in mDNS responder (Windows 10 1903+).
- **Linux**: Uses socket-based mDNS (no Avahi daemon dependency; works even
  without Avahi installed, though Avahi improves interop with other services).

### 5.4 LAN Passphrase Verification (Challenge-Response)

Before exchanging changesets, two devices must prove they share the same sync
passphrase. This prevents unauthorized devices on the same LAN from reading
sync data.

#### 5.4.1 Protocol

```
Device A (client)                        Device B (server)
      │                                        │
      │  GET /sync/challenge                   │
      │───────────────────────────────────────►│
      │                                        │
      │  200 OK { "nonce": "<32-byte-hex>" }   │
      │◄───────────────────────────────────────│
      │                                        │
      │  POST /sync/verify                     │
      │  { "response": HMAC-SHA256(nonce, key) }│
      │  where key = Argon2id(passphrase, salt)│
      │───────────────────────────────────────►│
      │                                        │
      │  Server computes same HMAC, compares:  │
      │  200 OK { "verified": true }           │
      │  or 403 Forbidden                      │
      │◄───────────────────────────────────────│
```

- The KDF salt is derived deterministically from the `device_id` pair
  (sorted lexicographically, concatenated, then SHA-256 hashed to 16 bytes).
  This ensures both sides derive the same key without transmitting a salt.
- The nonce is a random 32-byte value, single-use, valid for 60 seconds.
- Verification result is cached per peer `device_id` for the duration of the
  session (until app restart or mDNS service removal).

#### 5.4.2 KDF Parameters

Same as `FileSyncTransport` (Argon2id, default parameters):

```rust
Argon2::default().hash_password_into(
    passphrase.as_bytes(),
    &derived_salt,
    &mut key_bytes, // 32 bytes
)
```

### 5.5 TLS with TOFU (Trust On First Use)

#### 5.5.1 Self-Signed Certificate Generation

On first LAN sync startup, each device generates a self-signed TLS certificate
using `rcgen`:

```rust
use rcgen::{CertificateParams, KeyPair};

fn generate_self_signed_cert(device_id: &str) -> (Vec<u8>, Vec<u8>) {
    let mut params = CertificateParams::new(vec![
        format!("{}.oneshim.local", device_id),
    ]).unwrap();
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + Duration::days(3650); // 10 years
    let key_pair = KeyPair::generate().unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    (cert.pem().into_bytes(), key_pair.serialize_pem().into_bytes())
}
```

```toml
# New dependency in crates/oneshim-network/Cargo.toml
rcgen = "0.14"
```

The certificate PEM and private key PEM are stored in the platform-specific
config directory:

```
{config_dir}/oneshim/
  lan_cert.pem       # Self-signed X.509 certificate
  lan_key.pem        # Private key (ECDSA P-256)
```

The SHA-256 fingerprint of the certificate's DER encoding is broadcast in the
mDNS TXT record (`fingerprint` field) and is used for TOFU verification.

#### 5.5.2 TOFU Pin Store

A simple SQLite table in the existing `oneshim-storage` database:

```sql
-- Migration V15 (or appended to V14 if not yet applied)
CREATE TABLE IF NOT EXISTS lan_peer_pins (
    device_id TEXT PRIMARY KEY,
    cert_fingerprint TEXT NOT NULL,   -- SHA-256 hex of DER-encoded cert
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    trust_revoked INTEGER NOT NULL DEFAULT 0
);
```

**TOFU protocol**:

1. On first connection to a new peer, the client compares the server's TLS
   certificate fingerprint with the `fingerprint` from the mDNS TXT record.
2. If they match AND the passphrase challenge-response succeeds, the fingerprint
   is persisted in `lan_peer_pins`.
3. On subsequent connections, the client verifies the server's cert fingerprint
   against the stored pin. If it does not match, the connection is rejected
   with `CoreError::Auth("LAN peer certificate mismatch (TOFU violation)")`.
4. Users can manually revoke a pin via the web dashboard (sets
   `trust_revoked = 1`), forcing re-verification on next connection.

#### 5.5.3 reqwest TLS Configuration for TOFU

The LAN peer client uses a custom `reqwest::Client` that accepts the specific
self-signed certificate of each known peer:

```rust
// Build a per-peer reqwest client that pins the expected cert
fn build_lan_client(peer_cert_pem: &[u8]) -> Result<reqwest::Client, CoreError> {
    let cert = reqwest::Certificate::from_pem(peer_cert_pem)
        .map_err(|e| CoreError::Network(format!("Invalid peer cert: {e}")))?;
    reqwest::Client::builder()
        .add_root_certificate(cert)
        .danger_accept_invalid_certs(false) // Strict: only accept pinned cert
        .build()
        .map_err(|e| CoreError::Network(format!("Failed to build LAN client: {e}")))
}
```

For the initial connection (before a pin exists), the client temporarily
accepts the self-signed cert to perform the passphrase challenge-response.
Only after successful verification is the pin stored.

### 5.6 LanPeerServer (Axum HTTPS)

Each device runs a lightweight Axum server to serve changesets to peers.

#### 5.6.1 Endpoints

```
GET  /sync/pull?since_wall_ms=X&since_counter=Y
  → Returns AES-256-GCM encrypted ChangeSet (or 204 No Content)
  → Requires valid passphrase verification (session token cookie)

POST /sync/push
  → Receives AES-256-GCM encrypted ChangeSet from a peer
  → 200 OK on success

GET  /sync/challenge
  → Returns { "nonce": "<hex>" } for passphrase verification

POST /sync/verify
  → Receives { "response": "<hmac-hex>" }, returns { "verified": true/false }
  → On success, sets a session cookie (HMAC-based, 1-hour TTL)

GET  /sync/info
  → Returns { "device_id": "...", "device_name": "...", "version": 1 }
  → No auth required (public endpoint for discovery confirmation)
```

#### 5.6.2 Server Lifecycle

```rust
pub struct LanPeerServer {
    /// Handle to shut down the server gracefully.
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// The port the server is listening on.
    port: u16,
    /// TLS cert fingerprint (SHA-256 hex).
    fingerprint: String,
}

impl LanPeerServer {
    /// Start the HTTPS server on an ephemeral port.
    /// Returns the bound port number.
    pub async fn start(
        tls_cert_pem: Vec<u8>,
        tls_key_pem: Vec<u8>,
        change_extractor: Arc<dyn ChangeExtractor>,
        change_merger: Arc<dyn ChangeMerger>,
        passphrase: String,
        local_device_id: String,
    ) -> Result<Self, CoreError>;

    /// Stop the server (called on app shutdown or transport switch).
    pub async fn stop(&mut self);
}
```

The server uses `axum_server::tls_rustls` or `tokio-rustls` for TLS
termination with the self-signed certificate.

#### 5.6.3 Port Selection

The server binds to `0.0.0.0:0` (ephemeral port) and reports the actual port
in the mDNS TXT record. This avoids port conflicts with the existing web
dashboard (port 10090) and other services.

#### 5.6.4 Request Rate Limiting

To prevent abuse on the LAN:
- Max 60 requests/minute per peer IP.
- Max 10 concurrent connections.
- Enforced via `tower::limit::RateLimitLayer` and `tower::limit::ConcurrencyLimitLayer`.

### 5.7 LanSyncTransport Implementation

```rust
/// LAN peer-to-peer sync transport.
///
/// Combines mDNS discovery, an HTTPS server for inbound requests,
/// and an HTTPS client for outbound requests.
pub struct LanSyncTransport {
    /// mDNS service daemon handle.
    discovery: LanDiscovery,
    /// Local HTTPS server for serving changesets to peers.
    server: LanPeerServer,
    /// Discovered peers (updated by mDNS browse events).
    peers: Arc<RwLock<HashMap<String, LanPeerInfo>>>,
    /// Per-peer reqwest clients (with pinned certs).
    peer_clients: Arc<RwLock<HashMap<String, reqwest::Client>>>,
    /// Local device ID.
    local_device_id: String,
    /// Passphrase for encryption + challenge-response.
    passphrase: String,
    /// Retry policy.
    retry_policy: RetryBackoffPolicy,
}
```

#### 5.7.1 Extended PeerInfo for LAN

```rust
/// LAN-specific peer information (extends PeerInfo with network details).
struct LanPeerInfo {
    /// Base PeerInfo (device_id, device_name, last_sync_at, watermark).
    info: PeerInfo,
    /// Peer's IP address (from mDNS resolution).
    addr: IpAddr,
    /// Peer's HTTPS port (from mDNS TXT record).
    port: u16,
    /// Peer's TLS cert fingerprint (from mDNS TXT record).
    fingerprint: String,
    /// Whether passphrase has been verified this session.
    verified: bool,
}
```

#### 5.7.2 SyncTransport Trait Implementation

**Push**: Fan-out to ALL discovered (and verified) peers:

```rust
async fn push(&self, changes: &ChangeSet) -> Result<(), CoreError> {
    let peers = self.peers.read().clone();
    let mut errors = Vec::new();

    for (device_id, peer) in &peers {
        if !peer.verified {
            // Skip unverified peers — they will be verified on next discover_peers call
            continue;
        }
        match self.push_to_peer(peer, changes).await {
            Ok(()) => debug!(peer = %device_id, "LAN push succeeded"),
            Err(e) => {
                warn!(peer = %device_id, error = %e, "LAN push failed");
                errors.push(e);
            }
        }
    }

    // Push succeeds if at least one peer received the changeset,
    // or if there are no peers (changeset will be served via pull later).
    if !peers.is_empty() && errors.len() == peers.len() {
        Err(CoreError::Network("All LAN peers unreachable".into()))
    } else {
        Ok(())
    }
}
```

**Pull**: Query the first available verified peer (round-robin):

```rust
async fn pull(&self, since: &Hlc) -> Result<Option<ChangeSet>, CoreError> {
    let peers = self.peers.read().clone();

    for (device_id, peer) in &peers {
        if !peer.verified {
            continue;
        }
        match self.pull_from_peer(peer, since).await {
            Ok(changeset) => return Ok(changeset),
            Err(e) => {
                warn!(peer = %device_id, error = %e, "LAN pull failed, trying next peer");
                continue;
            }
        }
    }

    // No peers available or all failed
    Ok(None)
}
```

**Discover peers**: Return all mDNS-discovered peers:

```rust
async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError> {
    let peers = self.peers.read();
    Ok(peers.values().map(|p| p.info.clone()).collect())
}
```

### 5.8 Config Extensions for LAN

New fields in `SyncConfig`:

```rust
/// Port for the LAN sync HTTPS server. 0 = ephemeral (auto-assigned).
/// Default: 0.
#[serde(default)]
pub lan_port: u16,

/// Whether to advertise this device via mDNS for LAN discovery.
/// Default: true (when transport == Lan).
#[serde(default = "default_true")]
pub lan_advertise: bool,
```

## 6. Security Model

### 6.1 Encryption Layers

| Transport | At-rest encryption | In-transit encryption | Key management |
|-----------|-------------------|----------------------|----------------|
| File | AES-256-GCM (passphrase-derived) | N/A (filesystem) | User passphrase via Argon2id KDF |
| Remote | AES-256-GCM (passphrase-derived) | TLS 1.2+ (HTTPS) | User passphrase + Bearer/API key |
| LAN | AES-256-GCM (passphrase-derived) | TLS 1.2+ (self-signed + TOFU) | User passphrase + TOFU cert pins |

All three transports share the same AES-256-GCM encryption functions from
`FileSyncTransport`. The encrypted payload is opaque to the transport layer.

### 6.2 Threat Model

| Threat | Remote mitigation | LAN mitigation |
|--------|-------------------|----------------|
| Eavesdropping | TLS (HTTPS) | TLS (self-signed + TOFU) |
| Man-in-the-middle | TLS CA verification | TOFU cert pinning + passphrase challenge |
| Unauthorized peer | Bearer token / API key | Passphrase challenge-response |
| Replay attack | Server-side nonce/watermark validation | Nonce + HLC watermark monotonicity |
| Compromised endpoint | E2E encryption (server sees only ciphertext) | E2E encryption (all payloads encrypted) |
| Rogue device on LAN | Passphrase required for data exchange | Passphrase + TOFU prevents access |

### 6.3 Passphrase Sharing

The user enters the same passphrase on all their devices. This passphrase is:
- **Never transmitted** over the network (only an HMAC of a nonce is sent).
- **Never stored in plaintext** on disk (only the Argon2id hash is stored in
  `SyncConfig::passphrase_hash` for local verification on setup).
- **Held in memory** only while the `SyncEngine` is alive (same as `FileSyncTransport`).

### 6.4 Consent Gate

All transports are gated by the same consent checks from the parent spec:

```rust
if !config.sync.enabled {
    return Ok(());
}
if !consent_manager.is_permitted(|p| p.cross_device_sync) {
    return Ok(());
}
```

No additional consent fields are needed for Remote or LAN transports. The
`cross_device_sync` permission covers all transport types equally.

## 7. Error Handling

### 7.1 Principles

- Network failures MUST NOT crash the sync loop. All errors are caught and
  logged at `warn!` level.
- The `SyncEngine` continues to the next sync cycle on any transport error.
- Transient errors (timeouts, connection refused, rate limits) are retried
  with exponential backoff.
- Permanent errors (auth failure, TOFU violation) are logged at `error!`
  level and surfaced to the user via the web dashboard sync health panel.

### 7.2 Error Mapping

| HTTP Status | CoreError variant | Retryable? |
|------------|-------------------|------------|
| 200, 204 | (success) | N/A |
| 400 | `Validation` | No |
| 401 | `Auth` | No |
| 403 | `Auth` | No |
| 404 | `NotFound` | No |
| 409 | (handled as conflict, re-pull) | No (but triggers re-pull) |
| 429 | `RateLimit` | Yes (honor Retry-After) |
| 500 | `Internal` | Yes |
| 503 | `ServiceUnavailable` | Yes |
| Connection refused / timeout | `Network` / `RequestTimeout` | Yes |

### 7.3 LAN-Specific Errors

| Scenario | Behavior |
|----------|----------|
| mDNS daemon fails to start | Log error, fall back to manual peer address config |
| Peer goes offline (mDNS removal) | Remove from peer list, skip on next push/pull |
| TOFU violation (cert fingerprint mismatch) | Reject connection, log `error!`, notify user |
| Passphrase mismatch | Return 403, log `warn!`, do not retry |
| All peers unreachable | `push()` returns error; `pull()` returns `Ok(None)` |

## 8. Platform Considerations

### 8.1 mDNS Platform Support

| Platform | mDNS Implementation | Notes |
|----------|-------------------|-------|
| macOS | Bonjour (built-in) | Zero config, always works |
| Windows | Built-in mDNS (Win10 1903+) | Works out of the box on modern Windows |
| Linux | Socket-based (via `mdns-sd`) | No Avahi dependency; firewall must allow UDP 5353 |

### 8.2 Firewall Considerations

LAN sync requires:
- **UDP port 5353** (mDNS multicast) — outbound and inbound.
- **TCP port N** (LanPeerServer HTTPS) — inbound from LAN only.

On macOS, the system firewall prompts the user automatically. On Linux, the
user may need to configure `iptables`/`ufw`. On Windows, the app should
register a firewall exception during installation.

The web dashboard sync settings panel should display a note about firewall
requirements when LAN transport is selected.

### 8.3 Network Interface Selection

The mDNS service is advertised on all active network interfaces by default.
The `mdns-sd` crate handles multi-interface broadcasting automatically. If the
user is on multiple networks (e.g., Wi-Fi + Ethernet), peers on either network
are discoverable.

## 9. Dependencies

### 9.1 New Workspace Dependencies

```toml
# Cargo.toml (workspace root) — [workspace.dependencies]
mdns-sd = "0.12"           # mDNS/DNS-SD service discovery
rcgen = "0.14"             # Self-signed TLS certificate generation
tokio-rustls = "0.26"      # TLS for Axum server (LAN peer server)
rustls-pemfile = "2"       # PEM parsing for TLS cert/key loading
```

### 9.2 oneshim-network Cargo.toml Changes

```toml
[dependencies]
# ... existing dependencies ...

# Sync transports (Phase 3b)
aes-gcm = "0.10"           # AES-256-GCM encryption (shared with oneshim-storage)
argon2 = "0.5"             # Argon2id KDF (shared with oneshim-storage)
hmac = { workspace = true } # HMAC-SHA256 for LAN challenge-response
mdns-sd = { workspace = true, optional = true }
rcgen = { workspace = true, optional = true }
tokio-rustls = { workspace = true, optional = true }
rustls-pemfile = { workspace = true, optional = true }

[features]
default = []
grpc = [...]  # existing
lan-sync = ["mdns-sd", "rcgen", "tokio-rustls", "rustls-pemfile"]
```

The `lan-sync` feature flag keeps the LAN dependencies optional. Remote sync
uses only `reqwest` (already a dependency) and the new `aes-gcm`/`argon2`
crates.

## 10. Testing Strategy

### 10.1 Unit Tests

| Component | Tests | Notes |
|-----------|-------|-------|
| `RemoteSyncTransport` | Push/pull/discover with `mockito` mock server | Same pattern as `HttpApiClient` tests |
| `RemoteSyncTransport` | Retry behavior on 429/503 | Verify exponential backoff |
| `RemoteSyncTransport` | Auth header injection (Bearer, ApiKey) | |
| Encryption roundtrip | Reuse `FileSyncTransport::encrypt/decrypt` | Verify cross-transport compatibility |
| `LanDiscovery` | Service registration + browse | In-process mDNS (loopback) |
| `LanPeerServer` | Endpoint responses (challenge, verify, pull, push) | Use `axum::test` helpers |
| TOFU pin store | Insert, verify, revoke, fingerprint mismatch | SQLite in-memory |
| Challenge-response | HMAC verification, nonce expiry, wrong passphrase | Pure crypto tests |

### 10.2 Integration Tests

| Scenario | Setup | Validation |
|----------|-------|------------|
| Remote push-pull roundtrip | Mock HTTP server | Changeset integrity after encrypt/decrypt |
| LAN two-device simulation | Two `LanSyncTransport` instances on loopback | Full discovery + verify + push + pull |
| Transport selection | Set `SyncConfig::transport` to each variant | Correct transport instantiated |
| Auth failure | Mock 401 response | `CoreError::Auth` returned, no retry |
| TOFU violation | Swap cert after pinning | Connection rejected |

### 10.3 Manual Testing Checklist

- [ ] Remote sync with self-hosted endpoint (nginx reverse proxy)
- [ ] Remote sync with auth token expiry and refresh
- [ ] LAN sync between macOS + macOS
- [ ] LAN sync between macOS + Windows
- [ ] LAN sync between macOS + Linux
- [ ] LAN sync with firewall blocking mDNS (graceful degradation)
- [ ] LAN sync with wrong passphrase (rejected, no data leak)
- [ ] LAN sync with device going offline mid-transfer (retry, no crash)
- [ ] Transport switch from File to Remote to LAN (no data loss)

## 11. Sync Health Metrics

Both transports expose metrics for the web dashboard sync health panel:

```rust
/// Sync transport health metrics (shared across all transports).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncHealthMetrics {
    /// Timestamp of last successful push.
    pub last_push_at: Option<String>,
    /// Timestamp of last successful pull.
    pub last_pull_at: Option<String>,
    /// Number of consecutive push failures.
    pub push_fail_streak: u32,
    /// Number of consecutive pull failures.
    pub pull_fail_streak: u32,
    /// Total changesets pushed since startup.
    pub total_pushed: u64,
    /// Total changesets pulled since startup.
    pub total_pulled: u64,
    /// Total bytes pushed since startup.
    pub total_bytes_pushed: u64,
    /// Total bytes pulled since startup.
    pub total_bytes_pulled: u64,
    /// Number of discovered peers.
    pub peer_count: usize,
    /// Active transport kind.
    pub transport: SyncTransportKind,
    /// Last error message (if any).
    pub last_error: Option<String>,
}
```

These metrics are stored in the `SyncEngine` (not the transport) and exposed
via the existing web dashboard REST API at `GET /api/sync/health`.

## 12. Web Dashboard Integration

### 12.1 Sync Settings Panel

The existing web dashboard sync settings (from Phase 3a-2) are extended with:

- **Transport selector**: Radio buttons for File / Remote / LAN.
- **Remote settings** (shown when Remote selected):
  - Endpoint URL text field.
  - Auth mode selector (Bearer Token / API Key).
  - Token/key input (masked, stored via SecretStore).
  - "Test Connection" button.
- **LAN settings** (shown when LAN selected):
  - Discovered peers list (auto-refreshing via SSE).
  - Passphrase input (masked).
  - "Re-scan Network" button.
  - Per-peer trust status (verified / unverified / revoked).
  - "Revoke Trust" button per peer.

### 12.2 Sync Health Panel

- Last push/pull timestamps.
- Peer count and status.
- Error count and last error message.
- Bytes transferred (push/pull).
- "Sync Now" button (triggers immediate sync cycle).

## 13. Migration Path

### 13.1 From Phase 3a-2 (FileSyncTransport)

Users on File transport can switch to Remote or LAN at any time via the web
dashboard. No data migration is needed because:
- The `ChangeSet` format is identical across all transports.
- The `SyncEngine` watermarks are per-peer, stored in `sync_peers` table.
- Switching transport creates a new peer entry for the new transport's
  peer discovery mechanism.

### 13.2 Schema Migration

If the `lan_peer_pins` table (section 5.5.2) is needed before V15 is
formalized, it can be created as a V14 addendum or deferred to V15:

```sql
-- V15: LAN peer trust pins
CREATE TABLE IF NOT EXISTS lan_peer_pins (
    device_id TEXT PRIMARY KEY,
    cert_fingerprint TEXT NOT NULL,
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    trust_revoked INTEGER NOT NULL DEFAULT 0
);
```

## 14. Phase Scope Breakdown

### Phase 3b-1: RemoteSyncTransport

- [ ] `SyncConfig` extensions (`remote_endpoint`, `remote_auth`)
- [ ] `RemoteSyncTransport` struct in `oneshim-network::sync::remote_transport`
- [ ] `SyncTransport` trait impl (push, pull, discover_peers)
- [ ] AES-256-GCM encrypt/decrypt (shared functions, extracted from FileSyncTransport)
- [ ] Auth header injection (Bearer token, API key via SecretStore)
- [ ] Retry with exponential backoff (reuse `resilience` module)
- [ ] Unit tests with `mockito`
- [ ] Integration test: push-pull roundtrip

### Phase 3b-2: LanSyncTransport

- [ ] `SyncConfig` extensions (`lan_port`, `lan_advertise`)
- [ ] `LanDiscovery` (mDNS service registration + browse via `mdns-sd`)
- [ ] TLS cert generation + storage (`rcgen`, `lan_cert.pem`, `lan_key.pem`)
- [ ] TOFU pin store (SQLite `lan_peer_pins` table, migration V15)
- [ ] Passphrase challenge-response protocol (HMAC-SHA256)
- [ ] `LanPeerServer` (Axum HTTPS with TLS, endpoints: challenge, verify, pull, push, info)
- [ ] `LanPeerClient` (reqwest with TOFU cert pinning)
- [ ] `LanSyncTransport` orchestrator (impl SyncTransport)
- [ ] Unit tests: discovery, TOFU, challenge-response, server endpoints
- [ ] Integration test: two-device simulation on loopback
- [ ] `lan-sync` feature flag in `oneshim-network`

### Phase 3b-3: Polish

- [ ] Web dashboard transport selector UI
- [ ] Web dashboard sync health panel
- [ ] `SyncHealthMetrics` collection in `SyncEngine`
- [ ] Firewall guidance in dashboard (per-platform notes)
- [ ] Manual cross-platform testing (macOS, Windows, Linux combinations)

## 15. Open Questions

1. **Multi-peer pull ordering**: When multiple LAN peers are available, should
   we pull from all of them (merge multiple changesets) or just the one with
   the highest watermark? Current design: pull from first available, iterate
   on next cycle.

2. **Remote endpoint specification**: Should ONESHIM provide a reference
   server implementation, or rely on users deploying their own? A minimal
   reference server (Python/FastAPI, matching the contract in section 4.2)
   would accelerate adoption.

3. **Certificate rotation**: Self-signed certs are generated with a 10-year
   validity. Should there be an automatic rotation mechanism, and if so, how
   do we update TOFU pins across peers?

4. **Bandwidth throttling**: The parent spec mentions "bandwidth throttling
   config" as a Phase 3b item. Should this be per-transport or global? Current
   design defers this to Phase 3b-3 as a `SyncConfig::max_bytes_per_cycle`
   field.

5. **Selective sync per transport**: Should different transports allow
   different table subsets (e.g., sync only segments via LAN but everything
   via Remote)? Current design: all transports sync the same tables per
   `SyncConfig`.

## 16. References

- [P3 Cross-Device Sync parent spec](2026-03-19-p3-cross-device-sync-design.md)
- [mdns-sd crate](https://crates.io/crates/mdns-sd) — Pure Rust mDNS/DNS-SD
- [rcgen crate](https://crates.io/crates/rcgen) — Rust certificate generation
- [RFC 6762](https://www.rfc-editor.org/rfc/rfc6762) — Multicast DNS
- [RFC 6763](https://www.rfc-editor.org/rfc/rfc6763) — DNS-Based Service Discovery
- [TOFU (Trust On First Use)](https://en.wikipedia.org/wiki/Trust_on_first_use) — SSH-style trust model
- [Argon2id](https://www.rfc-editor.org/rfc/rfc9106) — Password-Based Key Derivation
- [ADR-001: Rust Client Architecture Patterns](../../architecture/ADR-001-rust-client-architecture-patterns.md)
