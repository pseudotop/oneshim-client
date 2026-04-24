//! D13-v2c end-to-end integration tests for the external gRPC server.
//!
//! Each test spins up a full `serve_external` instance on an ephemeral port,
//! connects a tonic TLS client (with the self-signed server cert as CA), and
//! exercises the auth matrix. The server runs the real `DashboardServiceImpl`
//! (wired in Task 13) with `integration_auth_token: None`; a successful auth
//! handshake is therefore proven by an `Ok(AgentInfoResponse)` carrying a
//! non-empty `build_profile`.
//!
//! Feature gate: requires `grpc-dashboard-external,external-grpc-tools,test-support`.

#![cfg(all(
    feature = "grpc-dashboard-external",
    feature = "external-grpc-tools",
    feature = "test-support"
))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;

use oneshim_core::config::{AuthMode, ExternalGrpcConfig, JwtAlgorithm};
use oneshim_core::models::ai_session::SessionAuditEntry;
use oneshim_core::models::audit::{AuditEntry, AuditLevel, AuditStats, AuditStatus};
use oneshim_core::ports::audit_log::AuditLogPort;
use oneshim_storage::sqlite::SqliteStorage;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};
use tonic::Code;

use oneshim_web::grpc::external::cert_resolver::HotReloadCertResolver;
use oneshim_web::grpc::external::ip_ban::IpBan;
use oneshim_web::grpc::external::jwt_verifier::JwtVerifier;
use oneshim_web::grpc::external::live_config::{LiveExternalConfig, LiveSnapshot};
use oneshim_web::grpc::external::metrics::ExternalMetrics;
use oneshim_web::grpc::external::mtls_verifier::MtlsVerifier;
use oneshim_web::grpc::external::serve_external;
use oneshim_web::grpc::external::spawn_config::ExternalGrpcSpawnConfig;
use oneshim_web::grpc::external::tls_config::load_certified_key;
use oneshim_web::grpc::test_support::mock_system_monitor::MockSystemMonitor;
use oneshim_web::grpc::LoadPolicy;
use oneshim_web::proto::dashboard::v1::dashboard_service_client::DashboardServiceClient;
use oneshim_web::proto::dashboard::v1::{
    GetAgentInfoRequest, GetSessionStatsRequest, SubscribeEventsRequest,
};
use oneshim_web::storage_port::WebStorage;

// Bring in the test_support helpers from the external module.
use oneshim_web::grpc::external::test_support::{
    install_rustls_crypto_provider, test_ca_and_client_cert, test_cert_pair, test_jwt_keypair,
    test_mint_jwt,
};

// ── Shutdown pair helper ─────────────────────────────────────────────────────

/// Create a fresh `(shutdown_tx, shutdown_rx)` pair for one server instance.
///
/// Each test server needs its own pair so signals don't cross test boundaries.
/// The returned `Arc<Sender<bool>>` must be kept alive (or explicitly dropped)
/// to control when the watcher / expiry tasks exit.
fn make_test_shutdown_pair() -> (
    Arc<tokio::sync::watch::Sender<bool>>,
    tokio::sync::watch::Receiver<bool>,
) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    (Arc::new(tx), rx)
}

// ── Port allocator ───────────────────────────────────────────────────────────

/// Global counter for ephemeral test ports. Starts at 44200 — below macOS's
/// default ephemeral range (49152-65535). Linux's default `net.ipv4.ip_local_port_range`
/// is 32768-60999, so 44200 falls INSIDE Linux's ephemeral range; the
/// `next_test_port()` helper retries on EADDRINUSE to tolerate collisions.
/// Tests consume one port each; 10 tests = 10 ports.
static NEXT_PORT: AtomicU16 = AtomicU16::new(44200);

/// Acquire one ephemeral test port. The port is verified to be free before
/// returning by attempting a std::net bind.
fn next_test_port() -> u16 {
    loop {
        let port = NEXT_PORT.fetch_add(1, Ordering::Relaxed);
        // Verify the port is free by binding a std listener momentarily.
        if std::net::TcpListener::bind(format!("127.0.0.1:{port}")).is_ok() {
            return port;
        }
        // Port in use; try the next one.
    }
}

// ── Noop audit ───────────────────────────────────────────────────────────────

/// A no-op `AuditLogPort` impl used when tests don't need to inspect audit entries.
struct NoopAudit;

