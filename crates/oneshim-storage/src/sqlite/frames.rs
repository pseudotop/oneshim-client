//! 프레임 메타데이터 스토리지 메서드.
//!
//! 스크린샷 프레임의 메타데이터 저장 및 조회.

use chrono::{DateTime, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::context::WindowBounds;
use oneshim_core::models::frame::FrameMetadata;
use tracing::debug;

use super::{FrameRecord, SqliteStorage};

impl SqliteStorage {
    /// 프레임 메타데이터 저장
    ///
    /// # Arguments
    /// * `metadata` - 프레임 메타데이터
    /// * `file_path` - 저장된 이미지 파일의 상대 경로 (None이면 이미지 없음)
    /// * `ocr_text` - OCR 추출 텍스트 (있는 경우)
    pub fn save_frame_metadata(
        &self,
        metadata: &FrameMetadata,
        file_path: Option<&str>,
        ocr_text: Option<&str>,
    ) -> Result<i64, CoreError> {
        self.save_frame_metadata_with_bounds(metadata, file_path, ocr_text, None)
    }

    /// 프레임 메타데이터 저장 (창 위치 포함)
    ///
    /// # Arguments
    /// * `metadata` - 프레임 메타데이터
    /// * `file_path` - 저장된 이미지 파일의 상대 경로
    /// * `ocr_text` - OCR 추출 텍스트
    /// * `bounds` - 창 위치/크기
    pub fn save_frame_metadata_with_bounds(
        &self,
        metadata: &FrameMetadata,
        file_path: Option<&str>,
        ocr_text: Option<&str>,
        bounds: Option<&WindowBounds>,
    ) -> Result<i64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT INTO frames (timestamp, trigger_type, app_name, window_title, importance, resolution_w, resolution_h, has_image, file_path, ocr_text, window_x, window_y, window_width, window_height)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                metadata.timestamp.to_rfc3339(),
                metadata.trigger_type,
                metadata.app_name,
                metadata.window_title,
                metadata.importance,
                metadata.resolution.0,
                metadata.resolution.1,
                file_path.is_some(),
                file_path,
                ocr_text,
                bounds.map(|b| b.x),
                bounds.map(|b| b.y),
                bounds.map(|b| b.width as i32),
                bounds.map(|b| b.height as i32),
            ],
        )
        .map_err(|e| CoreError::Internal(format!("프레임 메타데이터 저장 실패: {e}")))?;

        let frame_id = conn.last_insert_rowid();
        debug!(
            "프레임 메타데이터 저장: id={}, app={}, file={}",
            frame_id,
            metadata.app_name,
            file_path.unwrap_or("-")
        );

        Ok(frame_id)
    }

    /// 프레임 메타데이터 목록 조회
    ///
    /// # Arguments
    /// * `from` - 시작 시각
    /// * `to` - 종료 시각
    /// * `limit` - 최대 조회 개수
    pub fn get_frames(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<FrameRecord>, CoreError> {
        let from_str = from.to_rfc3339();
        let to_str = to.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, trigger_type, app_name, window_title, importance, resolution_w, resolution_h, file_path, ocr_text
                 FROM frames
                 WHERE timestamp >= ?1 AND timestamp <= ?2
                 ORDER BY timestamp DESC
                 LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let frames = stmt
            .query_map(rusqlite::params![from_str, to_str, limit as i64], |row| {
                Ok(FrameRecord {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    trigger_type: row.get(2)?,
                    app_name: row.get(3)?,
                    window_title: row.get(4)?,
                    importance: row.get(5)?,
                    resolution_w: row.get(6)?,
                    resolution_h: row.get(7)?,
                    file_path: row.get(8)?,
                    ocr_text: row.get(9)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(frames)
    }
}
