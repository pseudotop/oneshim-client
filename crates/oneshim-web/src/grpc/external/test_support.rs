//! Shared test helpers for external gRPC integration tests.
//!
//! Gated on `#[cfg(any(test, feature = "test-support"))]` — the `test-support`
//! feature is strictly opt-in (NEVER enabled by default or transitively via
//! `grpc-dashboard-external`). Integration tests must invoke with
//! `--features grpc-dashboard-external,external-grpc-tools,test-support`.
//!
//! **ES256 note**: the `ring` crypto backend (default for rcgen) does NOT support
//! RSA key generation at runtime (RSA key-gen requires platform PRNG that ring
//! intentionally omits). All JWT helpers here use ES256 (P-256 ECDSA) which ring
//! fully supports.
//!
//! **rcgen 0.14 signed_by API**: `CertificateParams::signed_by` takes two arguments:
//! `(self, public_key: &impl PublicKeyData, issuer: &Issuer<'_, impl SigningKey>)`.
//! Create the `Issuer` using `Issuer::from_ca_cert_pem`.
//!
//! # Phase 0-9 test fixture API (CR4)
//!
//! - [`fixture_bridge`] / [`fixture_metrics`] — Tasks 0.6, 3.1
//! - [`InnerEcho`] — Tasks 0.6, 3.1 (trailer-status simulation)
//! - [`AuthContext::fixture`] / [`PeerInfo::fixture`] — Tasks 0.6, 3.1, 6.1
//! - [`PassthroughInner`] — Task 6.1
//! - [`connect_loopback`] / [`req_with_valid_auth`] — Task 9.4 G3 test dependencies

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Once, OnceLock};
use tempfile::TempDir;

// ── rustls crypto provider ────────────────────────────────────────────────────

static RUSTLS_INIT: Once = Once::new();