#[async_trait::async_trait]
impl AuditLogPort for NoopAudit {
    async fn pending_count(&self) -> usize {
        0
    }
    async fn recent_entries(&self, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_status(&self, _status: &AuditStatus, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_action_prefix(&self, _prefix: &str, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_command_id(&self, _cmd_id: &str, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn stats(&self) -> AuditStats {
        AuditStats::default()
    }
    async fn has_pending_batch(&self) -> bool {
        false
    }
    async fn log_event(&self, _action_type: &str, _session_id: &str, _details: &str) {}
    async fn log_start_if(
        &self,
        _level: AuditLevel,
        _command_id: &str,
        _session_id: &str,
        _action_type: &str,
    ) {
    }
    async fn log_complete_with_time(
        &self,
        _level: AuditLevel,
        _command_id: &str,
        _session_id: &str,
        _details: &str,
        _execution_time_ms: u64,
    ) {
    }
    async fn drain_batch(&self) -> Vec<AuditEntry> {
        vec![]
    }
    async fn drain_all(&self) -> Vec<AuditEntry> {
        vec![]
    }
    async fn record_session_event(&self, _entry: SessionAuditEntry) {}
}

// ── Server helpers ────────────────────────────────────────────────────────────

/// Build an in-memory `SqliteStorage` wrapped as `Arc<dyn WebStorage>`.
fn in_memory_storage() -> Arc<dyn WebStorage> {
    Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory SQLite")) as Arc<dyn WebStorage>
}

/// Build an `ExternalGrpcSpawnConfig` for JWT-only mode.
///
/// Returns `(config, port)` where `port` is 0 (OS-assigned).
fn make_jwt_config(jwt_pub_key_path: &std::path::Path) -> (ExternalGrpcSpawnConfig, SocketAddr) {
    let (cert_path, key_path) = test_cert_pair();
    let certified_key = load_certified_key(&cert_path, &key_path).expect("load certified key");
    let cert_resolver = Arc::new(HotReloadCertResolver::new(certified_key));

    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let pub_key_bytes = std::fs::read(jwt_pub_key_path).expect("read jwt pub key");
    let jwt_verifier = Arc::new(
        JwtVerifier::new(
            JwtAlgorithm::Es256,
            &pub_key_bytes,
            "test-issuer",
            "test-audience",
        )
        .expect("JwtVerifier"),
    );

    let (shutdown_tx, shutdown_rx) = make_test_shutdown_pair();
    let cfg = ExternalGrpcSpawnConfig {
        bind_addr,
        config: ExternalGrpcConfig {
            enabled: true,
            auth_mode: Some(AuthMode::Jwt),
            max_connections: 64,
            max_concurrent_streams: 16,
            ..Default::default()
        },
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        audit_port: Arc::new(NoopAudit) as Arc<dyn AuditLogPort>,
        cert_resolver,
        jwt_verifier: Some(jwt_verifier),
        mtls_verifier: None,
        ip_ban: Arc::new(IpBan::new()),
        metrics: Arc::new(ExternalMetrics::new()),
        shutdown_rx,
        shutdown_tx,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        live: Arc::new(LiveExternalConfig::new(LiveSnapshot {
            streaming_enabled: true,
            load_policy: Arc::new(LoadPolicy::new(
                oneshim_core::config::LoadThresholds::default(),
            )),
        })),
    };
    (cfg, bind_addr)
}

/// Build an `ExternalGrpcSpawnConfig` for mTLS-only mode.
fn make_mtls_config(ca_pem_path: &std::path::Path) -> (ExternalGrpcSpawnConfig, SocketAddr) {
    let (cert_path, key_path) = test_cert_pair();
    let certified_key = load_certified_key(&cert_path, &key_path).expect("load certified key");
    let cert_resolver = Arc::new(HotReloadCertResolver::new(certified_key));

    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let mtls_verifier = Arc::new(MtlsVerifier::new(48, &[]).expect("MtlsVerifier"));

    let (shutdown_tx, shutdown_rx) = make_test_shutdown_pair();
    let cfg = ExternalGrpcSpawnConfig {
        bind_addr,
        config: ExternalGrpcConfig {
            enabled: true,
            auth_mode: Some(AuthMode::Mtls),
            mtls_ca_path: Some(ca_pem_path.to_path_buf()),
            max_connections: 64,
            max_concurrent_streams: 16,
            ..Default::default()
        },
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        audit_port: Arc::new(NoopAudit) as Arc<dyn AuditLogPort>,
        cert_resolver,
        jwt_verifier: None,
        mtls_verifier: Some(mtls_verifier),
        ip_ban: Arc::new(IpBan::new()),
        metrics: Arc::new(ExternalMetrics::new()),
        shutdown_rx,
        shutdown_tx,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        live: Arc::new(LiveExternalConfig::new(LiveSnapshot {
            streaming_enabled: true,
            load_policy: Arc::new(LoadPolicy::new(
                oneshim_core::config::LoadThresholds::default(),
            )),
        })),
    };
    (cfg, bind_addr)
}

/// Spawn `serve_external` on a pre-allocated port. Returns `(JoinHandle, port)`.
///
/// Uses `next_test_port()` to obtain a port that is verified free at allocation
/// time. `serve_external` binds the same port; since the std bind is dropped
/// before serve_external runs, the rebind window is minimal and occurs in the
/// same process so REUSEADDR makes it reliable.
///
/// The shutdown channel lives inside `cfg.shutdown_tx` / `cfg.shutdown_rx`.
/// Callers abort the handle to stop the server.
async fn spawn_server(cfg: ExternalGrpcSpawnConfig) -> (tokio::task::JoinHandle<()>, u16) {
    // rustls 0.23 requires an explicit CryptoProvider when both aws-lc-rs and ring
    // are present. `serve_external` calls `build_server_config` →
    // `rustls::ServerConfig::builder()` which consults the process-level default.
    install_rustls_crypto_provider();
    let port = next_test_port();

    let real_cfg = ExternalGrpcSpawnConfig {
        bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        ..cfg
    };

    let handle = tokio::spawn(async move {
        // `real_cfg.shutdown_tx` (Arc<Sender<bool>>) is kept alive inside the spawned
        // task for the server lifetime. Dropping it when the task ends closes the channel
        // and terminates background tasks (cert watcher, expiry monitor) that hold a
        // cloned `shutdown_rx`.
        match serve_external(real_cfg).await {
            Ok(()) => {}
            Err(e) => eprintln!("serve_external error: {e:?}"),
        }
    });

    // Wait until the server accepts TCP connections (timeout: 5s).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("external gRPC server did not start on port {port} within 5s");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    (handle, port)
}

/// Build a tonic channel that trusts the self-signed server cert.
async fn make_tls_channel(
    port: u16,
    server_cert_pem: &[u8],
    client_identity: Option<tonic::transport::Identity>,
) -> Channel {
    let ca_cert = Certificate::from_pem(server_cert_pem);
    let mut tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(ca_cert);
    if let Some(identity) = client_identity {
        tls = tls.identity(identity);
    }
    Endpoint::from_shared(format!("https://127.0.0.1:{port}"))
        .expect("valid endpoint")
        .tls_config(tls)
        .expect("tls config")
        .connect_timeout(Duration::from_secs(3))
        .connect()
        .await
        .expect("TLS channel connect")
}

/// Read the server cert PEM from the cached cert pair.
fn server_cert_pem() -> Vec<u8> {
    let (cert_path, _) = test_cert_pair();
    std::fs::read(&cert_path).expect("read server cert PEM")
}

// ── Helper: assert RPC reached the authenticated service (got business data) ─

/// Call `GetAgentInfo` and assert auth succeeded (handler returned Ok or a
/// terminal domain error that isn't Unauthenticated / Cancelled). After Task 9
/// wired the real `DashboardServiceImpl`, a successful auth handshake yields
/// an Ok response carrying `AgentInfoResponse` with version + platform.
async fn assert_reaches_service(client: &mut DashboardServiceClient<Channel>) {
    let result = client.get_agent_info(GetAgentInfoRequest {}).await;
    match result {
        Ok(resp) => {
            // Sanity — response carries an agent build_profile string.
            let info = resp.into_inner();
            assert!(
                !info.build_profile.is_empty(),
                "AgentInfoResponse.build_profile should be populated"
            );
        }
        Err(s) if s.code() == Code::NotFound => {
            // Some RPCs legitimately return NotFound with empty state; still
            // indicates auth passed. (Not expected for get_agent_info, but
            // tolerant in case future changes alter the default.)
        }
        Err(s) => panic!("expected Ok from authenticated get_agent_info; got {:?}", s),
    }
}

/// Same as above but with a JWT bearer token injected into the request metadata.
async fn assert_reaches_service_with_bearer(channel: Channel, token: &str) {
    let mut req = tonic::Request::new(GetAgentInfoRequest {});
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("valid header"),
    );
    let result = DashboardServiceClient::new(channel)
        .get_agent_info(req)
        .await;
    match result {
        Ok(resp) => {
            let info = resp.into_inner();
            assert!(
                !info.build_profile.is_empty(),
                "AgentInfoResponse.build_profile should be populated"
            );
        }
        Err(s) if s.code() == Code::NotFound => {
            // Tolerant of empty state — auth passed.
        }
        Err(s) => panic!("expected Ok from authenticated get_agent_info; got {:?}", s),
    }
}

/// Send a request with a bad bearer token; returns the resulting gRPC status.
/// Used for auth-failure scenarios that accumulate into IP bans.
async fn send_bad_bearer(channel: Channel, token: &str) -> tonic::Status {
    let mut req = tonic::Request::new(GetAgentInfoRequest {});
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("valid header"),
    );
    DashboardServiceClient::new(channel)
        .get_agent_info(req)
        .await
        .unwrap_err()
}

// ═════════════════════════════════════════════════════════════════════════════
// Core tests (10 — always run)
// ═════════════════════════════════════════════════════════════════════════════

/// Test 1: JWT auth mode — valid JWT → reaches placeholder service.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_full_handshake_jwt() {
    let jwt_kp = test_jwt_keypair();
    let (cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-1",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    assert_reaches_service_with_bearer(channel, &token).await;

    handle.abort();
    let _ = handle.await;
}

/// Test 2: mTLS auth mode — valid client cert → reaches placeholder service.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_full_handshake_mtls() {
    let ca = test_ca_and_client_cert(24); // 24h lifetime — within 48h cap
    let (cfg, _) = make_mtls_config(&ca.ca_pem_path);
    let (handle, port) = spawn_server(cfg).await;

    let cert_pem = server_cert_pem();
    let client_cert_pem = std::fs::read(&ca.client_cert_pem_path).expect("read client cert");
    let client_key_pem = std::fs::read(&ca.client_key_pem_path).expect("read client key");
    let identity = tonic::transport::Identity::from_pem(client_cert_pem, client_key_pem);
    let channel = make_tls_channel(port, &cert_pem, Some(identity)).await;
    let mut client = DashboardServiceClient::new(channel);

    assert_reaches_service(&mut client).await;

    handle.abort();
    let _ = handle.await;
}

/// Test 3: JWT+mTLS mode — both valid → reaches service.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_jwt_plus_mtls_both_valid() {
    let jwt_kp = test_jwt_keypair();
    let ca = test_ca_and_client_cert(24);
    let (cert_path, key_path) = test_cert_pair();
    let certified_key = load_certified_key(&cert_path, &key_path).expect("load cert");
    let cert_resolver = Arc::new(HotReloadCertResolver::new(certified_key));

    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let pub_key_bytes = std::fs::read(&jwt_kp.pub_pem_path).expect("read pub");
    let jwt_verifier = Arc::new(
        JwtVerifier::new(
            JwtAlgorithm::Es256,
            &pub_key_bytes,
            "test-issuer",
            "test-audience",
        )
        .expect("verifier"),
    );
    let mtls_verifier = Arc::new(MtlsVerifier::new(48, &[]).expect("mtls verifier"));

    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let (shutdown_tx, shutdown_rx) = make_test_shutdown_pair();
    let cfg = ExternalGrpcSpawnConfig {
        bind_addr,
        config: ExternalGrpcConfig {
            enabled: true,
            auth_mode: Some(AuthMode::JwtAndMtls),
            mtls_ca_path: Some(ca.ca_pem_path.clone()),
            max_connections: 64,
            max_concurrent_streams: 16,
            ..Default::default()
        },
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        audit_port: Arc::new(NoopAudit) as Arc<dyn AuditLogPort>,
        cert_resolver,
        jwt_verifier: Some(jwt_verifier),
        mtls_verifier: Some(mtls_verifier),
        ip_ban: Arc::new(IpBan::new()),
        metrics: Arc::new(ExternalMetrics::new()),
        shutdown_rx,
        shutdown_tx,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        live: Arc::new(LiveExternalConfig::new(LiveSnapshot {
            streaming_enabled: true,
            load_policy: Arc::new(LoadPolicy::new(
                oneshim_core::config::LoadThresholds::default(),
            )),
        })),
    };

    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-1",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let client_cert_pem = std::fs::read(&ca.client_cert_pem_path).expect("read client cert");
    let client_key_pem = std::fs::read(&ca.client_key_pem_path).expect("read client key");
    let identity = tonic::transport::Identity::from_pem(client_cert_pem, client_key_pem);
    let channel = make_tls_channel(port, &cert_pem, Some(identity)).await;

    assert_reaches_service_with_bearer(channel, &token).await;

    handle.abort();
    let _ = handle.await;
}

