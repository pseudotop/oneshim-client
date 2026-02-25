use chrono::{DateTime, Utc};
use oneshim_core::config::{
    AiAccessMode, AiProviderType, AppConfig, ExternalApiEndpoint, ExternalDataPolicy,
    LlmProviderType, OcrProviderType, OcrValidationConfig, PiiFilterLevel, SandboxProfile,
    SceneActionOverrideConfig, SceneIntelligenceConfig, Weekday,
};

use crate::error::ApiError;
use crate::handlers::settings::{
    AiProviderSettings, AppSettings, AutomationSettings, ExternalApiSettings,
    MonitorControlSettings, NotificationSettings, OcrValidationSettings, PrivacySettings,
    SandboxSettings, SceneActionOverrideSettings, SceneIntelligenceSettings, ScheduleSettings,
    StorageStats, TelemetrySettings, UpdateSettings,
};
use crate::AppState;
use tracing::warn;

pub fn get_storage_stats(state: &AppState) -> Result<StorageStats, ApiError> {
    let stats = state
        .storage
        .get_storage_stats_summary()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let db_size_bytes = stats.page_count * stats.page_size;
    let frames_size_bytes = if let Some(ref frames_dir) = state.frames_dir {
        calculate_dir_size(frames_dir)
    } else {
        0
    };

    Ok(StorageStats {
        db_size_bytes,
        frames_size_bytes,
        total_size_bytes: db_size_bytes + frames_size_bytes,
        frame_count: stats.frame_count,
        event_count: stats.event_count,
        metric_count: stats.metric_count,
        oldest_data_date: stats.oldest_data_date,
        newest_data_date: stats.newest_data_date,
    })
}

pub fn get_settings(state: &AppState) -> AppSettings {
    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        config_to_settings(&config)
    } else {
        AppSettings::default()
    }
}

pub fn update_settings(state: &AppState, settings: &AppSettings) -> Result<(), ApiError> {
    validate_settings_input(settings)?;

    if let Some(ref config_manager) = state.config_manager {
        let previous_config = config_manager.get();
        let mut next_config = previous_config.clone();
        apply_settings_to_config(&mut next_config, settings)?;

        next_config
            .ai_provider
            .validate_selected_remote_endpoints()
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;

        config_manager
            .update(next_config.clone())
            .map_err(|e| ApiError::Internal(format!("Failed to save settings: {e}")))?;

        emit_policy_change_events(state, &previous_config, &next_config);
    }

    Ok(())
}

