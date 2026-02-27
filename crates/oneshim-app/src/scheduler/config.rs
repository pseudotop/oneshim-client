use base64::Engine;
use oneshim_core::config::{AiAccessMode, ExternalDataPolicy, PrivacyConfig};
use oneshim_core::error::CoreError;
use oneshim_core::models::context::WindowBounds;
use oneshim_core::models::event::Event;
use oneshim_core::models::frame::FrameMetadata;
use oneshim_core::ports::storage::MetricsStorage;
use oneshim_storage::sqlite::SqliteStorage;
use oneshim_vision::privacy::{sanitize_title_with_level, should_exclude};
use std::time::Duration;

pub trait SchedulerStorage: MetricsStorage + Send + Sync {
    fn save_frame_metadata_with_bounds(
        &self,
        metadata: &FrameMetadata,
        file_path: Option<&str>,
        ocr_text: Option<&str>,
        bounds: Option<&WindowBounds>,
    ) -> Result<i64, CoreError>;
}

impl SchedulerStorage for SqliteStorage {
    fn save_frame_metadata_with_bounds(
        &self,
        metadata: &FrameMetadata,
        file_path: Option<&str>,
        ocr_text: Option<&str>,
        bounds: Option<&WindowBounds>,
    ) -> Result<i64, CoreError> {
        SqliteStorage::save_frame_metadata_with_bounds(self, metadata, file_path, ocr_text, bounds)
    }
}

pub(super) fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

pub(super) const REDACTED_WINDOW_TITLE: &str = "[REDACTED_WINDOW_TITLE]";

#[derive(Clone)]
pub(super) struct PlatformEgressPolicy {
    enabled: bool,
    external_data_policy: ExternalDataPolicy,
    privacy_config: PrivacyConfig,
}

impl PlatformEgressPolicy {
    pub(super) fn new(config: &SchedulerConfig) -> Self {
        Self {
            enabled: !config.offline_mode
                && config.ai_access_mode == AiAccessMode::PlatformConnected,
            external_data_policy: config.external_data_policy,
            privacy_config: config.privacy_config.clone(),
        }
    }

    pub(super) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn prepare_event_for_upload(&self, mut event: Event) -> Option<Event> {
        if !self.enabled {
            return None;
        }

        match &mut event {
            Event::Context(ctx) => {
                let app_name = ctx.app_name.clone();
                let title = ctx.window_title.clone();
                if self.should_skip(&app_name, &title) {
                    return None;
                }
                ctx.window_title = self.sanitize_title(&title);
            }
            Event::Window(layout) => {
                let app_name = layout.window.app_name.clone();
                let title = layout.window.window_title.clone();
                if self.should_skip(&app_name, &title) {
                    return None;
                }
                layout.window.window_title = self.sanitize_title(&title);
            }
            Event::User(user) => {
                let app_name = user.app_name.clone();
                let title = user.window_title.clone();
                if self.should_skip(&app_name, &title) {
                    return None;
                }
                user.window_title = self.sanitize_title(&title);
            }
            Event::System(_) | Event::Input(_) | Event::Process(_) => {}
        }

        Some(event)
    }

    fn sanitize_title(&self, title: &str) -> String {
        match self.external_data_policy {
            ExternalDataPolicy::AllowFiltered => {
                sanitize_title_with_level(title, self.privacy_config.pii_filter_level)
            }
            ExternalDataPolicy::PiiFilterStrict | ExternalDataPolicy::PiiFilterStandard => {
                REDACTED_WINDOW_TITLE.to_string()
            }
        }
    }

    fn should_skip(&self, app_name: &str, window_title: &str) -> bool {
        should_exclude(
            app_name,
            window_title,
            &self.privacy_config.excluded_apps,
            &self.privacy_config.excluded_app_patterns,
            &self.privacy_config.excluded_title_patterns,
            self.privacy_config.auto_exclude_sensitive,
        )
    }
}

pub struct SchedulerConfig {
    pub poll_interval: Duration,
    pub metrics_interval: Duration,
    pub process_interval: Duration,
    pub detailed_process_interval: Duration,
    pub input_activity_interval: Duration,
    pub sync_interval: Duration,
    pub heartbeat_interval: Duration,
    pub aggregation_interval: Duration,
    pub session_id: String,
    pub offline_mode: bool,
    pub ai_access_mode: AiAccessMode,
    pub external_data_policy: ExternalDataPolicy,
    pub privacy_config: PrivacyConfig,
    pub idle_threshold_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            metrics_interval: Duration::from_secs(5),
            process_interval: Duration::from_secs(10),
            detailed_process_interval: Duration::from_secs(30), // 30 s
            input_activity_interval: Duration::from_secs(30),   // 30 s
            sync_interval: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            aggregation_interval: Duration::from_secs(3600), // 1 hour
            session_id: String::new(),                       // set by caller
            offline_mode: false,
            ai_access_mode: AiAccessMode::default(),
            external_data_policy: ExternalDataPolicy::default(),
            privacy_config: PrivacyConfig::default(),
            idle_threshold_secs: 300, // 5 min
        }
    }
}