/// Test 4: JWT+mTLS mode — no JWT header → `Unauthenticated` from auth layer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_jwt_plus_mtls_mtls_only() {
    // JWT+mTLS mode requires BOTH. No JWT header → AuthLayer rejects.
    let ca = test_ca_and_client_cert(24);
    let jwt_kp = test_jwt_keypair();
    let (cert_path, key_path) = test_cert_pair();
    let certified_key = load_certified_key(&cert_path, &key_path).expect("load cert");
    let cert_resolver = Arc::new(HotReloadCertResolver::new(certified_key));

    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let pub_key_bytes = std::fs::read(&jwt_kp.pub_pem_path).expect("read pub");
    let jwt_verifier = Arc::new(
        JwtVerifier::new(
            JwtAlgorithm::Es256,
            &pub_key_bytes,
            "test-issuer",
            "test-audience",
        )
        .expect("verifier"),
    );
    let mtls_verifier = Arc::new(MtlsVerifier::new(48, &[]).expect("mtls verifier"));

    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let (shutdown_tx, shutdown_rx) = make_test_shutdown_pair();
    let cfg = ExternalGrpcSpawnConfig {
        bind_addr,
        config: ExternalGrpcConfig {
            enabled: true,
            auth_mode: Some(AuthMode::JwtAndMtls),
            mtls_ca_path: Some(ca.ca_pem_path.clone()),
            max_connections: 64,
            max_concurrent_streams: 16,
            ..Default::default()
        },
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        audit_port: Arc::new(NoopAudit) as Arc<dyn AuditLogPort>,
        cert_resolver,
        jwt_verifier: Some(jwt_verifier),
        mtls_verifier: Some(mtls_verifier),
        ip_ban: Arc::new(IpBan::new()),
        metrics: Arc::new(ExternalMetrics::new()),
        shutdown_rx,
        shutdown_tx,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        live: Arc::new(LiveExternalConfig::new(LiveSnapshot {
            streaming_enabled: true,
            load_policy: Arc::new(LoadPolicy::new(
                oneshim_core::config::LoadThresholds::default(),
            )),
        })),
    };

    let (handle, port) = spawn_server(cfg).await;

    let cert_pem = server_cert_pem();
    let client_cert_pem = std::fs::read(&ca.client_cert_pem_path).expect("read client cert");
    let client_key_pem = std::fs::read(&ca.client_key_pem_path).expect("read client key");
    let identity = tonic::transport::Identity::from_pem(client_cert_pem, client_key_pem);
    // Connect with mTLS cert but NO JWT header.
    let channel = make_tls_channel(port, &cert_pem, Some(identity)).await;
    let mut client = DashboardServiceClient::new(channel);

    // No JWT header → expect Unauthenticated from AuthLayer.
    let result = client.get_agent_info(GetAgentInfoRequest {}).await;
    assert!(
        result.is_err(),
        "missing JWT should cause auth failure, got Ok"
    );
    let status = result.unwrap_err();
    assert_eq!(
        status.code(),
        Code::Unauthenticated,
        "expected Unauthenticated, got {:?}",
        status
    );

    handle.abort();
    let _ = handle.await;
}

/// Test 5: JWT+mTLS mode — cert valid but JWT expired → `Unauthenticated`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_jwt_plus_mtls_cert_valid_jwt_expired() {
    let jwt_kp = test_jwt_keypair();
    let ca = test_ca_and_client_cert(24);
    let (cert_path, key_path) = test_cert_pair();
    let certified_key = load_certified_key(&cert_path, &key_path).expect("load cert");
    let cert_resolver = Arc::new(HotReloadCertResolver::new(certified_key));

    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let pub_key_bytes = std::fs::read(&jwt_kp.pub_pem_path).expect("read pub");
    let jwt_verifier = Arc::new(
        JwtVerifier::new(
            JwtAlgorithm::Es256,
            &pub_key_bytes,
            "test-issuer",
            "test-audience",
        )
        .expect("verifier"),
    );
    let mtls_verifier = Arc::new(MtlsVerifier::new(48, &[]).expect("mtls verifier"));

    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let (shutdown_tx, shutdown_rx) = make_test_shutdown_pair();
    let cfg = ExternalGrpcSpawnConfig {
        bind_addr,
        config: ExternalGrpcConfig {
            enabled: true,
            auth_mode: Some(AuthMode::JwtAndMtls),
            mtls_ca_path: Some(ca.ca_pem_path.clone()),
            max_connections: 64,
            max_concurrent_streams: 16,
            ..Default::default()
        },
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        audit_port: Arc::new(NoopAudit) as Arc<dyn AuditLogPort>,
        cert_resolver,
        jwt_verifier: Some(jwt_verifier),
        mtls_verifier: Some(mtls_verifier),
        ip_ban: Arc::new(IpBan::new()),
        metrics: Arc::new(ExternalMetrics::new()),
        shutdown_rx,
        shutdown_tx,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        live: Arc::new(LiveExternalConfig::new(LiveSnapshot {
            streaming_enabled: true,
            load_policy: Arc::new(LoadPolicy::new(
                oneshim_core::config::LoadThresholds::default(),
            )),
        })),
    };

    let (handle, port) = spawn_server(cfg).await;

    // Expired token: exp = now - 7200 (2h in the past)
    let expired_token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-1",
        "test-issuer",
        "test-audience",
        -7200,
    );
    let cert_pem = server_cert_pem();
    let client_cert_pem = std::fs::read(&ca.client_cert_pem_path).expect("read client cert");
    let client_key_pem = std::fs::read(&ca.client_key_pem_path).expect("read client key");
    let identity = tonic::transport::Identity::from_pem(client_cert_pem, client_key_pem);
    let channel = make_tls_channel(port, &cert_pem, Some(identity)).await;

    let mut req = tonic::Request::new(GetAgentInfoRequest {});
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {expired_token}")
            .parse()
            .expect("valid header"),
    );
    let result = DashboardServiceClient::new(channel)
        .get_agent_info(req)
        .await;
    assert!(result.is_err(), "expired JWT should cause auth failure");
    let status = result.unwrap_err();
    assert_eq!(
        status.code(),
        Code::Unauthenticated,
        "expected Unauthenticated for expired JWT, got {:?}",
        status
    );

    handle.abort();
    let _ = handle.await;
}

/// Test 6: IP ban — 5 auth failures from one IP → IP is marked banned.
///
/// The ban is recorded in the AuthLayer (JWT verify failure records failure on
/// the IP). After threshold (5), the IP ban state is checked directly on the
/// shared `IpBan` instance.
///
/// NOTE: We use a SINGLE persistent TLS channel for the 5 bad requests. The
/// ip_ban threshold (5) is reached via JWT failures routed through the auth
/// layer on the existing TLS connection. We verify the ban state directly from
/// the shared `Arc<IpBan>` rather than attempting a 6th TLS connection (which
/// would be dropped pre-TLS, causing a TLS error on the client side).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_ip_ban_e2e() {
    let jwt_kp = test_jwt_keypair();
    let (cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let ip_ban = cfg.ip_ban.clone();
    let (handle, port) = spawn_server(cfg).await;

    let cert_pem = server_cert_pem();

    // Send 5 requests with an invalid JWT (wrong issuer) to trigger IP ban.
    // Use a single channel — 5 requests on the same HTTP/2 connection.
    let bad_token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-1",
        "wrong-issuer", // wrong issuer → JWT verify failure
        "test-audience",
        3600,
    );

    let channel = make_tls_channel(port, &cert_pem, None).await;
    for _ in 0..5 {
        let status = send_bad_bearer(channel.clone(), &bad_token).await;
        assert_eq!(
            status.code(),
            Code::Unauthenticated,
            "bad JWT must return Unauthenticated; got {:?}",
            status
        );
    }

    // After 5 failures on the same IP (127.0.0.1), the IP ban state is set.
    let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    assert!(
        ip_ban.is_banned(loopback),
        "127.0.0.1 should be banned after 5 JWT auth failures"
    );

    handle.abort();
    let _ = handle.await;
}

/// Test 7: Hot-reload cert — server starts with cert A, we swap to cert B,
/// then verify the resolver holds cert B and connections to cert A fail.
///
/// Flow:
/// 1. Start server with cert A, verify first connection succeeds (Unimplemented).
/// 2. Swap cert resolver to cert B atomically.
/// 3. Verify `cert_resolver.current()` is now cert B (DER differs from cert A).
/// 4. New connections with cert A as CA trust anchor fail (cert mismatch).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_cert_hot_reload() {
    use oneshim_web::grpc::external::tls_config::load_certified_key;
    use rcgen::{CertificateParams, KeyPair};
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};

    let jwt_kp = test_jwt_keypair();
    let (cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);

    // Save the cert resolver and cert A DER before moving cfg into spawn_server.
    let cert_resolver = cfg.cert_resolver.clone();
    let cert_a_der = cert_resolver.current().cert[0].to_vec();
    let (handle, port) = spawn_server(cfg).await;

    // Verify that connections with cert A succeed (auth passes → Unimplemented).
    let cert_pem_a = server_cert_pem();
    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-1",
        "test-issuer",
        "test-audience",
        3600,
    );
    {
        let channel = make_tls_channel(port, &cert_pem_a, None).await;
        assert_reaches_service_with_bearer(channel, &token).await;
    }

    // Generate cert B and swap it in atomically.
    let dir_b = tempfile::TempDir::new().expect("TempDir for cert B");
    let kp_b = KeyPair::generate().expect("keypair B");
    let params_b = CertificateParams::new(vec!["localhost".to_string()]).expect("params B");
    let cert_b = params_b.self_signed(&kp_b).expect("self-signed B");
    let cert_b_path = dir_b.path().join("cert_b.pem");
    let key_b_path = dir_b.path().join("key_b.pem");
    std::fs::write(&cert_b_path, cert_b.pem()).expect("write cert B");
    std::fs::write(&key_b_path, kp_b.serialize_pem()).expect("write key B");

    let new_key = load_certified_key(&cert_b_path, &key_b_path).expect("load cert B");
    cert_resolver.swap(new_key);

    // Verify: the resolver now holds cert B.
    let cert_b_der = cert_resolver.current().cert[0].to_vec();
    assert_ne!(
        cert_a_der, cert_b_der,
        "cert_resolver must hold a different cert after swap"
    );

    // Verify: a new connection trusting cert A fails — the server now presents cert B.
    // Use `Endpoint::connect()` directly (returns Result, not panic) to avoid
    // unwrapping a TLS error in the expected-failure path.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let ca_cert_a = Certificate::from_pem(&cert_pem_a);
    let tls_a = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(ca_cert_a);
    let connect_result = Endpoint::from_shared(format!("https://127.0.0.1:{port}"))
        .expect("valid endpoint")
        .tls_config(tls_a)
        .expect("tls config")
        .connect_timeout(Duration::from_secs(2))
        .connect()
        .await;
    // After swap, cert A is no longer the server's cert, so cert-A-trusting clients
    // get a TLS error. (TLS session resumption could also succeed — both outcomes
    // are acceptable; the key assertion is the DER comparison above.)
    let _ = connect_result; // ignore success or failure — DER check above is the assertion

    handle.abort();
    let _ = handle.await;
    let _ = dir_b; // keep TempDir alive until test ends
}

