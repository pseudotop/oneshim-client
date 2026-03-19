//! LanPeerServer -- Axum HTTPS peer server for LAN sync (Phase 3b-2).
//!
//! Lightweight server that serves changesets to authenticated LAN peers.
//! Uses self-signed TLS certificates (via `axum-server` + `rustls`) and
//! HMAC challenge-response authentication before allowing data exchange.
//!
//! ## Endpoints
//!
//! | Method | Path               | Auth     | Description                          |
//! |--------|--------------------|----------|--------------------------------------|
//! | GET    | `/sync/info`       | None     | Device info + protocol version       |
//! | POST   | `/sync/challenge`  | None     | Request an HMAC nonce for auth       |
//! | POST   | `/sync/verify`     | None     | Submit HMAC response, receive token  |
//! | GET    | `/sync/pull`       | Token    | Return encrypted changesets since HLC|
//! | POST   | `/sync/push`       | Token    | Receive encrypted changeset from peer|
//!
//! ## Authentication Flow
//!
//! 1. Client POSTs `/sync/challenge` with `{ "device_id": "..." }`
//! 2. Server returns `{ "nonce": "<hex>" }`
//! 3. Client computes `HMAC-SHA256(nonce, Argon2id(passphrase, peer_salt))`
//! 4. Client POSTs `/sync/verify` with `{ "device_id": "...", "nonce": "<hex>", "response": "<hex>" }`
//! 5. Server verifies and returns `{ "session_token": "<hex>", "expires_in_secs": 3600 }`
//! 6. Client includes `Authorization: Bearer <token>` header on `/sync/push` and `/sync/pull`
//!
//! ## TLS
//!
//! When valid PEM cert/key are provided, the server binds with TLS via `axum-server`.
//! If the cert/key are invalid or empty (e.g., in tests), falls back to plain HTTP.
//!
//! Requires the `lan-sync` feature flag.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use oneshim_core::error::CoreError;
use oneshim_core::models::sync::ChangeSet;
use oneshim_core::sync::Hlc;

use super::lan_crypto;
use super::sync_crypto;

/// Protocol version for compatibility negotiation.
const PROTOCOL_VERSION: &str = "1";

/// Maximum request body size: 16 MiB.
const MAX_BODY_SIZE: usize = 16 * 1024 * 1024;

/// Maximum number of outbound changesets held in memory.
/// When full, the oldest entries are evicted to make room.
const MAX_OUTBOUND_QUEUE: usize = 1000;

/// Session token TTL: 1 hour.
const SESSION_TTL: Duration = Duration::from_secs(3600);

/// Maximum number of active sessions (prevents memory exhaustion).
const MAX_SESSIONS: usize = 100;

/// Maximum number of pending nonces (prevents memory exhaustion).
const MAX_PENDING_NONCES: usize = 200;

/// Nonce TTL: 60 seconds (nonces must be used quickly).
const NONCE_TTL: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

/// Device info returned by `GET /sync/info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfoResponse {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub protocol_version: String,
}

/// Request body for `POST /sync/challenge`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeRequest {
    pub device_id: String,
}

/// Response from `POST /sync/challenge`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeResponse {
    pub nonce: String,
}

/// Request body for `POST /sync/verify`.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyRequest {
    pub device_id: String,
    pub nonce: String,
    pub response: String,
}

/// Response from `POST /sync/verify`.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub session_token: String,
    pub expires_in_secs: u64,
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

// ---------------------------------------------------------------------------
// Session store
// ---------------------------------------------------------------------------

/// A pending nonce awaiting verification.
struct PendingNonce {
    nonce_bytes: Vec<u8>,
    peer_device_id: String,
    created_at: Instant,
}

/// An authenticated session.
struct Session {
    #[allow(dead_code)]
    peer_device_id: String,
    created_at: Instant,
}

