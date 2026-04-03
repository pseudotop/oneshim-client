mod analysis_pipeline;
mod config;
/// GUI Activity Intelligence pipeline — wired into the monitor loop.
/// Called after `run_analysis_tick()` each cycle when `gui_intelligence.enabled`.
pub(crate) mod gui_pipeline;
pub(crate) mod heatmap;
mod loops;
pub(crate) mod shared_regime_state;

// ── Public re-exports (external API) ────────────────────────────────
pub use config::{SchedulerConfig, SchedulerStorage};
pub(crate) use loops::record_to_segment_summary;

use chrono::{Datelike, Timelike};
use oneshim_core::app_registry::AppRegistry;
use oneshim_core::config::{AppConfig, Weekday};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::consent::ConsentManager;
use oneshim_core::models::activity::SessionStats;
use oneshim_core::models::tiered_memory::ResolvedParams;
use oneshim_core::ports::accessibility::AccessibilityExtractor;
#[cfg(feature = "hnsw")]
use oneshim_core::ports::ann_index::AnnIndex;
use oneshim_core::ports::api_client::ApiClient;
use oneshim_core::ports::batch_sink::BatchSink;
use oneshim_core::ports::calibration_store::{CalibrationReader, CalibrationWriter};
use oneshim_core::ports::coaching_storage::CoachingStoragePort;
use oneshim_core::ports::frame_storage::FrameStoragePort;
use oneshim_core::ports::monitor::{ActivityMonitor, ProcessMonitor, SystemMonitor};
use oneshim_core::ports::overlay_driver::OverlayDriver;
use oneshim_core::ports::storage::StorageService;
use oneshim_core::ports::vector_index::VectorIndex;
use oneshim_core::ports::vision::{CaptureTrigger, FrameProcessor};
#[cfg(feature = "server")]
use oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator;
use oneshim_web::RealtimeEvent;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::focus_analyzer::FocusAnalyzer;
use crate::notification_manager::NotificationManager;

/// Wraps all components needed for the adaptive tiered-memory pipeline.
/// Kept as owned (non-Arc) so the monitor loop can mutate the components
/// without interior-mutability overhead.
pub(crate) struct AdaptiveTriggerState {
    // --- Base analysis pipeline ---
    pub trigger: oneshim_analysis::AdaptiveTrigger,
    pub segment_buffer: oneshim_analysis::SegmentBuffer,
    pub calibration_buffer: oneshim_analysis::CalibrationBuffer,
    pub title_bar_parser: oneshim_analysis::TitleBarParser,
    pub work_type_classifier: oneshim_analysis::WorkTypeClassifier,
    pub content_tracker: oneshim_analysis::ContentTracker,
    pub segment_summarizer: oneshim_analysis::SegmentSummarizer,
    pub params: ResolvedParams,
    pub calibration_writer: Arc<dyn CalibrationWriter>,

    // --- Regime-aware pipeline ---
    pub regime_classifier: oneshim_analysis::RegimeClassifier,
    pub regime_manager: oneshim_analysis::RegimeManager,
    pub regime_detector: oneshim_analysis::RegimeDetector,
    pub param_resolver: oneshim_analysis::ParamResolver,
    pub calibration_reader: Arc<dyn CalibrationReader>,
    /// ID of the current active regime (for transition detection).
    pub current_regime_id: Option<String>,
    /// Last time regime detection (k-means) was run.
    pub last_detection_time: Option<chrono::DateTime<chrono::Utc>>,

    // --- Auto-tuning ---
    /// Per-category EMA statistics tracker for auto-tuning trigger params.
    pub ema_tracker: oneshim_analysis::auto_tuner::EmaStatsTracker,
    /// EWMA-based drift detector for regime shift detection.
    pub drift_detector: oneshim_analysis::auto_tuner::DriftDetector,
    /// Tick counter for periodic auto-tune override generation.
    pub auto_tune_tick_count: u64,
    /// Regime analysis facade for constrained re-clustering.
    pub regime_analysis: Option<oneshim_analysis::RegimeAnalysisFacade>,
    /// Override store for loading user overrides during re-clustering.
    pub override_store: Option<Arc<dyn oneshim_core::ports::override_store::OverrideStore>>,
    /// Flag set by REST/Tauri to request on-demand re-clustering.
    pub recluster_requested: Arc<std::sync::atomic::AtomicBool>,
    /// Flag: last drift observation result. Set by analysis pipeline,
    /// read-and-cleared by coaching evaluation in the monitor loop.
    pub last_drift_detected: Arc<std::sync::atomic::AtomicBool>,

