use chrono::{DateTime, Utc};
use oneshim_api_contracts::provider_specs::{
    default_surface_id_for_access_mode as default_surface_id_from_catalog,
    surface_supports_capability as surface_supports_capability_from_catalog, SurfaceCapabilityKind,
};
use oneshim_api_contracts::settings::{
    AiProviderSettings, AppSettings, AutomationSettings, ExternalApiSettings,
    MonitorControlSettings, NotificationSettings, OcrValidationSettings, PrivacySettings,
    SandboxSettings, SceneActionOverrideSettings, SceneIntelligenceSettings, ScheduleSettings,
    StorageStats, TelemetrySettings, UpdateSettings,
};
use oneshim_core::config::{
    AiAccessMode, AiProviderType, AppConfig, CredentialAuthMode, CredentialBackendKind,
    CredentialBinding, ExternalApiEndpoint, ExternalDataPolicy, LlmProviderType, OcrProviderType,
    OcrValidationConfig, PiiFilterLevel, SandboxProfile, SceneActionOverrideConfig,
    SceneIntelligenceConfig, SecretRef, Weekday,
};
use oneshim_core::ports::secret_store::{provider_api_key_secret_ref, SecretStore, SecretStoreSet};
use oneshim_core::provider_surface::{
    canonical_provider_surface_id, provider_surface_spec, ProviderSurfaceTransport,
};
use std::sync::Arc;

use crate::error::ApiError;
use crate::AppState;

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
        config_to_settings(&config, state.default_secret_backend_kind)
    } else {
        AppSettings::default()
    }
}

pub async fn update_settings(state: &AppState, settings: &AppSettings) -> Result<(), ApiError> {
    validate_settings_input(settings)?;

    if let Some(ref config_manager) = state.config_manager {
        let previous_config = config_manager.get();
        let mut next_config = previous_config.clone();
        apply_settings_to_config(&mut next_config, settings)?;
        persist_api_key_bindings(
            &mut next_config,
            state.secret_store.clone(),
            state.secret_stores.as_ref(),
            state.default_secret_backend_kind,
        )
        .await?;

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

async fn persist_api_key_bindings(
    config: &mut AppConfig,
    secret_store: Option<Arc<dyn SecretStore>>,
    secret_stores: Option<&SecretStoreSet>,
    default_backend_kind: CredentialBackendKind,
) -> Result<(), ApiError> {
    let access_mode = config.ai_provider.access_mode;

    if let Some(endpoint) = config.ai_provider.ocr_api.as_mut() {
        persist_api_key_binding(
            endpoint,
            access_mode,
            ApiEndpointKind::Ocr,
            secret_store.clone(),
            secret_stores,
            default_backend_kind,
        )
        .await?;
    }

    if let Some(endpoint) = config.ai_provider.llm_api.as_mut() {
        persist_api_key_binding(
            endpoint,
            access_mode,
            ApiEndpointKind::Llm,
            secret_store,
            secret_stores,
            default_backend_kind,
        )
        .await?;
    }

    Ok(())
}

async fn persist_api_key_binding(
    endpoint: &mut ExternalApiEndpoint,
    access_mode: AiAccessMode,
    endpoint_kind: ApiEndpointKind,
    secret_store: Option<Arc<dyn SecretStore>>,
    secret_stores: Option<&SecretStoreSet>,
    default_backend_kind: CredentialBackendKind,
) -> Result<(), ApiError> {
    if endpoint
        .surface_id
        .as_deref()
        .is_some_and(surface_uses_no_auth)
    {
        endpoint.api_key.clear();
        endpoint.credential = None;
        return Ok(());
    }

    let auth_mode = endpoint
        .credential
        .as_ref()
        .map(|binding| binding.auth_mode)
        .unwrap_or_else(|| {
            derive_credential_auth_mode(endpoint.surface_id.as_deref(), access_mode, endpoint_kind)
        });

    if auth_mode != CredentialAuthMode::ApiKey {
        return Ok(());
    }

    let api_key = endpoint.api_key.trim();
    if api_key.is_empty() {
        return Ok(());
    }

    let backend_kind = endpoint
        .credential
        .as_ref()
        .map(|binding| binding.backend_kind)
        .unwrap_or(default_backend_kind);

    match backend_kind {
        CredentialBackendKind::Env => {
            return Err(ApiError::BadRequest(
                "Environment-backed provider credentials are read-only; update the environment source instead.".to_string(),
            ));
        }
        CredentialBackendKind::Unavailable => {
            return Err(ApiError::BadRequest(
                "No writable provider secret backend is available; configure a secret backend before saving API keys.".to_string(),
            ));
        }
        CredentialBackendKind::LegacyConfig => {
            endpoint.credential = Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::LegacyConfig,
                secret_ref: None,
                projection_enabled: endpoint
                    .credential
                    .as_ref()
                    .map(|binding| binding.projection_enabled)
                    .unwrap_or(false),
            });
            return Ok(());
        }
        CredentialBackendKind::BridgeManaged => {
            return Err(ApiError::BadRequest(
                "Bridge-managed credentials cannot be edited from Settings.".to_string(),
            ));
        }
        CredentialBackendKind::OsSecretStore | CredentialBackendKind::FileSecretStore => {}
    }

    let secret_store = secret_stores
        .and_then(|stores| stores.for_binding(endpoint.credential.as_ref()))
        .or(secret_store);

    let Some(secret_store) = secret_store else {
        return Err(ApiError::Internal(
            "Writable provider secret backend was selected, but no secret store was initialized."
                .to_string(),
        ));
    };

    let (namespace, key) = provider_api_key_secret_ref(
        provider_type_id(endpoint.provider_type),
        endpoint_kind.profile_id(),
    )
    .map_err(|e| ApiError::Internal(format!("Failed to derive secret namespace: {e}")))?;

    secret_store
        .store(&namespace, key, api_key)
        .await
        .map_err(|e| {
            ApiError::Internal(format!("Failed to persist API key to secret store: {e}"))
        })?;

    endpoint.credential = Some(CredentialBinding {
        auth_mode: CredentialAuthMode::ApiKey,
        backend_kind,
        secret_ref: Some(SecretRef {
            namespace,
            key: key.to_string(),
        }),
        projection_enabled: endpoint
            .credential
            .as_ref()
            .map(|binding| binding.projection_enabled)
            .unwrap_or(false),
    });
    endpoint.api_key.clear();

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
    let Some(logger) = state.audit_logger.clone() else {
        return;
    };
    let action_type = action_type.to_string();
    tokio::spawn(async move {
        logger.log_event(&action_type, "settings", &details).await;
    });
}

