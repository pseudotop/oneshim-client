use std::sync::Arc;

#[cfg(feature = "server")]
use async_trait::async_trait;
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::local_llm::LocalLlmProvider;
use oneshim_core::config::PrivacyConfig;
use oneshim_core::config::{
    AiAccessMode, AiProviderConfig, AiProviderType, LlmProviderType, OcrProviderType,
    PiiFilterLevel,
};
#[cfg(feature = "server")]
use oneshim_core::config::{ExternalApiEndpoint, ExternalDataPolicy, OcrValidationConfig};
#[cfg(not(feature = "server"))]
use oneshim_core::config::{ExternalDataPolicy, OcrValidationConfig};
use oneshim_core::consent::ConsentManager;
use oneshim_core::error::CoreError;
#[cfg(feature = "server")]
use oneshim_core::ports::credential_source::CredentialSource;
use oneshim_core::ports::llm_provider::LlmProvider;
use oneshim_core::ports::monitor::ProcessMonitor;
#[cfg(feature = "server")]
use oneshim_core::ports::oauth::OAuthPort;
use oneshim_core::ports::ocr_provider::OcrProvider;
use oneshim_core::ports::ocr_provider::OcrResult;
use oneshim_core::ports::secret_store::SecretStoreSet;
use oneshim_core::provider_surface::{
    provider_surface_spec, provider_vendor_id_or_default, ProviderSurfaceTransport,
};
#[cfg(feature = "server")]
use oneshim_network::ai_llm_client::RemoteLlmProvider;
#[cfg(feature = "server")]
use oneshim_network::ai_ocr_client::RemoteOcrProvider;
#[cfg(feature = "server")]
use oneshim_network::oauth::provider_config::OAuthProviderConfig;
use oneshim_vision::local_ocr_provider::LocalOcrProvider;
use oneshim_vision::privacy_gateway::{PrivacyGateway, SanitizedImage};
use std::path::PathBuf;
use tokio::sync::RwLock;
#[cfg(feature = "server")]
use tracing::debug;
use tracing::warn;

use oneshim_api_contracts::provider_specs::SurfaceCapabilityKind;

#[cfg(feature = "server")]
use crate::oauth_provider_registry::{
    managed_oauth_provider_id_for_endpoint, managed_oauth_transport_url_for_endpoint,
};
use crate::subprocess_provider::{
    cli_id_for_surface_id, preferred_cli_surface_for_config, probe_for_surface_id,
    probe_known_cli_surfaces, runtime_supported_for_surface, select_cli_surface_for_capability,
    select_cli_surface_for_config, ProbedSubprocessCli, SubprocessCliAuthStatus,
    SubprocessLlmProvider, SubprocessOcrProvider,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProviderSource {
    Local,
    Remote,
    LocalFallback,
    CliSubscription,
    OAuth,
}

impl ProviderSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::LocalFallback => "local-fallback",
            Self::CliSubscription => "cli-subscription",
            Self::OAuth => "oauth",
        }
    }
}

pub struct AiProviderAdapters {
    pub ocr: Arc<dyn OcrProvider>,
    pub llm: Arc<dyn LlmProvider>,
    pub ocr_source: ProviderSource,
    pub llm_source: ProviderSource,
    pub ocr_fallback_reason: Option<String>,
    pub llm_fallback_reason: Option<String>,
}

type OcrProviderResolution =
    Result<(Arc<dyn OcrProvider>, ProviderSource, Option<String>), CoreError>;
type LlmProviderResolution =
    Result<(Arc<dyn LlmProvider>, ProviderSource, Option<String>), CoreError>;

#[cfg(feature = "server")]
const DEFAULT_OPENAI_OAUTH_MODEL: &str = "gpt-5.4";

#[cfg_attr(not(feature = "server"), allow(dead_code))]
#[derive(Clone)]
pub struct ExternalOcrPrivacyGuard {
    consent_path: PathBuf,
    pii_filter_level: PiiFilterLevel,
    external_data_policy: ExternalDataPolicy,
    privacy_config: PrivacyConfig,
    process_monitor: Arc<dyn ProcessMonitor>,
    audit_logger: Option<Arc<RwLock<AuditLogger>>>,
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
impl ExternalOcrPrivacyGuard {
    pub fn new(
        consent_path: PathBuf,
        pii_filter_level: PiiFilterLevel,
        external_data_policy: ExternalDataPolicy,
        privacy_config: PrivacyConfig,
        process_monitor: Arc<dyn ProcessMonitor>,
        audit_logger: Option<Arc<RwLock<AuditLogger>>>,
    ) -> Self {
        Self {
            consent_path,
            pii_filter_level,
            external_data_policy,
            privacy_config,
            process_monitor,
            audit_logger,
        }
    }

    async fn prepare_image_for_external(
        &self,
        image_data: &[u8],
        provider_name: &str,
        allow_unredacted_external_ocr: bool,
    ) -> Result<SanitizedImage, CoreError> {
        let active_window = match self.process_monitor.get_active_window().await? {
            Some(window) => window,
            None => {
                let message = "External OCR blocked: active window context unavailable".to_string();
                self.log_event(
                    "privacy.external_ocr.denied",
                    &format!("provider={provider_name} reason=no_active_window"),
                )
                .await;
                return Err(CoreError::PolicyDenied(message));
            }
        };

        let gateway = PrivacyGateway::new(
            Arc::new(ConsentManager::new(self.consent_path.clone())),
            self.pii_filter_level,
            self.external_data_policy,
            self.privacy_config.clone(),
        );

        match gateway
            .prepare_image_for_external_with_override(
                image_data,
                &active_window.app_name,
                &active_window.title,
                allow_unredacted_external_ocr,
            )
            .await
        {
            Ok(sanitized) => {
                self.log_event(
                    "privacy.external_ocr.allowed",
                    &format!(
                        "provider={provider_name} app={} title={} redacted_regions={} metadata_stripped={}",
                        active_window.app_name,
                        active_window.title,
                        sanitized.redacted_regions,
                        sanitized.metadata_stripped
                    ),
                )
                .await;
                Ok(sanitized)
            }
            Err(err) => {
                self.log_event(
                    "privacy.external_ocr.denied",
                    &format!(
                        "provider={provider_name} app={} title={} reason={}",
                        active_window.app_name, active_window.title, err
                    ),
                )
                .await;
                Err(CoreError::PolicyDenied(format!(
                    "External OCR blocked: {err}"
                )))
            }
        }
    }

    async fn log_event(&self, action_type: &str, details: &str) {
        let Some(audit_logger) = self.audit_logger.as_ref() else {
            return;
        };

        let mut logger = audit_logger.write().await;
        logger.log_event(action_type, "runtime-ocr", details);
    }
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
struct GuardedOcrProvider {
    inner: Arc<dyn OcrProvider>,
    privacy_guard: ExternalOcrPrivacyGuard,
    allow_unredacted_external_ocr: bool,
    ocr_validation: OcrValidationConfig,
}

#[cfg_attr(not(feature = "server"), allow(dead_code))]
impl GuardedOcrProvider {
    fn new(
        inner: Arc<dyn OcrProvider>,
        privacy_guard: ExternalOcrPrivacyGuard,
        allow_unredacted_external_ocr: bool,
        ocr_validation: OcrValidationConfig,
    ) -> Self {
        Self {
            inner,
            privacy_guard,
            allow_unredacted_external_ocr,
            ocr_validation,
        }
    }

