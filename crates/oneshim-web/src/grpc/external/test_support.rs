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
