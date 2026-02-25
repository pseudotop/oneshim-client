//!

use std::sync::Arc;

use async_trait::async_trait;
use oneshim_automation::local_llm::LocalLlmProvider;
use oneshim_core::config::{
    AiAccessMode, AiProviderConfig, ExternalApiEndpoint, ExternalDataPolicy, LlmProviderType,
    OcrProviderType, OcrValidationConfig, PiiFilterLevel,
};
use oneshim_core::error::CoreError;
use oneshim_core::ports::llm_provider::LlmProvider;
use oneshim_core::ports::ocr_provider::{OcrProvider, OcrResult};
use oneshim_network::ai_llm_client::RemoteLlmProvider;
use oneshim_network::ai_ocr_client::RemoteOcrProvider;
use oneshim_vision::local_ocr_provider::LocalOcrProvider;
use oneshim_vision::privacy_gateway::PrivacyGateway;
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSource {
    Local,
    Remote,
    LocalFallback,
    CliSubscription,
    Platform,
}

impl ProviderSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::LocalFallback => "local-fallback",
            Self::CliSubscription => "cli-subscription",
            Self::Platform => "platform",
        }
    }
}

pub struct AiProviderAdapters {
    pub ocr: Arc<dyn OcrProvider>,
    pub llm: Arc<dyn LlmProvider>,
    pub ocr_source: ProviderSource,
    pub llm_source: ProviderSource,
}

///
struct GuardedOcrProvider {
    inner: Arc<dyn OcrProvider>,
    pii_filter_level: PiiFilterLevel,
    external_data_policy: ExternalDataPolicy,
    allow_unredacted_external_ocr: bool,
    ocr_validation: OcrValidationConfig,
}

