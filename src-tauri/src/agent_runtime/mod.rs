mod analysis_setup;
mod embedding_setup;
mod sync_setup;

use anyhow::Result;
use oneshim_core::config::AppConfig;
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::consent::ConsentManager;
use oneshim_core::ports::calibration_store::{CalibrationReader, CalibrationWriter};
use oneshim_core::ports::storage::StorageService;
#[cfg(feature = "server")]
use oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator;
use oneshim_web::RealtimeEvent;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, watch};
use tracing::{error, info};

use crate::agent_runtime_support::AgentSupportContextBuilder;
use crate::focus_analyzer::FocusStorage;
use crate::scheduler::shared_regime_state::SharedRegimeState;
use crate::scheduler::{Scheduler, SchedulerStorage};

#[derive(Clone)]
pub(crate) struct AgentRuntimeBundle {
    storage: Arc<dyn StorageService>,
    scheduler_storage: Arc<dyn SchedulerStorage>,
    focus_storage: Arc<dyn FocusStorage>,
    calibration_writer: Option<Arc<dyn CalibrationWriter>>,
    calibration_reader: Option<Arc<dyn CalibrationReader>>,
    override_store: Option<Arc<dyn oneshim_core::ports::override_store::OverrideStore>>,
    /// Shared flag for on-demand re-clustering requests from Tauri/REST.
    recluster_requested: Arc<std::sync::atomic::AtomicBool>,
    /// Pre-built VectorStore for the embedding pipeline (None if embedding disabled).
    vector_store: Option<Arc<dyn oneshim_core::ports::vector_store::VectorStore>>,
    data_dir: PathBuf,
    config: AppConfig,
    config_manager: ConfigManager,
    consent_manager: Option<Arc<ConsentManager>>,
    /// Concrete SQLite storage for sync engine wiring.
    sqlite_storage_concrete: Arc<oneshim_storage::sqlite::SqliteStorage>,
    offline_mode: bool,
    event_tx: Option<broadcast::Sender<RealtimeEvent>>,
    #[cfg(feature = "server")]
    oauth_coordinator: Option<Arc<TokenRefreshCoordinator>>,
    app_handle: AppHandle,
    coaching_engine: Option<Arc<oneshim_analysis::CoachingEngine>>,
    coaching_storage: Option<Arc<oneshim_storage::sqlite::SqliteStorage>>,
    magic_overlay: Option<crate::magic_overlay::MagicOverlayHandle>,
    overlay_driver: Option<Arc<dyn oneshim_core::ports::overlay_driver::OverlayDriver>>,
    capture_paused: Option<Arc<std::sync::atomic::AtomicBool>>,
    detection_active: Option<Arc<std::sync::atomic::AtomicBool>>,
    server_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    llm_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    cli_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    server_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    llm_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    cli_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    tray_app_handle: Option<tauri::AppHandle>,
    #[cfg(feature = "server")]
    suggestion_receiver: Option<Arc<oneshim_suggestion::receiver::SuggestionReceiver>>,
    suggestions_enabled: bool,
    focus_mode: Option<Arc<crate::focus_mode::FocusModeState>>,
    /// Shared suggestion queue — SAME Arc as SuggestionManager's queue,
    /// so SSE-received suggestions are visible in IPC queries.
    shared_suggestion_queue:
        Option<Arc<tokio::sync::Mutex<oneshim_suggestion::queue::SuggestionQueue>>>,
    /// SharedRegimeState passed through to the Scheduler so it shares the same
    /// instance as the SessionManager's context assembler.
    shared_regime: Option<Arc<SharedRegimeState>>,
}

impl AgentRuntimeBundle {
    pub(crate) fn spawn_on(&self, handle: &Handle, shutdown_rx: watch::Receiver<bool>) {
        let bundle = self.clone();
        handle.spawn(async move {
            if let Err(error) = bundle.run(shutdown_rx).await {
                error!(error = %error, "Agent error");
            }
        });
    }

