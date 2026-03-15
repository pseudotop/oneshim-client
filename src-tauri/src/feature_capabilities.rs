use serde::Serialize;
use std::time::Duration;

use crate::setup::SecretBackendCapabilities;
use crate::subprocess_provider::{
    probe_for_surface_id, probe_known_cli_surfaces, ProbedSubprocessCli, SubprocessCliAuthStatus,
};
use oneshim_api_contracts::provider_specs::{
    parse_surface_execution_kind, parse_surface_placement_kind, parse_surface_stability,
    provider_surface_catalog, subprocess_runtime_supported, ProviderAuthScheme,
    ProviderSurfaceSpec, SurfaceExecutionKind, SurfacePlacementKind, SurfaceStability,
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

pub struct FeatureCapabilityState(pub SecretBackendCapabilities);

pub async fn build_feature_capability_snapshot(
    secret_backend: &SecretBackendCapabilities,
) -> FeatureCapabilitySnapshot {
    let detected_surfaces = tokio::task::spawn_blocking(probe_known_cli_surfaces)
        .await
        .unwrap_or_default();
    build_feature_capability_snapshot_with_probes(secret_backend, &detected_surfaces).await
}

async fn build_feature_capability_snapshot_with_probes(
    secret_backend: &SecretBackendCapabilities,
    detected_surfaces: &[ProbedSubprocessCli],
) -> FeatureCapabilitySnapshot {
    let catalog = match provider_surface_catalog() {
        Ok(catalog) => catalog,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "Failed to load provider surface catalog for feature capability snapshot."
            );
            return FeatureCapabilitySnapshot {
                features: Vec::new(),
            };
        }
    };

    let mut features = Vec::new();
    for surface in &catalog.surfaces {
        match parse_surface_execution_kind(&surface.execution_kind) {
            Ok(SurfaceExecutionKind::ManagedHttp)
                if surface
                    .credential_kind
                    .eq_ignore_ascii_case("managed_oauth") =>
            {
                features.push(managed_oauth_feature(surface, secret_backend));
            }
            Ok(SurfaceExecutionKind::SubprocessCli) => {
                features.push(subprocess_cli_feature(surface, detected_surfaces));
            }
            Ok(SurfaceExecutionKind::DirectHttp) if surface.availability_probe.is_some() => {
                features.push(probed_http_surface_feature(surface).await);
            }
            _ => {}
        }
    }

    FeatureCapabilitySnapshot { features }
}

fn managed_oauth_feature(
    surface: &ProviderSurfaceSpec,
    secret_backend: &SecretBackendCapabilities,
) -> FeatureCapability {
    let available = secret_backend.oauth_available && secret_backend.os_secret_store_available;
    FeatureCapability {
        feature_id: surface.surface_id.clone(),
        maturity: feature_maturity(surface),
        availability: if available {
            FeatureAvailability::Available
        } else {
            FeatureAvailability::Unavailable
        },
        preferred: surface.preferred_for_product_auth,
        requires: vec!["os_secret_store".to_string()],
        status_reason: if available {
            None
        } else {
            Some("os_secret_store_unavailable".to_string())
        },
        status_copy_key: Some(surface_status_copy_key(
            &surface.surface_id,
            if available {
                "available"
            } else {
                "unavailable"
            },
        )),
    }
}

fn subprocess_cli_feature(
    surface: &ProviderSurfaceSpec,
    detected_surfaces: &[ProbedSubprocessCli],
) -> FeatureCapability {
    let detected = probe_for_surface_id(detected_surfaces, &surface.surface_id);
    let runtime_supported = subprocess_runtime_supported(&surface.surface_id).unwrap_or(false);
    let availability = match detected {
        Some(surface)
            if runtime_supported
                && surface.auth_status == SubprocessCliAuthStatus::Authenticated =>
        {
            FeatureAvailability::Available
        }
        Some(_) => FeatureAvailability::PartiallyAvailable,
        None => FeatureAvailability::Unavailable,
    };

    let (status_reason, copy_suffix) = match detected {
        Some(surface)
            if runtime_supported
                && surface.auth_status == SubprocessCliAuthStatus::Authenticated =>
        {
            ("cli_ready", "available")
        }
        Some(_) if !runtime_supported => ("cli_detected_runtime_pending", "partially_available"),
        Some(surface) => match surface.auth_status {
            SubprocessCliAuthStatus::Authenticated => {
                ("cli_detected_runtime_pending", "partially_available")
            }
            SubprocessCliAuthStatus::Unauthenticated => {
                ("cli_detected_auth_required", "auth_required")
            }
            SubprocessCliAuthStatus::Unknown => {
                ("cli_detected_auth_unverified", "partially_available")
            }
        },
        None => ("cli_not_installed", "unavailable"),
    };

    FeatureCapability {
        feature_id: surface.surface_id.clone(),
        maturity: feature_maturity(surface),
        availability,
        preferred: surface.preferred_for_product_auth,
        requires: surface
            .subprocess_transport
            .as_ref()
            .map(|transport| vec![format!("cli:{}", transport.tool_id)])
            .unwrap_or_default(),
        status_reason: Some(status_reason.to_string()),
        status_copy_key: Some(surface_status_copy_key(&surface.surface_id, copy_suffix)),
    }
}