fn validate_settings_input(settings: &AppSettings) -> Result<(), ApiError> {
    if settings.retention_days == 0 || settings.retention_days > 365 {
        return Err(ApiError::BadRequest(
            "Retention period must be between 1 and 365 days.".to_string(),
        ));
    }
    if settings.max_storage_mb < 100 || settings.max_storage_mb > 10000 {
        return Err(ApiError::BadRequest(
            "Maximum storage size must be between 100 MB and 10 GB.".to_string(),
        ));
    }
    if settings.web_port < 1024 {
        return Err(ApiError::BadRequest(
            "web_port must be 1024 or higher.".to_string(),
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
            "ai_provider.ocr_validation.min_confidence must be within 0.0..=1.0.".to_string(),
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
            "ai_provider.ocr_validation.max_invalid_ratio must be within 0.0..=1.0.".to_string(),
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
            "ai_provider.scene_intelligence.min_confidence must be within 0.0..=1.0.".to_string(),
        ));
    }
    if settings.ai_provider.scene_intelligence.max_elements == 0
        || settings.ai_provider.scene_intelligence.max_elements > 1000
    {
        return Err(ApiError::BadRequest(
            "ai_provider.scene_intelligence.max_elements must be within 1..=1000.".to_string(),
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
            "ai_provider.scene_intelligence.calibration_min_elements must be within 1..=1000."
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
            "ai_provider.scene_intelligence.calibration_min_avg_confidence must be within 0.0..=1.0."
                .to_string(),
        ));
    }
    Ok(())
}

fn config_to_settings(
    config: &AppConfig,
    default_secret_backend_kind: CredentialBackendKind,
) -> AppSettings {
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
            ocr_api: config.ai_provider.ocr_api.as_ref().map(|endpoint| {
                endpoint_to_api_settings(
                    endpoint,
                    config.ai_provider.access_mode,
                    ApiEndpointKind::Ocr,
                    default_secret_backend_kind,
                )
            }),
            llm_api: config.ai_provider.llm_api.as_ref().map(|endpoint| {
                endpoint_to_api_settings(
                    endpoint,
                    config.ai_provider.access_mode,
                    ApiEndpointKind::Llm,
                    default_secret_backend_kind,
                )
            }),
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
            "Invalid privacy.pii_filter_level value: {value}"
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
            "Invalid schedule.active_days value: {value}"
        ))),
    }
}

fn parse_sandbox_profile(value: &str) -> Result<SandboxProfile, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "permissive" => Ok(SandboxProfile::Permissive),
        "standard" | "balanced" => Ok(SandboxProfile::Standard),
        "strict" => Ok(SandboxProfile::Strict),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid sandbox.profile value: {value}"
        ))),
    }
}

fn parse_ocr_provider(value: &str) -> Result<OcrProviderType, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => Ok(OcrProviderType::Local),
        "remote" => Ok(OcrProviderType::Remote),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.ocr_provider value: {value}"
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
        "provideroauth" | "provider_oauth" | "oauth" => Ok(AiAccessMode::ProviderOAuth),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.access_mode value: {value}"
        ))),
    }
}

fn parse_ai_provider_type(value: &str) -> Result<AiProviderType, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Ok(AiProviderType::Anthropic),
        "openai" | "open_ai" => Ok(AiProviderType::OpenAi),
        "google" => Ok(AiProviderType::Google),
        "ollama" => Ok(AiProviderType::Ollama),
        "llamaindex" | "llama-index" | "openai-compatible" | "openai-like" | "openai_like"
        | "openailike" => Ok(AiProviderType::Generic),
        "generic" => Ok(AiProviderType::Generic),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.api.provider_type value: {value}"
        ))),
    }
}

fn parse_llm_provider(value: &str) -> Result<LlmProviderType, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => Ok(LlmProviderType::Local),
        "remote" => Ok(LlmProviderType::Remote),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.llm_provider value: {value}"
        ))),
    }
}

fn provider_type_id(value: AiProviderType) -> &'static str {
    match value {
        AiProviderType::Anthropic => "anthropic",
        AiProviderType::OpenAi => "openai",
        AiProviderType::Google => "google",
        AiProviderType::Ollama => "ollama",
        AiProviderType::Generic => "generic",
    }
}