    async fn run(self, shutdown_rx: watch::Receiver<bool>) -> Result<()> {
        info!("Agent initializing");
        let mut builder = AgentSupportContextBuilder::new(
            &self.data_dir,
            &self.config,
            self.focus_storage.clone(),
        )
        .with_storage(self.storage.clone())
        .with_app_handle(self.app_handle.clone());
        if let Some(ref shared_queue) = self.shared_suggestion_queue {
            builder = builder.with_shared_suggestion_queue(shared_queue.clone());
        }
        let support = builder.build().await?;

        let mut scheduler = Scheduler::new(
            support.scheduler_config,
            support.system_monitor,
            support.activity_monitor,
            support.process_monitor,
            support.capture_trigger,
            support.frame_processor,
            self.storage,
            self.scheduler_storage,
            Some(support.frame_storage),
            support.batch_sink_opt,
            support.api_client_opt,
        )
        .with_config_manager(self.config_manager)
        .with_notification_manager(support.notification_manager)
        .with_focus_analyzer(support.focus_analyzer);

        if let Some(analyzer) = support.context_analyzer {
            scheduler = scheduler.with_context_analyzer(analyzer);
        }

        #[cfg(feature = "server")]
        if let Some(coordinator) = self.oauth_coordinator {
            scheduler = scheduler.with_oauth_coordinator(coordinator);
        }

        if let Some(event_tx) = self.event_tx {
            scheduler = scheduler.with_event_tx(event_tx);
        }

        // --- Layer 2: Build embedding + LLM summary pipeline ---
        let mut embedding =
            embedding_setup::build_embedding_components(&self.config, self.vector_store.clone());

        // Wire embedding provider + vector store into scheduler if available.
        if let (Some(ref vs), Some(ref ep)) =
            (&embedding.vector_store, &embedding.embedding_provider)
        {
            scheduler = scheduler
                .with_vector_store(vs.clone())
                .with_embedding_provider(ep.clone());
        }

        // --- Layer 3: Tiered-memory analysis pipeline ---
        let analysis = analysis_setup::build_analysis_pipeline(
            &self.config,
            &self.consent_manager,
            self.calibration_writer,
            self.calibration_reader,
            self.override_store.clone(),
            self.recluster_requested.clone(),
            &mut embedding,
        );
        if let Some(state) = analysis.adaptive_trigger_state {
            scheduler = scheduler.with_adaptive_trigger(state);
        }

        // --- Cross-device sync engine ---
        let sync = sync_setup::build_sync_engine(
            &self.config,
            &self.data_dir,
            &self.sqlite_storage_concrete,
            self.consent_manager.clone(),
        )
        .await;
        if let Some(sync_engine) = sync.sync_engine {
            scheduler = scheduler.with_sync_engine(sync_engine);
        }

        // --- Phase 3: Wire ConsentManager into scheduler for runtime consent checks ---
        if let Some(ref cm) = self.consent_manager {
            scheduler = scheduler.with_consent_manager(cm.clone());
        }

        // --- Coaching engine + storage + overlay wiring ---
        if let Some(engine) = self.coaching_engine {
            scheduler = scheduler.with_coaching_engine(engine);
        }
        if let Some(coaching_storage) = self.coaching_storage {
            scheduler = scheduler.with_coaching_storage(coaching_storage);
        }
        if let Some(overlay) = self.magic_overlay {
            scheduler = scheduler.with_magic_overlay(overlay);
        }
        if let Some(driver) = self.overlay_driver {
            scheduler = scheduler.with_overlay_driver(driver);
        }
        if let Some(capture_paused) = self.capture_paused {
            scheduler = scheduler.with_capture_paused(capture_paused);
        }
        if let Some(detection_active) = self.detection_active {
            scheduler = scheduler.with_detection_active(detection_active);
        }

        // --- Focus mode state for coaching/notification suppression (A4) ---
        if let Some(focus_mode) = self.focus_mode {
            scheduler = scheduler.with_focus_mode(focus_mode);
        }

        // --- SharedRegimeState: thread through to scheduler for single-instance sharing ---
        if let Some(shared_regime) = self.shared_regime {
            scheduler = scheduler.with_shared_regime(shared_regime);
        }

        // --- Analysis provider for coaching LLM personalization ---
        #[cfg(feature = "analysis")]
        if let Some(ref llm_api) = self.config.ai_provider.llm_api {
            let provider: Arc<dyn oneshim_core::ports::analysis_provider::AnalysisProvider> =
                Arc::new(oneshim_network::analysis_client::AnalysisClient::new(
                    llm_api,
                ));
            scheduler = scheduler.with_analysis_provider(provider);
        }

        // --- Health check flags ---
        if let (Some(s), Some(l), Some(c)) = (
            self.server_health_flag,
            self.llm_health_flag,
            self.cli_health_flag,
        ) {
            scheduler = scheduler.with_health_flags(s, l, c);
        }
        if let (Some(s), Some(l), Some(c)) = (
            self.server_connected,
            self.llm_connected,
            self.cli_connected,
        ) {
            scheduler = scheduler.with_connection_flags(s, l, c);
        }
        if let Some(handle) = self.tray_app_handle {
            scheduler = scheduler.with_tray_app_handle(handle);
        }

        // --- Suggestion reception ---
        scheduler = scheduler.with_suggestions_enabled(self.suggestions_enabled);
        // Prefer receiver from support context (built with SSE client);
        // fall back to externally-injected receiver if present.
        #[cfg(feature = "server")]
        {
            let receiver = support.suggestion_receiver.or(self.suggestion_receiver);
            if let Some(receiver) = receiver {
                scheduler = scheduler.with_suggestion_receiver(receiver);
            }
        }

        // --- Phase 2: Accessibility extractor (gated by config + consent) ---
        {
            let text_config = self.config.analysis.text_intelligence.clone();
            let ax_consent_ok = self
                .consent_manager
                .as_ref()
                .and_then(|cm| cm.current_consent())
                .map(|c| c.permissions.activity_pattern_learning)
                .unwrap_or(false);

            if text_config.enabled && text_config.accessibility_extraction && ax_consent_ok {
                if let Some(extractor) = oneshim_vision::accessibility::create_extractor() {
                    info!(
                        name = extractor.name(),
                        "Accessibility extractor enabled (Phase 2)"
                    );
                    scheduler = scheduler.with_accessibility_extractor(extractor);
                }
            }
        }

        info!("Agent started (offline={})", self.offline_mode);
        scheduler.run(shutdown_rx, Some(self.app_handle)).await;
        info!("Agent ended");
        Ok(())
    }
}