    fn validate_ocr_results(&self, results: Vec<OcrResult>) -> Result<Vec<OcrResult>, CoreError> {
        if !self.ocr_validation.enabled || results.is_empty() {
            return Ok(results);
        }

        let total = results.len();
        let mut invalid = 0usize;
        let mut filtered = Vec::with_capacity(total);

        for mut result in results {
            let text = result.text.trim();
            let is_valid_geometry =
                result.x >= 0 && result.y >= 0 && result.width > 0 && result.height > 0;
            let is_valid_confidence =
                result.confidence.is_finite() && (0.0..=1.0).contains(&result.confidence);

            if text.is_empty()
                || !is_valid_geometry
                || !is_valid_confidence
                || result.confidence < self.ocr_validation.min_confidence
            {
                invalid += 1;
                continue;
            }

            result.text = text.to_string();
            filtered.push(result);
        }

        let invalid_ratio = invalid as f64 / total as f64;
        if invalid_ratio > self.ocr_validation.max_invalid_ratio {
            return Err(CoreError::OcrError(format!(
                "OCR calibration validation failure: invalid_ratio={invalid_ratio:.2}, max_invalid_ratio={:.2}",
                self.ocr_validation.max_invalid_ratio
            )));
        }

        Ok(filtered)
    }
}

#[cfg(feature = "server")]
#[async_trait]
impl OcrProvider for GuardedOcrProvider {
    async fn extract_elements(
        &self,
        image: &[u8],
        image_format: &str,
    ) -> Result<Vec<OcrResult>, CoreError> {
        if !self.inner.is_external() {
            return self.inner.extract_elements(image, image_format).await;
        }

        let sanitized = self
            .privacy_guard
            .prepare_image_for_external(
                image,
                self.inner.provider_name(),
                self.allow_unredacted_external_ocr,
            )
            .await?;

        debug!(
            redacted_regions = sanitized.redacted_regions,
            allow_unredacted_external_ocr = self.allow_unredacted_external_ocr,
            "External OCR image sanitization completed"
        );

        let results = self
            .inner
            .extract_elements(&sanitized.image_data, image_format)
            .await?;
        self.validate_ocr_results(results)
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }

