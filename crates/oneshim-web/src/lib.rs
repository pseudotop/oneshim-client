//! # oneshim-web
//!
//! ## Hexagonal Architecture — ADR-001 §7 (Port Location Rules)
//!
//! ### Violation 1 — `oneshim-automation` concrete types in AppState — RESOLVED
//!
//! **Status**: Migration steps 1-6 completed.
//!   - `AuditLogPort` defined in `oneshim-core/src/ports/audit_log.rs`
//!   - `AutomationPort` defined in `oneshim-core/src/ports/automation.rs`
//!   - `GuiInteractionError` moved to `oneshim-core::error`
//!   - `AuditEntry`, `AuditStatus`, `AuditLevel`, `AuditStats` in `oneshim-core::models::audit`
//!   - `AppState` uses `Arc<dyn AuditLogPort>` and `Arc<dyn AutomationPort>`
//!   - `AuditLogAdapter` in `oneshim-automation::audit` bridges `AuditLogger` to the port
//!
//! **Remaining**: `oneshim-automation` moved to `[dev-dependencies]` — only used
//!   for test-only `AutomationController` construction in `automation_gui::tests`.
//!
//! ### Violation 2 — `oneshim-storage` concrete types — RESOLVED
//!
//! **Status**: All 4 migration steps completed.
//!   - 14 row types promoted to `oneshim-core::models::storage_records`
//!   - `WebStorage` trait moved to `oneshim-core/src/ports/web_storage.rs`
//!   - `impl WebStorage for SqliteStorage` moved to `oneshim-storage::sqlite::web_storage_impl`
//!   - `oneshim-storage` moved to `[dev-dependencies]` (test-only `SqliteStorage::open_in_memory`)

pub mod embedded;
pub mod error;
pub mod handlers;
pub mod routes;
pub mod services;
pub mod storage_port;
pub mod update_control;

use crate::storage_port::WebStorage;
use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::response::Response;
use axum::Router;
use oneshim_core::config::{CredentialBackendKind, WebConfig};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::ports::audit_log::AuditLogPort;
use oneshim_core::ports::automation::AutomationPort;
use oneshim_core::ports::secret_store::{SecretStore, SecretStoreSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, oneshot, watch};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

pub use oneshim_api_contracts::stream::{
    AiRuntimeStatus, FrameUpdate, IdleUpdate, MetricsUpdate, RealtimeEvent,
};

pub use oneshim_core::config::WebConfig as CoreWebConfig;

const EVENT_CHANNEL_CAPACITY: usize = 256;

const MAX_PORT_ATTEMPTS: u16 = 10;
const INTEGRATION_TOKEN_HEADER: &str = "x-oneshim-integration-token";

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn WebStorage>,
    pub frames_dir: Option<std::path::PathBuf>,
    pub event_tx: broadcast::Sender<RealtimeEvent>,
    pub config_manager: Option<ConfigManager>,
    pub default_secret_backend_kind: CredentialBackendKind,
    pub secret_store: Option<Arc<dyn SecretStore>>,
    pub secret_stores: Option<SecretStoreSet>,
    pub audit_logger: Option<Arc<dyn AuditLogPort>>,
    pub automation_controller: Option<Arc<dyn AutomationPort>>,
    pub ai_runtime_status: Option<AiRuntimeStatus>,
    pub update_control: Option<update_control::UpdateControl>,
}

pub struct WebServer {
    config: WebConfig,
    state: AppState,
    bound_port_state: Option<Arc<AtomicU16>>,
    bound_port_notifier: Option<oneshot::Sender<u16>>,
}

