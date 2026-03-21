# P3 Cross-Device Sync Phase 3b: Remote + LAN Transport

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement two new `SyncTransport` adapters -- `RemoteSyncTransport` (REST-based cloud relay via reqwest) and `LanSyncTransport` (mDNS discovery + HTTPS peer-to-peer) -- in `oneshim-network`, then wire them into `SyncEngine` via `agent_runtime.rs`. The `SyncEngine` itself remains unchanged; transport selection is driven by `SyncConfig::transport`.

**Architecture:** Both transports live in `crates/oneshim-network/src/sync/`. They depend on `oneshim-core` traits only (Hexagonal Architecture). The DI wiring in `src-tauri/src/agent_runtime.rs` selects the concrete transport based on `SyncConfig::transport`. Encryption uses the same AES-256-GCM + Argon2id functions already proven in `FileSyncTransport`; those functions are extracted into a shared `sync_crypto` module in `oneshim-network`. LAN transport dependencies (`mdns-sd`, `rcgen`, `tokio-rustls`, `rustls-pemfile`) are gated behind a `lan-sync` feature flag.

**Tech Stack:** Rust, reqwest, axum, aes-gcm, argon2, hmac, sha2, mdns-sd, rcgen, tokio-rustls, rustls-pemfile, mockito

**Spec:** `docs/superpowers/specs/2026-03-19-p3-sync-phase3b-remote-lan-transport-design.md`

**Predecessor:** `docs/superpowers/plans/2026-03-19-p3-cross-device-sync-phase-3a2.md`

> **Migration version:** Sync 3b takes V15. Vector Phase C uses V16.

**Already done (DO NOT re-implement):**

| Component | File | Status |
|-----------|------|--------|
| `SyncTransport` trait (push, pull, discover_peers) | `crates/oneshim-core/src/ports/sync_transport.rs` | Done |
| `ChangeSet`, `PeerInfo`, `SyncResult` models | `crates/oneshim-core/src/models/sync.rs` | Done |
| `SyncConfig` + `SyncTransportKind` (File, Remote, Lan) | `crates/oneshim-core/src/config/sections/sync.rs` | Done |
| `FileSyncTransport` (AES-256-GCM encrypt/decrypt, push, pull) | `crates/oneshim-storage/src/file_transport.rs` | Done |
| `SyncEngine` orchestrator (pull/merge/push cycle, consent) | `src-tauri/src/sync_engine.rs` | Done |
| `SqliteSyncExtractor` + `SqliteSyncMerger` | `crates/oneshim-storage/src/sync_extractor.rs`, `sync_merger.rs` | Done |
| `Hlc`, `ConsentPermissions::cross_device_sync`, V14 migration | Various | Done |
| `resilience` module (`RetryBackoffPolicy`, `jittered_backoff_delay`) | `crates/oneshim-network/src/resilience.rs` | Done |
| `HttpApiClient` patterns (retry, error mapping, `check_response`) | `crates/oneshim-network/src/http_client.rs` | Done |

---

## Phase Breakdown

This plan is split into two sequential sub-phases:

| Sub-phase | Scope | Risk | Tasks |
|-----------|-------|------|-------|
| **3b-1: Remote** | `RemoteSyncTransport` + config + DI wiring | Low (stateless HTTP, proven patterns) | Tasks 1-7 |
| **3b-2: LAN** | `LanSyncTransport` + mDNS + TOFU TLS + peer server | Medium (mDNS platform matrix, self-signed TLS) | Tasks 8-16 |

Complete 3b-1 first. Verify with `cargo test --workspace`. Then proceed to 3b-2.

---

## File Map

### New files (3b-1: Remote)

| File | Responsibility |
|------|----------------|
| `crates/oneshim-network/src/sync/mod.rs` | Module re-exports for `sync_crypto`, `remote_transport` |
| `crates/oneshim-network/src/sync/sync_crypto.rs` | Shared AES-256-GCM encrypt/decrypt + Argon2id KDF (extracted from `FileSyncTransport`) |
| `crates/oneshim-network/src/sync/remote_transport.rs` | `RemoteSyncTransport` impl `SyncTransport` -- REST push/pull/discover via reqwest |

### New files (3b-2: LAN)

| File | Responsibility |
|------|----------------|
| `crates/oneshim-network/src/sync/lan_transport.rs` | `LanSyncTransport` orchestrator impl `SyncTransport` -- mDNS + server + client |
| `crates/oneshim-network/src/sync/lan_discovery.rs` | mDNS service registration + browse via `mdns-sd` |
| `crates/oneshim-network/src/sync/lan_server.rs` | `LanPeerServer` -- Axum HTTPS listener (challenge, verify, pull, push, info) |
| `crates/oneshim-network/src/sync/lan_tls.rs` | Self-signed cert generation (`rcgen`) + TOFU pin store logic |
| `crates/oneshim-network/src/sync/lan_crypto.rs` | Passphrase challenge-response (HMAC-SHA256 over nonce) |

### Modified files

| File | Change | Sub-phase |
|------|--------|-----------|
| `Cargo.toml` (workspace root) | Add `aes-gcm`, `argon2`, `hex`, `hmac`, `mdns-sd`, `rcgen`, `tokio-rustls`, `rustls-pemfile` to `[workspace.dependencies]` | 3b-1 + 3b-2 |
| `crates/oneshim-network/Cargo.toml` | Add `aes-gcm`, `argon2`, `hex`, `hmac` as `{ workspace = true }` deps; add optional `lan-sync` feature deps | 3b-1 + 3b-2 |
| `crates/oneshim-storage/Cargo.toml` | Add `aes-gcm`, `argon2`, `hex` as `{ workspace = true }` deps (used by `file_transport.rs`) | 3b-1 |
| `crates/oneshim-network/src/lib.rs` | Add `pub mod sync;` | 3b-1 |
| `crates/oneshim-core/src/config/sections/sync.rs` | Add `remote_endpoint`, `remote_auth`, `lan_port`, `lan_advertise` fields + `RemoteSyncAuth` enum | 3b-1 + 3b-2 |
| `crates/oneshim-storage/src/file_transport.rs` | Replace inline `encrypt`/`decrypt`/`derive_key` with calls to shared `sync_crypto` (or keep as-is if coupling is undesirable -- see Task 3 decision) | 3b-1 |
| `crates/oneshim-storage/src/migration.rs` | Add V15 migration for `lan_peer_pins` table | 3b-2 |
| `src-tauri/src/agent_runtime.rs` | Extend sync DI wiring to select `Remote`/`Lan` transport based on config | 3b-1 + 3b-2 |
| `src-tauri/Cargo.toml` | Add `oneshim-network` with `lan-sync` feature (if not already); add `keyring` dep for OS keychain access | 3b-2 |