/// Thread-safe session store for HMAC challenge-response authentication.
#[derive(Clone)]
struct SessionStore {
    /// Pending nonces: nonce_hex -> PendingNonce
    pending: Arc<RwLock<HashMap<String, PendingNonce>>>,
    /// Active sessions: token_hex -> Session
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl SessionStore {
    fn new() -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate a random nonce and store it as pending.
    fn create_nonce(&self, peer_device_id: &str) -> Vec<u8> {
        let mut nonce = vec![0u8; 32];
        rand::rng().fill_bytes(&mut nonce);
        let hex_key = hex::encode(&nonce);

        let mut pending = self.pending.write();
        // Evict expired nonces
        let now = Instant::now();
        pending.retain(|_, v| now.duration_since(v.created_at) < NONCE_TTL);
        // Evict oldest if at capacity
        if pending.len() >= MAX_PENDING_NONCES {
            if let Some(oldest_key) = pending
                .iter()
                .min_by_key(|(_, v)| v.created_at)
                .map(|(k, _)| k.clone())
            {
                pending.remove(&oldest_key);
            }
        }
        pending.insert(
            hex_key,
            PendingNonce {
                nonce_bytes: nonce.clone(),
                peer_device_id: peer_device_id.to_string(),
                created_at: Instant::now(),
            },
        );
        nonce
    }

    /// Consume a pending nonce (one-time use). Returns (nonce_bytes, peer_device_id).
    fn take_nonce(&self, nonce_hex: &str) -> Option<(Vec<u8>, String)> {
        let mut pending = self.pending.write();
        let entry = pending.remove(nonce_hex)?;
        // Check expiry
        if Instant::now().duration_since(entry.created_at) >= NONCE_TTL {
            return None;
        }
        Some((entry.nonce_bytes, entry.peer_device_id))
    }

    /// Create a session token for an authenticated peer.
    fn create_session(&self, peer_device_id: &str) -> String {
        let mut token = vec![0u8; 32];
        rand::rng().fill_bytes(&mut token);
        let token_hex = hex::encode(&token);

        let mut sessions = self.sessions.write();
        // Evict expired sessions
        let now = Instant::now();
        sessions.retain(|_, v| now.duration_since(v.created_at) < SESSION_TTL);
        // Evict oldest if at capacity
        if sessions.len() >= MAX_SESSIONS {
            if let Some(oldest_key) = sessions
                .iter()
                .min_by_key(|(_, v)| v.created_at)
                .map(|(k, _)| k.clone())
            {
                sessions.remove(&oldest_key);
            }
        }
        sessions.insert(
            token_hex.clone(),
            Session {
                peer_device_id: peer_device_id.to_string(),
                created_at: Instant::now(),
            },
        );
        token_hex
    }

    /// Validate a session token. Returns true if valid and not expired.
    fn validate_token(&self, token: &str) -> bool {
        let sessions = self.sessions.read();
        match sessions.get(token) {
            Some(session) => Instant::now().duration_since(session.created_at) < SESSION_TTL,
            None => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

/// Shared state for the Axum server.
#[derive(Clone)]
struct ServerState {
    device_id: String,
    device_name: String,
    fingerprint: String,
    passphrase: String,
    session_store: SessionStore,
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
    tls_enabled: bool,
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
            tls_enabled: false,
            received_changesets: Arc::new(RwLock::new(Vec::new())),
            outbound_changesets: Arc::new(RwLock::new(Vec::new())),
            shutdown_tx: None,
            server_handle: None,
            running: false,
        }
    }

    /// Start the HTTPS server on the specified port (0 = ephemeral OS-assigned).
    ///
    /// Returns the actual bound port. The server runs in a background tokio task
    /// and can be stopped via `stop()`.
    ///
    /// ## TLS
    ///
    /// If valid PEM-encoded `cert_pem` and `key_pem` are provided, the server
    /// binds with TLS via `axum-server` + `rustls`. Otherwise falls back to
    /// plain HTTP (e.g., in unit tests or when cert generation fails).
    pub async fn start(
        &mut self,
        cert_pem: &[u8],
        key_pem: &[u8],
        port: u16,
    ) -> Result<u16, CoreError> {
        if self.running {
            return Ok(self.port);
        }

        let state = ServerState {
            device_id: self.device_id.clone(),
            device_name: self.device_name.clone(),
            fingerprint: self.fingerprint.clone(),
            passphrase: self.passphrase.clone(),
            session_store: SessionStore::new(),
            received_changesets: Arc::clone(&self.received_changesets),
            outbound_changesets: Arc::clone(&self.outbound_changesets),
        };

        let app = build_router(state);
        let addr = SocketAddr::from(([0, 0, 0, 0], port));

        // Try TLS, fall back to plain HTTP on failure
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let tls_config = try_build_tls_config(cert_pem, key_pem).await;
        let tls_enabled = tls_config.is_some();

        let actual_port = if let Some(config) = tls_config {
            // TLS path via axum-server.
            // Bind a TcpListener first to discover the actual port (important
            // when the requested port is 0), then hand the std listener to
            // axum-server for TLS wrapping.
            let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
                CoreError::Internal(format!("failed to bind LAN server on {addr}: {e}"))
            })?;
            let actual_addr = listener
                .local_addr()
                .map_err(|e| CoreError::Internal(format!("failed to get local addr: {e}")))?;
            let bound_port = actual_addr.port();

            // Convert to std listener for axum-server
            let std_listener = listener
                .into_std()
                .map_err(|e| CoreError::Internal(format!("failed to convert listener: {e}")))?;

            let axum_handle = axum_server::Handle::new();
            let shutdown_handle = axum_handle.clone();

            let tls_server = axum_server::from_tcp_rustls(std_listener, config)
                .map_err(|e| CoreError::Internal(format!("TLS server init: {e}")))?;

            let handle = tokio::task::spawn(async move {
                // Shutdown listener: wait for the oneshot then trigger graceful shutdown
                tokio::spawn(async move {
                    let _ = shutdown_rx.await;
                    shutdown_handle.graceful_shutdown(Some(Duration::from_secs(2)));
                });

                let serve_result = tls_server
                    .handle(axum_handle)
                    .serve(app.into_make_service())
                    .await;

                if let Err(e) = serve_result {
                    error!("LAN peer server (TLS) error: {e}");
                }
            });

            self.server_handle = Some(handle);
            bound_port
        } else {
            // Plain HTTP fallback
            let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
                CoreError::Internal(format!("failed to bind LAN server on {addr}: {e}"))
            })?;