fn parse_external_data_policy(value: &str) -> Result<ExternalDataPolicy, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "piifilterstrict" => Ok(ExternalDataPolicy::PiiFilterStrict),
        "piifilterstandard" => Ok(ExternalDataPolicy::PiiFilterStandard),
        "allowfiltered" => Ok(ExternalDataPolicy::AllowFiltered),
        "disabled" => Ok(ExternalDataPolicy::PiiFilterStrict),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.external_data_policy value: {value}"
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
        let existing_endpoint = config.ai_provider.ocr_api.as_ref();
        config.ai_provider.ocr_api = Some(api_settings_to_endpoint(
            ocr_settings,
            existing_endpoint,
            config.ai_provider.access_mode,
            ApiEndpointKind::Ocr,
        )?);
    } else {
        config.ai_provider.ocr_api = None;
    }

    if let Some(ref llm_settings) = settings.ai_provider.llm_api {
        let existing_endpoint = config.ai_provider.llm_api.as_ref();
        config.ai_provider.llm_api = Some(api_settings_to_endpoint(
            llm_settings,
            existing_endpoint,
            config.ai_provider.access_mode,
            ApiEndpointKind::Llm,
        )?);
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

#[derive(Clone, Copy)]
enum ApiEndpointKind {
    Ocr,
    Llm,
}

impl ApiEndpointKind {
    fn profile_id(self) -> &'static str {
        match self {
            Self::Ocr => "ocr",
            Self::Llm => "llm",
        }
    }
}

fn endpoint_to_api_settings(
    endpoint: &ExternalApiEndpoint,
    access_mode: AiAccessMode,
    endpoint_kind: ApiEndpointKind,
    default_backend_kind: CredentialBackendKind,
) -> ExternalApiSettings {
    let no_auth_surface = endpoint
        .surface_id
        .as_deref()
        .is_some_and(surface_uses_no_auth);
    let binding = endpoint.credential.as_ref();
    let auth_mode = endpoint
        .credential
        .as_ref()
        .map(|binding| binding.auth_mode)
        .unwrap_or_else(|| {
            derive_credential_auth_mode(endpoint.surface_id.as_deref(), access_mode, endpoint_kind)
        });
    let has_plaintext_secret = !endpoint.api_key.trim().is_empty();
    let backend_kind = endpoint
        .credential
        .as_ref()
        .map(|binding| binding.backend_kind)
        .unwrap_or_else(|| {
            derive_credential_backend_kind(auth_mode, has_plaintext_secret, default_backend_kind)
        });
    let has_secret = !no_auth_surface
        && (has_plaintext_secret
            || binding
                .and_then(|value| value.secret_ref.as_ref())
                .is_some()
            || binding.is_some_and(|value| {
                value.auth_mode == CredentialAuthMode::ApiKey
                    && value.backend_kind == CredentialBackendKind::Env
            }));
    let masked_plaintext_secret = has_plaintext_secret.then(|| mask_api_key(&endpoint.api_key));

    ExternalApiSettings {
        endpoint: endpoint.endpoint.clone(),
        api_key_masked: masked_plaintext_secret.clone().unwrap_or_default(),
        model: endpoint.model.clone(),
        provider_type: format!("{:?}", endpoint.provider_type),
        surface_id: endpoint.surface_id.clone().or_else(|| {
            default_surface_id_for_endpoint(endpoint.provider_type, access_mode, endpoint_kind)
                .map(str::to_string)
        }),
        timeout_secs: endpoint.timeout_secs,
        auth_mode: credential_auth_mode_to_wire(auth_mode).to_string(),
        backend_kind: credential_backend_kind_to_wire(if no_auth_surface {
            CredentialBackendKind::Unavailable
        } else {
            backend_kind
        })
        .to_string(),
        has_secret,
        can_edit_secret: !no_auth_surface && can_edit_secret(auth_mode, backend_kind),
        secret_display_hint: masked_plaintext_secret,
        projection_enabled: binding
            .map(|value| value.projection_enabled)
            .unwrap_or(false),
    }
}

fn api_settings_to_endpoint(
    settings: &ExternalApiSettings,
    existing_endpoint: Option<&ExternalApiEndpoint>,
    access_mode: AiAccessMode,
    endpoint_kind: ApiEndpointKind,
) -> Result<ExternalApiEndpoint, ApiError> {
    let provider_type = parse_ai_provider_type(&settings.provider_type)?;
    let surface_id = resolve_endpoint_surface_id(
        settings,
        existing_endpoint,
        provider_type,
        access_mode,
        endpoint_kind,
    )?;
    let existing_key = existing_endpoint
        .map(|endpoint| endpoint.api_key.as_str())
        .unwrap_or("");
    let api_key = if is_managed_auth_mode(&settings.auth_mode)
        || surface_id.as_deref().is_some_and(surface_uses_no_auth)
    {
        String::new()
    } else if is_masked_key(&settings.api_key_masked) || settings.api_key_masked.is_empty() {
        existing_key.to_string()
    } else {
        settings.api_key_masked.clone()
    };

    let credential =
        updated_credential_binding(settings, existing_endpoint, surface_id.as_deref())?;

    Ok(ExternalApiEndpoint {
        endpoint: settings.endpoint.clone(),
        api_key,
        model: settings.model.clone(),
        timeout_secs: settings.timeout_secs,
        provider_type,
        surface_id,
        credential,
    })
}

