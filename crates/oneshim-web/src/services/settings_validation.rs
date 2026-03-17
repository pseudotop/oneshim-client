use chrono::{DateTime, Utc};
use oneshim_api_contracts::settings::AppSettings;
use oneshim_core::config::{
    AiAccessMode, AiProviderType, CredentialAuthMode, CredentialBackendKind, ExternalDataPolicy,
    LlmProviderType, OcrProviderType, PiiFilterLevel, SandboxProfile, Weekday,
};
use oneshim_core::ports::secret_store::validate_secret_segment;
use oneshim_core::provider_surface::provider_type_from_vendor_id;
use std::collections::HashSet;

use crate::error::ApiError;

pub(crate) fn validate_settings_input(settings: &AppSettings) -> Result<(), ApiError> {
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
    validate_ai_provider_profiles_input(settings)?;
    Ok(())
}

fn validate_ai_provider_profiles_input(settings: &AppSettings) -> Result<(), ApiError> {
    let mut seen_profile_ids = HashSet::new();

    for profile in settings.ai_provider.saved_profiles.iter() {
        validate_secret_segment(
            &profile.profile_id,
            "ai_provider.saved_profiles[].profile_id",
        )
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

        if profile.name.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "ai_provider.saved_profiles[].name must not be empty.".to_string(),
            ));
        }

        if !seen_profile_ids.insert(profile.profile_id.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "Duplicate ai_provider.saved_profiles profile_id: {}",
                profile.profile_id
            )));
        }
    }

    if let Some(active_profile_id) = settings
        .ai_provider
        .active_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !settings
            .ai_provider
            .saved_profiles
            .iter()
            .any(|profile| profile.profile_id == active_profile_id)
        {
            return Err(ApiError::BadRequest(format!(
                "ai_provider.active_profile_id references an unknown saved profile: {active_profile_id}"
            )));
        }
    }

    Ok(())
}

pub(crate) fn parse_pii_filter_level(value: &str) -> Result<PiiFilterLevel, ApiError> {
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

pub(crate) fn parse_weekday(value: &str) -> Result<Weekday, ApiError> {
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

pub(crate) fn parse_sandbox_profile(value: &str) -> Result<SandboxProfile, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "permissive" => Ok(SandboxProfile::Permissive),
        "standard" | "balanced" => Ok(SandboxProfile::Standard),
        "strict" => Ok(SandboxProfile::Strict),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid sandbox.profile value: {value}"
        ))),
    }
}

pub(crate) fn parse_ocr_provider(value: &str) -> Result<OcrProviderType, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => Ok(OcrProviderType::Local),
        "remote" => Ok(OcrProviderType::Remote),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.ocr_provider value: {value}"
        ))),
    }
}

pub(crate) fn parse_ai_access_mode(value: &str) -> Result<AiAccessMode, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "providerapikey" | "provider_api_key" | "api" | "apikey" => {
            Ok(AiAccessMode::ProviderApiKey)
        }
        "localmodel" | "local_model" | "local" => Ok(AiAccessMode::LocalModel),
        "providersubscriptioncli" | "provider_subscription_cli" | "cli" | "subscription" => {
            Ok(AiAccessMode::ProviderSubscriptionCli)
        }
        "provideroauth" | "provider_oauth" | "oauth" => Ok(AiAccessMode::ProviderOAuth),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.access_mode value: {value}"
        ))),
    }
}

pub(crate) fn parse_ai_provider_type(value: &str) -> Result<AiProviderType, ApiError> {
    provider_type_from_vendor_id(value).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "Invalid ai_provider.api.provider_type value: {value}"
        ))
    })
}

pub(crate) fn parse_llm_provider(value: &str) -> Result<LlmProviderType, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "local" => Ok(LlmProviderType::Local),
        "remote" => Ok(LlmProviderType::Remote),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.llm_provider value: {value}"
        ))),
    }
}

pub(crate) fn parse_external_data_policy(value: &str) -> Result<ExternalDataPolicy, ApiError> {
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

pub(crate) fn parse_credential_auth_mode(value: &str) -> Result<CredentialAuthMode, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "api_key" | "apikey" => Ok(CredentialAuthMode::ApiKey),
        "managed_oauth" | "managedoauth" => Ok(CredentialAuthMode::ManagedOAuth),
        "cli_bridge" | "clibridge" => Ok(CredentialAuthMode::CliBridge),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.api.auth_mode value: {value}"
        ))),
    }
}

pub(crate) fn parse_credential_backend_kind(
    value: &str,
) -> Result<CredentialBackendKind, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "os_secret_store" | "ossecretstore" => Ok(CredentialBackendKind::OsSecretStore),
        "file_secret_store" | "filesecretstore" => Ok(CredentialBackendKind::FileSecretStore),
        "env" => Ok(CredentialBackendKind::Env),
        "bridge_managed" | "bridgemanaged" => Ok(CredentialBackendKind::BridgeManaged),
        "unavailable" => Ok(CredentialBackendKind::Unavailable),
        _ => Err(ApiError::BadRequest(format!(
            "Invalid ai_provider.api.backend_kind value: {value}"
        ))),
    }
}

pub(crate) fn is_managed_auth_mode(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "managed_oauth" | "cli_bridge"
    )
}

pub(crate) fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn parse_optional_rfc3339_utc(
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
