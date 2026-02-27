use super::enums::*;
use crate::error::CoreError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── IntegrityConfig ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityConfig {
    #[serde(default = "default_integrity_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub require_signed_policy_bundle: bool,
    #[serde(default)]
    pub policy_file_path: Option<String>,
    #[serde(default)]
    pub policy_signature_path: Option<String>,
    #[serde(default)]
    pub policy_public_key: Option<String>,
}

impl Default for IntegrityConfig {
    fn default() -> Self {
        Self {
            enabled: default_integrity_enabled(),
            require_signed_policy_bundle: true,
            policy_file_path: None,
            policy_signature_path: None,
            policy_public_key: None,
        }
    }
}

// ── TelemetryConfig ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub crash_reports: bool,
    #[serde(default)]
    pub usage_analytics: bool,
    #[serde(default)]
    pub performance_metrics: bool,
}

// ── PrivacyConfig ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    #[serde(default)]
    pub excluded_apps: Vec<String>,
    #[serde(default)]
    pub excluded_app_patterns: Vec<String>,
    #[serde(default)]
    pub excluded_title_patterns: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_exclude_sensitive: bool,
    #[serde(default)]
    pub pii_filter_level: PiiFilterLevel,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            excluded_apps: Vec::new(),
            excluded_app_patterns: Vec::new(),
            excluded_title_patterns: Vec::new(),
            auto_exclude_sensitive: true,
            pii_filter_level: PiiFilterLevel::Standard,
        }
    }
}

// ── ScheduleConfig ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    #[serde(default)]
    pub active_hours_enabled: bool,
    #[serde(default = "default_active_start_hour")]
    pub active_start_hour: u8,
    #[serde(default = "default_active_end_hour")]
    pub active_end_hour: u8,
    #[serde(default = "default_active_days")]
    pub active_days: Vec<Weekday>,
    #[serde(default = "default_true")]
    pub pause_on_screen_lock: bool,
    #[serde(default)]
    pub pause_on_battery_saver: bool,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            active_hours_enabled: false,
            active_start_hour: default_active_start_hour(),
            active_end_hour: default_active_end_hour(),
            active_days: default_active_days(),
            pause_on_screen_lock: true,
            pause_on_battery_saver: false,
        }
    }
}

// ── FileAccessConfig ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccessConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub monitored_folders: Vec<PathBuf>,
    #[serde(default = "default_excluded_extensions")]
    pub excluded_extensions: Vec<String>,
    #[serde(default = "default_max_events_per_minute")]
    pub max_events_per_minute: u32,
}

impl Default for FileAccessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            monitored_folders: Vec::new(),
            excluded_extensions: default_excluded_extensions(),
            max_events_per_minute: default_max_events_per_minute(),
        }
    }
}

// ── AutomationConfig ───────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutomationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub custom_presets: Vec<crate::models::intent::WorkflowPreset>,
}

// ── SandboxConfig ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub profile: SandboxProfile,
    #[serde(default)]
    pub allowed_read_paths: Vec<String>,
    #[serde(default)]
    pub allowed_write_paths: Vec<String>,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default)]
    pub max_memory_bytes: u64,
    #[serde(default)]
    pub max_cpu_time_ms: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: SandboxProfile::Standard,
            allowed_read_paths: Vec::new(),
            allowed_write_paths: Vec::new(),
            allow_network: false,
            max_memory_bytes: 0,
            max_cpu_time_ms: 0,
        }
    }
}

// ── AiProviderConfig ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    #[serde(default)]
    pub access_mode: AiAccessMode,
    #[serde(default)]
    pub ocr_provider: OcrProviderType,
    #[serde(default)]
    pub llm_provider: LlmProviderType,
    #[serde(default)]
    pub ocr_api: Option<ExternalApiEndpoint>,
    #[serde(default)]
    pub llm_api: Option<ExternalApiEndpoint>,
    #[serde(default)]
    pub external_data_policy: ExternalDataPolicy,
    #[serde(default)]
    pub allow_unredacted_external_ocr: bool,
    #[serde(default)]
    pub ocr_validation: OcrValidationConfig,
    #[serde(default)]
    pub scene_action_override: SceneActionOverrideConfig,
    #[serde(default)]
    pub scene_intelligence: SceneIntelligenceConfig,
    #[serde(default = "default_true")]
    pub fallback_to_local: bool,
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self {
            access_mode: AiAccessMode::default(),
            ocr_provider: OcrProviderType::default(),
            llm_provider: LlmProviderType::default(),
            ocr_api: None,
            llm_api: None,
            external_data_policy: ExternalDataPolicy::default(),
            allow_unredacted_external_ocr: false,
            ocr_validation: OcrValidationConfig::default(),
            scene_action_override: SceneActionOverrideConfig::default(),
            scene_intelligence: SceneIntelligenceConfig::default(),
            fallback_to_local: true,
        }
    }
}

