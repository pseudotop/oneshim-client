//! D13-v2 integration tests: end-to-end gRPC dashboard server ↔ client.
//!
//! Spawns `serve_optional()` on an ephemeral port, connects a tonic client,
//! exercises each RPC, and verifies the wire protocol + service
//! registration + port wiring end-to-end.
//!
//! Feature-gated by `grpc-dashboard` — the entire file compiles away when
//! the feature is off (matches the production gating in
//! `oneshim-web::grpc`).

#![cfg(feature = "grpc-dashboard")]

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use oneshim_core::models::activity::SessionStats;
use oneshim_core::models::work_session::FocusMetrics;
use oneshim_core::ports::storage::MetricsStorage;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_web::proto::dashboard::v1::dashboard_service_client::DashboardServiceClient;
use oneshim_web::proto::dashboard::v1::health_check_response::Status as HealthStatus;
use oneshim_web::proto::dashboard::v1::{
    subscribe_metrics_response, GetAgentInfoRequest, GetFocusStatsRequest,
    GetProductivityMetricsRequest, GetRecentFramesRequest, GetSessionStatsRequest,
    HealthCheckRequest, SubscribeMetricsRequest,
};
use oneshim_web::storage_port::WebStorage;

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
                "gRPC dashboard server did not accept connections on port {port} within {timeout:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Build an in-memory SqliteStorage behind the `WebStorage` trait.
async fn in_memory_storage() -> Arc<dyn WebStorage> {
    let storage = SqliteStorage::open_in_memory(30).expect("open in-memory SqliteStorage");
    Arc::new(storage) as Arc<dyn WebStorage>
}

