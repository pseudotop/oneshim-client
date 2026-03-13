use std::sync::Arc;

#[cfg(feature = "server")]
use async_trait::async_trait;
use oneshim_automation::audit::AuditLogger;
use oneshim_automation::local_llm::LocalLlmProvider;
use oneshim_core::config::PrivacyConfig;
use oneshim_core::config::{
    AiAccessMode, AiProviderConfig, LlmProviderType, OcrProviderType, PiiFilterLevel,
};
#[cfg(feature = "server")]
use oneshim_core::config::{
    AiProviderType, ExternalApiEndpoint, ExternalDataPolicy, OcrValidationConfig,
};
#[cfg(not(feature = "server"))]
use oneshim_core::config::{ExternalDataPolicy, OcrValidationConfig};
use oneshim_core::consent::ConsentManager;
use oneshim_core::error::CoreError;
use oneshim_core::ports::llm_provider::LlmProvider;
use oneshim_core::ports::monitor::ProcessMonitor;
#[cfg(feature = "server")]
use oneshim_core::ports::oauth::OAuthPort;
use oneshim_core::ports::ocr_provider::OcrProvider;
use oneshim_core::ports::ocr_provider::OcrResult;
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
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProviderSource {
    Local,
    Remote,
    LocalFallback,
    CliSubscription,
    Platform,
    OAuth,
}