/// Install the aws-lc-rs CryptoProvider as the process-level default for rustls.
///
/// rustls 0.23 requires an explicit provider when both `aws-lc-rs` and `ring`
/// are present in the dependency graph. Tests that call
/// `rustls::ServerConfig::builder()` or `WebPkiClientVerifier::builder()`
/// must call this function first — those paths consult the process-level
/// default, which is unset unless installed explicitly.
///
/// Idempotent: the `Once` guard ensures the install runs at most once per
/// process, regardless of how many tests call this function.
pub fn install_rustls_crypto_provider() {
    RUSTLS_INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

// ── Server TLS cert pair ─────────────────────────────────────────────────────

/// Cached (TempDir, cert_pem_path, key_pem_path). The `TempDir` must be kept
/// alive for the lifetime of the process so the files remain on disk.
static CERT_CACHE: OnceLock<(TempDir, PathBuf, PathBuf)> = OnceLock::new();

/// Return cached (cert_pem_path, key_pem_path) for a self-signed server cert.
///
/// The cert has SANs `localhost` + `127.0.0.1`. Files are written once and
/// re-used across all tests in the same process invocation.
pub fn test_cert_pair() -> (PathBuf, PathBuf) {
    let (_, cp, kp) = CERT_CACHE.get_or_init(|| {
        use rcgen::{CertificateParams, KeyPair, SanType};
        let dir = TempDir::new().expect("TempDir for server cert");
        let kp = KeyPair::generate().expect("server keypair");
        let mut params =
            CertificateParams::new(vec!["localhost".to_string()]).expect("cert params");
        params
            .subject_alt_names
            .push(SanType::IpAddress(IpAddr::from([127, 0, 0, 1])));
        let cert = params.self_signed(&kp).expect("self-signed server cert");
        let cp = dir.path().join("cert.pem");
        let kp_p = dir.path().join("key.pem");
        std::fs::write(&cp, cert.pem()).expect("write cert.pem");
        std::fs::write(&kp_p, kp.serialize_pem()).expect("write key.pem");
        (dir, cp, kp_p)
    });
    (cp.clone(), kp.clone())
}

// ── JWT key pair ─────────────────────────────────────────────────────────────

/// JWT test key pair — public key path on disk + encoding key in memory.
pub struct TestJwt {
    /// Path to the EC public key PEM (used to configure `JwtVerifier`).
    pub pub_pem_path: PathBuf,
    /// Encoding key for minting tokens inside tests.
    pub enc_key: jsonwebtoken::EncodingKey,
    /// Keep-alive for the temp directory that holds the public key file.
    pub _dir: TempDir,
}

/// Generate an ES256 key pair and write the public key to a temp file.
///
/// ES256 is used instead of RS256 because the `ring` backend (default rcgen
/// feature) does not support RSA key generation. ES256 uses P-256 ECDSA which
/// ring fully supports.
pub fn test_jwt_keypair() -> TestJwt {
    use rcgen::{KeyPair, PKCS_ECDSA_P256_SHA256};
    let dir = TempDir::new().expect("TempDir for JWT keypair");
    let kp = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("EC P-256 keypair");
    let pub_pem = kp.public_key_pem();
    let pub_pem_path = dir.path().join("jwt_pub.pem");
    std::fs::write(&pub_pem_path, &pub_pem).expect("write jwt_pub.pem");
    // The encoding key needs the private key in EC PEM format.
    let enc_key = jsonwebtoken::EncodingKey::from_ec_pem(kp.serialize_pem().as_bytes())
        .expect("EncodingKey from EC PEM");
    TestJwt {
        pub_pem_path,
        enc_key,
        _dir: dir,
    }
}

/// Mint an ES256 JWT with the given claims.
///
/// - `exp_offset_secs`: added to `now()` for the `exp` claim. Use a negative
///   value to produce an already-expired token.
pub fn test_mint_jwt(
    enc: &jsonwebtoken::EncodingKey,
    sub: &str,
    iss: &str,
    aud: &str,
    exp_offset_secs: i64,
) -> String {
    use jsonwebtoken::{encode, Algorithm, Header};
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_secs() as i64;
    let claims = serde_json::json!({
        "sub": sub,
        "iss": iss,
        "aud": aud,
        "exp": now + exp_offset_secs,
        "iat": now,
    });
    encode(&Header::new(Algorithm::ES256), &claims, enc).expect("encode JWT")
}

// ── CA + client cert ─────────────────────────────────────────────────────────

/// CA + client certificate issued by that CA.
pub struct TestCaAndClient {
    /// Path to the CA cert PEM (used as `mtls_ca_path`).
    pub ca_pem_path: PathBuf,
    /// Path to the client cert PEM.
    pub client_cert_pem_path: PathBuf,
    /// Path to the client private key PEM.
    pub client_key_pem_path: PathBuf,
    /// Client cert DER bytes — convenience for `MtlsVerifier` tests.
    pub client_cert_der: Vec<u8>,
    /// Keep-alive for the temp directory.
    pub _dir: TempDir,
}

/// Generate a CA cert + a client cert signed by that CA.
///
/// `lifetime_hours` controls the client cert validity window. Use values
/// ≤ 48 for "accepted" tests and > 48 for "rejected" tests (the default
/// `mtls_max_cert_lifetime_hours` cap is 48).
///
/// **rcgen 0.14 API note**: `Issuer::new(params, key)` does not require the
/// `x509-parser` feature (unlike `from_ca_cert_pem`/`from_ca_cert_der`). We
/// build the CA cert via `self_signed` and simultaneously construct an `Issuer`
/// from a duplicate CA params set (rcgen 0.14 `Issuer::new` consumes params).
pub fn test_ca_and_client_cert(lifetime_hours: i64) -> TestCaAndClient {
    use chrono::{Datelike, Duration as ChronoDuration, Utc};
    use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};

    let dir = TempDir::new().expect("TempDir for CA+client certs");

    // ── CA ────────────────────────────────────────────────────────────────────
    let ca_kp = KeyPair::generate().expect("CA keypair");

    // Build CA params — we need two independent sets because both self_signed
    // (for the cert file) and Issuer::new (for signing the client cert) consume
    // CertificateParams. We construct the cert for the CA PEM file first, then
    // build the Issuer from a second params set.
    let make_ca_params = || -> CertificateParams {
        let mut p = CertificateParams::new(vec!["test-ca".to_string()]).expect("CA params");
        p.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        p.distinguished_name
            .push(rcgen::DnType::CommonName, "test-ca");
        p
    };

    // CA cert — written as PEM for `mtls_ca_path`.
    let ca_cert = make_ca_params()
        .self_signed(&ca_kp)
        .expect("CA self-signed cert");
    let ca_pem_path = dir.path().join("ca.pem");
    std::fs::write(&ca_pem_path, ca_cert.pem()).expect("write ca.pem");

    // Build an Issuer using a fresh CA params copy + the CA keypair.
    // Issuer::new does NOT need x509-parser (unlike from_ca_cert_pem).
    let issuer: Issuer<'_, KeyPair> = Issuer::new(make_ca_params(), ca_kp);

    // ── Client cert ───────────────────────────────────────────────────────────
    let client_kp = KeyPair::generate().expect("client keypair");
    let now = Utc::now();
    let expiry = now + ChronoDuration::hours(lifetime_hours);
    let mut client_params =
        CertificateParams::new(vec!["test-client".to_string()]).expect("client params");
    client_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "test-client");
    // rcgen 0.14 uses date_time_ymd helper for not_before / not_after.
    client_params.not_before = rcgen::date_time_ymd(now.year(), now.month() as u8, now.day() as u8);
    client_params.not_after =
        rcgen::date_time_ymd(expiry.year(), expiry.month() as u8, expiry.day() as u8);
    // rcgen 0.14: signed_by(public_key, &issuer) — public_key is the subject's key.
    let client_cert = client_params
        .signed_by(&client_kp, &issuer)
        .expect("client cert signed by CA");

    let client_cert_pem_path = dir.path().join("client_cert.pem");
    let client_key_pem_path = dir.path().join("client_key.pem");
    std::fs::write(&client_cert_pem_path, client_cert.pem()).expect("write client_cert.pem");
    std::fs::write(&client_key_pem_path, client_kp.serialize_pem()).expect("write client_key.pem");

    let client_cert_der = client_cert.der().to_vec();

    TestCaAndClient {
        ca_pem_path,
        client_cert_pem_path,
        client_key_pem_path,
        client_cert_der,
        _dir: dir,
    }
}

