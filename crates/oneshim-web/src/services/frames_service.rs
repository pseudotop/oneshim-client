use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use oneshim_api_contracts::frames::FrameResponse;

use crate::error::ApiError;
use crate::services::frames_assembler::assemble_frame_response;
use crate::services::web_contexts::StorageWebContext;
use oneshim_api_contracts::common::{PaginatedResponse, PaginationMeta, TimeRangeQuery};

#[derive(Clone)]
pub struct FramesQueryService {
    ctx: StorageWebContext,
}

impl FramesQueryService {
    pub fn new(ctx: StorageWebContext) -> Self {
        Self { ctx }
    }

    pub fn get_frames(
        &self,
        params: &TimeRangeQuery,
    ) -> Result<PaginatedResponse<FrameResponse>, ApiError> {
        let from = params.from_datetime();
        let to = params.to_datetime();
        let limit = params.limit_or_default();
        let offset = params.offset_or_default();

        let min_importance = params.min_importance.unwrap_or(0.0) as f32;

        // Fetch all frames in range (no limit) so we can filter by importance first,
        // then paginate the filtered result for correct total count.
        let all_frames = self.ctx.storage.get_frames(from, to, usize::MAX)?;

        let filtered: Vec<_> = all_frames
            .into_iter()
            .filter(|f| f.importance >= min_importance)
            .collect();

        let total = filtered.len() as u64;

        let data: Vec<FrameResponse> = filtered
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(assemble_frame_response)
            .collect();

        let data: Vec<FrameResponse> = {
            let frame_ids: Vec<i64> = data.iter().map(|frame| frame.id).collect();
            if frame_ids.is_empty() {
                data
            } else {
                let tag_map = self
                    .ctx
                    .storage
                    .get_tag_ids_for_frames(&frame_ids)
                    .map_err(ApiError::from)?;

                data.into_iter()
                    .map(|mut frame| {
                        if let Some(tags) = tag_map.get(&frame.id) {
                            frame.tag_ids = tags.clone();
                        }
                        frame
                    })
                    .collect()
            }
        };

        let has_more = (offset + data.len()) < total as usize;

        Ok(PaginatedResponse {
            data,
            pagination: PaginationMeta {
                total,
                offset,
                limit,
                has_more,
            },
        })
    }

    pub fn get_frame_image(&self, frame_id: i64) -> Response {
        let file_path = match self.ctx.storage.get_frame_file_path(frame_id) {
            Ok(Some(path)) => path,
            Ok(None) => {
                return ApiError::NotFound(format!("frame {frame_id} has no image"))
                    .into_response();
            }
            Err(error) => return ApiError::Internal(error.to_string()).into_response(),
        };

        let full_path = if let Some(ref frames_dir) = self.ctx.frames_dir {
            let joined = frames_dir.join(&file_path);
            match joined.canonicalize() {
                Ok(canonical) => {
                    let frames_canonical = frames_dir
                        .canonicalize()
                        .unwrap_or_else(|_| frames_dir.clone());
                    if !canonical.starts_with(&frames_canonical) {
                        return ApiError::BadRequest("Invalid file path".to_string())
                            .into_response();
                    }
                    canonical
                }
                Err(_) => {
                    return ApiError::NotFound(format!("Image file not found: {}", file_path))
                        .into_response();
                }
            }
        } else {
            std::path::PathBuf::from(&file_path)
        };

        let data = match std::fs::read(&full_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return ApiError::Internal(format!("file read failure: {error}")).into_response();
            }
        };

        let content_type = mime_guess::from_path(&full_path)
            .first_or_octet_stream()
            .to_string();

        (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], data).into_response()
    }
}
