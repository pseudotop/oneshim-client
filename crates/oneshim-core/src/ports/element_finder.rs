//!

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::intent::{ElementBounds, UiElement};
use crate::models::ui_scene::UiScene;

///
#[async_trait]
pub trait ElementFinder: Send + Sync {
    ///
    async fn find_element(
        &self,
        text: Option<&str>,
        role: Option<&str>,
        region: Option<&ElementBounds>,
    ) -> Result<Vec<UiElement>, CoreError>;

    ///
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

    ///
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

    fn name(&self) -> &str;
}