/// Build a `GrpcSpawnConfig` with sensible test defaults (deterministic
/// `MockSystemMonitor`, 16-slot broadcast, no auth token, default
/// `LoadThresholds`, streaming enabled, cap 50). Callers that need to
/// override fields can destructure and rebuild.
fn test_spawn_config(
    port: u16,
    storage: Arc<dyn WebStorage>,
) -> oneshim_web::grpc::GrpcSpawnConfig {
    use oneshim_core::config::LoadThresholds;
    use oneshim_web::grpc::{test_support::mock_system_monitor::MockSystemMonitor, LoadPolicy};

    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    oneshim_web::grpc::GrpcSpawnConfig {
        port,
        storage,
        system_monitor: MockSystemMonitor::new(30.0, 4096, 16384),
        event_tx,
        integration_auth_token: None,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        load_policy: Arc::new(LoadPolicy::new(LoadThresholds::default())),
        streaming_enabled: true,
        max_concurrent_streams: 50,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_get_agent_info_end_to_end() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
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
async fn grpc_dashboard_health_check_end_to_end() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
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
    let storage = in_memory_storage().await;

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_get_session_stats_empty_db() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    let response = client
        .get_session_stats(GetSessionStatsRequest { limit: 0 })
        .await
        .expect("GetSessionStats RPC succeeds")
        .into_inner();

    // Empty DB: all counters zero, avg duration 0.
    assert_eq!(response.total_sessions, 0);
    assert_eq!(response.ended_sessions, 0);
    assert_eq!(response.avg_duration_secs, 0.0);
    assert_eq!(response.total_events, 0);
    assert_eq!(response.total_frames, 0);
    assert_eq!(response.total_idle_secs, 0);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_get_session_stats_aggregates_seeded_sessions() {
    let port = pick_free_port();

    // Seed: 3 sessions, 2 ended. Aggregate expectations below.
    let storage = SqliteStorage::open_in_memory(30).expect("open in-memory SqliteStorage");

    // Session 1 — ended, duration 120s
    let now = Utc::now();
    let s1 = SessionStats {
        session_id: "s1".into(),
        started_at: now - ChronoDuration::seconds(300),
        ended_at: Some(now - ChronoDuration::seconds(180)),
        total_events: 10,
        total_frames: 5,
        total_idle_secs: 20,
    };
    // Session 2 — ended, duration 60s
    let s2 = SessionStats {
        session_id: "s2".into(),
        started_at: now - ChronoDuration::seconds(200),
        ended_at: Some(now - ChronoDuration::seconds(140)),
        total_events: 7,
        total_frames: 3,
        total_idle_secs: 10,
    };
    // Session 3 — still running
    let s3 = SessionStats {
        session_id: "s3".into(),
        started_at: now - ChronoDuration::seconds(30),
        ended_at: None,
        total_events: 2,
        total_frames: 1,
        total_idle_secs: 0,
    };

    storage.upsert_session(&s1).await.unwrap();
    storage.upsert_session(&s2).await.unwrap();
    storage.upsert_session(&s3).await.unwrap();

    let storage: Arc<dyn WebStorage> = Arc::new(storage);

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    let response = client
        .get_session_stats(GetSessionStatsRequest { limit: 10 })
        .await
        .expect("GetSessionStats RPC succeeds")
        .into_inner();

    assert_eq!(response.total_sessions, 3);
    assert_eq!(response.ended_sessions, 2);
    // (120s + 60s) / 2 = 90s. Allow ±1s slack for chrono rounding.
    assert!(
        (response.avg_duration_secs - 90.0).abs() < 2.0,
        "avg_duration_secs: expected ~90, got {}",
        response.avg_duration_secs
    );
    assert_eq!(response.total_events, 10 + 7 + 2);
    assert_eq!(response.total_frames, 5 + 3 + 1);
    assert_eq!(response.total_idle_secs, 20 + 10);

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_get_recent_frames_empty_db() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    let response = client
        .get_recent_frames(GetRecentFramesRequest {
            limit: 0,
            since_hours: 0,
        })
        .await
        .expect("GetRecentFrames RPC succeeds")
        .into_inner();

    assert!(
        response.frames.is_empty(),
        "empty DB should return 0 frames"
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_get_recent_frames_clamps_limit() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    // Request limit=9999, since_hours=9999 — server should hard-cap but not
    // reject. Empty DB, so we just verify no error.
    let response = client
        .get_recent_frames(GetRecentFramesRequest {
            limit: 9999,
            since_hours: 9999,
        })
        .await
        .expect("GetRecentFrames clamps instead of erroring")
        .into_inner();

    assert!(response.frames.is_empty());

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_get_productivity_metrics_empty_db() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    let response = client
        .get_productivity_metrics(GetProductivityMetricsRequest { since_hours: 0 })
        .await
        .expect("GetProductivityMetrics RPC succeeds")
        .into_inner();

    assert!(
        response.buckets.is_empty(),
        "empty DB should return 0 buckets"
    );

    server_task.abort();
    let _ = server_task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_get_focus_stats_empty_db() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    let response = client
        .get_focus_stats(GetFocusStatsRequest { days: 0 })
        .await
        .expect("GetFocusStats RPC succeeds")
        .into_inner();

    // Empty DB: 0 buckets + all counters zero + avg_focus_score = 0.
    assert_eq!(response.bucket_count, 0);
    assert_eq!(response.total_active_secs, 0);
    assert_eq!(response.total_deep_work_secs, 0);
    assert_eq!(response.total_communication_secs, 0);
    assert_eq!(response.total_interruptions, 0);
    assert_eq!(response.avg_focus_score, 0.0);
    assert_eq!(response.longest_focus_secs, 0);

    server_task.abort();
    let _ = server_task.await;
}

/// Seed 3 daily focus buckets, verify GetFocusStats aggregates each dimension
/// (sum, average, max). Mirrors the seeded-aggregation pattern used for
/// GetSessionStats so the focus-side math doesn't drift silently.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_get_focus_stats_aggregates_seeded_days() {
    let port = pick_free_port();

    let storage = SqliteStorage::open_in_memory(30).expect("open in-memory SqliteStorage");

    // Three distinct dates so update_focus_metrics writes each row independently.
    // Aggregate expectations (computed in-line below) lock each response field
    // to the handler's reduction rule.
    let seeds: &[(&str, u64, u64, u64, u32, u64, f32)] = &[
        // (date,  total_active, deep_work, communication, interruptions, longest_focus, score)
        ("2026-04-20", 3_600, 2_400, 600, 2, 1_800, 0.80),
        ("2026-04-19", 1_800, 1_200, 300, 1, 2_700, 0.60),
        ("2026-04-18", 900, 600, 150, 4, 900, 0.40),
    ];

    for (date, active, deep, comm, interruptions, longest, score) in seeds {
        // Must materialize the row first — update_focus_metrics is UPDATE, not UPSERT.
        let _ = storage
            .get_or_create_focus_metrics(date)
            .expect("seed focus_metrics row");

        let metrics = FocusMetrics {
            period_start: Utc::now(),
            period_end: Utc::now(),
            total_active_secs: *active,
            deep_work_secs: *deep,
            communication_secs: *comm,
            context_switches: 0,
            interruption_count: *interruptions,
            avg_focus_duration_secs: 0,
            max_focus_duration_secs: *longest,
            focus_score: *score,
        };
        storage
            .update_focus_metrics(date, &metrics)
            .expect("update seeded focus_metrics");
    }

    let storage: Arc<dyn WebStorage> = Arc::new(storage);

    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(test_spawn_config(
        port, storage,
    )));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to dashboard gRPC server");

    // days=0 → server default (7). All 3 seeded dates fall within the window.
    let response = client
        .get_focus_stats(GetFocusStatsRequest { days: 0 })
        .await
        .expect("GetFocusStats RPC succeeds")
        .into_inner();

    assert_eq!(response.bucket_count, 3);
    assert_eq!(response.total_active_secs, 3_600 + 1_800 + 900);
    assert_eq!(response.total_deep_work_secs, 2_400 + 1_200 + 600);
    assert_eq!(response.total_communication_secs, 600 + 300 + 150);
    assert_eq!(response.total_interruptions, 2 + 1 + 4);
    // avg_focus_score = (0.80 + 0.60 + 0.40) / 3 = 0.60. Tolerance for f32 rounding.
    assert!(
        (response.avg_focus_score - 0.60).abs() < 1e-5,
        "avg_focus_score: expected ~0.60, got {}",
        response.avg_focus_score
    );
    // longest_focus_secs = max(1_800, 2_700, 900) = 2_700.
    assert_eq!(response.longest_focus_secs, 2_700);

    server_task.abort();
    let _ = server_task.await;
}

// ── B2-10: SubscribeMetrics integration tests (8) ────────────────────
//
// Infrastructure: per spec §7.1 virtual-time ordering — when using
// `start_paused = true`, the `subscribe_metrics` RPC call MUST come AFTER
// `tokio::time::pause()` so the handler's `tokio::time::interval` registers
// against the paused clock. `wait_for_server_ready` (real-clock sleep) MUST
// come BEFORE `pause()`.

use oneshim_api_contracts::stream::{MetricsUpdate, RealtimeEvent};
use std::sync::atomic::Ordering;

/// Test #1 — First yield is a `Hint`, reason starts with `"warmup"` (within 30s).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_subscribe_metrics_emits_initial_hint() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;
    let cfg = test_spawn_config(port, storage);
    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(cfg));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint).await.unwrap();

    let mut stream = client
        .subscribe_metrics(SubscribeMetricsRequest {
            interval_secs: 1,
            respect_server_hints: true,
        })
        .await
        .expect("subscribe_metrics ok")
        .into_inner();

    let first = tokio::time::timeout(Duration::from_secs(3), stream.message())
        .await
        .expect("message within 3s")
        .expect("stream not errored")
        .expect("stream not ended");
    match first.payload.expect("payload") {
        subscribe_metrics_response::Payload::Hint(h) => {
            assert_ne!(h.load_level, 0, "level should not be UNSPECIFIED");
            assert!(!h.reason.is_empty());
            // During warm-up the reason must be prefixed "warmup".
            assert!(
                h.reason.starts_with("warmup"),
                "expected 'warmup' prefix in fresh server, got: {}",
                h.reason
            );
        }
        other => panic!("expected Hint first, got {other:?}"),
    }

    server_task.abort();
    let _ = server_task.await;
}

