use oneshim_core::error::CoreError;
use std::collections::HashMap;
use tracing::debug;

use super::{FrameRecord, SqliteStorage, TagRecord};

impl SqliteStorage {
    pub fn get_tag_ids_for_frames(
        &self,
        frame_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<i64>>, CoreError> {
        if frame_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let placeholders: Vec<String> = frame_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT frame_id, tag_id FROM frame_tags WHERE frame_id IN ({})",
            placeholders.join(",")
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let params: Vec<Box<dyn rusqlite::types::ToSql>> = frame_ids
            .iter()
            .map(|id| Box::new(*id) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?;

        let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
        for row in rows {
            let (frame_id, tag_id) =
                row.map_err(|e| CoreError::Internal(format!("Failed to read row: {e}")))?;
            map.entry(frame_id).or_default().push(tag_id);
        }

        Ok(map)
    }

    /// # Arguments
    pub fn create_tag(&self, name: &str, color: &str) -> Result<TagRecord, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "INSERT INTO tags (name, color) VALUES (?1, ?2)",
            rusqlite::params![name, color],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to create tag: {e}")))?;

        let tag_id = conn.last_insert_rowid();
        let created_at: String = conn
            .query_row(
                "SELECT created_at FROM tags WHERE id = ?1",
                rusqlite::params![tag_id],
                |row| row.get(0),
            )
            .map_err(|e| CoreError::Internal(format!("Failed to query tag: {e}")))?;

        debug!("create: id={}, name={}", tag_id, name);

        Ok(TagRecord {
            id: tag_id,
            name: name.to_string(),
            color: color.to_string(),
            created_at,
        })
    }

    pub fn get_all_tags(&self) -> Result<Vec<TagRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare("SELECT id, name, color, created_at FROM tags ORDER BY name")
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let tags = stmt
            .query_map([], |row| {
                Ok(TagRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tags)
    }

    pub fn get_tag(&self, tag_id: i64) -> Result<Option<TagRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

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
            Err(e) => Err(CoreError::Internal(format!("Failed to query tag: {e}"))),
        }
    }

    pub fn delete_tag(&self, tag_id: i64) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let deleted = conn
            .execute("DELETE FROM tags WHERE id = ?1", rusqlite::params![tag_id])
            .map_err(|e| CoreError::Internal(format!("Failed to delete tag: {e}")))?;

        debug!("delete: id={}, affected={}", tag_id, deleted);
        Ok(deleted > 0)
    }

    pub fn add_tag_to_frame(&self, frame_id: i64, tag_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        conn.execute(
            "INSERT OR IGNORE INTO frame_tags (frame_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![frame_id, tag_id],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to add frame tag: {e}")))?;

        debug!("frame add: frame_id={}, tag_id={}", frame_id, tag_id);
        Ok(())
    }

    pub fn remove_tag_from_frame(&self, frame_id: i64, tag_id: i64) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let deleted = conn
            .execute(
                "DELETE FROM frame_tags WHERE frame_id = ?1 AND tag_id = ?2",
                rusqlite::params![frame_id, tag_id],
            )
            .map_err(|e| CoreError::Internal(format!("Failed to remove frame tag: {e}")))?;

        debug!(
            "frame 태그 제거: frame_id={}, tag_id={}, affected={}",
            frame_id, tag_id, deleted
        );
        Ok(deleted > 0)
    }

    pub fn get_tags_for_frame(&self, frame_id: i64) -> Result<Vec<TagRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.name, t.color, t.created_at
                 FROM tags t
                 INNER JOIN frame_tags ft ON t.id = ft.tag_id
                 WHERE ft.frame_id = ?1
                 ORDER BY t.name",
            )
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

        let tags = stmt
            .query_map(rusqlite::params![frame_id], |row| {
                Ok(TagRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tags)
    }

    pub fn get_frames_by_tag(
        &self,
        tag_id: i64,
        limit: usize,
    ) -> Result<Vec<FrameRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("Failed to prepare query: {e}")))?;

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
            .map_err(|e| CoreError::Internal(format!("Failed to execute query: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(frames)
    }

    pub fn update_tag(&self, tag_id: i64, name: &str, color: &str) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("Failed to acquire lock: {e}")))?;

        let updated = conn
            .execute(
                "UPDATE tags SET name = ?1, color = ?2 WHERE id = ?3",
                rusqlite::params![name, color, tag_id],
            )
            .map_err(|e| CoreError::Internal(format!("Failed to update tag: {e}")))?;

        debug!("update: id={}, affected={}", tag_id, updated);
        Ok(updated > 0)
    }
}
