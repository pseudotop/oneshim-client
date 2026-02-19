//! gRPC 클라이언트 설정
//!
//! gRPC 연결 설정 및 Feature Flag 관리
//! `oneshim-core`의 `GrpcConfig`를 확장하여 REST fallback 엔드포인트 추가

use oneshim_core::config::GrpcConfig as CoreGrpcConfig;
use serde::{Deserialize, Serialize};

/// gRPC 클라이언트 설정
///
/// `oneshim-core::config::GrpcConfig`를 기반으로 REST fallback 엔드포인트 추가
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    /// gRPC 인증 사용 여부 (Phase 34-2 완료 후 true)
    #[serde(default)]
    pub use_grpc_auth: bool,

    /// gRPC 컨텍스트 전송 사용 여부 (Phase 34-3 완료 후 true)
    #[serde(default)]
    pub use_grpc_context: bool,

    /// gRPC 엔드포인트 (기본 포트)
    #[serde(default = "default_grpc_endpoint")]
    pub grpc_endpoint: String,

    /// gRPC fallback 포트 목록 (기본 포트 연결 실패 시 순차 시도)
    #[serde(default = "default_grpc_fallback_ports")]
    pub grpc_fallback_ports: Vec<u16>,

    /// REST 엔드포인트 (fallback)
    #[serde(default = "default_rest_endpoint")]
    pub rest_endpoint: String,

    /// 연결 타임아웃 (초)
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,

    /// 요청 타임아웃 (초)
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,

    /// TLS 사용 여부
    #[serde(default = "default_use_tls")]
    pub use_tls: bool,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            use_grpc_auth: false,
            use_grpc_context: false,
            grpc_endpoint: default_grpc_endpoint(),
            grpc_fallback_ports: default_grpc_fallback_ports(),
            rest_endpoint: default_rest_endpoint(),
            connect_timeout_secs: default_connect_timeout(),
            request_timeout_secs: default_request_timeout(),
            use_tls: default_use_tls(),
        }
    }
}

impl From<CoreGrpcConfig> for GrpcConfig {
    fn from(core: CoreGrpcConfig) -> Self {
        Self {
            use_grpc_auth: core.use_grpc_auth,
            use_grpc_context: core.use_grpc_context,
            grpc_endpoint: core.grpc_endpoint,
            grpc_fallback_ports: core.grpc_fallback_ports,
            rest_endpoint: default_rest_endpoint(),
            connect_timeout_secs: core.connect_timeout_secs,
            request_timeout_secs: core.request_timeout_secs,
            use_tls: core.use_tls,
        }
    }
}

impl GrpcConfig {
    /// `oneshim-core`의 설정과 REST endpoint를 조합하여 생성
    pub fn from_core_with_rest(core: &CoreGrpcConfig, rest_endpoint: &str) -> Self {
        Self {
            use_grpc_auth: core.use_grpc_auth,
            use_grpc_context: core.use_grpc_context,
            grpc_endpoint: core.grpc_endpoint.clone(),
            grpc_fallback_ports: core.grpc_fallback_ports.clone(),
            rest_endpoint: rest_endpoint.to_string(),
            connect_timeout_secs: core.connect_timeout_secs,
            request_timeout_secs: core.request_timeout_secs,
            use_tls: core.use_tls,
        }
    }

    /// 시도할 모든 gRPC 엔드포인트 목록 반환 (기본 + fallback)
    pub fn all_endpoints(&self) -> Vec<String> {
        let mut endpoints = vec![self.grpc_endpoint.clone()];

        // 기본 엔드포인트에서 호스트 추출
        if let Some(base) = self.grpc_endpoint.rsplit_once(':') {
            let host = base.0;
            for port in &self.grpc_fallback_ports {
                endpoints.push(format!("{}:{}", host, port));
            }
        }

        endpoints
    }
}

fn default_grpc_endpoint() -> String {
    "http://localhost:50051".to_string()
}

/// gRPC fallback 포트 목록 (서버가 다른 포트에서 실행될 수 있음)
/// 50052: Python betterproto/grpclib 서버 포트
/// 50053: 추가 예비 포트
fn default_grpc_fallback_ports() -> Vec<u16> {
    vec![50052, 50053]
}

fn default_rest_endpoint() -> String {
    "http://localhost:8000".to_string()
}

fn default_connect_timeout() -> u64 {
    10
}

fn default_request_timeout() -> u64 {
    30
}

fn default_use_tls() -> bool {
    false
}

impl GrpcConfig {
    /// REST fallback 필요 여부 확인
    pub fn needs_rest_fallback(&self) -> bool {
        !self.use_grpc_auth || !self.use_grpc_context
    }

    /// 인증에 gRPC 사용 여부
    pub fn should_use_grpc_for_auth(&self) -> bool {
        self.use_grpc_auth
    }

    /// 컨텍스트 전송에 gRPC 사용 여부
    pub fn should_use_grpc_for_context(&self) -> bool {
        self.use_grpc_context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GrpcConfig::default();
        assert!(!config.use_grpc_auth);
        assert!(!config.use_grpc_context);
        assert_eq!(config.grpc_endpoint, "http://localhost:50051");
        assert!(config.needs_rest_fallback());
    }

    #[test]
    fn test_grpc_enabled() {
        let config = GrpcConfig {
            use_grpc_auth: true,
            use_grpc_context: true,
            ..Default::default()
        };
        assert!(!config.needs_rest_fallback());
        assert!(config.should_use_grpc_for_auth());
        assert!(config.should_use_grpc_for_context());
    }

    #[test]
    fn test_fallback_ports() {
        let config = GrpcConfig::default();
        // 기본 fallback 포트 확인
        assert_eq!(config.grpc_fallback_ports, vec![50052, 50053]);
    }

    #[test]
    fn test_all_endpoints() {
        let config = GrpcConfig::default();
        let endpoints = config.all_endpoints();
        // 기본 엔드포인트 + fallback 포트들
        assert_eq!(endpoints.len(), 3);
        assert_eq!(endpoints[0], "http://localhost:50051");
        assert_eq!(endpoints[1], "http://localhost:50052");
        assert_eq!(endpoints[2], "http://localhost:50053");
    }

    #[test]
    fn test_all_endpoints_custom() {
        let config = GrpcConfig {
            grpc_endpoint: "http://example.com:9000".to_string(),
            grpc_fallback_ports: vec![9001, 9002],
            ..Default::default()
        };
        let endpoints = config.all_endpoints();
        assert_eq!(endpoints.len(), 3);
        assert_eq!(endpoints[0], "http://example.com:9000");
        assert_eq!(endpoints[1], "http://example.com:9001");
        assert_eq!(endpoints[2], "http://example.com:9002");
    }
}