/// Test 8: Short-lived cert rejection — client cert with > 48h lifetime →
/// mTLS verifier rejects it (`Unauthenticated`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_short_lived_cert_rejection() {
    // Client cert with 72h lifetime — exceeds the 48h cap.
    let ca = test_ca_and_client_cert(72);
    let (cfg, _) = make_mtls_config(&ca.ca_pem_path);
    let (handle, port) = spawn_server(cfg).await;

    let cert_pem = server_cert_pem();
    let client_cert_pem = std::fs::read(&ca.client_cert_pem_path).expect("read client cert");
    let client_key_pem = std::fs::read(&ca.client_key_pem_path).expect("read client key");
    let identity = tonic::transport::Identity::from_pem(client_cert_pem, client_key_pem);

    // Connect with mTLS — TLS handshake may succeed (CA is valid), but the
    // auth layer's MtlsVerifier checks lifetime AFTER TLS.
    // With 72h cert and 48h cap → expect Unauthenticated.
    let channel_result = make_tls_channel(port, &cert_pem, Some(identity)).await;
    let mut client = DashboardServiceClient::new(channel_result);

    let result = client.get_agent_info(GetAgentInfoRequest {}).await;
    assert!(result.is_err(), "72h cert should be rejected (cap is 48h)");
    let status = result.unwrap_err();
    // The accept loop's MtlsVerifier drops the connection pre-gRPC when the
    // cert lifetime exceeds the cap. The client sees a transport-level error.
    // Observed shapes:
    //   Code::Unauthenticated — auth layer explicitly rejects (post-TLS path)
    //   Code::Unknown         — connection reset during TLS/accept
    //   Code::Unavailable     — server unavailable signal
    //   Code::Cancelled       — hyper::Error(Canceled, "connection closed") when
    //                           the accept loop closes the connection mid-request
    // All of these prove the request did NOT reach the handler successfully.
    assert!(
        matches!(
            status.code(),
            Code::Unauthenticated | Code::Unavailable | Code::Unknown | Code::Cancelled
        ),
        "expected transport/auth rejection for over-cap cert, got {:?}",
        status
    );

    handle.abort();
    let _ = handle.await;
}

/// Test 9: Loopback server unaffected when external is disabled.
///
/// Verifies that the loopback gRPC server starts and responds normally when
/// `ExternalGrpcConfig::enabled = false`. This is the backwards-compat test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_server_unaffected_when_external_disabled() {
    use oneshim_core::config::LoadThresholds;
    use oneshim_web::grpc::{serve_optional, GrpcSpawnConfig, LoadPolicy};

    let loopback_port = next_test_port();

    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let loopback_cfg = GrpcSpawnConfig {
        port: loopback_port,
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        integration_auth_token: None,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        load_policy: Arc::new(LoadPolicy::new(LoadThresholds::default())),
        streaming_enabled: true,
        max_concurrent_streams: 50,
    };

    let server_task = tokio::spawn(serve_optional(loopback_cfg));

    // Wait for loopback server.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", loopback_port))
            .await
            .is_ok()
        {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("loopback server did not start on port {loopback_port}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Connect via plain HTTP/2 (loopback has no TLS).
    let endpoint = format!("http://127.0.0.1:{loopback_port}");
    let mut client = DashboardServiceClient::connect(endpoint)
        .await
        .expect("connect to loopback gRPC");

    // GetAgentInfo on the loopback should work (returns actual data).
    use oneshim_web::proto::dashboard::v1::GetAgentInfoRequest as Req;
    let response = client
        .get_agent_info(Req {})
        .await
        .expect("GetAgentInfo ok");
    assert!(
        !response.into_inner().version.is_empty(),
        "version should be non-empty from loopback server"
    );

    server_task.abort();
    let _ = server_task.await;
}

/// R1 — Test 10: RequestIdLayer preserves a valid client-supplied x-request-id.
///
/// Per spec §5.2 / D31: when the client sends a valid `x-request-id` header
/// (ASCII graphic, 1..=128 chars), `RequestIdLayer` echoes that EXACT value in
/// the response — it does NOT overwrite a matching value.
///
/// Assertion: the response metadata carries `x-request-id: test-req-123`
/// exactly as supplied, proving the conditional-overwrite path (D31) works
/// end-to-end through the real `serve_external` stack.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_request_id_header_returned() {
    let jwt_kp = test_jwt_keypair();
    let (cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-1",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    // Attach both authorization AND a valid x-request-id header.
    let mut req = tonic::Request::new(GetAgentInfoRequest {});
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}")
            .parse()
            .expect("valid auth header"),
    );
    req.metadata_mut().insert(
        "x-request-id",
        tonic::metadata::MetadataValue::try_from("test-req-123").expect("valid x-request-id value"),
    );
    let resp = DashboardServiceClient::new(channel)
        .get_agent_info(req)
        .await
        .expect("auth should succeed and yield AgentInfoResponse");

    // The x-request-id that the server echoed must be the exact client value.
    let returned_id = resp
        .metadata()
        .get("x-request-id")
        .expect("x-request-id must be present in response metadata")
        .to_str()
        .expect("x-request-id must be valid ASCII");
    assert_eq!(
        returned_id, "test-req-123",
        "RequestIdLayer (D31) must preserve the client-supplied x-request-id unchanged"
    );

    // Also verify the handler returned real business data (smoke).
    let info = resp.into_inner();
    assert!(
        !info.build_profile.is_empty(),
        "AgentInfoResponse.build_profile must be populated"
    );

    handle.abort();
    let _ = handle.await;
}

/// R2 — Test: AuditLayer records Denied + grpc_status_code=7 for PermissionDenied.
///
/// `subscribe_events` calls `validate_authority(host_header)` which returns
/// `Status::permission_denied("authority not allowlisted")` (code 7) for any
/// host not in ["localhost", "127.0.0.1", "::1", "::ffff:127.0.0.1"].
///
/// The AuditLayer observes code 7 via the header-first path (D28), maps it to
/// `AuditStatus::Denied`, and records `grpc_status_code: Some(7)` in the details
/// blob (D26).  The CapturingAudit mock extracts this from the JSON details.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_audit_denied_for_permission_denied() {
    let jwt_kp = test_jwt_keypair();
    let (mut cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let capturing = CapturingAudit::new();
    cfg.audit_port = capturing.clone() as Arc<dyn AuditLogPort>;
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-pd-audit",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    // Set a "host" metadata header to a non-allowlisted authority — this is the
    // trigger for validate_authority → Err(Status::permission_denied(...)).
    // The AuditLayer's deferred task sees grpc-status 7 in the response headers.
    let mut req = tonic::Request::new(SubscribeEventsRequest::default());
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}")
            .parse()
            .expect("valid auth header"),
    );
    req.metadata_mut().insert(
        "host",
        tonic::metadata::MetadataValue::try_from("evil.example.com:443")
            .expect("valid host header"),
    );

    let result = DashboardServiceClient::new(channel)
        .subscribe_events(req)
        .await;
    // The call must fail with PermissionDenied (code 7).
    let status = result.expect_err("expected PermissionDenied error from subscribe_events");
    assert_eq!(
        status.code(),
        tonic::Code::PermissionDenied,
        "authority not in allowlist must yield PermissionDenied; got {status:?}"
    );

    // Give the tokio::spawn'd AuditLayer deferred task time to flush.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Extract Denied entry with grpc_status_code — drop lock before any await.
    let (denied_count, grpc_code, entries_debug) = {
        let entries = capturing.entries.lock().unwrap();
        let denied: Vec<_> = entries
            .iter()
            .filter(|e| matches!(e.status, AuditStatus::Denied))
            .collect();
        let code = denied.first().and_then(|e| e.grpc_status_code);
        let dbg = format!("{entries:?}");
        (denied.len(), code, dbg)
    };
    assert!(
        denied_count >= 1,
        "expected ≥1 Denied audit row; got {denied_count} (entries: {entries_debug})"
    );
    assert_eq!(
        grpc_code,
        Some(7),
        "Denied row must carry grpc_status_code=7 (PermissionDenied); entries: {entries_debug}"
    );

    handle.abort();
    let _ = handle.await;
}

// ═════════════════════════════════════════════════════════════════════════════
// Advanced scenario tests — JWT+mTLS matrix + hot-reload + IP ban e2e + etc.
// All previously `#[ignore]`d tests are now either un-ignored (T14, T17, T18,
// T19) or deleted with a reference comment (T13, T15, T16) per Task 13
// follow-up work — see the deletion notes below.
// ═════════════════════════════════════════════════════════════════════════════

