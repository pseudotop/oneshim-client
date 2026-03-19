//! LanPeerServer -- Axum HTTP(S) peer server for LAN sync (Phase 3b-2).
//!
//! Lightweight server that serves changesets to authenticated LAN peers.
//!
//! ## Endpoints
//!
//! | Method | Path          | Description                          |
//! |--------|---------------|--------------------------------------|
//! | GET    | `/sync/info`  | Device info + protocol version       |
//! | GET    | `/sync/pull`  | Return encrypted changesets since HLC |
//! | POST   | `/sync/push`  | Receive encrypted changeset from peer|
//!
//! ## TLS Upgrade Path
//!
//! The current implementation binds a plain HTTP server. For production
//! LAN sync, wrap the listener with `tokio-rustls` using the self-signed
//! cert from `lan_tls.rs`:
//!
//! ```ignore
//! let tls_config = rustls::ServerConfig::builder()
//!     .with_no_client_auth()
//!     .with_single_cert(cert_chain, private_key)?;
//! let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));
//! // Wrap each accepted TCP stream with tls_acceptor.accept(stream)
//! ```
//!
//! Requires the `lan-sync` feature flag.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

use oneshim_core::error::CoreError;
use oneshim_core::models::sync::ChangeSet;
use oneshim_core::sync::Hlc;

use super::sync_crypto;

/// Protocol version for compatibility negotiation.
const PROTOCOL_VERSION: &str = "1";

/// Maximum request body size: 16 MiB.
const MAX_BODY_SIZE: usize = 16 * 1024 * 1024;

/// Maximum number of outbound changesets held in memory.
/// When full, the oldest entries are evicted to make room.
const MAX_OUTBOUND_QUEUE: usize = 1000;

/// Device info returned by `GET /sync/info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfoResponse {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub protocol_version: String,
}

/// Query parameters for `GET /sync/pull`.
#[derive(Debug, Deserialize)]
pub struct PullQuery {
    /// Wall-clock milliseconds of the HLC watermark.
    pub since_wall_ms: Option<u64>,
    /// Counter component of the HLC watermark.
    pub since_counter: Option<u32>,
    /// Requesting device's ID.
    pub device_id: Option<String>,
}

/// Shared state for the Axum server.
#[derive(Clone)]
struct ServerState {
    device_id: String,
    device_name: String,
    fingerprint: String,
    passphrase: String,
    /// Changesets received via push, keyed by origin_device_id.
    /// In a full implementation this would be backed by the storage layer.
    received_changesets: Arc<RwLock<Vec<ChangeSet>>>,
    /// Pending outbound changesets for peers to pull.
    /// Populated by the SyncEngine when local changes are extracted.
    outbound_changesets: Arc<RwLock<Vec<ChangeSet>>>,
}

/// LAN peer server state.
pub struct LanPeerServer {
    device_id: String,
    device_name: String,
    passphrase: String,
    port: u16,
    fingerprint: String,
    /// Changesets received from peers via POST /sync/push.
    received_changesets: Arc<RwLock<Vec<ChangeSet>>>,
    /// Changesets queued for peers to pull via GET /sync/pull.
    outbound_changesets: Arc<RwLock<Vec<ChangeSet>>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
    running: bool,
}

impl LanPeerServer {
    /// Create a new peer server (not yet started).
    pub fn new(
        device_id: String,
        device_name: String,
        passphrase: String,
        fingerprint: String,
    ) -> Self {
        Self {
            device_id,
            device_name,
            passphrase,
            port: 0,
            fingerprint,
            received_changesets: Arc::new(RwLock::new(Vec::new())),
            outbound_changesets: Arc::new(RwLock::new(Vec::new())),
            shutdown_tx: None,
            server_handle: None,
            running: false,
        }
    }