fn default_surface_id_for_endpoint(
    provider_type: AiProviderType,
    access_mode: AiAccessMode,
    endpoint_kind: ApiEndpointKind,
) -> Option<&'static str> {
    let direct_default = || {
        default_surface_id_from_catalog(
            provider_type,
            AiAccessMode::ProviderApiKey,
            match endpoint_kind {
                ApiEndpointKind::Ocr => SurfaceCapabilityKind::Ocr,
                ApiEndpointKind::Llm => SurfaceCapabilityKind::Llm,
            },
        )
        .ok()
        .flatten()
    };

    default_surface_id_from_catalog(
        provider_type,
        access_mode,
        match endpoint_kind {
            ApiEndpointKind::Ocr => SurfaceCapabilityKind::Ocr,
            ApiEndpointKind::Llm => SurfaceCapabilityKind::Llm,
        },
    )
    .ok()
    .flatten()
    .or_else(|| {
        matches!(endpoint_kind, ApiEndpointKind::Ocr)
            .then(direct_default)
            .flatten()
    })
}

fn resolve_endpoint_surface_id(
    settings: &ExternalApiSettings,
    existing_endpoint: Option<&ExternalApiEndpoint>,
    provider_type: AiProviderType,
    access_mode: AiAccessMode,
    endpoint_kind: ApiEndpointKind,
) -> Result<Option<String>, ApiError> {
    let auth_mode = parse_credential_auth_mode(&settings.auth_mode)?;
    let requested = settings
        .surface_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| existing_endpoint.and_then(|endpoint| endpoint.surface_id.clone()));

    let default_surface =
        default_surface_id_for_endpoint(provider_type, access_mode, endpoint_kind)
            .map(str::to_string);
    let resolved = requested.or(default_surface);

    let Some(surface_id) = resolved else {
        return Ok(None);
    };

    let Some(spec) = provider_surface_spec(&surface_id) else {
        return Err(ApiError::BadRequest(format!(
            "Unsupported ai_provider.api.surface_id value: {surface_id}"
        )));
    };

    if spec.provider_type != provider_type {
        return Err(ApiError::BadRequest(format!(
            "Provider surface '{surface_id}' does not match provider_type '{}'.",
            provider_type_id(provider_type)
        )));
    }

    let expected_transport = transport_for_auth_mode(auth_mode);

    if spec.transport != expected_transport {
        return Err(ApiError::BadRequest(format!(
            "Provider surface '{surface_id}' is incompatible with access_mode '{:?}' for this endpoint.",
            access_mode
        )));
    }

    if matches!(endpoint_kind, ApiEndpointKind::Llm) {
        match access_mode {
            AiAccessMode::ProviderOAuth
                if expected_transport != ProviderSurfaceTransport::ManagedOAuth =>
            {
                return Err(ApiError::BadRequest(
                    "ProviderOAuth mode requires an LLM provider surface with managed_oauth transport."
                        .to_string(),
                ));
            }
            AiAccessMode::ProviderSubscriptionCli
                if expected_transport != ProviderSurfaceTransport::SubprocessCli =>
            {
                return Err(ApiError::BadRequest(
                    "ProviderSubscriptionCli mode requires an LLM provider surface with subprocess_cli transport."
                        .to_string(),
                ));
            }
            _ => {}
        }
    }

    let required_capability = match endpoint_kind {
        ApiEndpointKind::Ocr => SurfaceCapabilityKind::Ocr,
        ApiEndpointKind::Llm => SurfaceCapabilityKind::Llm,
    };
    if !surface_supports_capability_from_catalog(&surface_id, required_capability)
        .map_err(ApiError::BadRequest)?
    {
        return Err(ApiError::BadRequest(format!(
            "Provider surface '{surface_id}' does not support the selected endpoint capability."
        )));
    }

    Ok(canonical_provider_surface_id(&surface_id).map(str::to_string))
}

fn derive_credential_auth_mode(
    surface_id: Option<&str>,
    access_mode: AiAccessMode,
    endpoint_kind: ApiEndpointKind,
) -> CredentialAuthMode {
    if let Some(surface_id) = surface_id {
        if let Some(spec) = provider_surface_spec(surface_id) {
            return match spec.transport {
                ProviderSurfaceTransport::ManagedOAuth => CredentialAuthMode::ManagedOAuth,
                ProviderSurfaceTransport::SubprocessCli => CredentialAuthMode::CliBridge,
                ProviderSurfaceTransport::DirectApi => CredentialAuthMode::ApiKey,
            };
        }
    }

    match (access_mode, endpoint_kind) {
        (AiAccessMode::ProviderOAuth, ApiEndpointKind::Llm) => CredentialAuthMode::ManagedOAuth,
        (AiAccessMode::ProviderSubscriptionCli, ApiEndpointKind::Llm) => {
            CredentialAuthMode::CliBridge
        }
        _ => CredentialAuthMode::ApiKey,
    }
}

fn transport_for_auth_mode(auth_mode: CredentialAuthMode) -> ProviderSurfaceTransport {
    match auth_mode {
        CredentialAuthMode::ManagedOAuth => ProviderSurfaceTransport::ManagedOAuth,
        CredentialAuthMode::CliBridge => ProviderSurfaceTransport::SubprocessCli,
        CredentialAuthMode::ApiKey => ProviderSurfaceTransport::DirectApi,
    }
}