impl AiProviderConfig {
    pub fn validate_selected_remote_endpoints(&self) -> Result<(), CoreError> {
        self.ocr_validation.validate()?;
        self.scene_action_override.validate()?;
        self.scene_intelligence.validate()?;

        match self.access_mode {
            AiAccessMode::ProviderApiKey | AiAccessMode::PlatformConnected => {
                if self.ocr_provider == OcrProviderType::Remote {
                    validate_remote_endpoint(self.ocr_api.as_ref(), "ocr_api")?;
                }
                if self.llm_provider == LlmProviderType::Remote {
                    validate_remote_endpoint(self.llm_api.as_ref(), "llm_api")?;
                }
            }
            AiAccessMode::LocalModel => {}
            AiAccessMode::ProviderSubscriptionCli => {
                if self.ocr_provider == OcrProviderType::Remote
                    || self.llm_provider == LlmProviderType::Remote
                {
                    return Err(CoreError::Config(
                        "Provider subscription (CLI) mode requires local OCR/LLM providers instead of remote providers."
                            .to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

// ── SceneActionOverrideConfig ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneActionOverrideConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

impl SceneActionOverrideConfig {
    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        if !self.enabled {
            return false;
        }

        let reason = self.reason.as_deref().map(str::trim).unwrap_or_default();
        let approved_by = self
            .approved_by
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        let Some(expires_at) = self.expires_at else {
            return false;
        };

        !reason.is_empty() && !approved_by.is_empty() && expires_at > now
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if !self.enabled {
            return Ok(());
        }

        let reason = self.reason.as_deref().map(str::trim).unwrap_or_default();
        if reason.is_empty() {
            return Err(CoreError::Config(
                "`ai_provider.scene_action_override.reason` is required.".to_string(),
            ));
        }

        let approved_by = self
            .approved_by
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        if approved_by.is_empty() {
            return Err(CoreError::Config(
                "`ai_provider.scene_action_override.approved_by` is required.".to_string(),
            ));
        }

        let expires_at = self.expires_at.ok_or_else(|| {
            CoreError::Config(
                "`ai_provider.scene_action_override.expires_at` is required.".to_string(),
            )
        })?;

        if expires_at <= Utc::now() {
            return Err(CoreError::Config(
                "`ai_provider.scene_action_override.expires_at` must be in the future.".to_string(),
            ));
        }

        Ok(())
    }
}

// ── SceneIntelligenceConfig ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneIntelligenceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub overlay_enabled: bool,
    #[serde(default = "default_false")]
    pub allow_action_execution: bool,
    #[serde(default = "default_scene_min_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_scene_max_elements")]
    pub max_elements: usize,
    #[serde(default = "default_true")]
    pub calibration_enabled: bool,
    #[serde(default = "default_scene_calibration_min_elements")]
    pub calibration_min_elements: usize,
    #[serde(default = "default_scene_calibration_min_avg_confidence")]
    pub calibration_min_avg_confidence: f64,
}

impl Default for SceneIntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            overlay_enabled: true,
            allow_action_execution: default_false(),
            min_confidence: default_scene_min_confidence(),
            max_elements: default_scene_max_elements(),
            calibration_enabled: true,
            calibration_min_elements: default_scene_calibration_min_elements(),
            calibration_min_avg_confidence: default_scene_calibration_min_avg_confidence(),
        }
    }
}

impl SceneIntelligenceConfig {
    pub fn validate(&self) -> Result<(), CoreError> {
        if !self.min_confidence.is_finite() || !(0.0..=1.0).contains(&self.min_confidence) {
            return Err(CoreError::Config(
                "`ai_provider.scene_intelligence.min_confidence` must be within 0.0..=1.0."
                    .to_string(),
            ));
        }
        if self.max_elements == 0 || self.max_elements > 1000 {
            return Err(CoreError::Config(
                "`ai_provider.scene_intelligence.max_elements` must be within 1..=1000."
                    .to_string(),
            ));
        }
        if self.calibration_min_elements == 0 || self.calibration_min_elements > 1000 {
            return Err(CoreError::Config(
                "`ai_provider.scene_intelligence.calibration_min_elements` must be within 1..=1000."
                    .to_string(),
            ));
        }
        if !self.calibration_min_avg_confidence.is_finite()
            || !(0.0..=1.0).contains(&self.calibration_min_avg_confidence)
        {
            return Err(CoreError::Config(
                "`ai_provider.scene_intelligence.calibration_min_avg_confidence` must be within 0.0..=1.0."
                    .to_string(),
            ));
        }
        Ok(())
    }
}

// ── ExternalApiEndpoint ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalApiEndpoint {
    pub endpoint: String,
    #[serde(default)]
    pub api_key: String,
    pub model: Option<String>,
    #[serde(default = "default_api_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub provider_type: AiProviderType,
}

// ── OcrValidationConfig ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrValidationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_ocr_min_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_ocr_max_invalid_ratio")]
    pub max_invalid_ratio: f64,
}

