#[cfg(feature = "server")]
use oneshim_api_contracts::ai_providers::ProviderTransportSpec;
#[cfg(feature = "server")]
use oneshim_api_contracts::provider_specs::{
    self, parse_surface_execution_kind, provider_surface_spec, ProviderTransportKind,
    SurfaceExecutionKind,
};
#[cfg(feature = "server")]
use oneshim_core::config::{AiProviderConfig, ExternalApiEndpoint};
#[cfg(feature = "server")]
use oneshim_core::error::CoreError;
#[cfg(feature = "server")]
use oneshim_network::oauth::provider_config::OAuthProviderConfig;

#[cfg(feature = "server")]
const OPENAI_MANAGED_OAUTH_SURFACE_ID: &str = "provider_surface.openai.managed_oauth";
#[cfg(feature = "server")]
const GOOGLE_MANAGED_OAUTH_SURFACE_ID: &str = "provider_surface.google.managed_oauth";

#[cfg(feature = "server")]
#[derive(Clone, Copy)]
struct ManagedOAuthProviderFactory {
    surface_id: &'static str,
    build: fn() -> Option<OAuthProviderConfig>,
}

#[cfg(feature = "server")]
fn managed_oauth_provider_factories() -> [ManagedOAuthProviderFactory; 2] {
    [
        ManagedOAuthProviderFactory {
            surface_id: OPENAI_MANAGED_OAUTH_SURFACE_ID,
            build: build_openai_managed_oauth_provider,
        },
        ManagedOAuthProviderFactory {
            surface_id: GOOGLE_MANAGED_OAUTH_SURFACE_ID,
            build: build_google_managed_oauth_provider,
        },
    ]
}

#[cfg(feature = "server")]
pub fn configured_oauth_provider_configs() -> Vec<OAuthProviderConfig> {
    managed_oauth_provider_factories()
        .into_iter()
        .filter_map(|factory| {
            provider_surface_spec(factory.surface_id)
                .ok()
                .and_then(|_| (factory.build)())
        })
        .collect()
}

#[cfg(feature = "server")]
pub fn configured_oauth_provider_ids() -> Vec<String> {
    configured_oauth_provider_configs()
        .into_iter()
        .map(|provider| provider.provider_id)
        .collect()
}

#[cfg(feature = "server")]
pub fn selected_managed_oauth_provider_ids(
    config: &AiProviderConfig,
) -> Result<Vec<String>, CoreError> {
    let mut provider_ids = Vec::new();

    if let Some(endpoint) = config.llm_api.as_ref() {
        maybe_push_managed_provider(&mut provider_ids, endpoint, ProviderTransportKind::Llm)?;
    } else if config.llm_provider == oneshim_core::config::LlmProviderType::Remote {
        provider_ids.push(
            provider_surface_spec(OPENAI_MANAGED_OAUTH_SURFACE_ID)
                .map(|surface| surface.vendor_id.clone())
                .unwrap_or_else(|_| "openai".to_string()),
        );
    }

    if let Some(endpoint) = config.ocr_api.as_ref() {
        maybe_push_managed_provider(&mut provider_ids, endpoint, ProviderTransportKind::Ocr)?;
    }

    Ok(provider_ids)
}

#[cfg(feature = "server")]
pub fn managed_oauth_provider_id_for_endpoint(
    endpoint: &ExternalApiEndpoint,
    _kind: ProviderTransportKind,
) -> Result<String, CoreError> {
    Ok(managed_oauth_surface(endpoint)?.vendor_id.clone())
}

#[cfg(feature = "server")]
pub fn managed_oauth_transport_url_for_endpoint(
    endpoint: &ExternalApiEndpoint,
    kind: ProviderTransportKind,
) -> Result<String, CoreError> {
    Ok(managed_oauth_transport_spec(endpoint, kind)?.url.clone())
}

#[cfg(feature = "server")]
fn managed_oauth_transport_spec<'a>(
    endpoint: &'a ExternalApiEndpoint,
    kind: ProviderTransportKind,
) -> Result<&'a ProviderTransportSpec, CoreError> {
    managed_oauth_surface(endpoint)?;
    let spec = provider_specs::resolved_transport_spec(
        endpoint.provider_type,
        endpoint.surface_id.as_deref(),
        kind,
    )
    .map_err(CoreError::Internal)?;

    Ok(spec)
}