impl WebServer {
    pub fn new(storage: Arc<dyn WebStorage>, config: WebConfig) -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            config,
            state: AppState {
                storage,
                frames_dir: None,
                event_tx,
                config_manager: None,
                default_secret_backend_kind: CredentialBackendKind::Unavailable,
                secret_store: None,
                secret_stores: None,
                audit_logger: None,
                automation_controller: None,
                ai_runtime_status: None,
                update_control: None,
            },
            bound_port_state: None,
            bound_port_notifier: None,
        }
    }

    pub fn with_update_control(mut self, control: update_control::UpdateControl) -> Self {
        self.state.update_control = Some(control);
        self
    }

    pub fn with_config_manager(mut self, config_manager: ConfigManager) -> Self {
        self.state.config_manager = Some(config_manager);
        self
    }

    pub fn with_default_secret_backend_kind(
        mut self,
        default_secret_backend_kind: CredentialBackendKind,
    ) -> Self {
        self.state.default_secret_backend_kind = default_secret_backend_kind;
        self
    }

    pub fn with_secret_store(mut self, secret_store: Arc<dyn SecretStore>) -> Self {
        self.state.secret_store = Some(secret_store);
        self
    }

    pub fn with_secret_stores(mut self, secret_stores: SecretStoreSet) -> Self {
        self.state.secret_stores = Some(secret_stores);
        self
    }

    pub fn with_audit_logger(mut self, logger: Arc<dyn AuditLogPort>) -> Self {
        self.state.audit_logger = Some(logger);
        self
    }

    pub fn with_automation_controller(mut self, controller: Arc<dyn AutomationPort>) -> Self {
        self.state.automation_controller = Some(controller);
        self
    }

    pub fn with_ai_runtime_status(mut self, status: AiRuntimeStatus) -> Self {
        self.state.ai_runtime_status = Some(status);
        self
    }

    pub fn event_sender(&self) -> broadcast::Sender<RealtimeEvent> {
        self.state.event_tx.clone()
    }

    pub fn with_event_tx(mut self, event_tx: broadcast::Sender<RealtimeEvent>) -> Self {
        self.state.event_tx = event_tx;
        self
    }

    pub fn with_frames_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.state.frames_dir = Some(dir);
        self
    }

    pub fn with_bound_port_state(mut self, bound_port_state: Arc<AtomicU16>) -> Self {
        self.bound_port_state = Some(bound_port_state);
        self
    }

    pub fn with_bound_port_notifier(mut self, bound_port_notifier: oneshot::Sender<u16>) -> Self {
        self.bound_port_notifier = Some(bound_port_notifier);
        self
    }

    /// TCP 바인딩 없이 Router만 반환 — Tauri 커스텀 프로토콜 등에서 사용
    pub fn build_router(state: AppState) -> Router {
        use axum::http::HeaderValue;
        use tower_http::cors::AllowOrigin;

        // localhost origin만 허용 (tauri:// + http://127.0.0.1:{port range})
        let allowed_origins: Vec<HeaderValue> = (10090..=10099)
            .flat_map(|port| {
                [
                    format!("http://127.0.0.1:{port}").parse().ok(),
                    format!("http://localhost:{port}").parse().ok(),
                ]
                .into_iter()
                .flatten()
            })
            .chain(std::iter::once("tauri://localhost".parse().unwrap()))
            .collect();

        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::list(allowed_origins))
            .allow_methods(Any)
            .allow_headers(Any);

        let internal_api =
            routes::api_routes().route_layer(middleware::from_fn(require_loopback_client));
        let integration_api = routes::integration_routes().route_layer(
            middleware::from_fn_with_state(state.clone(), require_integration_auth),
        );

        Router::new()
            .nest("/api", internal_api)
            .nest("/integration/v1", integration_api)
            .fallback(loopback_only_static)
            .layer(cors)
            .layer(TraceLayer::new_for_http())
            .with_state(state)
    }

    pub async fn run(self, mut shutdown_rx: watch::Receiver<bool>) -> Result<(), std::io::Error> {
        let Self {
            config,
            state,
            bound_port_state,
            mut bound_port_notifier,
        } = self;

        let integration_auth_configured = config
            .integration_auth_token
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        let host = if config.allow_external && integration_auth_configured {
            "0.0.0.0"
        } else {
            if config.allow_external && !integration_auth_configured {
                warn!(
                    "External access requested but web.integration_auth_token is not configured; falling back to loopback-only binding"
                );
            }
            "127.0.0.1"
        };

        let app = Self::build_router(state);

        let base_port = config.port;
        let mut last_error = None;

        for attempt in 0..MAX_PORT_ATTEMPTS {
            let port = base_port.saturating_add(attempt);

            if port < base_port && attempt > 0 {
                break;
            }

            let addr: SocketAddr = match format!("{}:{}", host, port).parse() {
                Ok(a) => a,
                Err(e) => {
                    error!("{}:{} - {}", host, port, e);
                    continue; // next port attempt
                }
            };

            match TcpListener::bind(addr).await {
                Ok(listener) => {
                    if attempt > 0 {
                        warn!("port {} not-available, port {}", base_port, port);
                    }
                    if let Some(shared_port) = &bound_port_state {
                        shared_port.store(port, Ordering::Relaxed);
                    }
                    if let Some(port_tx) = bound_port_notifier.take() {
                        let _ = port_tx.send(port);
                    }
                    info!("server started: http://{}", addr);

                    axum::serve(
                        listener,
                        app.into_make_service_with_connect_info::<SocketAddr>(),
                    )
                    .with_graceful_shutdown(async move {
                        loop {
                            if *shutdown_rx.borrow() {
                                info!("server ended received");
                                break;
                            }
                            if shutdown_rx.changed().await.is_err() {
                                break;
                            }
                        }
                    })
                    .await?;

                    info!("server ended");
                    return Ok(());
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::AddrInUse {
                        warn!("port {} in progress, next port attempt...", port);
                        last_error = Some(e);
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                format!(
                    "port {}-{} 모두 사용 not-available",
                    base_port,
                    base_port.saturating_add(MAX_PORT_ATTEMPTS - 1)
                ),
            )
        }))
    }

    pub fn url(&self) -> String {
        let port = self
            .bound_port_state
            .as_ref()
            .map(|shared_port| shared_port.load(Ordering::Relaxed))
            .unwrap_or(self.config.port);
        format!("http://localhost:{port}")
    }
}