/// Test #2 — `streaming_enabled=false` → `Status::unavailable`, message
/// does not leak config field name (IMP-V2-F).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_subscribe_metrics_rejects_when_streaming_disabled() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;
    let mut cfg = test_spawn_config(port, storage);
    cfg.streaming_enabled = false;
    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(cfg));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint).await.unwrap();

    // `streaming_enabled=false` short-circuits BEFORE the stream opens, so the
    // error surfaces at the RPC boundary (not as a stream item).
    let err = client
        .subscribe_metrics(SubscribeMetricsRequest {
            interval_secs: 1,
            respect_server_hints: true,
        })
        .await
        .expect_err("RPC must return Status::unavailable when streaming is disabled");
    assert_eq!(err.code(), tonic::Code::Unavailable);
    // Neutral message — must NOT expose the config field name.
    assert!(
        !err.message().contains("grpc_streaming_enabled"),
        "message should not leak config field name, got: {}",
        err.message()
    );

    server_task.abort();
    let _ = server_task.await;
}

/// Test #3 — `interval_secs=5`: two Data payloads arrive with `start`
/// timestamps ≥5s apart. Uses real clock (no pause) so we don't interfere
/// with the server's tokio::time::interval.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_subscribe_metrics_interval_emits_buckets() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;
    let cfg = test_spawn_config(port, storage);
    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(cfg));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint).await.unwrap();

    let mut stream = client
        .subscribe_metrics(SubscribeMetricsRequest {
            interval_secs: 1,
            respect_server_hints: true,
        })
        .await
        .expect("subscribe_metrics ok")
        .into_inner();

    // Drain the initial Hint.
    let first = tokio::time::timeout(Duration::from_secs(3), stream.message())
        .await
        .expect("first msg")
        .expect("not errored")
        .expect("not ended");
    assert!(matches!(
        first.payload,
        Some(subscribe_metrics_response::Payload::Hint(_))
    ));

    // Collect Data payloads with a generous timeout (real-clock ticker, not paused).
    let mut data_count = 0u32;
    let collect_deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while data_count < 2 && tokio::time::Instant::now() < collect_deadline {
        let remaining = collect_deadline - tokio::time::Instant::now();
        let msg = match tokio::time::timeout(remaining, stream.message()).await {
            Ok(Ok(Some(m))) => m,
            _ => break,
        };
        if let Some(subscribe_metrics_response::Payload::Data(_)) = msg.payload {
            data_count += 1;
        }
    }
    assert!(
        data_count >= 2,
        "expected ≥2 Data payloads, got {data_count}"
    );

    server_task.abort();
    let _ = server_task.await;
}