impl Default for OcrValidationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: default_ocr_min_confidence(),
            max_invalid_ratio: default_ocr_max_invalid_ratio(),
        }
    }
}

impl OcrValidationConfig {
    pub fn validate(&self) -> Result<(), CoreError> {
        if !self.enabled {
            return Ok(());
        }

        if !self.min_confidence.is_finite() || !(0.0..=1.0).contains(&self.min_confidence) {
            return Err(CoreError::Config(
                "`ai_provider.ocr_validation.min_confidence` must be within 0.0..=1.0.".to_string(),
            ));
        }

        if !self.max_invalid_ratio.is_finite() || !(0.0..=1.0).contains(&self.max_invalid_ratio) {
            return Err(CoreError::Config(
                "`ai_provider.ocr_validation.max_invalid_ratio` must be within 0.0..=1.0."
                    .to_string(),
            ));
        }

        Ok(())
    }
}

// ── GrpcConfig ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    #[serde(default)]
    pub use_grpc_auth: bool,
    #[serde(default)]
    pub use_grpc_context: bool,
    #[serde(default = "default_grpc_endpoint")]
    pub grpc_endpoint: String,
    #[serde(default = "default_grpc_fallback_ports")]
    pub grpc_fallback_ports: Vec<u16>,
    #[serde(default = "default_grpc_connect_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_grpc_request_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub use_tls: bool,

    #[serde(default)]
    pub mtls_enabled: bool,
    #[serde(default)]
    pub tls_domain_name: Option<String>,
    #[serde(default)]
    pub tls_ca_cert_path: Option<String>,
    #[serde(default)]
    pub tls_client_cert_path: Option<String>,
    #[serde(default)]
    pub tls_client_key_path: Option<String>,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            use_grpc_auth: false,
            use_grpc_context: false,
            grpc_endpoint: default_grpc_endpoint(),
            grpc_fallback_ports: default_grpc_fallback_ports(),
            connect_timeout_secs: default_grpc_connect_timeout(),
            request_timeout_secs: default_grpc_request_timeout(),
            use_tls: false,
            mtls_enabled: false,
            tls_domain_name: None,
            tls_ca_cert_path: None,
            tls_client_cert_path: None,
            tls_client_key_path: None,
        }
    }
}

// ── NotificationConfig ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    #[serde(default = "default_notification_enabled")]
    pub enabled: bool,
    #[serde(default = "default_idle_notification")]
    pub idle_notification: bool,
    #[serde(default = "default_idle_notification_mins")]
    pub idle_notification_mins: u32,
    #[serde(default = "default_long_session_notification")]
    pub long_session_notification: bool,
    #[serde(default = "default_long_session_mins")]
    pub long_session_mins: u32,
    #[serde(default = "default_high_usage_notification")]
    pub high_usage_notification: bool,
    #[serde(default = "default_high_usage_threshold")]
    pub high_usage_threshold: u32,
    #[serde(default)]
    pub daily_summary_notification: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: default_notification_enabled(),
            idle_notification: default_idle_notification(),
            idle_notification_mins: default_idle_notification_mins(),
            long_session_notification: default_long_session_notification(),
            long_session_mins: default_long_session_mins(),
            high_usage_notification: default_high_usage_notification(),
            high_usage_threshold: default_high_usage_threshold(),
            daily_summary_notification: false,
        }
    }
}