fn emit_policy_change_events(state: &AppState, previous: &AppConfig, next: &AppConfig) {
    if previous.ai_provider.allow_unredacted_external_ocr
        != next.ai_provider.allow_unredacted_external_ocr
    {
        log_policy_event(
            state,
            "policy.settings.allow_unredacted_external_ocr.changed",
            format!(
                "from={} to={}",
                previous.ai_provider.allow_unredacted_external_ocr,
                next.ai_provider.allow_unredacted_external_ocr
            ),
        );
    }

    let prev_override = &previous.ai_provider.scene_action_override;
    let next_override = &next.ai_provider.scene_action_override;
    let override_changed = prev_override.enabled != next_override.enabled
        || prev_override.reason != next_override.reason
        || prev_override.approved_by != next_override.approved_by
        || prev_override.expires_at != next_override.expires_at;

    if override_changed {
        log_policy_event(
            state,
            "policy.settings.scene_action_override.changed",
            format!(
                "from_enabled={} to_enabled={} from_reason={:?} to_reason={:?} from_approved_by={:?} to_approved_by={:?} from_expires_at={:?} to_expires_at={:?}",
                prev_override.enabled,
                next_override.enabled,
                prev_override.reason.as_deref(),
                next_override.reason.as_deref(),
                prev_override.approved_by.as_deref(),
                next_override.approved_by.as_deref(),
                prev_override.expires_at.map(|value| value.to_rfc3339()),
                next_override.expires_at.map(|value| value.to_rfc3339()),
            ),
        );
    }

    let prev_scene = &previous.ai_provider.scene_intelligence;
    let next_scene = &next.ai_provider.scene_intelligence;
    let scene_changed = prev_scene.enabled != next_scene.enabled
        || prev_scene.overlay_enabled != next_scene.overlay_enabled
        || prev_scene.allow_action_execution != next_scene.allow_action_execution
        || (prev_scene.min_confidence - next_scene.min_confidence).abs() > f64::EPSILON
        || prev_scene.max_elements != next_scene.max_elements
        || prev_scene.calibration_enabled != next_scene.calibration_enabled
        || prev_scene.calibration_min_elements != next_scene.calibration_min_elements
        || (prev_scene.calibration_min_avg_confidence - next_scene.calibration_min_avg_confidence)
            .abs()
            > f64::EPSILON;

    if scene_changed {
        log_policy_event(
            state,
            "policy.settings.scene_intelligence.changed",
            format!(
                "enabled {}->{} overlay {}->{} allow_action_execution {}->{} min_confidence {:.2}->{:.2} max_elements {}->{} calibration_enabled {}->{} calibration_min_elements {}->{} calibration_min_avg_confidence {:.2}->{:.2}",
                prev_scene.enabled,
                next_scene.enabled,
                prev_scene.overlay_enabled,
                next_scene.overlay_enabled,
                prev_scene.allow_action_execution,
                next_scene.allow_action_execution,
                prev_scene.min_confidence,
                next_scene.min_confidence,
                prev_scene.max_elements,
                next_scene.max_elements,
                prev_scene.calibration_enabled,
                next_scene.calibration_enabled,
                prev_scene.calibration_min_elements,
                next_scene.calibration_min_elements,
                prev_scene.calibration_min_avg_confidence,
                next_scene.calibration_min_avg_confidence,
            ),
        );
    }
}

fn log_policy_event(state: &AppState, action_type: &str, details: String) {
    let Some(logger) = state.audit_logger.as_ref() else {
        return;
    };

    match logger.try_write() {
        Ok(mut guard) => guard.log_event(action_type, "settings", &details),
        Err(_) => warn!(
            action_type = action_type,
            "audit logger busy; policy setting change event was dropped"
        ),
    }
}

fn validate_settings_input(settings: &AppSettings) -> Result<(), ApiError> {
    if settings.retention_days == 0 || settings.retention_days > 365 {
        return Err(ApiError::BadRequest(
            "보존 period은 1-365일 사이여야 합니다".to_string(),
        ));
    }
    if settings.max_storage_mb < 100 || settings.max_storage_mb > 10000 {
        return Err(ApiError::BadRequest(
            "최대 save소 용량은 100MB-10GB 사이여야 합니다".to_string(),
        ));
    }
    if settings.web_port < 1024 {
        return Err(ApiError::BadRequest(
            "port는 1024 이상이어야 합니다".to_string(),
        ));
    }
    if !settings
        .ai_provider
        .ocr_validation
        .min_confidence
        .is_finite()
        || !(0.0..=1.0).contains(&settings.ai_provider.ocr_validation.min_confidence)
    {
        return Err(ApiError::BadRequest(
            "ai_provider.ocr_validation.min_confidence는 0.0~1.0 범위여야 합니다".to_string(),
        ));
    }
    if !settings
        .ai_provider
        .ocr_validation
        .max_invalid_ratio
        .is_finite()
        || !(0.0..=1.0).contains(&settings.ai_provider.ocr_validation.max_invalid_ratio)
    {
        return Err(ApiError::BadRequest(
            "ai_provider.ocr_validation.max_invalid_ratio는 0.0~1.0 범위여야 합니다".to_string(),
        ));
    }
    if !settings
        .ai_provider
        .scene_intelligence
        .min_confidence
        .is_finite()
        || !(0.0..=1.0).contains(&settings.ai_provider.scene_intelligence.min_confidence)
    {
        return Err(ApiError::BadRequest(
            "ai_provider.scene_intelligence.min_confidence는 0.0~1.0 범위여야 합니다".to_string(),
        ));
    }
    if settings.ai_provider.scene_intelligence.max_elements == 0
        || settings.ai_provider.scene_intelligence.max_elements > 1000
    {
        return Err(ApiError::BadRequest(
            "ai_provider.scene_intelligence.max_elements는 1~1000 범위여야 합니다".to_string(),
        ));
    }
    if settings
        .ai_provider
        .scene_intelligence
        .calibration_min_elements
        == 0
        || settings
            .ai_provider
            .scene_intelligence
            .calibration_min_elements
            > 1000
    {
        return Err(ApiError::BadRequest(
            "ai_provider.scene_intelligence.calibration_min_elements는 1~1000 범위여야 합니다"
                .to_string(),
        ));
    }
    if !settings
        .ai_provider
        .scene_intelligence
        .calibration_min_avg_confidence
        .is_finite()
        || !(0.0..=1.0).contains(
            &settings
                .ai_provider
                .scene_intelligence
                .calibration_min_avg_confidence,
        )
    {
        return Err(ApiError::BadRequest(
            "ai_provider.scene_intelligence.calibration_min_avg_confidence는 0.0~1.0 범위여야 합니다"
                .to_string(),
        ));
    }
    Ok(())
}

