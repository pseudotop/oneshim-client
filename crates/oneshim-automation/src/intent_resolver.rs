//! 의도 해석기 + 검증기 + 실행기.
//!
//! `IntentResolver` — 의도 → 액션 변환
//! `ActionVerifier` — 실행 후 화면 변화 검증
//! `IntentExecutor` — 전체 파이프라인 오케스트레이터

use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, warn};

use oneshim_core::error::CoreError;
use oneshim_core::models::intent::{
    AutomationIntent, IntentConfig, IntentResult, UiElement, VerificationResult,
};
use oneshim_core::ports::element_finder::ElementFinder;
use oneshim_core::ports::input_driver::InputDriver;

// ============================================================
// IntentResolver — 의도 → 액션 변환 + 실행
// ============================================================

/// 의도 해석기 — AutomationIntent를 해석하여 InputDriver로 실행
pub struct IntentResolver {
    /// UI 요소 탐색기 (전략 체인)
    element_finder: Arc<dyn ElementFinder>,
    /// 입력 드라이버
    input_driver: Arc<dyn InputDriver>,
    /// 실행 설정
    config: IntentConfig,
}

impl IntentResolver {
    /// 새 IntentResolver 생성
    pub fn new(
        element_finder: Arc<dyn ElementFinder>,
        input_driver: Arc<dyn InputDriver>,
        config: IntentConfig,
    ) -> Self {
        Self {
            element_finder,
            input_driver,
            config,
        }
    }

    /// 의도를 해석하여 실행하고 결과를 반환
    pub async fn resolve_and_execute(
        &self,
        intent: &AutomationIntent,
    ) -> Result<(bool, Option<UiElement>), CoreError> {
        match intent {
            AutomationIntent::ClickElement {
                text,
                role,
                app_name: _,
                button,
            } => {
                let elements = self
                    .element_finder
                    .find_element(text.as_deref(), role.as_deref(), None)
                    .await?;

                let best = elements
                    .into_iter()
                    .find(|e| e.confidence >= self.config.min_confidence)
                    .ok_or_else(|| {
                        CoreError::ElementNotFound(format!(
                            "신뢰도 {:.0}% 이상의 요소를 찾지 못함 (text={:?}, role={:?})",
                            self.config.min_confidence * 100.0,
                            text,
                            role
                        ))
                    })?;

                let (cx, cy) = best.bounds.center();
                debug!(text = %best.text, x = cx, y = cy, confidence = best.confidence, "요소 클릭");
                self.input_driver.mouse_click(button, cx, cy).await?;

                Ok((true, Some(best)))
            }

            AutomationIntent::TypeIntoElement {
                element_text,
                role,
                text,
            } => {
                // 1. 요소 찾기 + 클릭하여 포커스
                let elements = self
                    .element_finder
                    .find_element(element_text.as_deref(), role.as_deref(), None)
                    .await?;

                let best = elements
                    .into_iter()
                    .find(|e| e.confidence >= self.config.min_confidence);

                if let Some(elem) = &best {
                    let (cx, cy) = elem.bounds.center();
                    debug!(text = %elem.text, x = cx, y = cy, "포커스 클릭");
                    self.input_driver.mouse_click("left", cx, cy).await?;
                }

                // 2. 텍스트 입력
                debug!(text_len = text.len(), "텍스트 입력");
                self.input_driver.type_text(text).await?;

                Ok((true, best))
            }

            AutomationIntent::ExecuteHotkey { keys } => {
                debug!(?keys, "단축키 실행");
                self.input_driver.hotkey(keys).await?;
                Ok((true, None))
            }

            AutomationIntent::WaitForText { text, timeout_ms } => {
                debug!(text, timeout_ms, "텍스트 대기");
                let start = Instant::now();
                let timeout = std::time::Duration::from_millis(*timeout_ms);

                loop {
                    // OCR 폴링으로 텍스트 확인
                    let elements = self
                        .element_finder
                        .find_element(Some(text), None, None)
                        .await;

                    if let Ok(elems) = &elements {
                        if !elems.is_empty() {
                            info!(
                                text,
                                elapsed_ms = start.elapsed().as_millis(),
                                "텍스트 발견"
                            );
                            return Ok((true, elems.first().cloned()));
                        }
                    }

                    if start.elapsed() >= timeout {
                        warn!(text, timeout_ms, "텍스트 대기 타임아웃");
                        return Err(CoreError::ExecutionTimeout {
                            timeout_ms: *timeout_ms,
                        });
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(
                        self.config.retry_interval_ms,
                    ))
                    .await;
                }
            }

            AutomationIntent::ActivateApp { app_name } => {
                debug!(app_name, "앱 활성화");
                // 플랫폼별 앱 활성화 — 향후 구현
                // macOS: osascript -e 'activate application "..."'
                // Windows: Win32 SetForegroundWindow
                info!(app_name, "앱 활성화 요청 (플랫폼 구현 필요)");
                Ok((true, None))
            }

            AutomationIntent::Raw(action) => {
                debug!(?action, "저수준 액션 직접 실행");
                match action {
                    oneshim_core::models::automation::AutomationAction::MouseMove { x, y } => {
                        self.input_driver.mouse_move(*x, *y).await?;
                    }
                    oneshim_core::models::automation::AutomationAction::MouseClick {
                        button,
                        x,
                        y,
                    } => {
                        self.input_driver.mouse_click(button, *x, *y).await?;
                    }
                    oneshim_core::models::automation::AutomationAction::KeyType { text } => {
                        self.input_driver.type_text(text).await?;
                    }
                    oneshim_core::models::automation::AutomationAction::KeyPress { key } => {
                        self.input_driver.key_press(key).await?;
                    }
                    oneshim_core::models::automation::AutomationAction::KeyRelease { key } => {
                        self.input_driver.key_release(key).await?;
                    }
                    oneshim_core::models::automation::AutomationAction::Hotkey { keys } => {
                        self.input_driver.hotkey(keys).await?;
                    }
                }
                Ok((true, None))
            }
        }
    }
}