            let actual_port = listener
                .local_addr()
                .map_err(|e| CoreError::Internal(format!("failed to get local addr: {e}")))?
                .port();

            let handle = tokio::task::spawn(async move {
                let serve_result = axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = shutdown_rx.await;
                    })
                    .await;

                if let Err(e) = serve_result {
                    error!("LAN peer server (HTTP) error: {e}");
                }
            });

            self.server_handle = Some(handle);
            actual_port
        };

        self.port = actual_port;
        self.shutdown_tx = Some(shutdown_tx);
        self.tls_enabled = tls_enabled;
        self.running = true;

        info!(
            port = actual_port,
            tls = tls_enabled,
            device_id = %self.device_id,
            "LAN peer server started"
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

    /// Check if TLS is enabled.
    pub fn is_tls_enabled(&self) -> bool {
        self.tls_enabled
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
// TLS configuration
// ---------------------------------------------------------------------------

/// Attempt to build a `rustls` `ServerConfig` from PEM cert/key bytes.
///
/// Returns `None` if the input is empty or malformed, allowing fallback
/// to plain HTTP. This keeps the server usable in tests and on first
/// run before certificates are generated.
async fn try_build_tls_config(
    cert_pem: &[u8],
    key_pem: &[u8],
) -> Option<axum_server::tls_rustls::RustlsConfig> {
    if cert_pem.is_empty() || key_pem.is_empty() {
        debug!("empty cert/key -- TLS disabled, using plain HTTP");
        return None;
    }

    // Parse certificate chain from PEM
    let cert_reader = &mut std::io::BufReader::new(cert_pem);
    let certs: Vec<_> = rustls_pemfile::certs(cert_reader)
        .filter_map(|r| r.ok())
        .collect();

    if certs.is_empty() {
        warn!("no valid certificates in PEM data -- TLS disabled");
        return None;
    }

    // Parse private key from PEM
    let key_reader = &mut std::io::BufReader::new(key_pem);
    let key = rustls_pemfile::private_key(key_reader).ok().flatten();

    let key = match key {
        Some(k) => k,
        None => {
            warn!("no valid private key in PEM data -- TLS disabled");
            return None;
        }
    };

    // Build axum-server RustlsConfig (async constructor)
    let config = axum_server::tls_rustls::RustlsConfig::from_der(
        certs.into_iter().map(|c| c.to_vec()).collect(),
        key.secret_der().to_vec(),
    )
    .await;

    match config {
        Ok(c) => {
            debug!("TLS configuration built successfully");
            Some(c)
        }
        Err(e) => {
            warn!("failed to build TLS config: {e} -- TLS disabled");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Axum router + handlers
// ---------------------------------------------------------------------------

fn build_router(state: ServerState) -> Router {
    Router::new()
        // Public endpoints (no auth required)
        .route("/sync/info", get(handle_info))
        .route("/sync/challenge", post(handle_challenge))
        .route("/sync/verify", post(handle_verify))
        // Protected endpoints (session token required)
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

/// POST /sync/challenge -- generate and return a random nonce.
///
/// The peer must compute `HMAC-SHA256(nonce, derived_key)` and submit it
/// via `/sync/verify` to obtain a session token.
async fn handle_challenge(
    State(state): State<ServerState>,
    Json(req): Json<ChallengeRequest>,
) -> impl IntoResponse {
    if req.device_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "device_id required"})),
        )
            .into_response();
    }

    let nonce = state.session_store.create_nonce(&req.device_id);
    let nonce_hex = hex::encode(&nonce);

    debug!(
        peer_device_id = %req.device_id,
        "challenge nonce issued"
    );

    Json(ChallengeResponse { nonce: nonce_hex }).into_response()
}

/// POST /sync/verify -- verify the HMAC challenge response and issue a session token.
async fn handle_verify(
    State(state): State<ServerState>,
    Json(req): Json<VerifyRequest>,
) -> impl IntoResponse {
    if req.device_id.is_empty() || req.nonce.is_empty() || req.response.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "device_id, nonce, and response required"})),
        )
            .into_response();
    }

    // Consume the pending nonce (one-time use)
    let (nonce_bytes, expected_peer_id) = match state.session_store.take_nonce(&req.nonce) {
        Some(v) => v,
        None => {
            warn!(
                peer_device_id = %req.device_id,
                "verify failed: unknown or expired nonce"
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "invalid or expired nonce"})),
            )
                .into_response();
        }
    };

    // Verify the device_id matches
    if req.device_id != expected_peer_id {
        warn!(
            expected = %expected_peer_id,
            actual = %req.device_id,
            "verify failed: device_id mismatch"
        );
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "device_id mismatch"})),
        )
            .into_response();
    }

    // Decode the HMAC response
    let response_bytes = match hex::decode(&req.response) {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid hex response"})),
            )
                .into_response();
        }
    };

    // Verify the HMAC
    let valid = match lan_crypto::verify_challenge_response(
        &nonce_bytes,
        &response_bytes,
        &state.passphrase,
        &state.device_id, // local device (server)
        &req.device_id,   // peer device (client)
    ) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "HMAC verification error");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "verification error"})),
            )
                .into_response();
        }
    };

    if !valid {
        warn!(
            peer_device_id = %req.device_id,
            "verify failed: HMAC mismatch (wrong passphrase?)"
        );
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "authentication failed"})),
        )
            .into_response();
    }

    // Issue a session token
    let token = state.session_store.create_session(&req.device_id);

    info!(
        peer_device_id = %req.device_id,
        "peer authenticated via HMAC challenge-response"
    );

    Json(VerifyResponse {
        session_token: token,
        expires_in_secs: SESSION_TTL.as_secs(),
    })
    .into_response()
}