fn config_to_settings(config: &AppConfig) -> AppSettings {
    AppSettings {
        retention_days: config.storage.retention_days,
        max_storage_mb: config.storage.max_storage_mb as u32,
        web_port: config.web.port,
        allow_external: config.web.allow_external,
        capture_enabled: config.vision.capture_enabled,
        idle_threshold_secs: config.monitor.idle_threshold_secs as u32,
        metrics_interval_secs: (config.monitor.poll_interval_ms / 1000) as u32,
        process_interval_secs: config.monitor.process_interval_secs as u32,
        notification: NotificationSettings {
            enabled: config.notification.enabled,
            idle_notification: config.notification.idle_notification,
            idle_notification_mins: config.notification.idle_notification_mins,
            long_session_notification: config.notification.long_session_notification,
            long_session_mins: config.notification.long_session_mins,
            high_usage_notification: config.notification.high_usage_notification,
            high_usage_threshold: config.notification.high_usage_threshold,
        },
        update: UpdateSettings {
            enabled: config.update.enabled,
            check_interval_hours: config.update.check_interval_hours,
            include_prerelease: config.update.include_prerelease,
            auto_install: config.update.auto_install,
        },
        telemetry: TelemetrySettings {
            enabled: config.telemetry.enabled,
            crash_reports: config.telemetry.crash_reports,
            usage_analytics: config.telemetry.usage_analytics,
            performance_metrics: config.telemetry.performance_metrics,
        },
        monitor: MonitorControlSettings {
            process_monitoring: config.monitor.process_monitoring,
            input_activity: config.monitor.input_activity,
            privacy_mode: config.vision.privacy_mode,
        },
        privacy: PrivacySettings {
            excluded_apps: config.privacy.excluded_apps.clone(),
            excluded_app_patterns: config.privacy.excluded_app_patterns.clone(),
            excluded_title_patterns: config.privacy.excluded_title_patterns.clone(),
            auto_exclude_sensitive: config.privacy.auto_exclude_sensitive,
            pii_filter_level: format!("{:?}", config.privacy.pii_filter_level),
        },
        schedule: ScheduleSettings {
            active_hours_enabled: config.schedule.active_hours_enabled,
            active_start_hour: config.schedule.active_start_hour,
            active_end_hour: config.schedule.active_end_hour,
            active_days: config
                .schedule
                .active_days
                .iter()
                .map(|d| format!("{:?}", d))
                .collect(),
            pause_on_screen_lock: config.schedule.pause_on_screen_lock,
            pause_on_battery_saver: config.schedule.pause_on_battery_saver,
        },
        automation: AutomationSettings {
            enabled: config.automation.enabled,
        },
        sandbox: SandboxSettings {
            enabled: config.automation.sandbox.enabled,
            profile: format!("{:?}", config.automation.sandbox.profile),
            allowed_read_paths: config.automation.sandbox.allowed_read_paths.clone(),
            allowed_write_paths: config.automation.sandbox.allowed_write_paths.clone(),
            allow_network: config.automation.sandbox.allow_network,
            max_memory_bytes: config.automation.sandbox.max_memory_bytes,
            max_cpu_time_ms: config.automation.sandbox.max_cpu_time_ms,
        },
        ai_provider: AiProviderSettings {
            access_mode: format!("{:?}", config.ai_provider.access_mode),
            ocr_provider: format!("{:?}", config.ai_provider.ocr_provider),
            llm_provider: format!("{:?}", config.ai_provider.llm_provider),
            external_data_policy: format!("{:?}", config.ai_provider.external_data_policy),
            allow_unredacted_external_ocr: config.ai_provider.allow_unredacted_external_ocr,
            ocr_validation: OcrValidationSettings {
                enabled: config.ai_provider.ocr_validation.enabled,
                min_confidence: config.ai_provider.ocr_validation.min_confidence,
                max_invalid_ratio: config.ai_provider.ocr_validation.max_invalid_ratio,
            },
            scene_action_override: SceneActionOverrideSettings {
                enabled: config.ai_provider.scene_action_override.enabled,
                reason: config
                    .ai_provider
                    .scene_action_override
                    .reason
                    .clone()
                    .unwrap_or_default(),
                approved_by: config
                    .ai_provider
                    .scene_action_override
                    .approved_by
                    .clone()
                    .unwrap_or_default(),
                expires_at: config
                    .ai_provider
                    .scene_action_override
                    .expires_at
                    .map(|v| v.to_rfc3339()),
            },
            scene_intelligence: SceneIntelligenceSettings {
                enabled: config.ai_provider.scene_intelligence.enabled,
                overlay_enabled: config.ai_provider.scene_intelligence.overlay_enabled,
                allow_action_execution: config
                    .ai_provider
                    .scene_intelligence
                    .allow_action_execution,
                min_confidence: config.ai_provider.scene_intelligence.min_confidence,
                max_elements: config.ai_provider.scene_intelligence.max_elements as u32,
                calibration_enabled: config.ai_provider.scene_intelligence.calibration_enabled,
                calibration_min_elements: config
                    .ai_provider
                    .scene_intelligence
                    .calibration_min_elements as u32,
                calibration_min_avg_confidence: config
                    .ai_provider
                    .scene_intelligence
                    .calibration_min_avg_confidence,
            },
            fallback_to_local: config.ai_provider.fallback_to_local,
            ocr_api: config
                .ai_provider
                .ocr_api
                .as_ref()
                .map(endpoint_to_api_settings),
            llm_api: config
                .ai_provider
                .llm_api
                .as_ref()
                .map(endpoint_to_api_settings),
        },
    }
}

