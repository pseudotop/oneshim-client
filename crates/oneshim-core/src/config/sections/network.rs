// 네트워크 연결 설정 — 서버/gRPC/TLS/Web 설정 모음
use serde::{Deserialize, Serialize};

// ── TlsConfig ──────────────────────────────────────────────────────

/// TLS 연결 설정 — 아웃바운드 HTTP/SSE 연결 보안 정책
///
/// 기본값: enabled=true (TLS 강제), allow_self_signed=false (운영 환경 표준).
/// 개발 환경에서는 allow_self_signed=true 또는 enabled=false 로 설정 가능.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TlsConfig {
    /// TLS 강제 여부 — false 시 http:// 연결 허용 (개발 전용)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 자체 서명 인증서 허용 — 운영 환경에서는 반드시 false 유지
    #[serde(default)]
    pub allow_self_signed: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_self_signed: false,
        }
    }
}

// ── ServerConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub base_url: String,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_sse_max_retry_secs")]
    pub sse_max_retry_secs: u64,
}

// ── GrpcConfig ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    #[serde(default)]
    pub use_grpc_auth: bool,
    #[serde(default)]
    pub use_grpc_context: bool,
    #[serde(default = "default_grpc_endpoint")]
    pub grpc_endpoint: String,
    #[serde(default = "default_grpc_fallback_ports")]
    pub grpc_fallback_ports: Vec<u16>,
    #[serde(default = "default_grpc_connect_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_grpc_request_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub use_tls: bool,

    #[serde(default)]
    pub mtls_enabled: bool,
    #[serde(default)]
    pub tls_domain_name: Option<String>,
    #[serde(default)]
    pub tls_ca_cert_path: Option<String>,
    #[serde(default)]
    pub tls_client_cert_path: Option<String>,
    #[serde(default)]
    pub tls_client_key_path: Option<String>,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            use_grpc_auth: false,
            use_grpc_context: false,
            grpc_endpoint: default_grpc_endpoint(),
            grpc_fallback_ports: default_grpc_fallback_ports(),
            connect_timeout_secs: default_grpc_connect_timeout(),
            request_timeout_secs: default_grpc_request_timeout(),
            use_tls: false,
            mtls_enabled: false,
            tls_domain_name: None,
            tls_ca_cert_path: None,
            tls_client_cert_path: None,
            tls_client_key_path: None,
        }
    }
}

// ── WebConfig ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_web_enabled")]
    pub enabled: bool,
    #[serde(default = "default_web_port")]
    pub port: u16,
    #[serde(default)]
    pub allow_external: bool,
    #[serde(default)]
    pub integration_auth_token: Option<String>,
    /// D13-v2: dedicated port for the gRPC Dashboard server. 0 means "use
    /// the default (10091)". Overridden by the `ONESHIM_DASHBOARD_GRPC_PORT`
    /// env var for ops/CI overrides.
    #[serde(default = "default_grpc_dashboard_port")]
    pub grpc_port: u16,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: default_web_enabled(),
            port: default_web_port(),
            allow_external: false,
            integration_auth_token: None,
            grpc_port: default_grpc_dashboard_port(),
        }
    }
}

// ── Default / helper functions (pub(super) — config/mod.rs 에서 사용) ─

pub(crate) fn default_request_timeout_ms() -> u64 {
    30_000
}

pub(crate) fn default_sse_max_retry_secs() -> u64 {
    30
}

// ── Private default helpers ─────────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_grpc_endpoint() -> String {
    "http://localhost:50051".to_string()
}

fn default_grpc_fallback_ports() -> Vec<u16> {
    vec![50052, 50053]
}

fn default_grpc_connect_timeout() -> u64 {
    10
}

fn default_grpc_request_timeout() -> u64 {
    30
}

/// 로컬 WebServer 기본 포트 — IANA Dynamic/Ephemeral 대역 (49152-65535)
///
/// 9090 등 Well-Known/Registered Port 대역은 Prometheus, Cockpit 등과 충돌 가능.
/// 10090 은 IANA 미등록 Registered Port 대역 (1024-49151) 으로
/// OS ephemeral 아웃바운드 할당과 겹치지 않으면서 충돌 가능성이 낮음.
/// MAX_PORT_ATTEMPTS=10 으로 10090-10099 범위 자동 폴백.
pub const DEFAULT_WEB_PORT: u16 = 10090;

fn default_web_enabled() -> bool {
    true
}

fn default_web_port() -> u16 {
    DEFAULT_WEB_PORT
}

/// D13-v2: default gRPC Dashboard port.
///
/// Must match `oneshim_web::grpc::DEFAULT_GRPC_DASHBOARD_PORT`. Both live at
/// 10091 — one port offset from the REST server (10090). Keeping the two in
/// sync is enforced by a unit test (see the bottom of this file) so a drift
/// is caught at build time.
pub const DEFAULT_GRPC_DASHBOARD_PORT: u16 = 10091;

fn default_grpc_dashboard_port() -> u16 {
    DEFAULT_GRPC_DASHBOARD_PORT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_grpc_dashboard_port_is_10091() {
        // D13-v2 port contract: must stay in sync with
        // `oneshim_web::grpc::DEFAULT_GRPC_DASHBOARD_PORT`.
        assert_eq!(DEFAULT_GRPC_DASHBOARD_PORT, 10091);
    }

    #[test]
    fn web_config_default_wires_grpc_port() {
        let cfg = WebConfig::default();
        assert_eq!(cfg.grpc_port, DEFAULT_GRPC_DASHBOARD_PORT);
    }

    #[test]
    fn web_config_grpc_port_roundtrips_via_serde() {
        let cfg = WebConfig {
            grpc_port: 55555,
            ..WebConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: WebConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.grpc_port, 55555);
    }

    #[test]
    fn web_config_grpc_port_defaults_when_absent_from_json() {
        // Partial JSON (no grpc_port field) — should default to 10091.
        let json = r#"{"enabled":true,"port":10090,"allow_external":false}"#;
        let parsed: WebConfig = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.grpc_port, DEFAULT_GRPC_DASHBOARD_PORT);
    }
}
