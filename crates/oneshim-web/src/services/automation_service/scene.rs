use oneshim_api_contracts::automation::{SceneCalibrationDto, SceneCalibrationQuery, SceneQuery};
use oneshim_core::error::CoreError;
use oneshim_core::models::ui_scene::UiScene;

use crate::error::ApiError;
use crate::services::web_contexts::AutomationWebContext;

use super::helpers::{
    apply_scene_intelligence_filter, build_scene_calibration, infer_image_format,
    read_scene_intelligence_config, resolve_frame_image_path,
};

#[derive(Clone)]
pub struct AutomationSceneQueryService {
    ctx: AutomationWebContext,
}

impl AutomationSceneQueryService {
    pub fn new(ctx: AutomationWebContext) -> Self {
        Self { ctx }
    }

    async fn analyze_scene_by_query(
        &self,
        controller: &dyn oneshim_core::ports::automation::AutomationPort,
        frame_id: Option<i64>,
        app_name: Option<&str>,
        screen_id: Option<&str>,
    ) -> Result<UiScene, ApiError> {
        let analyze_result = if let Some(frame_id) = frame_id {
            let stored_path = self
                .ctx
                .storage
                .get_frame_file_path(frame_id)
                .map_err(|e| ApiError::Internal(format!("frame path query failure: {e}")))?
                .ok_or_else(|| ApiError::NotFound(format!("frame {frame_id} has no image")))?;

            let image_path = resolve_frame_image_path(&self.ctx, &stored_path)?;
            let image_data = std::fs::read(&image_path)
                .map_err(|e| ApiError::Internal(format!("Failed to read frame image: {e}")))?;

            controller
                .analyze_scene_from_image(
                    image_data,
                    infer_image_format(&image_path),
                    app_name,
                    screen_id,
                )
                .await
        } else {
            controller.analyze_scene(app_name, screen_id).await
        };

        #[allow(deprecated)]
        match analyze_result {
            Ok(scene) => Ok(scene),
            // V2 variants
            Err(
                CoreError::PolicyDeniedV2 { message: msg, .. }
                | CoreError::InvalidArgumentsV2 { message: msg, .. },
            ) => Err(ApiError::BadRequest(msg)),
            Err(CoreError::ElementNotFoundV2 { name: msg, .. }) => Err(ApiError::BadRequest(msg)),
            // V1 deprecated variants
            Err(
                CoreError::PolicyDenied(msg)
                | CoreError::InvalidArguments(msg)
                | CoreError::ElementNotFound(msg),
            ) => Err(ApiError::BadRequest(msg)),
            Err(CoreError::InternalV2 {
                code: _,
                message: msg,
            }) if msg.contains("Scene analyzer")
                || msg.contains("scene analysis is not supported")
                || msg.contains("direct image scene analysis") =>
            {
                Err(ApiError::BadRequest(msg))
            }
            Err(e) => Err(ApiError::Internal(format!("Scene analysis failed: {e}"))),
        }
    }

    pub async fn get_scene(&self, query: SceneQuery) -> Result<UiScene, ApiError> {
        let Some(ref controller) = self.ctx.automation_controller else {
            return Err(ApiError::BadRequest(
                "Automation controller is not active.".to_string(),
            ));
        };

        let scene_cfg = read_scene_intelligence_config(&self.ctx);
        let scene = self
            .analyze_scene_by_query(
                controller.as_ref(),
                query.frame_id,
                query.app_name.as_deref(),
                query.screen_id.as_deref(),
            )
            .await?;
        apply_scene_intelligence_filter(scene, &scene_cfg)
    }

    pub async fn get_scene_calibration(
        &self,
        query: SceneCalibrationQuery,
    ) -> Result<SceneCalibrationDto, ApiError> {
        let Some(ref controller) = self.ctx.automation_controller else {
            return Err(ApiError::BadRequest(
                "Automation controller is not active.".to_string(),
            ));
        };

        let scene_cfg = read_scene_intelligence_config(&self.ctx);
        let scene = self
            .analyze_scene_by_query(
                controller.as_ref(),
                query.frame_id,
                query.app_name.as_deref(),
                query.screen_id.as_deref(),
            )
            .await?;
        let filtered = apply_scene_intelligence_filter(scene, &scene_cfg)?;
        Ok(build_scene_calibration(&filtered, &scene_cfg))
    }
}