    /// Start the HTTP server on the specified port (0 = ephemeral OS-assigned).
    ///
    /// Returns the actual bound port. The server runs in a background tokio task
    /// and can be stopped via `stop()`.
    ///
    /// ## TLS
    ///
    /// `cert_pem` and `key_pem` are accepted for API compatibility with the
    /// planned TLS upgrade. The current implementation binds plain HTTP.
    /// See module docs for the TLS upgrade path.
    pub async fn start(
        &mut self,
        _cert_pem: &[u8],
        _key_pem: &[u8],
        port: u16,
    ) -> Result<u16, CoreError> {
        if self.running {
            return Ok(self.port);
        }

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            CoreError::Internal(format!("failed to bind LAN server on {addr}: {e}"))
        })?;

        let actual_port = listener
            .local_addr()
            .map_err(|e| CoreError::Internal(format!("failed to get local addr: {e}")))?
            .port();

        let state = ServerState {
            device_id: self.device_id.clone(),
            device_name: self.device_name.clone(),
            fingerprint: self.fingerprint.clone(),
            passphrase: self.passphrase.clone(),
            received_changesets: Arc::clone(&self.received_changesets),
            outbound_changesets: Arc::clone(&self.outbound_changesets),
        };

        let app = build_router(state);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let handle = tokio::task::spawn(async move {
            let serve_result = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;

            if let Err(e) = serve_result {
                error!("LAN peer server error: {e}");
            }
        });

        self.port = actual_port;
        self.shutdown_tx = Some(shutdown_tx);
        self.server_handle = Some(handle);
        self.running = true;

        info!(
            port = actual_port,
            device_id = %self.device_id,
            "LAN peer server started (HTTP, TLS upgrade pending)"
        );

        Ok(actual_port)
    }

    /// Stop the server gracefully.
    ///
    /// Sends a shutdown signal and spawns a background task to abort
    /// the server if it does not complete within a grace period. This
    /// avoids the race between the oneshot signal and an immediate
    /// `abort()` while remaining safe to call from both sync and async
    /// contexts.
    pub fn stop(&mut self) {
        if !self.running {
            return;
        }

        // Signal graceful shutdown
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Give the server task a grace period before aborting.
        // Spawn a fire-and-forget task so we don't block the caller.
        if let Some(handle) = self.server_handle.take() {
            if let Ok(_rt) = tokio::runtime::Handle::try_current() {
                tokio::spawn(async move {
                    match tokio::time::timeout(std::time::Duration::from_secs(2), handle).await {
                        Ok(_) => debug!("LAN peer server task completed gracefully"),
                        Err(_) => warn!("LAN peer server task did not complete in time"),
                    }
                });
            } else {
                // No tokio runtime available (e.g., during Drop) -- abort immediately
                handle.abort();
            }
        }

        self.running = false;
        info!("LAN peer server stopped");
    }

    /// Get the bound port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the certificate fingerprint.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Check if server is running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Enqueue a changeset for peers to pull.
    ///
    /// Called by the SyncEngine when local changes are extracted.
    /// If the queue exceeds `MAX_OUTBOUND_QUEUE`, the oldest entries
    /// are evicted to make room.
    pub fn enqueue_outbound(&self, changeset: ChangeSet) {
        let mut queue = self.outbound_changesets.write();
        queue.push(changeset);
        if queue.len() > MAX_OUTBOUND_QUEUE {
            let excess = queue.len() - MAX_OUTBOUND_QUEUE;
            queue.drain(..excess);
            debug!(
                evicted = excess,
                "outbound queue exceeded capacity, evicted oldest entries"
            );
        }
    }

    /// Drain received changesets (from peers pushing to us).
    ///
    /// Called by the SyncEngine to process incoming peer data.
    pub fn drain_received(&self) -> Vec<ChangeSet> {
        let mut store = self.received_changesets.write();
        std::mem::take(&mut *store)
    }
}

impl Drop for LanPeerServer {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Axum router + handlers
// ---------------------------------------------------------------------------

fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/sync/info", get(handle_info))
        .route("/sync/pull", get(handle_pull))
        .route("/sync/push", post(handle_push))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_SIZE))
        .with_state(state)
}