// ── AuthContext::fixture / PeerInfo::fixture ─────────────────────────────────

use super::conn_info::{AuthContext, AuthType, PeerInfo};

impl AuthContext {
    /// Canonical fixture used by Tasks 0.6, 3.1, 6.1, 9.4.
    ///
    /// Returns a deterministic `AuthContext` with `AuthType::Jwt`, a fixed
    /// `client_id`, a fixed `jti`, and a stable ULID-shaped `command_id`.
    pub fn fixture() -> Self {
        Self {
            auth_type: AuthType::Jwt,
            client_id: "test-client".into(),
            jti: Some("test-jti".into()),
            command_id: "01HXFIXTURE0000000000000000".into(),
        }
    }
}

impl PeerInfo {
    /// Canonical fixture used by Tasks 0.6, 3.1, 6.1, 9.4.
    ///
    /// Returns `127.0.0.1:50001` with no mTLS cert and TLS 1.3 version string.
    pub fn fixture() -> Self {
        Self {
            remote_addr: "127.0.0.1:50001".parse().expect("fixture addr"),
            peer_cert_der: None,
            cert_subject_cn: None,
            tls_version: "TLSv1.3".into(),
        }
    }
}

// ── InnerEcho — minimal tower::Service returning preset HTTP responses ────────

use std::convert::Infallible;
use std::task::{Context, Poll};