// ── WebConfig ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_web_enabled")]
    pub enabled: bool,
    #[serde(default = "default_web_port")]
    pub port: u16,
    #[serde(default)]
    pub allow_external: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: default_web_enabled(),
            port: default_web_port(),
            allow_external: false,
        }
    }
}

// ── UpdateConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_update_enabled")]
    pub enabled: bool,
    #[serde(default = "default_repo_owner")]
    pub repo_owner: String,
    #[serde(default = "default_repo_name")]
    pub repo_name: String,
    #[serde(default = "default_check_interval_hours")]
    pub check_interval_hours: u32,
    #[serde(default)]
    pub include_prerelease: bool,
    #[serde(default)]
    pub auto_install: bool,
    #[serde(default = "default_update_require_signature")]
    pub require_signature_verification: bool,
    #[serde(default = "default_update_signature_public_key")]
    pub signature_public_key: String,
    #[serde(default)]
    pub min_allowed_version: Option<String>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: default_update_enabled(),
            repo_owner: default_repo_owner(),
            repo_name: default_repo_name(),
            check_interval_hours: default_check_interval_hours(),
            include_prerelease: false,
            auto_install: false,
            require_signature_verification: default_update_require_signature(),
            signature_public_key: default_update_signature_public_key(),
            min_allowed_version: None,
        }
    }
}

impl UpdateConfig {
    pub fn validate_integrity_policy(&self) -> Result<(), CoreError> {
        if !self.enabled {
            return Ok(());
        }

        if !self.require_signature_verification {
            return Err(CoreError::Config(
                "update.require_signature_verification must be true when updates are enabled"
                    .to_string(),
            ));
        }

        let key_b64 = self
            .signature_public_key
            .split_whitespace()
            .next()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                CoreError::Config(
                    "update.signature_public_key is missing while updates are enabled".to_string(),
                )
            })?;

        let key_bytes = BASE64.decode(key_b64).map_err(|e| {
            CoreError::Config(format!(
                "update.signature_public_key must be valid base64: {}",
                e
            ))
        })?;

        if key_bytes.len() != 32 {
            return Err(CoreError::Config(format!(
                "update.signature_public_key must decode to 32 bytes, got {}",
                key_bytes.len()
            )));
        }

        if let Some(version_floor) = self
            .min_allowed_version
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            semver::Version::parse(version_floor).map_err(|e| {
                CoreError::Config(format!(
                    "update.min_allowed_version must be valid semver: {}",
                    e
                ))
            })?;
        }

        Ok(())
    }
}

// ── ServerConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub base_url: String,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_sse_max_retry_secs")]
    pub sse_max_retry_secs: u64,
}

// ── MonitorConfig ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_sync_interval_ms")]
    pub sync_interval_ms: u64,
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: u64,
    #[serde(default = "default_idle_threshold_secs")]
    pub idle_threshold_secs: u64,
    #[serde(default = "default_process_interval_secs")]
    pub process_interval_secs: u64,
    #[serde(default = "default_true")]
    pub process_monitoring: bool,
    #[serde(default = "default_true")]
    pub input_activity: bool,
}

// ── StorageConfig ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub db_path: Option<PathBuf>,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_max_storage_mb")]
    pub max_storage_mb: u64,
}

// ── VisionConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    #[serde(default = "default_capture_enabled")]
    pub capture_enabled: bool,
    #[serde(default = "default_capture_throttle_ms")]
    pub capture_throttle_ms: u64,
    #[serde(default = "default_thumbnail_width")]
    pub thumbnail_width: u32,
    #[serde(default = "default_thumbnail_height")]
    pub thumbnail_height: u32,
    #[serde(default)]
    pub ocr_enabled: bool,
    #[serde(default)]
    pub privacy_mode: bool,
}

// ── validate_remote_endpoint (free function) ───────────────────────

