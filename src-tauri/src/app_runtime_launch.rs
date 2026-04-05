use anyhow::Result;
use oneshim_core::consent::ConsentManager;
use oneshim_core::ports::coaching_storage::CoachingStoragePort;
use oneshim_core::ports::session_context_store::SessionContextStorePort;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::agent_runtime::AgentRuntimeBuilder;
use crate::bootstrap_runtime::BootstrapRuntimeBundle;
use crate::capture_services::SharedCaptureServices;
use crate::launch_resources::LaunchCoreResourcesBuilder;
use crate::magic_overlay::MagicOverlayHandle;
use crate::runtime_bridges::RuntimeBridgeSpawner;
use crate::runtime_state::{
    AiSessionRuntimeState, AppState, AudioContext, AudioRuntimeState, CaptureContext,
    ConfigRuntimeState, ConnectionStatus, DetectionRuntimeState, ManagedStateBuilder,
    SuggestionRuntimeState,
};
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
            background_runtime,
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
        let encryption_key = core_resources.storage_runtime.encryption_key.clone();
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
        let detection_active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Connection status flags — start disconnected, updated by health check loop.
        let server_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let llm_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cli_connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Focus mode state — transient, not persisted across restarts.
        let focus_mode = Arc::new(crate::focus_mode::FocusModeState::new());

        // Shared capture services are reused by scheduler and IPC commands so capture
        // semantics stay aligned across background monitoring and ad-hoc user actions.
        let shared_capture_services = match handle.block_on(SharedCaptureServices::build(
            &data_dir_path,
            &config,
            encryption_key.clone(),
        )) {
            Ok(services) => Some(Arc::new(services)),
            Err(error) => {
                tracing::warn!("shared capture services init failed: {error}");
                None
            }
        };
        let capture_consent_manager = shared_capture_services
            .as_ref()
            .map(|services| services.consent_manager.clone())
            .unwrap_or_else(|| Arc::new(ConsentManager::new(data_dir_path.join("consent.json"))));

        // SuggestionManager for overlay panel (A3).
        // The queue Arc is created here and passed to BOTH the SuggestionManager
        // and the agent runtime (which builds SuggestionReceiver). This ensures
        // SSE-received suggestions appear in IPC queries via get_pending_suggestions.
        #[cfg(feature = "server")]
        let shared_suggestion_queue = Arc::new(tokio::sync::Mutex::new(
            oneshim_suggestion::queue::SuggestionQueue::new(config.analysis.max_suggestions),
        ));

        // Restore pending suggestions from SQLite into the queue.
        #[cfg(feature = "server")]
        {
            let pending = sqlite_storage
                .list_suggestions_by_state("pending", 50)
                .unwrap_or_default();
            if !pending.is_empty() {
                let mut queue = handle.block_on(shared_suggestion_queue.lock());
                let mut restored = 0usize;
                for record in pending {
                    if let Some(suggestion) = record.try_into_suggestion() {
                        if queue.push(suggestion) {
                            restored += 1;
                        }
                    }
                }
                if restored > 0 {
                    tracing::info!(count = restored, "restored suggestions from storage");
                }
            }
        }

        #[cfg(feature = "server")]
        let shared_scorer = Arc::new(tokio::sync::Mutex::new(
            oneshim_suggestion::scorer::FeedbackScorer::new(),
        ));
        #[cfg(feature = "server")]
        let suggestion_manager: Option<Arc<crate::suggestion_manager::SuggestionManager>> = {
            use oneshim_network::auth::TokenManager;
            use oneshim_network::http_client::HttpApiClient;

            let token_manager = Arc::new(
                TokenManager::new_with_tls(
                    &config.server.base_url,
                    &config.tls,
                    Some(config.request_timeout()),
                )
                .unwrap_or_else(|_| TokenManager::new(&config.server.base_url)),
            );

            #[cfg(feature = "grpc")]
            let api_result: Result<
                Arc<dyn oneshim_core::ports::api_client::ApiClient>,
                _,
            > = {
                use oneshim_network::grpc::{GrpcApiAdapter, GrpcConfig, UnifiedClient};
                let grpc_config = GrpcConfig::from_core_with_rest_tls(
                    &config.grpc,
                    &config.server.base_url,
                    &config.tls,
                );
                match (
                    UnifiedClient::new(grpc_config, token_manager.clone()),
                    HttpApiClient::new_with_tls(
                        &config.server.base_url,
                        token_manager.clone(),
                        config.request_timeout(),
                        &config.tls,
                    ),
                ) {
                    (Ok(unified), Ok(http_fallback)) => Ok(Arc::new(GrpcApiAdapter::new(
                        Arc::new(unified),
                        http_fallback,
                    ))),
                    (Err(e), _) => Err(anyhow::anyhow!("UnifiedClient init failed: {e}")),
                    (_, Err(e)) => Err(anyhow::anyhow!("HttpApiClient init failed: {e}")),
                }
            };

            #[cfg(not(feature = "grpc"))]
            let api_result: Result<
                Arc<dyn oneshim_core::ports::api_client::ApiClient>,
                _,
            > = {
                HttpApiClient::new_with_tls(
                    &config.server.base_url,
                    token_manager,
                    config.request_timeout(),
                    &config.tls,
                )
                .map(|c| Arc::new(c) as Arc<dyn oneshim_core::ports::api_client::ApiClient>)
                .map_err(|e| anyhow::anyhow!("{e}"))
            };

            match api_result {
                Ok(api) => {
                    let history = Arc::new(tokio::sync::Mutex::new(
                        oneshim_suggestion::history::SuggestionHistory::new(100),
                    ));
                    let feedback = Arc::new(oneshim_suggestion::feedback::FeedbackSender::new(api));
                    let deferred = Arc::new(tokio::sync::Mutex::new(
                        oneshim_suggestion::deferred::DeferredManager::new(50),
                    ));
                    let retry_queue = Arc::new(tokio::sync::Mutex::new(
                        oneshim_suggestion::feedback_retry::FeedbackRetryQueue::new(100, 5),
                    ));
                    Some(Arc::new(crate::suggestion_manager::SuggestionManager::new(
                        shared_suggestion_queue.clone(),
                        history,
                        feedback,
                        shared_scorer.clone(),
                        deferred,
                        retry_queue,
                        sqlite_storage.clone(),
                    )))
                }
                Err(e) => {
                    tracing::warn!("SuggestionManager init skipped: {e}");
                    None
                }
            }
        };
        #[cfg(not(feature = "server"))]
        let suggestion_manager: Option<Arc<crate::suggestion_manager::SuggestionManager>> = None;

        // Restore deferred suggestions and pending feedbacks from SQLite.
        #[cfg(feature = "server")]
        if let Some(ref mgr) = suggestion_manager {
            // A. Deferred suggestions → DeferredManager or queue (if already due)
            let deferred_records = sqlite_storage
                .list_suggestions_by_state("deferred", 50)
                .unwrap_or_default();
            if !deferred_records.is_empty() {
                let total = deferred_records.len();
                let entries: Vec<_> = deferred_records
                    .into_iter()
                    .filter_map(|record| {
                        let resurface_at = record
                            .resurface_at
                            .as_ref()
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))?;
                        let created_at = chrono::DateTime::parse_from_rfc3339(&record.created_at)
                            .ok()
                            .map(|dt| dt.with_timezone(&chrono::Utc))?;
                        let suggestion = record.try_into_suggestion()?;
                        Some((suggestion, created_at, resurface_at))
                    })
                    .collect();
                if entries.len() < total {
                    tracing::warn!(
                        dropped = total - entries.len(),
                        "skipped malformed deferred records"
                    );
                }

                let mut deferred_mgr = handle.block_on(mgr.deferred().lock());
                let already_due = deferred_mgr.restore(entries);
                let deferred_count = deferred_mgr.pending_count();
                drop(deferred_mgr);

                if !already_due.is_empty() {
                    let mut queue = handle.block_on(shared_suggestion_queue.lock());
                    for s in already_due {
                        queue.push(s);
                    }
                }
                if deferred_count > 0 {
                    tracing::info!(count = deferred_count, "restored deferred suggestions");
                }
            }

            // B. Pending feedbacks → FeedbackRetryQueue
            // Note: enqueue() recalculates next_retry_at from the attempt count,
            // so the persisted schedule is not honored exactly. This is acceptable
            // — SQLite is the durability guarantee, in-memory queue is best-effort.
            let pending_feedbacks = sqlite_storage
                .list_pending_feedbacks(100)
                .unwrap_or_default();
            if !pending_feedbacks.is_empty() {
                let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
                let mut rq = handle.block_on(mgr.retry_queue().lock());
                let mut fb_count = 0usize;
                for record in pending_feedbacks {
                    // Skip orphaned rows older than 7 days
                    if record.created_at < cutoff {
                        let _ = sqlite_storage.delete_pending_feedback(&record.suggestion_id);
                        continue;
                    }
                    if let Some((sid, ft, comment, attempts, next_retry)) =
                        record.into_domain_parts()
                    {
                        rq.enqueue(oneshim_suggestion::feedback_retry::PendingFeedback {
                            suggestion_id: sid,
                            feedback_type: ft,
                            comment,
                            attempts,
                            next_retry_at: next_retry,
                        });
                        fb_count += 1;
                    }
                }
                if fb_count > 0 {
                    tracing::info!(count = fb_count, "restored pending feedbacks for retry");
                }
            }
        }

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
        let coaching_storage: Arc<dyn CoachingStoragePort> = sqlite_storage.clone();

        // Create MagicOverlay handle (window created at startup in setup.rs)
        let magic_overlay =
            MagicOverlayHandle::new(self.app_handle.clone(), config.coaching.overlay_mode);

        // Shared SharedRegimeState — single instance used by both SessionManager (context
        // assembler) and Scheduler (monitor/coaching loops). Created before both consumers.
        let shared_regime_state = Arc::new(SharedRegimeState::new());

        // Obtain shutdown receiver for idle reaper before core_resources is consumed.
        let reaper_shutdown_rx = core_resources.background_runtime.shutdown_rx();

        let agent_runtime = {
            let mut builder = AgentRuntimeBuilder::new(
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
            ));
            if let Some(ref capture_services) = shared_capture_services {
                builder = builder.with_shared_capture_services(capture_services.clone());
            }
            let builder = builder
                .with_offline_mode(false)
                .with_event_tx(
                    core_resources
                        .background_runtime
                        .agent_event_tx(config.web.enabled),
                )
                .with_calibration_writer(sqlite_storage.clone())
                .with_calibration_reader(sqlite_storage.clone())
                .with_override_store(sqlite_storage.clone())
                .with_consent_manager(capture_consent_manager.clone())
                .with_coaching_engine(coaching_engine.clone())
                .with_coaching_storage(coaching_storage.clone())
                .with_magic_overlay(magic_overlay.clone())
                .with_overlay_driver(Arc::new(
                    crate::magic_overlay_driver::MagicOverlayDriver::new(self.app_handle.clone()),
                ))
                .with_capture_paused(capture_paused.clone())
                .with_detection_active(detection_active.clone())
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
            let builder = builder
                .with_shared_suggestion_queue(shared_suggestion_queue)
                .with_shared_scorer(shared_scorer);
            #[cfg(feature = "server")]
            let builder = if let Some(ref mgr) = suggestion_manager {
                builder.with_suggestion_manager(mgr.clone())
            } else {
                builder
            };
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
            let storage_for_audit = sqlite_storage.clone();
            let persistence_cb: Arc<dyn oneshim_automation::audit::AuditPersistence> =
                Arc::new(move |entry: &oneshim_core::models::audit::AuditEntry| {
                    storage_for_audit.save_audit_entry(entry);
                });
            let audit_logger = Arc::new(tokio::sync::RwLock::new(
                oneshim_automation::audit::AuditLogger::new(500, 50)
                    .with_persistence(persistence_cb),
            ));
            let audit_port: Arc<dyn oneshim_core::ports::audit_log::AuditLogPort> = Arc::new(
                oneshim_automation::audit::AuditLogAdapter::new(audit_logger),
            );

            let session_config = Arc::new(config.ai_session.clone());
            let idle_reaper_interval =
                std::time::Duration::from_secs(session_config.health_check_interval_secs);
            let session_context_store: Arc<dyn SessionContextStorePort> = sqlite_storage.clone();

            let context_assembler = Arc::new(SessionContextAssembler::new(
                session_context_store,
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
            manager = manager.with_app_handle(self.app_handle.clone());
            Some((Arc::new(manager), idle_reaper_interval))
        };

        // Spawn idle reaper background task — periodically calls reap_idle_sessions
        // to transition Active→Idle→Terminated for sessions that exceed the idle timeout.
        if let Some((ref sm, idle_reaper_interval)) = session_manager {
            let sm_clone = sm.clone();
            let ss_clone: Arc<dyn oneshim_core::ports::session_storage::SessionStoragePort> =
                sqlite_storage.clone();
            let retention_days = config.ai_session.audit_retention_days;
            let mut shutdown_rx = reaper_shutdown_rx;
            handle.spawn(async move {
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(idle_reaper_interval) => {
                            sm_clone.reap_idle_sessions().await;
                            // Purge expired persisted sessions
                            if let Ok(count) = ss_clone.purge_expired(retention_days).await {
                                if count > 0 {
                                    tracing::info!("purged {count} expired session records");
                                }
                            }
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
            if let Some((ref sm, _)) = session_manager {
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

        // Audio capture and STT engine — wired when audio feature + config enabled.
        let model_dir: std::path::PathBuf = self
            .app_handle
            .path()
            .app_data_dir()
            .map(|d| d.join("models"))
            .unwrap_or_else(|_| std::path::PathBuf::from("models"));

        let audio_capture: Option<Arc<dyn oneshim_core::ports::audio_capture::AudioCapturePort>> = {
            #[cfg(feature = "audio")]
            {
                if config.audio.enabled {
                    Some(Arc::new(oneshim_audio::AudioCapture::new()))
                } else {
                    tracing::debug!("audio capture disabled by config");
                    None
                }
            }
            #[cfg(not(feature = "audio"))]
            {
                None
            }
        };

        let stt_engine: Option<Arc<dyn oneshim_core::ports::stt_provider::SttProvider>> = {
            #[cfg(feature = "stt")]
            {
                if !config.audio.enabled {
                    None
                } else {
                    // Try model_dir first (when download feature available), then legacy whisper_model_path
                    let model_path = if config.audio.whisper_model_path.is_empty() {
                        #[cfg(feature = "download")]
                        {
                            model_dir.join(oneshim_audio::model_downloader::model_filename(
                                config.audio.model_size,
                            ))
                        }
                        #[cfg(not(feature = "download"))]
                        {
                            self.app_handle
                                .path()
                                .resource_dir()
                                .map(|d| d.join("ggml-base.bin"))
                                .unwrap_or_default()
                        }
                    } else {
                        std::path::PathBuf::from(&config.audio.whisper_model_path)
                    };
                    if model_path.exists() {
                        match oneshim_audio::WhisperSttProvider::new(
                            &model_path,
                            config.audio.language,
                        ) {
                            Ok(provider) => {
                                tracing::info!("Whisper STT loaded: {}", model_path.display());
                                Some(Arc::new(provider) as _)
                            }
                            Err(e) => {
                                tracing::warn!("Failed to load Whisper model: {e}");
                                None
                            }
                        }
                    } else {
                        tracing::info!(
                            "Whisper model not found at {}; STT disabled",
                            model_path.display()
                        );
                        None
                    }
                }
            }
            #[cfg(not(feature = "stt"))]
            {
                None
            }
        };

        let model_downloader: Option<
            Arc<dyn oneshim_core::ports::model_downloader::ModelDownloader>,
        > = {
            #[cfg(feature = "download")]
            {
                Some(Arc::new(oneshim_audio::WhisperModelDownloader::new()))
            }
            #[cfg(not(feature = "download"))]
            {
                None
            }
        };

        let ai_session_runtime_state = AiSessionRuntimeState::new(
            session_manager.as_ref().map(|(sm, _)| sm.clone()),
            Some(sqlite_storage.clone()),
            config.ai_session.max_history_turns,
        );
        let audio_runtime_state = AudioRuntimeState::new(
            config_manager.clone(),
            AudioContext {
                capture: audio_capture,
                stt_engine: Arc::new(tokio::sync::RwLock::new(stt_engine)),
                model_downloader,
                model_dir,
                downloading: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                download_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                vad_state: Arc::new(parking_lot::Mutex::new("idle".into())),
            },
        );
        let config_runtime_state =
            ConfigRuntimeState::new(config_manager.clone(), web_port.clone());
        let suggestion_runtime_state =
            SuggestionRuntimeState::new(suggestion_manager.clone(), Some(magic_overlay.clone()));
        let detection_runtime_state = DetectionRuntimeState::new(
            detection_active.clone(),
            automation_controller
                .as_ref()
                .and_then(|controller| controller.scene_finder().cloned()),
            Some(magic_overlay.clone()),
        );

        let state_builder = ManagedStateBuilder::new(
            AppState {
                runtime_handle: handle,
                background_runtime,
                config,
                storage: sqlite_storage.clone(),
                update_control: Some(update_control),
                update_action_tx,
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
                    frame_processor: shared_capture_services
                        .as_ref()
                        .map(|services| services.frame_processor.clone()),
                    frame_storage: shared_capture_services
                        .as_ref()
                        .map(|services| services.frame_storage.clone()),
                    activity_monitor: shared_capture_services
                        .as_ref()
                        .map(|services| services.activity_monitor.clone()),
                    accessibility_extractor: shared_capture_services
                        .as_ref()
                        .and_then(|services| services.accessibility_extractor.clone()),
                    consent_manager: Some(capture_consent_manager),
                    work_classifier: Some(Arc::new(
                        oneshim_vision::work_classifier::RuleBasedClassifier,
                    )),
                },
            },
            config_runtime_state,
        )
        .with_ai_session_runtime(ai_session_runtime_state)
        .with_audio_runtime(audio_runtime_state)
        .with_suggestion_runtime(suggestion_runtime_state)
        .with_detection_runtime(detection_runtime_state);
        #[cfg(feature = "server")]
        let state_builder = server_context.configure_state_builder(state_builder);

        Ok(AppRuntimeLaunchResult {
            frontend_web_port,
            state_builder,
        })
    }
}