async fn probed_http_surface_feature(surface: &ProviderSurfaceSpec) -> FeatureCapability {
    let availability = match probe_surface_http_reachability(surface).await {
        Ok(true) => FeatureAvailability::Available,
        Ok(false) => FeatureAvailability::Unavailable,
        Err(error) => {
            tracing::warn!(
                surface_id = %surface.surface_id,
                error = %error,
                "Provider surface availability probe failed."
            );
            FeatureAvailability::PartiallyAvailable
        }
    };

    let placement = parse_surface_placement_kind(&surface.placement_kind)
        .unwrap_or(SurfacePlacementKind::CustomHosted);
    let requires = match placement {
        SurfacePlacementKind::SelfHosted => vec![format!("local_server:{}", surface.vendor_id)],
        SurfacePlacementKind::CustomHosted => vec![format!("endpoint:{}", surface.vendor_id)],
        _ => Vec::new(),
    };
    let (status_reason, copy_suffix) = match availability {
        FeatureAvailability::Available => ("service_reachable", "available"),
        FeatureAvailability::Unavailable => ("service_unreachable", "unavailable"),
        FeatureAvailability::PartiallyAvailable => ("service_probe_failed", "partially_available"),
    };

    FeatureCapability {
        feature_id: surface.surface_id.clone(),
        maturity: feature_maturity(surface),
        availability,
        preferred: surface.preferred_for_product_auth,
        requires,
        status_reason: Some(status_reason.to_string()),
        status_copy_key: Some(surface_status_copy_key(&surface.surface_id, copy_suffix)),
    }
}

