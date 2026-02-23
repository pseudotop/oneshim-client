//! 자동화 런타임 와이어링.
//!
//! AI 제공자 설정 + 최신 프레임 소스를 이용해 IntentExecutor를 구성한다.

use async_trait::async_trait;
use oneshim_automation::input_driver::{NoOpElementFinder, NoOpInputDriver};
use oneshim_automation::intent_planner::{IntentPlanner, LlmIntentPlanner};
use oneshim_automation::intent_resolver::{IntentExecutor, IntentResolver};
use oneshim_core::config::AiProviderConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::intent::{ElementBounds, IntentConfig, UiElement};
use oneshim_core::ports::element_finder::ElementFinder;
use oneshim_core::ports::input_driver::InputDriver;
use oneshim_storage::frame_storage::FrameFileStorage;
use oneshim_vision::element_finder::OcrElementFinder;
use std::sync::Arc;
use tracing::warn;

use crate::provider_adapters::{resolve_ai_provider_adapters, ProviderSource};

/// 자동화 실행기에 필요한 런타임 구성요소.
pub struct AutomationRuntime {
    pub intent_executor: Arc<IntentExecutor>,
    pub intent_planner: Arc<dyn IntentPlanner>,
    pub ocr_provider_name: String,
    pub llm_provider_name: String,
    pub ocr_source: ProviderSource,
    pub llm_source: ProviderSource,
}

/// AI 제공자 + 최신 프레임 기반 자동화 런타임 생성.
pub fn build_automation_runtime(
    ai_config: &AiProviderConfig,
    frame_storage: Option<Arc<FrameFileStorage>>,
) -> Result<AutomationRuntime, CoreError> {
    let adapters = resolve_ai_provider_adapters(ai_config)?;

    let ocr_provider_name = adapters.ocr.provider_name().to_string();
    let llm_provider_name = adapters.llm.provider_name().to_string();

    let element_finder: Arc<dyn ElementFinder> = if let Some(frame_storage) = frame_storage {
        Arc::new(LatestFrameOcrElementFinder::new(
            frame_storage,
            adapters.ocr.clone(),
        ))
    } else {
        warn!("프레임 저장소 미설정: NoOpElementFinder로 폴백");
        Arc::new(NoOpElementFinder)
    };

    let input_driver: Arc<dyn InputDriver> = Arc::new(NoOpInputDriver);
    let resolver = IntentResolver::new(
        element_finder.clone(),
        input_driver,
        IntentConfig::default(),
    );
    let intent_executor = Arc::new(IntentExecutor::new(resolver, IntentConfig::default()));
    let intent_planner: Arc<dyn IntentPlanner> = Arc::new(LlmIntentPlanner::new(
        adapters.llm.clone(),
        element_finder.clone(),
    ));

    Ok(AutomationRuntime {
        intent_executor,
        intent_planner,
        ocr_provider_name,
        llm_provider_name,
        ocr_source: adapters.ocr_source,
        llm_source: adapters.llm_source,
    })
}

/// 안전 폴백용 NoOp 실행기 생성.
pub fn build_noop_intent_executor() -> Arc<IntentExecutor> {
    let input_driver: Arc<dyn InputDriver> = Arc::new(NoOpInputDriver);
    let element_finder: Arc<dyn ElementFinder> = Arc::new(NoOpElementFinder);
    let resolver = IntentResolver::new(element_finder, input_driver, IntentConfig::default());
    Arc::new(IntentExecutor::new(resolver, IntentConfig::default()))
}

/// 최신 저장 프레임을 자동으로 로드해 OCR 기반 탐색을 수행하는 ElementFinder.
pub struct LatestFrameOcrElementFinder {
    frame_storage: Arc<FrameFileStorage>,
    inner: OcrElementFinder,
}

impl LatestFrameOcrElementFinder {
    pub fn new(
        frame_storage: Arc<FrameFileStorage>,
        ocr_provider: Arc<dyn oneshim_core::ports::ocr_provider::OcrProvider>,
    ) -> Self {
        Self {
            frame_storage,
            inner: OcrElementFinder::new(ocr_provider),
        }
    }

