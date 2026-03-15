use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};

use crate::setup::SecretBackendCapabilities;

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
    let codex_cli = detect_cli(&["codex"]);
    let claude_code_cli = detect_cli(&["claude", "claude-code"]);
    let gemini_cli = detect_cli(&["gemini", "gemini-cli"]);

    FeatureCapabilitySnapshot {
        features: vec![
            managed_oauth_feature(secret_backend),
            subprocess_cli_feature(
                "openai",
                "codex",
                codex_cli,
                "featureCapability.providerSurface.openaiSubprocessCli",
            ),
            subprocess_cli_feature(
                "anthropic",
                "claude-code",
                claude_code_cli,
                "featureCapability.providerSurface.anthropicSubprocessCli",
            ),
            subprocess_cli_feature(
                "google",
                "gemini-cli",
                gemini_cli,
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
    vendor_id: &str,
    cli_id: &str,
    detection: Option<PathBuf>,
    copy_key_prefix: &str,
) -> FeatureCapability {
    let detected = detection.is_some();
    FeatureCapability {
        feature_id: format!("provider_surface.{vendor_id}.subprocess_cli"),
        maturity: FeatureMaturity::Experimental,
        availability: if detected {
            FeatureAvailability::PartiallyAvailable
        } else {
            FeatureAvailability::Unavailable
        },
        preferred: true,
        requires: vec![format!("cli:{cli_id}")],
        status_reason: Some(if detected {
            "cli_detected_runtime_pending".to_string()
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

fn detect_cli(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .find_map(|candidate| find_executable(candidate))
}

fn find_executable(name: &str) -> Option<PathBuf> {
    if name.contains(std::path::MAIN_SEPARATOR) {
        let path = PathBuf::from(name);
        return is_executable(&path).then_some(path);
    }

    let path_var = env::var_os("PATH")?;
    #[cfg(windows)]
    let exts: Vec<String> = env::var_os("PATHEXT")
        .map(|value| {
            env::split_paths(&PathBuf::from(value))
                .map(|path| path.to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                ".COM".to_string(),
                ".EXE".to_string(),
                ".BAT".to_string(),
                ".CMD".to_string(),
            ]
        });

    for dir in env::split_paths(&path_var) {
        let base = dir.join(name);
        if is_executable(&base) {
            return Some(base);
        }
        #[cfg(windows)]
        {
            for ext in &exts {
                let candidate = dir.join(format!("{name}{ext}"));
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
        false
    }

    #[cfg(not(unix))]
    {
        true
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
            "openai",
            "codex",
            Some(PathBuf::from("/usr/bin/codex")),
            "featureCapability.providerSurface.openaiSubprocessCli",
        );
        assert_eq!(
            feature.availability,
            FeatureAvailability::PartiallyAvailable
        );
        assert!(feature.preferred);
        assert_eq!(feature.requires, vec!["cli:codex".to_string()]);
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