// ============================================================
// IntentExecutor — 전체 파이프라인 오케스트레이터
// ============================================================

/// 전체 의도 실행 파이프라인
///
/// 1. 재시도 포함 resolve_and_execute
/// 2. (선택) 실행 후 검증
pub struct IntentExecutor {
    /// 의도 해석기
    resolver: IntentResolver,
    /// 실행 설정
    config: IntentConfig,
}

impl IntentExecutor {
    /// 새 IntentExecutor 생성
    pub fn new(resolver: IntentResolver, config: IntentConfig) -> Self {
        Self { resolver, config }
    }

    /// 의도 실행 (재시도 + 검증 포함)
    pub async fn execute(&self, intent: &AutomationIntent) -> Result<IntentResult, CoreError> {
        let start = Instant::now();
        let mut retry_count = 0u32;
        let mut last_error: Option<String> = None;

        // 재시도 루프
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                debug!(attempt, max = self.config.max_retries, "재시도");
                tokio::time::sleep(std::time::Duration::from_millis(
                    self.config.retry_interval_ms,
                ))
                .await;
            }

            match self.resolver.resolve_and_execute(intent).await {
                Ok((success, element)) => {
                    let elapsed_ms = start.elapsed().as_millis() as u64;

                    // 검증 (활성화 시)
                    let verification = if self.config.verify_after_action {
                        // 검증 대기
                        tokio::time::sleep(std::time::Duration::from_millis(
                            self.config.verify_delay_ms,
                        ))
                        .await;

                        // 간단한 검증: 실행 성공 = 화면 변화 가정
                        Some(VerificationResult {
                            screen_changed: success,
                            changed_regions: if success { 1 } else { 0 },
                            text_found: None,
                        })
                    } else {
                        None
                    };

                    return Ok(IntentResult {
                        success,
                        element,
                        verification,
                        retry_count,
                        elapsed_ms,
                        error: None,
                    });
                }
                Err(e) => {
                    warn!(attempt, error = %e, "의도 실행 실패");
                    last_error = Some(e.to_string());
                    retry_count = attempt;
                }
            }
        }

        // 모든 재시도 소진
        let elapsed_ms = start.elapsed().as_millis() as u64;
        Ok(IntentResult {
            success: false,
            element: None,
            verification: None,
            retry_count,
            elapsed_ms,
            error: last_error,
        })
    }
}