/// Extract and validate the session token from the Authorization header.
///
/// Expects: `Authorization: Bearer <token_hex>`
fn extract_session_token(
    headers: &HeaderMap,
    session_store: &SessionStore,
) -> Result<(), StatusCode> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = auth_header.strip_prefix("Bearer ").unwrap_or("");

    if token.is_empty() || !session_store.validate_token(token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

/// GET /sync/pull -- return encrypted changesets newer than the given HLC.
///
/// Requires a valid session token via `Authorization: Bearer <token>`.
/// Query parameters: `since_wall_ms`, `since_counter`, `device_id`.
/// Response: AES-256-GCM encrypted JSON array of changesets, or 204 if none.
async fn handle_pull(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(params): Query<PullQuery>,
) -> impl IntoResponse {
    // Authenticate
    if let Err(status) = extract_session_token(&headers, &state.session_store) {
        return (status, "unauthorized").into_response();
    }

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
/// Requires a valid session token via `Authorization: Bearer <token>`.
/// Request body: AES-256-GCM encrypted JSON changeset.
/// Response: 200 OK on success, 400 on decryption/parse failure.
async fn handle_push(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Authenticate
    if let Err(status) = extract_session_token(&headers, &state.session_store) {
        return (status, "unauthorized").into_response();
    }

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

    /// Helper: perform challenge-response to get a session token.
    async fn authenticate(
        client: &reqwest::Client,
        base: &str,
        passphrase: &str,
        local_device_id: &str,
        server_device_id: &str,
    ) -> String {
        // Step 1: request challenge
        let challenge_resp = client
            .post(format!("{base}/sync/challenge"))
            .json(&ChallengeRequest {
                device_id: local_device_id.to_string(),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(challenge_resp.status(), 200);
        let challenge: ChallengeResponse = challenge_resp.json().await.unwrap();

        // Step 2: compute HMAC response
        let nonce_bytes = hex::decode(&challenge.nonce).unwrap();
        let hmac_response = lan_crypto::compute_challenge_response(
            &nonce_bytes,
            passphrase,
            local_device_id,
            server_device_id,
        )
        .unwrap();

        // Step 3: verify
        let verify_resp = client
            .post(format!("{base}/sync/verify"))
            .json(&VerifyRequest {
                device_id: local_device_id.to_string(),
                nonce: challenge.nonce.clone(),
                response: hex::encode(&hmac_response),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(verify_resp.status(), 200);
        let verify: VerifyResponse = verify_resp.json().await.unwrap();
        assert!(!verify.session_token.is_empty());
        assert!(verify.expires_in_secs > 0);

        verify.session_token
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
        assert!(!server.is_tls_enabled()); // invalid PEM -> fallback

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
    async fn challenge_verify_flow() {
        let passphrase = "shared-secret";
        let mut server = LanPeerServer::new(
            "server-dev".to_string(),
            "Server".to_string(),
            passphrase.to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"", b"", 0).await.unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        let token = authenticate(&client, &base, passphrase, "client-dev", "server-dev").await;
        assert!(!token.is_empty());
        assert_eq!(token.len(), 64); // 32 bytes hex-encoded

        server.stop();
    }

    #[tokio::test]
    async fn challenge_wrong_passphrase_fails() {
        let mut server = LanPeerServer::new(
            "server-dev".to_string(),
            "Server".to_string(),
            "correct-pass".to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"", b"", 0).await.unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        // Get challenge
        let challenge_resp = client
            .post(format!("{base}/sync/challenge"))
            .json(&ChallengeRequest {
                device_id: "client-dev".to_string(),
            })
            .send()
            .await
            .unwrap();
        let challenge: ChallengeResponse = challenge_resp.json().await.unwrap();
        let nonce_bytes = hex::decode(&challenge.nonce).unwrap();

        // Compute HMAC with wrong passphrase
        let hmac_response = lan_crypto::compute_challenge_response(
            &nonce_bytes,
            "wrong-pass",
            "client-dev",
            "server-dev",
        )
        .unwrap();

        // Verify should fail
        let verify_resp = client
            .post(format!("{base}/sync/verify"))
            .json(&VerifyRequest {
                device_id: "client-dev".to_string(),
                nonce: challenge.nonce,
                response: hex::encode(&hmac_response),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(verify_resp.status(), 401);

        server.stop();
    }

    #[tokio::test]
    async fn pull_push_require_auth() {
        let mut server = LanPeerServer::new(
            "dev-auth".to_string(),
            "Auth".to_string(),
            "pass".to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"", b"", 0).await.unwrap();
        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{port}");

        // Pull without token -> 401
        let pull_resp = client
            .get(format!("{base}/sync/pull?since_wall_ms=0&since_counter=0"))
            .send()
            .await
            .unwrap();
        assert_eq!(pull_resp.status(), 401);

        // Push without token -> 401
        let push_resp = client
            .post(format!("{base}/sync/push"))
            .body(vec![1, 2, 3])
            .send()
            .await
            .unwrap();
        assert_eq!(push_resp.status(), 401);

        // Pull with invalid token -> 401
        let pull_resp = client
            .get(format!("{base}/sync/pull?since_wall_ms=0&since_counter=0"))
            .header("authorization", "Bearer invalid-token")
            .send()
            .await
            .unwrap();
        assert_eq!(pull_resp.status(), 401);

        server.stop();
    }

    #[tokio::test]
    async fn pull_returns_204_when_empty() {
        let passphrase = "test-pass";
        let mut server = LanPeerServer::new(
            "dev-pull".to_string(),
            "Pull Test".to_string(),
            passphrase.to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"", b"", 0).await.unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        let token = authenticate(&client, &base, passphrase, "client-dev", "dev-pull").await;

        let resp = client
            .get(format!("{base}/sync/pull?since_wall_ms=0&since_counter=0"))
            .header("authorization", format!("Bearer {token}"))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 204);
        server.stop();
    }

    #[tokio::test]
    async fn push_and_pull_roundtrip() {
        let passphrase = "test-roundtrip-pass";
        let server_id = "dev-rt";
        let client_id = "client-rt";

        let mut server = LanPeerServer::new(
            server_id.to_string(),
            "Roundtrip".to_string(),
            passphrase.to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"", b"", 0).await.unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        // Authenticate
        let token = authenticate(&client, &base, passphrase, client_id, server_id).await;

        // Push an encrypted changeset
        let cs = test_changeset();
        let json = serde_json::to_vec(&cs).unwrap();
        let encrypted = sync_crypto::encrypt(passphrase, &json).unwrap();

        let push_resp = client
            .post(format!("{base}/sync/push"))
            .header("authorization", format!("Bearer {token}"))
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
            origin_device_id: server_id.to_string(),
            origin_device_name: "Roundtrip".to_string(),
            watermark: Hlc {
                wall_ms: 200,
                counter: 1,
                device_id: server_id.to_string(),
            },
            segments: vec![serde_json::json!({"id": "seg-out"})],
            ..Default::default()
        };
        server.enqueue_outbound(outbound_cs);

        let pull_resp = client
            .get(format!("{base}/sync/pull?since_wall_ms=0&since_counter=0"))
            .header("authorization", format!("Bearer {token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(pull_resp.status(), 200);

        let pull_bytes = pull_resp.bytes().await.unwrap();
        let decrypted = sync_crypto::decrypt(passphrase, &pull_bytes).unwrap();
        let pulled: Vec<ChangeSet> = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(pulled.len(), 1);
        assert_eq!(pulled[0].origin_device_id, server_id);

        server.stop();
    }

    #[tokio::test]
    async fn push_wrong_passphrase_returns_400() {
        let server_pass = "correct-pass";
        let server_id = "dev-auth";

        let mut server = LanPeerServer::new(
            server_id.to_string(),
            "Auth Test".to_string(),
            server_pass.to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"", b"", 0).await.unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        // Authenticate with correct passphrase
        let token = authenticate(&client, &base, server_pass, "client-1", server_id).await;

        // But encrypt the payload with wrong passphrase
        let cs = test_changeset();
        let json = serde_json::to_vec(&cs).unwrap();
        let encrypted = sync_crypto::encrypt("wrong-pass", &json).unwrap();

        let resp = client
            .post(format!("{base}/sync/push"))
            .header("authorization", format!("Bearer {token}"))
            .body(encrypted)
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 400);
        server.stop();
    }

    #[tokio::test]
    async fn push_empty_body_returns_400() {
        let passphrase = "pass";
        let server_id = "dev-empty";

        let mut server = LanPeerServer::new(
            server_id.to_string(),
            "Empty".to_string(),
            passphrase.to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"", b"", 0).await.unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        let token = authenticate(&client, &base, passphrase, "client-1", server_id).await;

        let resp = client
            .post(format!("{base}/sync/push"))
            .header("authorization", format!("Bearer {token}"))
            .body(vec![])
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 400);
        server.stop();
    }

    #[tokio::test]
    async fn nonce_is_single_use() {
        let passphrase = "single-use";
        let server_id = "dev-nonce";

        let mut server = LanPeerServer::new(
            server_id.to_string(),
            "Nonce".to_string(),
            passphrase.to_string(),
            "fp".to_string(),
        );
        let port = server.start(b"", b"", 0).await.unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        // Get a challenge
        let challenge_resp = client
            .post(format!("{base}/sync/challenge"))
            .json(&ChallengeRequest {
                device_id: "client-dev".to_string(),
            })
            .send()
            .await
            .unwrap();
        let challenge: ChallengeResponse = challenge_resp.json().await.unwrap();
        let nonce_bytes = hex::decode(&challenge.nonce).unwrap();

        // First verify should succeed
        let hmac_response = lan_crypto::compute_challenge_response(
            &nonce_bytes,
            passphrase,
            "client-dev",
            server_id,
        )
        .unwrap();

        let verify_resp = client
            .post(format!("{base}/sync/verify"))
            .json(&VerifyRequest {
                device_id: "client-dev".to_string(),
                nonce: challenge.nonce.clone(),
                response: hex::encode(&hmac_response),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(verify_resp.status(), 200);

        // Second verify with same nonce should fail (consumed)
        let verify_resp2 = client
            .post(format!("{base}/sync/verify"))
            .json(&VerifyRequest {
                device_id: "client-dev".to_string(),
                nonce: challenge.nonce,
                response: hex::encode(&hmac_response),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(verify_resp2.status(), 401);

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

    #[test]
    fn session_store_basics() {
        let store = SessionStore::new();

        // Create nonce
        let nonce = store.create_nonce("peer-1");
        assert_eq!(nonce.len(), 32);

        // Take nonce -- single use
        let hex = hex::encode(&nonce);
        let taken = store.take_nonce(&hex);
        assert!(taken.is_some());
        let (bytes, peer_id) = taken.unwrap();
        assert_eq!(bytes, nonce);
        assert_eq!(peer_id, "peer-1");

        // Second take fails
        assert!(store.take_nonce(&hex).is_none());

        // Create and validate session
        let token = store.create_session("peer-1");
        assert!(store.validate_token(&token));
        assert!(!store.validate_token("invalid"));
    }

    #[tokio::test]
    async fn tls_with_real_cert() {
        use super::super::lan_tls;

        // Generate a real self-signed cert
        let (cert_pem, key_pem) = lan_tls::generate_self_signed_cert("test-tls-dev").unwrap();

        let mut server = LanPeerServer::new(
            "test-tls-dev".to_string(),
            "TLS Test".to_string(),
            "pass".to_string(),
            "fp-tls".to_string(),
        );
        let port = server.start(&cert_pem, &key_pem, 0).await.unwrap();
        assert!(port > 0);
        assert!(server.is_running());
        assert!(server.is_tls_enabled());

        // Verify TLS is active by confirming a plain HTTP connection fails.
        // Note: reqwest in this project uses native-tls, which may have protocol
        // compatibility issues with rustls on some platforms. So instead we verify
        // that plain HTTP to a TLS port gets rejected (connection reset or error).
        let plain_result = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/sync/info"))
            .send()
            .await;
        assert!(
            plain_result.is_err(),
            "plain HTTP should not succeed against TLS server"
        );

        server.stop();
    }
}