    async fn refresh_latest_frame(&self) -> Result<bool, CoreError> {
        match self.frame_storage.load_latest_frame().await? {
            Some((image_data, image_format)) => {
                self.inner.set_image(image_data, image_format).await;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[async_trait]
impl ElementFinder for LatestFrameOcrElementFinder {
    async fn find_element(
        &self,
        text: Option<&str>,
        role: Option<&str>,
        region: Option<&ElementBounds>,
    ) -> Result<Vec<UiElement>, CoreError> {
        if !self.refresh_latest_frame().await? {
            return Err(CoreError::ElementNotFound(
                "자동화용 최신 프레임이 없습니다".to_string(),
            ));
        }
        self.inner.find_element(text, role, region).await
    }

    fn name(&self) -> &str {
        "latest-frame-ocr"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use oneshim_core::config::{
        AiProviderConfig, AiProviderType, ExternalApiEndpoint, LlmProviderType, OcrProviderType,
    };
    use oneshim_core::ports::ocr_provider::{OcrProvider, OcrResult};
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct FakeOcrProvider;

    #[async_trait]
    impl OcrProvider for FakeOcrProvider {
        async fn extract_elements(
            &self,
            image: &[u8],
            image_format: &str,
        ) -> Result<Vec<OcrResult>, CoreError> {
            if image.is_empty() {
                return Ok(vec![]);
            }

            if image_format != "webp" {
                return Err(CoreError::OcrError(format!(
                    "예상치 못한 포맷: {image_format}"
                )));
            }

            Ok(vec![OcrResult {
                text: "저장".to_string(),
                x: 100,
                y: 100,
                width: 60,
                height: 24,
                confidence: 0.9,
            }])
        }

        fn provider_name(&self) -> &str {
            "fake-ocr"
        }

        fn is_external(&self) -> bool {
            false
        }
    }

    async fn create_test_storage(base_dir: PathBuf) -> FrameFileStorage {
        FrameFileStorage::new(base_dir, 100, 7)
            .await
            .expect("테스트 프레임 저장소 생성 실패")
    }

    #[tokio::test]
    async fn latest_frame_finder_reads_frame_and_matches_text() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(create_test_storage(temp_dir.path().to_path_buf()).await);
        storage
            .save_frame(Utc::now(), b"fake-webp-binary")
            .await
            .unwrap();

        let finder = LatestFrameOcrElementFinder::new(storage, Arc::new(FakeOcrProvider));
        let result = finder
            .find_element(Some("저장"), Some("button"), None)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "저장");
    }

    #[tokio::test]
    async fn latest_frame_finder_returns_not_found_when_no_frame_exists() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(create_test_storage(temp_dir.path().to_path_buf()).await);
        let finder = LatestFrameOcrElementFinder::new(storage, Arc::new(FakeOcrProvider));

        let err = finder
            .find_element(Some("저장"), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::ElementNotFound(_)));
    }

    #[test]
    fn build_runtime_falls_back_when_remote_config_is_missing() {
        let config = AiProviderConfig {
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Remote,
            ocr_api: None,
            llm_api: None,
            fallback_to_local: true,
            ..AiProviderConfig::default()
        };

        let runtime = build_automation_runtime(&config, None).unwrap();
        assert_eq!(runtime.ocr_source, ProviderSource::LocalFallback);
        assert_eq!(runtime.llm_source, ProviderSource::LocalFallback);
    }

    #[test]
    fn build_runtime_errors_when_remote_config_missing_and_fallback_disabled() {
        let config = AiProviderConfig {
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Local,
            ocr_api: None,
            llm_api: None,
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        match build_automation_runtime(&config, None) {
            Ok(_) => panic!("오류가 발생해야 함"),
            Err(err) => assert!(matches!(err, CoreError::Config(_))),
        }
    }

    #[test]
    fn build_runtime_uses_remote_sources_when_endpoints_are_valid() {
        let endpoint = ExternalApiEndpoint {
            endpoint: "https://api.example.com/v1".to_string(),
            api_key: "test-key".to_string(),
            model: Some("model-test".to_string()),
            timeout_secs: 30,
            provider_type: AiProviderType::Generic,
        };
        let config = AiProviderConfig {
            ocr_provider: OcrProviderType::Remote,
            llm_provider: LlmProviderType::Remote,
            ocr_api: Some(endpoint.clone()),
            llm_api: Some(endpoint),
            fallback_to_local: false,
            ..AiProviderConfig::default()
        };

        let runtime = build_automation_runtime(&config, None).unwrap();
        assert_eq!(runtime.ocr_source, ProviderSource::Remote);
        assert_eq!(runtime.llm_source, ProviderSource::Remote);
        assert_eq!(runtime.ocr_provider_name, "remote-ocr");
    }
}
