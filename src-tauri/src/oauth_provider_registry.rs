#[cfg(any(feature = "server", test))]
use oneshim_api_contracts::ai_providers::ProviderTransportSpec;
#[cfg(any(feature = "server", test))]
use oneshim_api_contracts::provider_specs::{
    self, parse_surface_execution_kind, provider_surface_spec, ProviderTransportKind,
    SurfaceExecutionKind,
};
#[cfg(any(feature = "server", test))]
use oneshim_core::config::{AiProviderConfig, AiProviderType, ExternalApiEndpoint};
#[cfg(any(feature = "server", test))]
use oneshim_core::error::CoreError;
#[cfg(any(feature = "server", test))]
use oneshim_network::oauth::provider_config::OAuthProviderConfig;

#[cfg(any(feature = "server", test))]
const GOOGLE_OAUTH_CLIENT_ID_ENV: &str = "ONESHIM_GOOGLE_OAUTH_CLIENT_ID";

#[cfg(any(feature = "server", test))]
pub fn configured_oauth_provider_configs() -> Vec<OAuthProviderConfig> {
    let mut providers = vec![OAuthProviderConfig::openai_codex()];

    if let Some(client_id) = google_oauth_client_id() {
        providers.push(OAuthProviderConfig::google_cloud_vision(client_id));
    }

    providers
}

#[cfg(any(feature = "server", test))]
pub fn configured_oauth_provider_ids() -> Vec<String> {
    configured_oauth_provider_configs()
        .into_iter()
        .map(|provider| provider.provider_id)
        .collect()
}

#[cfg(any(feature = "server", test))]
pub fn selected_managed_oauth_provider_ids(
    config: &AiProviderConfig,
) -> Result<Vec<String>, CoreError> {
    let mut provider_ids = Vec::new();

    if let Some(endpoint) = config.llm_api.as_ref() {
        maybe_push_managed_provider(&mut provider_ids, endpoint, ProviderTransportKind::Llm)?;
    } else if config.llm_provider == oneshim_core::config::LlmProviderType::Remote {
        provider_ids.push("openai".to_string());
    }

    if let Some(endpoint) = config.ocr_api.as_ref() {
        maybe_push_managed_provider(&mut provider_ids, endpoint, ProviderTransportKind::Ocr)?;
    }

    Ok(provider_ids)
}

#[cfg(any(feature = "server", test))]
pub fn managed_oauth_provider_id_for_endpoint(
    endpoint: &ExternalApiEndpoint,
    kind: ProviderTransportKind,
) -> Result<String, CoreError> {
    managed_oauth_transport_spec(endpoint, kind)?;
    Ok(provider_type_id(endpoint.provider_type).to_string())
}

#[cfg(any(feature = "server", test))]
pub fn managed_oauth_transport_url_for_endpoint(
    endpoint: &ExternalApiEndpoint,
    kind: ProviderTransportKind,
) -> Result<String, CoreError> {
    Ok(managed_oauth_transport_spec(endpoint, kind)?.url.clone())
}

#[cfg(any(feature = "server", test))]
fn managed_oauth_transport_spec<'a>(
    endpoint: &'a ExternalApiEndpoint,
    kind: ProviderTransportKind,
) -> Result<&'a ProviderTransportSpec, CoreError> {
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

    let spec = provider_specs::resolved_transport_spec(
        endpoint.provider_type,
        endpoint.surface_id.as_deref(),
        kind,
    )
    .map_err(CoreError::Internal)?;

    Ok(spec)
}

#[cfg(any(feature = "server", test))]
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

#[cfg(any(feature = "server", test))]
fn google_oauth_client_id() -> Option<String> {
    std::env::var(GOOGLE_OAUTH_CLIENT_ID_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(any(feature = "server", test))]
fn provider_type_id(provider_type: AiProviderType) -> &'static str {
    match provider_type {
        AiProviderType::OpenAi => "openai",
        AiProviderType::Anthropic => "anthropic",
        AiProviderType::Google => "google",
        AiProviderType::Ollama => "ollama",
        AiProviderType::Generic => "generic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::{
        CredentialAuthMode, CredentialBackendKind, CredentialBinding, ExternalApiEndpoint,
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
}