pub(crate) struct AgentRuntimeBuilder<'a> {
    storage: Arc<dyn StorageService>,
    scheduler_storage: Arc<dyn SchedulerStorage>,
    focus_storage: Arc<dyn FocusStorage>,
    calibration_writer: Option<Arc<dyn CalibrationWriter>>,
    calibration_reader: Option<Arc<dyn CalibrationReader>>,
    override_store: Option<Arc<dyn oneshim_core::ports::override_store::OverrideStore>>,
    recluster_requested: Arc<std::sync::atomic::AtomicBool>,
    vector_store: Option<Arc<dyn oneshim_core::ports::vector_store::VectorStore>>,
    data_dir: &'a Path,
    config: &'a AppConfig,
    config_manager: ConfigManager,
    consent_manager: Option<Arc<ConsentManager>>,
    /// Concrete SQLite storage for sync engine wiring.
    sqlite_storage_concrete: Arc<oneshim_storage::sqlite::SqliteStorage>,
    offline_mode: bool,
    event_tx: Option<broadcast::Sender<RealtimeEvent>>,
    #[cfg(feature = "server")]
    oauth_coordinator: Option<Arc<TokenRefreshCoordinator>>,
    app_handle: AppHandle,
    coaching_engine: Option<Arc<oneshim_analysis::CoachingEngine>>,
    coaching_storage: Option<Arc<oneshim_storage::sqlite::SqliteStorage>>,
    magic_overlay: Option<crate::magic_overlay::MagicOverlayHandle>,
    overlay_driver: Option<Arc<dyn oneshim_core::ports::overlay_driver::OverlayDriver>>,
    capture_paused: Option<Arc<std::sync::atomic::AtomicBool>>,
    detection_active: Option<Arc<std::sync::atomic::AtomicBool>>,
    server_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    llm_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    cli_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    server_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    llm_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    cli_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    tray_app_handle: Option<tauri::AppHandle>,
    #[cfg(feature = "server")]
    suggestion_receiver: Option<Arc<oneshim_suggestion::receiver::SuggestionReceiver>>,
    suggestions_enabled: bool,
    focus_mode: Option<Arc<crate::focus_mode::FocusModeState>>,
    /// Shared suggestion queue — passed through to AgentSupportContextBuilder
    /// so the SuggestionReceiver uses the same queue as SuggestionManager.
    shared_suggestion_queue:
        Option<Arc<tokio::sync::Mutex<oneshim_suggestion::queue::SuggestionQueue>>>,
    /// SharedRegimeState — passed through to the Scheduler so it shares the same
    /// instance as the SessionManager's context assembler.
    shared_regime: Option<Arc<SharedRegimeState>>,
}