/// Minimal `tower::Service` test double that echoes a preset HTTP status code
/// into both the response status and a `grpc-status` trailer.
///
/// Used by Tasks 0.6 and 3.1 to simulate handler responses with specific
/// gRPC status codes without standing up a real tonic server.
///
/// # Variants
///
/// - `InnerEcho::with_trailer_status(code)` — body + `grpc-status` trailer
/// - `InnerEcho::trailers_only_with_status(code)` — empty body; trailer only
#[derive(Clone)]
pub struct InnerEcho {
    grpc_status: i32,
    body_bytes: &'static [u8],
}

impl InnerEcho {
    /// Return a response with a non-empty body and a `grpc-status` trailer.
    pub fn with_trailer_status(grpc_status: i32) -> Self {
        Self {
            grpc_status,
            body_bytes: b"body",
        }
    }

    /// Return a response with an empty body and only a `grpc-status` trailer.
    pub fn trailers_only_with_status(grpc_status: i32) -> Self {
        Self {
            grpc_status,
            body_bytes: b"",
        }
    }
}

impl<B> tower::Service<http::Request<B>> for InnerEcho
where
    B: Send + 'static,
{
    type Response = http::Response<Vec<u8>>;
    type Error = Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: http::Request<B>) -> Self::Future {
        let grpc_status = self.grpc_status;
        let body = self.body_bytes.to_vec();
        Box::pin(async move {
            let resp = http::Response::builder()
                .status(200)
                .header("content-type", "application/grpc")
                // grpc-status trailer encoded in header position (tonic convention
                // for unary responses — the value is identical; both positions are
                // inspected by the spec §3.1 audit-layer tests).
                .header("grpc-status", grpc_status.to_string())
                .body(body)
                .expect("InnerEcho response");
            Ok(resp)
        })
    }
}

// ── PassthroughInner — transparent Service that records call counts ───────────

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Transparent `tower::Service` that delegates to the inner service and counts
/// how many times `call` was invoked. Used by Task 6.1 to verify that
/// the audit or auth layer does (or does not) forward requests to the handler.
#[derive(Clone)]
pub struct PassthroughInner {
    /// Number of times `call` has been invoked.
    pub call_count: Arc<AtomicUsize>,
}

impl PassthroughInner {
    /// Create a new `PassthroughInner` with the counter at zero.
    pub fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Snapshot the call count at this instant.
    pub fn count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl Default for PassthroughInner {
    fn default() -> Self {
        Self::new()
    }
}

impl<B> tower::Service<http::Request<B>> for PassthroughInner
where
    B: Send + 'static,
{
    type Response = http::Response<Vec<u8>>;
    type Error = Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: http::Request<B>) -> Self::Future {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            Ok(http::Response::builder()
                .status(200)
                .body(vec![])
                .expect("PassthroughInner response"))
        })
    }
}

// ── fixture_bridge / fixture_metrics ─────────────────────────────────────────

use super::audit_bridge::AuditBridge;
use super::metrics::ExternalMetrics;
use oneshim_core::models::ai_session::SessionAuditEntry;
use oneshim_core::models::audit::{AuditEntry, AuditLevel, AuditStats, AuditStatus};
use oneshim_core::ports::audit_log::AuditLogPort;

/// Capturing mock that records every `log_complete_with_time` call.
///
/// Used by `fixture_bridge` so that Tasks 0.6 and 3.1 can assert on the
/// exact audit entries emitted by `AuditBridge::record` /
/// `AuditBridge::record_completion`.
pub struct MockRecorder {
    /// All entries captured by `log_complete_with_time`.
    pub entries: std::sync::Mutex<Vec<AuditEntry>>,
}