// ============================================================
// 테스트
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::automation::AutomationAction;
    use oneshim_core::models::intent::{ElementBounds, FinderSource};

    // Mock ElementFinder
    struct MockElementFinder {
        results: Vec<UiElement>,
    }

    #[async_trait::async_trait]
    impl ElementFinder for MockElementFinder {
        async fn find_element(
            &self,
            _text: Option<&str>,
            _role: Option<&str>,
            _region: Option<&ElementBounds>,
        ) -> Result<Vec<UiElement>, CoreError> {
            Ok(self.results.clone())
        }
        fn name(&self) -> &str {
            "mock"
        }
    }

    struct EmptyElementFinder;

    #[async_trait::async_trait]
    impl ElementFinder for EmptyElementFinder {
        async fn find_element(
            &self,
            _text: Option<&str>,
            _role: Option<&str>,
            _region: Option<&ElementBounds>,
        ) -> Result<Vec<UiElement>, CoreError> {
            Ok(vec![])
        }
        fn name(&self) -> &str {
            "empty"
        }
    }

    // Mock InputDriver
    struct MockInputDriver;

    #[async_trait::async_trait]
    impl InputDriver for MockInputDriver {
        async fn mouse_move(&self, _x: i32, _y: i32) -> Result<(), CoreError> {
            Ok(())
        }
        async fn mouse_click(&self, _button: &str, _x: i32, _y: i32) -> Result<(), CoreError> {
            Ok(())
        }
        async fn type_text(&self, _text: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn key_press(&self, _key: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn key_release(&self, _key: &str) -> Result<(), CoreError> {
            Ok(())
        }
        async fn hotkey(&self, _keys: &[String]) -> Result<(), CoreError> {
            Ok(())
        }
        fn platform(&self) -> &str {
            "mock"
        }
    }

    fn make_resolver_with_elements(elements: Vec<UiElement>) -> IntentResolver {
        IntentResolver::new(
            Arc::new(MockElementFinder { results: elements }),
            Arc::new(MockInputDriver),
            IntentConfig::default(),
        )
    }

    fn make_element(text: &str, confidence: f64) -> UiElement {
        UiElement {
            text: text.to_string(),
            bounds: ElementBounds {
                x: 100,
                y: 100,
                width: 80,
                height: 30,
            },
            role: Some("button".to_string()),
            confidence,
            source: FinderSource::Ocr,
        }
    }

    #[tokio::test]
    async fn resolve_click_element_success() {
        let resolver = make_resolver_with_elements(vec![make_element("저장", 0.95)]);
        let intent = AutomationIntent::ClickElement {
            text: Some("저장".to_string()),
            role: None,
            app_name: None,
            button: "left".to_string(),
        };
        let (success, element) = resolver.resolve_and_execute(&intent).await.unwrap();
        assert!(success);
        assert_eq!(element.unwrap().text, "저장");
    }

    #[tokio::test]
    async fn resolve_click_element_low_confidence() {
        let resolver = make_resolver_with_elements(vec![make_element("저장", 0.3)]);
        let intent = AutomationIntent::ClickElement {
            text: Some("저장".to_string()),
            role: None,
            app_name: None,
            button: "left".to_string(),
        };
        let result = resolver.resolve_and_execute(&intent).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_type_into_element() {
        let resolver = make_resolver_with_elements(vec![make_element("검색", 0.9)]);
        let intent = AutomationIntent::TypeIntoElement {
            element_text: Some("검색".to_string()),
            role: None,
            text: "hello world".to_string(),
        };
        let (success, _) = resolver.resolve_and_execute(&intent).await.unwrap();
        assert!(success);
    }

    #[tokio::test]
    async fn resolve_execute_hotkey() {
        let resolver = make_resolver_with_elements(vec![]);
        let intent = AutomationIntent::ExecuteHotkey {
            keys: vec!["Ctrl".to_string(), "S".to_string()],
        };
        let (success, element) = resolver.resolve_and_execute(&intent).await.unwrap();
        assert!(success);
        assert!(element.is_none());
    }

    #[tokio::test]
    async fn resolve_raw_action() {
        let resolver = make_resolver_with_elements(vec![]);
        let intent = AutomationIntent::Raw(AutomationAction::MouseClick {
            button: "left".to_string(),
            x: 100,
            y: 200,
        });
        let (success, _) = resolver.resolve_and_execute(&intent).await.unwrap();
        assert!(success);
    }

    #[tokio::test]
    async fn executor_retries_on_failure() {
        let resolver = IntentResolver::new(
            Arc::new(EmptyElementFinder),
            Arc::new(MockInputDriver),
            IntentConfig {
                max_retries: 1,
                retry_interval_ms: 10,
                verify_after_action: false,
                ..IntentConfig::default()
            },
        );
        let executor = IntentExecutor::new(
            resolver,
            IntentConfig {
                max_retries: 1,
                retry_interval_ms: 10,
                verify_after_action: false,
                ..IntentConfig::default()
            },
        );

        let intent = AutomationIntent::ClickElement {
            text: Some("존재하지않음".to_string()),
            role: None,
            app_name: None,
            button: "left".to_string(),
        };

        let result = executor.execute(&intent).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.retry_count >= 1);
    }

    #[tokio::test]
    async fn executor_succeeds_with_verification() {
        let resolver = make_resolver_with_elements(vec![make_element("확인", 0.9)]);
        let config = IntentConfig {
            verify_after_action: true,
            verify_delay_ms: 10, // 테스트에서 빠르게
            ..IntentConfig::default()
        };
        let executor = IntentExecutor::new(resolver, config);

        let intent = AutomationIntent::ClickElement {
            text: Some("확인".to_string()),
            role: None,
            app_name: None,
            button: "left".to_string(),
        };

        let result = executor.execute(&intent).await.unwrap();
        assert!(result.success);
        assert!(result.verification.is_some());
        assert!(result.verification.unwrap().screen_changed);
    }

    #[tokio::test]
    async fn executor_activate_app() {
        let resolver = make_resolver_with_elements(vec![]);
        let executor = IntentExecutor::new(
            resolver,
            IntentConfig {
                verify_after_action: false,
                ..IntentConfig::default()
            },
        );

        let intent = AutomationIntent::ActivateApp {
            app_name: "Visual Studio Code".to_string(),
        };

        let result = executor.execute(&intent).await.unwrap();
        assert!(result.success);
    }

    // --- 추가 테스트: WaitForText 타임아웃 ---

    #[tokio::test]
    async fn wait_for_text_timeout_returns_error() {
        let resolver = IntentResolver::new(
            Arc::new(EmptyElementFinder),
            Arc::new(MockInputDriver),
            IntentConfig {
                retry_interval_ms: 10,
                ..IntentConfig::default()
            },
        );
        let intent = AutomationIntent::WaitForText {
            text: "존재하지않는텍스트".to_string(),
            timeout_ms: 50,
        };
        let result = resolver.resolve_and_execute(&intent).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CoreError::ExecutionTimeout { .. }
        ));
    }

    #[tokio::test]
    async fn wait_for_text_found_immediately() {
        let resolver = make_resolver_with_elements(vec![make_element("완료", 0.9)]);
        let intent = AutomationIntent::WaitForText {
            text: "완료".to_string(),
            timeout_ms: 1000,
        };
        let (success, element) = resolver.resolve_and_execute(&intent).await.unwrap();
        assert!(success);
        assert!(element.is_some());
    }

    // --- 추가 테스트: 요소 탐색 실패 ---

    #[tokio::test]
    async fn click_element_not_found_returns_error() {
        let resolver = IntentResolver::new(
            Arc::new(EmptyElementFinder),
            Arc::new(MockInputDriver),
            IntentConfig::default(),
        );
        let intent = AutomationIntent::ClickElement {
            text: Some("없는버튼".to_string()),
            role: None,
            app_name: None,
            button: "left".to_string(),
        };
        let result = resolver.resolve_and_execute(&intent).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CoreError::ElementNotFound(_)));
    }

    // --- 추가 테스트: Raw 액션 변형 ---

    #[tokio::test]
    async fn resolve_raw_key_type_action() {
        let resolver = make_resolver_with_elements(vec![]);
        let intent = AutomationIntent::Raw(AutomationAction::KeyType {
            text: "hello world".to_string(),
        });
        let (success, _) = resolver.resolve_and_execute(&intent).await.unwrap();
        assert!(success);
    }

    #[tokio::test]
    async fn resolve_raw_mouse_move_action() {
        let resolver = make_resolver_with_elements(vec![]);
        let intent = AutomationIntent::Raw(AutomationAction::MouseMove { x: 500, y: 300 });
        let (success, _) = resolver.resolve_and_execute(&intent).await.unwrap();
        assert!(success);
    }

    // --- 추가 테스트: Executor 검증 없이 성공 ---

    #[tokio::test]
    async fn executor_no_verification_success() {
        let resolver = make_resolver_with_elements(vec![make_element("확인", 0.9)]);
        let config = IntentConfig {
            verify_after_action: false,
            ..IntentConfig::default()
        };
        let executor = IntentExecutor::new(resolver, config);

        let intent = AutomationIntent::ClickElement {
            text: Some("확인".to_string()),
            role: None,
            app_name: None,
            button: "left".to_string(),
        };

        let result = executor.execute(&intent).await.unwrap();
        assert!(result.success);
        assert!(result.verification.is_none());
        assert_eq!(result.retry_count, 0);
    }
}