#[cfg(feature = "server")]
fn managed_oauth_surface<'a>(
    endpoint: &'a ExternalApiEndpoint,
) -> Result<&'a oneshim_api_contracts::provider_specs::ProviderSurfaceSpec, CoreError> {
    let surface = provider_surface_spec(endpoint.surface_id.as_deref().ok_or_else(|| {
        CoreError::Config(
            "Managed OAuth endpoint is missing provider surface metadata.".to_string(),
        )
    })?)
    .map_err(CoreError::Internal)?;
    if parse_surface_execution_kind(&surface.execution_kind).map_err(CoreError::Internal)?
        != SurfaceExecutionKind::ManagedHttp
    {
        return Err(CoreError::Config(
            "Selected provider surface does not use managed OAuth transport.".to_string(),
        ));
    }
    Ok(surface)
}

#[cfg(feature = "server")]
fn maybe_push_managed_provider(
    provider_ids: &mut Vec<String>,
    endpoint: &ExternalApiEndpoint,
    kind: ProviderTransportKind,
) -> Result<(), CoreError> {
    match managed_oauth_transport_spec(endpoint, kind) {
        Ok(_) => {
            let provider_id = managed_oauth_provider_id_for_endpoint(endpoint, kind)?;
            if !provider_ids
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&provider_id))
            {
                provider_ids.push(provider_id);
            }
            Ok(())
        }
        Err(CoreError::Config(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(feature = "server")]
fn google_oauth_client_id() -> Option<String> {
    provider_surface_spec(GOOGLE_MANAGED_OAUTH_SURFACE_ID)
        .ok()
        .and_then(|surface| surface.provisioning.as_ref())
        .and_then(|provisioning| provisioning.configuration_env_vars.first())
        .and_then(|env_var| std::env::var(env_var).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(feature = "server")]
fn build_openai_managed_oauth_provider() -> Option<OAuthProviderConfig> {
    Some(OAuthProviderConfig::openai_codex())
}

#[cfg(feature = "server")]
fn build_google_managed_oauth_provider() -> Option<OAuthProviderConfig> {
    google_oauth_client_id().map(OAuthProviderConfig::google_cloud_vision)
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use oneshim_core::config::{
        AiProviderType, CredentialAuthMode, CredentialBackendKind, CredentialBinding,
        ExternalApiEndpoint,
    };

    fn managed_google_ocr_endpoint() -> ExternalApiEndpoint {
        ExternalApiEndpoint {
            endpoint: "https://vision.googleapis.com/v1/images:annotate".to_string(),
            api_key: String::new(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Google,
            surface_id: Some("provider_surface.google.managed_oauth".to_string()),
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ManagedOAuth,
                backend_kind: CredentialBackendKind::OsSecretStore,
                secret_ref: None,
                projection_enabled: false,
            }),
        }
    }

    #[test]
    fn managed_oauth_url_uses_surface_transport() {
        let endpoint = managed_google_ocr_endpoint();
        let url = managed_oauth_transport_url_for_endpoint(&endpoint, ProviderTransportKind::Ocr)
            .expect("managed OAuth transport URL should resolve");
        assert_eq!(url, "https://vision.googleapis.com/v1/images:annotate");
    }

    #[test]
    fn selected_managed_oauth_provider_ids_collects_google() {
        let config = AiProviderConfig {
            access_mode: oneshim_core::config::AiAccessMode::ProviderOAuth,
            ocr_provider: oneshim_core::config::OcrProviderType::Remote,
            llm_provider: oneshim_core::config::LlmProviderType::Local,
            ocr_api: Some(managed_google_ocr_endpoint()),
            ..AiProviderConfig::default()
        };

        let providers =
            selected_managed_oauth_provider_ids(&config).expect("provider IDs should resolve");
        assert_eq!(providers, vec!["google".to_string()]);
    }

    #[test]
    fn google_oauth_client_id_uses_surface_provisioning_env_var() {
        std::env::set_var("ONESHIM_GOOGLE_OAUTH_CLIENT_ID", "test-google-client-id");
        let client_id = google_oauth_client_id();
        std::env::remove_var("ONESHIM_GOOGLE_OAUTH_CLIENT_ID");
        assert_eq!(client_id.as_deref(), Some("test-google-client-id"));
    }
}