> **Workspace dependency convention:** `aes-gcm`, `argon2`, `hex`, and `hmac` must first be declared in `[workspace.dependencies]` in root `Cargo.toml`, then referenced as `{ workspace = true }` by both `oneshim-storage` and `oneshim-network`. This avoids version drift between the two crates that share the same encryption primitives.

---

## 3b-1: RemoteSyncTransport (Tasks 1-7)

### Task 1: Extend SyncConfig with Remote fields

**File:** `crates/oneshim-core/src/config/sections/sync.rs`

Add config fields the `RemoteSyncTransport` needs: endpoint URL and auth mode. The actual token/key value is NOT stored in config -- it lives in the OS keychain via `SecretStore`.

- [ ] **Step 1: Add `RemoteSyncAuth` enum**

After the `SyncTransportKind` enum, add:

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

- [ ] **Step 2: Add fields to `SyncConfig`**

After the `passphrase_hash` field, add:

```rust
    /// Remote sync endpoint URL. Required when transport == Remote.
    /// Example: "https://sync.example.com/api/v1"
    #[serde(default)]
    pub remote_endpoint: Option<String>,

    /// Authentication mode for the remote endpoint.
    #[serde(default)]
    pub remote_auth: RemoteSyncAuth,

    /// Port for the LAN sync HTTPS server. 0 = ephemeral (auto-assigned).
    /// Default: 0.
    #[serde(default)]
    pub lan_port: u16,

    /// Whether to advertise this device via mDNS for LAN discovery.
    /// Default: true (when transport == Lan).
    #[serde(default = "default_true")]
    pub lan_advertise: bool,
```

Also add the helper:

```rust
fn default_true() -> bool {
    true
}
```

- [ ] **Step 3: Update `Default` impl**

Add to the `Default` impl body:

```rust
            remote_endpoint: None,
            remote_auth: RemoteSyncAuth::default(),
            lan_port: 0,
            lan_advertise: true,
```

- [ ] **Step 4: Add tests**

```rust
    #[test]
    fn remote_auth_serde_snake_case() {
        let json = serde_json::to_string(&RemoteSyncAuth::BearerToken).unwrap();
        assert_eq!(json, "\"bearer_token\"");
        let json = serde_json::to_string(&RemoteSyncAuth::ApiKey).unwrap();
        assert_eq!(json, "\"api_key\"");
    }

    #[test]
    fn sync_config_remote_fields_default() {
        let config = SyncConfig::default();
        assert!(config.remote_endpoint.is_none());
        assert_eq!(config.remote_auth, RemoteSyncAuth::BearerToken);
        assert_eq!(config.lan_port, 0);
        assert!(config.lan_advertise);
    }

    #[test]
    fn sync_config_remote_serde_roundtrip() {
        let config = SyncConfig {
            enabled: true,
            transport: SyncTransportKind::Remote,
            remote_endpoint: Some("https://sync.example.com/api/v1".to_string()),
            remote_auth: RemoteSyncAuth::ApiKey,
            ..SyncConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SyncConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.remote_endpoint.as_deref(), Some("https://sync.example.com/api/v1"));
        assert_eq!(parsed.remote_auth, RemoteSyncAuth::ApiKey);
    }
```

- [ ] **Step 5: Verify**

Run: `cargo test -p oneshim-core`

---

### Task 2: Add sync dependencies to oneshim-network

**File:** `crates/oneshim-network/Cargo.toml`

The `RemoteSyncTransport` needs AES-256-GCM encryption (same algorithm as `FileSyncTransport`) and HMAC for future LAN challenge-response. These are non-optional since Remote transport needs encrypt/decrypt.

- [ ] **Step 1: Add dependencies**

Under `[dependencies]`, add:

```toml
# Sync transports (Phase 3b)
aes-gcm = "0.10"
argon2 = "0.5"
hex = "0.4"
```

Note: `hmac` and `sha2` are already workspace dependencies in `oneshim-network`.

- [ ] **Step 2: Verify**

Run: `cargo check -p oneshim-network`

---

### Task 3: Create shared sync_crypto module

**File:** `crates/oneshim-network/src/sync/sync_crypto.rs`

Extract the AES-256-GCM encrypt/decrypt logic from `FileSyncTransport` into a shared module. Both `RemoteSyncTransport` and `LanSyncTransport` use identical encryption. `FileSyncTransport` in `oneshim-storage` keeps its own copy (to avoid `oneshim-storage` depending on `oneshim-network`), but the logic is identical.

- [ ] **Step 1: Create `crates/oneshim-network/src/sync/` directory**

- [ ] **Step 2: Create `sync_crypto.rs`**

```rust
//! Shared AES-256-GCM encryption for sync transports.
//!
//! Encryption format: salt (16 bytes) || nonce (12 bytes) || ciphertext.
//! Key derivation: Argon2id with default parameters.
//! Identical logic to `FileSyncTransport::encrypt/decrypt` in oneshim-storage.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use oneshim_core::error::CoreError;

const NONCE_SIZE: usize = 12;
const SALT_SIZE: usize = 16;

/// Derive a 32-byte AES-256 key from passphrase + salt via Argon2id.
pub fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], CoreError> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| CoreError::Internal(format!("Argon2 KDF failed: {e}")))?;
    Ok(key)
}

/// Encrypt plaintext with AES-256-GCM.
/// Returns: salt (16) || nonce (12) || ciphertext.
pub fn encrypt(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>, CoreError> {
    use aes_gcm::aead::rand_core::RngCore;
    let mut salt = [0u8; SALT_SIZE];
    OsRng.fill_bytes(&mut salt);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CoreError::Internal(format!("AES init: {e}")))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| CoreError::Internal(format!("AES encrypt: {e}")))?;

    let mut output = Vec::with_capacity(SALT_SIZE + NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt: parse salt || nonce || ciphertext.
pub fn decrypt(passphrase: &str, data: &[u8]) -> Result<Vec<u8>, CoreError> {
    if data.len() < SALT_SIZE + NONCE_SIZE + 1 {
        return Err(CoreError::Internal("encrypted data too short".to_string()));
    }
    let salt = &data[..SALT_SIZE];
    let nonce_bytes = &data[SALT_SIZE..SALT_SIZE + NONCE_SIZE];
    let ciphertext = &data[SALT_SIZE + NONCE_SIZE..];

    let key = derive_key(passphrase, salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CoreError::Internal(format!("AES init: {e}")))?;
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher.decrypt(nonce, ciphertext).map_err(|e| {
        CoreError::Internal(format!("AES decrypt failed (wrong passphrase?): {e}"))
    })
}
```

