use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::Json;
use oneshim_api_contracts::frames::FrameResponse;

use crate::error::ApiError;
use crate::services::frames_service::FramesQueryService;
use crate::services::web_contexts::StorageWebContext;

use super::{PaginatedResponse, TimeRangeQuery};

/// GET /api/frames?from=&to=&limit=&offset=
pub async fn get_frames(
    State(context): State<StorageWebContext>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<PaginatedResponse<FrameResponse>>, ApiError> {
    Ok(Json(FramesQueryService::new(context).get_frames(&params)?))
}

/// GET /api/frames/:id/image
pub async fn get_frame_image(
    State(context): State<StorageWebContext>,
    Path(frame_id): Path<i64>,
) -> Response {
    FramesQueryService::new(context).get_frame_image(frame_id)
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
