
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::error::ApiError;
use crate::AppState;

use super::{PaginatedResponse, PaginationMeta, TimeRangeQuery};

#[derive(Debug, Serialize)]
pub struct FrameResponse {
    pub id: i64,
    pub timestamp: String,
    pub trigger_type: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
    pub resolution: String,
    pub file_path: Option<String>,
    pub ocr_text: Option<String>,
    pub image_url: Option<String>,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
}

///
/// GET /api/frames?from=&to=&limit=&offset=
pub async fn get_frames(
    State(state): State<AppState>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<PaginatedResponse<FrameResponse>>, ApiError> {
    let from = params.from_datetime();
    let to = params.to_datetime();
    let limit = params.limit_or_default();
    let offset = params.offset_or_default();

    let total = state
        .storage
        .count_frames_in_range(&from.to_rfc3339(), &to.to_rfc3339())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let fetch_limit = limit + offset;
    let frames = state.storage.get_frames(from, to, fetch_limit)?;

    let data: Vec<FrameResponse> = frames
        .into_iter()
        .skip(offset)
        .map(|f| {
            let image_url = f
                .file_path
                .as_ref()
                .map(|_| format!("/api/frames/{}/image", f.id));
            FrameResponse {
                id: f.id,
                timestamp: f.timestamp.clone(),
                trigger_type: f.trigger_type,
                app_name: f.app_name,
                window_title: f.window_title,
                importance: f.importance,
                resolution: format!("{}x{}", f.resolution_w, f.resolution_h),
                file_path: f.file_path,
                ocr_text: f.ocr_text,
                image_url,
                tag_ids: Vec::new(),
            }
        })
        .collect();

    let data: Vec<FrameResponse> = {
        let frame_ids: Vec<i64> = data.iter().map(|f| f.id).collect();
        if frame_ids.is_empty() {
            data
        } else {
            let tag_map = state
                .storage
                .get_tag_ids_for_frames(&frame_ids)
                .map_err(|e| ApiError::Internal(e.to_string()))?;

            data.into_iter()
                .map(|mut f| {
                    if let Some(tags) = tag_map.get(&f.id) {
                        f.tag_ids = tags.clone();
                    }
                    f
                })
                .collect()
        }
    };

    let has_more = (offset + data.len()) < total as usize;

    Ok(Json(PaginatedResponse {
        data,
        pagination: PaginationMeta {
            total,
            offset,
            limit,
            has_more,
        },
    }))
}

///
/// GET /api/frames/:id/image
pub async fn get_frame_image(State(state): State<AppState>, Path(frame_id): Path<i64>) -> Response {
    let file_path = match state.storage.get_frame_file_path(frame_id) {
        Ok(Some(path)) => path,
        Ok(None) => {
            return ApiError::NotFound(format!("frame {frame_id}에 이미지가 none")).into_response()
        }
        Err(e) => return ApiError::Internal(e.to_string()).into_response(),
    };

    let full_path = if let Some(ref frames_dir) = state.frames_dir {
        let joined = frames_dir.join(&file_path);
        match joined.canonicalize() {
            Ok(canonical) => {
                let frames_canonical = frames_dir
                    .canonicalize()
                    .unwrap_or_else(|_| frames_dir.clone());
                if !canonical.starts_with(&frames_canonical) {
                    return ApiError::BadRequest("잘못된 file path".to_string()).into_response();
                }
                canonical
            }
            Err(_) => {
                return ApiError::NotFound(format!("이미지 file none: {}", file_path))
                    .into_response();
            }
        }
    } else {
        std::path::PathBuf::from(&file_path)
    };

    let data = match std::fs::read(&full_path) {
        Ok(d) => d,
        Err(e) => return ApiError::Internal(format!("file read failure: {e}")).into_response(),
    };

    let content_type = mime_guess::from_path(&full_path)
        .first_or_octet_stream()
        .to_string();

    (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], data).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_response_serializes() {
        let frame = FrameResponse {
            id: 1,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            trigger_type: "AppSwitch".to_string(),
            app_name: "Code".to_string(),
            window_title: "main.rs".to_string(),
            importance: 0.85,
            resolution: "1920x1080".to_string(),
            file_path: Some("frames/123.webp".to_string()),
            ocr_text: None,
            image_url: Some("/api/frames/1/image".to_string()),
            tag_ids: vec![1, 3],
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("Code"));
        assert!(json.contains("tag_ids"));
    }
}
