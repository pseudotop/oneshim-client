use oneshim_api_contracts::integration::IntegrationOutboundRuntimeStatus;
use oneshim_automation::audit::{AuditLogAdapter, AuditLogger};
use oneshim_automation::controller::AutomationController;
use oneshim_core::config::AppConfig;
#[cfg(feature = "server")]
use oneshim_core::config::CredentialBackendKind;
use oneshim_core::config_manager::ConfigManager;
#[cfg(feature = "server")]
use oneshim_core::ports::integration::{
    IntegrationAuditPort, IntegrationAuthPort, IntegrationInboxPort, IntegrationInboxStorePort,
    IntegrationOutboxPort, IntegrationRuntimeTelemetryPort, IntegrationSessionPort,
};
#[cfg(feature = "server")]
use oneshim_core::ports::oauth::OAuthPort;
#[cfg(feature = "server")]
use oneshim_core::ports::secret_store::{SecretStore, SecretStoreSet};
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_web::update_control::UpdateControl;
use oneshim_web::{AiRuntimeStatus, RealtimeEvent, WebServer, WebServerRuntimeBindings};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, watch};
use tracing::{error, info, warn};

use crate::automation_controller_builder::AutomationControllerBuilder;

pub(crate) struct WebServerLaunchResult {
    pub(crate) automation_controller: Option<Arc<AutomationController>>,
}

pub(crate) struct WebServerLaunchContext<'a> {
    runtime_handle: &'a Handle,
    shutdown_tx: &'a watch::Sender<bool>,
    event_tx: broadcast::Sender<RealtimeEvent>,
    web_port_state: Arc<AtomicU16>,
}

impl<'a> WebServerLaunchContext<'a> {
    pub(crate) fn new(
        runtime_handle: &'a Handle,
        shutdown_tx: &'a watch::Sender<bool>,
        event_tx: broadcast::Sender<RealtimeEvent>,
        web_port_state: Arc<AtomicU16>,
    ) -> Self {
        Self {
            runtime_handle,
            shutdown_tx,
            event_tx,
            web_port_state,
        }
    }
}

#[cfg(feature = "server")]
pub(crate) struct WebServerServerSupport {
    integration_auth: Option<Arc<dyn IntegrationAuthPort>>,
    integration_session: Option<Arc<dyn IntegrationSessionPort>>,
    integration_outbox: Option<Arc<dyn IntegrationOutboxPort>>,
    integration_inbox: Option<Arc<dyn IntegrationInboxPort>>,
    integration_inbox_store: Option<Arc<dyn IntegrationInboxStorePort>>,
    integration_audit: Option<Arc<dyn IntegrationAuditPort>>,
    integration_runtime_telemetry: Option<Arc<dyn IntegrationRuntimeTelemetryPort>>,
    secret_store: Option<Arc<dyn SecretStore>>,
    secret_stores: Option<SecretStoreSet>,
    default_secret_backend_kind: Option<CredentialBackendKind>,
    oauth_port: Option<Arc<dyn OAuthPort>>,
}

#[cfg(feature = "server")]
impl WebServerServerSupport {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        integration_auth: Option<Arc<dyn IntegrationAuthPort>>,
        integration_session: Option<Arc<dyn IntegrationSessionPort>>,
        integration_outbox: Option<Arc<dyn IntegrationOutboxPort>>,
        integration_inbox: Option<Arc<dyn IntegrationInboxPort>>,
        integration_inbox_store: Option<Arc<dyn IntegrationInboxStorePort>>,
        integration_audit: Option<Arc<dyn IntegrationAuditPort>>,
        integration_runtime_telemetry: Option<Arc<dyn IntegrationRuntimeTelemetryPort>>,
        secret_store: Option<Arc<dyn SecretStore>>,
        secret_stores: Option<SecretStoreSet>,
        default_secret_backend_kind: Option<CredentialBackendKind>,
        oauth_port: Option<Arc<dyn OAuthPort>>,
    ) -> Self {
        Self {
            integration_auth,
            integration_session,
            integration_outbox,
            integration_inbox,
            integration_inbox_store,
            integration_audit,
            integration_runtime_telemetry,
            secret_store,
            secret_stores,
            default_secret_backend_kind,
            oauth_port,
        }
    }
}

pub(crate) struct WebServerSupportContext {
    config_manager: ConfigManager,
    update_control: UpdateControl,
    integration_runtime_status: IntegrationOutboundRuntimeStatus,
    app_handle: Option<tauri::AppHandle>,
    cli_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    #[cfg(feature = "server")]
    server: Option<WebServerServerSupport>,
}

