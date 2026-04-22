//! D13: gRPC dashboard server. Exposes `DashboardService` on a dedicated port
//! alongside the Axum REST server for external CLI/integration tools.
//!
//! Feature-gated via `grpc-dashboard` — when disabled, this module and its
//! dependencies (tonic, tonic-health, etc.) compile away entirely.
//!
//! The `#[cfg(feature = "grpc-dashboard")]` gate lives on `pub mod grpc;` in
//! `lib.rs`. A matching inner-attribute here would be redundant (and trips
//! clippy's `duplicated_attributes` lint).

mod auth_gate;
mod hint_emitter;
mod load_policy;
mod spawn_config;
mod stream_counter;
mod subscribe_metrics;
pub use auth_gate::{honor_opt_out, validate_authority};
pub use hint_emitter::{HintEmitter, HEARTBEAT};
pub use load_policy::{LoadLevel, LoadPolicy, INTERVAL_CEILING, INTERVAL_FLOOR, WARMUP};
pub use spawn_config::GrpcSpawnConfig;
pub use stream_counter::StreamCounterGuard;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Instant;

use oneshim_api_contracts::stream::RealtimeEvent;
use oneshim_core::ports::monitor::SystemMonitor;
use tokio::sync::broadcast;
use tokio_stream::Stream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use crate::proto::dashboard::v1::dashboard_service_server::{
    DashboardService, DashboardServiceServer,
};
use crate::proto::dashboard::v1::health_check_response::Status as HealthStatus;
use crate::proto::dashboard::v1::{
    recent_frames_response, AgentInfoResponse, FocusStatsResponse, GetAgentInfoRequest,
    GetFocusStatsRequest, GetProductivityMetricsRequest, GetRecentFramesRequest,
    GetSessionStatsRequest, HealthCheckRequest, HealthCheckResponse, MetricBucket,
    ProductivityMetricsResponse, RecentFramesResponse, SessionStatsResponse,
    SubscribeEventsRequest, SubscribeEventsResponse, SubscribeMetricsRequest,
};
use crate::storage_port::WebStorage;

/// Default gRPC dashboard port when the config field is 0 / unset.
pub const DEFAULT_GRPC_DASHBOARD_PORT: u16 = 10091;

/// Default sample size when `GetSessionStatsRequest::limit == 0`.
const DEFAULT_SESSION_STATS_LIMIT: usize = 1000;

/// Default + hard cap for GetRecentFrames.
const DEFAULT_RECENT_FRAMES_LIMIT: u32 = 50;
const MAX_RECENT_FRAMES_LIMIT: u32 = 500;
const DEFAULT_RECENT_FRAMES_SINCE_HOURS: u32 = 1;
const MAX_RECENT_FRAMES_SINCE_HOURS: u32 = 168; // 7 days

/// Default + hard cap for GetProductivityMetrics.
const DEFAULT_METRICS_SINCE_HOURS: u32 = 24;
const MAX_METRICS_SINCE_HOURS: u32 = 168;

/// Default + hard cap for GetFocusStats.
const DEFAULT_FOCUS_DAYS: u32 = 7;
const MAX_FOCUS_DAYS: u32 = 90;