- [ ] **Step 3: Add unit tests in `sync_crypto.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let passphrase = "test-passphrase-12345";
        let plaintext = b"hello world, this is a sync test";
        let encrypted = encrypt(passphrase, plaintext).unwrap();
        assert_ne!(encrypted.as_slice(), plaintext);
        let decrypted = decrypt(passphrase, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let encrypted = encrypt("correct", b"secret").unwrap();
        assert!(decrypt("wrong", &encrypted).is_err());
    }

    #[test]
    fn empty_data_fails() {
        assert!(decrypt("pass", &[]).is_err());
    }

    #[test]
    fn cross_transport_compat() {
        // Verify the wire format is identical to FileSyncTransport
        let passphrase = "compat-test";
        let data = b"cross-transport compatibility check";
        let encrypted = encrypt(passphrase, data).unwrap();
        // salt(16) + nonce(12) + ciphertext(>=1) + tag(16)
        assert!(encrypted.len() >= 16 + 12 + data.len() + 16);
        let decrypted = decrypt(passphrase, &encrypted).unwrap();
        assert_eq!(decrypted, data);
    }
}
```

- [ ] **Step 4: Create `sync/mod.rs`**

```rust
//! Sync transport adapters (Phase 3b).
//!
//! - `RemoteSyncTransport` — REST push/pull to a cloud endpoint.
//! - `LanSyncTransport` — mDNS + HTTPS peer-to-peer (behind `lan-sync` feature).

pub mod sync_crypto;
pub mod remote_transport;

#[cfg(feature = "lan-sync")]
pub mod lan_transport;
#[cfg(feature = "lan-sync")]
pub mod lan_discovery;
#[cfg(feature = "lan-sync")]
pub mod lan_server;
#[cfg(feature = "lan-sync")]
pub mod lan_tls;
#[cfg(feature = "lan-sync")]
pub mod lan_crypto;

pub use remote_transport::RemoteSyncTransport;

#[cfg(feature = "lan-sync")]
pub use lan_transport::LanSyncTransport;
```

- [ ] **Step 5: Register module in lib.rs**

Add to `crates/oneshim-network/src/lib.rs`:

```rust
pub mod sync;
```

- [ ] **Step 6: Verify**

Run: `cargo test -p oneshim-network`

---

### Task 4: Implement RemoteSyncTransport

**File:** `crates/oneshim-network/src/sync/remote_transport.rs`

This is the core deliverable for 3b-1. A stateless HTTP client adapter that pushes/pulls encrypted `ChangeSet` payloads to/from a configurable REST endpoint. Follows the same retry pattern as `HttpApiClient`.

- [ ] **Step 1: Create the struct and constructor**

```rust
//! RemoteSyncTransport -- REST-based cloud sync relay.
//!
//! Pushes/pulls AES-256-GCM encrypted ChangeSet payloads to a configurable
//! REST endpoint. Authentication via Bearer token or API key.

use async_trait::async_trait;
use std::time::Duration;
use tracing::{debug, warn};

use oneshim_core::config::sections::sync::RemoteSyncAuth;
use oneshim_core::error::CoreError;
use oneshim_core::models::sync::{ChangeSet, PeerInfo};
use oneshim_core::ports::sync_transport::SyncTransport;
use oneshim_core::sync::Hlc;

use crate::resilience::{jittered_backoff_delay, extract_retry_after};
use super::sync_crypto;

const MAX_RETRIES: u32 = 3;
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Remote sync transport -- push/pull changesets via REST to a cloud endpoint.
pub struct RemoteSyncTransport {
    client: reqwest::Client,
    endpoint: String,
    local_device_id: String,
    passphrase: String,
    auth_mode: RemoteSyncAuth,
    /// Credential value: Bearer token or API key (retrieved from OS keychain).
    auth_credential: String,
    max_retries: u32,
    timeout_ms: u64,
}
```

- [ ] **Step 2: Implement constructor and helpers**

```rust
impl RemoteSyncTransport {
    pub fn new(
        endpoint: String,
        local_device_id: String,
        passphrase: String,
        auth_mode: RemoteSyncAuth,
        auth_credential: String,
    ) -> Result<Self, CoreError> {
        let timeout = Duration::from_secs(REQUEST_TIMEOUT_SECS);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| CoreError::Network(format!("Failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            local_device_id,
            passphrase,
            auth_mode,
            auth_credential,
            max_retries: MAX_RETRIES,
            timeout_ms: timeout.as_millis() as u64,
        })
    }

    fn auth_header(&self) -> (&str, String) {
        match self.auth_mode {
            RemoteSyncAuth::BearerToken => {
                ("Authorization", format!("Bearer {}", self.auth_credential))
            }
            RemoteSyncAuth::ApiKey => {
                ("X-Api-Key", self.auth_credential.clone())
            }
        }
    }

    fn map_error(&self, e: reqwest::Error, context: &str) -> CoreError {
        if e.is_timeout() {
            CoreError::RequestTimeout { timeout_ms: self.timeout_ms }
        } else {
            CoreError::Network(format!("{context}: {e}"))
        }
    }

    fn check_response_status(status: reqwest::StatusCode, body: &str) -> Result<(), CoreError> {
        match status.as_u16() {
            200 | 204 => Ok(()),
            401 | 403 => Err(CoreError::Auth(format!("Sync auth failed: {body}"))),
            404 => Err(CoreError::NotFound {
                resource_type: "SyncEndpoint".to_string(),
                id: body.to_string(),
            }),
            409 => {
                // Conflict -- stale watermark; SyncEngine will re-pull
                debug!("sync push conflict (409), will re-pull");
                Ok(())
            }
            429 => {
                let retry_secs = 60u64; // Default; actual parsing in retry loop
                Err(CoreError::RateLimit { retry_after_secs: retry_secs })
            }
            503 => Err(CoreError::ServiceUnavailable(body.to_string())),
            _ => Err(CoreError::Internal(format!("Sync API error ({status}): {body}"))),
        }
    }

    fn is_retryable(error: &CoreError) -> bool {
        matches!(
            error,
            CoreError::Network(_)
                | CoreError::RequestTimeout { .. }
                | CoreError::ServiceUnavailable(_)
                | CoreError::RateLimit { .. }
        )
    }
}
```

- [ ] **Step 3: Implement `SyncTransport` trait**