impl<'a> AgentRuntimeBuilder<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        storage: Arc<dyn StorageService>,
        scheduler_storage: Arc<dyn SchedulerStorage>,
        focus_storage: Arc<dyn FocusStorage>,
        sqlite_storage_concrete: Arc<oneshim_storage::sqlite::SqliteStorage>,
        data_dir: &'a Path,
        config: &'a AppConfig,
        config_manager: ConfigManager,
        recluster_requested: Arc<std::sync::atomic::AtomicBool>,
        app_handle: AppHandle,
    ) -> Self {
        Self {
            storage,
            scheduler_storage,
            focus_storage,
            calibration_writer: None,
            calibration_reader: None,
            override_store: None,
            recluster_requested,
            vector_store: None,
            sqlite_storage_concrete,
            data_dir,
            config,
            config_manager,
            consent_manager: None,
            offline_mode: false,
            event_tx: None,
            #[cfg(feature = "server")]
            oauth_coordinator: None,
            app_handle,
            coaching_engine: None,
            coaching_storage: None,
            magic_overlay: None,
            overlay_driver: None,
            capture_paused: None,
            detection_active: None,
            server_health_flag: None,
            llm_health_flag: None,
            cli_health_flag: None,
            server_connected: None,
            llm_connected: None,
            cli_connected: None,
            tray_app_handle: None,
            #[cfg(feature = "server")]
            suggestion_receiver: None,
            suggestions_enabled: false,
            focus_mode: None,
            shared_suggestion_queue: None,
            shared_regime: None,
        }
    }

    pub(crate) fn with_focus_mode(
        mut self,
        focus_mode: Arc<crate::focus_mode::FocusModeState>,
    ) -> Self {
        self.focus_mode = Some(focus_mode);
        self
    }

    pub(crate) fn with_calibration_writer(mut self, writer: Arc<dyn CalibrationWriter>) -> Self {
        self.calibration_writer = Some(writer);
        self
    }

    pub(crate) fn with_calibration_reader(mut self, reader: Arc<dyn CalibrationReader>) -> Self {
        self.calibration_reader = Some(reader);
        self
    }

    pub(crate) fn with_override_store(
        mut self,
        store: Arc<dyn oneshim_core::ports::override_store::OverrideStore>,
    ) -> Self {
        self.override_store = Some(store);
        self
    }

    pub(crate) fn with_vector_store(
        mut self,
        store: Arc<dyn oneshim_core::ports::vector_store::VectorStore>,
    ) -> Self {
        self.vector_store = Some(store);
        self
    }

    pub(crate) fn with_consent_manager(mut self, cm: Arc<ConsentManager>) -> Self {
        self.consent_manager = Some(cm);
        self
    }

    pub(crate) fn with_offline_mode(mut self, offline_mode: bool) -> Self {
        self.offline_mode = offline_mode;
        self
    }

    pub(crate) fn with_event_tx(
        mut self,
        event_tx: Option<broadcast::Sender<RealtimeEvent>>,
    ) -> Self {
        self.event_tx = event_tx;
        self
    }

    #[cfg(feature = "server")]
    pub(crate) fn with_oauth_coordinator(
        mut self,
        oauth_coordinator: Option<Arc<TokenRefreshCoordinator>>,
    ) -> Self {
        self.oauth_coordinator = oauth_coordinator;
        self
    }

    pub(crate) fn with_coaching_engine(
        mut self,
        engine: Arc<oneshim_analysis::CoachingEngine>,
    ) -> Self {
        self.coaching_engine = Some(engine);
        self
    }

    pub(crate) fn with_coaching_storage(
        mut self,
        storage: Arc<oneshim_storage::sqlite::SqliteStorage>,
    ) -> Self {
        self.coaching_storage = Some(storage);
        self
    }

    pub(crate) fn with_magic_overlay(
        mut self,
        overlay: crate::magic_overlay::MagicOverlayHandle,
    ) -> Self {
        self.magic_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_overlay_driver(
        mut self,
        driver: Arc<dyn oneshim_core::ports::overlay_driver::OverlayDriver>,
    ) -> Self {
        self.overlay_driver = Some(driver);
        self
    }

    pub(crate) fn with_capture_paused(mut self, flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.capture_paused = Some(flag);
        self
    }

    pub(crate) fn with_detection_active(
        mut self,
        flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        self.detection_active = Some(flag);
        self
    }

    pub(crate) fn with_health_flags(
        mut self,
        server: Arc<std::sync::atomic::AtomicBool>,
        llm: Arc<std::sync::atomic::AtomicBool>,
        cli: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        self.server_health_flag = Some(server);
        self.llm_health_flag = Some(llm);
        self.cli_health_flag = Some(cli);
        self
    }

    pub(crate) fn with_connection_flags(
        mut self,
        server: Arc<std::sync::atomic::AtomicBool>,
        llm: Arc<std::sync::atomic::AtomicBool>,
        cli: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        self.server_connected = Some(server);
        self.llm_connected = Some(llm);
        self.cli_connected = Some(cli);
        self
    }

    pub(crate) fn with_tray_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.tray_app_handle = Some(handle);
        self
    }

    #[cfg(feature = "server")]
    #[allow(dead_code)] // retained for external injection; support context is the primary path
    pub(crate) fn with_suggestion_receiver(
        mut self,
        receiver: Arc<oneshim_suggestion::receiver::SuggestionReceiver>,
    ) -> Self {
        self.suggestion_receiver = Some(receiver);
        self
    }

    pub(crate) fn with_suggestions_enabled(mut self, enabled: bool) -> Self {
        self.suggestions_enabled = enabled;
        self
    }

    #[allow(dead_code)] // used when feature = "server"
    pub(crate) fn with_shared_suggestion_queue(
        mut self,
        queue: Arc<tokio::sync::Mutex<oneshim_suggestion::queue::SuggestionQueue>>,
    ) -> Self {
        self.shared_suggestion_queue = Some(queue);
        self
    }

    pub(crate) fn with_shared_regime(mut self, regime: Arc<SharedRegimeState>) -> Self {
        self.shared_regime = Some(regime);
        self
    }

    pub(crate) fn build(self) -> AgentRuntimeBundle {
        AgentRuntimeBundle {
            storage: self.storage,
            scheduler_storage: self.scheduler_storage,
            focus_storage: self.focus_storage,
            calibration_writer: self.calibration_writer,
            calibration_reader: self.calibration_reader,
            override_store: self.override_store,
            recluster_requested: self.recluster_requested,
            vector_store: self.vector_store,
            data_dir: self.data_dir.to_path_buf(),
            config: self.config.clone(),
            config_manager: self.config_manager,
            consent_manager: self.consent_manager,
            sqlite_storage_concrete: self.sqlite_storage_concrete,
            offline_mode: self.offline_mode,
            event_tx: self.event_tx,
            #[cfg(feature = "server")]
            oauth_coordinator: self.oauth_coordinator,
            app_handle: self.app_handle,
            coaching_engine: self.coaching_engine,
            coaching_storage: self.coaching_storage,
            magic_overlay: self.magic_overlay,
            overlay_driver: self.overlay_driver,
            capture_paused: self.capture_paused,
            detection_active: self.detection_active,
            server_health_flag: self.server_health_flag,
            llm_health_flag: self.llm_health_flag,
            cli_health_flag: self.cli_health_flag,
            server_connected: self.server_connected,
            llm_connected: self.llm_connected,
            cli_connected: self.cli_connected,
            tray_app_handle: self.tray_app_handle,
            #[cfg(feature = "server")]
            suggestion_receiver: self.suggestion_receiver,
            suggestions_enabled: self.suggestions_enabled,
            focus_mode: self.focus_mode,
            shared_suggestion_queue: self.shared_suggestion_queue,
            shared_regime: self.shared_regime,
        }
    }
}