fn parse_pii_filter_level(value: &str) -> Result<PiiFilterLevel, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Ok(PiiFilterLevel::Off),
        "basic" => Ok(PiiFilterLevel::Basic),
        "standard" => Ok(PiiFilterLevel::Standard),
        "strict" => Ok(PiiFilterLevel::Strict),
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 privacy.pii_filter_level 값: {value}"
        ))),
    }
}

fn parse_weekday(value: &str) -> Result<Weekday, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mon" => Ok(Weekday::Mon),
        "tue" => Ok(Weekday::Tue),
        "wed" => Ok(Weekday::Wed),
        "thu" => Ok(Weekday::Thu),
        "fri" => Ok(Weekday::Fri),
        "sat" => Ok(Weekday::Sat),
        "sun" => Ok(Weekday::Sun),
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 schedule.active_days 값: {value}"
        ))),
    }
}

fn parse_sandbox_profile(value: &str) -> Result<SandboxProfile, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "permissive" => Ok(SandboxProfile::Permissive),
        "standard" | "balanced" => Ok(SandboxProfile::Standard),
        "strict" => Ok(SandboxProfile::Strict),
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 sandbox.profile 값: {value}"
        ))),
    }
}

fn parse_ocr_provider(value: &str) -> Result<OcrProviderType, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => Ok(OcrProviderType::Local),
        "remote" => Ok(OcrProviderType::Remote),
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 ai_provider.ocr_provider 값: {value}"
        ))),
    }
}

