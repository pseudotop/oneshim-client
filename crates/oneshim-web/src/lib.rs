//! # oneshim-web

pub mod embedded;
pub mod error;
pub mod handlers;
pub mod routes;
pub mod services;
pub mod storage_port;
pub mod update_control;

use crate::storage_port::WebStorage;
use axum::Router;
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::controller::AutomationController;
use oneshim_core::config::WebConfig;
use oneshim_core::config_manager::ConfigManager;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch, RwLock};
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
    pub audit_logger: Option<Arc<RwLock<AuditLogger>>>,
    pub automation_controller: Option<Arc<AutomationController>>,
    pub ai_runtime_status: Option<AiRuntimeStatus>,
    pub update_control: Option<update_control::UpdateControl>,
}

pub struct WebServer {
    config: WebConfig,
    state: AppState,
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

    pub fn with_audit_logger(mut self, logger: Arc<RwLock<AuditLogger>>) -> Self {
        self.state.audit_logger = Some(logger);
        self
    }

    pub fn with_automation_controller(mut self, controller: Arc<AutomationController>) -> Self {
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

    /// TCP 바인딩 없이 Router만 반환 — Tauri 커스텀 프로토콜 등에서 사용
    pub fn build_router(state: AppState) -> Router {
        let cors = CorsLayer::new()
            .allow_origin(Any)
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
        let host = if self.config.allow_external {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };

        let app = Self::build_router(self.state);

        let base_port = self.config.port;
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
        format!("http://localhost:{}", self.config.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_storage::sqlite::SqliteStorage;

    #[test]
    fn default_config() {
        let config = WebConfig::default();
        assert_eq!(config.port, 9090);
        assert!(!config.allow_external);
    }

    #[test]
    fn web_server_url() {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).unwrap());
        let server = WebServer::new(storage, WebConfig::default());
        assert_eq!(server.url(), "http://localhost:9090");
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