/// GET /sync/info -- return device info and protocol version.
async fn handle_info(State(state): State<ServerState>) -> Json<DeviceInfoResponse> {
    Json(DeviceInfoResponse {
        device_id: state.device_id.clone(),
        device_name: state.device_name.clone(),
        fingerprint: state.fingerprint.clone(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    })
}

/// GET /sync/pull -- return encrypted changesets newer than the given HLC.
///
/// Query parameters: `since_wall_ms`, `since_counter`, `device_id`.
/// Response: AES-256-GCM encrypted JSON array of changesets, or 204 if none.
async fn handle_pull(
    State(state): State<ServerState>,
    Query(params): Query<PullQuery>,
) -> impl IntoResponse {
    let since = Hlc {
        wall_ms: params.since_wall_ms.unwrap_or(0),
        counter: params.since_counter.unwrap_or(0),
        device_id: params.device_id.unwrap_or_default(),
    };

    // Filter outbound changesets newer than the given watermark
    let outbound = state.outbound_changesets.read();
    let newer: Vec<&ChangeSet> = outbound
        .iter()
        .filter(|cs| {
            cs.watermark.wall_ms > since.wall_ms
                || (cs.watermark.wall_ms == since.wall_ms && cs.watermark.counter > since.counter)
        })
        .collect();

    if newer.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }

    // Serialize and encrypt
    let json = match serde_json::to_vec(&newer) {
        Ok(j) => j,
        Err(e) => {
            warn!("failed to serialize changesets: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let encrypted = match sync_crypto::encrypt(&state.passphrase, &json) {
        Ok(enc) => enc,
        Err(e) => {
            warn!("failed to encrypt changesets: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    debug!(
        count = newer.len(),
        bytes = encrypted.len(),
        "serving pull request"
    );

    (
        StatusCode::OK,
        [("content-type", "application/octet-stream")],
        encrypted,
    )
        .into_response()
}

/// POST /sync/push -- receive an encrypted changeset from a peer.
///
/// Request body: AES-256-GCM encrypted JSON changeset.
/// Response: 200 OK on success, 400 on decryption/parse failure.
async fn handle_push(
    State(state): State<ServerState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if body.len() > MAX_BODY_SIZE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload exceeds 16 MiB limit",
        )
            .into_response();
    }

    if body.is_empty() {
        return (StatusCode::BAD_REQUEST, "empty body").into_response();
    }

    // Decrypt
    let plaintext = match sync_crypto::decrypt(&state.passphrase, &body) {
        Ok(pt) => pt,
        Err(e) => {
            warn!("push decryption failed (wrong passphrase?): {e}");
            return (StatusCode::BAD_REQUEST, "decryption failed").into_response();
        }
    };

    // Deserialize
    let changeset: ChangeSet = match serde_json::from_slice(&plaintext) {
        Ok(cs) => cs,
        Err(e) => {
            warn!("push deserialization failed: {e}");
            return (StatusCode::BAD_REQUEST, "invalid changeset JSON").into_response();
        }
    };

    debug!(
        origin = %changeset.origin_device_id,
        rows = changeset.row_count(),
        "received push from LAN peer"
    );

    state.received_changesets.write().push(changeset);

    StatusCode::OK.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::sync::ChangeSetKind;

    fn test_changeset() -> ChangeSet {
        ChangeSet {
            kind: ChangeSetKind::Data,
            origin_device_id: "peer-1".to_string(),
            origin_device_name: "Peer Mac".to_string(),
            watermark: Hlc {
                wall_ms: 100,
                counter: 1,
                device_id: "peer-1".to_string(),
            },
            segments: vec![serde_json::json!({"id": "seg-1"})],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn server_start_stop() {
        let mut server = LanPeerServer::new(
            "dev-1".to_string(),
            "Test".to_string(),
            "pass".to_string(),
            "fp123".to_string(),
        );
        assert!(!server.is_running());

        let port = server.start(b"cert", b"key", 0).await.unwrap();
        assert!(port > 0);
        assert!(server.is_running());

        server.stop();
        assert!(!server.is_running());
    }

    #[tokio::test]
    async fn info_endpoint() {
        let mut server = LanPeerServer::new(
            "dev-info".to_string(),
            "Info Test".to_string(),
            "pass".to_string(),
            "fp-info".to_string(),
        );
        let port = server.start(b"cert", b"key", 0).await.unwrap();

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/sync/info"))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let info: DeviceInfoResponse = resp.json().await.unwrap();
        assert_eq!(info.device_id, "dev-info");
        assert_eq!(info.device_name, "Info Test");
        assert_eq!(info.fingerprint, "fp-info");
        assert_eq!(info.protocol_version, PROTOCOL_VERSION);

        server.stop();
    }

    #[tokio::test]
    async fn pull_returns_204_when_empty() {
        let mut server = LanPeerServer::new(
            "dev-pull".to_string(),
            "Pull Test".to_string(),
            "pass".to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"cert", b"key", 0).await.unwrap();

        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "http://127.0.0.1:{port}/sync/pull?since_wall_ms=0&since_counter=0"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 204);
        server.stop();
    }

    #[tokio::test]
    async fn push_and_pull_roundtrip() {
        let passphrase = "test-roundtrip-pass";
        let mut server = LanPeerServer::new(
            "dev-rt".to_string(),
            "Roundtrip".to_string(),
            passphrase.to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"cert", b"key", 0).await.unwrap();
        let client = reqwest::Client::new();

        // Push an encrypted changeset
        let cs = test_changeset();
        let json = serde_json::to_vec(&cs).unwrap();
        let encrypted = sync_crypto::encrypt(passphrase, &json).unwrap();

        let push_resp = client
            .post(format!("http://127.0.0.1:{port}/sync/push"))
            .body(encrypted)
            .send()
            .await
            .unwrap();
        assert_eq!(push_resp.status(), 200);

        // Verify the server received it
        let received = server.drain_received();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].origin_device_id, "peer-1");

        // Enqueue an outbound changeset and pull it
        let outbound_cs = ChangeSet {
            origin_device_id: "dev-rt".to_string(),
            origin_device_name: "Roundtrip".to_string(),
            watermark: Hlc {
                wall_ms: 200,
                counter: 1,
                device_id: "dev-rt".to_string(),
            },
            segments: vec![serde_json::json!({"id": "seg-out"})],
            ..Default::default()
        };
        server.enqueue_outbound(outbound_cs);

        let pull_resp = client
            .get(format!(
                "http://127.0.0.1:{port}/sync/pull?since_wall_ms=0&since_counter=0"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(pull_resp.status(), 200);

        let pull_bytes = pull_resp.bytes().await.unwrap();
        let decrypted = sync_crypto::decrypt(passphrase, &pull_bytes).unwrap();
        let pulled: Vec<ChangeSet> = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(pulled.len(), 1);
        assert_eq!(pulled[0].origin_device_id, "dev-rt");

        server.stop();
    }

    #[tokio::test]
    async fn push_wrong_passphrase_returns_400() {
        let mut server = LanPeerServer::new(
            "dev-auth".to_string(),
            "Auth Test".to_string(),
            "correct-pass".to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"cert", b"key", 0).await.unwrap();

        // Encrypt with wrong passphrase
        let cs = test_changeset();
        let json = serde_json::to_vec(&cs).unwrap();
        let encrypted = sync_crypto::encrypt("wrong-pass", &json).unwrap();

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/sync/push"))
            .body(encrypted)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 400);
        server.stop();
    }

    #[tokio::test]
    async fn push_empty_body_returns_400() {
        let mut server = LanPeerServer::new(
            "dev-empty".to_string(),
            "Empty".to_string(),
            "pass".to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"cert", b"key", 0).await.unwrap();

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{port}/sync/push"))
            .body(vec![])
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 400);
        server.stop();
    }

    #[test]
    fn fingerprint_accessible() {
        let server = LanPeerServer::new(
            "dev-1".to_string(),
            "Test".to_string(),
            "pass".to_string(),
            "abc123def".to_string(),
        );
        assert_eq!(server.fingerprint(), "abc123def");
    }

    #[test]
    fn enqueue_and_drain() {
        let server = LanPeerServer::new(
            "dev-q".to_string(),
            "Queue".to_string(),
            "pass".to_string(),
            "fp".to_string(),
        );
        assert!(server.drain_received().is_empty());

        server.enqueue_outbound(test_changeset());
        // outbound is separate from received
        assert!(server.drain_received().is_empty());
    }
}
