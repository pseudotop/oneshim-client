
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct TagResponse {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTagRequest {
    pub name: String,
    pub color: String,
}

///
/// GET /api/tags
pub async fn list_tags(State(state): State<AppState>) -> Result<Json<Vec<TagResponse>>, ApiError> {
    let tags = state.storage.get_all_tags()?;

    let response: Vec<TagResponse> = tags
        .into_iter()
        .map(|t| TagResponse {
            id: t.id,
            name: t.name,
            color: t.color,
            created_at: t.created_at,
        })
        .collect();

    Ok(Json(response))
}

///
/// POST /api/tags
pub async fn create_tag(
    State(state): State<AppState>,
    Json(req): Json<CreateTagRequest>,
) -> Result<Json<TagResponse>, ApiError> {
    let color = req.color.unwrap_or_else(|| "#3b82f6".to_string());

    let tag = state.storage.create_tag(&req.name, &color)?;

    Ok(Json(TagResponse {
        id: tag.id,
        name: tag.name,
        color: tag.color,
        created_at: tag.created_at,
    }))
}

///
/// GET /api/tags/:id
pub async fn get_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
) -> Result<Json<TagResponse>, ApiError> {
    let tag = state
        .storage
        .get_tag(tag_id)?
        .ok_or_else(|| ApiError::NotFound(format!("태그 ID: {tag_id}")))?;

    Ok(Json(TagResponse {
        id: tag.id,
        name: tag.name,
        color: tag.color,
        created_at: tag.created_at,
    }))
}

///
/// PUT /api/tags/:id
pub async fn update_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
    Json(req): Json<UpdateTagRequest>,
) -> Result<Json<TagResponse>, ApiError> {
    let updated = state.storage.update_tag(tag_id, &req.name, &req.color)?;

    if !updated {
        return Err(ApiError::NotFound(format!("태그 ID: {tag_id}")));
    }

    let tag = state
        .storage
        .get_tag(tag_id)?
        .ok_or_else(|| ApiError::NotFound(format!("태그 ID: {tag_id}")))?;

    Ok(Json(TagResponse {
        id: tag.id,
        name: tag.name,
        color: tag.color,
        created_at: tag.created_at,
    }))
}

///
/// DELETE /api/tags/:id
pub async fn delete_tag(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state.storage.delete_tag(tag_id)?;

    if !deleted {
        return Err(ApiError::NotFound(format!("태그 ID: {tag_id}")));
    }

    Ok(Json(
        serde_json::json!({ "message": "태그가 delete되었습니다" }),
    ))
}

///
/// GET /api/frames/:frame_id/tags
pub async fn get_frame_tags(
    State(state): State<AppState>,
    Path(frame_id): Path<i64>,
) -> Result<Json<Vec<TagResponse>>, ApiError> {
    let tags = state.storage.get_tags_for_frame(frame_id)?;

    let response: Vec<TagResponse> = tags
        .into_iter()
        .map(|t| TagResponse {
            id: t.id,
            name: t.name,
            color: t.color,
            created_at: t.created_at,
        })
        .collect();

    Ok(Json(response))
}

///
/// POST /api/frames/:frame_id/tags/:tag_id
pub async fn add_tag_to_frame(
    State(state): State<AppState>,
    Path((frame_id, tag_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.storage.add_tag_to_frame(frame_id, tag_id)?;

    Ok(Json(
        serde_json::json!({ "message": "태그가 add되었습니다" }),
    ))
}

///
/// DELETE /api/frames/:frame_id/tags/:tag_id
pub async fn remove_tag_from_frame(
    State(state): State<AppState>,
    Path((frame_id, tag_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = state.storage.remove_tag_from_frame(frame_id, tag_id)?;

    if !removed {
        return Err(ApiError::NotFound(format!(
            "frame {frame_id}에 태그 {tag_id}가 none"
        )));
    }

    Ok(Json(
        serde_json::json!({ "message": "태그가 제거되었습니다" }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_response_serializes() {
        let tag = TagResponse {
            id: 1,
            name: "important".to_string(),
            color: "#ef4444".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&tag).unwrap();
        assert!(json.contains("important"));
        assert!(json.contains("#ef4444"));
    }

    #[test]
    fn create_tag_request_deserializes() {
        let json = r##"{"name": "work", "color": "#3b82f6"}"##;
        let req: CreateTagRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "work");
        assert_eq!(req.color, Some("#3b82f6".to_string()));
    }

    #[test]
    fn create_tag_request_without_color() {
        let json = r##"{"name": "work"}"##;
        let req: CreateTagRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "work");
        assert!(req.color.is_none());
    }
}
