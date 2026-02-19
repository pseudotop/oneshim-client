//! # oneshim-web
//!
//! 로컬 웹 대시보드 서버.
//! Axum 기반 REST API + React 프론트엔드 임베드.
//!
//! ## 기능
//! - 시스템 메트릭 조회
//! - 프로세스 스냅샷 조회
//! - 유휴 기간 조회
//! - 세션 통계 조회
//! - 프레임(스크린샷) 조회
//! - 정적 파일 서빙 (React 앱)

pub mod embedded;
pub mod error;
pub mod handlers;
pub mod routes;

use axum::Router;
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::controller::AutomationController;
use oneshim_core::config::WebConfig;
use oneshim_core::config_manager::ConfigManager;
use oneshim_storage::sqlite::SqliteStorage;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

// 실시간 이벤트 타입 re-export
pub use handlers::stream::{FrameUpdate, IdleUpdate, MetricsUpdate, RealtimeEvent};

// oneshim_core::config::WebConfig를 re-export
pub use oneshim_core::config::WebConfig as CoreWebConfig;

/// 실시간 이벤트 브로드캐스트 채널 용량
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// 포트 바인드 최대 시도 횟수
const MAX_PORT_ATTEMPTS: u16 = 10;

/// 웹 서버 애플리케이션 상태
#[derive(Clone)]
pub struct AppState {
    /// SQLite 저장소
    pub storage: Arc<SqliteStorage>,
    /// 프레임 저장 디렉토리
    pub frames_dir: Option<std::path::PathBuf>,
    /// 실시간 이벤트 송신 채널
    pub event_tx: broadcast::Sender<RealtimeEvent>,
    /// 설정 관리자
    pub config_manager: Option<ConfigManager>,
    /// 감사 로거 (자동화 시스템)
    pub audit_logger: Option<Arc<RwLock<AuditLogger>>>,
    /// 자동화 제어기
    pub automation_controller: Option<Arc<AutomationController>>,
}

/// 로컬 웹 대시보드 서버
pub struct WebServer {
    config: WebConfig,
    state: AppState,
}

impl WebServer {
    /// 새 웹 서버 생성
    pub fn new(storage: Arc<SqliteStorage>, config: WebConfig) -> Self {
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
            },
        }
    }

    /// 설정 관리자 설정
    pub fn with_config_manager(mut self, config_manager: ConfigManager) -> Self {
        self.state.config_manager = Some(config_manager);
        self
    }

    /// 감사 로거 설정
    pub fn with_audit_logger(mut self, logger: Arc<RwLock<AuditLogger>>) -> Self {
        self.state.audit_logger = Some(logger);
        self
    }

    /// 자동화 제어기 설정
    pub fn with_automation_controller(mut self, controller: Arc<AutomationController>) -> Self {
        self.state.automation_controller = Some(controller);
        self
    }

    /// 실시간 이벤트 송신 채널 반환
    ///
    /// 외부에서 이벤트를 브로드캐스트할 때 사용.
    pub fn event_sender(&self) -> broadcast::Sender<RealtimeEvent> {
        self.state.event_tx.clone()
    }

    /// 외부에서 생성된 이벤트 브로드캐스트 채널 설정
    ///
    /// 스케줄러와 웹서버가 동일한 채널을 공유할 때 사용.
    pub fn with_event_tx(mut self, event_tx: broadcast::Sender<RealtimeEvent>) -> Self {
        self.state.event_tx = event_tx;
        self
    }

    /// 프레임 저장 디렉토리 설정
    pub fn with_frames_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.state.frames_dir = Some(dir);
        self
    }

    /// 서버 실행
    ///
    /// 기본 포트에서 시작하여, 포트가 이미 사용 중이면 다음 포트를 시도합니다.
    /// 최대 10개 포트를 시도한 후 실패하면 에러를 반환합니다.
    ///
    /// # Arguments
    /// * `shutdown_rx` - 종료 신호 수신 채널
    ///
    /// # Returns
    /// 성공 시 `Ok(())`, 모든 포트 바인드 실패 시 `Err`
    pub async fn run(self, mut shutdown_rx: watch::Receiver<bool>) -> Result<(), std::io::Error> {
        let host = if self.config.allow_external {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };

        // CORS 설정
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        // 라우터 구성
        let app = Router::new()
            .nest("/api", routes::api_routes())
            .fallback(embedded::serve_static)
            .layer(cors)
            .layer(TraceLayer::new_for_http())
            .with_state(self.state);

        // 포트 바인드 시도 (최대 MAX_PORT_ATTEMPTS번)
        let base_port = self.config.port;
        let mut last_error = None;

        for attempt in 0..MAX_PORT_ATTEMPTS {
            let port = base_port.saturating_add(attempt);

            // 포트 오버플로우 체크
            if port < base_port && attempt > 0 {
                break;
            }

            let addr: SocketAddr = match format!("{}:{}", host, port).parse() {
                Ok(a) => a,
                Err(e) => {
                    error!("잘못된 주소 {}:{} — {}", host, port, e);
                    continue; // 다음 포트 시도
                }
            };

            match TcpListener::bind(addr).await {
                Ok(listener) => {
                    // 기본 포트가 아닌 경우 경고 로그
                    if attempt > 0 {
                        warn!("포트 {} 사용 불가, 대체 포트 {} 사용", base_port, port);
                    }
                    info!("웹 대시보드 서버 시작: http://{}", addr);

                    // Graceful shutdown과 함께 서버 실행
                    axum::serve(listener, app)
                        .with_graceful_shutdown(async move {
                            loop {
                                if *shutdown_rx.borrow() {
                                    info!("웹 서버 종료 신호 수신");
                                    break;
                                }
                                if shutdown_rx.changed().await.is_err() {
                                    break;
                                }
                            }
                        })
                        .await?;

                    info!("웹 대시보드 서버 종료");
                    return Ok(());
                }
                Err(e) => {
                    // AddrInUse 에러인 경우 다음 포트 시도
                    if e.kind() == std::io::ErrorKind::AddrInUse {
                        warn!("포트 {} 이미 사용 중, 다음 포트 시도...", port);
                        last_error = Some(e);
                        continue;
                    }
                    // 다른 에러는 즉시 반환
                    return Err(e);
                }
            }
        }

        // 모든 시도 실패
        Err(last_error.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                format!(
                    "포트 {}-{} 모두 사용 불가",
                    base_port,
                    base_port.saturating_add(MAX_PORT_ATTEMPTS - 1)
                ),
            )
        }))
    }

    /// 서버 URL 반환
    pub fn url(&self) -> String {
        format!("http://localhost:{}", self.config.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // 최소 1번, 최대 100번 사이
        assert!(MAX_PORT_ATTEMPTS >= 1);
        assert!(MAX_PORT_ATTEMPTS <= 100);
    }

    #[test]
    fn port_overflow_protection() {
        // u16::MAX에서 시작해도 오버플로우가 발생하지 않아야 함
        let base_port: u16 = 65530;
        for attempt in 0..MAX_PORT_ATTEMPTS {
            let port = base_port.saturating_add(attempt);
            // saturating_add로 오버플로우 방지 확인
            assert!(port >= base_port || port == u16::MAX);
        }
    }
}
