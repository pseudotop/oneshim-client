use serde::Serialize;

use crate::setup::SecretBackendCapabilities;
use crate::subprocess_provider::{
    detect_known_cli_surfaces, DetectedSubprocessCli, SubprocessCliSurfaceId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum FeatureMaturity {
    Stable,
    Beta,
    Experimental,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureAvailability {
    Available,
    Unavailable,
    PartiallyAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureCapability {
    pub feature_id: String,
    pub maturity: FeatureMaturity,
    pub availability: FeatureAvailability,
    pub preferred: bool,
    pub requires: Vec<String>,
    pub status_reason: Option<String>,
    pub status_copy_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureCapabilitySnapshot {
    pub features: Vec<FeatureCapability>,
}

pub struct FeatureCapabilityState(pub FeatureCapabilitySnapshot);

pub fn build_feature_capability_snapshot(
    secret_backend: &SecretBackendCapabilities,
) -> FeatureCapabilitySnapshot {
    let detected_surfaces = detect_known_cli_surfaces();

    FeatureCapabilitySnapshot {
        features: vec![
            managed_oauth_feature(secret_backend),
            subprocess_cli_feature(
                SubprocessCliSurfaceId::OpenAiCodex,
                &detected_surfaces,
                "featureCapability.providerSurface.openaiSubprocessCli",
            ),
            subprocess_cli_feature(
                SubprocessCliSurfaceId::AnthropicClaudeCode,
                &detected_surfaces,
                "featureCapability.providerSurface.anthropicSubprocessCli",
            ),
            subprocess_cli_feature(
                SubprocessCliSurfaceId::GoogleGeminiCli,
                &detected_surfaces,
                "featureCapability.providerSurface.googleSubprocessCli",
            ),
        ],
    }
}

fn managed_oauth_feature(secret_backend: &SecretBackendCapabilities) -> FeatureCapability {
    let available = secret_backend.oauth_available && secret_backend.os_secret_store_available;
    FeatureCapability {
        feature_id: "provider_surface.openai.managed_oauth".to_string(),
        maturity: FeatureMaturity::Experimental,
        availability: if available {
            FeatureAvailability::Available
        } else {
            FeatureAvailability::Unavailable
        },
        preferred: false,
        requires: vec!["os_secret_store".to_string()],
        status_reason: if available {
            None
        } else {
            Some("os_secret_store_unavailable".to_string())
        },
        status_copy_key: Some(if available {
            "featureCapability.providerSurface.openaiManagedOAuth.available".to_string()
        } else {
            "featureCapability.providerSurface.openaiManagedOAuth.unavailable".to_string()
        }),
    }
}

fn subprocess_cli_feature(
    surface_id: SubprocessCliSurfaceId,
    detected_surfaces: &[DetectedSubprocessCli],
    copy_key_prefix: &str,
) -> FeatureCapability {
    let detected = detected_surfaces
        .iter()
        .any(|surface| surface.surface_id == surface_id);
    let runtime_supported = surface_id.runtime_supported();
    FeatureCapability {
        feature_id: surface_id.feature_id().to_string(),
        maturity: FeatureMaturity::Experimental,
        availability: if detected {
            FeatureAvailability::PartiallyAvailable
        } else {
            FeatureAvailability::Unavailable
        },
        preferred: true,
        requires: vec![format!("cli:{}", surface_id.cli_id())],
        status_reason: Some(if detected {
            if runtime_supported {
                "cli_detected_auth_unverified".to_string()
            } else {
                "cli_detected_runtime_pending".to_string()
            }
        } else {
            "cli_not_installed".to_string()
        }),
        status_copy_key: Some(if detected {
            format!("{copy_key_prefix}.partiallyAvailable")
        } else {
            format!("{copy_key_prefix}.unavailable")
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend_caps(oauth_available: bool) -> SecretBackendCapabilities {
        SecretBackendCapabilities {
            os_secret_store_available: oauth_available,
            oauth_available,
            default_backend_kind: "os_secret_store".into(),
            byok_backend_kind: "os_secret_store".into(),
            fallback_backend_kind: "legacy_config".into(),
        }
    }

    #[test]
    fn managed_oauth_feature_is_experimental_and_unavailable_without_keychain() {
        let feature = managed_oauth_feature(&backend_caps(false));
        assert_eq!(feature.maturity, FeatureMaturity::Experimental);
        assert_eq!(feature.availability, FeatureAvailability::Unavailable);
        assert!(!feature.preferred);
    }

    #[test]
    fn subprocess_cli_feature_is_preferred_but_partial_when_cli_detected() {
        let feature = subprocess_cli_feature(
            SubprocessCliSurfaceId::OpenAiCodex,
            &[DetectedSubprocessCli {
                surface_id: SubprocessCliSurfaceId::OpenAiCodex,
                executable_path: "/usr/bin/codex".into(),
            }],
            "featureCapability.providerSurface.openaiSubprocessCli",
        );
        assert_eq!(
            feature.availability,
            FeatureAvailability::PartiallyAvailable
        );
        assert!(feature.preferred);
        assert_eq!(feature.requires, vec!["cli:codex".to_string()]);
        assert_eq!(
            feature.status_reason.as_deref(),
            Some("cli_detected_auth_unverified")
        );
    }

    #[test]
    fn snapshot_contains_expected_feature_ids() {
        let snapshot = build_feature_capability_snapshot(&backend_caps(true));
        let ids: Vec<&str> = snapshot
            .features
            .iter()
            .map(|feature| feature.feature_id.as_str())
            .collect();
        assert!(ids.contains(&"provider_surface.openai.managed_oauth"));
        assert!(ids.contains(&"provider_surface.openai.subprocess_cli"));
        assert!(ids.contains(&"provider_surface.anthropic.subprocess_cli"));
        assert!(ids.contains(&"provider_surface.google.subprocess_cli"));
    }
}
