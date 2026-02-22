//! gRPC 인증 클라이언트
//!
//! 서버의 AuthenticationService와 통신합니다.

use oneshim_core::error::CoreError;
use std::collections::HashMap;
use tonic::transport::Channel;
use tracing::{debug, error, info};

use super::{map_grpc_status_error, GrpcConfig};
use crate::proto::auth::{
    authentication_service_client::AuthenticationServiceClient, LoginRequest, LoginResponse,
    RefreshTokenRequest, TokenRefreshResponse,
};

/// gRPC 인증 클라이언트
pub struct GrpcAuthClient {
    client: AuthenticationServiceClient<Channel>,
    config: GrpcConfig,
}

impl GrpcAuthClient {
    /// 새 gRPC 인증 클라이언트 생성 (포트 fallback 지원)
    pub async fn connect(config: GrpcConfig) -> Result<Self, CoreError> {
        let endpoints = config.all_endpoints();
        let mut last_error = None;

        for endpoint_url in &endpoints {
            info!(endpoint = %endpoint_url, "gRPC 인증 클라이언트 연결 시도");

            match config.connect_channel(endpoint_url).await {
                Ok(channel) => {
                    let client = AuthenticationServiceClient::new(channel);
                    info!(endpoint = %endpoint_url, "gRPC 인증 클라이언트 연결 완료");
                    return Ok(Self { client, config });
                }
                Err(e) => {
                    debug!(endpoint = %endpoint_url, error = %e, "gRPC 연결 실패, 다음 포트 시도");
                    last_error = Some(e);
                }
            }
        }

        // 모든 포트 시도 실패
        error!(endpoints = ?endpoints, "모든 gRPC 엔드포인트 연결 실패");
        Err(last_error.unwrap_or_else(|| CoreError::Network("gRPC 엔드포인트 없음".to_string())))
    }

    /// 로그인
    pub async fn login(
        &mut self,
        identifier: &str,
        password: &str,
        organization_id: &str,
        device_info: HashMap<String, String>,
    ) -> Result<LoginResponse, CoreError> {
        debug!(identifier = %identifier, "gRPC 로그인 요청");

        let request = tonic::Request::new(LoginRequest {
            identifier: identifier.to_string(),
            password: password.to_string(),
            organization_id: organization_id.to_string(),
            device_info,
            remember_device: false,
            mfa_token: None,
        });

        let response = self.client.login(request).await.map_err(|status| {
            error!(error = %status, "gRPC 로그인 실패");
            map_grpc_status_error("grpc login failed", status)
        })?;

        Ok(response.into_inner())
    }

    /// 토큰 갱신
    pub async fn refresh_token(
        &mut self,
        refresh_token: &str,
        user_id: &str,
        session_id: Option<&str>,
    ) -> Result<TokenRefreshResponse, CoreError> {
        debug!(user_id = %user_id, "gRPC 토큰 갱신 요청");

        let request = tonic::Request::new(RefreshTokenRequest {
            refresh_token: refresh_token.to_string(),
            user_id: user_id.to_string(),
            session_id: session_id.map(String::from),
        });

        let response = self.client.refresh_token(request).await.map_err(|status| {
            error!(error = %status, "gRPC 토큰 갱신 실패");
            map_grpc_status_error("grpc token refresh failed", status)
        })?;

        Ok(response.into_inner())
    }

    /// 설정 조회
    pub fn config(&self) -> &GrpcConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grpc_auth_client_config() {
        let config = GrpcConfig::default();
        assert!(!config.use_grpc_auth);
    }

    #[test]
    fn test_login_request_creation() {
        let request = LoginRequest {
            identifier: "test@example.com".to_string(),
            password: "test-password-placeholder".to_string(),
            organization_id: "org-1".to_string(),
            device_info: HashMap::new(),
            remember_device: false,
            mfa_token: None,
        };
        assert_eq!(request.identifier, "test@example.com");
    }
}