impl WebServerSupportContext {
    pub(crate) fn new(
        config_manager: ConfigManager,
        update_control: UpdateControl,
        integration_runtime_status: IntegrationOutboundRuntimeStatus,
    ) -> Self {
        Self {
            config_manager,
            update_control,
            integration_runtime_status,
            app_handle: None,
            cli_health_flag: None,
            #[cfg(feature = "server")]
            server: None,
        }
    }

    pub(crate) fn with_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.app_handle = Some(handle);
        self
    }

    pub(crate) fn with_cli_health_flag(mut self, flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.cli_health_flag = Some(flag);
        self
    }

    #[cfg(feature = "server")]
    pub(crate) fn with_server_support(mut self, server: WebServerServerSupport) -> Self {
        self.server = Some(server);
        self
    }

    fn configure_automation_builder<'a>(
        &self,
        builder: AutomationControllerBuilder<'a>,
    ) -> AutomationControllerBuilder<'a> {
        let builder = if let Some(ref flag) = self.cli_health_flag {
            builder.with_cli_health_flag(flag.clone())
        } else {
            builder
        };
        let builder = if let Some(ref handle) = self.app_handle {
            builder.with_app_handle(handle.clone())
        } else {
            builder
        };

        #[cfg(feature = "server")]
        {
            if let Some(server) = self.server.as_ref() {
                return builder
                    .with_provider_secret_stores(server.secret_stores.clone().unwrap_or_default())
                    .with_oauth_port(server.oauth_port.clone());
            }
        }

        builder
    }

    fn build_runtime_bindings(
        &self,
        event_tx: broadcast::Sender<RealtimeEvent>,
        data_dir: &Path,
        audit_logger: Arc<AuditLogAdapter>,
        ai_runtime_status: Option<AiRuntimeStatus>,
    ) -> WebServerRuntimeBindings {
        let runtime_bindings = WebServerRuntimeBindings {
            event_tx: Some(event_tx),
            frames_dir: Some(data_dir.to_path_buf()),
            config_manager: Some(self.config_manager.clone()),
            audit_logger: Some(audit_logger),
            ai_runtime_status,
            integration_runtime_status: Some(self.integration_runtime_status.clone()),
            update_control: Some(self.update_control.clone()),
            ..Default::default()
        };

        #[cfg(feature = "server")]
        {
            let mut runtime_bindings = runtime_bindings;
            if let Some(server) = self.server.as_ref() {
                runtime_bindings.integration_auth = server.integration_auth.clone();
                runtime_bindings.integration_session = server.integration_session.clone();
                runtime_bindings.integration_outbox = server.integration_outbox.clone();
                runtime_bindings.integration_inbox = server.integration_inbox.clone();
                runtime_bindings.integration_inbox_store = server.integration_inbox_store.clone();
                runtime_bindings.integration_audit = server.integration_audit.clone();
                runtime_bindings.integration_runtime_telemetry =
                    server.integration_runtime_telemetry.clone();
                runtime_bindings.secret_store = server.secret_store.clone();
                runtime_bindings.secret_stores = server.secret_stores.clone();
                runtime_bindings.default_secret_backend_kind = server.default_secret_backend_kind;
            }
            runtime_bindings
        }

        #[cfg(not(feature = "server"))]
        {
            runtime_bindings
        }
    }
}

pub(crate) struct WebServerRuntimeBuilder<'a> {
    storage: Arc<SqliteStorage>,
    config: &'a AppConfig,
    data_dir: &'a Path,
    launch_context: WebServerLaunchContext<'a>,
    support_context: WebServerSupportContext,
    override_store: Option<Arc<dyn oneshim_core::ports::override_store::OverrideStore>>,
    recluster_requested: Option<Arc<std::sync::atomic::AtomicBool>>,
    coaching_engine: Option<Arc<dyn oneshim_core::ports::coaching::CoachingPort>>,
    session_manager: Option<Arc<dyn oneshim_core::ports::conversation_session::SessionManager>>,
}

impl<'a> WebServerRuntimeBuilder<'a> {
    pub(crate) fn new(
        storage: Arc<SqliteStorage>,
        config: &'a AppConfig,
        data_dir: &'a Path,
        launch_context: WebServerLaunchContext<'a>,
        support_context: WebServerSupportContext,
    ) -> Self {
        Self {
            storage,
            config,
            data_dir,
            launch_context,
            support_context,
            override_store: None,
            recluster_requested: None,
            coaching_engine: None,
            session_manager: None,
        }
    }

    #[cfg(feature = "server")]
    pub(crate) fn with_server_support(mut self, server: WebServerServerSupport) -> Self {
        self.support_context = self.support_context.with_server_support(server);
        self
    }