fn validate_remote_endpoint(
    endpoint: Option<&ExternalApiEndpoint>,
    field_name: &str,
) -> Result<(), CoreError> {
    let endpoint = endpoint.ok_or_else(|| {
        CoreError::Config(format!(
            "`{field_name}` is required when a remote provider is selected."
        ))
    })?;

    let endpoint_url = endpoint.endpoint.trim();
    if endpoint_url.is_empty() {
        return Err(CoreError::Config(format!(
            "`{field_name}.endpoint` must not be empty."
        )));
    }
    if !(endpoint_url.starts_with("http://") || endpoint_url.starts_with("https://")) {
        return Err(CoreError::Config(format!(
            "`{field_name}.endpoint` must be an http:// or https:// URL."
        )));
    }

    if endpoint.api_key.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "`{field_name}.api_key` must not be empty."
        )));
    }

    if endpoint.timeout_secs == 0 {
        return Err(CoreError::Config(format!(
            "`{field_name}.timeout_secs` must be >= 1."
        )));
    }

    if let Some(model) = endpoint
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let decision = crate::ai_model_lifecycle_policy::evaluate_model_lifecycle_now(
            endpoint.provider_type,
            model,
        )?;
        if let crate::ai_model_lifecycle_policy::ModelLifecycleDecision::Block { message, .. } =
            decision
        {
            return Err(CoreError::PolicyDenied(message));
        }
    }

    Ok(())
}

// ── Default / helper functions ─────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_integrity_enabled() -> bool {
    true
}

fn default_active_start_hour() -> u8 {
    9
}

fn default_active_end_hour() -> u8 {
    18
}

fn default_active_days() -> Vec<Weekday> {
    vec![
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
    ]
}

fn default_excluded_extensions() -> Vec<String> {
    vec![
        ".tmp".to_string(),
        ".log".to_string(),
        ".lock".to_string(),
        ".swp".to_string(),
    ]
}

fn default_max_events_per_minute() -> u32 {
    100
}

fn default_api_timeout_secs() -> u64 {
    30
}

fn default_ocr_min_confidence() -> f64 {
    0.25
}

fn default_ocr_max_invalid_ratio() -> f64 {
    0.6
}

fn default_scene_min_confidence() -> f64 {
    0.35
}

fn default_scene_max_elements() -> usize {
    120
}

fn default_scene_calibration_min_elements() -> usize {
    8
}

fn default_scene_calibration_min_avg_confidence() -> f64 {
    0.55
}

fn default_grpc_endpoint() -> String {
    "http://localhost:50051".to_string()
}

fn default_grpc_fallback_ports() -> Vec<u16> {
    vec![50052, 50053]
}

fn default_grpc_connect_timeout() -> u64 {
    10
}

fn default_grpc_request_timeout() -> u64 {
    30
}

fn default_notification_enabled() -> bool {
    true
}

fn default_idle_notification() -> bool {
    true
}

fn default_idle_notification_mins() -> u32 {
    30
}

fn default_long_session_notification() -> bool {
    true
}

fn default_long_session_mins() -> u32 {
    60
}

fn default_high_usage_notification() -> bool {
    false
}

fn default_high_usage_threshold() -> u32 {
    90
}

fn default_web_enabled() -> bool {
    true
}

fn default_web_port() -> u16 {
    9090
}

fn default_update_enabled() -> bool {
    true
}

fn default_repo_owner() -> String {
    "pseudotop".to_string()
}

fn default_repo_name() -> String {
    "oneshim-client".to_string()
}

fn default_check_interval_hours() -> u32 {
    24
}

fn default_update_require_signature() -> bool {
    true
}

fn default_update_signature_public_key() -> String {
    "GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E=".to_string()
}

// Functions also used by AppConfig::default_config() in mod.rs
pub(super) fn default_request_timeout_ms() -> u64 {
    30_000
}
pub(super) fn default_sse_max_retry_secs() -> u64 {
    30
}
pub(super) fn default_poll_interval_ms() -> u64 {
    1_000
}
pub(super) fn default_sync_interval_ms() -> u64 {
    10_000
}
pub(super) fn default_heartbeat_interval_ms() -> u64 {
    30_000
}
pub(super) fn default_idle_threshold_secs() -> u64 {
    300 // 5 min
}
pub(super) fn default_process_interval_secs() -> u64 {
    10
}
pub(super) fn default_retention_days() -> u32 {
    30
}
pub(super) fn default_max_storage_mb() -> u64 {
    500
}
pub(super) fn default_capture_enabled() -> bool {
    true
}
pub(super) fn default_capture_throttle_ms() -> u64 {
    5_000
}
pub(super) fn default_thumbnail_width() -> u32 {
    480
}
pub(super) fn default_thumbnail_height() -> u32 {
    270
}