fn parse_ai_access_mode(value: &str) -> Result<AiAccessMode, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "providerapikey" | "provider_api_key" | "api" | "apikey" => {
            Ok(AiAccessMode::ProviderApiKey)
        }
        "localmodel" | "local_model" | "local" => Ok(AiAccessMode::LocalModel),
        "providersubscriptioncli" | "provider_subscription_cli" | "cli" | "subscription" => {
            Ok(AiAccessMode::ProviderSubscriptionCli)
        }
        "platformconnected" | "platform_connected" | "platform" => {
            Ok(AiAccessMode::PlatformConnected)
        }
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 ai_provider.access_mode 값: {value}"
        ))),
    }
}

fn parse_ai_provider_type(value: &str) -> Result<AiProviderType, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Ok(AiProviderType::Anthropic),
        "openai" | "open_ai" | "openai-compatible" => Ok(AiProviderType::OpenAi),
        "google" => Ok(AiProviderType::Google),
        "generic" => Ok(AiProviderType::Generic),
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 ai_provider.api.provider_type 값: {value}"
        ))),
    }
}

fn parse_llm_provider(value: &str) -> Result<LlmProviderType, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => Ok(LlmProviderType::Local),
        "remote" => Ok(LlmProviderType::Remote),
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 ai_provider.llm_provider 값: {value}"
        ))),
    }
}

fn parse_external_data_policy(value: &str) -> Result<ExternalDataPolicy, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "piifilterstrict" => Ok(ExternalDataPolicy::PiiFilterStrict),
        "piifilterstandard" => Ok(ExternalDataPolicy::PiiFilterStandard),
        "allowfiltered" => Ok(ExternalDataPolicy::AllowFiltered),
        "disabled" => Ok(ExternalDataPolicy::PiiFilterStrict),
        _ => Err(ApiError::BadRequest(format!(
            "유효하지 않은 ai_provider.external_data_policy 값: {value}"
        ))),
    }
}

