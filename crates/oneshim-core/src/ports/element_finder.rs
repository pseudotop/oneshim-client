//! UI 요소 탐색기 포트 (전략 패턴).
//!
//! OCR, 접근성 API, 템플릿 매칭 등 다양한 전략으로
//! 화면에서 UI 요소를 탐색하는 인터페이스를 정의한다.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::intent::{ElementBounds, UiElement};

/// UI 요소 탐색기 — 화면에서 UI 요소를 찾는 전략 인터페이스
///
/// 구현체: `OcrElementFinder`, `AccessibilityFinder`, `TemplateMatcherFinder`
#[async_trait]
pub trait ElementFinder: Send + Sync {
    /// 텍스트/역할 기반으로 화면에서 UI 요소 탐색
    ///
    /// - `text`: 검색 대상 텍스트 (None이면 역할로만 검색)
    /// - `role`: 대상 역할 (button, input 등)
    /// - `region`: 검색 영역 제한 (None이면 전체 화면)
    async fn find_element(
        &self,
        text: Option<&str>,
        role: Option<&str>,
        region: Option<&ElementBounds>,
    ) -> Result<Vec<UiElement>, CoreError>;

    /// 탐색기 이름 (예: "ocr", "accessibility", "template")
    fn name(&self) -> &str;
}
