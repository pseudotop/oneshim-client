use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, services::settings_service, AppState};

#[derive(Debug, Serialize)]
pub struct StorageStats {
    pub db_size_bytes: u64,
    pub frames_size_bytes: u64,
    pub total_size_bytes: u64,
    pub frame_count: u64,
    pub event_count: u64,
    pub metric_count: u64,
    pub oldest_data_date: Option<String>,
    pub newest_data_date: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub retention_days: u32,
    pub max_storage_mb: u32,
    pub web_port: u16,
    pub allow_external: bool,
    pub capture_enabled: bool,
    pub idle_threshold_secs: u32,
    pub metrics_interval_secs: u32,
    pub process_interval_secs: u32,
    #[serde(default)]
    pub notification: NotificationSettings,
    #[serde(default)]
    pub update: UpdateSettings,
    #[serde(default)]
    pub telemetry: TelemetrySettings,
    #[serde(default)]
    pub monitor: MonitorControlSettings,
    #[serde(default)]
    pub privacy: PrivacySettings,
    #[serde(default)]
    pub schedule: ScheduleSettings,
    #[serde(default)]
    pub automation: AutomationSettings,
    #[serde(default)]
    pub sandbox: SandboxSettings,
    #[serde(default)]
    pub ai_provider: AiProviderSettings,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub idle_notification: bool,
    pub idle_notification_mins: u32,
    pub long_session_notification: bool,
    pub long_session_mins: u32,
    pub high_usage_notification: bool,
    pub high_usage_threshold: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSettings {
    pub enabled: bool,
    pub check_interval_hours: u32,
    pub include_prerelease: bool,
    pub auto_install: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_hours: 24,
            include_prerelease: false,
            auto_install: false,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TelemetrySettings {
    pub enabled: bool,
    pub crash_reports: bool,
    pub usage_analytics: bool,
    pub performance_metrics: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MonitorControlSettings {
    pub process_monitoring: bool,
    pub input_activity: bool,
    pub privacy_mode: bool,
}

impl Default for MonitorControlSettings {
    fn default() -> Self {
        Self {
            process_monitoring: true,
            input_activity: true,
            privacy_mode: false,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub excluded_apps: Vec<String>,
    pub excluded_app_patterns: Vec<String>,
    pub excluded_title_patterns: Vec<String>,
    pub auto_exclude_sensitive: bool,
    pub pii_filter_level: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduleSettings {
    pub active_hours_enabled: bool,
    pub active_start_hour: u8,
    pub active_end_hour: u8,
    pub active_days: Vec<String>,
    pub pause_on_screen_lock: bool,
    pub pause_on_battery_saver: bool,
}

impl Default for ScheduleSettings {
    fn default() -> Self {
        Self {
            active_hours_enabled: false,
            active_start_hour: 9,
            active_end_hour: 18,
            active_days: vec![
                "Mon".to_string(),
                "Tue".to_string(),
                "Wed".to_string(),
                "Thu".to_string(),
                "Fri".to_string(),
            ],
            pause_on_screen_lock: true,
            pause_on_battery_saver: false,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AutomationSettings {
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SandboxSettings {
    pub enabled: bool,
    pub profile: String,
    pub allowed_read_paths: Vec<String>,
    pub allowed_write_paths: Vec<String>,
    pub allow_network: bool,
    pub max_memory_bytes: u64,
    pub max_cpu_time_ms: u64,
}

impl Default for SandboxSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: "Standard".to_string(),
            allowed_read_paths: Vec::new(),
            allowed_write_paths: Vec::new(),
            allow_network: false,
            max_memory_bytes: 0,
            max_cpu_time_ms: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiProviderSettings {
    pub access_mode: String,
    pub ocr_provider: String,
    pub llm_provider: String,
    pub external_data_policy: String,
    #[serde(default)]
    pub allow_unredacted_external_ocr: bool,
    #[serde(default)]
    pub ocr_validation: OcrValidationSettings,
    #[serde(default)]
    pub scene_action_override: SceneActionOverrideSettings,
    #[serde(default)]
    pub scene_intelligence: SceneIntelligenceSettings,
    pub fallback_to_local: bool,
    pub ocr_api: Option<ExternalApiSettings>,
    pub llm_api: Option<ExternalApiSettings>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OcrValidationSettings {
    pub enabled: bool,
    pub min_confidence: f64,
    pub max_invalid_ratio: f64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SceneActionOverrideSettings {
    pub enabled: bool,
    pub reason: String,
    pub approved_by: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SceneIntelligenceSettings {
    pub enabled: bool,
    pub overlay_enabled: bool,
    pub allow_action_execution: bool,
    pub min_confidence: f64,
    pub max_elements: u32,
    pub calibration_enabled: bool,
    pub calibration_min_elements: u32,
    pub calibration_min_avg_confidence: f64,
}

impl Default for OcrValidationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.25,
            max_invalid_ratio: 0.6,
        }
    }
}

impl Default for SceneIntelligenceSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            overlay_enabled: true,
            allow_action_execution: false,
            min_confidence: 0.35,
            max_elements: 120,
            calibration_enabled: true,
            calibration_min_elements: 8,
            calibration_min_avg_confidence: 0.55,
        }
    }
}

impl Default for AiProviderSettings {
    fn default() -> Self {
        Self {
            access_mode: "ProviderApiKey".to_string(),
            ocr_provider: "Local".to_string(),
            llm_provider: "Local".to_string(),
            external_data_policy: "PiiFilterStrict".to_string(),
            allow_unredacted_external_ocr: false,
            ocr_validation: OcrValidationSettings::default(),
            scene_action_override: SceneActionOverrideSettings::default(),
            scene_intelligence: SceneIntelligenceSettings::default(),
            fallback_to_local: true,
            ocr_api: None,
            llm_api: None,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExternalApiSettings {
    pub endpoint: String,
    pub api_key_masked: String,
    pub model: Option<String>,
    #[serde(default = "default_provider_type")]
    pub provider_type: String,
    #[serde(default = "default_external_timeout")]
    pub timeout_secs: u64,
}

fn default_external_timeout() -> u64 {
    30
}

fn default_provider_type() -> String {
    "Generic".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            retention_days: 30,
            max_storage_mb: 500,
            web_port: 9090,
            allow_external: false,
            capture_enabled: true,
            idle_threshold_secs: 300,
            metrics_interval_secs: 5,
            process_interval_secs: 10,
            notification: NotificationSettings {
                enabled: true,
                idle_notification: true,
                idle_notification_mins: 30,
                long_session_notification: true,
                long_session_mins: 60,
                high_usage_notification: false,
                high_usage_threshold: 90,
            },
            update: UpdateSettings::default(),
            telemetry: TelemetrySettings::default(),
            monitor: MonitorControlSettings::default(),
            privacy: PrivacySettings {
                auto_exclude_sensitive: true,
                pii_filter_level: "Standard".to_string(),
                ..Default::default()
            },
            schedule: ScheduleSettings::default(),
            automation: AutomationSettings::default(),
            sandbox: SandboxSettings::default(),
            ai_provider: AiProviderSettings::default(),
        }
    }
}

pub async fn get_storage_stats(
    State(state): State<AppState>,
) -> Result<Json<StorageStats>, ApiError> {
    Ok(Json(settings_service::get_storage_stats(&state)?))
}

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<AppSettings>, ApiError> {
    Ok(Json(settings_service::get_settings(&state)))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(settings): Json<AppSettings>,
) -> Result<Json<AppSettings>, ApiError> {
    settings_service::update_settings(&state, &settings)?;
    Ok(Json(settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::settings_service;
    use oneshim_core::config::AppConfig;

    #[test]
    fn default_settings_valid() {
        let settings = AppSettings::default();
        assert_eq!(settings.retention_days, 30);
        assert_eq!(settings.max_storage_mb, 500);
        assert_eq!(settings.web_port, 9090);
        assert!(!settings.allow_external);
        assert!(settings.capture_enabled);
    }

    #[test]
    fn default_settings_includes_automation() {
        let settings = AppSettings::default();
        assert!(!settings.automation.enabled);
        assert!(!settings.sandbox.enabled);
        assert_eq!(settings.sandbox.profile, "Standard");
        assert_eq!(settings.ai_provider.access_mode, "ProviderApiKey");
        assert_eq!(settings.ai_provider.ocr_provider, "Local");
        assert_eq!(settings.ai_provider.llm_provider, "Local");
        assert!(settings.ai_provider.fallback_to_local);
    }

    #[test]
    fn settings_serde_roundtrip() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let deser: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.automation.enabled, settings.automation.enabled);
        assert_eq!(deser.sandbox.profile, settings.sandbox.profile);
        assert_eq!(
            deser.ai_provider.ocr_provider,
            settings.ai_provider.ocr_provider
        );
    }

    #[test]
    fn mask_api_key_works() {
        assert_eq!(
            settings_service::mask_api_key("sk-1234567890abcdef"),
            "sk...cdef"
        );
        assert_eq!(settings_service::mask_api_key("short"), "***");
        assert_eq!(settings_service::mask_api_key("12345678"), "***");
        assert_eq!(settings_service::mask_api_key("123456789"), "12...6789");
    }

    #[test]
    fn is_masked_key_detection() {
        assert!(settings_service::is_masked_key("sk...cdef"));
        assert!(settings_service::is_masked_key("ab...1234"));
        assert!(!settings_service::is_masked_key("sk-1234567890abcdef"));
        assert!(!settings_service::is_masked_key(""));
    }

    #[test]
    fn storage_stats_serializes() {
        let stats = StorageStats {
            db_size_bytes: 1024 * 1024,
            frames_size_bytes: 5 * 1024 * 1024,
            total_size_bytes: 6 * 1024 * 1024,
            frame_count: 100,
            event_count: 500,
            metric_count: 1000,
            oldest_data_date: Some("2024-01-01T00:00:00Z".to_string()),
            newest_data_date: Some("2024-01-30T23:59:59Z".to_string()),
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("db_size_bytes"));
        assert!(json.contains("frame_count"));
    }

    #[test]
    fn apply_settings_to_config_validates_remote_ai_requirements() {
        let mut app_config = AppConfig::default_config();
        let mut settings = AppSettings::default();

        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: "https://api.example.com/ocr".to_string(),
            api_key_masked: "".to_string(),
            model: None,
            provider_type: "Generic".to_string(),
            timeout_secs: 30,
        });

        settings_service::apply_settings_to_config(&mut app_config, &settings).unwrap();
        let result = app_config.ai_provider.validate_selected_remote_endpoints();
        assert!(result.is_err());
    }

    #[test]
    fn apply_settings_to_config_rejects_unknown_sandbox_profile() {
        let mut app_config = AppConfig::default_config();
        let mut settings = AppSettings::default();
        settings.sandbox.profile = "Unknown".to_string();

        let result = settings_service::apply_settings_to_config(&mut app_config, &settings);
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn apply_settings_to_config_rejects_unknown_weekday() {
        let mut app_config = AppConfig::default_config();
        let mut settings = AppSettings::default();
        settings.schedule.active_days = vec!["Mon".to_string(), "Funday".to_string()];

        let result = settings_service::apply_settings_to_config(&mut app_config, &settings);
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }
}