pub(crate) fn apply_settings_to_config(
    config: &mut AppConfig,
    settings: &AppSettings,
) -> Result<(), ApiError> {
    config.storage.retention_days = settings.retention_days;
    config.storage.max_storage_mb = settings.max_storage_mb as u64;
    config.web.port = settings.web_port;
    config.web.allow_external = settings.allow_external;
    config.vision.capture_enabled = settings.capture_enabled;
    config.monitor.poll_interval_ms = (settings.metrics_interval_secs as u64) * 1000;
    config.monitor.idle_threshold_secs = settings.idle_threshold_secs as u64;
    config.monitor.process_interval_secs = settings.process_interval_secs as u64;
    config.notification.enabled = settings.notification.enabled;
    config.notification.idle_notification = settings.notification.idle_notification;
    config.notification.idle_notification_mins = settings.notification.idle_notification_mins;
    config.notification.long_session_notification = settings.notification.long_session_notification;
    config.notification.long_session_mins = settings.notification.long_session_mins;
    config.notification.high_usage_notification = settings.notification.high_usage_notification;
    config.notification.high_usage_threshold = settings.notification.high_usage_threshold;
    config.update.enabled = settings.update.enabled;
    config.update.check_interval_hours = settings.update.check_interval_hours;
    config.update.include_prerelease = settings.update.include_prerelease;
    config.update.auto_install = settings.update.auto_install;
    config.telemetry.enabled = settings.telemetry.enabled;
    config.telemetry.crash_reports = settings.telemetry.crash_reports;
    config.telemetry.usage_analytics = settings.telemetry.usage_analytics;
    config.telemetry.performance_metrics = settings.telemetry.performance_metrics;
    config.monitor.process_monitoring = settings.monitor.process_monitoring;
    config.monitor.input_activity = settings.monitor.input_activity;
    config.vision.privacy_mode = settings.monitor.privacy_mode;
    config.privacy.excluded_apps = settings.privacy.excluded_apps.clone();
    config.privacy.excluded_app_patterns = settings.privacy.excluded_app_patterns.clone();
    config.privacy.excluded_title_patterns = settings.privacy.excluded_title_patterns.clone();
    config.privacy.auto_exclude_sensitive = settings.privacy.auto_exclude_sensitive;
    config.privacy.pii_filter_level = parse_pii_filter_level(&settings.privacy.pii_filter_level)?;
    config.schedule.active_hours_enabled = settings.schedule.active_hours_enabled;
    config.schedule.active_start_hour = settings.schedule.active_start_hour;
    config.schedule.active_end_hour = settings.schedule.active_end_hour;
    config.schedule.active_days = settings
        .schedule
        .active_days
        .iter()
        .map(|d| parse_weekday(d))
        .collect::<Result<Vec<_>, _>>()?;
    config.schedule.pause_on_screen_lock = settings.schedule.pause_on_screen_lock;
    config.schedule.pause_on_battery_saver = settings.schedule.pause_on_battery_saver;
    config.automation.enabled = settings.automation.enabled;
    config.automation.sandbox.enabled = settings.sandbox.enabled;
    config.automation.sandbox.profile = parse_sandbox_profile(&settings.sandbox.profile)?;
    config.automation.sandbox.allowed_read_paths = settings.sandbox.allowed_read_paths.clone();
    config.automation.sandbox.allowed_write_paths = settings.sandbox.allowed_write_paths.clone();
    config.automation.sandbox.allow_network = settings.sandbox.allow_network;
    config.automation.sandbox.max_memory_bytes = settings.sandbox.max_memory_bytes;
    config.automation.sandbox.max_cpu_time_ms = settings.sandbox.max_cpu_time_ms;
    config.ai_provider.access_mode = parse_ai_access_mode(&settings.ai_provider.access_mode)?;
    config.ai_provider.ocr_provider = parse_ocr_provider(&settings.ai_provider.ocr_provider)?;
    config.ai_provider.llm_provider = parse_llm_provider(&settings.ai_provider.llm_provider)?;
    config.ai_provider.external_data_policy =
        parse_external_data_policy(&settings.ai_provider.external_data_policy)?;
    config.ai_provider.allow_unredacted_external_ocr =
        settings.ai_provider.allow_unredacted_external_ocr;
    config.ai_provider.ocr_validation = OcrValidationConfig {
        enabled: settings.ai_provider.ocr_validation.enabled,
        min_confidence: settings.ai_provider.ocr_validation.min_confidence,
        max_invalid_ratio: settings.ai_provider.ocr_validation.max_invalid_ratio,
    };
    config.ai_provider.scene_action_override = SceneActionOverrideConfig {
        enabled: settings.ai_provider.scene_action_override.enabled,
        reason: trim_to_option(&settings.ai_provider.scene_action_override.reason),
        approved_by: trim_to_option(&settings.ai_provider.scene_action_override.approved_by),
        expires_at: parse_optional_rfc3339_utc(
            settings
                .ai_provider
                .scene_action_override
                .expires_at
                .as_deref(),
            "ai_provider.scene_action_override.expires_at",
        )?,
    };
    config.ai_provider.scene_intelligence = SceneIntelligenceConfig {
        enabled: settings.ai_provider.scene_intelligence.enabled,
        overlay_enabled: settings.ai_provider.scene_intelligence.overlay_enabled,
        allow_action_execution: settings
            .ai_provider
            .scene_intelligence
            .allow_action_execution,
        min_confidence: settings.ai_provider.scene_intelligence.min_confidence,
        max_elements: settings.ai_provider.scene_intelligence.max_elements as usize,
        calibration_enabled: settings.ai_provider.scene_intelligence.calibration_enabled,
        calibration_min_elements: settings
            .ai_provider
            .scene_intelligence
            .calibration_min_elements as usize,
        calibration_min_avg_confidence: settings
            .ai_provider
            .scene_intelligence
            .calibration_min_avg_confidence,
    };
    config.ai_provider.fallback_to_local = settings.ai_provider.fallback_to_local;

    if let Some(ref ocr_settings) = settings.ai_provider.ocr_api {
        let existing_key = config
            .ai_provider
            .ocr_api
            .as_ref()
            .map(|e| e.api_key.as_str())
            .unwrap_or("");
        config.ai_provider.ocr_api = Some(api_settings_to_endpoint(ocr_settings, existing_key)?);
    } else {
        config.ai_provider.ocr_api = None;
    }

    if let Some(ref llm_settings) = settings.ai_provider.llm_api {
        let existing_key = config
            .ai_provider
            .llm_api
            .as_ref()
            .map(|e| e.api_key.as_str())
            .unwrap_or("");
        config.ai_provider.llm_api = Some(api_settings_to_endpoint(llm_settings, existing_key)?);
    } else {
        config.ai_provider.llm_api = None;
    }

    Ok(())
}