async fn probe_surface_http_reachability(surface: &ProviderSurfaceSpec) -> Result<bool, String> {
    let probe = surface.availability_probe.as_ref().ok_or_else(|| {
        format!(
            "Surface '{}' is missing availability_probe.",
            surface.surface_id
        )
    })?;
    let auth_scheme = match probe.auth_scheme.trim().to_ascii_lowercase().as_str() {
        "none" => ProviderAuthScheme::None,
        other => {
            return Err(format!(
                "Self-hosted availability probe for '{}' uses unsupported auth_scheme '{}'.",
                surface.surface_id, other
            ))
        }
    };
    if auth_scheme != ProviderAuthScheme::None {
        return Err(format!(
            "Self-hosted availability probe for '{}' currently requires auth_scheme=none.",
            surface.surface_id
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .map_err(|error| format!("Failed to build availability probe client: {error}"))?;
    let method = probe.method.trim().to_ascii_uppercase();
    let response = match method.as_str() {
        "GET" => client.get(&probe.url).send().await,
        "HEAD" => client.head(&probe.url).send().await,
        other => {
            return Err(format!(
                "Self-hosted availability probe for '{}' uses unsupported method '{}'.",
                surface.surface_id, other
            ))
        }
    }
    .map_err(|error| format!("Availability probe request failed: {error}"))?;

    Ok(response.status().is_success())
}

fn feature_maturity(surface: &ProviderSurfaceSpec) -> FeatureMaturity {
    match parse_surface_stability(&surface.stability).unwrap_or(SurfaceStability::Experimental) {
        SurfaceStability::Ga => FeatureMaturity::Stable,
        SurfaceStability::Preview => FeatureMaturity::Beta,
        SurfaceStability::Experimental => FeatureMaturity::Experimental,
        SurfaceStability::Deprecated => FeatureMaturity::Deprecated,
    }
}

fn surface_status_copy_key(surface_id: &str, suffix: &str) -> String {
    format!("featureCapability.surface.{surface_id}.{suffix}")
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
        let feature = managed_oauth_feature(
            &provider_surface_catalog()
                .expect("catalog should load")
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == "provider_surface.openai.managed_oauth")
                .expect("openai managed oauth surface should exist")
                .clone(),
            &backend_caps(false),
        );
        assert_eq!(feature.maturity, FeatureMaturity::Experimental);
        assert_eq!(feature.availability, FeatureAvailability::Unavailable);
        assert!(!feature.preferred);
    }

    #[test]
    fn subprocess_cli_feature_is_preferred_but_partial_when_cli_detected() {
        let surface = provider_surface_catalog()
            .expect("catalog should load")
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == "provider_surface.openai.subprocess_cli")
            .expect("openai subprocess surface should exist")
            .clone();
        let feature = subprocess_cli_feature(
            &surface,
            &[ProbedSubprocessCli {
                detected: crate::subprocess_provider::DetectedSubprocessCli {
                    surface_id: "provider_surface.openai.subprocess_cli".to_string(),
                    executable_path: "/usr/bin/codex".into(),
                },
                auth_status: SubprocessCliAuthStatus::Unknown,
                auth_detail: Some("probe_failed:test".to_string()),
            }],
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
        assert_eq!(
            feature.status_copy_key.as_deref(),
            Some("featureCapability.surface.provider_surface.openai.subprocess_cli.partially_available")
        );
    }

    #[test]
    fn subprocess_cli_feature_is_available_when_cli_is_authenticated() {
        let surface = provider_surface_catalog()
            .expect("catalog should load")
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == "provider_surface.openai.subprocess_cli")
            .expect("openai subprocess surface should exist")
            .clone();
        let feature = subprocess_cli_feature(
            &surface,
            &[ProbedSubprocessCli {
                detected: crate::subprocess_provider::DetectedSubprocessCli {
                    surface_id: "provider_surface.openai.subprocess_cli".to_string(),
                    executable_path: "/usr/bin/codex".into(),
                },
                auth_status: SubprocessCliAuthStatus::Authenticated,
                auth_detail: Some("cli_authenticated".to_string()),
            }],
        );
        assert_eq!(feature.maturity, FeatureMaturity::Beta);
        assert_eq!(feature.availability, FeatureAvailability::Available);
        assert_eq!(feature.status_reason.as_deref(), Some("cli_ready"));
        assert_eq!(
            feature.status_copy_key.as_deref(),
            Some("featureCapability.surface.provider_surface.openai.subprocess_cli.available")
        );
    }

    #[test]
    fn subprocess_cli_feature_reports_auth_required_when_logged_out() {
        let surface = provider_surface_catalog()
            .expect("catalog should load")
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == "provider_surface.anthropic.subprocess_cli")
            .expect("anthropic subprocess surface should exist")
            .clone();
        let feature = subprocess_cli_feature(
            &surface,
            &[ProbedSubprocessCli {
                detected: crate::subprocess_provider::DetectedSubprocessCli {
                    surface_id: "provider_surface.anthropic.subprocess_cli".to_string(),
                    executable_path: "/usr/bin/claude".into(),
                },
                auth_status: SubprocessCliAuthStatus::Unauthenticated,
                auth_detail: Some("cli_auth_required".to_string()),
            }],
        );
        assert_eq!(
            feature.availability,
            FeatureAvailability::PartiallyAvailable
        );
        assert_eq!(
            feature.status_reason.as_deref(),
            Some("cli_detected_auth_required")
        );
        assert_eq!(
            feature.status_copy_key.as_deref(),
            Some(
                "featureCapability.surface.provider_surface.anthropic.subprocess_cli.auth_required"
            )
        );
    }

    #[tokio::test]
    async fn snapshot_contains_expected_feature_ids() {
        let snapshot = build_feature_capability_snapshot(&backend_caps(true)).await;
        let ids: Vec<&str> = snapshot
            .features
            .iter()
            .map(|feature| feature.feature_id.as_str())
            .collect();
        assert!(ids.contains(&"provider_surface.openai.managed_oauth"));
        assert!(ids.contains(&"provider_surface.openai.subprocess_cli"));
        assert!(ids.contains(&"provider_surface.anthropic.subprocess_cli"));
        assert!(ids.contains(&"provider_surface.google.subprocess_cli"));
        assert!(ids.contains(&"provider_surface.ollama.local_http"));
    }

    #[tokio::test]
    async fn self_hosted_surface_feature_declares_local_service_requirement() {
        let surface = provider_surface_catalog()
            .expect("catalog should load")
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == "provider_surface.ollama.local_http")
            .expect("ollama surface should exist")
            .clone();
        let feature = probed_http_surface_feature(&surface).await;
        assert!(feature
            .requires
            .iter()
            .any(|value| value == "local_server:ollama"));
    }
}