async fn require_loopback_client(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    if addr.ip().is_loopback() {
        return next.run(request).await;
    }

    crate::error::ApiError::Forbidden(
        "The internal /api surface is available only from loopback clients.".to_string(),
    )
    .into_response()
}

async fn require_integration_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(config_manager) = state.config_manager.as_ref() else {
        return crate::error::ApiError::ServiceUnavailable(
            "Integration API is unavailable because config management is not initialized."
                .to_string(),
        )
        .into_response();
    };

    let expected_token = config_manager
        .get()
        .web
        .integration_auth_token
        .unwrap_or_default()
        .trim()
        .to_string();

    if expected_token.is_empty() {
        return crate::error::ApiError::ServiceUnavailable(
            "Integration API is not configured. Set web.integration_auth_token in config.json before using external access."
                .to_string(),
        )
        .into_response();
    }

    let header_token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            request
                .headers()
                .get(INTEGRATION_TOKEN_HEADER)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        });

    if header_token.as_deref() != Some(expected_token.as_str()) {
        return crate::error::ApiError::Unauthorized(
            "Integration API requires a valid bearer token.".to_string(),
        )
        .into_response();
    }

    next.run(request).await
}

async fn loopback_only_static(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    uri: axum::http::Uri,
) -> Response {
    if addr.ip().is_loopback() {
        return embedded::serve_static(uri).await;
    }

    crate::error::ApiError::Forbidden(
        "The embedded dashboard is available only from loopback clients.".to_string(),
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::{Request, StatusCode};
    use oneshim_core::config::AppConfig;
    use oneshim_core::config_manager::ConfigManager;
    use oneshim_storage::sqlite::SqliteStorage;
    use tempfile::tempdir;
    use tower::ServiceExt;

    #[test]
    fn default_config() {
        let config = WebConfig::default();
        assert_eq!(config.port, oneshim_core::config::DEFAULT_WEB_PORT);
        assert!(!config.allow_external);
    }

    #[test]
    fn web_server_url() {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let server = WebServer::new(storage, WebConfig::default());
        let expected = format!(
            "http://localhost:{}",
            oneshim_core::config::DEFAULT_WEB_PORT
        );
        assert_eq!(server.url(), expected);
    }

    #[test]
    fn web_server_url_prefers_bound_port_state() {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let bound_port_state = Arc::new(AtomicU16::new(11091));
        let server =
            WebServer::new(storage, WebConfig::default()).with_bound_port_state(bound_port_state);

        assert_eq!(server.url(), "http://localhost:11091");
    }

    #[tokio::test]
    async fn web_server_fallback_updates_bound_port_state() {
        let reserved_listener =
            TcpListener::bind(("127.0.0.1", oneshim_core::config::DEFAULT_WEB_PORT))
                .await
                .unwrap();
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let config = WebConfig::default();
        let bound_port_state = Arc::new(AtomicU16::new(config.port));
        let (bound_port_tx, bound_port_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let server = WebServer::new(storage, config)
            .with_bound_port_state(bound_port_state.clone())
            .with_bound_port_notifier(bound_port_tx);

        let server_handle = tokio::spawn(async move { server.run(shutdown_rx).await });

        let fallback_port = tokio::time::timeout(std::time::Duration::from_secs(3), bound_port_rx)
            .await
            .unwrap()
            .unwrap();

        assert_ne!(fallback_port, oneshim_core::config::DEFAULT_WEB_PORT);
        assert_eq!(bound_port_state.load(Ordering::Relaxed), fallback_port);

        let _ = shutdown_tx.send(true);
        let server_result = tokio::time::timeout(std::time::Duration::from_secs(3), server_handle)
            .await
            .unwrap()
            .unwrap();

        assert!(server_result.is_ok());
        drop(reserved_listener);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn max_port_attempts_is_reasonable() {
        assert!(MAX_PORT_ATTEMPTS >= 1);
        assert!(MAX_PORT_ATTEMPTS <= 100);
    }

    #[test]
    fn port_overflow_protection() {
        let base_port: u16 = 65530;
        for attempt in 0..MAX_PORT_ATTEMPTS {
            let port = base_port.saturating_add(attempt);
            assert!(port >= base_port || port == u16::MAX);
        }
    }

    fn test_state_with_config_manager(config_manager: Option<ConfigManager>) -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let (event_tx, _) = broadcast::channel(16);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager,
            default_secret_backend_kind: CredentialBackendKind::Unavailable,
            secret_store: None,
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        }
    }

    fn config_manager_with_integration_token(token: &str) -> ConfigManager {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let manager = ConfigManager::with_path(config_path).unwrap();
        let mut config = AppConfig::default_config();
        config.web.integration_auth_token = Some(token.to_string());
        manager.update(config).unwrap();
        manager
    }

    #[tokio::test]
    async fn internal_api_rejects_non_loopback_clients() {
        let app = WebServer::build_router(test_state_with_config_manager(None)).layer(
            MockConnectInfo(SocketAddr::from(([192, 168, 0, 10], 43000))),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/ai/provider-surfaces")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn integration_api_requires_matching_token() {
        let app = WebServer::build_router(test_state_with_config_manager(Some(
            config_manager_with_integration_token("integration-secret"),
        )))
        .layer(MockConnectInfo(SocketAddr::from(([10, 0, 0, 24], 44000))));

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/integration/v1/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let authorized = app
            .oneshot(
                Request::builder()
                    .uri("/integration/v1/status")
                    .header("authorization", "Bearer integration-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
    }
}
