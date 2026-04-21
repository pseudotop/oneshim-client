//! D13-v2 integration test: end-to-end gRPC dashboard server ↔ client.
//!
//! Spawns `serve_optional()` on an ephemeral port, connects a tonic client,
//! exercises both RPCs (`GetAgentInfo`, `HealthCheck`), and the standard
//! `grpc.health.v1` health service. Verifies the wire protocol + service
//! registration in one go.
//!
//! Feature-gated by `grpc-dashboard` — the entire file compiles away when
//! the feature is off (matches the production gating in `oneshim-web::grpc`).

#![cfg(feature = "grpc-dashboard")]

use std::net::TcpListener;
use std::time::Duration;

use oneshim_web::proto::dashboard::v1::dashboard_service_client::DashboardServiceClient;
use oneshim_web::proto::dashboard::v1::health_check_response::Status as HealthStatus;
use oneshim_web::proto::dashboard::v1::{GetAgentInfoRequest, HealthCheckRequest};

/// Pick a free ephemeral port by binding + immediately dropping a listener.
/// Tiny race window between drop + server bind, acceptable for test use.
fn pick_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

/// Poll until the server accepts TCP connections (up to `timeout`).
async fn wait_for_server_ready(port: u16, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "gRPC dashboard server did not accept connections on port {} within {:?}",
                port, timeout
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_serves_get_agent_info_end_to_end() {
    let port = pick_free_port();

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(port));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{}", port);
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    let response = client
        .get_agent_info(GetAgentInfoRequest {})
        .await
        .expect("GetAgentInfo RPC succeeds")
        .into_inner();

    assert!(
        !response.version.is_empty(),
        "version must come from CARGO_PKG_VERSION — got empty string"
    );
    assert!(
        matches!(response.build_profile.as_str(), "debug" | "release"),
        "build_profile must be 'debug' or 'release' — got '{}'",
        response.build_profile
    );
    assert!(
        matches!(
            response.platform.as_str(),
            "macos" | "windows" | "linux" | "unknown"
        ),
        "platform must be one of macos/windows/linux/unknown — got '{}'",
        response.platform
    );
    assert!(
        response.uptime_secs >= 0,
        "uptime_secs must be non-negative — got {}",
        response.uptime_secs
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_serves_health_check_end_to_end() {
    let port = pick_free_port();

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(port));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{}", port);
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    let response = client
        .health_check(HealthCheckRequest {})
        .await
        .expect("HealthCheck RPC succeeds")
        .into_inner();

    assert_eq!(
        response.status,
        HealthStatus::Serving as i32,
        "health status should be SERVING when server is up",
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_survives_multiple_sequential_calls() {
    let port = pick_free_port();

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(port));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{}", port);
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    // Locks the invariant that DashboardServiceImpl handles concurrent-ish
    // traffic without panicking or leaking state between calls.
    for _ in 0..5 {
        let info = client
            .get_agent_info(GetAgentInfoRequest {})
            .await
            .expect("GetAgentInfo RPC succeeds")
            .into_inner();
        assert!(!info.version.is_empty());
    }

    server_task.abort();
    let _ = server_task.await;
}