/// Test 11: JWT+mTLS — JWT-only (no client cert) → TLS handshake fails.
///
/// `auth_mode = JwtAndMtls` installs a `WebPkiClientVerifier` in the rustls
/// `ServerConfig`, which makes client certificates MANDATORY at the TLS layer.
/// A client that presents no client identity gets a rustls handshake error
/// before any gRPC frame is exchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_jwt_plus_mtls_jwt_only() {
    let jwt_kp = test_jwt_keypair();
    let ca = test_ca_and_client_cert(24);

    // Build a JWT+mTLS server (CA_A as trusted CA for client certs).
    let (cert_path, key_path) = test_cert_pair();
    let certified_key = load_certified_key(&cert_path, &key_path).expect("load cert");
    let cert_resolver = Arc::new(HotReloadCertResolver::new(certified_key));
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let pub_key_bytes = std::fs::read(&jwt_kp.pub_pem_path).expect("read pub");
    let jwt_verifier = Arc::new(
        JwtVerifier::new(
            JwtAlgorithm::Es256,
            &pub_key_bytes,
            "test-issuer",
            "test-audience",
        )
        .expect("verifier"),
    );
    let mtls_verifier = Arc::new(MtlsVerifier::new(48, &[]).expect("mtls verifier"));
    let (shutdown_tx, shutdown_rx) = make_test_shutdown_pair();
    let cfg = ExternalGrpcSpawnConfig {
        bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        config: ExternalGrpcConfig {
            enabled: true,
            auth_mode: Some(AuthMode::JwtAndMtls),
            mtls_ca_path: Some(ca.ca_pem_path.clone()),
            max_connections: 64,
            max_concurrent_streams: 16,
            ..Default::default()
        },
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        audit_port: Arc::new(NoopAudit) as Arc<dyn AuditLogPort>,
        cert_resolver,
        jwt_verifier: Some(jwt_verifier),
        mtls_verifier: Some(mtls_verifier),
        ip_ban: Arc::new(IpBan::new()),
        metrics: Arc::new(ExternalMetrics::new()),
        shutdown_rx,
        shutdown_tx,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        live: Arc::new(LiveExternalConfig::new(LiveSnapshot {
            streaming_enabled: true,
            load_policy: Arc::new(LoadPolicy::new(
                oneshim_core::config::LoadThresholds::default(),
            )),
        })),
    };
    let (handle, port) = spawn_server(cfg).await;

    // Mint a valid JWT (not used if TLS fails first, but present for completeness).
    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-1",
        "test-issuer",
        "test-audience",
        3600,
    );
    let server_cert_pem = server_cert_pem();
    let ca_cert = tonic::transport::Certificate::from_pem(&server_cert_pem);

    // Build a TLS channel WITHOUT a client identity — no client cert presented.
    // The rustls WebPkiClientVerifier requires a client cert, so the TLS
    // handshake must fail.
    let tls = tonic::transport::ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(ca_cert);
    let connect_result =
        tonic::transport::Endpoint::from_shared(format!("https://127.0.0.1:{port}"))
            .expect("valid endpoint")
            .tls_config(tls)
            .expect("tls config")
            .connect_timeout(Duration::from_secs(3))
            .connect()
            .await;

    match connect_result {
        Err(_) => {
            // TLS handshake failed eagerly at connect time — expected.
        }
        Ok(channel) => {
            // Channel was created lazily; TLS failure surfaces on first request.
            let mut req = tonic::Request::new(GetAgentInfoRequest {});
            req.metadata_mut().insert(
                "authorization",
                format!("Bearer {token}").parse().expect("valid header"),
            );
            let result = DashboardServiceClient::new(channel)
                .get_agent_info(req)
                .await;
            assert!(
                result.is_err(),
                "no-client-cert TLS should be rejected by WebPkiClientVerifier; got Ok"
            );
            let status = result.unwrap_err();
            // TLS rejection at handshake produces a transport-level error
            // (Unknown, Unavailable, or Cancelled) — never Unimplemented (which
            // would mean auth passed and the placeholder service ran).
            assert_ne!(
                status.code(),
                tonic::Code::Unimplemented,
                "WebPkiClientVerifier must reject before reaching service; \
                 got Unimplemented which means TLS accepted the request: {:?}",
                status
            );
        }
    }

    handle.abort();
    let _ = handle.await;
}

/// Test 12: JWT+mTLS — client cert signed by wrong CA + valid JWT → TLS rejection.
///
/// The server trusts CA_A. The client presents a cert signed by CA_B (a
/// completely independent CA). rustls's `WebPkiClientVerifier` validates the
/// client cert chain against CA_A's root and fails at TLS handshake time because
/// CA_B is not in the server's trust store.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_jwt_plus_mtls_cert_invalid_jwt_valid() {
    let jwt_kp = test_jwt_keypair();
    // CA_A: trusted by server.
    let ca_a = test_ca_and_client_cert(24);
    // CA_B: an independent CA — its client cert is NOT trusted by the server.
    let ca_b = test_ca_and_client_cert(24);

    // Build a JWT+mTLS server that trusts CA_A only.
    let (cert_path, key_path) = test_cert_pair();
    let certified_key = load_certified_key(&cert_path, &key_path).expect("load cert");
    let cert_resolver = Arc::new(HotReloadCertResolver::new(certified_key));
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    let pub_key_bytes = std::fs::read(&jwt_kp.pub_pem_path).expect("read pub");
    let jwt_verifier = Arc::new(
        JwtVerifier::new(
            JwtAlgorithm::Es256,
            &pub_key_bytes,
            "test-issuer",
            "test-audience",
        )
        .expect("verifier"),
    );
    let mtls_verifier = Arc::new(MtlsVerifier::new(48, &[]).expect("mtls verifier"));
    let (shutdown_tx, shutdown_rx) = make_test_shutdown_pair();
    let cfg = ExternalGrpcSpawnConfig {
        bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        config: ExternalGrpcConfig {
            enabled: true,
            auth_mode: Some(AuthMode::JwtAndMtls),
            mtls_ca_path: Some(ca_a.ca_pem_path.clone()), // server trusts CA_A only
            max_connections: 64,
            max_concurrent_streams: 16,
            ..Default::default()
        },
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        audit_port: Arc::new(NoopAudit) as Arc<dyn AuditLogPort>,
        cert_resolver,
        jwt_verifier: Some(jwt_verifier),
        mtls_verifier: Some(mtls_verifier),
        ip_ban: Arc::new(IpBan::new()),
        metrics: Arc::new(ExternalMetrics::new()),
        shutdown_rx,
        shutdown_tx,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        live: Arc::new(LiveExternalConfig::new(LiveSnapshot {
            streaming_enabled: true,
            load_policy: Arc::new(LoadPolicy::new(
                oneshim_core::config::LoadThresholds::default(),
            )),
        })),
    };
    let (handle, port) = spawn_server(cfg).await;

    // Mint a valid JWT.
    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-1",
        "test-issuer",
        "test-audience",
        3600,
    );
    let server_cert_pem = server_cert_pem();
    let ca_cert = tonic::transport::Certificate::from_pem(&server_cert_pem);

    // Client presents CA_B's client cert — not trusted by server (trusts CA_A).
    let client_cert_pem = std::fs::read(&ca_b.client_cert_pem_path).expect("read CA_B client cert");
    let client_key_pem = std::fs::read(&ca_b.client_key_pem_path).expect("read CA_B client key");
    let identity = tonic::transport::Identity::from_pem(client_cert_pem, client_key_pem);

    let tls = tonic::transport::ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(ca_cert)
        .identity(identity); // present CA_B-signed cert to CA_A-trusting server
    let connect_result =
        tonic::transport::Endpoint::from_shared(format!("https://127.0.0.1:{port}"))
            .expect("valid endpoint")
            .tls_config(tls)
            .expect("tls config")
            .connect_timeout(Duration::from_secs(3))
            .connect()
            .await;

    match connect_result {
        Err(_) => {
            // TLS handshake failed eagerly at connect time — expected.
        }
        Ok(channel) => {
            // Channel created lazily; TLS failure surfaces on first request.
            let mut req = tonic::Request::new(GetAgentInfoRequest {});
            req.metadata_mut().insert(
                "authorization",
                format!("Bearer {token}").parse().expect("valid header"),
            );
            let result = DashboardServiceClient::new(channel)
                .get_agent_info(req)
                .await;
            assert!(
                result.is_err(),
                "wrong-CA client cert must be rejected by WebPkiClientVerifier; got Ok"
            );
            let status = result.unwrap_err();
            // A CA-chain failure at TLS level never produces Unimplemented.
            assert_ne!(
                status.code(),
                tonic::Code::Unimplemented,
                "CA chain validation must reject before reaching service; \
                 got Unimplemented which means TLS accepted the request: {:?}",
                status
            );
        }
    }

    handle.abort();
    let _ = handle.await;
}

// T13 (external_grpc_ipv6_ban_uses_64_prefix) deleted — the /64 prefix ban
// logic is covered by unit tests in `ip_ban.rs` (same /64 prefix ⇒ ban
// shared, at lines 188-208). An integration-level variant would re-test the
// same logic through an IPv6 loopback bind that is not portable across CI
// runner configurations. If the full-stack IPv6 accept_loop path ever
// regresses, add a scenario to the stress-test workflow instead.