    fn is_external(&self) -> bool {
        self.inner.is_external()
    }
}

pub fn resolve_ai_provider_adapters(
    config: &AiProviderConfig,
    pii_filter_level: PiiFilterLevel,
    external_ocr_privacy_guard: Option<ExternalOcrPrivacyGuard>,
    secret_stores: Option<SecretStoreSet>,
    #[cfg(feature = "server")] oauth_port: Option<Arc<dyn OAuthPort>>,
) -> Result<AiProviderAdapters, CoreError> {
    match config.access_mode.normalized_for_ai_surfaces() {
        AiAccessMode::LocalModel => Ok(AiProviderAdapters {
            ocr: Arc::new(LocalOcrProvider::new()),
            llm: Arc::new(LocalLlmProvider::new()),
            ocr_source: ProviderSource::Local,
            llm_source: ProviderSource::Local,
            ocr_fallback_reason: None,
            llm_fallback_reason: None,
        }),
        AiAccessMode::ProviderSubscriptionCli => {
            let probed = probe_known_cli_surfaces();
            let (ocr, ocr_source, ocr_fallback_reason) = resolve_cli_subscription_ocr_provider(
                config,
                pii_filter_level,
                external_ocr_privacy_guard.clone(),
                secret_stores.clone(),
                &probed,
            )?;
            let (llm, llm_source, llm_fallback_reason) =
                resolve_cli_subscription_llm_provider_with_detected(config, &probed)?;
            Ok(AiProviderAdapters {
                ocr,
                llm,
                ocr_source,
                llm_source,
                ocr_fallback_reason,
                llm_fallback_reason,
            })
        }
        AiAccessMode::ProviderApiKey => resolve_direct_surface_adapters(
            config,
            pii_filter_level,
            external_ocr_privacy_guard,
            secret_stores,
        ),
        AiAccessMode::ProviderOAuth => {
            #[cfg(feature = "server")]
            {
                let ocr_uses_managed = matches!(
                    configured_ocr_surface_transport(config),
                    Some((_, ProviderSurfaceTransport::ManagedOAuth))
                );
                let llm_uses_managed = llm_uses_managed_oauth(config);
                let oauth = if ocr_uses_managed || llm_uses_managed {
                    Some(oauth_port.ok_or_else(|| {
                        CoreError::Config(
                            "ProviderOAuth mode requires an initialized OAuth runtime for managed provider surfaces.".to_string(),
                        )
                    })?)
                } else {
                    None
                };
                let (ocr, ocr_source, ocr_fallback_reason) = if ocr_uses_managed {
                    resolve_ocr_provider_oauth(
                        config,
                        pii_filter_level,
                        external_ocr_privacy_guard.clone(),
                        oauth
                            .clone()
                            .expect("oauth runtime should exist for managed OCR"),
                    )?
                } else {
                    resolve_ocr_provider(
                        config,
                        pii_filter_level,
                        external_ocr_privacy_guard.clone(),
                        secret_stores.clone(),
                    )?
                };
                let (llm, llm_source, llm_fallback_reason) = if llm_uses_managed {
                    resolve_llm_provider_oauth(
                        config,
                        oauth
                            .clone()
                            .expect("oauth runtime should exist for managed LLM"),
                    )?
                } else {
                    resolve_llm_provider(config, secret_stores.clone())?
                };
                Ok(AiProviderAdapters {
                    ocr,
                    llm,
                    ocr_source,
                    llm_source,
                    ocr_fallback_reason,
                    llm_fallback_reason,
                })
            }
            #[cfg(not(feature = "server"))]
            {
                Err(CoreError::Config(
                    "ProviderOAuth mode requires the 'server' feature".to_string(),
                ))
            }
        }
        AiAccessMode::PlatformConnected => unreachable!("legacy access mode should normalize"),
    }
}

fn resolve_direct_surface_adapters(
    config: &AiProviderConfig,
    pii_filter_level: PiiFilterLevel,
    external_ocr_privacy_guard: Option<ExternalOcrPrivacyGuard>,
    secret_stores: Option<SecretStoreSet>,
) -> Result<AiProviderAdapters, CoreError> {
    let (ocr, ocr_source, ocr_fallback_reason) = resolve_ocr_provider(
        config,
        pii_filter_level,
        external_ocr_privacy_guard,
        secret_stores.clone(),
    )?;
    let (llm, llm_source, llm_fallback_reason) = resolve_llm_provider(config, secret_stores)?;

    Ok(AiProviderAdapters {
        ocr,
        llm,
        ocr_source,
        llm_source,
        ocr_fallback_reason,
        llm_fallback_reason,
    })
}

fn configured_ocr_surface_transport(
    config: &AiProviderConfig,
) -> Option<(String, ProviderSurfaceTransport)> {
    config
        .ocr_api
        .as_ref()
        .and_then(|endpoint| endpoint.surface_id.as_deref())
        .and_then(|surface_id| {
            provider_surface_spec(surface_id).map(|spec| (spec.id.to_string(), spec.transport))
        })
}

#[cfg(feature = "server")]
fn configured_llm_surface_transport(
    config: &AiProviderConfig,
) -> Option<(String, ProviderSurfaceTransport)> {
    config
        .llm_api
        .as_ref()
        .and_then(|endpoint| endpoint.surface_id.as_deref())
        .and_then(|surface_id| {
            provider_surface_spec(surface_id).map(|spec| (spec.id.to_string(), spec.transport))
        })
}

#[cfg(feature = "server")]
fn llm_uses_managed_oauth(config: &AiProviderConfig) -> bool {
    if config.llm_provider != LlmProviderType::Remote {
        return false;
    }

    match configured_llm_surface_transport(config) {
        Some((_, ProviderSurfaceTransport::ManagedOAuth)) => true,
        Some(_) => false,
        None => true,
    }
}

fn unsupported_ocr_surface_runtime(
    config: &AiProviderConfig,
    surface_id: &str,
    transport: ProviderSurfaceTransport,
) -> OcrProviderResolution {
    let runtime_label = match transport {
        ProviderSurfaceTransport::DirectApi => "direct_http",
        ProviderSurfaceTransport::ManagedOAuth => "managed_oauth",
        ProviderSurfaceTransport::SubprocessCli => "subprocess_cli",
    };
    let reason = format!(
        "Selected OCR provider surface '{surface_id}' uses {runtime_label}, but an OCR runtime adapter for that transport is not implemented yet."
    );

    if config.fallback_to_local {
        warn!(
            fallback_reason = %reason,
            "OCR runtime unavailable for selected provider surface, falling back to local OCR"
        );
        return Ok((
            Arc::new(LocalOcrProvider::new()) as Arc<dyn OcrProvider>,
            ProviderSource::LocalFallback,
            Some(reason),
        ));
    }

    Err(CoreError::Config(reason))
}

fn resolve_cli_subscription_ocr_provider(
    config: &AiProviderConfig,
    pii_filter_level: PiiFilterLevel,
    external_ocr_privacy_guard: Option<ExternalOcrPrivacyGuard>,
    secret_stores: Option<SecretStoreSet>,
    detected: &[ProbedSubprocessCli],
) -> OcrProviderResolution {
    if let Some((surface_id, transport)) = configured_ocr_surface_transport(config) {
        if transport == ProviderSurfaceTransport::SubprocessCli {
            if let Some(surface) =
                select_cli_surface_for_capability(config, detected, SurfaceCapabilityKind::Ocr)
            {
                return Ok((
                    Arc::new(SubprocessOcrProvider::new(surface, config)) as Arc<dyn OcrProvider>,
                    ProviderSource::CliSubscription,
                    None,
                ));
            }

            if let Some(surface) = probe_for_surface_id(detected, &surface_id) {
                let cli_label = cli_id_for_surface_id(&surface.detected.surface_id)
                    .unwrap_or_else(|_| surface.detected.surface_id.clone());
                let reason = match surface.auth_status {
                    SubprocessCliAuthStatus::Authenticated => format!(
                        "Selected OCR provider surface '{surface_id}' uses installed {cli_label}, but the OCR subprocess runtime could not be selected."
                    ),
                    SubprocessCliAuthStatus::Unauthenticated => format!(
                        "Selected OCR provider surface '{surface_id}' uses installed {cli_label}, but the CLI is not authenticated. Sign in through the provider-owned CLI first."
                    ),
                    SubprocessCliAuthStatus::Unknown => format!(
                        "Selected OCR provider surface '{surface_id}' uses installed {cli_label}, but authentication status could not be verified."
                    ),
                };
                if config.fallback_to_local {
                    warn!(
                        fallback_reason = %reason,
                        "CLI OCR runtime unavailable, falling back to local OCR"
                    );
                    return Ok((
                        Arc::new(LocalOcrProvider::new()) as Arc<dyn OcrProvider>,
                        ProviderSource::LocalFallback,
                        Some(reason),
                    ));
                }
                return Err(CoreError::Config(reason));
            }

            return unsupported_ocr_surface_runtime(config, &surface_id, transport);
        }
    }

    match config.ocr_provider {
        OcrProviderType::Local => Ok((
            Arc::new(LocalOcrProvider::new()) as Arc<dyn OcrProvider>,
            ProviderSource::Local,
            None,
        )),
        OcrProviderType::Remote => resolve_ocr_provider(
            config,
            pii_filter_level,
            external_ocr_privacy_guard,
            secret_stores,
        ),
    }
}

fn resolve_cli_subscription_llm_provider_with_detected(
    config: &AiProviderConfig,
    detected: &[ProbedSubprocessCli],
) -> LlmProviderResolution {
    if let Some(surface) = select_cli_surface_for_config(config, detected) {
        return Ok((
            Arc::new(SubprocessLlmProvider::new(surface, config)) as Arc<dyn LlmProvider>,
            ProviderSource::CliSubscription,
            None,
        ));
    }

    let reason = cli_subscription_unavailable_reason(config, detected);
    if config.fallback_to_local {
        warn!(
            fallback_reason = %reason,
            "CLI subscription runtime unavailable, falling back to local rule-based LLM"
        );
        return Ok((
            Arc::new(LocalLlmProvider::new()) as Arc<dyn LlmProvider>,
            ProviderSource::LocalFallback,
            Some(reason),
        ));
    }

    Err(CoreError::Config(reason))
}

fn cli_subscription_unavailable_reason(
    config: &AiProviderConfig,
    detected: &[ProbedSubprocessCli],
) -> String {
    if let Some(provider_type) = config
        .llm_api
        .as_ref()
        .map(|endpoint| endpoint.provider_type)
        .filter(|provider_type| *provider_type != AiProviderType::Generic)
    {
        if let Some(surface_id) = preferred_cli_surface_for_config(config) {
            if let Some(surface) = probe_for_surface_id(detected, &surface_id) {
                return match surface.auth_status {
                    SubprocessCliAuthStatus::Authenticated => format!(
                        "Installed {} CLI was detected but the runtime adapter could not be selected.",
                        cli_id_for_surface_id(&surface.detected.surface_id)
                            .unwrap_or_else(|_| surface.detected.surface_id.clone())
                    ),
                    SubprocessCliAuthStatus::Unauthenticated => format!(
                        "Installed {} CLI is not authenticated. Sign in through the provider-owned CLI first.",
                        cli_id_for_surface_id(&surface.detected.surface_id)
                            .unwrap_or_else(|_| surface.detected.surface_id.clone())
                    ),
                    SubprocessCliAuthStatus::Unknown => format!(
                        "Installed {} CLI was detected, but authentication status could not be verified.",
                        cli_id_for_surface_id(&surface.detected.surface_id)
                            .unwrap_or_else(|_| surface.detected.surface_id.clone())
                    ),
                };
            }
        }

        let provider_label = provider_vendor_id_or_default(provider_type);

        return format!(
            "No supported installed CLI runtime was detected for provider '{provider_label}'."
        );
    }

    if detected
        .iter()
        .any(|surface| !runtime_supported_for_surface(&surface.detected.surface_id))
    {
        return "Detected provider CLI executables do not yet have a supported runtime adapter."
            .to_string();
    }

    "No supported provider CLI runtime was detected on PATH (checked: codex, claude, claude-code, gemini)."
        .to_string()
}

#[allow(unused_variables)]
fn resolve_ocr_provider(
    config: &AiProviderConfig,
    pii_filter_level: PiiFilterLevel,
    external_ocr_privacy_guard: Option<ExternalOcrPrivacyGuard>,
    secret_stores: Option<SecretStoreSet>,
) -> OcrProviderResolution {
    match config.ocr_provider {
        OcrProviderType::Local => Ok((
            Arc::new(LocalOcrProvider::new()),
            ProviderSource::Local,
            None,
        )),
        OcrProviderType::Remote => {
            if let Some((surface_id, transport)) = configured_ocr_surface_transport(config) {
                if transport != ProviderSurfaceTransport::DirectApi {
                    return unsupported_ocr_surface_runtime(config, &surface_id, transport);
                }
            }
            #[cfg(feature = "server")]
            {
                resolve_remote_with_optional_fallback(
                    "ocr",
                    config.fallback_to_local,
                    || {
                        let endpoint = require_endpoint_config(config.ocr_api.as_ref(), "ocr_api")?;
                        let secret_store = secret_stores
                            .as_ref()
                            .and_then(|stores| stores.for_binding(endpoint.credential.as_ref()));
                        let credential = CredentialSource::from_api_key_endpoint_for_profile(
                            endpoint,
                            Some("ocr"),
                            secret_store,
                        )?;
                        let privacy_guard =
                            external_ocr_privacy_guard.clone().ok_or_else(|| {
                                CoreError::Config(
                                    "Remote OCR provider requires a runtime privacy guard"
                                        .to_string(),
                                )
                            })?;
                        let remote = Arc::new(RemoteOcrProvider::new_with_credential(
                            endpoint, credential,
                        )?) as Arc<dyn OcrProvider>;
                        Ok(Arc::new(GuardedOcrProvider::new(
                            remote,
                            privacy_guard,
                            config.allow_unredacted_external_ocr,
                            config.ocr_validation.clone(),
                        )) as Arc<dyn OcrProvider>)
                    },
                    || Arc::new(LocalOcrProvider::new()) as Arc<dyn OcrProvider>,
                )
            }
            #[cfg(not(feature = "server"))]
            {
                Err(CoreError::Config(
                    "Remote OCR provider requires the 'server' feature".to_string(),
                ))
            }
        }
    }
}

#[allow(unused_variables)]
fn resolve_llm_provider(
    config: &AiProviderConfig,
    secret_stores: Option<SecretStoreSet>,
) -> LlmProviderResolution {
    match config.llm_provider {
        LlmProviderType::Local => Ok((
            Arc::new(LocalLlmProvider::new()),
            ProviderSource::Local,
            None,
        )),
        LlmProviderType::Remote => {
            #[cfg(feature = "server")]
            {
                resolve_remote_with_optional_fallback(
                    "llm",
                    config.fallback_to_local,
                    || {
                        let endpoint = require_endpoint_config(config.llm_api.as_ref(), "llm_api")?;
                        let secret_store = secret_stores
                            .as_ref()
                            .and_then(|stores| stores.for_binding(endpoint.credential.as_ref()));
                        let credential = CredentialSource::from_api_key_endpoint_for_profile(
                            endpoint,
                            Some("llm"),
                            secret_store,
                        )?;
                        Ok(Arc::new(RemoteLlmProvider::new_with_credential(
                            endpoint, credential,
                        )?) as Arc<dyn LlmProvider>)
                    },
                    || Arc::new(LocalLlmProvider::new()) as Arc<dyn LlmProvider>,
                )
            }
            #[cfg(not(feature = "server"))]
            {
                Err(CoreError::Config(
                    "Remote LLM provider requires the 'server' feature".to_string(),
                ))
            }
        }
    }
}

/// Resolve LLM provider using OAuth-managed credentials.
///
/// Uses surface metadata to resolve the provider ID and authenticated request
/// URL. OpenAI keeps a default managed surface fallback when `llm_api` is omitted.
#[cfg(feature = "server")]
fn resolve_llm_provider_oauth(
    config: &AiProviderConfig,
    oauth_port: Arc<dyn OAuthPort>,
) -> LlmProviderResolution {
    let endpoint = oauth_llm_endpoint(config);
    let provider_id = managed_oauth_provider_id_for_endpoint(
        &endpoint,
        oneshim_api_contracts::provider_specs::ProviderTransportKind::Llm,
    )?;
    let api_base_url = managed_oauth_transport_url_for_endpoint(
        &endpoint,
        oneshim_api_contracts::provider_specs::ProviderTransportKind::Llm,
    )?;
    let credential = CredentialSource::ManagedOAuth {
        provider_id,
        oauth_port,
        api_base_url,
    };

    let provider = RemoteLlmProvider::new_with_credential(&endpoint, credential)?;
    debug!(
        model = %provider.provider_name(),
        "LLM provider resolved with OAuth credential"
    );
    Ok((
        Arc::new(provider) as Arc<dyn LlmProvider>,
        ProviderSource::OAuth,
        None,
    ))
}

#[cfg(feature = "server")]
fn resolve_ocr_provider_oauth(
    config: &AiProviderConfig,
    _pii_filter_level: PiiFilterLevel,
    external_ocr_privacy_guard: Option<ExternalOcrPrivacyGuard>,
    oauth_port: Arc<dyn OAuthPort>,
) -> OcrProviderResolution {
    let endpoint = require_endpoint_config(config.ocr_api.as_ref(), "ocr_api")?;
    let provider_id = managed_oauth_provider_id_for_endpoint(
        endpoint,
        oneshim_api_contracts::provider_specs::ProviderTransportKind::Ocr,
    )?;
    let api_base_url = managed_oauth_transport_url_for_endpoint(
        endpoint,
        oneshim_api_contracts::provider_specs::ProviderTransportKind::Ocr,
    )?;
    let credential = CredentialSource::ManagedOAuth {
        provider_id,
        oauth_port,
        api_base_url,
    };
    let privacy_guard = external_ocr_privacy_guard.ok_or_else(|| {
        CoreError::Config("Remote OCR provider requires a runtime privacy guard".to_string())
    })?;
    let remote = Arc::new(RemoteOcrProvider::new_with_credential(
        endpoint, credential,
    )?) as Arc<dyn OcrProvider>;
    Ok((
        Arc::new(GuardedOcrProvider::new(
            remote,
            privacy_guard,
            config.allow_unredacted_external_ocr,
            config.ocr_validation.clone(),
        )) as Arc<dyn OcrProvider>,
        ProviderSource::OAuth,
        None,
    ))
}

#[cfg(feature = "server")]
fn oauth_llm_endpoint(config: &AiProviderConfig) -> ExternalApiEndpoint {
    let mut endpoint = config.llm_api.clone().unwrap_or(ExternalApiEndpoint {
        endpoint: OAuthProviderConfig::OPENAI_API_BASE_URL.to_string(),
        api_key: String::new(),
        model: Some(DEFAULT_OPENAI_OAUTH_MODEL.to_string()),
        timeout_secs: 30,
        provider_type: AiProviderType::OpenAi,
        surface_id: Some("provider_surface.openai.managed_oauth".to_string()),
        credential: None,
    });

    if endpoint.endpoint.trim().is_empty() {
        endpoint.endpoint = OAuthProviderConfig::OPENAI_API_BASE_URL.to_string();
    }
    if endpoint.timeout_secs == 0 {
        endpoint.timeout_secs = 30;
    }
    if endpoint
        .model
        .as_deref()
        .map(|model| model.trim().is_empty())
        .unwrap_or(true)
    {
        endpoint.model = Some(DEFAULT_OPENAI_OAUTH_MODEL.to_string());
    }
    endpoint.provider_type = AiProviderType::OpenAi;
    endpoint.api_key.clear();
    endpoint
}

#[cfg(feature = "server")]
fn require_endpoint_config<'a>(
    endpoint: Option<&'a ExternalApiEndpoint>,
    field_name: &str,
) -> Result<&'a ExternalApiEndpoint, CoreError> {
    let endpoint = endpoint.ok_or_else(|| {
        CoreError::Config(format!(
            "Remote AI provider usage requires `{field_name}` to be configured."
        ))
    })?;