impl GuardedOcrProvider {
    fn new(
        inner: Arc<dyn OcrProvider>,
        pii_filter_level: PiiFilterLevel,
        external_data_policy: ExternalDataPolicy,
        allow_unredacted_external_ocr: bool,
        ocr_validation: OcrValidationConfig,
    ) -> Self {
        Self {
            inner,
            pii_filter_level,
            external_data_policy,
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

        let sanitized = PrivacyGateway::sanitize_image_for_external_policy(
            image,
            self.pii_filter_level,
            self.external_data_policy,
            self.allow_unredacted_external_ocr,
        )
        .await;

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
) -> Result<AiProviderAdapters, CoreError> {
    match config.access_mode {
        AiAccessMode::LocalModel => Ok(AiProviderAdapters {
            ocr: Arc::new(LocalOcrProvider::new()),
            llm: Arc::new(LocalLlmProvider::new()),
            ocr_source: ProviderSource::Local,
            llm_source: ProviderSource::Local,
        }),
        AiAccessMode::ProviderSubscriptionCli => Ok(AiProviderAdapters {
            ocr: Arc::new(LocalOcrProvider::new()),
            llm: Arc::new(LocalLlmProvider::new()),
            ocr_source: ProviderSource::CliSubscription,
            llm_source: ProviderSource::CliSubscription,
        }),
        AiAccessMode::ProviderApiKey => {
            let (ocr, ocr_source) = resolve_ocr_provider(config, pii_filter_level)?;
            let (llm, llm_source) = resolve_llm_provider(config)?;
            Ok(AiProviderAdapters {
                ocr,
                llm,
                ocr_source,
                llm_source,
            })
        }
        AiAccessMode::PlatformConnected => {
            let (ocr, ocr_source) = resolve_ocr_provider(config, pii_filter_level)?;
            let (llm, llm_source) = resolve_llm_provider(config)?;
            Ok(AiProviderAdapters {
                ocr,
                llm,
                ocr_source: to_platform_source(ocr_source),
                llm_source: to_platform_source(llm_source),
            })
        }
    }
}

fn to_platform_source(source: ProviderSource) -> ProviderSource {
    match source {
        ProviderSource::Remote => ProviderSource::Platform,
        other => other,
    }
}

fn resolve_ocr_provider(
    config: &AiProviderConfig,
    pii_filter_level: PiiFilterLevel,
) -> Result<(Arc<dyn OcrProvider>, ProviderSource), CoreError> {
    match config.ocr_provider {
        OcrProviderType::Local => Ok((Arc::new(LocalOcrProvider::new()), ProviderSource::Local)),
        OcrProviderType::Remote => resolve_remote_with_optional_fallback(
            "ocr",
            config.fallback_to_local,
            || {
                let endpoint = require_endpoint_config(config.ocr_api.as_ref(), "ocr_api")?;
                let remote = Arc::new(RemoteOcrProvider::new(endpoint)?) as Arc<dyn OcrProvider>;
                Ok(Arc::new(GuardedOcrProvider::new(
                    remote,
                    pii_filter_level,
                    config.external_data_policy,
                    config.allow_unredacted_external_ocr,
                    config.ocr_validation.clone(),
                )) as Arc<dyn OcrProvider>)
            },
            || Arc::new(LocalOcrProvider::new()) as Arc<dyn OcrProvider>,
        ),
    }
}

fn resolve_llm_provider(
    config: &AiProviderConfig,
) -> Result<(Arc<dyn LlmProvider>, ProviderSource), CoreError> {
    match config.llm_provider {
        LlmProviderType::Local => Ok((Arc::new(LocalLlmProvider::new()), ProviderSource::Local)),
        LlmProviderType::Remote => resolve_remote_with_optional_fallback(
            "llm",
            config.fallback_to_local,
            || {
                let endpoint = require_endpoint_config(config.llm_api.as_ref(), "llm_api")?;
                Ok(Arc::new(RemoteLlmProvider::new(endpoint)?) as Arc<dyn LlmProvider>)
            },
            || Arc::new(LocalLlmProvider::new()) as Arc<dyn LlmProvider>,
        ),
    }
}

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

fn resolve_remote_with_optional_fallback<T: ?Sized>(
    provider_kind: &str,
    fallback_to_local: bool,
    remote_builder: impl FnOnce() -> Result<Arc<T>, CoreError>,
    local_builder: impl FnOnce() -> Arc<T>,
) -> Result<(Arc<T>, ProviderSource), CoreError> {
    match remote_builder() {
        Ok(provider) => Ok((provider, ProviderSource::Remote)),
        Err(err) if fallback_to_local => {
            warn!(
                provider = provider_kind,
                error = %err,
                "원격 제공자 initialize failure, 로컬 제공자로 폴백"
            );
            Ok((local_builder(), ProviderSource::LocalFallback))
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::{
        AiAccessMode, AiProviderType, ExternalApiEndpoint, ExternalDataPolicy, OcrValidationConfig,
    };
    use oneshim_core::ports::ocr_provider::OcrResult;

    fn remote_endpoint() -> ExternalApiEndpoint {
        ExternalApiEndpoint {
            endpoint: "https://api.example.com/v1/messages".to_string(),
            api_key: "test-api-key".to_string(),
            model: Some("test-model".to_string()),
            timeout_secs: 5,
            provider_type: AiProviderType::Generic,
        }
    }

    #[test]
    fn resolves_local_providers_by_default() {
        let config = AiProviderConfig::default();
        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard)
            .expect("Failed to resolve default configuration");

        assert_eq!(adapters.ocr_source, ProviderSource::Local);
        assert_eq!(adapters.llm_source, ProviderSource::Local);
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

        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard)
            .expect("Failed to resolve remote configuration");

        assert_eq!(adapters.ocr_source, ProviderSource::Remote);
        assert_eq!(adapters.llm_source, ProviderSource::Remote);
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

        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard)
            .expect("Fallback configuration resolution should not fail");

        assert_eq!(adapters.ocr_source, ProviderSource::LocalFallback);
        assert_eq!(adapters.llm_source, ProviderSource::LocalFallback);
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

        match resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard) {
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

        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard)
            .expect("Failed to resolve local mode");
        assert_eq!(adapters.ocr_source, ProviderSource::Local);
        assert_eq!(adapters.llm_source, ProviderSource::Local);
        assert!(!adapters.ocr.is_external());
        assert!(!adapters.llm.is_external());
    }

    #[test]
    fn cli_subscription_mode_marks_cli_source() {
        let config = AiProviderConfig {
            access_mode: AiAccessMode::ProviderSubscriptionCli,
            ..AiProviderConfig::default()
        };

        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard)
            .expect("Failed to resolve CLI mode");
        assert_eq!(adapters.ocr_source, ProviderSource::CliSubscription);
        assert_eq!(adapters.llm_source, ProviderSource::CliSubscription);
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

        let adapters = resolve_ai_provider_adapters(&config, PiiFilterLevel::Standard)
            .expect("Failed to resolve platform mode");
        assert_eq!(adapters.ocr_source, ProviderSource::Platform);
        assert_eq!(adapters.llm_source, ProviderSource::Platform);
        assert!(adapters.ocr.is_external());
        assert!(adapters.llm.is_external());
    }

    struct FakeExternalOcrProvider {
        responses: Vec<OcrResult>,
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
        let guarded = GuardedOcrProvider::new(
            inner,
            PiiFilterLevel::Standard,
            ExternalDataPolicy::PiiFilterStandard,
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
        let guarded = GuardedOcrProvider::new(
            inner,
            PiiFilterLevel::Standard,
            ExternalDataPolicy::PiiFilterStrict,
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
}
