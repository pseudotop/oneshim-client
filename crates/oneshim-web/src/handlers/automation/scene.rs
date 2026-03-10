use axum::{
    extract::{Query, State},
    Json,
};

use oneshim_api_contracts::automation::{SceneCalibrationDto, SceneCalibrationQuery, SceneQuery};
use oneshim_core::error::CoreError;
use oneshim_core::models::ui_scene::UiScene;

use crate::{error::ApiError, AppState};

use super::helpers::{
    apply_scene_intelligence_filter, build_scene_calibration, infer_image_format,
    read_scene_intelligence_config, resolve_frame_image_path,
};

pub(super) async fn analyze_scene_by_query(
    state: &AppState,
    controller: &dyn oneshim_core::ports::automation::AutomationPort,
    frame_id: Option<i64>,
    app_name: Option<&str>,
    screen_id: Option<&str>,
) -> Result<UiScene, ApiError> {
    let analyze_result = if let Some(frame_id) = frame_id {
        let stored_path = state
            .storage
            .get_frame_file_path(frame_id)
            .map_err(|e| ApiError::Internal(format!("frame path query failure: {e}")))?
            .ok_or_else(|| ApiError::NotFound(format!("frame {frame_id} has no image")))?;

        let image_path = resolve_frame_image_path(state, &stored_path)?;
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

    match analyze_result {
        Ok(scene) => Ok(scene),
        Err(
            CoreError::PolicyDenied(msg)
            | CoreError::InvalidArguments(msg)
            | CoreError::ElementNotFound(msg),
        ) => Err(ApiError::BadRequest(msg)),
        Err(CoreError::Internal(msg))
            if msg.contains("Scene 분석기")
                || msg.contains("scene 분석을 지원하지")
                || msg.contains("이미지 직접 scene 분석") =>
        {
            Err(ApiError::BadRequest(msg))
        }
        Err(e) => Err(ApiError::Internal(format!("Scene analysis failed: {e}"))),
    }
}

pub async fn get_automation_scene(
    State(state): State<AppState>,
    Query(query): Query<SceneQuery>,
) -> Result<Json<UiScene>, ApiError> {
    let Some(ref controller) = state.automation_controller else {
        return Err(ApiError::BadRequest(
            "자동화 컨트롤러가 active화되지 않았습니다".to_string(),
        ));
    };

    let scene_cfg = read_scene_intelligence_config(&state);
    let scene = analyze_scene_by_query(
        &state,
        controller.as_ref(),
        query.frame_id,
        query.app_name.as_deref(),
        query.screen_id.as_deref(),
    )
    .await?;
    let filtered = apply_scene_intelligence_filter(scene, &scene_cfg)?;

    Ok(Json(filtered))
}

pub async fn get_automation_scene_calibration(
    State(state): State<AppState>,
    Query(query): Query<SceneCalibrationQuery>,
) -> Result<Json<SceneCalibrationDto>, ApiError> {
    let Some(ref controller) = state.automation_controller else {
        return Err(ApiError::BadRequest(
            "자동화 컨트롤러가 active화되지 않았습니다".to_string(),
        ));
    };

    let scene_cfg = read_scene_intelligence_config(&state);
    let scene = analyze_scene_by_query(
        &state,
        controller.as_ref(),
        query.frame_id,
        query.app_name.as_deref(),
        query.screen_id.as_deref(),
    )
    .await?;
    let filtered = apply_scene_intelligence_filter(scene, &scene_cfg)?;
    let report = build_scene_calibration(&filtered, &scene_cfg);
    Ok(Json(report))
}
