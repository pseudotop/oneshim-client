//!

use oneshim_core::error::CoreError;
use std::collections::HashMap;
use tonic::transport::Channel;
use tracing::{debug, error, info};

use super::{map_grpc_status_error, GrpcConfig};
use crate::proto::auth::{
    session_service_client::SessionServiceClient, CreateSessionRequest, CreateSessionResponse,
    SessionHeartbeatRequest, SessionHeartbeatResponse,
};

pub struct GrpcSessionClient {
    client: SessionServiceClient<Channel>,
    config: GrpcConfig,
}

impl GrpcSessionClient {
    pub async fn connect(config: GrpcConfig) -> Result<Self, CoreError> {
        let endpoints = config.all_endpoints();
        let mut last_error = None;

        for endpoint_url in &endpoints {
            info!(endpoint = %endpoint_url, "gRPC session client connection attempt");

            match config.connect_channel(endpoint_url).await {
                Ok(channel) => {
                    let client = SessionServiceClient::new(channel);
                    info!(endpoint = %endpoint_url, "gRPC session client connection completed");
                    return Ok(Self { client, config });
                }
                Err(e) => {
                    debug!(endpoint = %endpoint_url, error = %e, "gRPC connection failure, next port attempt");
                    last_error = Some(e);
                }
            }
        }

        error!(endpoints = ?endpoints, "all gRPC endpoint connection failure");
        Err(last_error.unwrap_or_else(|| CoreError::Network("gRPC endpoint none".to_string())))
    }

    pub async fn create_session(
        &mut self,
        client_id: &str,
        device_info: HashMap<String, String>,
    ) -> Result<CreateSessionResponse, CoreError> {
        debug!(client_id = %client_id, "gRPC session create request");

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
                error!(error = %status, "gRPC session create failure");
                map_grpc_status_error("grpc session creation failed", status)
            })?;

        Ok(response.into_inner())
    }

    pub async fn end_session(&mut self, session_id: &str) -> Result<(), CoreError> {
        debug!(session_id = %session_id, "gRPC session ended request");

        let request = tonic::Request::new(crate::proto::auth::EndSessionRequest {
            session_id: session_id.to_string(),
            reason: None,
        });

        self.client.end_session(request).await.map_err(|status| {
            error!(error = %status, "gRPC session ended failure");
            map_grpc_status_error("grpc session termination failed", status)
        })?;

        Ok(())
    }

    pub async fn heartbeat(
        &mut self,
        session_id: &str,
        client_id: &str,
        client_state: HashMap<String, String>,
    ) -> Result<SessionHeartbeatResponse, CoreError> {
        debug!(session_id = %session_id, "gRPC heartbeat sent");

        let _ = client_id; // session API does not currently use client_id
        let request = tonic::Request::new(SessionHeartbeatRequest {
            session_id: session_id.to_string(),
            client_timestamp: None,
            client_state,
        });

        let response = self.client.heartbeat(request).await.map_err(|status| {
            error!(error = %status, "gRPC heartbeat failure");
            map_grpc_status_error("grpc session heartbeat failed", status)
        })?;

        Ok(response.into_inner())
    }

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