impl ProviderSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::LocalFallback => "local-fallback",
            Self::CliSubscription => "cli-subscription",
            Self::Platform => "platform",
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
const DEFAULT_OPENAI_OAUTH_MODEL: &str = "gpt-4.1-mini";

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
            "외부 OCR sent 전 이미지 세정 completed"
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
    #[cfg(feature = "server")] oauth_port: Option<Arc<dyn OAuthPort>>,
) -> Result<AiProviderAdapters, CoreError> {
    match config.access_mode {
        AiAccessMode::LocalModel => Ok(AiProviderAdapters {
            ocr: Arc::new(LocalOcrProvider::new()),
            llm: Arc::new(LocalLlmProvider::new()),
            ocr_source: ProviderSource::Local,
            llm_source: ProviderSource::Local,
            ocr_fallback_reason: None,
            llm_fallback_reason: None,
        }),
        AiAccessMode::ProviderSubscriptionCli => Ok(AiProviderAdapters {
            ocr: Arc::new(LocalOcrProvider::new()),
            llm: Arc::new(LocalLlmProvider::new()),
            ocr_source: ProviderSource::CliSubscription,
            llm_source: ProviderSource::CliSubscription,
            ocr_fallback_reason: None,
            llm_fallback_reason: None,
        }),
        AiAccessMode::ProviderApiKey => {
            let (ocr, ocr_source, ocr_fallback_reason) =
                resolve_ocr_provider(config, pii_filter_level, external_ocr_privacy_guard.clone())?;
            let (llm, llm_source, llm_fallback_reason) = resolve_llm_provider(config)?;
            Ok(AiProviderAdapters {
                ocr,
                llm,
                ocr_source,
                llm_source,
                ocr_fallback_reason,
                llm_fallback_reason,
            })
        }
        AiAccessMode::PlatformConnected => {
            let (ocr, ocr_source, ocr_fallback_reason) =
                resolve_ocr_provider(config, pii_filter_level, external_ocr_privacy_guard.clone())?;
            let (llm, llm_source, llm_fallback_reason) = resolve_llm_provider(config)?;
            Ok(AiProviderAdapters {
                ocr,
                llm,
                ocr_source: to_platform_source(ocr_source),
                llm_source: to_platform_source(llm_source),
                ocr_fallback_reason,
                llm_fallback_reason,
            })
        }
        AiAccessMode::ProviderOAuth => {
            // OAuth mode intentionally does NOT respect fallback_to_local for LLM.
            // An authentication failure means the user has not connected via OAuth yet
            // and should be prompted to do so — silently falling back to a local model
            // would hide the misconfiguration.
            #[cfg(feature = "server")]
            {
                if config.llm_provider != LlmProviderType::Remote {
                    return Err(CoreError::Config(
                        "ProviderOAuth mode requires llm_provider=Remote.".to_string(),
                    ));
                }
                let oauth = oauth_port.ok_or_else(|| {
                    CoreError::Config(
                        "ProviderOAuth mode requires an initialized OAuth runtime.".to_string(),
                    )
                })?;
                let (ocr, ocr_source, ocr_fallback_reason) = resolve_ocr_provider(
                    config,
                    pii_filter_level,
                    external_ocr_privacy_guard.clone(),
                )?;
                let (llm, llm_source, llm_fallback_reason) =
                    resolve_llm_provider_oauth(config, oauth)?;
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
    }
}

fn to_platform_source(source: ProviderSource) -> ProviderSource {
    match source {
        ProviderSource::Remote => ProviderSource::Platform,
        other => other,
    }
}

#[allow(unused_variables)]
fn resolve_ocr_provider(
    config: &AiProviderConfig,
    pii_filter_level: PiiFilterLevel,
    external_ocr_privacy_guard: Option<ExternalOcrPrivacyGuard>,
) -> OcrProviderResolution {
    match config.ocr_provider {
        OcrProviderType::Local => Ok((
            Arc::new(LocalOcrProvider::new()),
            ProviderSource::Local,
            None,
        )),
        OcrProviderType::Remote => {
            #[cfg(feature = "server")]
            {
                resolve_remote_with_optional_fallback(
                    "ocr",
                    config.fallback_to_local,
                    || {
                        let endpoint = require_endpoint_config(config.ocr_api.as_ref(), "ocr_api")?;
                        let privacy_guard =
                            external_ocr_privacy_guard.clone().ok_or_else(|| {
                                CoreError::Config(
                                    "Remote OCR provider requires a runtime privacy guard"
                                        .to_string(),
                                )
                            })?;
                        let remote =
                            Arc::new(RemoteOcrProvider::new(endpoint)?) as Arc<dyn OcrProvider>;
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

fn resolve_llm_provider(config: &AiProviderConfig) -> LlmProviderResolution {
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
                        Ok(Arc::new(RemoteLlmProvider::new(endpoint)?) as Arc<dyn LlmProvider>)
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
/// Uses `OAuthProviderConfig::openai_codex()` defaults for endpoint/model
/// when `config.llm_api` is not set. The credential's `api_base_url`
/// overrides the endpoint at request time (ChatGPT OAuth uses a different
/// API endpoint than the standard OpenAI API).
#[cfg(feature = "server")]
fn resolve_llm_provider_oauth(
    config: &AiProviderConfig,
    oauth_port: Arc<dyn OAuthPort>,
) -> LlmProviderResolution {
    use oneshim_core::ports::credential_source::CredentialSource;

    // Currently the only supported OAuth provider. When adding more providers,
    // this should be derived from config (e.g., config.oauth_provider_id).
    let provider_id = "openai".to_string();
    let api_base_url = OAuthProviderConfig::openai_codex().api_base_url;

    let credential = CredentialSource::ManagedOAuth {
        provider_id,
        oauth_port,
        api_base_url,
    };

    let endpoint = oauth_llm_endpoint(config);

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
fn oauth_llm_endpoint(config: &AiProviderConfig) -> ExternalApiEndpoint {
    let mut endpoint = config.llm_api.clone().unwrap_or(ExternalApiEndpoint {
        endpoint: OAuthProviderConfig::OPENAI_API_BASE_URL.to_string(),
        api_key: String::new(),
        model: Some(DEFAULT_OPENAI_OAUTH_MODEL.to_string()),
        timeout_secs: 30,
        provider_type: AiProviderType::OpenAi,
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
            "원격 AI 제공자를 사용하려면 `{field_name}` 설정이 필요합니다."
        ))
    })?;

    if endpoint.endpoint.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "`{field_name}.endpoint` 값이 비어 있습니다."
        )));
    }
    if !(endpoint.endpoint.starts_with("http://") || endpoint.endpoint.starts_with("https://")) {
        return Err(CoreError::Config(format!(
            "`{field_name}.endpoint`는 http:// https:// URL."
        )));
    }
    if endpoint.timeout_secs == 0 {
        return Err(CoreError::Config(format!(
            "`{field_name}.timeout_secs`는 1 이상이어야 합니다."
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
                "원격 제공자 initialize failure, 로컬 제공자로 폴백"
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
        AiAccessMode, AiProviderType, ExternalApiEndpoint, ExternalDataPolicy, OcrValidationConfig,
        PrivacyConfig,
    };
    use oneshim_core::consent::{ConsentManager, ConsentPermissions};
    use oneshim_core::models::context::{ProcessInfo, WindowInfo};
    use oneshim_core::models::event::ProcessDetail;
    use oneshim_core::ports::monitor::ProcessMonitor;
    use oneshim_core::ports::oauth::{
        OAuthConnectionStatus, OAuthFlowHandle, OAuthFlowStatus, OAuthPort,
    };
    use oneshim_core::ports::ocr_provider::OcrResult;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    fn remote_endpoint() -> ExternalApiEndpoint {
        ExternalApiEndpoint {
            endpoint: "https://api.example.com/v1/messages".to_string(),
            api_key: "test-api-key".to_string(),
            model: Some("test-model".to_string()),
            timeout_secs: 5,
            provider_type: AiProviderType::Generic,
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
        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None)
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

        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None)
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

        match resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None) {
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

        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None)
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

        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None)
            .expect("Failed to resolve CLI mode");
        assert_eq!(adapters.ocr_source, ProviderSource::CliSubscription);
        assert_eq!(adapters.llm_source, ProviderSource::CliSubscription);
        assert!(adapters.ocr_fallback_reason.is_none());
        assert!(adapters.llm_fallback_reason.is_none());
        assert!(!adapters.ocr.is_external());
        assert!(!adapters.llm.is_external());
    }

    #[test]
    fn platform_mode_marks_remote_as_platform_source() {
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
        )
        .expect("Failed to resolve platform mode");
        assert_eq!(adapters.ocr_source, ProviderSource::Platform);
        assert_eq!(adapters.llm_source, ProviderSource::Platform);
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
            })
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

        let result = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None);
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

        let result = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, None);
        assert!(
            result.is_err(),
            "ProviderOAuth mode should require an OAuth port"
        );
        let err = result.err().unwrap();
        assert!(err.to_string().contains("OAuth runtime"));
    }

    #[test]
    fn oauth_mode_rejects_local_llm_configuration() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            llm_provider: LlmProviderType::Local,
            ..AiProviderConfig::default()
        };

        let oauth = Arc::new(FakeOAuthPort { connected: true }) as Arc<dyn OAuthPort>;
        let result =
            resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, Some(oauth));
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("llm_provider=Remote"));
    }

    #[test]
    fn oauth_mode_defaults_to_openai_model() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderOAuth,
            llm_provider: LlmProviderType::Remote,
            ..AiProviderConfig::default()
        };

        let oauth = Arc::new(FakeOAuthPort { connected: true }) as Arc<dyn OAuthPort>;
        let adapters =
            resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard, None, Some(oauth))
                .expect("OAuth mode should resolve when a port is provided");

        assert_eq!(adapters.llm_source, ProviderSource::OAuth);
        assert_eq!(adapters.llm.provider_name(), DEFAULT_OPENAI_OAUTH_MODEL);
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
