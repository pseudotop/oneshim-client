use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::update_control::{UpdateAction, UpdateStatus};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateActionRequest {
    pub action: UpdateAction,
}

#[derive(Debug, Serialize)]
pub struct UpdateActionResponse {
    pub accepted: bool,
    pub status: UpdateStatus,
}

pub async fn get_update_status(
    State(state): State<AppState>,
) -> Result<Json<UpdateStatus>, ApiError> {
    let Some(control) = state.update_control else {
        return Err(ApiError::NotFound(
            "업데이트 제어가 활성화되지 않았습니다".to_string(),
        ));
    };

    let snapshot = control.state.read().await.clone();
    Ok(Json(snapshot))
}

pub async fn post_update_action(
    State(state): State<AppState>,
    Json(body): Json<UpdateActionRequest>,
) -> Result<Json<UpdateActionResponse>, ApiError> {
    let Some(control) = state.update_control else {
        return Err(ApiError::NotFound(
            "업데이트 제어가 활성화되지 않았습니다".to_string(),
        ));
    };

    control
        .action_tx
        .send(body.action)
        .map_err(|e| ApiError::Internal(format!("업데이트 액션 전달 실패: {}", e)))?;

    let snapshot = control.state.read().await.clone();
    Ok(Json(UpdateActionResponse {
        accepted: true,
        status: snapshot,
    }))
}