pub(crate) fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..2], &key[key.len() - 4..])
}

pub(crate) fn is_masked_key(value: &str) -> bool {
    value.contains("...") && value.len() <= 12
}

fn endpoint_to_api_settings(endpoint: &ExternalApiEndpoint) -> ExternalApiSettings {
    ExternalApiSettings {
        endpoint: endpoint.endpoint.clone(),
        api_key_masked: mask_api_key(&endpoint.api_key),
        model: endpoint.model.clone(),
        provider_type: format!("{:?}", endpoint.provider_type),
        timeout_secs: endpoint.timeout_secs,
    }
}

fn api_settings_to_endpoint(
    settings: &ExternalApiSettings,
    existing_key: &str,
) -> Result<ExternalApiEndpoint, ApiError> {
    let api_key = if is_masked_key(&settings.api_key_masked) || settings.api_key_masked.is_empty() {
        existing_key.to_string()
    } else {
        settings.api_key_masked.clone()
    };

    Ok(ExternalApiEndpoint {
        endpoint: settings.endpoint.clone(),
        api_key,
        model: settings.model.clone(),
        timeout_secs: settings.timeout_secs,
        provider_type: parse_ai_provider_type(&settings.provider_type)?,
    })
}

fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_optional_rfc3339_utc(
    value: Option<&str>,
    field_name: &str,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let parsed = DateTime::parse_from_rfc3339(trimmed).map_err(|_| {
        ApiError::BadRequest(format!(
            "{field_name}는 RFC3339 형식이어야 합니다. 예: 2026-02-24T03:00:00Z"
        ))
    })?;

    Ok(Some(parsed.with_timezone(&Utc)))
}

fn calculate_dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0;

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = std::fs::metadata(&path) {
                    total += metadata.len();
                }
            } else if path.is_dir() {
                total += calculate_dir_size(&path);
            }
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    fn test_state_without_config_manager() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        }
    }

    #[test]
    fn update_settings_validates_input_without_config_manager() {
        let state = test_state_without_config_manager();
        let settings = AppSettings {
            web_port: 80,
            ..AppSettings::default()
        };

        let result = update_settings(&state, &settings);
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn update_settings_accepts_valid_defaults_without_config_manager() {
        let state = test_state_without_config_manager();
        let settings = AppSettings::default();

        let result = update_settings(&state, &settings);
        assert!(result.is_ok());
    }
}