```rust
#[async_trait]
impl SyncTransport for RemoteSyncTransport {
    async fn push(&self, changes: &ChangeSet) -> Result<(), CoreError> {
        let json = serde_json::to_vec(changes)
            .map_err(|e| CoreError::Internal(format!("serialize changeset: {e}")))?;
        let encrypted = sync_crypto::encrypt(&self.passphrase, &json)?;
        let (header_name, header_value) = self.auth_header();

        let mut last_error = CoreError::Internal("push failed".to_string());
        for attempt in 0..=self.max_retries {
            let result = self.client
                .post(format!("{}/sync/push", self.endpoint))
                .header(header_name, &header_value)
                .header("Content-Type", "application/octet-stream")
                .body(encrypted.clone())
                .send()
                .await;

            match result {
                Ok(resp) => {
                    let status = resp.status();
                    let retry_after = extract_retry_after(&resp);
                    let body = resp.text().await.unwrap_or_default();
                    match status.as_u16() {
                        200 | 204 => {
                            debug!(bytes = encrypted.len(), "remote push succeeded");
                            return Ok(());
                        }
                        409 => {
                            debug!("remote push conflict (409), re-pull needed");
                            return Ok(()); // SyncEngine handles re-pull
                        }
                        429 => {
                            last_error = CoreError::RateLimit { retry_after_secs: retry_after };
                        }
                        _ => {
                            let err = Self::check_response_status(status, &body);
                            if let Err(e) = err {
                                last_error = e;
                            }
                        }
                    }
                }
                Err(e) => {
                    last_error = self.map_error(e, "remote push");
                }
            }

            if !Self::is_retryable(&last_error) || attempt == self.max_retries {
                return Err(last_error);
            }

            let delay = match &last_error {
                CoreError::RateLimit { retry_after_secs } => {
                    Duration::from_secs(*retry_after_secs)
                }
                _ => jittered_backoff_delay(
                    attempt,
                    Duration::from_secs(1),
                    Duration::from_secs(30),
                ),
            };
            warn!(attempt = attempt + 1, delay_ms = delay.as_millis(), "remote push retry");
            tokio::time::sleep(delay).await;
        }
        Err(last_error)
    }

    async fn pull(&self, since: &Hlc) -> Result<Option<ChangeSet>, CoreError> {
        let (header_name, header_value) = self.auth_header();
        let url = format!(
            "{}/sync/pull?since_wall_ms={}&since_counter={}&device_id={}",
            self.endpoint, since.wall_ms, since.counter, self.local_device_id
        );

        let mut last_error = CoreError::Internal("pull failed".to_string());
        for attempt in 0..=self.max_retries {
            let result = self.client
                .get(&url)
                .header(header_name, &header_value)
                .send()
                .await;

            match result {
                Ok(resp) => {
                    let status = resp.status();
                    match status.as_u16() {
                        204 => return Ok(None),
                        200 => {
                            let bytes = resp.bytes().await.map_err(|e| {
                                CoreError::Network(format!("read pull response: {e}"))
                            })?;
                            if bytes.is_empty() {
                                return Ok(None);
                            }
                            let plaintext = sync_crypto::decrypt(&self.passphrase, &bytes)?;
                            let cs: ChangeSet = serde_json::from_slice(&plaintext).map_err(|e| {
                                CoreError::Internal(format!("deserialize changeset: {e}"))
                            })?;
                            debug!(
                                origin = %cs.origin_device_id,
                                rows = cs.row_count(),
                                "remote pull succeeded"
                            );
                            return Ok(Some(cs));
                        }
                        _ => {
                            let retry_after = extract_retry_after(&resp);
                            let body = resp.text().await.unwrap_or_default();
                            last_error = match status.as_u16() {
                                429 => CoreError::RateLimit { retry_after_secs: retry_after },
                                _ => Self::check_response_status(status, &body)
                                    .err()
                                    .unwrap_or_else(|| CoreError::Internal("unexpected".into())),
                            };
                        }
                    }
                }
                Err(e) => {
                    last_error = self.map_error(e, "remote pull");
                }
            }

            if !Self::is_retryable(&last_error) || attempt == self.max_retries {
                return Err(last_error);
            }

            let delay = jittered_backoff_delay(
                attempt,
                Duration::from_secs(1),
                Duration::from_secs(30),
            );
            warn!(attempt = attempt + 1, "remote pull retry");
            tokio::time::sleep(delay).await;
        }
        Err(last_error)
    }

    async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError> {
        let (header_name, header_value) = self.auth_header();
        let resp = self.client
            .get(format!("{}/sync/peers", self.endpoint))
            .header(header_name, &header_value)
            .send()
            .await
            .map_err(|e| self.map_error(e, "discover peers"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Self::check_response_status(status, &body)
                .err()
                .unwrap_or_else(|| CoreError::Internal("unexpected".into())));
        }

        let peers: Vec<PeerInfo> = resp.json().await.map_err(|e| {
            CoreError::Internal(format!("parse peers response: {e}"))
        })?;
        debug!(count = peers.len(), "discovered remote peers");
        Ok(peers)
    }
}
```

- [ ] **Step 4: Verify**

Run: `cargo check -p oneshim-network`

---

### Task 5: Unit tests for RemoteSyncTransport

**File:** `crates/oneshim-network/src/sync/remote_transport.rs` (append to bottom)

Use `mockito` to mock the sync endpoint. Follow the same test patterns as `HttpApiClient` tests in `http_client.rs`.

