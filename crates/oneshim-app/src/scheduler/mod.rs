mod config;
mod loops;

// ── Public re-exports (external API) ────────────────────────────────
pub use config::{SchedulerConfig, SchedulerStorage};

use chrono::{Datelike, Timelike};
use oneshim_core::config::{AppConfig, Weekday};
use oneshim_core::models::activity::SessionStats;
use oneshim_core::ports::api_client::ApiClient;
use oneshim_core::ports::batch_sink::BatchSink;
use oneshim_core::ports::monitor::{ActivityMonitor, ProcessMonitor, SystemMonitor};
use oneshim_core::ports::storage::StorageService;
use oneshim_core::ports::vision::{CaptureTrigger, FrameProcessor};
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_web::RealtimeEvent;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::focus_analyzer::FocusAnalyzer;
use crate::notification_manager::NotificationManager;

pub struct Scheduler {
    pub(super) config: SchedulerConfig,
    #[allow(dead_code)]
    pub(super) app_config: Arc<tokio::sync::RwLock<AppConfig>>,
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
}

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: SchedulerConfig,
        app_config: Arc<tokio::sync::RwLock<AppConfig>>,
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
            app_config,
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
        }
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

    #[allow(dead_code)]
    pub fn app_config(&self) -> Arc<tokio::sync::RwLock<AppConfig>> {
        self.app_config.clone()
    }

    pub(super) async fn initialize_session(&self, session_id: &str) {
        let sqlite_init = self.sqlite_storage.clone();
        let session_stats = SessionStats::new(session_id.to_string());
        if let Err(e) = sqlite_init.upsert_session(&session_stats).await {
            warn!("session initialize failure: {e}");
        }
    }

    pub async fn run(&self, shutdown_rx: tokio::sync::watch::Receiver<bool>) {
        info!(
            "scheduler started: monitoring={}ms, metrics={}ms, process={}ms, sync={}ms, heartbeat={}ms, aggregation={}ms",
            self.config.poll_interval.as_millis(),
            self.config.metrics_interval.as_millis(),
            self.config.process_interval.as_millis(),
            self.config.sync_interval.as_millis(),
            self.config.heartbeat_interval.as_millis(),
            self.config.aggregation_interval.as_millis(),
        );
        self.run_scheduler_loops(shutdown_rx).await;
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
    use oneshim_core::config::{AiAccessMode, ExternalDataPolicy, PiiFilterLevel, PrivacyConfig};
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
        assert_eq!(config.poll_interval, Duration::from_secs(5));
        assert_eq!(config.metrics_interval, Duration::from_secs(5));
        assert_eq!(config.ai_access_mode, AiAccessMode::ProviderApiKey);
        assert_eq!(config.idle_threshold_secs, 300);
    }

    #[test]
    fn platform_sync_enabled_only_for_platform_connected_mode() {
        let mut config = SchedulerConfig {
            offline_mode: false,
            ai_access_mode: AiAccessMode::ProviderApiKey,
            ..SchedulerConfig::default()
        };

        let policy = PlatformEgressPolicy::new(&config);
        assert!(!policy.is_enabled());

        config.ai_access_mode = AiAccessMode::PlatformConnected;
        let policy = PlatformEgressPolicy::new(&config);
        assert!(policy.is_enabled());
    }

    #[test]
    fn strict_policy_redacts_window_title() {
        let config = SchedulerConfig {
            offline_mode: false,
            ai_access_mode: AiAccessMode::PlatformConnected,
            external_data_policy: ExternalDataPolicy::PiiFilterStrict,
            ..SchedulerConfig::default()
        };
        let policy = PlatformEgressPolicy::new(&config);
        let event = Event::Context(ContextEvent {
            app_name: "Chrome".to_string(),
            window_title: "Inbox user@example.com".to_string(),
            prev_app_name: None,
            timestamp: chrono::Utc::now(),
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
            offline_mode: false,
            ai_access_mode: AiAccessMode::PlatformConnected,
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
            offline_mode: false,
            ai_access_mode: AiAccessMode::PlatformConnected,
            ..SchedulerConfig::default()
        };
        let policy = PlatformEgressPolicy::new(&config);
        let event = Event::Context(ContextEvent {
            app_name: "Bitwarden".to_string(),
            window_title: "Vault".to_string(),
            prev_app_name: None,
            timestamp: chrono::Utc::now(),
        });

        assert!(policy.prepare_event_for_upload(event).is_none());
    }
}