fn derive_credential_backend_kind(
    auth_mode: CredentialAuthMode,
    has_plaintext_secret: bool,
    default_backend_kind: CredentialBackendKind,
) -> CredentialBackendKind {
    match auth_mode {
        CredentialAuthMode::ManagedOAuth => CredentialBackendKind::OsSecretStore,
        CredentialAuthMode::CliBridge => CredentialBackendKind::BridgeManaged,
        CredentialAuthMode::ApiKey if has_plaintext_secret => CredentialBackendKind::LegacyConfig,
        CredentialAuthMode::ApiKey => default_backend_kind,
    }
}

fn updated_credential_binding(
    settings: &ExternalApiSettings,
    existing_endpoint: Option<&ExternalApiEndpoint>,
    resolved_surface_id: Option<&str>,
) -> Result<Option<CredentialBinding>, ApiError> {
    if resolved_surface_id.is_some_and(surface_uses_no_auth) {
        return Ok(None);
    }

    let auth_mode = parse_credential_auth_mode(&settings.auth_mode)?;
    let backend_kind = parse_credential_backend_kind(&settings.backend_kind)?;
    validate_projection_binding(auth_mode, backend_kind, settings.projection_enabled)?;

    if matches!(
        auth_mode,
        CredentialAuthMode::ManagedOAuth | CredentialAuthMode::CliBridge
    ) {
        return Ok(Some(CredentialBinding {
            auth_mode,
            backend_kind,
            secret_ref: None,
            projection_enabled: false,
        }));
    }

    if let Some(mut binding) = existing_endpoint.and_then(|endpoint| endpoint.credential.clone()) {
        if binding.auth_mode != auth_mode {
            binding = CredentialBinding {
                auth_mode,
                backend_kind,
                secret_ref: None,
                projection_enabled: settings.projection_enabled,
            };
            return Ok(Some(binding));
        }
        if binding.backend_kind != backend_kind {
            return Err(ApiError::BadRequest(
                "Changing provider credential auth mode or backend for an existing endpoint is not supported from Settings. Use the dedicated migration or reconnect flow instead.".to_string(),
            ));
        }
        binding.projection_enabled = settings.projection_enabled;
        return Ok(Some(binding));
    }

    if settings.projection_enabled
        || !matches!(
            backend_kind,
            CredentialBackendKind::LegacyConfig | CredentialBackendKind::Unavailable
        )
    {
        return Ok(Some(CredentialBinding {
            auth_mode,
            backend_kind,
            secret_ref: None,
            projection_enabled: settings.projection_enabled,
        }));
    }

    Ok(None)
}

fn validate_projection_binding(
    auth_mode: CredentialAuthMode,
    backend_kind: CredentialBackendKind,
    projection_enabled: bool,
) -> Result<(), ApiError> {
    if !projection_enabled {
        return Ok(());
    }

    if auth_mode != CredentialAuthMode::ApiKey {
        return Err(ApiError::BadRequest(
            "Projection can only be enabled for API-key credentials.".to_string(),
        ));
    }

    if !matches!(
        backend_kind,
        CredentialBackendKind::OsSecretStore | CredentialBackendKind::FileSecretStore
    ) {
        return Err(ApiError::BadRequest(format!(
            "Projection is not supported for backend kind {:?}.",
            backend_kind
        )));
    }

    Ok(())
}

fn surface_uses_no_auth(surface_id: &str) -> bool {
    oneshim_api_contracts::provider_specs::provider_surface_spec(surface_id)
        .map(|surface| {
            surface
                .llm_transport
                .as_ref()
                .is_some_and(|transport| transport.auth_scheme.eq_ignore_ascii_case("none"))
                || surface
                    .ocr_transport
                    .as_ref()
                    .is_some_and(|transport| transport.auth_scheme.eq_ignore_ascii_case("none"))
        })
        .unwrap_or(false)
}

fn parse_credential_auth_mode(value: &str) -> Result<CredentialAuthMode, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "api_key" | "apikey" => Ok(CredentialAuthMode::ApiKey),
        "managed_oauth" | "managedoauth" => Ok(CredentialAuthMode::ManagedOAuth),
        "cli_bridge" | "clibridge" => Ok(CredentialAuthMode::CliBridge),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.api.auth_mode value: {value}"
        ))),
    }
}

fn parse_credential_backend_kind(value: &str) -> Result<CredentialBackendKind, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "os_secret_store" | "ossecretstore" => Ok(CredentialBackendKind::OsSecretStore),
        "file_secret_store" | "filesecretstore" => Ok(CredentialBackendKind::FileSecretStore),
        "env" => Ok(CredentialBackendKind::Env),
        "bridge_managed" | "bridgemanaged" => Ok(CredentialBackendKind::BridgeManaged),
        "legacy_config" | "legacyconfig" => Ok(CredentialBackendKind::LegacyConfig),
        "unavailable" => Ok(CredentialBackendKind::Unavailable),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.api.backend_kind value: {value}"
        ))),
    }
}

fn can_edit_secret(auth_mode: CredentialAuthMode, backend_kind: CredentialBackendKind) -> bool {
    matches!(auth_mode, CredentialAuthMode::ApiKey)
        && matches!(
            backend_kind,
            CredentialBackendKind::OsSecretStore
                | CredentialBackendKind::FileSecretStore
                | CredentialBackendKind::LegacyConfig
        )
}

fn credential_auth_mode_to_wire(value: CredentialAuthMode) -> &'static str {
    match value {
        CredentialAuthMode::ApiKey => "api_key",
        CredentialAuthMode::ManagedOAuth => "managed_oauth",
        CredentialAuthMode::CliBridge => "cli_bridge",
    }
}