impl MockRecorder {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: std::sync::Mutex::new(vec![]),
        })
    }

    /// Snapshot entries without holding the lock past the call.
    pub fn snapshot(&self) -> Vec<AuditEntry> {
        self.entries.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl AuditLogPort for MockRecorder {
    async fn pending_count(&self) -> usize {
        0
    }
    async fn recent_entries(&self, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_status(&self, _s: &AuditStatus, _l: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_action_prefix(&self, _p: &str, _l: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn stats(&self) -> AuditStats {
        AuditStats::default()
    }
    async fn has_pending_batch(&self) -> bool {
        false
    }
    async fn log_event(&self, _a: &str, _s: &str, _d: &str) {}
    async fn log_start_if(&self, _l: AuditLevel, _c: &str, _s: &str, _a: &str) {}
    async fn log_complete_with_time(
        &self,
        _level: AuditLevel,
        command_id: &str,
        session_id: &str,
        details: &str,
        execution_time_ms: u64,
    ) {
        use ulid::Ulid;
        let status = serde_json::from_str::<serde_json::Value>(details)
            .ok()
            .and_then(|v| {
                v.get("result").and_then(|r| r.as_str()).map(|r| match r {
                    "ok" => AuditStatus::Completed,
                    "denied" => AuditStatus::Denied,
                    "timeout" => AuditStatus::Timeout,
                    _ => AuditStatus::Failed,
                })
            })
            .unwrap_or(AuditStatus::Completed);
        self.entries.lock().unwrap().push(AuditEntry {
            entry_id: Ulid::new().to_string(),
            timestamp: chrono::Utc::now(),
            session_id: session_id.into(),
            command_id: command_id.into(),
            action_type: "external_grpc".into(),
            status,
            details: Some(details.into()),
            execution_time_ms: Some(execution_time_ms),
        });
    }
    async fn drain_batch(&self) -> Vec<AuditEntry> {
        vec![]
    }
    async fn drain_all(&self) -> Vec<AuditEntry> {
        vec![]
    }
    async fn record_session_event(&self, _e: SessionAuditEntry) {}
}

/// Build an `(AuditBridge, Arc<MockRecorder>)` pair ready for unit tests.
///
/// The `MockRecorder` captures every `log_complete_with_time` call; the
/// `AuditBridge` delegates to it. Used by Tasks 0.6 and 3.1.
pub fn fixture_bridge() -> (AuditBridge, Arc<MockRecorder>) {
    let recorder = MockRecorder::new();
    let bridge = AuditBridge::new(recorder.clone() as Arc<dyn AuditLogPort>);
    (bridge, recorder)
}

/// Build a fresh `Arc<ExternalMetrics>` pre-zeroed. Used by Tasks 0.6 and 3.1.
pub fn fixture_metrics() -> Arc<ExternalMetrics> {
    Arc::new(ExternalMetrics::new())
}

// ── connect_loopback / req_with_valid_auth ────────────────────────────────────
// Task 9.4 G3 test dependencies — created in Task 0.0 per plan §Step 2.

/// Connect to a loopback gRPC server for test-local use (no TLS).
///
/// Returns a connected `tonic::transport::Channel` pointing at
/// `http://127.0.0.1:{port}`. Use only for in-process integration tests
/// where the server is also bound to localhost without TLS.
pub async fn connect_loopback(port: u16) -> tonic::transport::Channel {
    let addr = format!("http://127.0.0.1:{port}");
    tonic::transport::Channel::from_shared(addr)
        .expect("valid loopback URI")
        .connect()
        .await
        .expect("connect to loopback server")
}

/// Build a test request with a bearer authorization header.
///
/// Inserts `Authorization: Bearer TEST_TOKEN_PLACEHOLDER` into the request
/// metadata. Task 9.4 implementers should replace `TEST_TOKEN_PLACEHOLDER`
/// with a real token minted via `test_mint_jwt` using the test ES256 keypair.
pub fn req_with_valid_auth<T>(body: T) -> tonic::Request<T> {
    let mut req = tonic::Request::new(body);
    req.metadata_mut().insert(
        "authorization",
        tonic::metadata::MetadataValue::from_static("Bearer TEST_TOKEN_PLACEHOLDER"),
    );
    req
}