    // --- LLM/embedding pipeline ---
    pub(crate) llm_summarizer: Option<Arc<oneshim_analysis::LlmSegmentSummarizer>>,
    pub(crate) embedding_pipeline: Option<Arc<oneshim_analysis::EmbeddingPipeline>>,

    // --- GUI Activity Intelligence ---
    pub(crate) gui_pipeline_state: Option<gui_pipeline::GuiPipelineState>,
    pub(crate) gui_work_type_refiner: oneshim_analysis::GuiWorkTypeRefiner,

    /// Optional LLM-based work type refinement. When present, refines rule-based
    /// classification using AnalysisProvider. Background prefetch + LRU cache.
    pub(crate) llm_work_type_refiner: Option<Arc<oneshim_analysis::LlmWorkTypeRefiner>>,

    // --- Application classification ---
    /// Centralized app registry for subcategory classification.
    /// Replaces the hardcoded `infer_subcategory()` fallback.
    pub(crate) app_registry: Arc<AppRegistry>,

    // --- Heatmap ---
    /// Mouse interaction heatmap aggregator for overlay visualization.
    pub(crate) heatmap_aggregator: heatmap::HeatmapAggregator,
}

pub struct Scheduler {
    pub(super) config: SchedulerConfig,
    pub(super) system_monitor: Arc<dyn SystemMonitor>,
    pub(super) activity_monitor: Arc<dyn ActivityMonitor>,
    pub(super) process_monitor: Arc<dyn ProcessMonitor>,
    pub(super) capture_trigger: Arc<dyn CaptureTrigger>,
    pub(super) frame_processor: Arc<dyn FrameProcessor>,
    pub(super) storage: Arc<dyn StorageService>,
    pub(super) sqlite_storage: Arc<dyn SchedulerStorage>,
    pub(super) frame_storage: Option<Arc<dyn FrameStoragePort>>,
    pub(super) batch_sink: Option<Arc<dyn BatchSink>>,
    pub(super) api_client: Option<Arc<dyn ApiClient>>,
    pub(super) event_tx: Option<broadcast::Sender<RealtimeEvent>>,
    pub(super) notification_manager: Option<Arc<NotificationManager>>,
    pub(super) focus_analyzer: Option<Arc<FocusAnalyzer>>,
    #[cfg(feature = "server")]
    pub(super) oauth_coordinator: Option<Arc<TokenRefreshCoordinator>>,
    pub(super) context_analyzer: Option<Arc<oneshim_analysis::ContextAnalyzer>>,
    pub(super) config_manager: Option<ConfigManager>,
    pub(super) vector_store: Option<Arc<dyn oneshim_core::ports::vector_store::VectorStore>>,
    pub(super) embedding_provider:
        Option<Arc<dyn oneshim_core::ports::embedding_provider::EmbeddingProvider>>,
    pub(super) vector_index: Option<Arc<dyn VectorIndex>>,
    pub(super) search_coordinator: Option<Arc<oneshim_analysis::AdaptiveSearchCoordinator>>,
    /// Optional HNSW ANN index for approximate nearest neighbor search.
    /// Only present when the `hnsw` feature is enabled and configured.
    #[cfg(feature = "hnsw")]
    pub(super) ann_index: Option<Arc<dyn AnnIndex>>,
    /// Wrapped in Mutex so `run_scheduler_loops(&self)` can take ownership
    /// and move it into the monitor loop's async block.
    pub(super) adaptive_trigger: Mutex<Option<AdaptiveTriggerState>>,
    /// Cross-device sync engine (P3 Phase 3a-2). Optional — only present
    /// when sync is enabled AND configured (folder + passphrase).
    pub(super) sync_engine: Option<Arc<crate::sync_engine::SyncEngine>>,
    /// Accessibility API extractor for focused element context (Phase 2).
    /// `None` when `text_intelligence.accessibility_extraction` is disabled
    /// or platform does not support it.
    pub(super) accessibility_extractor: Option<Arc<dyn AccessibilityExtractor>>,
    /// ConsentManager for runtime consent checks (e.g., full_text_extraction).
    /// Wrapped in Arc for shared access across async blocks.
    pub(super) consent_manager: Option<Arc<ConsentManager>>,
    /// Coaching engine for proactive coaching messages (Phase 1).
    /// `None` when coaching is not configured. The engine checks `enabled`
    /// internally and returns `None` from `evaluate()` when disabled.
    pub(super) coaching_engine: Option<Arc<oneshim_analysis::CoachingEngine>>,
    /// MagicOverlay handle for coaching message delivery (Phase 2).
    pub(super) magic_overlay: Option<crate::magic_overlay::MagicOverlayHandle>,
    /// OverlayDriver port for rendering focus highlights on screen elements.
    /// Uses `show_highlights()` / `clear_highlights()` to draw bounding boxes
    /// around the currently focused accessibility element.
    pub(super) overlay_driver: Option<Arc<dyn OverlayDriver>>,
    /// Analysis provider for LLM personalization of coaching messages (Phase 2).
    pub(super) analysis_provider:
        Option<Arc<dyn oneshim_core::ports::analysis_provider::AnalysisProvider>>,
    /// Focused coaching event persistence port (Phase 2).
    pub(super) coaching_storage: Option<Arc<dyn CoachingStoragePort>>,
    /// Shared flag: when `true` the monitor loop skips capture/frame processing.
    pub(super) capture_paused: Arc<std::sync::atomic::AtomicBool>,
    /// Whether detection overlay is active. When `true`, the monitor loop
    /// re-triggers scene analysis on window focus changes.
    pub(super) detection_active: Arc<std::sync::atomic::AtomicBool>,
    /// Element finder for detection overlay re-analysis on window change.
    /// `None` when automation controller is not configured or has no scene_finder.
    pub(super) scene_finder: Option<Arc<dyn oneshim_core::ports::element_finder::ElementFinder>>,
    /// Adapter-side health flag for server (BatchUploader / HttpApiClient).
    pub(super) server_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Adapter-side health flag for LLM provider (RemoteLlmProvider).
    pub(super) llm_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Adapter-side health flag for CLI / automation controller.
    pub(super) cli_health_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// UI-facing connection status flag for server.
    pub(super) server_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// UI-facing connection status flag for LLM.
    pub(super) llm_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// UI-facing connection status flag for CLI.
    pub(super) cli_connected: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// App handle for tray icon sync from the health check loop.
    pub(super) tray_app_handle: Option<tauri::AppHandle>,
    /// Suggestion receiver for real-time SSE suggestion reception.
    #[cfg(feature = "server")]
    pub(super) suggestion_receiver: Option<Arc<oneshim_suggestion::receiver::SuggestionReceiver>>,
    /// Whether suggestion reception is enabled (from SuggestionConfig).
    pub(super) suggestions_enabled: bool,
    /// Focus mode state — shared with IPC commands and scheduler loops (A4).
    /// When active, coaching evaluation and notifications are suppressed,
    /// and capture importance threshold is elevated.
    pub(super) focus_mode: Arc<crate::focus_mode::FocusModeState>,
    /// SharedRegimeState injected from app_runtime — shares a single instance
    /// with SessionManager's context assembler. Falls back to a local instance
    /// in run_scheduler_loops if not provided.
    pub(super) shared_regime: Option<Arc<shared_regime_state::SharedRegimeState>>,
}

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: SchedulerConfig,
        system_monitor: Arc<dyn SystemMonitor>,
        activity_monitor: Arc<dyn ActivityMonitor>,
        process_monitor: Arc<dyn ProcessMonitor>,
        capture_trigger: Arc<dyn CaptureTrigger>,
        frame_processor: Arc<dyn FrameProcessor>,
        storage: Arc<dyn StorageService>,
        sqlite_storage: Arc<dyn SchedulerStorage>,
        frame_storage: Option<Arc<dyn FrameStoragePort>>,
        batch_sink: Option<Arc<dyn BatchSink>>,
        api_client: Option<Arc<dyn ApiClient>>,
    ) -> Self {
        Self {
            config,
            system_monitor,
            activity_monitor,
            process_monitor,
            capture_trigger,
            frame_processor,
            storage,
            sqlite_storage,
            frame_storage,
            batch_sink,
            api_client,
            event_tx: None,
            notification_manager: None,
            focus_analyzer: None,
            #[cfg(feature = "server")]
            oauth_coordinator: None,
            context_analyzer: None,
            config_manager: None,
            vector_store: None,
            embedding_provider: None,
            vector_index: None,
            search_coordinator: None,
            #[cfg(feature = "hnsw")]
            ann_index: None,
            adaptive_trigger: Mutex::new(None),
            sync_engine: None,
            accessibility_extractor: None,
            consent_manager: None,
            coaching_engine: None,
            magic_overlay: None,
            overlay_driver: None,
            analysis_provider: None,
            coaching_storage: None,
            capture_paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            detection_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            scene_finder: None,
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
            focus_mode: Arc::new(crate::focus_mode::FocusModeState::new()),
            shared_regime: None,
        }
    }

    pub fn with_config_manager(mut self, config_manager: ConfigManager) -> Self {
        self.config_manager = Some(config_manager);
        self
    }

    pub fn with_event_tx(mut self, event_tx: broadcast::Sender<RealtimeEvent>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }

    pub fn with_notification_manager(mut self, manager: Arc<NotificationManager>) -> Self {
        self.notification_manager = Some(manager);
        self
    }

    pub fn with_focus_analyzer(mut self, analyzer: Arc<FocusAnalyzer>) -> Self {
        self.focus_analyzer = Some(analyzer);
        self
    }

    #[cfg(feature = "server")]
    pub fn with_oauth_coordinator(mut self, coordinator: Arc<TokenRefreshCoordinator>) -> Self {
        self.oauth_coordinator = Some(coordinator);
        self
    }

    pub fn with_context_analyzer(
        mut self,
        analyzer: Arc<oneshim_analysis::ContextAnalyzer>,
    ) -> Self {
        self.context_analyzer = Some(analyzer);
        self
    }

    pub fn with_vector_store(
        mut self,
        store: Arc<dyn oneshim_core::ports::vector_store::VectorStore>,
    ) -> Self {
        self.vector_store = Some(store);
        self
    }

    pub fn with_embedding_provider(
        mut self,
        provider: Arc<dyn oneshim_core::ports::embedding_provider::EmbeddingProvider>,
    ) -> Self {
        self.embedding_provider = Some(provider);
        self
    }

    pub fn with_vector_index(mut self, index: Arc<dyn VectorIndex>) -> Self {
        self.vector_index = Some(index);
        self
    }

    pub fn with_search_coordinator(
        mut self,
        coordinator: Arc<oneshim_analysis::AdaptiveSearchCoordinator>,
    ) -> Self {
        self.search_coordinator = Some(coordinator);
        self
    }

    /// Attach an HNSW ANN index for approximate nearest neighbor search.
    #[cfg(feature = "hnsw")]
    #[allow(dead_code)]
    pub fn with_ann_index(mut self, ann: Arc<dyn AnnIndex>) -> Self {
        self.ann_index = Some(ann);
        self
    }

    pub fn with_adaptive_trigger(self, state: AdaptiveTriggerState) -> Self {
        *self.adaptive_trigger.lock().expect("adaptive trigger lock") = Some(state);
        self
    }

    pub fn with_sync_engine(mut self, engine: Arc<crate::sync_engine::SyncEngine>) -> Self {
        self.sync_engine = Some(engine);
        self
    }

    pub fn with_accessibility_extractor(
        mut self,
        extractor: Arc<dyn AccessibilityExtractor>,
    ) -> Self {
        self.accessibility_extractor = Some(extractor);
        self
    }

    pub fn with_consent_manager(mut self, consent_manager: Arc<ConsentManager>) -> Self {
        self.consent_manager = Some(consent_manager);
        self
    }

    pub fn with_coaching_engine(mut self, engine: Arc<oneshim_analysis::CoachingEngine>) -> Self {
        self.coaching_engine = Some(engine);
        self
    }

    pub fn with_magic_overlay(mut self, overlay: crate::magic_overlay::MagicOverlayHandle) -> Self {
        self.magic_overlay = Some(overlay);
        self
    }

    pub fn with_overlay_driver(mut self, driver: Arc<dyn OverlayDriver>) -> Self {
        self.overlay_driver = Some(driver);
        self
    }

    pub fn with_analysis_provider(
        mut self,
        provider: Arc<dyn oneshim_core::ports::analysis_provider::AnalysisProvider>,
    ) -> Self {
        self.analysis_provider = Some(provider);
        self
    }

    #[allow(dead_code)]
    pub fn with_coaching_storage(mut self, storage: Arc<dyn CoachingStoragePort>) -> Self {
        self.coaching_storage = Some(storage);
        self
    }

    pub fn with_capture_paused(mut self, flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.capture_paused = flag;
        self
    }

    pub fn with_detection_active(mut self, flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.detection_active = flag;
        self
    }

    #[allow(dead_code)] // wired when automation controller provides a scene_finder
    pub fn with_scene_finder(
        mut self,
        finder: Arc<dyn oneshim_core::ports::element_finder::ElementFinder>,
    ) -> Self {
        self.scene_finder = Some(finder);
        self
    }

    pub fn with_health_flags(
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

    pub fn with_connection_flags(
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

    pub fn with_tray_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.tray_app_handle = Some(handle);
        self
    }

    #[cfg(feature = "server")]
    pub fn with_suggestion_receiver(
        mut self,
        receiver: Arc<oneshim_suggestion::receiver::SuggestionReceiver>,
    ) -> Self {
        self.suggestion_receiver = Some(receiver);
        self
    }

    pub fn with_suggestions_enabled(mut self, enabled: bool) -> Self {
        self.suggestions_enabled = enabled;
        self
    }

    pub fn with_focus_mode(mut self, focus_mode: Arc<crate::focus_mode::FocusModeState>) -> Self {
        self.focus_mode = focus_mode;
        self
    }

    pub fn with_shared_regime(
        mut self,
        regime: Arc<shared_regime_state::SharedRegimeState>,
    ) -> Self {
        self.shared_regime = Some(regime);
        self
    }

    pub(super) async fn initialize_session(&self, session_id: &str) {
        let sqlite_init = self.sqlite_storage.clone();
        let session_stats = SessionStats::new(session_id.to_string());
        if let Err(e) = sqlite_init.upsert_session(&session_stats).await {
            warn!("session initialize failure: {e}");
        }
    }

    pub async fn run(
        &self,
        shutdown_rx: tokio::sync::watch::Receiver<bool>,
        app_handle: Option<tauri::AppHandle>,
    ) {
        info!(
            monitor_poll_ms = self.config.poll_interval.as_millis() as u64,
            metrics_ms = self.config.metrics_interval.as_millis() as u64,
            process_ms = self.config.process_interval.as_millis() as u64,
            detailed_process_ms = self.config.detailed_process_interval.as_millis() as u64,
            input_activity_ms = self.config.input_activity_interval.as_millis() as u64,
            sync_ms = self.config.sync_interval.as_millis() as u64,
            heartbeat_ms = self.config.heartbeat_interval.as_millis() as u64,
            aggregation_ms = self.config.aggregation_interval.as_millis() as u64,
            health_check_secs = 60,
            coaching_secs = config::COACHING_INTERVAL_SECS,
            sqlite_maintenance_mins = config::SQLITE_MAINTENANCE_INTERVAL_MINS,
            "scheduler loops starting"
        );
        self.run_scheduler_loops(shutdown_rx, app_handle).await;
    }
}

