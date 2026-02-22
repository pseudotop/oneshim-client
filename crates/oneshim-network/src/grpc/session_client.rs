//! gRPC 세션 클라이언트
//!
//! 서버의 SessionService와 통신합니다.

use oneshim_core::error::CoreError;
use std::collections::HashMap;
use tonic::transport::Channel;
use tracing::{debug, error, info};

use super::{map_grpc_status_error, GrpcConfig};
use crate::proto::auth::{
    session_service_client::SessionServiceClient, CreateSessionRequest, CreateSessionResponse,
    SessionHeartbeatRequest, SessionHeartbeatResponse,
};

/// gRPC 세션 클라이언트
pub struct GrpcSessionClient {
    client: SessionServiceClient<Channel>,
    config: GrpcConfig,
}

impl GrpcSessionClient {
    /// 새 gRPC 세션 클라이언트 생성 (포트 fallback 지원)
    pub async fn connect(config: GrpcConfig) -> Result<Self, CoreError> {
        let endpoints = config.all_endpoints();
        let mut last_error = None;

        for endpoint_url in &endpoints {
            info!(endpoint = %endpoint_url, "gRPC 세션 클라이언트 연결 시도");

            match config.connect_channel(endpoint_url).await {
                Ok(channel) => {
                    let client = SessionServiceClient::new(channel);
                    info!(endpoint = %endpoint_url, "gRPC 세션 클라이언트 연결 완료");
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

    /// 세션 생성
    pub async fn create_session(
        &mut self,
        client_id: &str,
        device_info: HashMap<String, String>,
    ) -> Result<CreateSessionResponse, CoreError> {
        debug!(client_id = %client_id, "gRPC 세션 생성 요청");

        let request = tonic::Request::new(CreateSessionRequest {
            client_id: client_id.to_string(),
            device_info,
            ip_address: None,
            user_agent: None,
        });

        let response = self
            .client
            .create_session(request)
            .await
            .map_err(|status| {
                error!(error = %status, "gRPC 세션 생성 실패");
                map_grpc_status_error("grpc session creation failed", status)
            })?;

        Ok(response.into_inner())
    }

    /// 세션 종료
    pub async fn end_session(&mut self, session_id: &str) -> Result<(), CoreError> {
        debug!(session_id = %session_id, "gRPC 세션 종료 요청");

        let request = tonic::Request::new(crate::proto::auth::EndSessionRequest {
            session_id: session_id.to_string(),
            reason: None,
        });

        self.client.end_session(request).await.map_err(|status| {
            error!(error = %status, "gRPC 세션 종료 실패");
            map_grpc_status_error("grpc session termination failed", status)
        })?;

        Ok(())
    }

    /// 하트비트 전송
    pub async fn heartbeat(
        &mut self,
        session_id: &str,
        client_id: &str,
        client_state: HashMap<String, String>,
    ) -> Result<SessionHeartbeatResponse, CoreError> {
        debug!(session_id = %session_id, "gRPC 하트비트 전송");

        let _ = client_id; // client_id는 session에서는 사용하지 않음
        let request = tonic::Request::new(SessionHeartbeatRequest {
            session_id: session_id.to_string(),
            client_timestamp: None,
            client_state,
        });

        let response = self.client.heartbeat(request).await.map_err(|status| {
            error!(error = %status, "gRPC 하트비트 실패");
            map_grpc_status_error("grpc session heartbeat failed", status)
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

    #[test]
    fn test_create_session_request() {
        let request = CreateSessionRequest {
            client_id: "client-123".to_string(),
            device_info: HashMap::new(),
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("ONESHIM/0.1.0".to_string()),
        };
        assert_eq!(request.client_id, "client-123");
    }

    #[test]
    fn test_heartbeat_request() {
        let mut state = HashMap::new();
        state.insert("status".to_string(), "active".to_string());

        let request = SessionHeartbeatRequest {
            session_id: "session-123".to_string(),
            client_timestamp: None,
            client_state: state,
        };
        assert_eq!(request.session_id, "session-123");
    }
}