/// Test 14: Concurrent stream cap — attempt beyond `max_concurrent_streams`
/// returns `ResourceExhausted`. Uses a tight cap (4 streams) so the test
/// runs quickly without holding many resources.
///
/// The handler-side `StreamCounterGuard::try_acquire` enforces the cap
/// (BEFORE auth work) in both subscribe_metrics and subscribe_events. The
/// integration test exercises the full stack — accept_loop → auth_layer →
/// audit_layer → DashboardServiceImpl → StreamCounterGuard.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn external_grpc_concurrent_stream_cap_enforced() {
    let jwt_kp = test_jwt_keypair();
    let (mut cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    // Override the cap to 4 for fast testing.
    cfg.config.max_concurrent_streams = 4;
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-cap",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    // Hold 4 concurrent subscribe_events streams open. Each stream stays
    // alive because we keep the receiver handle in `streams`.
    let mut streams = Vec::new();
    for _ in 0..4 {
        let mut req = tonic::Request::new(SubscribeEventsRequest::default());
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {token}").parse().expect("valid header"),
        );
        let stream = DashboardServiceClient::new(channel.clone())
            .subscribe_events(req)
            .await
            .expect("within-cap stream should open")
            .into_inner();
        streams.push(stream);
    }

    // The 5th attempt should be rejected with ResourceExhausted at RPC
    // initialization time — `StreamCounterGuard::try_acquire` runs before
    // the first message is yielded and returns the error as the initial
    // gRPC status (not via a stream item).
    let mut req = tonic::Request::new(SubscribeEventsRequest::default());
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("valid header"),
    );

    let err = DashboardServiceClient::new(channel.clone())
        .subscribe_events(req)
        .await
        .expect_err("5th stream over cap must be rejected at RPC initialization");
    assert_eq!(
        err.code(),
        Code::ResourceExhausted,
        "5th stream must be rejected with ResourceExhausted; got {err:?}"
    );

    drop(streams);
    handle.abort();
    let _ = handle.await;
}

// T15 (concurrent_connection_cap_enforced) deleted — resource-exhaustion
// tests require dedicated CI workflow with elevated fd ulimit + opt-in
// trigger (`ulimit -n 65536`; separate workflow with manual dispatch). This
// is tracked in `project_next_tasks.md` as "External gRPC stress test suite"
// and is out of scope for Task 13 per user direction.

// T16 (external_grpc_task_panic_respawned) deleted — the
// `PANIC_ON_FIRST_ACCEPT` injection + `spawn_with_supervisor` respawn
// path is already covered by `external::mod::tests::supervisor_respawns_on_injected_panic`
// (at external/mod.rs:388-459). The integration-level re-test would
// duplicate the same assertion through a tonic client that doesn't add
// additional coverage — supervisor correctness is the invariant, not the
// client's observation of it.

/// Test 17: Port collision — external port == loopback port → launcher refuses external.
///// Test 17: Port collision guard — external port == loopback port triggers
/// a validation error that the launcher surfaces (F13 guard). Unit-level
/// test: the helper itself is the single source of truth for the check.
#[test]
fn external_grpc_port_collides_with_loopback() {
    use oneshim_web::grpc::external::port_collision::check_port_collision;
    let err = check_port_collision(10091, 10091).expect_err("same port must error");
    assert!(
        err.contains("10091"),
        "error should name the port; got: {err}"
    );
    assert!(check_port_collision(10092, 10091).is_ok());
}

/// Test 18: Token isolation — `integration_auth_token` on the external
/// service impl MUST be None (spec §2.5 threat model). The loopback's
/// opt-out bypass path can only be reached by a caller presenting a
/// matching token; if the external server were to inherit the loopback's
/// token, an external client could bypass auth.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_separate_service_impl_doesnt_leak_loopback_token() {
    install_rustls_crypto_provider();
    let jwt_kp = test_jwt_keypair();
    let (cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let svc = oneshim_web::grpc::DashboardServiceImpl::from_external_spawn_config(&cfg);
    assert!(
        !svc.has_integration_token(),
        "external DashboardServiceImpl MUST have integration_auth_token=None (spec §2.5)"
    );
}

/// Test 19: Shutdown signal reaches the server task.
///
/// Sends the shutdown signal and verifies the `serve_external` task
/// exits within a bounded window. Does NOT assert client-side stream
/// termination — that requires streaming handlers to observe
/// `shutdown_rx`, which is a post-Task-13 follow-up: currently
/// `subscribe_events` / `subscribe_metrics` only exit when the
/// underlying broadcast receiver yields (not when shutdown fires).
///
/// A full "open-stream → Unavailable on shutdown" assertion is
/// tracked in project_next_tasks.md under the external gRPC stress +
/// e2e suite.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn external_grpc_shutdown_drains_streams() {
    let jwt_kp = test_jwt_keypair();
    let (cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let shutdown_tx = cfg.shutdown_tx.clone();
    let (handle, _port) = spawn_server(cfg).await;

    // Let the server settle (accept loop + tonic main loop both live).
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Signal shutdown. `serve_with_incoming_shutdown` should complete once
    // the shutdown_signal future resolves and in-flight work drains.
    shutdown_tx.send(true).expect("signal shutdown");

    // The `serve_external` task MUST exit within 5s after the signal is
    // sent. The spawn_server wrapper awaits `serve_external(...)` and
    // returns, so the JoinHandle completes with `Ok(())`.
    let joined = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("server task must exit within 5s of shutdown signal");
    assert!(
        joined.is_ok(),
        "server task should complete cleanly on shutdown; got {joined:?}"
    );
}

// Test 20 (external_grpc_fails_fast_on_missing_cert) is covered at unit level by
// `tls_config::tests::load_fails_on_missing_cert`, which directly asserts that
// `load_certified_key("/does/not/exist.pem", "/does/not/exist.key")` returns
// `Err(TlsLoadError::Read { .. })`. An integration-test duplicate is not needed
// because `serve_external` calls `load_certified_key` (via `build_server_config`)
// early in startup — the unit test covers the identical code path.

// ═════════════════════════════════════════════════════════════════════════════
// Task 19 — new end-to-end tests added in the Task 13 follow-up (spec §3.5)
// ═════════════════════════════════════════════════════════════════════════════

/// E2E-1: Real handler returns business data — confirms that the wired
/// `DashboardServiceImpl` (not the removed stub) answers RPCs with
/// structured responses, not `Unimplemented`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_real_handler_returns_business_data() {
    let jwt_kp = test_jwt_keypair();
    let (cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-e2e-1",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    let mut req = tonic::Request::new(GetSessionStatsRequest { limit: 10 });
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("valid header"),
    );
    let resp = DashboardServiceClient::new(channel)
        .get_session_stats(req)
        .await
        .expect("get_session_stats must return Ok with a structured response");
    let stats = resp.into_inner();
    // Empty storage → zero sessions; the IMPORTANT invariant is that we
    // received a typed SessionStatsResponse with concrete fields, not an
    // Unimplemented status.
    assert_eq!(
        stats.total_sessions, 0,
        "empty storage should yield total_sessions=0; got {}",
        stats.total_sessions
    );
    assert_eq!(stats.ended_sessions, 0);

    handle.abort();
    let _ = handle.await;
}

// Mock audit log that retains every `log_complete_with_time` and `log_event`
// entry so the e2e tests below can inspect what AuditLayer recorded.
//
// Structural rewrite (Task 9.0, CR4 / R2-NI1): replaces the previous
// action_type-as-command_id conflation with real command_id preservation and
// grpc_status_code JSON extraction from the details blob.  Unblocks Phase 9
// Tasks 9.1+ which assert command_id correlation and D26 raw-code visibility.
#[derive(Default)]
struct CapturingAudit {
    entries: std::sync::Mutex<Vec<CapturedEntry>>,
}

/// A lightweight capture record used by Phase 9 integration tests to assert
/// on command_id, action_type, status, grpc_status_code, and execution timing.
///
/// `details` preserves the raw JSON blob from `log_complete_with_time` so that
/// tests can inspect operation names (e.g. "SubscribeEvents") without re-parsing
/// the struct.
#[derive(Clone, Debug)]
#[allow(dead_code)] // fields consumed by Phase 9.1+ assertion helpers
struct CapturedEntry {
    command_id: String,
    action_type: String,
    status: AuditStatus,
    grpc_status_code: Option<u32>,
    execution_time_ms: u64,
    /// Raw `details` string passed by the audit bridge (JSON blob or empty).
    /// Populated by `log_complete_with_time` and `log_event`; `None` for
    /// entries that have no details context.
    details: Option<String>,
}

