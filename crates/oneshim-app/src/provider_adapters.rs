//! AI 제공자 어댑터 해석기.
//!
//! 앱 구성(`AiProviderConfig`)을 기준으로 OCR/LLM 제공자를 일관되게 선택한다.
//! 원격 제공자 초기화 실패 시 `fallback_to_local` 정책을 공통 처리한다.

use std::sync::Arc;

use oneshim_automation::local_llm::LocalLlmProvider;
use oneshim_core::config::{
    AiProviderConfig, ExternalApiEndpoint, LlmProviderType, OcrProviderType,
};
use oneshim_core::error::CoreError;
use oneshim_core::ports::llm_provider::LlmProvider;
use oneshim_core::ports::ocr_provider::OcrProvider;
use oneshim_network::ai_llm_client::RemoteLlmProvider;
use oneshim_network::ai_ocr_client::RemoteOcrProvider;
use oneshim_vision::local_ocr_provider::LocalOcrProvider;
use tracing::warn;

/// 제공자 선택 출처.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSource {
    /// 설정에서 Local 명시 또는 기본값
    Local,
    /// 설정에서 Remote 명시 + 원격 초기화 성공
    Remote,
    /// 설정은 Remote였지만 오류로 Local 폴백
    LocalFallback,
}

impl ProviderSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::LocalFallback => "local-fallback",
        }
    }
}

/// 런타임에서 사용할 AI 제공자 묶음.
pub struct AiProviderAdapters {
    pub ocr: Arc<dyn OcrProvider>,
    pub llm: Arc<dyn LlmProvider>,
    pub ocr_source: ProviderSource,
    pub llm_source: ProviderSource,
}

/// 앱 설정 기준으로 OCR/LLM 제공자 어댑터를 해석한다.
pub fn resolve_ai_provider_adapters(
    config: &AiProviderConfig,
) -> Result<AiProviderAdapters, CoreError> {
    let (ocr, ocr_source) = resolve_ocr_provider(config)?;
    let (llm, llm_source) = resolve_llm_provider(config)?;

    Ok(AiProviderAdapters {
        ocr,
        llm,
        ocr_source,
        llm_source,
    })
}

fn resolve_ocr_provider(
    config: &AiProviderConfig,
) -> Result<(Arc<dyn OcrProvider>, ProviderSource), CoreError> {
    match config.ocr_provider {
        OcrProviderType::Local => Ok((Arc::new(LocalOcrProvider::new()), ProviderSource::Local)),
        OcrProviderType::Remote => resolve_remote_with_optional_fallback(
            "ocr",
            config.fallback_to_local,
            || {
                let endpoint = require_endpoint_config(config.ocr_api.as_ref(), "ocr_api")?;
                Ok(Arc::new(RemoteOcrProvider::new(endpoint)?) as Arc<dyn OcrProvider>)
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
            "`{field_name}.endpoint`는 http:// 또는 https:// URL이어야 합니다."
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
                "원격 제공자 초기화 실패, 로컬 제공자로 폴백"
            );
            Ok((local_builder(), ProviderSource::LocalFallback))
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};

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
        let adapters = resolve_ai_provider_adapters(&config).expect("기본 설정 해석 실패");

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

        let adapters = resolve_ai_provider_adapters(&config).expect("원격 설정 해석 실패");

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

        let adapters =
            resolve_ai_provider_adapters(&config).expect("폴백 설정에서 해석 실패하면 안됨");

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

        match resolve_ai_provider_adapters(&config) {
            Ok(_) => panic!("오류가 발생해야 함"),
            Err(CoreError::Config(msg)) => assert!(msg.contains("ocr_api")),
            Err(other) => panic!("예상치 못한 에러 타입: {other}"),
        }
    }
}