    if endpoint.endpoint.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "`{field_name}.endpoint` must not be empty."
        )));
    }
    if !(endpoint.endpoint.starts_with("http://") || endpoint.endpoint.starts_with("https://")) {
        return Err(CoreError::Config(format!(
            "`{field_name}.endpoint` must be an http:// or https:// URL."
        )));
    }
    if endpoint.timeout_secs == 0 {
        return Err(CoreError::Config(format!(
            "`{field_name}.timeout_secs` must be greater than 0."
        )));
    }

    Ok(endpoint)
}

#[cfg(feature = "server")]
fn resolve_remote_with_optional_fallback<T: ?Sized>(
    provider_kind: &str,
    fallback_to_local: bool,
    remote_builder: impl FnOnce() -> Result<Arc<T>, CoreError>,
    local_builder: impl FnOnce() -> Arc<T>,
) -> Result<(Arc<T>, ProviderSource, Option<String>), CoreError> {
    match remote_builder() {
        Ok(provider) => Ok((provider, ProviderSource::Remote, None)),
        Err(err) if fallback_to_local => {
            let fallback_reason = format_fallback_reason(&err);
            warn!(
                provider = provider_kind,
                error = %err,
                fallback_reason = %fallback_reason,
                "Remote provider initialization failed, falling back to the local provider"
            );
            Ok((
                local_builder(),
                ProviderSource::LocalFallback,
                Some(fallback_reason),
            ))
        }
        Err(err) => Err(err),
    }
}

