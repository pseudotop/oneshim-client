//! D13: gRPC dashboard server. Exposes `DashboardService` on a dedicated port
//! alongside the Axum REST server for external CLI/integration tools.
//!
//! Feature-gated via `grpc-dashboard` — when disabled, this module and its
//! dependencies (tonic, tonic-health, etc.) compile away entirely.

#![cfg(feature = "grpc-dashboard")]

use std::net::SocketAddr;
use std::time::Instant;

use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use crate::proto::dashboard::v1::dashboard_service_server::{
    DashboardService, DashboardServiceServer,
};
use crate::proto::dashboard::v1::health_check_response::Status as HealthStatus;
use crate::proto::dashboard::v1::{
    AgentInfoResponse, GetAgentInfoRequest, HealthCheckRequest, HealthCheckResponse,
};

/// Default gRPC dashboard port when the config field is 0 / unset.
pub const DEFAULT_GRPC_DASHBOARD_PORT: u16 = 10091;

pub struct DashboardServiceImpl {
    started_at: Instant,
}

impl DashboardServiceImpl {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl Default for DashboardServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl DashboardService for DashboardServiceImpl {
    async fn get_agent_info(
        &self,
        _req: Request<GetAgentInfoRequest>,
    ) -> Result<Response<AgentInfoResponse>, Status> {
        let platform = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "unknown"
        };

        let build_profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };

        let response = AgentInfoResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_profile: build_profile.to_string(),
            uptime_secs: self.started_at.elapsed().as_secs() as i64,
            platform: platform.to_string(),
        };

        Ok(Response::new(response))
    }

    async fn health_check(
        &self,
        _req: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        // v1: always SERVING once the server is up. Future iters can probe
        // storage / scheduler state for a richer readiness signal.
        let response = HealthCheckResponse {
            status: HealthStatus::Serving as i32,
            message: String::new(),
        };
        Ok(Response::new(response))
    }
}

/// Spawn the gRPC dashboard server on the given port. The server runs until
/// shutdown (error or task cancellation). If `port == 0` the default
/// `DEFAULT_GRPC_DASHBOARD_PORT` is used.
pub async fn serve(port: u16) -> Result<(), tonic::transport::Error> {
    let port = if port == 0 {
        DEFAULT_GRPC_DASHBOARD_PORT
    } else {
        port
    };
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();

    info!(%addr, "starting gRPC dashboard server (D13)");

    let service = DashboardServiceImpl::new();

    // Register the standard grpc.health.v1 health service for external
    // liveness checks (`grpc_health_probe -addr=localhost:10091`).
    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<DashboardServiceServer<DashboardServiceImpl>>()
        .await;

    Server::builder()
        .add_service(DashboardServiceServer::new(service))
        .add_service(health_service)
        .serve(addr)
        .await
}

/// Non-fatal wrapper: logs failures instead of panicking. Use when the gRPC
/// server is optional (user can still use REST).
pub async fn serve_optional(port: u16) {
    if let Err(e) = serve(port).await {
        warn!(error = %e, "gRPC dashboard server terminated with error");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_port_is_10091() {
        assert_eq!(DEFAULT_GRPC_DASHBOARD_PORT, 10091);
    }

    #[tokio::test]
    async fn get_agent_info_returns_populated_response() {
        let service = DashboardServiceImpl::new();
        let response = service
            .get_agent_info(Request::new(GetAgentInfoRequest {}))
            .await
            .expect("get_agent_info should not fail");
        let info = response.into_inner();
        assert!(
            !info.version.is_empty(),
            "version should be CARGO_PKG_VERSION"
        );
        assert!(
            info.build_profile == "debug" || info.build_profile == "release",
            "build_profile must be debug or release, got: {}",
            info.build_profile
        );
        assert!(
            ["macos", "windows", "linux", "unknown"].contains(&info.platform.as_str()),
            "unexpected platform: {}",
            info.platform
        );
        assert!(info.uptime_secs >= 0, "uptime must be non-negative");
    }

    #[tokio::test]
    async fn health_check_returns_serving() {
        let service = DashboardServiceImpl::new();
        let response = service
            .health_check(Request::new(HealthCheckRequest {}))
            .await
            .expect("health_check should not fail");
        let health = response.into_inner();
        assert_eq!(health.status, HealthStatus::Serving as i32);
    }
}
