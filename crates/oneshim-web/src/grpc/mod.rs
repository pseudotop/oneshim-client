//! D13: gRPC dashboard server. Exposes `DashboardService` on a dedicated port
//! alongside the Axum REST server for external CLI/integration tools.
//!
//! Feature-gated via `grpc-dashboard` — when disabled, this module and its
//! dependencies (tonic, tonic-health, etc.) compile away entirely.

#![cfg(feature = "grpc-dashboard")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use crate::proto::dashboard::v1::dashboard_service_server::{
    DashboardService, DashboardServiceServer,
};
use crate::proto::dashboard::v1::health_check_response::Status as HealthStatus;
use crate::proto::dashboard::v1::{
    AgentInfoResponse, GetAgentInfoRequest, GetSessionStatsRequest, HealthCheckRequest,
    HealthCheckResponse, SessionStatsResponse,
};
use crate::storage_port::WebStorage;

/// Default gRPC dashboard port when the config field is 0 / unset.
pub const DEFAULT_GRPC_DASHBOARD_PORT: u16 = 10091;

/// Default sample size when `GetSessionStatsRequest::limit == 0`.
const DEFAULT_SESSION_STATS_LIMIT: usize = 1000;

pub struct DashboardServiceImpl {
    started_at: Instant,
    storage: Arc<dyn WebStorage>,
}

impl DashboardServiceImpl {
    pub fn new(storage: Arc<dyn WebStorage>) -> Self {
        Self {
            started_at: Instant::now(),
            storage,
        }
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

    async fn get_session_stats(
        &self,
        req: Request<GetSessionStatsRequest>,
    ) -> Result<Response<SessionStatsResponse>, Status> {
        let limit = match req.into_inner().limit {
            0 => DEFAULT_SESSION_STATS_LIMIT,
            n => n as usize,
        };

        // `list_session_stats` is synchronous + hits SQLite; run under
        // spawn_blocking to avoid starving tokio's async runtime.
        let storage = self.storage.clone();
        let rows = tokio::task::spawn_blocking(move || storage.list_session_stats(limit))
            .await
            .map_err(|e| Status::internal(format!("spawn_blocking join: {e}")))?
            .map_err(|e| Status::internal(format!("list_session_stats: {e}")))?;

        // Aggregate in-memory. For very large result sets this is O(n) and
        // cheap — small structs, no string allocation.
        let total_sessions = rows.len() as u32;
        let mut ended_sessions: u32 = 0;
        let mut total_events: u64 = 0;
        let mut total_frames: u64 = 0;
        let mut total_idle_secs: u64 = 0;
        let mut total_duration_secs: i64 = 0;
        for row in &rows {
            total_events += row.total_events;
            total_frames += row.total_frames;
            total_idle_secs += row.total_idle_secs;
            if let Some(ended_at) = row.ended_at {
                ended_sessions += 1;
                let duration = (ended_at - row.started_at).num_seconds();
                if duration > 0 {
                    total_duration_secs += duration;
                }
            }
        }
        let avg_duration_secs = if ended_sessions > 0 {
            total_duration_secs as f64 / f64::from(ended_sessions)
        } else {
            0.0
        };

        Ok(Response::new(SessionStatsResponse {
            total_sessions,
            ended_sessions,
            avg_duration_secs,
            total_events,
            total_frames,
            total_idle_secs,
        }))
    }
}

/// Spawn the gRPC dashboard server on the given port. The server runs until
/// shutdown (error or task cancellation). If `port == 0` the default
/// `DEFAULT_GRPC_DASHBOARD_PORT` is used.
///
/// D13-v2a: takes an `Arc<dyn WebStorage>` so per-domain RPCs (starting
/// with `GetSessionStats`) can read aggregated data.
pub async fn serve(port: u16, storage: Arc<dyn WebStorage>) -> Result<(), tonic::transport::Error> {
    let port = if port == 0 {
        DEFAULT_GRPC_DASHBOARD_PORT
    } else {
        port
    };
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();

    info!(%addr, "starting gRPC dashboard server (D13)");

    let service = DashboardServiceImpl::new(storage);

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
pub async fn serve_optional(port: u16, storage: Arc<dyn WebStorage>) {
    if let Err(e) = serve(port, storage).await {
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

    // RPC-surface behavior is covered in
    // `crates/oneshim-web/tests/grpc_dashboard_integration.rs` which seeds a
    // real `SqliteStorage::open_in_memory` — mocking the 10+ WebStorage
    // sub-traits for a unit test adds more surface than it saves. The
    // aggregation math in `get_session_stats` is exercised end-to-end there.
}