pub fn should_run_now(config: &AppConfig) -> bool {
    let schedule = &config.schedule;
    if !schedule.active_hours_enabled {
        return true;
    }

    let now = chrono::Local::now();
    let hour = now.hour() as u8;
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => Weekday::Mon,
        chrono::Weekday::Tue => Weekday::Tue,
        chrono::Weekday::Wed => Weekday::Wed,
        chrono::Weekday::Thu => Weekday::Thu,
        chrono::Weekday::Fri => Weekday::Fri,
        chrono::Weekday::Sat => Weekday::Sat,
        chrono::Weekday::Sun => Weekday::Sun,
    };

    if !schedule.active_days.contains(&weekday) {
        return false;
    }

    hour >= schedule.active_start_hour && hour < schedule.active_end_hour
}

#[cfg(test)]
mod tests {
    use self::config::{PlatformEgressPolicy, REDACTED_WINDOW_TITLE};
    use super::*;
    use oneshim_core::config::{ExternalDataPolicy, PiiFilterLevel, PrivacyConfig};
    use oneshim_core::models::event::{ContextEvent, Event};
    use std::time::Duration;

    #[test]
    fn should_run_when_disabled() {
        let config = AppConfig::default_config();
        assert!(should_run_now(&config));
    }

    #[test]
    fn scheduler_config_default() {
        let config = SchedulerConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(1));
        assert_eq!(config.metrics_interval, Duration::from_secs(5));
        assert_eq!(config.idle_threshold_secs, 300);
    }

    #[test]
    fn platform_sync_is_disabled_in_current_ai_runtime() {
        let config = SchedulerConfig {
            ..SchedulerConfig::default()
        };

        let policy = PlatformEgressPolicy::new(&config);
        assert!(!policy.is_enabled());

        let policy = PlatformEgressPolicy::new(&config);
        assert!(!policy.is_enabled());
    }

    #[test]
    fn strict_policy_redacts_window_title() {
        let config = SchedulerConfig {
            upload_enabled: true,
            external_data_policy: ExternalDataPolicy::PiiFilterStrict,
            ..SchedulerConfig::default()
        };
        let policy = PlatformEgressPolicy::new(&config);
        let event = Event::Context(ContextEvent {
            app_name: "Chrome".to_string(),
            window_title: "Inbox user@example.com".to_string(),
            prev_app_name: None,
            timestamp: chrono::Utc::now(),
            ..Default::default()
        });

        let uploaded = policy.prepare_event_for_upload(event);
        let Some(Event::Context(ctx)) = uploaded else {
            panic!("context event should be uploadable");
        };
        assert_eq!(ctx.window_title, REDACTED_WINDOW_TITLE);
    }

    #[test]
    fn allow_filtered_policy_uses_pii_filter() {
        let privacy = PrivacyConfig {
            pii_filter_level: PiiFilterLevel::Basic,
            ..PrivacyConfig::default()
        };
        let config = SchedulerConfig {
            upload_enabled: true,
            external_data_policy: ExternalDataPolicy::AllowFiltered,
            privacy_config: privacy,
            ..SchedulerConfig::default()
        };
        let policy = PlatformEgressPolicy::new(&config);
        let event = Event::Context(ContextEvent {
            app_name: "Chrome".to_string(),
            window_title: "Inbox user@example.com".to_string(),
            prev_app_name: None,
            timestamp: chrono::Utc::now(),
            ..Default::default()
        });

        let uploaded = policy.prepare_event_for_upload(event);
        let Some(Event::Context(ctx)) = uploaded else {
            panic!("context event should be uploadable");
        };
        assert!(ctx.window_title.contains("[EMAIL]"));
        assert!(!ctx.window_title.contains('@'));
    }

    #[test]
    fn sensitive_apps_are_skipped_from_upload() {
        let config = SchedulerConfig {
            ..SchedulerConfig::default()
        };
        let policy = PlatformEgressPolicy::new(&config);
        let event = Event::Context(ContextEvent {
            app_name: "Bitwarden".to_string(),
            window_title: "Vault".to_string(),
            prev_app_name: None,
            timestamp: chrono::Utc::now(),
            ..Default::default()
        });

        assert!(policy.prepare_event_for_upload(event).is_none());
    }
}