#[cfg(feature = "server")]
const MAX_FALLBACK_REASON_CHARS: usize = 240;

#[cfg(feature = "server")]
fn format_fallback_reason(err: &CoreError) -> String {
    let raw = err.to_string().replace(['\n', '\r'], " ");
    let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= MAX_FALLBACK_REASON_CHARS {
        return normalized;
    }

    let truncated: String = normalized.chars().take(MAX_FALLBACK_REASON_CHARS).collect();
    format!("{truncated}...")
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_automation::audit::AuditLogger;
    use oneshim_core::config::{
        AiAccessMode, AiProviderType, CredentialAuthMode, CredentialBackendKind, CredentialBinding,
        ExternalApiEndpoint, ExternalDataPolicy, OcrValidationConfig, PrivacyConfig, SecretRef,
    };
    use oneshim_core::consent::{ConsentManager, ConsentPermissions};
    use oneshim_core::models::context::{ProcessInfo, WindowInfo};
    use oneshim_core::models::event::ProcessDetail;
    use oneshim_core::ports::monitor::ProcessMonitor;
    use oneshim_core::ports::oauth::{
        OAuthConnectionStatus, OAuthFlowHandle, OAuthFlowStatus, OAuthPort, RefreshResult,
    };
    use oneshim_core::ports::ocr_provider::OcrResult;
    use oneshim_core::ports::secret_store::secret_env_var_name;
    use oneshim_storage::env_secret_store::EnvSecretStore;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    fn remote_endpoint() -> ExternalApiEndpoint {
        ExternalApiEndpoint {
            endpoint: "https://api.example.com/v1/messages".to_string(),
            api_key: "test-api-key".to_string(),
            model: Some("test-model".to_string()),
            timeout_secs: 5,
            provider_type: AiProviderType::Generic,
            surface_id: None,
            credential: None,
        }
    }

    struct StaticProcessMonitor {
        active_window: Option<WindowInfo>,
    }

    #[async_trait]
    impl ProcessMonitor for StaticProcessMonitor {
        async fn get_active_window(&self) -> Result<Option<WindowInfo>, CoreError> {
            Ok(self.active_window.clone())
        }

        async fn get_top_processes(&self, _limit: usize) -> Result<Vec<ProcessInfo>, CoreError> {
            Ok(vec![])
        }

        async fn get_detailed_processes(
            &self,
            _foreground_pid: Option<u32>,
            _top_n: usize,
        ) -> Result<Vec<ProcessDetail>, CoreError> {
            Ok(vec![])
        }
    }

    fn write_consent(path: &std::path::Path, ocr_permitted: bool) {
        let mut consent_manager = ConsentManager::new(path.to_path_buf());
        if !ocr_permitted {
            return;
        }

        consent_manager
            .grant_consent(
                ConsentPermissions {
                    ocr_processing: true,
                    screen_capture: true,
                    ..Default::default()
                },
                30,
            )
            .expect("Failed to write consent");
    }

    fn make_external_ocr_guard(
        ocr_permitted: bool,
        active_window: Option<WindowInfo>,
        audit_logger: Option<Arc<RwLock<AuditLogger>>>,
    ) -> (ExternalOcrPrivacyGuard, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let consent_path = temp_dir.path().join("consent.json");
        write_consent(&consent_path, ocr_permitted);

        (
            ExternalOcrPrivacyGuard::new(
                consent_path,
                PiiFilterLevel::Standard,
                ExternalDataPolicy::PiiFilterStandard,
                PrivacyConfig::default(),
                Arc::new(StaticProcessMonitor { active_window }),
                audit_logger,
            ),
            temp_dir,
        )
    }

    #[test]
    fn resolves_local_providers_by_default() {
        let config = AiProviderConfig::default();
        let adapters =
            resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None, None)
                .expect("Failed to resolve default configuration");

        assert_eq!(adapters.ocr_source, ProviderSource::Local);
        assert_eq!(adapters.llm_source, ProviderSource::Local);
        assert!(adapters.ocr_fallback_reason.is_none());
        assert!(adapters.llm_fallback_reason.is_none());
        assert!(!adapters.ocr.is_external());
        assert!(!adapters.llm.is_external());
        assert_eq!(adapters.ocr.provider_name(), "local-tesseract");
        assert_eq!(adapters.llm.provider_name(), "local-rule-based");
    }

    #[test]
    fn resolves_remote_providers_when_configured() {
        let config = AiProviderConfig {
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Remote,
            ocr_api: Some(remote_endpoint()),
            llm_api: Some(remote_endpoint()),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let (privacy_guard, _temp_dir) = make_external_ocr_guard(
            true,
            Some(WindowInfo {
                title: "main.rs".to_string(),
                app_name: "Code".to_string(),
                pid: 7,
                bounds: None,
            }),
            None,
        );
        let adapters = resolve_ai_provider_adapters(
            &config,
            PiiFilterLevel::Standard,
            Some(privacy_guard),
            None,
            None,
        )
        .expect("Failed to resolve remote configuration");

        assert_eq!(adapters.ocr_source, ProviderSource::Remote);
        assert_eq!(adapters.llm_source, ProviderSource::Remote);
        assert!(adapters.ocr_fallback_reason.is_none());
        assert!(adapters.llm_fallback_reason.is_none());
        assert!(adapters.ocr.is_external());
        assert!(adapters.llm.is_external());
    }

    #[test]
    fn falls_back_to_local_when_remote_config_missing() {
        let config = AiProviderConfig {
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Remote,
            ocr_api: None,
            llm_api: None,
            fallback_to_local: true,
            ..AiProviderConfig::default()
        };

        let adapters =
            resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None, None)
                .expect("Fallback configuration resolution should not fail");

        assert_eq!(adapters.ocr_source, ProviderSource::LocalFallback);
        assert_eq!(adapters.llm_source, ProviderSource::LocalFallback);
        assert!(adapters
            .ocr_fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("ocr_api")));
        assert!(adapters
            .llm_fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("llm_api")));
        assert!(!adapters.ocr.is_external());
        assert!(!adapters.llm.is_external());
    }

    #[test]
    fn returns_error_when_remote_config_missing_and_fallback_disabled() {
        let config = AiProviderConfig {
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Local,
            ocr_api: None,
            llm_api: None,
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        match resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None, None) {
            Ok(_) => panic!("Expected an error"),
            Err(CoreError::Config(msg)) => assert!(msg.contains("ocr_api")),
            Err(other) => panic!("Unexpected error type: {other}"),
        }
    }

    #[test]
    fn local_mode_forces_local_adapters_even_if_remote_is_requested() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::LocalModel,
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Remote,
            ocr_api: Some(remote_endpoint()),
            llm_api: Some(remote_endpoint()),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let adapters =
            resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None, None)
                .expect("Failed to resolve local mode");
        assert_eq!(adapters.ocr_source, ProviderSource::Local);
        assert_eq!(adapters.llm_source, ProviderSource::Local);
        assert!(adapters.ocr_fallback_reason.is_none());
        assert!(adapters.llm_fallback_reason.is_none());
        assert!(!adapters.ocr.is_external());
        assert!(!adapters.llm.is_external());
    }

    #[test]
    fn cli_subscription_mode_marks_cli_source() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderSubscriptionCli,
            ..AiProviderConfig::default()
        };

        let (llm, llm_source, llm_fallback_reason) =
            resolve_cli_subscription_llm_provider_with_detected(
                &config,
                &[ProbedSubprocessCli {
                    detected: crate::subprocess_provider::DetectedSubprocessCli {
                        surface_id: "provider_surface.openai.subprocess_cli".to_string(),
                        executable_path: "/tmp/codex".into(),
                    },
                    auth_status: SubprocessCliAuthStatus::Authenticated,
                    auth_detail: Some("cli_authenticated".to_string()),
                }],
            )
            .expect("Failed to resolve CLI mode");

        assert_eq!(llm_source, ProviderSource::CliSubscription);
        assert!(llm_fallback_reason.is_none());
        assert_eq!(llm.provider_name(), "subprocess-codex");
        assert!(llm.is_external());

        let adapters =
            resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None, None)
                .expect("Failed to resolve CLI mode");
        assert_eq!(adapters.ocr_source, ProviderSource::Local);
        assert!(!adapters.ocr.is_external());
        assert!(matches!(
            adapters.llm_source,
            ProviderSource::CliSubscription | ProviderSource::LocalFallback
        ));
    }

    #[test]
    fn cli_subscription_mode_keeps_direct_remote_ocr_when_configured() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderSubscriptionCli,
            ocr_provider: OcrProviderType::Remote,
            ocr_api: Some(remote_endpoint()),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let (privacy_guard, _temp_dir) = make_external_ocr_guard(
            true,
            Some(WindowInfo {
                title: "capture.png".to_string(),
                app_name: "Preview".to_string(),
                pid: 11,
                bounds: None,
            }),
            None,
        );

        let (ocr, ocr_source, ocr_fallback_reason) = resolve_cli_subscription_ocr_provider(
            &config,
            PiiFilterLevel::Standard,
            Some(privacy_guard),
            None,
            &[],
        )
        .expect("CLI mode should allow direct remote OCR");

        assert_eq!(ocr_source, ProviderSource::Remote);
        assert!(ocr_fallback_reason.is_none());
        assert!(ocr.is_external());
    }

    #[test]
    fn cli_subscription_mode_uses_subprocess_ocr_when_supported() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderSubscriptionCli,
            ocr_provider: OcrProviderType::Remote,
            ocr_api: Some(ExternalApiEndpoint {
                endpoint: String::new(),
                api_key: String::new(),
                model: None,
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
                surface_id: Some("provider_surface.openai.subprocess_cli".to_string()),
                credential: None,
            }),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let (ocr, ocr_source, ocr_fallback_reason) = resolve_cli_subscription_ocr_provider(
            &config,
            PiiFilterLevel::Standard,
            None,
            None,
            &[ProbedSubprocessCli {
                detected: crate::subprocess_provider::DetectedSubprocessCli {
                    surface_id: "provider_surface.openai.subprocess_cli".to_string(),
                    executable_path: "/tmp/codex".into(),
                },
                auth_status: SubprocessCliAuthStatus::Authenticated,
                auth_detail: Some("cli_authenticated".to_string()),
            }],
        )
        .expect("expected OCR subprocess runtime to resolve");

        assert_eq!(ocr_source, ProviderSource::CliSubscription);
        assert!(ocr_fallback_reason.is_none());
        assert!(ocr.is_external());
        assert_eq!(ocr.provider_name(), "subprocess-codex");
    }

    #[test]
    fn cli_subscription_mode_falls_back_to_local_when_no_supported_cli_runtime_exists() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderSubscriptionCli,
            fallback_to_local: true,
            ..AiProviderConfig::default()
        };

        let (llm, llm_source, llm_fallback_reason) =
            resolve_cli_subscription_llm_provider_with_detected(&config, &[])
                .expect("CLI mode should fall back to local LLM");

        assert_eq!(llm_source, ProviderSource::LocalFallback);
        assert!(llm_fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("No supported provider CLI runtime")));
        assert_eq!(llm.provider_name(), "local-rule-based");
        assert!(!llm.is_external());
    }

    #[test]
    fn cli_subscription_mode_prefers_matching_provider_surface() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderSubscriptionCli,
            llm_api: Some(ExternalApiEndpoint {
                provider_type: AiProviderType::Anthropic,
                ..remote_endpoint()
            }),
            ..AiProviderConfig::default()
        };

        let (llm, llm_source, llm_fallback_reason) =
            resolve_cli_subscription_llm_provider_with_detected(
                &config,
                &[
                    ProbedSubprocessCli {
                        detected: crate::subprocess_provider::DetectedSubprocessCli {
                            surface_id: "provider_surface.openai.subprocess_cli".to_string(),
                            executable_path: "/tmp/codex".into(),
                        },
                        auth_status: SubprocessCliAuthStatus::Authenticated,
                        auth_detail: Some("cli_authenticated".to_string()),
                    },
                    ProbedSubprocessCli {
                        detected: crate::subprocess_provider::DetectedSubprocessCli {
                            surface_id: "provider_surface.anthropic.subprocess_cli".to_string(),
                            executable_path: "/tmp/claude".into(),
                        },
                        auth_status: SubprocessCliAuthStatus::Authenticated,
                        auth_detail: Some("cli_authenticated".to_string()),
                    },
                ],
            )
            .expect("CLI mode should resolve the Anthropic surface");

        assert_eq!(llm_source, ProviderSource::CliSubscription);
        assert!(llm_fallback_reason.is_none());
        assert_eq!(llm.provider_name(), "subprocess-claude-code");
    }

    #[test]
    fn cli_subscription_mode_reports_auth_required_when_matching_cli_is_logged_out() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderSubscriptionCli,
            llm_api: Some(ExternalApiEndpoint {
                provider_type: AiProviderType::OpenAi,
                ..remote_endpoint()
            }),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        match resolve_cli_subscription_llm_provider_with_detected(
            &config,
            &[ProbedSubprocessCli {
                detected: crate::subprocess_provider::DetectedSubprocessCli {
                    surface_id: "provider_surface.openai.subprocess_cli".to_string(),
                    executable_path: "/tmp/codex".into(),
                },
                auth_status: SubprocessCliAuthStatus::Unauthenticated,
                auth_detail: Some("cli_auth_required".to_string()),
            }],
        ) {
            Err(CoreError::Config(message)) => {
                assert!(message.contains("not authenticated"));
                assert!(message.contains("codex"));
            }
            Ok(_) => panic!("Expected an authentication error"),
            Err(other) => panic!("Unexpected error: {other}"),
        }
    }

    #[test]
    fn cli_subscription_mode_accepts_unknown_auth_for_probe_less_surface() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderSubscriptionCli,
            llm_api: Some(ExternalApiEndpoint {
                provider_type: AiProviderType::Google,
                ..remote_endpoint()
            }),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let (llm, llm_source, llm_fallback_reason) =
            resolve_cli_subscription_llm_provider_with_detected(
                &config,
                &[ProbedSubprocessCli {
                    detected: crate::subprocess_provider::DetectedSubprocessCli {
                        surface_id: "provider_surface.google.subprocess_cli".to_string(),
                        executable_path: "/tmp/gemini".into(),
                    },
                    auth_status: SubprocessCliAuthStatus::Unknown,
                    auth_detail: Some("auth_status_probe_not_implemented".to_string()),
                }],
            )
            .expect("CLI mode should allow probe-less Gemini runtime");

        assert_eq!(llm_source, ProviderSource::CliSubscription);
        assert!(llm_fallback_reason.is_none());
        assert_eq!(llm.provider_name(), "subprocess-gemini-cli");
    }

    #[test]
    fn legacy_platform_connected_config_reuses_direct_remote_sources() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::PlatformConnected,
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Remote,
            ocr_api: Some(remote_endpoint()),
            llm_api: Some(remote_endpoint()),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let (privacy_guard, _temp_dir) = make_external_ocr_guard(
            true,
            Some(WindowInfo {
                title: "mail".to_string(),
                app_name: "Code".to_string(),
                pid: 9,
                bounds: None,
            }),
            None,
        );
        let adapters = resolve_ai_provider_adapters(
            &config,
            PiiFilterLevel::Standard,
            Some(privacy_guard),
            None,
            None,
        )
        .expect("Failed to resolve legacy platform-connected config");
        assert_eq!(adapters.ocr_source, ProviderSource::Remote);
        assert_eq!(adapters.llm_source, ProviderSource::Remote);
        assert!(adapters.ocr_fallback_reason.is_none());
        assert!(adapters.llm_fallback_reason.is_none());
        assert!(adapters.ocr.is_external());
        assert!(adapters.llm.is_external());
    }

    struct FakeExternalOcrProvider {
        responses: Vec<OcrResult>,
    }

    struct FakeOAuthPort {
        connected: bool,
    }

    #[async_trait]
    impl OAuthPort for FakeOAuthPort {
        async fn start_flow(&self, _provider_id: &str) -> Result<OAuthFlowHandle, CoreError> {
            Ok(OAuthFlowHandle {
                flow_id: "flow-1".to_string(),
                auth_url: "https://example.com/oauth".to_string(),
            })
        }

        async fn flow_status(&self, _flow_id: &str) -> Result<OAuthFlowStatus, CoreError> {
            Ok(OAuthFlowStatus::Completed)
        }

        async fn cancel_flow(&self, _flow_id: &str) -> Result<(), CoreError> {
            Ok(())
        }

        async fn get_access_token(&self, _provider_id: &str) -> Result<Option<String>, CoreError> {
            Ok(self.connected.then(|| "token".to_string()))
        }

        async fn revoke(&self, _provider_id: &str) -> Result<(), CoreError> {
            Ok(())
        }

        async fn connection_status(
            &self,
            provider_id: &str,
        ) -> Result<OAuthConnectionStatus, CoreError> {
            Ok(OAuthConnectionStatus {
                provider_id: provider_id.to_string(),
                connected: self.connected,
                expires_at: None,
                scopes: vec![],
                api_base_url: None,
                has_refresh_token: false,
            })
        }

        async fn refresh_access_token(
            &self,
            _provider_id: &str,
            _min_valid_for_secs: i64,
        ) -> Result<RefreshResult, CoreError> {
            if self.connected {
                Ok(RefreshResult::AlreadyFresh {
                    expires_at: chrono::Utc::now().to_rfc3339(),
                })
            } else {
                Ok(RefreshResult::NotAuthenticated)
            }
        }
    }

    #[async_trait]
    impl OcrProvider for FakeExternalOcrProvider {
        async fn extract_elements(
            &self,
            _image: &[u8],
            _image_format: &str,
        ) -> Result<Vec<OcrResult>, CoreError> {
            Ok(self.responses.clone())
        }

        fn provider_name(&self) -> &str {
            "fake-external"
        }

        fn is_external(&self) -> bool {
            true
        }
    }

    #[test]
    fn remote_ocr_requires_runtime_privacy_guard() {
        let config = AiProviderConfig {
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Local,
            ocr_api: Some(remote_endpoint()),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let result =
            resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None, None);
        assert!(
            result.is_err(),
            "Expected remote OCR resolution to require a privacy guard"
        );
        let err = result.err().unwrap();
        assert!(err.to_string().contains("runtime privacy guard"));
    }

    #[test]
    fn oauth_mode_requires_oauth_port() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            llm_provider: LlmProviderType::Remote,
            ..AiProviderConfig::default()
        };

        let result =
            resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None, None);
        assert!(
            result.is_err(),
            "ProviderOAuth mode should require an OAuth port"
        );
        let err = result.err().unwrap();
        assert!(err.to_string().contains("OAuth runtime"));
    }

    #[test]
    fn oauth_mode_allows_local_llm_when_no_managed_llm_surface_is_selected() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            llm_provider: LlmProviderType::Local,
            ..AiProviderConfig::default()
        };

        let oauth = Arc::new(FakeOAuthPort { connected: true }) as Arc<dyn OAuthPort>;
        let result = resolve_ai_provider_adapters(
            &config,
            PiiFilterLevel::Standard,
            None,
            None,
            Some(oauth),
        )
        .expect(
            "ProviderOAuth mode should allow local LLM when no managed LLM surface is selected",
        );
        assert_eq!(result.llm_source, ProviderSource::Local);
    }

    #[test]
    fn oauth_mode_defaults_to_openai_model() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            llm_provider: LlmProviderType::Remote,
            ..AiProviderConfig::default()
        };

        let oauth = Arc::new(FakeOAuthPort { connected: true }) as Arc<dyn OAuthPort>;
        let adapters = resolve_ai_provider_adapters(
            &config,
            PiiFilterLevel::Standard,
            None,
            None,
            Some(oauth),
        )
        .expect("OAuth mode should resolve when a port is provided");

        assert_eq!(adapters.llm_source, ProviderSource::OAuth);
        assert_eq!(adapters.llm.provider_name(), DEFAULT_OPENAI_OAUTH_MODEL);
    }

    #[test]
    fn remote_ocr_falls_back_when_selected_managed_ocr_surface_lacks_runtime() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            ocr_provider: OcrProviderType::Remote,
            ocr_api: Some(ExternalApiEndpoint {
                endpoint: String::new(),
                api_key: String::new(),
                model: None,
                timeout_secs: 30,
                provider_type: AiProviderType::OpenAi,
                surface_id: Some("provider_surface.openai.managed_oauth".to_string()),
                credential: None,
            }),
            fallback_to_local: true,
            ..AiProviderConfig::default()
        };

        let (ocr, source, reason) =
            resolve_ocr_provider(&config, PiiFilterLevel::Standard, None, None)
                .expect("managed OCR surface should fall back to local when enabled");

        assert_eq!(source, ProviderSource::LocalFallback);
        assert!(reason
            .as_deref()
            .is_some_and(|message| message.contains("managed_oauth")));
        assert!(!ocr.is_external());
    }

    #[test]
    fn oauth_mode_resolves_google_managed_ocr_surface() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            llm_provider: LlmProviderType::Local,
            ocr_provider: OcrProviderType::Remote,
            ocr_api: Some(ExternalApiEndpoint {
                endpoint: "https://vision.googleapis.com/v1/images:annotate".to_string(),
                api_key: String::new(),
                model: None,
                timeout_secs: 30,
                provider_type: AiProviderType::Google,
                surface_id: Some("provider_surface.google.managed_oauth".to_string()),
                credential: None,
            }),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let oauth = Arc::new(FakeOAuthPort { connected: true }) as Arc<dyn OAuthPort>;
        let (privacy_guard, _tempdir) = make_external_ocr_guard(
            true,
            Some(WindowInfo {
                app_name: "Terminal".to_string(),
                title: "OCR".to_string(),
                pid: 4242,
                bounds: None,
            }),
            None,
        );
        let adapters = resolve_ai_provider_adapters(
            &config,
            PiiFilterLevel::Standard,
            Some(privacy_guard),
            None,
            Some(oauth),
        )
        .expect("Google OCR managed OAuth should resolve when an OAuth port is available");

        assert_eq!(adapters.ocr_source, ProviderSource::OAuth);
        assert!(adapters.ocr.is_external());
    }

    #[test]
    fn resolves_remote_providers_from_secret_binding_with_plaintext_empty() {
        let namespace = "provider/openai/default";
        let key = "api_key";
        let mut snapshot = std::collections::HashMap::new();
        snapshot.insert(
            secret_env_var_name(namespace, key),
            "sk-secret-store".to_string(),
        );
        let secret_store = Arc::new(EnvSecretStore::from_snapshot(snapshot));
        let secret_stores = SecretStoreSet {
            os_secret_store: None,
            file_secret_store: None,
            env_secret_store: Some(secret_store),
            default_backend_kind: CredentialBackendKind::Env,
            fallback_backend_kind: CredentialBackendKind::LegacyConfig,
        };

        let secret_bound_endpoint = ExternalApiEndpoint {
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: Some("gpt-5.4".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::OpenAi,
            surface_id: None,
            credential: Some(CredentialBinding {
                auth_mode: CredentialAuthMode::ApiKey,
                backend_kind: CredentialBackendKind::Env,
                secret_ref: Some(SecretRef {
                    namespace: namespace.to_string(),
                    key: key.to_string(),
                }),
                projection_enabled: false,
            }),
        };
        let config = AiProviderConfig {
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Remote,
            ocr_api: Some(secret_bound_endpoint.clone()),
            llm_api: Some(secret_bound_endpoint),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let (privacy_guard, _temp_dir) = make_external_ocr_guard(
            true,
            Some(WindowInfo {
                title: "mail".to_string(),
                app_name: "Code".to_string(),
                pid: 9,
                bounds: None,
            }),
            None,
        );
        let adapters = resolve_ai_provider_adapters(
            &config,
            PiiFilterLevel::Standard,
            Some(privacy_guard),
            Some(secret_stores),
            None,
        )
        .expect("Secret-bound API key configuration should resolve");

        assert_eq!(adapters.ocr_source, ProviderSource::Remote);
        assert_eq!(adapters.llm_source, ProviderSource::Remote);
        assert!(adapters.ocr_fallback_reason.is_none());
        assert!(adapters.llm_fallback_reason.is_none());
        assert!(adapters.ocr.is_external());
        assert!(adapters.llm.is_external());
    }

    #[tokio::test]
    async fn guarded_ocr_provider_filters_invalid_results_when_ratio_is_within_limit() {
        let inner = Arc::new(FakeExternalOcrProvider {
            responses: vec![
                OcrResult {
                    text: "save".to_string(),
                    x: 10,
                    y: 10,
                    width: 40,
                    height: 20,
                    confidence: 0.9,
                },
                OcrResult {
                    text: "   ".to_string(),
                    x: 12,
                    y: 10,
                    width: 10,
                    height: 20,
                    confidence: 0.9,
                },
                OcrResult {
                    text: "bad-confidence".to_string(),
                    x: 30,
                    y: 22,
                    width: 20,
                    height: 10,
                    confidence: 1.5,
                },
            ],
        }) as Arc<dyn OcrProvider>;
        let (privacy_guard, _temp_dir) = make_external_ocr_guard(
            true,
            Some(WindowInfo {
                title: "main.rs".to_string(),
                app_name: "Code".to_string(),
                pid: 11,
                bounds: None,
            }),
            None,
        );
        let guarded = GuardedOcrProvider::new(
            inner,
            privacy_guard,
            true,
            OcrValidationConfig {
                enabled: true,
                min_confidence: 0.5,
                max_invalid_ratio: 0.8,
            },
        );

        let results = guarded.extract_elements(b"dummy", "png").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "save");
    }

    #[tokio::test]
    async fn guarded_ocr_provider_rejects_when_invalid_ratio_exceeds_limit() {
        let inner = Arc::new(FakeExternalOcrProvider {
            responses: vec![
                OcrResult {
                    text: "ok".to_string(),
                    x: 1,
                    y: 1,
                    width: 10,
                    height: 10,
                    confidence: 0.9,
                },
                OcrResult {
                    text: "".to_string(),
                    x: 1,
                    y: 1,
                    width: 0,
                    height: 0,
                    confidence: 0.9,
                },
            ],
        }) as Arc<dyn OcrProvider>;
        let audit_logger = Arc::new(RwLock::new(AuditLogger::new(32, 8)));
        let (privacy_guard, _temp_dir) = make_external_ocr_guard(
            true,
            Some(WindowInfo {
                title: "main.rs".to_string(),
                app_name: "Code".to_string(),
                pid: 12,
                bounds: None,
            }),
            Some(audit_logger.clone()),
        );
        let guarded = GuardedOcrProvider::new(
            inner,
            privacy_guard,
            true,
            OcrValidationConfig {
                enabled: true,
                min_confidence: 0.5,
                max_invalid_ratio: 0.2,
            },
        );

        let err = guarded.extract_elements(b"dummy", "png").await.unwrap_err();
        assert!(err.to_string().contains("invalid_ratio"));
    }

    #[tokio::test]
    async fn guarded_ocr_provider_denies_without_ocr_consent_and_audits_it() {
        let inner = Arc::new(FakeExternalOcrProvider {
            responses: vec![OcrResult {
                text: "save".to_string(),
                x: 1,
                y: 1,
                width: 10,
                height: 10,
                confidence: 0.9,
            }],
        }) as Arc<dyn OcrProvider>;
        let audit_logger = Arc::new(RwLock::new(AuditLogger::new(32, 8)));
        let (privacy_guard, _temp_dir) = make_external_ocr_guard(
            false,
            Some(WindowInfo {
                title: "main.rs".to_string(),
                app_name: "Code".to_string(),
                pid: 13,
                bounds: None,
            }),
            Some(audit_logger.clone()),
        );
        let guarded =
            GuardedOcrProvider::new(inner, privacy_guard, false, OcrValidationConfig::default());

        let err = guarded.extract_elements(b"dummy", "png").await.unwrap_err();
        assert!(err.to_string().contains("OCR consent is required"));

        let logger = audit_logger.read().await;
        assert_eq!(logger.pending_count(), 1);
        assert!(logger.recent_entries(1)[0]
            .details
            .as_deref()
            .is_some_and(|details| details.contains("reason=OCR consent is required")));
    }

    #[tokio::test]
    async fn guarded_ocr_provider_denies_sensitive_apps() {
        let inner = Arc::new(FakeExternalOcrProvider {
            responses: vec![OcrResult {
                text: "save".to_string(),
                x: 1,
                y: 1,
                width: 10,
                height: 10,
                confidence: 0.9,
            }],
        }) as Arc<dyn OcrProvider>;
        let (privacy_guard, _temp_dir) = make_external_ocr_guard(
            true,
            Some(WindowInfo {
                title: "Vault".to_string(),
                app_name: "1Password".to_string(),
                pid: 14,
                bounds: None,
            }),
            None,
        );
        let guarded =
            GuardedOcrProvider::new(inner, privacy_guard, false, OcrValidationConfig::default());

        let err = guarded.extract_elements(b"dummy", "png").await.unwrap_err();
        assert!(err.to_string().contains("Blocked sensitive app"));
    }
}