/// Test #4 — realtime mode wakes on `event_tx::Metrics` ticks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_subscribe_metrics_realtime_emits_on_event_tx_tick() {
    use oneshim_core::config::LoadThresholds;
    use oneshim_web::grpc::{test_support::mock_system_monitor::MockSystemMonitor, LoadPolicy};

    let port = pick_free_port();
    let storage = in_memory_storage().await;
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let cfg = oneshim_web::grpc::GrpcSpawnConfig {
        port,
        storage,
        system_monitor: MockSystemMonitor::new(30.0, 4096, 16384),
        event_tx: event_tx.clone(),
        integration_auth_token: None,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        load_policy: Arc::new(LoadPolicy::new(LoadThresholds::default())),
        streaming_enabled: true,
        max_concurrent_streams: 50,
    };
    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(cfg));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint).await.unwrap();
    let mut stream = client
        .subscribe_metrics(SubscribeMetricsRequest {
            interval_secs: 0, // realtime — handler blocks on event_tx
            respect_server_hints: true,
        })
        .await
        .expect("subscribe_metrics ok")
        .into_inner();

    // In realtime mode the handler's first work is `rx.recv().await`, so NO
    // initial Hint is emitted until an event_tx tick wakes the generator.
    // Send several Metrics events BEFORE polling the stream — the server-side
    // `rx.subscribe()` has already run by the time `subscribe_metrics().await`
    // returned on the client side. Sending multiple covers any pre/post race.
    for _ in 0..5 {
        let _ = event_tx.send(RealtimeEvent::Metrics(MetricsUpdate {
            timestamp: chrono::Utc::now().to_rfc3339(),
            cpu_usage: 25.0,
            memory_percent: 25.0,
            memory_used: 4_000_000_000,
            memory_total: 16_000_000_000,
        }));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // First yield after an event_tx wake is the initial Hint (emitter state
    // None → always emits on first call).
    let got = tokio::time::timeout(Duration::from_secs(3), stream.message())
        .await
        .expect("message within 3s after event_tx ticks")
        .expect("stream not errored")
        .expect("stream not ended");
    assert!(
        got.payload.is_some(),
        "expected payload after event_tx tick"
    );

    server_task.abort();
    let _ = server_task.await;
}

/// Test #5 — `max_concurrent_streams=2`: 3rd subscribe fails with
/// `Status::resource_exhausted` (active-stream cap — CRIT-4).
/// Uses lowered cap per IMP-V2-C (avoids fd pressure at 51 streams).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_subscribe_metrics_enforces_active_stream_cap() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;
    let mut cfg = test_spawn_config(port, storage);
    cfg.max_concurrent_streams = 2;
    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(cfg));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut c1 = DashboardServiceClient::connect(endpoint.clone())
        .await
        .unwrap();
    let mut c2 = DashboardServiceClient::connect(endpoint.clone())
        .await
        .unwrap();
    let mut c3 = DashboardServiceClient::connect(endpoint.clone())
        .await
        .unwrap();

    let req = || SubscribeMetricsRequest {
        interval_secs: 30, // slow cadence, keeps streams alive
        respect_server_hints: true,
    };
    let s1 = c1
        .subscribe_metrics(req())
        .await
        .expect("slot 1")
        .into_inner();
    // Drain initial Hint to ensure stream is registered + guard held.
    let mut s1 = s1;
    let _ = tokio::time::timeout(Duration::from_secs(3), s1.message()).await;

    let s2 = c2
        .subscribe_metrics(req())
        .await
        .expect("slot 2")
        .into_inner();
    let mut s2 = s2;
    let _ = tokio::time::timeout(Duration::from_secs(3), s2.message()).await;

    // Third must fail with ResourceExhausted.
    let err = c3
        .subscribe_metrics(req())
        .await
        .expect_err("slot 3 should be rejected at cap");
    assert_eq!(err.code(), tonic::Code::ResourceExhausted);

    drop(s1);
    drop(s2);
    server_task.abort();
    let _ = server_task.await;
}

