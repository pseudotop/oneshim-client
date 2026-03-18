use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::ports::text_search::{TextSearchProvider, TextSearchResult};
use tracing::warn;

use super::SqliteStorage;

#[async_trait]
impl TextSearchProvider for SqliteStorage {
    async fn search_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TextSearchResult>, CoreError> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let query = query.to_string();
        self.with_conn(move |conn| {
            // Check if the FTS5 table exists before querying
            let table_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='search_fts'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if !table_exists {
                warn!("search_fts table not available; returning empty results");
                return Ok(vec![]);
            }

            let mut stmt = conn
                .prepare(
                    "SELECT segment_id, content_type, searchable_text, rank
                     FROM search_fts
                     WHERE search_fts MATCH ?1
                     ORDER BY rank
                     LIMIT ?2",
                )
                .map_err(|e| CoreError::Internal(format!("FTS5 query prepare failed: {e}")))?;

            let results = stmt
                .query_map(rusqlite::params![query, limit as i64], |row| {
                    Ok(TextSearchResult {
                        segment_id: row.get(0)?,
                        content_type: row.get(1)?,
                        matched_text: row.get(2)?,
                        rank: row.get(3)?,
                    })
                })
                .map_err(|e| CoreError::Internal(format!("FTS5 query failed: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(results)
        })
        .await
    }

    fn sync_segment(&self, segment_id: &str, searchable_text: &str) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;

        // Check if the FTS5 table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='search_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !table_exists {
            warn!("search_fts table not available; skipping sync_segment");
            return Ok(());
        }

        // Delete existing entry then insert (FTS5 does not support INSERT OR REPLACE)
        conn.execute(
            "DELETE FROM search_fts WHERE segment_id = ?1",
            rusqlite::params![segment_id],
        )
        .map_err(|e| CoreError::Internal(format!("FTS5 delete failed: {e}")))?;

        conn.execute(
            "INSERT INTO search_fts (segment_id, content_type, searchable_text) VALUES (?1, ?2, ?3)",
            rusqlite::params![segment_id, "segment", searchable_text],
        )
        .map_err(|e| CoreError::Internal(format!("FTS5 insert failed: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sync_and_search_roundtrip() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage
            .sync_segment("seg-001", "deep work on authentication module")
            .unwrap();
        storage
            .sync_segment("seg-002", "slack communication with team")
            .unwrap();

        let results = storage.search_fts("authentication", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].segment_id, "seg-001");
        assert!(results[0].matched_text.contains("authentication"));
    }

    #[tokio::test]
    async fn search_empty_query_returns_empty() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage.sync_segment("seg-001", "some content").unwrap();

        let results = storage.search_fts("", 10).await.unwrap();
        assert!(results.is_empty());

        let results = storage.search_fts("   ", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn multiple_results_ordering() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage
            .sync_segment("seg-001", "rust programming language systems")
            .unwrap();
        storage
            .sync_segment("seg-002", "rust compiler optimization rust")
            .unwrap();
        storage
            .sync_segment("seg-003", "python web development")
            .unwrap();

        let results = storage.search_fts("rust", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        // seg-002 mentions "rust" twice, so it should rank better (lower rank value in FTS5)
        assert_eq!(results[0].segment_id, "seg-002");
    }

    #[tokio::test]
    async fn sync_segment_updates_existing() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        storage
            .sync_segment("seg-001", "original content about rust")
            .unwrap();
        storage
            .sync_segment("seg-001", "updated content about python")
            .unwrap();

        let rust_results = storage.search_fts("rust", 10).await.unwrap();
        assert!(rust_results.is_empty());

        let python_results = storage.search_fts("python", 10).await.unwrap();
        assert_eq!(python_results.len(), 1);
        assert_eq!(python_results[0].segment_id, "seg-001");
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();

        for i in 0..5 {
            storage
                .sync_segment(&format!("seg-{i:03}"), "common keyword content")
                .unwrap();
        }

        let results = storage.search_fts("keyword", 2).await.unwrap();
        assert_eq!(results.len(), 2);
    }
}