/// Convert a `chrono::DateTime<Utc>` to the generated
/// `prost_types::Timestamp` used on the wire for v2a + v2b fields.
/// `pub(super)` so sibling grpc sub-modules (PR-B2 subscribe_metrics,
/// PR-B3 subscribe_events) can reuse it.
pub(super) fn to_proto_ts(dt: chrono::DateTime<chrono::Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

pub struct DashboardServiceImpl {
    started_at: Instant,
    storage: Arc<dyn WebStorage>,
    // v2b additions (all plumbed for subscribe_metrics handler in B2-9):
    #[allow(dead_code)] // read in B2-9 handler
    system_monitor: Arc<dyn SystemMonitor>,
    #[allow(dead_code)] // read in B2-9 handler
    event_tx: broadcast::Sender<RealtimeEvent>,
    #[allow(dead_code)] // read in B2-9 handler
    integration_auth_token: Option<String>,
    #[allow(dead_code)] // read in B2-9 handler
    load_policy: Arc<LoadPolicy>,
    #[allow(dead_code)] // read in B2-9 handler
    streaming_enabled: bool,
    #[allow(dead_code)] // read in B2-9 handler
    active_streams: Arc<AtomicUsize>,
    #[allow(dead_code)] // read in B2-9 handler
    max_concurrent_streams: usize,
}

impl DashboardServiceImpl {
    /// Construct from a `GrpcSpawnConfig`. `started_at` is set to `Instant::now()`
    /// and `active_streams` to 0.
    pub fn from_spawn_config(cfg: &GrpcSpawnConfig) -> Self {
        Self {
            started_at: Instant::now(),
            storage: cfg.storage.clone(),
            system_monitor: cfg.system_monitor.clone(),
            event_tx: cfg.event_tx.clone(),
            integration_auth_token: cfg.integration_auth_token.clone(),
            load_policy: cfg.load_policy.clone(),
            streaming_enabled: cfg.streaming_enabled,
            active_streams: Arc::new(AtomicUsize::new(0)),
            max_concurrent_streams: cfg.max_concurrent_streams,
        }
    }

    /// Test-only: active concurrent-stream count. Gated so the release binary
    /// does not expose it (IMP-V2-D invariant).
    #[cfg(any(test, feature = "test-support"))]
    pub fn active_stream_count(&self) -> usize {
        self.active_streams
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[tonic::async_trait]
impl DashboardService for DashboardServiceImpl {
    type SubscribeMetricsStream = subscribe_metrics::SubscribeMetricsStream;
    type SubscribeEventsStream =
        Pin<Box<dyn Stream<Item = Result<SubscribeEventsResponse, Status>> + Send>>;

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

    async fn get_recent_frames(
        &self,
        req: Request<GetRecentFramesRequest>,
    ) -> Result<Response<RecentFramesResponse>, Status> {
        let req = req.into_inner();
        let limit = match req.limit {
            0 => DEFAULT_RECENT_FRAMES_LIMIT,
            n => n.min(MAX_RECENT_FRAMES_LIMIT),
        };
        let since_hours = match req.since_hours {
            0 => DEFAULT_RECENT_FRAMES_SINCE_HOURS,
            n => n.min(MAX_RECENT_FRAMES_SINCE_HOURS),
        };

        let to = chrono::Utc::now();
        let from = to - chrono::Duration::hours(i64::from(since_hours));

        let storage = self.storage.clone();
        let limit_usize = limit as usize;
        let records =
            tokio::task::spawn_blocking(move || storage.get_frames(from, to, limit_usize))
                .await
                .map_err(|e| Status::internal(format!("spawn_blocking join: {e}")))?
                .map_err(|e| Status::internal(format!("get_frames: {e}")))?;

        let frames = records
            .into_iter()
            .map(|r| recent_frames_response::FrameMetadata {
                frame_id: r.id,
                captured_at: r.timestamp,
                trigger_type: r.trigger_type,
                app_name: r.app_name,
                window_title: r.window_title,
                importance: r.importance,
                resolution_w: r.resolution_w,
                resolution_h: r.resolution_h,
            })
            .collect();

        Ok(Response::new(RecentFramesResponse { frames }))
    }

    async fn get_productivity_metrics(
        &self,
        req: Request<GetProductivityMetricsRequest>,
    ) -> Result<Response<ProductivityMetricsResponse>, Status> {
        let since_hours = match req.into_inner().since_hours {
            0 => DEFAULT_METRICS_SINCE_HOURS,
            n => n.min(MAX_METRICS_SINCE_HOURS),
        };
        let from =
            (chrono::Utc::now() - chrono::Duration::hours(i64::from(since_hours))).to_rfc3339();

        let storage = self.storage.clone();
        let records = tokio::task::spawn_blocking(move || storage.list_hourly_metrics_since(&from))
            .await
            .map_err(|e| Status::internal(format!("spawn_blocking join: {e}")))?
            .map_err(|e| Status::internal(format!("list_hourly_metrics_since: {e}")))?;

        let buckets = records
            .into_iter()
            .map(|r| {
                // Parse the RFC3339 hour key ("YYYY-MM-DDTHH:00:00Z") produced
                // by the SQLite metrics aggregation. On parse failure, fall back
                // to None so the bucket is still emitted with an absent start
                // rather than being silently dropped.
                let start = chrono::DateTime::parse_from_rfc3339(&r.hour)
                    .ok()
                    .map(|dt| to_proto_ts(dt.with_timezone(&chrono::Utc)));
                MetricBucket {
                    start,
                    cpu_avg_pct: r.cpu_avg,
                    // HourlyMetricsRecord stores bytes; convert to MB for the wire field.
                    memory_avg_mb: r.memory_avg as f64 / 1_048_576.0,
                    // Keystroke/click counters are not tracked in the hourly
                    // metrics table (v2a); they land in v2b streaming buckets.
                    active_keystrokes: 0,
                    active_mouse_clicks: 0,
                }
            })
            .collect();

        Ok(Response::new(ProductivityMetricsResponse { buckets }))
    }

    async fn get_focus_stats(
        &self,
        req: Request<GetFocusStatsRequest>,
    ) -> Result<Response<FocusStatsResponse>, Status> {
        let days = match req.into_inner().days {
            0 => DEFAULT_FOCUS_DAYS,
            n => n.min(MAX_FOCUS_DAYS),
        };

        let storage = self.storage.clone();
        let days_usize = days as usize;
        let records =
            tokio::task::spawn_blocking(move || storage.get_recent_focus_metrics(days_usize))
                .await
                .map_err(|e| Status::internal(format!("spawn_blocking join: {e}")))?
                .map_err(|e| Status::internal(format!("get_recent_focus_metrics: {e}")))?;

        let bucket_count = records.len() as u32;
        let mut total_active_secs: u64 = 0;
        let mut total_deep_work_secs: u64 = 0;
        let mut total_communication_secs: u64 = 0;
        let mut total_interruptions: u32 = 0;
        let mut focus_score_sum: f32 = 0.0;
        let mut longest_focus_secs: u64 = 0;
        for (_date, m) in &records {
            total_active_secs += m.total_active_secs;
            total_deep_work_secs += m.deep_work_secs;
            total_communication_secs += m.communication_secs;
            total_interruptions = total_interruptions.saturating_add(m.interruption_count);
            focus_score_sum += m.focus_score;
            if m.max_focus_duration_secs > longest_focus_secs {
                longest_focus_secs = m.max_focus_duration_secs;
            }
        }
        let avg_focus_score = if bucket_count > 0 {
            focus_score_sum / bucket_count as f32
        } else {
            0.0
        };

        Ok(Response::new(FocusStatsResponse {
            bucket_count,
            total_active_secs,
            total_deep_work_secs,
            total_communication_secs,
            total_interruptions,
            avg_focus_score,
            longest_focus_secs,
        }))
    }

    async fn subscribe_metrics(
        &self,
        req: Request<SubscribeMetricsRequest>,
    ) -> Result<Response<Self::SubscribeMetricsStream>, Status> {
        subscribe_metrics::subscribe_metrics(
            req,
            self.storage.clone(),
            self.system_monitor.clone(),
            self.event_tx.clone(),
            self.integration_auth_token.clone(),
            self.load_policy.clone(),
            self.streaming_enabled,
            self.active_streams.clone(),
            self.max_concurrent_streams,
        )
        .await
    }

    async fn subscribe_events(
        &self,
        _req: Request<SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        Err(Status::unimplemented("SubscribeEvents stub lands in PR-B3"))
    }
}

/// Spawn the gRPC dashboard server. The server runs until shutdown (error or
/// task cancellation). If `cfg.port == 0` the default
/// `DEFAULT_GRPC_DASHBOARD_PORT` is used.
///
/// D13-v2b: takes a `GrpcSpawnConfig` struct so v2b streaming RPCs can receive
/// SystemMonitor / event_tx / auth token / load_policy / kill switch / stream cap.
pub async fn serve(cfg: GrpcSpawnConfig) -> Result<(), tonic::transport::Error> {
    let port = if cfg.port == 0 {
        DEFAULT_GRPC_DASHBOARD_PORT
    } else {
        cfg.port
    };
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();

    info!(%addr, "starting gRPC dashboard server (D13-v2b)");

    let service = DashboardServiceImpl::from_spawn_config(&cfg);

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
pub async fn serve_optional(cfg: GrpcSpawnConfig) {
    if let Err(e) = serve(cfg).await {
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
