//! UI 요소 탐색기 포트 (전략 패턴).
//!
//! OCR, 접근성 API, 템플릿 매칭 등 다양한 전략으로
//! 화면에서 UI 요소를 탐색하는 인터페이스를 정의한다.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::intent::{ElementBounds, UiElement};
use crate::models::ui_scene::UiScene;

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

    /// 현재 화면 기준 구조화된 UI Scene 분석.
    ///
    /// 기본 구현은 미지원 에러를 반환한다.
    /// Scene 분석을 제공하는 구현체는 override 해야 한다.
    async fn analyze_scene(
        &self,
        _app_name: Option<&str>,
        _screen_id: Option<&str>,
    ) -> Result<UiScene, CoreError> {
        Err(CoreError::Internal(format!(
            "ElementFinder '{}'는 scene 분석을 지원하지 않습니다",
            self.name()
        )))
    }

    /// 주어진 이미지로 구조화된 UI Scene 분석.
    ///
    /// 기본 구현은 미지원 에러를 반환한다.
    async fn analyze_scene_from_image(
        &self,
        _image_data: Vec<u8>,
        _image_format: String,
        _app_name: Option<&str>,
        _screen_id: Option<&str>,
    ) -> Result<UiScene, CoreError> {
        Err(CoreError::Internal(format!(
            "ElementFinder '{}'는 이미지 직접 scene 분석을 지원하지 않습니다",
            self.name()
        )))
    }

    /// 탐색기 이름 (예: "ocr", "accessibility", "template")
    fn name(&self) -> &str;
}