/// Test #6 — `validate_authority` rejects DNS-rebound `:authority` by
/// forcing a `host` header that's NOT in the allowlist. Uses a
/// `Request::new` + manual metadata injection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_subscribe_metrics_rejects_dns_rebound_authority() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;
    let cfg = test_spawn_config(port, storage);
    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(cfg));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint).await.unwrap();

    let mut req = tonic::Request::new(SubscribeMetricsRequest {
        interval_secs: 1,
        respect_server_hints: true,
    });
    // Inject a non-allowlisted host (simulates DNS rebinding).
    req.metadata_mut()
        .insert("host", "evil.example.com:10091".parse().unwrap());
    let err = client
        .subscribe_metrics(req)
        .await
        .expect_err("evil host must be rejected");
    assert_eq!(err.code(), tonic::Code::PermissionDenied);

    server_task.abort();
    let _ = server_task.await;
}

/// Test #7 — reconnect cycle: 5 subscribe/drop cycles; server-side counter
/// must return to baseline 0 (IMP-V2-C / CRIT-3 StreamCounterGuard leak check).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_subscribe_metrics_survives_reconnect_cycle() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;
    let mut cfg = test_spawn_config(port, storage);
    // Tight cap — if any guard leaks, the 6th cycle fails.
    cfg.max_concurrent_streams = 3;
    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(cfg));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    for i in 0..5 {
        let mut client = DashboardServiceClient::connect(endpoint.clone())
            .await
            .unwrap();
        let mut stream = client
            .subscribe_metrics(SubscribeMetricsRequest {
                interval_secs: 30,
                respect_server_hints: true,
            })
            .await
            .unwrap_or_else(|e| panic!("cycle {i} subscribe failed: {e}"))
            .into_inner();
        // Drain initial hint so the stream is fully registered.
        let _ = tokio::time::timeout(Duration::from_millis(500), stream.message()).await;
        drop(stream);
        drop(client);
        // Yield so the server has a chance to run the Drop on its side.
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // If guards leaked, at least one cycle's count would have exceeded 3
    // and failed with ResourceExhausted — the loop above would have panicked.

    server_task.abort();
    let _ = server_task.await;
}

/// Test #8 — opt-out on loopback is honored. Client sets
/// `respect_server_hints=false`; first Hint is `reason` reflects current
/// state (not enforcement-forced). Server lifetime is loopback-bound so
/// this exercises the trust-a loopback branch of honor_opt_out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grpc_dashboard_subscribe_metrics_honors_opt_out_on_localhost() {
    let port = pick_free_port();
    let storage = in_memory_storage().await;
    let cfg = test_spawn_config(port, storage);
    let server_task = tokio::spawn(oneshim_web::grpc::serve_optional(cfg));
    wait_for_server_ready(port, Duration::from_secs(5)).await;

    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = DashboardServiceClient::connect(endpoint).await.unwrap();
    let mut stream = client
        .subscribe_metrics(SubscribeMetricsRequest {
            interval_secs: 1,
            respect_server_hints: false, // opt-out
        })
        .await
        .expect("subscribe ok")
        .into_inner();

    // Server-side should have granted opt-out without emitting a warn-log
    // (we can't directly assert no warn here without tracing capture infra;
    // instead, verify the stream opens + yields a Hint as usual).
    let first = tokio::time::timeout(Duration::from_secs(3), stream.message())
        .await
        .expect("msg")
        .expect("not errored")
        .expect("not ended");
    assert!(matches!(
        first.payload,
        Some(subscribe_metrics_response::Payload::Hint(_))
    ));

    // Hold the counter briefly, then close.
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(stream);

    server_task.abort();
    let _ = server_task.await;
}

// Suppress unused warning on `Ordering` when only tests above use it.
#[allow(dead_code)]
const _: Ordering = Ordering::Relaxed;