    pub(crate) fn with_override_store(
        mut self,
        store: Arc<dyn oneshim_core::ports::override_store::OverrideStore>,
    ) -> Self {
        self.override_store = Some(store);
        self
    }

    pub(crate) fn with_recluster_requested(
        mut self,
        flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        self.recluster_requested = Some(flag);
        self
    }

    pub(crate) fn with_coaching_engine(
        mut self,
        engine: Arc<dyn oneshim_core::ports::coaching::CoachingPort>,
    ) -> Self {
        self.coaching_engine = Some(engine);
        self
    }

    pub(crate) fn with_session_manager(
        mut self,
        manager: Arc<dyn oneshim_core::ports::conversation_session::SessionManager>,
    ) -> Self {
        self.session_manager = Some(manager);
        self
    }

    pub(crate) fn build_and_spawn(self) -> WebServerLaunchResult {
        let web_shutdown_rx = self.launch_context.shutdown_tx.subscribe();
        let web_audit_logger = Arc::new(tokio::sync::RwLock::new(AuditLogger::default()));
        let (bound_port_tx, bound_port_rx) = tokio::sync::oneshot::channel::<u16>();

        let automation_frame_storage = match self.launch_context.runtime_handle.block_on(async {
            FrameFileStorage::new(
                self.data_dir.to_path_buf(),
                self.config.storage.max_storage_mb,
                self.config.storage.retention_days,
            )
            .await
        }) {
            Ok(storage) => Some(Arc::new(storage)),
            Err(err) => {
                warn!(error = %err, "frame storage init failure, falling back to NoOp");
                None
            }
        };

        let automation_build = {
            let builder = AutomationControllerBuilder::new(
                self.config,
                self.data_dir,
                self.launch_context.runtime_handle,
                web_audit_logger.clone(),
                automation_frame_storage,
            );
            let builder = self.support_context.configure_automation_builder(builder);
            builder.build()
        };

        let ai_runtime_status = automation_build.ai_runtime_status.clone();
        let automation_controller = automation_build.controller;
        let automation_controller_for_state = automation_controller.clone();
        let gui_audit_logger = web_audit_logger.clone();
        let mut runtime_bindings = self.support_context.build_runtime_bindings(
            self.launch_context.event_tx.clone(),
            self.data_dir,
            Arc::new(AuditLogAdapter::new(web_audit_logger)),
            ai_runtime_status,
        );
        runtime_bindings.override_store = self.override_store;
        runtime_bindings.recluster_requested = self.recluster_requested;
        runtime_bindings.coaching_engine = self.coaching_engine;
        runtime_bindings.session_manager = self.session_manager;

        // Spawn GUI audit forwarder if the automation controller has a GUI service
        if let Some(ref controller) = automation_controller {
            spawn_gui_audit_forwarder(controller, gui_audit_logger);
        }

        let web_storage = self.storage.clone();
        let web_config = self.config.web.clone();
        let web_port_state = self.launch_context.web_port_state.clone();
        self.launch_context.runtime_handle.spawn(async move {
            if let Some(controller) = automation_controller {
                runtime_bindings.automation_controller = Some(controller);
            }
            let web_server = WebServer::new(web_storage, web_config)
                .with_bound_port_state(web_port_state)
                .with_bound_port_notifier(bound_port_tx)
                .with_runtime_bindings(runtime_bindings);
            if let Err(error) = web_server.run(web_shutdown_rx).await {
                error!("WebServer error: {error}");
            }
        });

        let frontend_port = self.launch_context.runtime_handle.block_on(async {
            tokio::time::timeout(Duration::from_secs(3), bound_port_rx)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_else(|| self.launch_context.web_port_state.load(Ordering::Relaxed))
        });
        info!("WebServer: http://localhost:{frontend_port}");

        WebServerLaunchResult {
            automation_controller: automation_controller_for_state,
        }
    }
}

/// Subscribes to GUI session events and forwards them to the audit logger.
fn spawn_gui_audit_forwarder(
    automation_controller: &Arc<AutomationController>,
    audit_logger: Arc<tokio::sync::RwLock<AuditLogger>>,
) {
    let Some(gui_service) = automation_controller.gui_service() else {
        tracing::debug!("GUI service not configured; skipping audit forwarder");
        return;
    };

    let mut rx = gui_service.subscribe();

    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::warn!("No tokio runtime — GUI audit forwarder not started");
        return;
    };
    handle.spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let action_type = format!("gui.session.{}", event.event_type);
                    let details = event.message.unwrap_or_default();
                    let mut logger = audit_logger.write().await;
                    logger.log_event(&action_type, &event.session_id, &details);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("GUI audit forwarder lagged by {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!("GUI event channel closed; audit forwarder exiting");
                    break;
                }
            }
        }
    });
}