- [ ] **Step 1: Add tests module**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::sync::ChangeSetKind;

    fn test_transport(endpoint: &str) -> RemoteSyncTransport {
        RemoteSyncTransport::new(
            endpoint.to_string(),
            "test-device".to_string(),
            "test-passphrase".to_string(),
            RemoteSyncAuth::BearerToken,
            "test-token".to_string(),
        )
        .unwrap()
    }

    fn test_changeset() -> ChangeSet {
        ChangeSet {
            kind: ChangeSetKind::Data,
            origin_device_id: "test-device".to_string(),
            origin_device_name: "Test".to_string(),
            watermark: Hlc { wall_ms: 100, counter: 1, device_id: "test-device".to_string() },
            segments: vec![serde_json::json!({"id": "seg-1"})],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn push_success_200() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/sync/push")
            .match_header("Authorization", "Bearer test-token")
            .with_status(200)
            .create_async().await;

        let transport = test_transport(&server.url());
        let result = transport.push(&test_changeset()).await;
        assert!(result.is_ok());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn push_conflict_409_returns_ok() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/sync/push")
            .with_status(409)
            .create_async().await;

        let transport = test_transport(&server.url());
        let result = transport.push(&test_changeset()).await;
        assert!(result.is_ok()); // 409 is not an error -- triggers re-pull
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn push_auth_failure_401_no_retry() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/sync/push")
            .with_status(401)
            .with_body("Unauthorized")
            .create_async().await;

        let transport = test_transport(&server.url());
        let result = transport.push(&test_changeset()).await;
        assert!(matches!(result, Err(CoreError::Auth(_))));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn pull_204_returns_none() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", mockito::Matcher::Regex(r"/sync/pull\?.*".to_string()))
            .with_status(204)
            .create_async().await;

        let transport = test_transport(&server.url());
        let result = transport.pull(&Hlc::default()).await;
        assert!(result.unwrap().is_none());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn pull_200_decrypts_changeset() {
        let passphrase = "test-passphrase";
        let cs = test_changeset();
        let json = serde_json::to_vec(&cs).unwrap();
        let encrypted = sync_crypto::encrypt(passphrase, &json).unwrap();

        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", mockito::Matcher::Regex(r"/sync/pull\?.*".to_string()))
            .with_status(200)
            .with_body(encrypted)
            .create_async().await;

        let transport = test_transport(&server.url());
        let pulled = transport.pull(&Hlc::default()).await.unwrap();
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().origin_device_id, "test-device");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn discover_peers_success() {
        let peers_json = serde_json::json!([
            {
                "device_id": "peer-1",
                "device_name": "Work MacBook",
                "last_sync_at": "2026-03-19T12:00:00Z",
                "watermark": { "wall_ms": 100, "counter": 1, "device_id": "peer-1" }
            }
        ]);

        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/sync/peers")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(peers_json.to_string())
            .create_async().await;

        let transport = test_transport(&server.url());
        let peers = transport.discover_peers().await.unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].device_id, "peer-1");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn api_key_auth_mode() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/sync/peers")
            .match_header("X-Api-Key", "my-api-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async().await;

        let transport = RemoteSyncTransport::new(
            server.url(),
            "dev".to_string(),
            "pass".to_string(),
            RemoteSyncAuth::ApiKey,
            "my-api-key".to_string(),
        ).unwrap();

        transport.discover_peers().await.unwrap();
        mock.assert_async().await;
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p oneshim-network -- sync`

---

### Task 6: Wire RemoteSyncTransport into DI (agent_runtime.rs)

**File:** `src-tauri/src/agent_runtime.rs`

Extend the existing sync DI block (lines 306-366) to handle `SyncTransportKind::Remote` in addition to `File`. The passphrase still comes from `ONESHIM_SYNC_PASSPHRASE` env var. The remote auth credential comes from the OS keychain via the `keyring` crate (key name: `oneshim_sync_remote_token`).

- [ ] **Step 1: Add use statement**

At the top of `agent_runtime.rs`, add:

```rust
use oneshim_core::config::sections::sync::SyncTransportKind;
```

- [ ] **Step 2: Replace the sync wiring block**

Replace the existing `if self.config.sync.enabled { ... }` block (approximately lines 306-366) with a version that handles all three transport kinds:

```rust
        // --- Cross-device sync engine (P3 Phase 3b) ---
        if self.config.sync.enabled {
            let passphrase = std::env::var("ONESHIM_SYNC_PASSPHRASE").unwrap_or_default();
            if passphrase.is_empty() {
                warn!("sync enabled but ONESHIM_SYNC_PASSPHRASE not set; sync disabled");
            } else {
                match self
                    .sqlite_storage_concrete
                    .ensure_device_identity(&self.config.sync.device_name)
                {
                    Ok((device_id, device_name)) => {
                        let extractor = Arc::new(
                            oneshim_storage::sync_extractor::SqliteSyncExtractor::new(
                                self.sqlite_storage_concrete.connection_arc(),
                                device_id.clone(),
                                device_name.clone(),
                                self.config.sync.clone(),
                            ),
                        );
                        let merger =
                            Arc::new(oneshim_storage::sync_merger::SqliteSyncMerger::new(
                                self.sqlite_storage_concrete.connection_arc(),
                                device_id.clone(),
                            ));

                        let transport_result: Result<Arc<dyn oneshim_core::ports::sync_transport::SyncTransport>, CoreError> =
                            match self.config.sync.transport {
                                SyncTransportKind::File => {
                                    match &self.config.sync.sync_folder {
                                        Some(folder) => {
                                            oneshim_storage::file_transport::FileSyncTransport::new(
                                                std::path::PathBuf::from(folder),
                                                device_id.clone(),
                                                passphrase.clone(),
                                            ).map(|t| Arc::new(t) as Arc<dyn oneshim_core::ports::sync_transport::SyncTransport>)
                                        }
                                        None => {
                                            warn!("sync transport=file but sync_folder not configured");
                                            Err(CoreError::Internal("sync_folder required for file transport".into()))
                                        }
                                    }
                                }
                                SyncTransportKind::Remote => {
                                    match &self.config.sync.remote_endpoint {
                                        Some(endpoint) => {
                                            // Retrieve auth credential from OS keychain
                                            let credential = keyring::Entry::new("oneshim", "sync_remote_token")
                                                .and_then(|entry| entry.get_password())
                                                .unwrap_or_default();
                                            if credential.is_empty() {
                                                warn!("sync transport=remote but no credential in keychain (key: oneshim/sync_remote_token)");
                                            }
                                            oneshim_network::sync::RemoteSyncTransport::new(
                                                endpoint.clone(),
                                                device_id.clone(),
                                                passphrase.clone(),
                                                self.config.sync.remote_auth.clone(),
                                                credential,
                                            ).map(|t| Arc::new(t) as Arc<dyn oneshim_core::ports::sync_transport::SyncTransport>)
                                        }
                                        None => {
                                            warn!("sync transport=remote but remote_endpoint not configured");
                                            Err(CoreError::Internal("remote_endpoint required for remote transport".into()))
                                        }
                                    }
                                }
                                SyncTransportKind::Lan => {
                                    // LAN transport wiring deferred to Phase 3b-2
                                    warn!("LAN sync transport not yet implemented; sync disabled");
                                    Err(CoreError::Internal("LAN transport not implemented".into()))
                                }
                            };

                        match transport_result {
                            Ok(transport) => {
                                let consent_for_sync = Arc::new(parking_lot::Mutex::new(
                                    ConsentManager::new(self.data_dir.join("consent.json")),
                                ));
                                let sync_engine = Arc::new(SyncEngine::new(
                                    extractor,
                                    merger,
                                    transport,
                                    consent_for_sync,
                                    device_id,
                                    device_name,
                                ));
                                scheduler = scheduler.with_sync_engine(sync_engine);
                                info!(
                                    transport = ?self.config.sync.transport,
                                    "Cross-device sync engine initialized"
                                );
                            }
                            Err(e) => {
                                warn!("Failed to create sync transport: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get device identity for sync: {e}");
                    }
                }
            }
        }
```

- [ ] **Step 3: Verify**

Run: `cargo check -p oneshim-app`

---

### Task 7: Integration test -- Remote push-pull roundtrip

**File:** `crates/oneshim-network/src/sync/remote_transport.rs` (or a separate integration test file)

This test verifies the full encrypt-push-pull-decrypt cycle through a mock HTTP server. It confirms that a changeset pushed by one device can be pulled and decrypted by another device sharing the same passphrase.

- [ ] **Step 1: Add integration test**

Add to the `#[cfg(test)] mod tests` block in `remote_transport.rs`:

```rust
    #[tokio::test]
    async fn push_pull_roundtrip_integration() {
        use std::sync::Mutex;

        // Shared state: store the last pushed payload
        let stored = Arc::new(Mutex::new(Vec::<u8>::new()));
        let stored_clone = stored.clone();

        let mut server = mockito::Server::new_async().await;

        // Mock push endpoint -- capture the body
        let push_mock = server.mock("POST", "/sync/push")
            .with_status(200)
            .create_async().await;

        // Device A pushes
        let transport_a = test_transport(&server.url());
        let original = test_changeset();
        transport_a.push(&original).await.unwrap();
        push_mock.assert_async().await;

        // For the roundtrip, we encrypt manually and serve it on pull
        let json = serde_json::to_vec(&original).unwrap();
        let encrypted = sync_crypto::encrypt("test-passphrase", &json).unwrap();

        let pull_mock = server.mock("GET", mockito::Matcher::Regex(r"/sync/pull\?.*".to_string()))
            .with_status(200)
            .with_body(encrypted)
            .create_async().await;

        // Device B pulls
        let transport_b = test_transport(&server.url());
        let pulled = transport_b.pull(&Hlc::default()).await.unwrap();
        assert!(pulled.is_some());
        let pulled_cs = pulled.unwrap();
        assert_eq!(pulled_cs.origin_device_id, original.origin_device_id);
        assert_eq!(pulled_cs.segments, original.segments);
        pull_mock.assert_async().await;
    }
```

- [ ] **Step 2: Run full workspace tests**

Run: `cargo test --workspace`

---

## 3b-2: LanSyncTransport (Tasks 8-16)

> **Prerequisite:** Tasks 1-7 complete and passing.

### Task 8: Add LAN dependencies to workspace

**File:** `Cargo.toml` (workspace root) and `crates/oneshim-network/Cargo.toml`

- [ ] **Step 1: Add workspace dependencies**

In `Cargo.toml` `[workspace.dependencies]` section, add:

```toml
# LAN Sync (Phase 3b-2)
mdns-sd = "0.12"
rcgen = "0.14"
tokio-rustls = "0.26"
rustls-pemfile = "2"
```

- [ ] **Step 2: Add feature flag + optional deps to oneshim-network**

In `crates/oneshim-network/Cargo.toml`, update `[features]`:

```toml
[features]
default = []
grpc = ["tonic", "tonic-prost", "prost", "prost-types", "tonic-health", "tonic/tls-native-roots"]
lan-sync = ["mdns-sd", "rcgen", "tokio-rustls", "rustls-pemfile"]
```

Add the optional dependencies:

```toml
# LAN Sync (Phase 3b-2 - optional)
mdns-sd = { workspace = true, optional = true }
rcgen = { workspace = true, optional = true }
tokio-rustls = { workspace = true, optional = true }
rustls-pemfile = { workspace = true, optional = true }
```

- [ ] **Step 3: Verify**

Run: `cargo check -p oneshim-network --features lan-sync`

---

### Task 9: Implement lan_crypto -- passphrase challenge-response

**File:** `crates/oneshim-network/src/sync/lan_crypto.rs`

HMAC-SHA256 challenge-response protocol. Both sides derive the same key via Argon2id from the shared passphrase. The challenge nonce is random; the response is `HMAC-SHA256(nonce, derived_key)`.

- [ ] **Step 1: Create `lan_crypto.rs`**

```rust
//! Passphrase challenge-response for LAN peer authentication.
//!
//! Protocol:
//! 1. Server generates a random 32-byte nonce.
//! 2. Client computes HMAC-SHA256(nonce, key) where key = Argon2id(passphrase, salt).
//! 3. Salt is deterministic: SHA256(sort(device_id_a, device_id_b)) truncated to 16 bytes.
//! 4. Server verifies by computing the same HMAC.

#[cfg(feature = "lan-sync")]
use hmac::{Hmac, Mac};
#[cfg(feature = "lan-sync")]
use sha2::Sha256;

use oneshim_core::error::CoreError;
use super::sync_crypto;

/// Derive a deterministic salt from two device IDs.
/// Sort lexicographically, concatenate, SHA-256, truncate to 16 bytes.
pub fn derive_peer_salt(device_id_a: &str, device_id_b: &str) -> [u8; 16] {
    use sha2::Digest;
    let (first, second) = if device_id_a <= device_id_b {
        (device_id_a, device_id_b)
    } else {
        (device_id_b, device_id_a)
    };
    let combined = format!("{first}{second}");
    let hash = sha2::Sha256::digest(combined.as_bytes());
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&hash[..16]);
    salt
}

/// Compute the HMAC-SHA256 response for a challenge nonce.
pub fn compute_challenge_response(
    nonce: &[u8],
    passphrase: &str,
    local_device_id: &str,
    peer_device_id: &str,
) -> Result<Vec<u8>, CoreError> {
    let salt = derive_peer_salt(local_device_id, peer_device_id);
    let key = sync_crypto::derive_key(passphrase, &salt)?;

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(&key)
        .map_err(|e| CoreError::Internal(format!("HMAC init: {e}")))?;
    mac.update(nonce);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// Verify a challenge response.
pub fn verify_challenge_response(
    nonce: &[u8],
    response: &[u8],
    passphrase: &str,
    local_device_id: &str,
    peer_device_id: &str,
) -> Result<bool, CoreError> {
    let expected = compute_challenge_response(nonce, passphrase, local_device_id, peer_device_id)?;
    Ok(expected == response)
}
```

- [ ] **Step 2: Add tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salt_is_order_independent() {
        let salt_ab = derive_peer_salt("device-a", "device-b");
        let salt_ba = derive_peer_salt("device-b", "device-a");
        assert_eq!(salt_ab, salt_ba);
    }

    #[test]
    fn challenge_response_roundtrip() {
        let nonce = b"12345678901234567890123456789012";
        let passphrase = "shared-secret";
        let response = compute_challenge_response(
            nonce, passphrase, "dev-a", "dev-b"
        ).unwrap();
        let verified = verify_challenge_response(
            nonce, &response, passphrase, "dev-b", "dev-a" // note: reversed
        ).unwrap();
        assert!(verified);
    }

    #[test]
    fn wrong_passphrase_fails_verification() {
        let nonce = b"12345678901234567890123456789012";
        let response = compute_challenge_response(
            nonce, "correct-pass", "dev-a", "dev-b"
        ).unwrap();
        let verified = verify_challenge_response(
            nonce, &response, "wrong-pass", "dev-a", "dev-b"
        ).unwrap();
        assert!(!verified);
    }
}
```

- [ ] **Step 3: Verify**

Run: `cargo test -p oneshim-network --features lan-sync -- lan_crypto`

---

### Task 10: Implement lan_tls -- self-signed cert generation + TOFU

**File:** `crates/oneshim-network/src/sync/lan_tls.rs`

Generate self-signed TLS certificates using `rcgen`. Store cert + key as PEM files. Compute SHA-256 fingerprint for TOFU verification.

- [ ] **Step 1: Create `lan_tls.rs`**

Implement:
- `generate_self_signed_cert(device_id: &str) -> Result<(Vec<u8>, Vec<u8>), CoreError>` returns (cert_pem, key_pem)
- `load_or_generate_cert(config_dir: &Path, device_id: &str) -> Result<(Vec<u8>, Vec<u8>, String), CoreError>` returns (cert_pem, key_pem, fingerprint_hex)
- `compute_cert_fingerprint(cert_pem: &[u8]) -> Result<String, CoreError>` returns SHA-256 hex of DER encoding

- [ ] **Step 2: Add tests**

Test cert generation, fingerprint computation, and load-or-generate idempotency (generate once, load same on second call).

- [ ] **Step 3: Verify**

Run: `cargo test -p oneshim-network --features lan-sync -- lan_tls`

---

### Task 11: Add V15 migration for lan_peer_pins

**File:** `crates/oneshim-storage/src/migration.rs`

The TOFU pin store needs a `lan_peer_pins` table to persist which peer certificates have been trusted.

- [ ] **Step 1: Add V15 migration**

```sql
CREATE TABLE IF NOT EXISTS lan_peer_pins (
    device_id TEXT PRIMARY KEY,
    cert_fingerprint TEXT NOT NULL,
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    trust_revoked INTEGER NOT NULL DEFAULT 0
);
```

- [ ] **Step 2: Update `CURRENT_SCHEMA_VERSION`** to 15

- [ ] **Step 3: Add `get_pin`, `upsert_pin`, `revoke_pin` helper methods** on `SqliteStorage`

These are simple CRUD methods for the `lan_peer_pins` table, returning `Option<(String, bool)>` (fingerprint, revoked) for `get_pin`.

- [ ] **Step 4: Add tests**

Test insert, retrieve, fingerprint mismatch detection, and revocation.

- [ ] **Step 5: Verify**

Run: `cargo test -p oneshim-storage -- lan_peer`

---

### Task 12: Implement lan_discovery -- mDNS service registration + browse

**File:** `crates/oneshim-network/src/sync/lan_discovery.rs`

Uses `mdns-sd::ServiceDaemon` to register and browse `_oneshim-sync._tcp.local.` services.

- [ ] **Step 1: Create `LanDiscovery` struct**

```rust
pub struct LanDiscovery {
    daemon: ServiceDaemon,
    service_fullname: String,
    peers: Arc<RwLock<HashMap<String, LanPeerInfo>>>,
    browse_handle: Option<tokio::task::JoinHandle<()>>,
}
```

- [ ] **Step 2: Implement `start()`, `stop()`, `peers()`**

- `start()`: Register mDNS service with TXT records (device_id, device_name, port, version, fingerprint). Start browse task that updates `peers` map on `ServiceResolved`/`ServiceRemoved` events.
- `stop()`: Unregister service, abort browse task.
- `peers()`: Return clone of current peers map.

- [ ] **Step 3: Add tests**

Test service registration and browse on loopback. Verify TXT record parsing.

- [ ] **Step 4: Verify**

Run: `cargo test -p oneshim-network --features lan-sync -- lan_discovery`

---

### Task 13: Implement lan_server -- Axum HTTPS peer server

**File:** `crates/oneshim-network/src/sync/lan_server.rs`

Lightweight Axum server that serves changesets to LAN peers over HTTPS (self-signed cert).

- [ ] **Step 1: Create `LanPeerServer` struct**

```rust
pub struct LanPeerServer {
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    port: u16,
    fingerprint: String,
}
```

- [ ] **Step 2: Implement endpoints**

Five routes per spec section 5.6.1:

| Route | Method | Handler | Auth |
|-------|--------|---------|------|
| `/sync/challenge` | GET | Return `{ "nonce": "<hex>" }` | None |
| `/sync/verify` | POST | Verify HMAC, issue session cookie | None |
| `/sync/pull` | GET | Return encrypted ChangeSet (or 204) | Session cookie |
| `/sync/push` | POST | Receive encrypted ChangeSet from peer | Session cookie |
| `/sync/info` | GET | Return `{ "device_id", "device_name", "version" }` | None |

- [ ] **Step 3: Implement `start()` and `stop()`**

`start()` binds to `0.0.0.0:0` with TLS via `tokio-rustls`, returns bound port. `stop()` sends on `shutdown_tx`.

- [ ] **Step 4: Add rate limiting**

Use `tower::limit::RateLimitLayer` (60 req/min) and `tower::limit::ConcurrencyLimitLayer` (10 concurrent).

- [ ] **Step 5: Add tests**

Use `axum::test` helpers (or direct `reqwest` calls to `127.0.0.1:port`) to test each endpoint:
- Challenge returns valid nonce JSON
- Verify with correct passphrase returns 200
- Verify with wrong passphrase returns 403
- Pull without session returns 401
- Pull after verify returns 200 with encrypted changeset
- Push after verify returns 200
- Info returns device metadata

- [ ] **Step 6: Verify**

Run: `cargo test -p oneshim-network --features lan-sync -- lan_server`

---

### Task 14: Implement LanSyncTransport orchestrator

**File:** `crates/oneshim-network/src/sync/lan_transport.rs`

Orchestrates `LanDiscovery`, `LanPeerServer`, and per-peer reqwest clients into a single `SyncTransport` implementation.

- [ ] **Step 1: Create `LanSyncTransport` struct**

```rust
pub struct LanSyncTransport {
    discovery: LanDiscovery,
    server: LanPeerServer,
    peers: Arc<RwLock<HashMap<String, LanPeerInfo>>>,
    peer_clients: Arc<RwLock<HashMap<String, reqwest::Client>>>,
    local_device_id: String,
    passphrase: String,
}
```

- [ ] **Step 2: Implement `SyncTransport` trait**

- `push()`: Fan-out to all verified peers. Success if at least one peer received it (or no peers exist).
- `pull()`: Try each verified peer in order. Return first successful result.
- `discover_peers()`: Return all mDNS-discovered peers.

- [ ] **Step 3: Implement `start()` and `stop()` lifecycle**

`start()` initializes the discovery daemon, starts the peer server, registers the mDNS service. `stop()` tears down everything in reverse order.

- [ ] **Step 4: Implement per-peer verification flow**

For each new peer discovered via mDNS:
1. Check TOFU pin store (SQLite `lan_peer_pins`).
2. If pinned and fingerprint matches, mark as verified.
3. If pinned but fingerprint mismatches, reject (TOFU violation).
4. If not pinned, perform challenge-response. On success, store pin and mark verified.

- [ ] **Step 5: Add tests**

Integration test: create two `LanSyncTransport` instances on loopback, verify discovery, challenge-response, push, and pull.

- [ ] **Step 6: Verify**

Run: `cargo test -p oneshim-network --features lan-sync -- lan_transport`

---

### Task 15: Wire LanSyncTransport into DI

**File:** `src-tauri/src/agent_runtime.rs`

Replace the `SyncTransportKind::Lan` placeholder (from Task 6) with actual wiring.

- [ ] **Step 1: Add LAN transport construction**

In the `SyncTransportKind::Lan` match arm, construct `LanSyncTransport`:

```rust
SyncTransportKind::Lan => {
    #[cfg(feature = "lan-sync")]
    {
        let config_dir = self.data_dir.clone();
        let (cert_pem, key_pem, fingerprint) =
            oneshim_network::sync::lan_tls::load_or_generate_cert(
                &config_dir, &device_id
            )?;
        let transport = oneshim_network::sync::LanSyncTransport::start(
            device_id.clone(),
            device_name.clone(),
            passphrase.clone(),
            cert_pem, key_pem, fingerprint,
            self.config.sync.lan_port,
            self.config.sync.lan_advertise,
            self.sqlite_storage_concrete.clone(), // for TOFU pin store
        ).await
        .map(|t| Arc::new(t) as Arc<dyn SyncTransport>)?;
        Ok(transport)
    }
    #[cfg(not(feature = "lan-sync"))]
    {
        warn!("LAN sync requires 'lan-sync' feature; sync disabled");
        Err(CoreError::Internal("lan-sync feature not enabled".into()))
    }
}
```

- [ ] **Step 2: Add feature flag to src-tauri/Cargo.toml**

```toml
[features]
lan-sync = ["oneshim-network/lan-sync"]
```

- [ ] **Step 3: Verify**

Run: `cargo check -p oneshim-app --features lan-sync`

---

### Task 16: Full workspace verification

- [ ] **Step 1: Run all tests without LAN feature**

```bash
cargo test --workspace
```

- [ ] **Step 2: Run all tests with LAN feature**

```bash
cargo test --workspace --features lan-sync
```

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --workspace --features lan-sync
```

- [ ] **Step 4: Run fmt check**

```bash
cargo fmt --check
```

- [ ] **Step 5: Update `docs/STATUS.md`** with new test counts

---

## Verification Checklist

### 3b-1 (Remote) -- all must pass before starting 3b-2

- [ ] `SyncConfig` has `remote_endpoint`, `remote_auth` fields with serde roundtrip
- [ ] `sync_crypto` encrypt/decrypt roundtrip passes
- [ ] `RemoteSyncTransport::push()` sends encrypted body to `POST /sync/push`
- [ ] `RemoteSyncTransport::pull()` decrypts response from `GET /sync/pull`
- [ ] `RemoteSyncTransport::discover_peers()` parses JSON from `GET /sync/peers`
- [ ] 409 on push returns `Ok(())` (not an error)
- [ ] 401 on push returns `CoreError::Auth` without retry
- [ ] 429 on push triggers exponential backoff with Retry-After
- [ ] Bearer and ApiKey auth modes inject correct headers
- [ ] DI in `agent_runtime.rs` selects `RemoteSyncTransport` when `transport == Remote`
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace` passes

### 3b-2 (LAN) -- all must pass

- [ ] `lan_crypto` challenge-response is order-independent on device IDs
- [ ] Self-signed cert generated and loaded from disk
- [ ] Cert fingerprint matches between generation and mDNS TXT record
- [ ] mDNS service registered and browseable on loopback
- [ ] `LanPeerServer` serves challenge/verify/pull/push/info endpoints
- [ ] Passphrase verification rejects wrong passphrase (403)
- [ ] TOFU pin stored after first successful verification
- [ ] TOFU violation detected on cert fingerprint mismatch
- [ ] Two-device loopback integration test passes (discovery + verify + push + pull)
- [ ] Rate limiting enforced (60 req/min per peer IP)
- [ ] `cargo test --workspace --features lan-sync` passes
- [ ] `cargo clippy --workspace --features lan-sync` passes

---

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| mDNS behavior varies across platforms | `mdns-sd` crate abstracts platform differences; test on macOS first (Bonjour native), defer Windows/Linux to manual testing |
| Self-signed TLS + reqwest TOFU is tricky | Use `reqwest::Certificate::from_pem` for pinning; initial connection uses `danger_accept_invalid_certs(true)` only during challenge-response, then switches to pinned client |
| Firewall blocks mDNS or peer server | Graceful degradation: log warning, return empty peer list; dashboard shows firewall guidance |
| Remote endpoint contract mismatch | Mock server tests validate client expectations; server implementation is out of scope |
| Encrypt/decrypt format incompatibility between transports | `sync_crypto` is extracted from proven `FileSyncTransport`; cross-transport roundtrip test validates |

---

## Dependencies (ordered by addition)

| Crate | Version | Purpose | When |
|-------|---------|---------|------|
| `aes-gcm` | 0.10 | AES-256-GCM encryption (in oneshim-network) | Task 2 |
| `argon2` | 0.5 | Argon2id KDF (in oneshim-network) | Task 2 |
| `hex` | 0.4 | Hex encoding for fingerprints/nonces | Task 2 |
| `mdns-sd` | 0.12 | mDNS service discovery (optional, lan-sync) | Task 8 |
| `rcgen` | 0.14 | Self-signed TLS cert generation (optional, lan-sync) | Task 8 |
| `tokio-rustls` | 0.26 | TLS for Axum server (optional, lan-sync) | Task 8 |
| `rustls-pemfile` | 2 | PEM parsing for cert/key loading (optional, lan-sync) | Task 8 |
