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
use tracing::{error, info, warn};

use oneshim_core::config::SyncTransportKind;
use oneshim_core::error::CoreError;

use crate::agent_runtime_support::AgentSupportContextBuilder;
use crate::focus_analyzer::FocusStorage;
use crate::scheduler::{AdaptiveTriggerState, Scheduler, SchedulerStorage};
use crate::sync_engine::SyncEngine;

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
        let support = AgentSupportContextBuilder::new(
            &self.data_dir,
            &self.config,
            self.focus_storage.clone(),
        )
        .with_storage(self.storage.clone())
        .build()
        .await?;

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

        // --- Layer 2: Build embedding + LLM summary pipeline first ---
        // These are created early so they can be wired into AdaptiveTriggerState below.
        let mut embedding_pipeline_arc: Option<Arc<oneshim_analysis::EmbeddingPipeline>> = None;
        let mut llm_summarizer_arc: Option<Arc<oneshim_analysis::LlmSegmentSummarizer>> = None;

        if self.config.analysis.embedding.enabled {
            let embedding_config = &self.config.analysis.embedding;
            let pii_level = self.config.privacy.pii_filter_level;

            // Use pre-built VectorStore from the builder
            let vector_store_opt = self.vector_store.clone();

            // Create EmbeddingProvider based on config
            let embedding_provider: Option<
                Arc<dyn oneshim_core::ports::embedding_provider::EmbeddingProvider>,
            > = match embedding_config.provider {
                #[cfg(feature = "embedding")]
                oneshim_core::config::EmbeddingProviderType::Local => {
                    match oneshim_embedding::LocalEmbeddingProvider::new() {
                        Ok(provider) => {
                            info!("Local embedding provider initialized");
                            Some(Arc::new(provider))
                        }
                        Err(e) => {
                            warn!("Local embedding provider init failed: {e}");
                            None
                        }
                    }
                }
                #[cfg(not(feature = "embedding"))]
                oneshim_core::config::EmbeddingProviderType::Local => {
                    warn!("Local embedding requested but 'embedding' feature not enabled");
                    None
                }
                oneshim_core::config::EmbeddingProviderType::Remote => {
                    if let Some(ref endpoint) = embedding_config.remote_endpoint {
                        let api_key = self
                            .config
                            .ai_provider
                            .llm_api
                            .as_ref()
                            .map(|api| api.api_key.clone())
                            .unwrap_or_default();
                        Some(Arc::new(
                            oneshim_network::remote_embedding_client::RemoteEmbeddingProvider::new(
                                endpoint.clone(),
                                api_key,
                                "text-embedding-3-small".to_string(),
                                384,
                                30,
                            ),
                        ))
                    } else {
                        warn!("Remote embedding requested but no endpoint configured");
                        None
                    }
                }
            };

            if let (Some(ref provider), Some(ref vector_store)) =
                (&embedding_provider, &vector_store_opt)
            {
                let pii_filter_embed: oneshim_analysis::PiiFilter = Box::new(move |text: &str| {
                    oneshim_vision::privacy::sanitize_title_with_level(text, pii_level)
                });
                let skip_float32 = embedding_config.quantization_enabled
                    && !embedding_config.quantization_float32_retention;
                let pipeline =
                    Arc::new(oneshim_analysis::EmbeddingPipeline::with_float32_retention(
                        provider.clone(),
                        pii_filter_embed,
                        vector_store.clone(),
                        embedding_config.quantization_enabled,
                        skip_float32,
                    ));
                embedding_pipeline_arc = Some(pipeline);

                // Build LlmSegmentSummarizer if LLM summary is enabled
                if embedding_config.llm_summary_enabled {
                    if let Some(ref llm_api) = self.config.ai_provider.llm_api {
                        let analysis_provider: Arc<
                            dyn oneshim_core::ports::analysis_provider::AnalysisProvider,
                        > = Arc::new(oneshim_network::analysis_client::AnalysisClient::new(
                            llm_api,
                        ));
                        let pii_level_summ = self.config.privacy.pii_filter_level;
                        let pii_filter_summ: oneshim_analysis::PiiFilter =
                            Box::new(move |text: &str| {
                                oneshim_vision::privacy::sanitize_title_with_level(
                                    text,
                                    pii_level_summ,
                                )
                            });
                        let min_duration = embedding_config.min_segment_for_summary_secs;
                        llm_summarizer_arc =
                            Some(Arc::new(oneshim_analysis::LlmSegmentSummarizer::new(
                                analysis_provider,
                                pii_filter_summ,
                                true,
                                min_duration,
                            )));
                        info!("LLM segment summarizer enabled");
                    } else {
                        warn!("LLM summary enabled but no LLM provider configured");
                    }
                }

                scheduler = scheduler
                    .with_vector_store(vector_store.clone())
                    .with_embedding_provider(provider.clone());

                info!(
                    provider = provider.model_id(),
                    "Layer 2 embedding pipeline wired"
                );
            }
        }

        // Config validation: embedding requires tiered_memory
        if self.config.analysis.embedding.enabled && !self.config.analysis.tiered_memory.enabled {
            warn!("embedding.enabled requires tiered_memory.enabled — embedding will not function");
        }

        // Config validation: gui_intelligence requires tiered_memory
        if self.config.analysis.gui_intelligence.enabled
            && !self.config.analysis.tiered_memory.enabled
        {
            warn!("gui_intelligence.enabled requires tiered_memory.enabled — GUI pipeline will not function");
        }

        // Wire adaptive tiered-memory pipeline when enabled + consented.
        // Gate on activity_pattern_learning consent (GDPR Tier 4).
        let consent_ok = self
            .consent_manager
            .as_ref()
            .and_then(|cm| cm.current_consent())
            .map(|c| c.permissions.activity_pattern_learning)
            .unwrap_or(false);

        if self.config.analysis.tiered_memory.enabled && !consent_ok {
            info!("activity_pattern_learning consent not granted, skipping tiered memory");
        }

        if self.config.analysis.tiered_memory.enabled && consent_ok {
            if let (Some(calibration_writer), Some(calibration_reader)) =
                (self.calibration_writer, self.calibration_reader)
            {
                let preset = self.config.analysis.tiered_memory.preset;
                let params = preset.default_params();
                let buf_cap = self.config.analysis.tiered_memory.buffer_capacity;
                let tm_config = &self.config.analysis.tiered_memory;
                let state = AdaptiveTriggerState {
                    trigger: oneshim_analysis::AdaptiveTrigger::new(),
                    segment_buffer: oneshim_analysis::SegmentBuffer::new(buf_cap),
                    calibration_buffer: oneshim_analysis::CalibrationBuffer::new(buf_cap, 60),
                    title_bar_parser: oneshim_analysis::TitleBarParser::new(),
                    work_type_classifier: oneshim_analysis::WorkTypeClassifier::new(),
                    content_tracker: oneshim_analysis::ContentTracker::new(),
                    segment_summarizer: oneshim_analysis::SegmentSummarizer::new(),
                    params,
                    calibration_writer,
                    regime_classifier: oneshim_analysis::RegimeClassifier::new(1.5),
                    regime_manager: oneshim_analysis::RegimeManager::new(tm_config),
                    regime_detector: oneshim_analysis::RegimeDetector::new(),
                    param_resolver: oneshim_analysis::ParamResolver::new(preset),
                    calibration_reader,
                    current_regime_id: None,
                    last_detection_time: None,
                    ema_tracker: oneshim_analysis::auto_tuner::EmaStatsTracker::new(
                        tm_config.auto_tuning.ema_alpha,
                    ),
                    drift_detector: oneshim_analysis::auto_tuner::DriftDetector::new(
                        tm_config.auto_tuning.ema_alpha,
                        tm_config.auto_tuning.drift_threshold,
                    ),
                    auto_tune_tick_count: 0,
                    clustering_strategy: {
                        match tm_config.clustering_algorithm {
                            oneshim_core::config::ClusteringAlgorithm::Hdbscan => {
                                #[cfg(feature = "hdbscan")]
                                {
                                    Some(Box::new(
                                        oneshim_analysis::hdbscan_detector::HdbscanDetector::new(
                                            5, None,
                                        ),
                                    ))
                                }
                                #[cfg(not(feature = "hdbscan"))]
                                {
                                    warn!("HDBSCAN requested but not compiled; falling back to k-means");
                                    Some(Box::new(
                                        oneshim_analysis::kmeans_adapter::KmeansDetector::new(),
                                    ))
                                }
                            }
                            oneshim_core::config::ClusteringAlgorithm::Kmeans => Some(Box::new(
                                oneshim_analysis::kmeans_adapter::KmeansDetector::new(),
                            )),
                            oneshim_core::config::ClusteringAlgorithm::Gmm => {
                                Some(Box::new(oneshim_analysis::gmm_detector::GmmDetector::new()))
                            }
                        }
                    },
                    override_store: self.override_store.clone(),
                    recluster_requested: self.recluster_requested.clone(),
                    last_drift_detected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    llm_summarizer: llm_summarizer_arc,
                    embedding_pipeline: embedding_pipeline_arc,
                    gui_pipeline_state: None,
                    gui_work_type_refiner: oneshim_analysis::GuiWorkTypeRefiner,
                    app_registry: Arc::new(oneshim_core::app_registry::AppRegistry::new()),
                };
                scheduler = scheduler.with_adaptive_trigger(state);
                info!("Adaptive tiered-memory pipeline enabled");
            } else {
                info!("Tiered memory enabled but no calibration writer/reader — skipped");
            }
        }

        // --- Cross-device sync engine (P3 Phase 3b) ---
        if self.config.sync.enabled {
            let passphrase = std::env::var("ONESHIM_SYNC_PASSPHRASE").unwrap_or_default();
            if passphrase.is_empty() {
                warn!("sync enabled but ONESHIM_SYNC_PASSPHRASE not set; sync disabled");
            } else {
                match self
                    .sqlite_storage_concrete
                    .ensure_device_identity(&self.config.sync.device_name)
                {
                    Ok((device_id, device_name)) => {
                        let extractor =
                            Arc::new(oneshim_storage::sync_extractor::SqliteSyncExtractor::new(
                                self.sqlite_storage_concrete.connection_arc(),
                                device_id.clone(),
                                device_name.clone(),
                                self.config.sync.clone(),
                            ));
                        let merger = Arc::new(oneshim_storage::sync_merger::SqliteSyncMerger::new(
                            self.sqlite_storage_concrete.connection_arc(),
                            device_id.clone(),
                        ));

                        let transport_result: Result<Arc<dyn oneshim_core::ports::sync_transport::SyncTransport>, CoreError> =
                            match self.config.sync.transport {
                                SyncTransportKind::File => {
                                    match &self.config.sync.sync_folder {
                                        Some(folder) => {
                                            oneshim_storage::file_transport::FileSyncTransport::new(
                                                std::path::PathBuf::from(folder),
                                                device_id.clone(),
                                                passphrase.clone(),
                                            ).map(|t| Arc::new(t) as Arc<dyn oneshim_core::ports::sync_transport::SyncTransport>)
                                        }
                                        None => {
                                            warn!("sync transport=file but sync_folder not configured");
                                            Err(CoreError::Internal("sync_folder required for file transport".into()))
                                        }
                                    }
                                }
                                SyncTransportKind::Remote => {
                                    match &self.config.sync.remote_endpoint {
                                        Some(endpoint) => {
                                            // Retrieve auth credential from OS keychain
                                            let credential = keyring::Entry::new("oneshim", "sync_remote_token")
                                                .and_then(|entry| entry.get_password())
                                                .unwrap_or_default();
                                            if credential.is_empty() {
                                                warn!("sync transport=remote but no credential in keychain (key: oneshim/sync_remote_token)");
                                            }
                                            oneshim_network::sync::RemoteSyncTransport::new(
                                                endpoint.clone(),
                                                device_id.clone(),
                                                passphrase.clone(),
                                                self.config.sync.remote_auth.clone(),
                                                credential,
                                            ).map(|t| Arc::new(t) as Arc<dyn oneshim_core::ports::sync_transport::SyncTransport>)
                                        }
                                        None => {
                                            warn!("sync transport=remote but remote_endpoint not configured");
                                            Err(CoreError::Internal("remote_endpoint required for remote transport".into()))
                                        }
                                    }
                                }
                                SyncTransportKind::Lan => {
                                    #[cfg(feature = "lan-sync")]
                                    {
                                        let config_dir = self.data_dir.clone();
                                        match oneshim_network::sync::lan_tls::load_or_generate_cert(
                                            &config_dir,
                                            &device_id,
                                        ) {
                                            Ok((cert_pem, key_pem, fingerprint)) => {
                                                // Use block_on to await the async start in sync context
                                                match tokio::runtime::Handle::current().block_on(
                                                    oneshim_network::sync::LanSyncTransport::start(
                                                        device_id.clone(),
                                                        device_name.clone(),
                                                        passphrase.clone(),
                                                        cert_pem,
                                                        key_pem,
                                                        fingerprint,
                                                        self.config.sync.lan_port,
                                                        self.config.sync.lan_advertise,
                                                    ),
                                                ) {
                                                    Ok(t) => Ok(Arc::new(t) as Arc<dyn oneshim_core::ports::sync_transport::SyncTransport>),
                                                    Err(e) => Err(e),
                                                }
                                            }
                                            Err(e) => Err(e),
                                        }
                                    }
                                    #[cfg(not(feature = "lan-sync"))]
                                    {
                                        warn!("LAN sync requires 'lan-sync' feature; sync disabled");
                                        Err(CoreError::Internal(
                                            "lan-sync feature not enabled".into(),
                                        ))
                                    }
                                }
                            };

                        match transport_result {
                            Ok(transport) => {
                                // Reuse the application-wide ConsentManager instead
                                // of constructing a separate instance from the file
                                // path. This ensures the SyncEngine sees the same
                                // in-memory consent state as the rest of the runtime.
                                let sync_engine = Arc::new(
                                    SyncEngine::new(
                                        extractor,
                                        merger,
                                        transport,
                                        self.consent_manager.clone(),
                                        device_id,
                                        device_name,
                                    )
                                    .await,
                                );
                                scheduler = scheduler.with_sync_engine(sync_engine);
                                info!(
                                    transport = ?self.config.sync.transport,
                                    "Cross-device sync engine initialized"
                                );
                            }
                            Err(e) => {
                                warn!("Failed to create sync transport: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get device identity for sync: {e}");
                    }
                }
            }
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
        }
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
        }
    }
}
