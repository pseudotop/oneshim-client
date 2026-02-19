//! 태그 관련 스토리지 메서드.
//!
//! 프레임에 태그를 추가/제거하고 태그별 프레임 조회.

use oneshim_core::error::CoreError;
use tracing::debug;

use super::{FrameRecord, SqliteStorage, TagRecord};

impl SqliteStorage {
    /// 태그 생성
    ///
    /// # Arguments
    /// * `name` - 태그 이름 (고유)
    /// * `color` - 태그 색상 (hex, 예: "#3b82f6")
    pub fn create_tag(&self, name: &str, color: &str) -> Result<TagRecord, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT INTO tags (name, color) VALUES (?1, ?2)",
            rusqlite::params![name, color],
        )
        .map_err(|e| CoreError::Internal(format!("태그 생성 실패: {e}")))?;

        let tag_id = conn.last_insert_rowid();
        let created_at: String = conn
            .query_row(
                "SELECT created_at FROM tags WHERE id = ?1",
                rusqlite::params![tag_id],
                |row| row.get(0),
            )
            .map_err(|e| CoreError::Internal(format!("태그 조회 실패: {e}")))?;

        debug!("태그 생성: id={}, name={}", tag_id, name);

        Ok(TagRecord {
            id: tag_id,
            name: name.to_string(),
            color: color.to_string(),
            created_at,
        })
    }

    /// 모든 태그 조회
    pub fn get_all_tags(&self) -> Result<Vec<TagRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT id, name, color, created_at FROM tags ORDER BY name")
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let tags = stmt
            .query_map([], |row| {
                Ok(TagRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tags)
    }

    /// 태그 조회 (ID로)
    pub fn get_tag(&self, tag_id: i64) -> Result<Option<TagRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let result = conn.query_row(
            "SELECT id, name, color, created_at FROM tags WHERE id = ?1",
            rusqlite::params![tag_id],
            |row| {
                Ok(TagRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        );

        match result {
            Ok(tag) => Ok(Some(tag)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Internal(format!("태그 조회 실패: {e}"))),
        }
    }

    /// 태그 삭제
    pub fn delete_tag(&self, tag_id: i64) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let deleted = conn
            .execute("DELETE FROM tags WHERE id = ?1", rusqlite::params![tag_id])
            .map_err(|e| CoreError::Internal(format!("태그 삭제 실패: {e}")))?;

        debug!("태그 삭제: id={}, affected={}", tag_id, deleted);
        Ok(deleted > 0)
    }

    /// 프레임에 태그 추가
    pub fn add_tag_to_frame(&self, frame_id: i64, tag_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO frame_tags (frame_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![frame_id, tag_id],
        )
        .map_err(|e| CoreError::Internal(format!("프레임 태그 추가 실패: {e}")))?;

        debug!("프레임 태그 추가: frame_id={}, tag_id={}", frame_id, tag_id);
        Ok(())
    }

    /// 프레임에서 태그 제거
    pub fn remove_tag_from_frame(&self, frame_id: i64, tag_id: i64) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let deleted = conn
            .execute(
                "DELETE FROM frame_tags WHERE frame_id = ?1 AND tag_id = ?2",
                rusqlite::params![frame_id, tag_id],
            )
            .map_err(|e| CoreError::Internal(format!("프레임 태그 제거 실패: {e}")))?;

        debug!(
            "프레임 태그 제거: frame_id={}, tag_id={}, affected={}",
            frame_id, tag_id, deleted
        );
        Ok(deleted > 0)
    }

    /// 프레임의 모든 태그 조회
    pub fn get_tags_for_frame(&self, frame_id: i64) -> Result<Vec<TagRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.name, t.color, t.created_at
                 FROM tags t
                 INNER JOIN frame_tags ft ON t.id = ft.tag_id
                 WHERE ft.frame_id = ?1
                 ORDER BY t.name",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let tags = stmt
            .query_map(rusqlite::params![frame_id], |row| {
                Ok(TagRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tags)
    }

    /// 특정 태그가 있는 프레임 조회
    pub fn get_frames_by_tag(
        &self,
        tag_id: i64,
        limit: usize,
    ) -> Result<Vec<FrameRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT f.id, f.timestamp, f.trigger_type, f.app_name, f.window_title,
                        f.importance, f.resolution_w, f.resolution_h, f.file_path, f.ocr_text
                 FROM frames f
                 INNER JOIN frame_tags ft ON f.id = ft.frame_id
                 WHERE ft.tag_id = ?1
                 ORDER BY f.timestamp DESC
                 LIMIT ?2",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let frames = stmt
            .query_map(rusqlite::params![tag_id, limit as i64], |row| {
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

    /// 태그 업데이트
    pub fn update_tag(&self, tag_id: i64, name: &str, color: &str) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let updated = conn
            .execute(
                "UPDATE tags SET name = ?1, color = ?2 WHERE id = ?3",
                rusqlite::params![name, color, tag_id],
            )
            .map_err(|e| CoreError::Internal(format!("태그 업데이트 실패: {e}")))?;

        debug!("태그 업데이트: id={}, affected={}", tag_id, updated);
        Ok(updated > 0)
    }
}
