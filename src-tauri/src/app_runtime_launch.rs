use anyhow::Result;
use oneshim_core::consent::ConsentManager;
use std::sync::Arc;
use tauri::AppHandle;
use tracing::info;

use crate::agent_runtime::AgentRuntimeBuilder;
use crate::bootstrap_runtime::BootstrapRuntimeBundle;
use crate::launch_resources::LaunchCoreResourcesBuilder;
use crate::magic_overlay::MagicOverlayHandle;
use crate::runtime_state::{AppState, ManagedStateBuilder};
#[cfg(feature = "server")]
use crate::server_runtime_context::ServerLaunchContext;
use crate::web_server_runtime::{
    WebServerLaunchContext, WebServerRuntimeBuilder, WebServerSupportContext,
};

pub(crate) struct AppRuntimeLaunchResult {
    pub(crate) frontend_web_port: u16,
    pub(crate) state_builder: ManagedStateBuilder,
}

pub(crate) struct AppRuntimeLaunchBuilder {
    bootstrap: BootstrapRuntimeBundle,
    app_handle: AppHandle,
}

impl AppRuntimeLaunchBuilder {
    pub(crate) fn new(bootstrap: BootstrapRuntimeBundle, app_handle: AppHandle) -> Self {
        Self {
            bootstrap,
            app_handle,
        }
    }

    pub(crate) fn build_and_spawn(self) -> Result<AppRuntimeLaunchResult> {
        let frontend_web_port = self.bootstrap.frontend_web_port();
        let integration_runtime_status = self.bootstrap.integration_runtime_status();

        let BootstrapRuntimeBundle {
            db_path,
            data_dir_path,
            config_manager,
            config,
            runtime_handle: handle,
            web_port,
            #[cfg(feature = "server")]
            server,
            #[cfg(not(feature = "server"))]
                integration_runtime_status: _integration_runtime_status,
        } = self.bootstrap;

        #[cfg(feature = "server")]
        let server_context = ServerLaunchContext::from_bootstrap(server);

        let core_resources = LaunchCoreResourcesBuilder::new(
            &config,
            &db_path,
            &data_dir_path,
            &handle,
            self.app_handle.clone(),
        )
        .build()?;
        let update_control = core_resources.update_runtime.update_control.clone();
        let update_action_tx = core_resources.update_runtime.update_action_tx.clone();
        let sqlite_storage = core_resources.storage_runtime.sqlite_storage.clone();
        let event_tx = core_resources.background_runtime.event_tx();
        let shutdown_tx = core_resources.background_runtime.shutdown_tx();

        // Shared flag for on-demand re-clustering: scheduler, web server, and Tauri IPC
        // all reference the same AtomicBool so any endpoint can trigger re-clustering.
        let recluster_requested = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Shared capture pause flag: scheduler monitor loop, tray menu, and IPC commands
        // all reference the same AtomicBool to toggle capture on/off.
        let capture_paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        // Tracking indicator visibility — initialized from persisted config.
        let indicator_visible = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            config.indicator.show_border,
        ));

        // Connection status flags — start disconnected, wired by scheduler health checks.
        let server_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let llm_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cli_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        #[cfg(feature = "server")]
        server_context
            .spawn_integration_loops(&core_resources.background_runtime, sqlite_storage.clone());

        // Create shared CoachingEngine for scheduler, web server, and Tauri IPC
        let coaching_engine = Arc::new(oneshim_analysis::CoachingEngine::new(
            config.coaching.clone(),
        ));

        // Create MagicOverlay handle (window is lazily created on first coaching message)
        let magic_overlay =
            MagicOverlayHandle::new(self.app_handle.clone(), config.coaching.overlay_mode);

        let agent_runtime = {
            let builder = AgentRuntimeBuilder::new(
                sqlite_storage.clone(),
                sqlite_storage.clone(),
                sqlite_storage.clone(),
                sqlite_storage.clone(),
                &data_dir_path,
                &config,
                config_manager.clone(),
                recluster_requested.clone(),
                self.app_handle.clone(),
            )
            .with_vector_store(Arc::new(
                oneshim_storage::sqlite::vector_store_impl::SqliteVectorStore::new(
                    sqlite_storage.connection_arc(),
                ),
            ))
            .with_offline_mode(false)
            .with_event_tx(
                core_resources
                    .background_runtime
                    .agent_event_tx(config.web.enabled),
            )
            .with_calibration_writer(sqlite_storage.clone())
            .with_calibration_reader(sqlite_storage.clone())
            .with_override_store(sqlite_storage.clone())
            .with_consent_manager(Arc::new(ConsentManager::new(
                data_dir_path.join("consent.json"),
            )))
            .with_coaching_engine(coaching_engine.clone())
            .with_coaching_storage(sqlite_storage.clone())
            .with_magic_overlay(magic_overlay.clone())
            .with_capture_paused(capture_paused.clone());
            #[cfg(feature = "server")]
            let builder = server_context.configure_agent_builder(builder);
            builder.build()
        };
        agent_runtime.spawn_on(&handle, core_resources.background_runtime.shutdown_rx());
        info!("Agent started");

        let automation_controller = if config.web.enabled {
            let launch_context =
                WebServerLaunchContext::new(&handle, &shutdown_tx, event_tx, web_port.clone());
            let support_context = WebServerSupportContext::new(
                config_manager.clone(),
                update_control.clone(),
                integration_runtime_status,
            )
            .with_app_handle(self.app_handle.clone());
            let builder = WebServerRuntimeBuilder::new(
                sqlite_storage.clone(),
                &config,
                &data_dir_path,
                launch_context,
                support_context,
            )
            .with_override_store(sqlite_storage.clone())
            .with_recluster_requested(recluster_requested.clone())
            .with_coaching_engine(
                coaching_engine.clone() as Arc<dyn oneshim_core::ports::coaching::CoachingPort>
            );
            #[cfg(feature = "server")]
            let builder = server_context.configure_web_server_builder(builder);
            let web_server_runtime = builder.build_and_spawn();
            web_server_runtime.automation_controller
        } else {
            None
        };

        // Wire initial connection status from config / adapter availability.
        // Server: mark connected when the server feature is compiled in.
        #[cfg(feature = "server")]
        server_connected.store(true, std::sync::atomic::Ordering::Relaxed);

        // LLM: mark connected when text intelligence analysis is enabled in config.
        if config.analysis.text_intelligence.enabled {
            llm_connected.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        // CLI: mark connected when the automation controller was successfully created.
        if automation_controller.is_some() {
            cli_connected.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        core_resources.background_runtime.spawn_runtime_bridges();

        let state_builder = ManagedStateBuilder::new(AppState {
            runtime_handle: handle,
            config,
            web_port,
            storage: sqlite_storage,
            config_manager,
            update_control: Some(update_control),
            update_action_tx,
            automation_controller,
            shutdown_tx,
            recluster_requested: recluster_requested.clone(),
            magic_overlay: Some(magic_overlay),
            coaching_engine: Some(
                coaching_engine as Arc<dyn oneshim_core::ports::coaching::CoachingPort>,
            ),
            capture_paused,
            indicator_visible,
            server_connected,
            llm_connected,
            cli_connected,
        });
        #[cfg(feature = "server")]
        let state_builder = server_context.configure_state_builder(state_builder);

        Ok(AppRuntimeLaunchResult {
            frontend_web_port,
            state_builder,
        })
    }
}