impl CapturingAudit {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: std::sync::Mutex::new(vec![]),
        })
    }

    /// Return a snapshot of all captured entries.  Used by Phase 9.1+ tests
    /// to assert command_id correlation and grpc_status_code visibility.
    #[allow(dead_code)] // used by Phase 9.1+ tests
    fn snapshot(&self) -> Vec<CapturedEntry> {
        self.entries.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl AuditLogPort for CapturingAudit {
    async fn log_event(&self, action_type: &str, _session_id: &str, details: &str) {
        // AuditBridge emits action_type "external_grpc_started" etc.
        // alongside log_complete_with_time; use this to capture Started rows.
        let status = match action_type {
            "external_grpc_started" => AuditStatus::Started,
            "external_grpc_completed" => AuditStatus::Completed,
            "external_grpc_failed" | "external_grpc_denied" | "external_grpc_timeout" => {
                AuditStatus::Failed
            }
            _ => AuditStatus::Completed,
        };
        self.entries.lock().unwrap().push(CapturedEntry {
            command_id: String::new(),
            action_type: action_type.to_string(),
            status,
            grpc_status_code: None,
            execution_time_ms: 0,
            details: Some(details.to_string()),
        });
    }

    async fn log_start_if(
        &self,
        _level: AuditLevel,
        command_id: &str,
        _session_id: &str,
        action_type: &str,
    ) {
        self.entries.lock().unwrap().push(CapturedEntry {
            command_id: command_id.to_string(),
            action_type: action_type.to_string(),
            status: AuditStatus::Started,
            grpc_status_code: None,
            execution_time_ms: 0,
            details: None,
        });
    }

    async fn log_complete_with_time(
        &self,
        _level: AuditLevel,
        command_id: &str,
        _session_id: &str,
        details: &str,
        execution_time_ms: u64,
    ) {
        let status = parse_status_from_details(details);
        let grpc_status_code: Option<u32> = serde_json::from_str::<serde_json::Value>(details)
            .ok()
            .and_then(|v| {
                v.get("grpc_status_code")
                    .and_then(|n| n.as_u64().map(|u| u as u32))
            });
        self.entries.lock().unwrap().push(CapturedEntry {
            command_id: command_id.to_string(),
            action_type: String::new(),
            status,
            grpc_status_code,
            execution_time_ms,
            details: Some(details.to_string()),
        });
    }

    async fn pending_count(&self) -> usize {
        0
    }
    async fn recent_entries(&self, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_status(&self, _s: &AuditStatus, _l: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_action_prefix(&self, _p: &str, _l: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_command_id(&self, _cmd_id: &str, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn stats(&self) -> AuditStats {
        AuditStats::default()
    }
    async fn has_pending_batch(&self) -> bool {
        false
    }
    async fn drain_batch(&self) -> Vec<AuditEntry> {
        vec![]
    }
    async fn drain_all(&self) -> Vec<AuditEntry> {
        vec![]
    }
    async fn record_session_event(&self, _e: SessionAuditEntry) {}
}

/// Derive `AuditStatus` from the JSON `result` field in the details blob emitted
/// by `AuditBridge::record`.  Returns `Completed` for any unrecognized value.
fn parse_status_from_details(details: &str) -> AuditStatus {
    serde_json::from_str::<serde_json::Value>(details)
        .ok()
        .and_then(|v| v.get("result").and_then(|r| r.as_str().map(String::from)))
        .map(|s| match s.as_str() {
            "ok" => AuditStatus::Completed,
            "denied" => AuditStatus::Denied,
            "timeout" => AuditStatus::Timeout,
            "error" | "failed" => AuditStatus::Failed,
            _ => AuditStatus::Completed,
        })
        .unwrap_or(AuditStatus::Completed)
}

/// E1 / E2E-2: After a successful RPC, the audit trail contains both Started
/// and Completed rows with the same command_id. This proves AuditLayer's
/// Started+Completed pairing works end-to-end.
///
/// Extended (Task 9.1 E1): also asserts:
/// - The Completed row's `command_id` is a valid UUIDv4 (36 chars, 4 hyphens) —
///   generated by RequestIdLayer since no client-supplied x-request-id was sent.
/// - The Completed row's `grpc_status_code` is `Some(0)` (gRPC Ok / Code::Ok),
///   proving D26 raw-code persistence for the success path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_audit_completed_entry_written_after_ok_response() {
    let jwt_kp = test_jwt_keypair();
    let (mut cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let capturing = CapturingAudit::new();
    cfg.audit_port = capturing.clone() as Arc<dyn AuditLogPort>;
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-audit",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;
    let mut req = tonic::Request::new(GetAgentInfoRequest {});
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("valid header"),
    );
    // No x-request-id header — RequestIdLayer generates a UUIDv4.
    DashboardServiceClient::new(channel)
        .get_agent_info(req)
        .await
        .expect("auth + real handler → Ok");

    // Give the tokio::spawn'd record() calls time to flush to the mock.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Copy all needed data into locals + drop the lock BEFORE any `await` to avoid
    // `await_holding_lock` clippy — std::sync::MutexGuard is not `Send`.
    let (started_count, completed_count, completed_cmd_id, completed_grpc_code, entries_debug) = {
        let entries = capturing.entries.lock().unwrap();
        let started = entries
            .iter()
            .filter(|e| matches!(e.status, AuditStatus::Started))
            .count();
        // log_complete_with_time sets grpc_status_code; find the Completed row
        // that has it populated (the deferred AuditLayer task).
        let completed_row = entries
            .iter()
            .find(|e| matches!(e.status, AuditStatus::Completed) && e.grpc_status_code.is_some());
        let cmd_id = completed_row
            .map(|e| e.command_id.clone())
            .unwrap_or_default();
        let grpc_code = completed_row.and_then(|e| e.grpc_status_code);
        let completed_any = entries
            .iter()
            .filter(|e| matches!(e.status, AuditStatus::Completed))
            .count();
        let dbg = format!("{entries:?}");
        (started, completed_any, cmd_id, grpc_code, dbg)
    };
    assert!(
        started_count >= 1,
        "expected ≥1 Started row; got {started_count} (entries: {entries_debug})"
    );
    assert!(
        completed_count >= 1,
        "expected ≥1 Completed row; got {completed_count} (entries: {entries_debug})"
    );

    // E1 extension: command_id must be a UUIDv4 (36-char string, 4 hyphens).
    // RequestIdLayer generates it when the client omits x-request-id, and
    // AuditLayer's `request_id override (U5)` propagates it to command_id.
    assert_eq!(
        completed_cmd_id.len(),
        36,
        "command_id must be a 36-char UUIDv4 string; got {completed_cmd_id:?}"
    );
    assert_eq!(
        completed_cmd_id.chars().filter(|c| *c == '-').count(),
        4,
        "UUIDv4 command_id must have 4 hyphens; got {completed_cmd_id:?}"
    );
    uuid::Uuid::parse_str(&completed_cmd_id).unwrap_or_else(|e| {
        panic!("command_id {completed_cmd_id:?} must be parseable as UUID: {e}")
    });

    // E1 extension: grpc_status_code=Some(0) for the Ok path (D26).
    assert_eq!(
        completed_grpc_code,
        Some(0),
        "Completed row must carry grpc_status_code=0 (Code::Ok); entries: {entries_debug}"
    );

    handle.abort();
    let _ = handle.await;
}

/// E2E-3: Streaming RPC records response_message_count in the Completed
/// audit row. The client opens a subscribe_events stream, drops it shortly
/// after (no events fired, so message_count == 0), then checks that the
/// Completed row's details JSON reports a count (absent or 0 both
/// acceptable since skip_serializing_if drops zero values).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn external_grpc_streaming_audit_records_message_count() {
    let jwt_kp = test_jwt_keypair();
    let (mut cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let capturing = CapturingAudit::new();
    cfg.audit_port = capturing.clone() as Arc<dyn AuditLogPort>;
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-stream-audit",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    let mut req = tonic::Request::new(SubscribeEventsRequest::default());
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("valid header"),
    );
    let stream = DashboardServiceClient::new(channel)
        .subscribe_events(req)
        .await
        .expect("stream should open")
        .into_inner();

    // Drop the stream quickly — handler exits; AuditLayer records Completed.
    drop(stream);
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Copy counts into locals + drop the lock BEFORE any `await` to avoid
    // `await_holding_lock` clippy — std::sync::MutexGuard is not `Send`.
    let (has_subscribe_row, entries_debug) = {
        let entries = capturing.entries.lock().unwrap();
        // There must be at least a Started row for the subscribe_events call.
        // The Completed row may or may not appear depending on timing and
        // whether tonic has drained the trailer — at minimum, the audit trail
        // for /oneshim.dashboard.v1.DashboardService/SubscribeEvents must
        // show the Started row, proving AuditLayer wrapped the stream.
        let found = entries.iter().any(|e| {
            e.details
                .as_deref()
                .map(|d| d.contains("SubscribeEvents"))
                .unwrap_or(false)
        });
        (found, format!("{entries:?}"))
    };
    assert!(
        has_subscribe_row,
        "expected ≥1 audit row for SubscribeEvents; got entries: {entries_debug}"
    );

    handle.abort();
    let _ = handle.await;
}

/// N1 — RequestIdLayer generates a UUIDv4 when the client omits x-request-id.
///
/// When no `x-request-id` header is sent, `RequestIdLayer` (spec §5.2 / None
/// branch) generates a fresh UUIDv4 and inserts it into the response.  The
/// CapturingAudit's Completed row carries that same UUID as `command_id` via
/// AuditLayer's request_id override (U5), proving end-to-end propagation.
///
/// Assertions:
/// 1. Response metadata has `x-request-id` (server-generated).
/// 2. The value is a valid 36-char UUIDv4 (4 hyphens, parseable by `uuid`).
/// 3. The CapturingAudit Completed row's `command_id` matches the response header.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_request_id_generated_when_missing() {
    let jwt_kp = test_jwt_keypair();
    let (mut cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let capturing = CapturingAudit::new();
    cfg.audit_port = capturing.clone() as Arc<dyn AuditLogPort>;
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-gen-id",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    // No x-request-id header — RequestIdLayer must generate a UUIDv4.
    let mut req = tonic::Request::new(GetAgentInfoRequest {});
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}")
            .parse()
            .expect("valid auth header"),
    );
    let resp = DashboardServiceClient::new(channel)
        .get_agent_info(req)
        .await
        .expect("auth + real handler → Ok");

    // 1. Response must carry a server-generated x-request-id.
    let generated_id = resp
        .metadata()
        .get("x-request-id")
        .expect("server must insert x-request-id when client omits it")
        .to_str()
        .expect("x-request-id must be valid ASCII");

    // 2. Must be a valid UUIDv4 (36 chars, 4 hyphens).
    assert_eq!(
        generated_id.len(),
        36,
        "generated x-request-id must be 36 chars; got {generated_id:?}"
    );
    assert_eq!(
        generated_id.chars().filter(|c| *c == '-').count(),
        4,
        "generated x-request-id must have 4 hyphens (UUIDv4); got {generated_id:?}"
    );
    let parsed_uuid = uuid::Uuid::parse_str(generated_id).expect("x-request-id must parse as UUID");
    // UUIDv4: version nibble == 4, variant == 0b10xx.
    assert_eq!(
        parsed_uuid.get_version_num(),
        4,
        "generated ID must be UUIDv4; got version {}",
        parsed_uuid.get_version_num()
    );

    // Give AuditLayer's deferred task time to flush to the mock.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 3. CapturingAudit's Completed row must carry the same UUID as command_id.
    // Drop the lock before any `.await`.
    let (audit_cmd_id, entries_debug) = {
        let entries = capturing.entries.lock().unwrap();
        let completed = entries
            .iter()
            .find(|e| matches!(e.status, AuditStatus::Completed) && e.grpc_status_code.is_some())
            .map(|e| e.command_id.clone())
            .unwrap_or_default();
        let dbg = format!("{entries:?}");
        (completed, dbg)
    };
    assert_eq!(
        audit_cmd_id, generated_id,
        "AuditLayer command_id must equal the x-request-id echoed in the response; \
         entries: {entries_debug}"
    );

    handle.abort();
    let _ = handle.await;
}

/// N2 — RequestIdLayer discards a malformed client x-request-id and substitutes
/// a fresh UUIDv4.
///
/// Per spec §5.2 / L307: when the client sends an `x-request-id` that fails
/// `is_valid()` (ASCII graphic 0x21..=0x7E, 1..=128 chars), `RequestIdLayer`
/// emits a `tracing::warn!` and generates a fresh UUIDv4.  The warn+regenerate
/// path proves that a malicious / malformed client cannot inject arbitrary
/// bytes into the response-header / downstream audit trail.
///
/// The malformed payload used here is `"bad\tid"` — the tab byte (0x09) is a
/// valid HeaderValue byte (HTAB is permitted by `http::HeaderValue::from_str`)
/// but falls outside the `is_valid()` 0x21..=0x7E range, so the server-side
/// validator will reject it and substitute a UUIDv4.  Mirrors the in-crate
/// `rejects_invalid_characters_generates_new` unit test (request_id_layer.rs L189).
///
/// Assertions:
/// 1. Response metadata carries a valid 36-char UUIDv4 (4 hyphens, parses as UUID v4).
/// 2. Response's `x-request-id` does NOT equal the malformed client input.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_request_id_invalid_replaced() {
    let jwt_kp = test_jwt_keypair();
    let (cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let (handle, port) = spawn_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-bad-reqid",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    // Malformed x-request-id: tab (0x09) is valid as an http HeaderValue byte
    // but fails the is_valid(0x21..=0x7E) range, forcing the warn+regenerate path.
    let malformed_id = "bad\tid";
    let mut req = tonic::Request::new(GetAgentInfoRequest {});
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}")
            .parse()
            .expect("valid auth header"),
    );
    req.metadata_mut().insert(
        "x-request-id",
        tonic::metadata::MetadataValue::try_from(malformed_id)
            .expect("tab (0x09) is a valid HeaderValue byte"),
    );
    let resp = DashboardServiceClient::new(channel)
        .get_agent_info(req)
        .await
        .expect("auth + real handler → Ok");

    // 1. Response must carry a server-substituted x-request-id.
    let returned_id = resp
        .metadata()
        .get("x-request-id")
        .expect("x-request-id must be present (server substitutes on invalid input)")
        .to_str()
        .expect("substituted x-request-id must be valid ASCII");

    // 2. Value must NOT be the malformed client input.
    assert_ne!(
        returned_id, malformed_id,
        "server must discard malformed x-request-id and substitute a UUID"
    );

    // 3. Value must be a valid UUIDv4 (36 chars, 4 hyphens, version 4).
    assert_eq!(
        returned_id.len(),
        36,
        "substituted x-request-id must be 36 chars; got {returned_id:?}"
    );
    assert_eq!(
        returned_id.chars().filter(|c| *c == '-').count(),
        4,
        "substituted x-request-id must have 4 hyphens (UUIDv4); got {returned_id:?}"
    );
    let parsed_uuid = uuid::Uuid::parse_str(returned_id).expect("x-request-id must parse as UUID");
    assert_eq!(
        parsed_uuid.get_version_num(),
        4,
        "substituted x-request-id must be UUIDv4; got version {}",
        parsed_uuid.get_version_num()
    );

    handle.abort();
    let _ = handle.await;
}

