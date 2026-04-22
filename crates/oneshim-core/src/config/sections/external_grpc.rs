//! External gRPC configuration (D13 V2c).
//!
//! Opt-in via `enabled: true`. Default `enabled: false` means zero
//! behavior change vs pre-v2c. All network-related defaults assume the
//! opt-in case (bind 0.0.0.0:10092, max 1024 TCP connections, etc.).

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// External gRPC configuration. Loaded from the user config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalGrpcConfig {
    /// Default false — external binding is opt-in.
    #[serde(default)]
    pub enabled: bool,

    /// IP or interface to bind. Default 0.0.0.0.
    #[serde(default = "default_bind_address")]
    pub bind_address: IpAddr,

    /// Port for external binding. Default 10092.
    #[serde(default = "default_external_port")]
    pub port: u16,

    /// Path to TLS server certificate PEM.
    #[serde(default)]
    pub tls_cert_path: Option<PathBuf>,

    /// Path to TLS server private key PEM.
    #[serde(default)]
    pub tls_key_path: Option<PathBuf>,

    /// Auth mode.
    #[serde(default)]
    pub auth_mode: Option<AuthMode>,

    /// JWT signing algorithm. RS256 or ES256 (asymmetric only).
    #[serde(default)]
    pub jwt_algorithm: Option<JwtAlgorithm>,

    /// Path to JWT public key PEM.
    #[serde(default)]
    pub jwt_public_key_path: Option<PathBuf>,

    /// Expected `iss` claim.
    #[serde(default)]
    pub jwt_expected_issuer: Option<String>,

    /// Expected `aud` claim.
    #[serde(default)]
    pub jwt_expected_audience: Option<String>,

    /// Path to mTLS CA PEM.
    #[serde(default)]
    pub mtls_ca_path: Option<PathBuf>,

    /// Path to mTLS fingerprint allowlist.
    #[serde(default)]
    pub mtls_fingerprint_allowlist_path: Option<PathBuf>,

    /// Max allowed client cert lifetime in hours.
    #[serde(default = "default_mtls_max_cert_lifetime")]
    pub mtls_max_cert_lifetime_hours: u32,

    /// Max concurrent streams per server.
    #[serde(default = "default_external_max_streams")]
    pub max_concurrent_streams: usize,

    /// Max concurrent TCP connections.
    #[serde(default = "default_external_max_connections")]
    pub max_connections: usize,

    /// Rate limiter burst capacity.
    #[serde(default = "default_external_burst_capacity")]
    pub burst_capacity: usize,
}

impl Default for ExternalGrpcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_address: default_bind_address(),
            port: default_external_port(),
            tls_cert_path: None,
            tls_key_path: None,
            auth_mode: None,
            jwt_algorithm: None,
            jwt_public_key_path: None,
            jwt_expected_issuer: None,
            jwt_expected_audience: None,
            mtls_ca_path: None,
            mtls_fingerprint_allowlist_path: None,
            mtls_max_cert_lifetime_hours: default_mtls_max_cert_lifetime(),
            max_concurrent_streams: default_external_max_streams(),
            max_connections: default_external_max_connections(),
            burst_capacity: default_external_burst_capacity(),
        }
    }
}

fn default_bind_address() -> IpAddr {
    IpAddr::V4(Ipv4Addr::UNSPECIFIED)
}
fn default_external_port() -> u16 {
    10092
}
fn default_mtls_max_cert_lifetime() -> u32 {
    48
}
fn default_external_max_streams() -> usize {
    16
}
fn default_external_max_connections() -> usize {
    1024
}
fn default_external_burst_capacity() -> usize {
    10
}

/// Authentication mode for external gRPC.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthMode {
    #[serde(rename = "jwt")]
    Jwt,
    #[serde(rename = "mtls")]
    Mtls,
    #[serde(rename = "jwt+mtls")]
    JwtAndMtls,
}

impl AuthMode {
    pub fn includes_jwt(&self) -> bool {
        matches!(self, Self::Jwt | Self::JwtAndMtls)
    }
    pub fn includes_mtls(&self) -> bool {
        matches!(self, Self::Mtls | Self::JwtAndMtls)
    }
}

/// JWT signing algorithm. Asymmetric-only.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum JwtAlgorithm {
    #[serde(rename = "RS256")]
    Rs256,
    #[serde(rename = "ES256")]
    Es256,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_mode_serde_jwt_only() {
        let s = serde_json::to_string(&AuthMode::Jwt).unwrap();
        assert_eq!(s, "\"jwt\"");
        let parsed: AuthMode = serde_json::from_str("\"jwt\"").unwrap();
        assert_eq!(parsed, AuthMode::Jwt);
    }

    #[test]
    fn auth_mode_serde_mtls() {
        let s = serde_json::to_string(&AuthMode::Mtls).unwrap();
        assert_eq!(s, "\"mtls\"");
    }

    #[test]
    fn auth_mode_serde_jwt_plus_mtls_preserves_plus() {
        let s = serde_json::to_string(&AuthMode::JwtAndMtls).unwrap();
        assert_eq!(s, "\"jwt+mtls\"", "'+' character must be preserved");
        let parsed: AuthMode = serde_json::from_str("\"jwt+mtls\"").unwrap();
        assert_eq!(parsed, AuthMode::JwtAndMtls);
    }

    #[test]
    fn auth_mode_includes_helpers() {
        assert!(AuthMode::Jwt.includes_jwt());
        assert!(!AuthMode::Jwt.includes_mtls());
        assert!(!AuthMode::Mtls.includes_jwt());
        assert!(AuthMode::Mtls.includes_mtls());
        assert!(AuthMode::JwtAndMtls.includes_jwt());
        assert!(AuthMode::JwtAndMtls.includes_mtls());
    }

    #[test]
    fn default_disabled() {
        let cfg: ExternalGrpcConfig = serde_json::from_str("{}").unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.bind_address, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(cfg.port, 10092);
        assert_eq!(cfg.max_concurrent_streams, 16);
        assert_eq!(cfg.max_connections, 1024);
        assert_eq!(cfg.burst_capacity, 10);
        assert_eq!(cfg.mtls_max_cert_lifetime_hours, 48);
    }

    #[test]
    fn jwt_algorithm_serde() {
        assert_eq!(
            serde_json::to_string(&JwtAlgorithm::Rs256).unwrap(),
            "\"RS256\""
        );
        assert_eq!(
            serde_json::to_string(&JwtAlgorithm::Es256).unwrap(),
            "\"ES256\""
        );
    }
}
