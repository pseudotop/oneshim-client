use anyhow::Result;
use oneshim_core::consent::ConsentManager;
use std::sync::Arc;
use tauri::AppHandle;
use tracing::info;

use crate::agent_runtime::AgentRuntimeBuilder;
use crate::bootstrap_runtime::BootstrapRuntimeBundle;
use crate::launch_resources::LaunchCoreResourcesBuilder;
use crate::magic_overlay::MagicOverlayHandle;
use crate::runtime_bridges::RuntimeBridgeSpawner;
use crate::runtime_state::{AppState, CaptureContext, ConnectionStatus, ManagedStateBuilder};
use crate::scheduler::shared_regime_state::SharedRegimeState;
#[cfg(feature = "server")]
use crate::server_runtime_context::ServerLaunchContext;
use crate::session_context::SessionContextAssembler;
use crate::session_manager::SessionManagerImpl;
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

        // Connection status flags — start disconnected, updated by health check loop.
        let server_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let llm_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cli_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Focus mode state — transient, not persisted across restarts.
        let focus_mode = Arc::new(crate::focus_mode::FocusModeState::new());

        // Frame processor, frame storage, and activity monitor for IPC capture commands (A1, A2).
        // These are INTENTIONALLY separate instances from the scheduler's copies.
        //
        // Why not share? The scheduler's instances are created inside AgentRuntimeBuilder
        // and moved into the spawned async task — they are not accessible after spawn.
        // Sharing would require either:
        //   (a) creating instances before the builder and passing Arc to both, or
        //   (b) exposing scheduler internals through a handle.
        // Both approaches couple the IPC layer to scheduler lifetime/wiring.
        //
        // Tradeoffs of separate instances:
        //   - FrameProcessor: stateless except for a Mutex<Option<prev_frame>> used for
        //     delta encoding. IPC manual captures always use importance=1.0 (Full mode),
        //     so delta state is irrelevant — no functional difference.
        //   - FrameFileStorage: manages the same directory (data_dir/frames). Concurrent
        //     writes are safe because file names are timestamp-based UUIDs.
        //   - ActivityTracker: wraps ProcessTracker which queries OS APIs. No shared state.
        //   - AccessibilityExtractor: stateless OS API wrapper.
        //   - ConsentManager: reads the same consent.json file. Read-only from IPC.
        let ipc_frame_storage: Option<Arc<oneshim_storage::frame_storage::FrameFileStorage>> =
            match handle.block_on(oneshim_storage::frame_storage::FrameFileStorage::new(
                data_dir_path.to_path_buf(),
                config.storage.max_storage_mb,
                config.storage.retention_days,
            )) {
                Ok(fs) => Some(Arc::new(fs)),
                Err(e) => {
                    tracing::warn!("IPC frame storage init failed: {e}");
                    None
                }
            };
        let ipc_frame_processor: Option<Arc<dyn oneshim_core::ports::vision::FrameProcessor>> = {
            let ocr_tessdata = std::env::var("ONESHIM_TESSDATA")
                .ok()
                .map(std::path::PathBuf::from);
            Some(Arc::new(
                oneshim_vision::processor::EdgeFrameProcessor::new(
                    config.vision.thumbnail_width,
                    config.vision.thumbnail_height,
                    ocr_tessdata,
                ),
            ))
        };
        let ipc_activity_monitor: Option<Arc<dyn oneshim_core::ports::monitor::ActivityMonitor>> = {
            let process_monitor: Arc<dyn oneshim_core::ports::monitor::ProcessMonitor> =
                Arc::new(oneshim_monitor::process::ProcessTracker::new());
            Some(Arc::new(oneshim_monitor::activity::ActivityTracker::new(
                process_monitor,
            )))
        };

        // Accessibility extractor for IPC scene analysis (A2).
        // Create a separate instance so IPC can call extract_focused_element
        // independently of the scheduler.
        let ipc_accessibility_extractor: Option<
            Arc<dyn oneshim_core::ports::accessibility::AccessibilityExtractor>,
        > = oneshim_vision::accessibility::create_extractor();

        // Consent manager for IPC — shared read-only instance for PII consent checks (A2).
        let ipc_consent_manager: Option<Arc<oneshim_core::consent::ConsentManager>> =
            Some(Arc::new(oneshim_core::consent::ConsentManager::new(
                data_dir_path.join("consent.json"),
            )));

        // SuggestionManager for overlay panel (A3).
        // The queue Arc is created here and passed to BOTH the SuggestionManager
        // and the agent runtime (which builds SuggestionReceiver). This ensures
        // SSE-received suggestions appear in IPC queries via get_pending_suggestions.
        #[cfg(feature = "server")]
        let shared_suggestion_queue = Arc::new(tokio::sync::Mutex::new(
            oneshim_suggestion::queue::SuggestionQueue::new(config.analysis.max_suggestions),
        ));
        #[cfg(feature = "server")]
        let suggestion_manager: Option<Arc<crate::suggestion_manager::SuggestionManager>> = {
            use oneshim_network::auth::TokenManager;
            use oneshim_network::http_client::HttpApiClient;
            match (
                TokenManager::new_with_tls(
                    &config.server.base_url,
                    &config.tls,
                    Some(config.request_timeout()),
                ),
                HttpApiClient::new_with_tls(
                    &config.server.base_url,
                    // TokenManager is needed, create a fresh one
                    Arc::new(
                        TokenManager::new_with_tls(
                            &config.server.base_url,
                            &config.tls,
                            Some(config.request_timeout()),
                        )
                        .unwrap_or_else(|_| TokenManager::new(&config.server.base_url)),
                    ),
                    config.request_timeout(),
                    &config.tls,
                ),
            ) {
                (_, Ok(api_client)) => {
                    let api: Arc<dyn oneshim_core::ports::api_client::ApiClient> =
                        Arc::new(api_client);
                    let history = Arc::new(tokio::sync::Mutex::new(
                        oneshim_suggestion::history::SuggestionHistory::new(100),
                    ));
                    let feedback = oneshim_suggestion::feedback::FeedbackSender::new(api);
                    Some(Arc::new(crate::suggestion_manager::SuggestionManager::new(
                        shared_suggestion_queue.clone(),
                        history,
                        feedback,
                    )))
                }
                _ => {
                    tracing::warn!("SuggestionManager init skipped: server transport unavailable");
                    None
                }
            }
        };
        #[cfg(not(feature = "server"))]
        let suggestion_manager: Option<Arc<crate::suggestion_manager::SuggestionManager>> = None;

        // Adapter-side health flags — written by adapters on success/failure,
        // read by the health check loop. The loop is the single source of truth
        // for connection status.
        let server_health_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let llm_health_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cli_health_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        #[cfg(feature = "server")]
        server_context
            .spawn_integration_loops(&core_resources.background_runtime, sqlite_storage.clone());

        // Create shared CoachingEngine for scheduler, web server, and Tauri IPC
        let coaching_engine = Arc::new(oneshim_analysis::CoachingEngine::new(
            config.coaching.clone(),
        ));

        // Create MagicOverlay handle (window created at startup in setup.rs)
        let magic_overlay =
            MagicOverlayHandle::new(self.app_handle.clone(), config.coaching.overlay_mode);

        // Shared SharedRegimeState — single instance used by both SessionManager (context
        // assembler) and Scheduler (monitor/coaching loops). Created before both consumers.
        let shared_regime_state = Arc::new(SharedRegimeState::new());

        // Obtain shutdown receiver for idle reaper before core_resources is consumed.
        let reaper_shutdown_rx = core_resources.background_runtime.shutdown_rx();

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
            .with_overlay_driver(Arc::new(
                crate::magic_overlay_driver::MagicOverlayDriver::new(self.app_handle.clone()),
            ))
            .with_capture_paused(capture_paused.clone())
            .with_focus_mode(focus_mode.clone())
            .with_shared_regime(shared_regime_state.clone())
            .with_health_flags(
                server_health_flag.clone(),
                llm_health_flag.clone(),
                cli_health_flag.clone(),
            )
            .with_connection_flags(
                server_connected.clone(),
                llm_connected.clone(),
                cli_connected.clone(),
            )
            .with_tray_app_handle(self.app_handle.clone())
            .with_suggestions_enabled(config.suggestions.enabled);
            #[cfg(feature = "server")]
            let builder = builder.with_shared_suggestion_queue(shared_suggestion_queue);
            #[cfg(feature = "server")]
            let builder = server_context.configure_agent_builder(builder);
            builder.build()
        };
        agent_runtime.spawn_on(&handle, core_resources.background_runtime.shutdown_rx());
        info!("Agent started");

        // Session manager wiring: AuditLogAdapter + SessionContextAssembler.
        // Creates a SEPARATE AuditLogger instance (not shared with web_server_runtime)
        // because the web server's logger is scoped to its own lifecycle and not exposed.
        let session_manager = {
            let audit_logger = Arc::new(tokio::sync::RwLock::new(
                oneshim_automation::audit::AuditLogger::new(500, 50),
            ));
            let audit_port: Arc<dyn oneshim_core::ports::audit_log::AuditLogPort> = Arc::new(
                oneshim_automation::audit::AuditLogAdapter::new(audit_logger),
            );

            let session_config = Arc::new(config.ai_session.clone());

            let context_assembler = Arc::new(SessionContextAssembler::new(
                sqlite_storage.clone(),
                Arc::new(config.clone()),
                shared_regime_state.clone(),
            ));

            // Resolve provider secret backend so HttpApi sessions can look up
            // API keys via CredentialSource::StoredSecret (keychain / file / env).
            let secret_store = {
                let config_dir = oneshim_core::config_manager::ConfigManager::config_dir()
                    .unwrap_or_else(|_| data_dir_path.to_path_buf());
                let os_store = crate::provider_secret_backend::create_os_secret_store(&config_dir);
                match crate::provider_secret_backend::resolve_provider_secret_backend(
                    &config_dir,
                    os_store,
                ) {
                    Ok(r) => r.secret_store,
                    Err(e) => {
                        tracing::debug!("provider secret backend unavailable: {e}");
                        None
                    }
                }
            };

            let mut manager =
                SessionManagerImpl::new(session_config, audit_port, Some(context_assembler));
            if let Some(store) = secret_store {
                manager = manager.with_secret_store(store);
            }
            Some(Arc::new(manager))
        };

        // Spawn idle reaper background task — periodically calls reap_idle_sessions
        // to transition Active→Idle→Terminated for sessions that exceed the idle timeout.
        if let Some(ref sm) = session_manager {
            let sm_clone = sm.clone();
            let mut shutdown_rx = reaper_shutdown_rx;
            handle.spawn(async move {
                let interval =
                    std::time::Duration::from_secs(sm_clone.config.health_check_interval_secs);
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {
                            sm_clone.reap_idle_sessions().await;
                        }
                        _ = shutdown_rx.changed() => break,
                    }
                }
            });
            info!("idle reaper background task started");
        }

        let automation_controller = if config.web.enabled {
            let launch_context =
                WebServerLaunchContext::new(&handle, &shutdown_tx, event_tx, web_port.clone());
            let support_context = WebServerSupportContext::new(
                config_manager.clone(),
                update_control.clone(),
                integration_runtime_status,
            )
            .with_app_handle(self.app_handle.clone())
            .with_cli_health_flag(cli_health_flag.clone());
            let mut builder = WebServerRuntimeBuilder::new(
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
            if let Some(ref sm) = session_manager {
                builder = builder.with_session_manager(sm.clone()
                    as Arc<dyn oneshim_core::ports::conversation_session::SessionManager>);
            }
            #[cfg(feature = "server")]
            let builder = server_context.configure_web_server_builder(builder);
            let web_server_runtime = builder.build_and_spawn();
            web_server_runtime.automation_controller
        } else {
            None
        };

        // Connection status is now driven by the health check loop —
        // no optimistic initialization. The loop reads adapter health flags
        // and updates connection flags as the single source of truth.

        core_resources.background_runtime.spawn_runtime_bridges();

        // Forward update status changes to Tauri frontend via broadcast → emit bridge.
        RuntimeBridgeSpawner::spawn_update_event_bridge(&handle, &self.app_handle, &update_control);

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
            connection: ConnectionStatus {
                server_connected,
                llm_connected,
                cli_connected,
            },
            focus_mode,
            capture: CaptureContext {
                frame_processor: ipc_frame_processor,
                frame_storage: ipc_frame_storage,
                activity_monitor: ipc_activity_monitor,
                accessibility_extractor: ipc_accessibility_extractor,
                consent_manager: ipc_consent_manager,
                work_classifier: Some(Arc::new(
                    oneshim_vision::work_classifier::RuleBasedClassifier,
                )),
            },
            suggestion_manager,
            session_manager,
        });
        #[cfg(feature = "server")]
        let state_builder = server_context.configure_state_builder(state_builder);

        Ok(AppRuntimeLaunchResult {
            frontend_web_port,
            state_builder,
        })
    }
}