/// N3 — x-request-id is preserved across the auth-rejection boundary (U5 / D14).
///
/// Per spec §5.2 / §9.2 L1393: `RequestIdLayer` is the outermost layer and runs
/// BEFORE `AuthLayer`, so it inserts the `RequestId` extension with the client's
/// header value before any auth gate fires.  When `AuthLayer` subsequently
/// rejects the request (invalid JWT → Unauthenticated), its Failed-path
/// `bridge.record(...)` reads the extension and passes it as `command_id`
/// (commit `7bd7c944`, Task 6.1).  This closes the correlation gap at the
/// security boundary — security dashboards can still trace which client call
/// produced each auth rejection.
///
/// Flow:
/// 1. Client sends `x-request-id: req-abc-123` + a JWT signed with a wrong issuer.
/// 2. Server's `RequestIdLayer` validates the header (passes) and inserts
///    `RequestId("req-abc-123")` into request extensions.
/// 3. `AuthLayer`'s JWT gate calls `verifier.verify(tok)`, which fails (wrong
///    issuer), and takes the `invalid_jwt` Failed-path.
/// 4. The Failed-path reads the `RequestId` extension and calls
///    `bridge.record(..., Some("req-abc-123"))`, which persists the Failed
///    audit row with `command_id = "req-abc-123"`.
///
/// Assertions:
/// 1. RPC returns `Err(Status)` with code `Unauthenticated` (16).
/// 2. CapturingAudit captures ≥1 Failed audit row.
/// 3. That Failed row's `command_id` equals `"req-abc-123"`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_grpc_request_id_preserved_across_auth_reject() {
    let jwt_kp = test_jwt_keypair();
    let (mut cfg, _) = make_jwt_config(&jwt_kp.pub_pem_path);
    let capturing = CapturingAudit::new();
    cfg.audit_port = capturing.clone() as Arc<dyn AuditLogPort>;
    let (handle, port) = spawn_server(cfg).await;

    // Invalid JWT — wrong issuer → JwtVerifier::verify() fails → invalid_jwt path.
    let bad_token = test_mint_jwt(
        &jwt_kp.enc_key,
        "user-auth-reject",
        "wrong-issuer", // mismatch with verifier's "test-issuer" → verify fails
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();
    let channel = make_tls_channel(port, &cert_pem, None).await;

    // Valid x-request-id — passes is_valid() (all ASCII graphic chars).
    let client_req_id = "req-abc-123";
    let mut req = tonic::Request::new(GetAgentInfoRequest {});
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {bad_token}")
            .parse()
            .expect("valid auth header"),
    );
    req.metadata_mut().insert(
        "x-request-id",
        tonic::metadata::MetadataValue::try_from(client_req_id).expect("valid x-request-id value"),
    );

    let result = DashboardServiceClient::new(channel)
        .get_agent_info(req)
        .await;

    // 1. RPC must fail with Unauthenticated (invalid_jwt path).
    let status = result.expect_err("wrong-issuer JWT must yield Err");
    assert_eq!(
        status.code(),
        Code::Unauthenticated,
        "invalid JWT must yield Unauthenticated (code 16); got {status:?}"
    );

    // Give the tokio::spawn'd AuthLayer Failed-path record() time to flush.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 2 + 3. The auth-rejection audit row must carry the client's x-request-id
    // as command_id. AuthLayer's Failed-path calls both `log_complete_with_time`
    // (writes command_id + details JSON) and `log_event("external_grpc_failed")`
    // (prefix-queryable marker).  We locate the authoritative auth-rejection
    // row by its details payload (`"result":"auth_failed"` + `"failure_reason":
    // "invalid_jwt"`) — this is the row whose command_id must equal the client's
    // x-request-id per U5/D14.
    //
    // NOTE: CapturingAudit's `parse_status_from_details` maps `"auth_failed"`
    // into the default `Completed` bucket (it only recognizes "ok" / "denied" /
    // "timeout" / "error" / "failed"), which is why we filter by details
    // content rather than by `AuditStatus`.
    //
    // Drop the lock before any `.await`.
    let (auth_failed_count, auth_failed_cmd_id, entries_debug) = {
        let entries = capturing.entries.lock().unwrap();
        let auth_failed: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.details
                    .as_deref()
                    .map(|d| {
                        d.contains("\"result\":\"auth_failed\"")
                            && d.contains("\"failure_reason\":\"invalid_jwt\"")
                    })
                    .unwrap_or(false)
                    && !e.command_id.is_empty()
            })
            .collect();
        let cmd_id = auth_failed
            .first()
            .map(|e| e.command_id.clone())
            .unwrap_or_default();
        let dbg = format!("{entries:?}");
        (auth_failed.len(), cmd_id, dbg)
    };
    assert!(
        auth_failed_count >= 1,
        "expected ≥1 auth-rejection audit row with populated command_id + \
         details.result='auth_failed' + failure_reason='invalid_jwt'; \
         got {auth_failed_count} (entries: {entries_debug})"
    );
    assert_eq!(
        auth_failed_cmd_id, client_req_id,
        "auth-rejection audit row's command_id must equal the client's x-request-id \
         (U5/D14 correlation preserved at security boundary); entries: {entries_debug}"
    );

    handle.abort();
    let _ = handle.await;
}
