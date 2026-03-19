use base64::Engine;
use chrono::{DateTime, Utc};
use oneshim_core::config::{AnalysisConfig, ExternalDataPolicy, PrivacyConfig};
use oneshim_core::error::CoreError;
use oneshim_core::models::context::WindowBounds;
use oneshim_core::models::event::Event;
use oneshim_core::models::frame::FrameMetadata;
use oneshim_core::models::tiered_memory::SegmentSummary;
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

    /// Check whether server-sourced suggestions exist within the given lookback
    /// window (in seconds). Used by the analysis loop to suppress local LLM
    /// analysis when the server is actively providing suggestions.
    fn has_recent_server_suggestions(&self, lookback_secs: u64) -> Result<bool, CoreError>;

    /// List recent weekly digests, newest first.
    fn list_weekly_digests(
        &self,
        limit: usize,
    ) -> Result<Vec<oneshim_core::models::weekly_digest::WeeklyDigest>, CoreError>;

    /// Save a weekly digest. Upserts by week_start.
    fn save_weekly_digest(
        &self,
        digest: &oneshim_core::models::weekly_digest::WeeklyDigest,
    ) -> Result<(), CoreError>;

    /// List closed segments whose time range falls within [from, to].
    fn list_segments_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<SegmentSummary>, CoreError>;

    /// Delete activity segments older than `max_days`. Returns the count of deleted rows.
    fn enforce_segment_retention(&self, max_days: u32) -> Result<usize, CoreError>;

    /// Delete weekly digests older than `max_weeks`. Returns the count of deleted rows.
    fn enforce_digest_retention(&self, max_weeks: u32) -> Result<usize, CoreError>;

    /// Get a cached daily digest by date (YYYY-MM-DD).
    fn get_daily_digest(
        &self,
        date: &str,
    ) -> Result<Option<oneshim_core::models::daily_digest::DailyDigest>, CoreError>;

    /// Save a daily digest. Upserts by date.
    fn save_daily_digest(
        &self,
        digest: &oneshim_core::models::daily_digest::DailyDigest,
    ) -> Result<(), CoreError>;

    /// Get activity segment summary records for a given date (YYYY-MM-DD).
    fn get_segments_for_date(
        &self,
        date: &str,
    ) -> Result<Vec<oneshim_core::models::storage_records::SegmentSummaryRecord>, CoreError>;

    /// Save a GUI interaction event (delegates to WebStorage V13 table).
    #[allow(dead_code)]
    fn save_gui_interaction(
        &self,
        input: &oneshim_core::models::storage_records::NewGuiInteraction<'_>,
    ) -> Result<(), CoreError>;
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

    fn has_recent_server_suggestions(&self, lookback_secs: u64) -> Result<bool, CoreError> {
        SqliteStorage::has_recent_server_suggestions(self, lookback_secs)
    }

    fn list_weekly_digests(
        &self,
        limit: usize,
    ) -> Result<Vec<oneshim_core::models::weekly_digest::WeeklyDigest>, CoreError> {
        use oneshim_core::ports::web_storage::WebStorage;
        WebStorage::list_weekly_digests(self, limit)
    }

    fn save_weekly_digest(
        &self,
        digest: &oneshim_core::models::weekly_digest::WeeklyDigest,
    ) -> Result<(), CoreError> {
        use oneshim_core::ports::web_storage::WebStorage;
        WebStorage::save_weekly_digest(self, digest)
    }

    fn list_segments_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<SegmentSummary>, CoreError> {
        SqliteStorage::list_segments_between(self, from, to)
    }

    fn enforce_segment_retention(&self, max_days: u32) -> Result<usize, CoreError> {
        SqliteStorage::enforce_segment_retention(self, max_days)
    }

    fn enforce_digest_retention(&self, max_weeks: u32) -> Result<usize, CoreError> {
        SqliteStorage::enforce_digest_retention(self, max_weeks)
    }

    fn get_daily_digest(
        &self,
        date: &str,
    ) -> Result<Option<oneshim_core::models::daily_digest::DailyDigest>, CoreError> {
        use oneshim_core::ports::web_storage::WebStorage;
        WebStorage::get_daily_digest(self, date)
    }

    fn save_daily_digest(
        &self,
        digest: &oneshim_core::models::daily_digest::DailyDigest,
    ) -> Result<(), CoreError> {
        use oneshim_core::ports::web_storage::WebStorage;
        WebStorage::save_daily_digest(self, digest)
    }

    fn get_segments_for_date(
        &self,
        date: &str,
    ) -> Result<Vec<oneshim_core::models::storage_records::SegmentSummaryRecord>, CoreError> {
        use oneshim_core::ports::web_storage::WebStorage;
        WebStorage::get_segments_for_date(self, date)
    }

    fn save_gui_interaction(
        &self,
        input: &oneshim_core::models::storage_records::NewGuiInteraction<'_>,
    ) -> Result<(), CoreError> {
        use oneshim_core::ports::web_storage::WebStorage;
        WebStorage::save_gui_interaction(self, input)
    }
}

pub(super) fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

pub(super) const REDACTED_WINDOW_TITLE: &str = "[REDACTED_WINDOW_TITLE]";

/// Retention: raw system metrics are kept for 24 hours.
pub(super) const RAW_METRICS_RETENTION_HOURS: i64 = 24;
/// Retention: process snapshots are kept for 7 days.
pub(super) const PROCESS_SNAPSHOT_RETENTION_DAYS: i64 = 7;
/// Retention: idle period records are kept for 30 days.
pub(super) const IDLE_PERIOD_RETENTION_DAYS: i64 = 30;

/// OAuth token refresh check interval (seconds).
#[cfg(feature = "server")]
pub(super) const OAUTH_REFRESH_INTERVAL_SECS: u64 = 120;

#[derive(Clone)]
pub(super) struct PlatformEgressPolicy {
    enabled: bool,
    external_data_policy: ExternalDataPolicy,
    privacy_config: PrivacyConfig,
}

impl PlatformEgressPolicy {
    pub(super) fn new(config: &SchedulerConfig) -> Self {
        Self {
            enabled: config.upload_enabled,
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
            Event::Clipboard(_) | Event::FileAccess(_) => {}
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
    pub external_data_policy: ExternalDataPolicy,
    pub privacy_config: PrivacyConfig,
    pub idle_threshold_secs: u64,
    pub upload_enabled: bool,
    pub analysis_config: AnalysisConfig,
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
            external_data_policy: ExternalDataPolicy::default(),
            privacy_config: PrivacyConfig::default(),
            idle_threshold_secs: 300, // 5 min
            upload_enabled: false,
            analysis_config: AnalysisConfig::default(),
        }
    }
}
