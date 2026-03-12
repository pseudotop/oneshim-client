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
use axum::Router;
use oneshim_core::config::WebConfig;
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::ports::audit_log::AuditLogPort;
use oneshim_core::ports::automation::AutomationPort;
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

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn WebStorage>,
    pub frames_dir: Option<std::path::PathBuf>,
    pub event_tx: broadcast::Sender<RealtimeEvent>,
    pub config_manager: Option<ConfigManager>,
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

        Router::new()
            .nest("/api", routes::api_routes())
            .fallback(embedded::serve_static)
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

        let host = if config.allow_external {
            "0.0.0.0"
        } else {
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

                    axum::serve(listener, app)
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

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_storage::sqlite::SqliteStorage;

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
}