fn credential_backend_kind_to_wire(value: CredentialBackendKind) -> &'static str {
    match value {
        CredentialBackendKind::OsSecretStore => "os_secret_store",
        CredentialBackendKind::FileSecretStore => "file_secret_store",
        CredentialBackendKind::Env => "env",
        CredentialBackendKind::BridgeManaged => "bridge_managed",
        CredentialBackendKind::LegacyConfig => "legacy_config",
        CredentialBackendKind::Unavailable => "unavailable",
    }
}

fn is_managed_auth_mode(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "managed_oauth" | "cli_bridge"
    )
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
            "{field_name} must use RFC3339 format. Example: 2026-02-24T03:00:00Z"
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
    use async_trait::async_trait;
    use oneshim_core::config_manager::ConfigManager;
    use oneshim_core::error::CoreError;
    use oneshim_core::ports::secret_store::SecretStore;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    struct TestSecretStore {
        values: Mutex<HashMap<(String, String), String>>,
    }

    impl TestSecretStore {
        fn new() -> Self {
            Self {
                values: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl SecretStore for TestSecretStore {
        async fn store(&self, namespace: &str, key: &str, value: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .insert((namespace.to_string(), key.to_string()), value.to_string());
            Ok(())
        }

        async fn retrieve(&self, namespace: &str, key: &str) -> Result<Option<String>, CoreError> {
            Ok(self
                .values
                .lock()
                .unwrap()
                .get(&(namespace.to_string(), key.to_string()))
                .cloned())
        }

        async fn delete(&self, namespace: &str, key: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .remove(&(namespace.to_string(), key.to_string()));
            Ok(())
        }

        async fn delete_namespace(&self, namespace: &str) -> Result<(), CoreError> {
            self.values
                .lock()
                .unwrap()
                .retain(|(existing_namespace, _), _| existing_namespace != namespace);
            Ok(())
        }
    }

    fn test_state_without_config_manager() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            default_secret_backend_kind: oneshim_core::config::CredentialBackendKind::LegacyConfig,
            secret_store: None,
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        }
    }

    fn test_state_with_config_manager(
        config_manager: ConfigManager,
        secret_store: Option<Arc<dyn SecretStore>>,
    ) -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: Some(config_manager),
            default_secret_backend_kind: oneshim_core::config::CredentialBackendKind::OsSecretStore,
            secret_store,
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        }
    }

    #[tokio::test]
    async fn update_settings_validates_input_without_config_manager() {
        let state = test_state_without_config_manager();
        let settings = AppSettings {
            web_port: 80,
            ..AppSettings::default()
        };

        let result = update_settings(&state, &settings).await;
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }

    #[tokio::test]
    async fn update_settings_accepts_valid_defaults_without_config_manager() {
        let state = test_state_without_config_manager();
        let settings = AppSettings::default();

        let result = update_settings(&state, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_settings_accepts_provider_oauth_roundtrip_without_config_manager() {
        let state = test_state_without_config_manager();
        let mut settings = AppSettings::default();
        settings.ai_provider.access_mode = "ProviderOAuth".to_string();
        settings.ai_provider.llm_provider = "Remote".to_string();

        let result = update_settings(&state, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_settings_persists_remote_api_key_to_secret_store_and_binding_metadata() {
        let temp_dir = TempDir::new().expect("temp dir");
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).expect("config manager");
        let secret_store = Arc::new(TestSecretStore::new()) as Arc<dyn SecretStore>;
        let state =
            test_state_with_config_manager(config_manager.clone(), Some(secret_store.clone()));

        let mut settings = AppSettings::default();
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: "sk-secret-123456".to_string(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: None,
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: true,
            can_edit_secret: true,
            secret_display_hint: None,
            projection_enabled: true,
        });

        update_settings(&state, &settings)
            .await
            .expect("settings update should succeed");

        let stored = secret_store
            .retrieve("provider/openai/llm", "api_key")
            .await
            .expect("secret lookup");
        assert_eq!(stored.as_deref(), Some("sk-secret-123456"));

        let saved = config_manager.get();
        let endpoint = saved.ai_provider.llm_api.expect("saved llm endpoint");
        let binding = endpoint.credential.expect("credential binding");
        assert_eq!(endpoint.api_key, "");
        assert_eq!(binding.backend_kind, CredentialBackendKind::OsSecretStore);
        assert_eq!(binding.auth_mode, CredentialAuthMode::ApiKey);
        assert!(binding.projection_enabled);
        let secret_ref = binding.secret_ref.expect("secret ref");
        assert_eq!(secret_ref.namespace, "provider/openai/llm");
        assert_eq!(secret_ref.key, "api_key");
    }

    #[test]
    fn config_to_settings_marks_plaintext_api_keys_as_legacy_config() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.example.com/v1".to_string(),
            api_key: "sk-test-1234567890".to_string(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 45,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.auth_mode, "api_key");
        assert_eq!(llm_api.backend_kind, "legacy_config");
        assert!(llm_api.has_secret);
        assert!(llm_api.can_edit_secret);
        assert_eq!(llm_api.secret_display_hint.as_deref(), Some("sk...7890"));
        assert!(!llm_api.projection_enabled);
    }

    #[test]
    fn config_to_settings_marks_ollama_surface_as_no_auth() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "http://localhost:11434/v1/responses".to_string(),
            api_key: String::new(),
            model: Some("qwen3:8b".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::Ollama,
            surface_id: Some("provider_surface.ollama.local_http".to_string()),
            credential: None,
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.auth_mode, "api_key");
        assert_eq!(llm_api.backend_kind, "unavailable");
        assert!(!llm_api.has_secret);
        assert!(!llm_api.can_edit_secret);
        assert!(llm_api.api_key_masked.is_empty());
    }

    #[test]
    fn config_to_settings_marks_provider_oauth_as_managed_oauth_metadata() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderOAuth;
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.auth_mode, "managed_oauth");
        assert_eq!(llm_api.backend_kind, "os_secret_store");
        assert_eq!(
            llm_api.surface_id.as_deref(),
            Some("provider_surface.openai.managed_oauth")
        );
        assert!(!llm_api.has_secret);
        assert!(!llm_api.can_edit_secret);
        assert_eq!(llm_api.secret_display_hint, None);
    }

    #[test]
    fn config_to_settings_marks_cli_subscription_as_cli_bridge_metadata() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.llm_provider = LlmProviderType::Local;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: String::new(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: None,
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.auth_mode, "cli_bridge");
        assert_eq!(llm_api.backend_kind, "bridge_managed");
        assert_eq!(
            llm_api.surface_id.as_deref(),
            Some("provider_surface.openai.subprocess_cli")
        );
        assert!(!llm_api.can_edit_secret);
        assert_eq!(llm_api.secret_display_hint, None);
    }

    #[test]
    fn config_to_settings_keeps_backend_managed_api_key_without_fake_mask() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: false,
            }),
        });

        let settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.backend_kind, "os_secret_store");
        assert!(llm_api.has_secret);
        assert_eq!(llm_api.api_key_masked, "");
        assert_eq!(llm_api.secret_display_hint, None);
    }

    #[test]
    fn config_to_settings_marks_env_bound_api_key_as_present_without_secret_ref() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::Env,
                secret_ref: None,
                projection_enabled: false,
            }),
        });

        let settings = config_to_settings(&config, CredentialBackendKind::Env);
        let llm_api = settings.ai_provider.llm_api.expect("llm api settings");

        assert_eq!(llm_api.backend_kind, "env");
        assert!(llm_api.has_secret);
        assert!(!llm_api.can_edit_secret);
        assert_eq!(llm_api.api_key_masked, "");
        assert_eq!(llm_api.secret_display_hint, None);
    }

    #[test]
    fn apply_settings_to_config_preserves_projection_enabled_on_existing_binding() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: false,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings.ai_provider.llm_api.as_mut().expect("llm settings");
        llm_api.projection_enabled = true;

        apply_settings_to_config(&mut config, &settings).expect("config update");

        let binding = config
            .ai_provider
            .llm_api
            .as_ref()
            .and_then(|endpoint| endpoint.credential.as_ref())
            .expect("binding");
        assert!(binding.projection_enabled);
    }

    #[test]
    fn apply_settings_to_config_rejects_projection_enabled_for_env_backend() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::Env);
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "openai".to_string(),
            surface_id: None,
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "env".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: true,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("projection guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_rejects_projection_enabled_for_managed_oauth() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderOAuth;
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "openai".to_string(),
            surface_id: None,
            timeout_secs: 30,
            auth_mode: "managed_oauth".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: true,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("projection guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_rejects_incompatible_surface_id_for_oauth_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderOAuth;
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "openai".to_string(),
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            timeout_secs: 30,
            auth_mode: "managed_oauth".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("surface guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_rejects_projection_enabled_for_bridge_managed_backend() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;

        let settings = AppSettings {
            ai_provider: AiProviderSettings {
                llm_provider: "Remote".to_string(),
                llm_api: Some(ExternalApiSettings {
                    endpoint: "https://api.openai.com/v1".to_string(),
                    api_key_masked: String::new(),
                    model: Some("gpt-5.4".to_string()),
                    provider_type: "OpenAi".to_string(),
                    surface_id: None,
                    timeout_secs: 30,
                    auth_mode: "api_key".to_string(),
                    backend_kind: "bridge_managed".to_string(),
                    has_secret: true,
                    can_edit_secret: false,
                    secret_display_hint: None,
                    projection_enabled: true,
                }),
                ..AiProviderSettings::default()
            },
            ..AppSettings::default()
        };

        let err = apply_settings_to_config(&mut config, &settings).unwrap_err();
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("Projection is not supported"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn apply_settings_to_config_rewrites_binding_when_switching_to_oauth_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test-123456".to_string(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: true,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderOAuth".to_string();
        let llm_api = settings
            .ai_provider
            .llm_api
            .as_mut()
            .expect("llm api settings");
        llm_api.surface_id = Some("provider_surface.openai.managed_oauth".to_string());
        llm_api.auth_mode = "managed_oauth".to_string();
        llm_api.backend_kind = "os_secret_store".to_string();
        llm_api.projection_enabled = false;

        apply_settings_to_config(&mut config, &settings).expect("oauth mode rewrite");

        let endpoint = config.ai_provider.llm_api.as_ref().expect("llm endpoint");
        let binding = endpoint.credential.as_ref().expect("credential binding");
        assert_eq!(binding.auth_mode, CredentialAuthMode::ManagedOAuth);
        assert_eq!(binding.backend_kind, CredentialBackendKind::OsSecretStore);
        assert!(binding.secret_ref.is_none());
        assert!(!binding.projection_enabled);
        assert!(endpoint.api_key.is_empty());
    }

    #[test]
    fn apply_settings_to_config_rewrites_binding_when_switching_from_cli_bridge_to_api_key() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.llm_provider = LlmProviderType::Local;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: String::new(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.subprocess_cli".to_string()),
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::CliBridge,
                backend_kind: CredentialBackendKind::BridgeManaged,
                secret_ref: None,
                projection_enabled: false,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderApiKey".to_string();
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "os_secret_store".to_string(),
            has_secret: false,
            can_edit_secret: true,
            secret_display_hint: None,
            projection_enabled: false,
        });

        apply_settings_to_config(&mut config, &settings).expect("api key mode rewrite");

        let endpoint = config.ai_provider.llm_api.as_ref().expect("llm endpoint");
        let binding = endpoint.credential.as_ref().expect("credential binding");
        assert_eq!(binding.auth_mode, CredentialAuthMode::ApiKey);
        assert_eq!(binding.backend_kind, CredentialBackendKind::OsSecretStore);
        assert!(binding.secret_ref.is_none());
        assert_eq!(
            endpoint.surface_id.as_deref(),
            Some("provider_surface.openai.direct_api")
        );
    }

    #[test]
    fn apply_settings_to_config_allows_direct_ocr_surface_in_cli_mode() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderSubscriptionCli".to_string();
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key_masked: "sk-ocr-123456".to_string(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "legacy_config".to_string(),
            has_secret: true,
            can_edit_secret: true,
            secret_display_hint: None,
            projection_enabled: false,
        });

        apply_settings_to_config(&mut config, &settings).expect("cli mode should keep direct OCR");

        let endpoint = config.ai_provider.ocr_api.as_ref().expect("ocr endpoint");
        assert_eq!(
            endpoint.surface_id.as_deref(),
            Some("provider_surface.openai.direct_api")
        );
        assert_eq!(endpoint.api_key, "sk-ocr-123456");
    }

    #[test]
    fn apply_settings_to_config_rejects_llm_only_cli_surface_for_ocr() {
        let mut config = AppConfig::default_config();
        config.ai_provider.access_mode = AiAccessMode::ProviderSubscriptionCli;
        config.ai_provider.ocr_provider = OcrProviderType::Remote;

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.access_mode = "ProviderSubscriptionCli".to_string();
        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: String::new(),
            api_key_masked: String::new(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: Some("provider_surface.openai.subprocess_cli".to_string()),
            timeout_secs: 30,
            auth_mode: "cli_bridge".to_string(),
            backend_kind: "bridge_managed".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err = apply_settings_to_config(&mut config, &settings).expect_err("ocr surface guard");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn apply_settings_to_config_clears_secret_binding_for_ollama_surface() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: Some("provider_surface.openai.direct_api".to_string()),
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: true,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "http://localhost:11434/v1/responses".to_string(),
            api_key_masked: String::new(),
            model: Some("qwen3:8b".to_string()),
            provider_type: "Ollama".to_string(),
            surface_id: Some("provider_surface.ollama.local_http".to_string()),
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "unavailable".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        apply_settings_to_config(&mut config, &settings).expect("ollama no-auth save should work");

        let endpoint = config.ai_provider.llm_api.expect("saved endpoint");
        assert_eq!(endpoint.provider_type, AiProviderType::Ollama);
        assert_eq!(
            endpoint.surface_id.as_deref(),
            Some("provider_surface.ollama.local_http")
        );
        assert!(endpoint.api_key.is_empty());
        assert!(endpoint.credential.is_none());
    }

    #[test]
    fn apply_settings_to_config_rejects_backend_change_for_existing_binding() {
        let mut config = AppConfig::default_config();
        config.ai_provider.llm_provider = LlmProviderType::Remote;
        config.ai_provider.llm_api = Some(ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: Some(SecretRef {
                    namespace: "provider/openai/llm".to_string(),
                    key: "api_key".to_string(),
                }),
                projection_enabled: false,
            }),
        });

        let mut settings = config_to_settings(&config, CredentialBackendKind::OsSecretStore);
        let llm_api = settings
            .ai_provider
            .llm_api
            .as_mut()
            .expect("llm api settings");
        llm_api.backend_kind = "file_secret_store".to_string();

        let err = apply_settings_to_config(&mut config, &settings).unwrap_err();
        match err {
            ApiError::BadRequest(message) => {
                assert!(message.contains("Changing provider credential auth mode or backend"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_settings_rejects_api_key_write_for_env_backend() {
        let temp_dir = TempDir::new().expect("temp dir");
        let config_path = temp_dir.path().join("config.json");
        let config_manager = ConfigManager::with_path(config_path).expect("config manager");
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        let state = AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: Some(config_manager),
            default_secret_backend_kind: CredentialBackendKind::Env,
            secret_store: Some(Arc::new(TestSecretStore::new())),
            secret_stores: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        };

        let mut settings = AppSettings::default();
        settings.ai_provider.llm_provider = "Remote".to_string();
        settings.ai_provider.llm_api = Some(ExternalApiSettings {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key_masked: "sk-secret-123456".to_string(),
            model: Some("gpt-5.4".to_string()),
            provider_type: "OpenAi".to_string(),
            surface_id: None,
            timeout_secs: 30,
            auth_mode: "api_key".to_string(),
            backend_kind: "env".to_string(),
            has_secret: false,
            can_edit_secret: false,
            secret_display_hint: None,
            projection_enabled: false,
        });

        let err = update_settings(&state, &settings)
            .await
            .expect_err("env backend should be read-only");

        assert!(matches!(err, ApiError::BadRequest(_)));
    }
}
