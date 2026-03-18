mod analysis_pipeline;
mod config;
mod loops;

// ── Public re-exports (external API) ────────────────────────────────
pub use config::{SchedulerConfig, SchedulerStorage};

use chrono::{Datelike, Timelike};
use oneshim_core::config::{AppConfig, Weekday};
use oneshim_core::config_manager::ConfigManager;
use oneshim_core::models::activity::SessionStats;
use oneshim_core::models::tiered_memory::ResolvedParams;
use oneshim_core::ports::api_client::ApiClient;
use oneshim_core::ports::batch_sink::BatchSink;
use oneshim_core::ports::calibration_store::{CalibrationReader, CalibrationWriter};
use oneshim_core::ports::monitor::{ActivityMonitor, ProcessMonitor, SystemMonitor};
use oneshim_core::ports::storage::StorageService;
use oneshim_core::ports::vision::{CaptureTrigger, FrameProcessor};
#[cfg(feature = "server")]
use oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator;
use oneshim_storage::frame_storage::FrameFileStorage;
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
    pub trigger: oneshim_analysis::AdaptiveTrigger,
    pub segment_buffer: oneshim_analysis::SegmentBuffer,
    pub calibration_buffer: oneshim_analysis::CalibrationBuffer,
    pub title_bar_parser: oneshim_analysis::TitleBarParser,
    pub work_type_classifier: oneshim_analysis::WorkTypeClassifier,
    pub content_tracker: oneshim_analysis::ContentTracker,
    pub segment_summarizer: oneshim_analysis::SegmentSummarizer,
    pub params: ResolvedParams,
    pub calibration_writer: Arc<dyn CalibrationWriter>,
    // --- Phase 1b Batch 2: regime-aware pipeline ---
    pub regime_classifier: oneshim_analysis::RegimeClassifier,
    pub regime_manager: oneshim_analysis::RegimeManager,
    pub regime_detector: oneshim_analysis::RegimeDetector,
    pub param_resolver: oneshim_analysis::ParamResolver,
    pub calibration_reader: Arc<dyn CalibrationReader>,
    /// ID of the current active regime (for transition detection).
    pub current_regime_id: Option<String>,
    /// Last time regime detection (k-means) was run.
    pub last_detection_time: Option<chrono::DateTime<chrono::Utc>>,
    // --- Layer 2: LLM summary + embedding pipeline ---
    pub(crate) llm_summarizer: Option<Arc<oneshim_analysis::LlmSegmentSummarizer>>,
    pub(crate) embedding_pipeline: Option<Arc<oneshim_analysis::EmbeddingPipeline>>,
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
    pub(super) frame_storage: Option<Arc<FrameFileStorage>>,
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
    /// Wrapped in Mutex so `run_scheduler_loops(&self)` can take ownership
    /// and move it into the monitor loop's async block.
    pub(super) adaptive_trigger: Mutex<Option<AdaptiveTriggerState>>,
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
        frame_storage: Option<Arc<FrameFileStorage>>,
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
            adaptive_trigger: Mutex::new(None),
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

    pub fn with_adaptive_trigger(self, state: AdaptiveTriggerState) -> Self {
        *self.adaptive_trigger.lock().expect("adaptive trigger lock") = Some(state);
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
            "스케줄러 started: 모니터링={}ms, 메트릭={}ms, 프로세스={}ms, 동기화={}ms, heartbeat={}ms, 집계={}ms",
            self.config.poll_interval.as_millis(),
            self.config.metrics_interval.as_millis(),
            self.config.process_interval.as_millis(),
            self.config.sync_interval.as_millis(),
            self.config.heartbeat_interval.as_millis(),
            self.config.aggregation_interval.as_millis(),
        );
        self.run_scheduler_loops(shutdown_rx, app_handle).await;
    }
}

#[allow(dead_code)]
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
