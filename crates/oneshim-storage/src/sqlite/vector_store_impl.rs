use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::embedding::{
    EmbeddingContentType, EmbeddingMetadata, SearchFilters, SearchResult,
};
use oneshim_core::ports::vector_store::VectorStore;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use tracing::debug;

/// SQLite-backed vector store with brute-force cosine similarity search.
///
/// Vectors are stored as little-endian f32 BLOBs in the `embedding_vectors` table.
/// Search is performed in-memory via brute-force cosine similarity with optional
/// exponential time decay weighting.
pub struct SqliteVectorStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteVectorStore {
    /// Create a new `SqliteVectorStore` sharing the same connection as `SqliteStorage`.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Wrap a synchronous closure on the connection via `spawn_blocking`.
    async fn with_conn<F, T>(&self, f: F) -> Result<T, CoreError>
    where
        F: FnOnce(&Connection) -> Result<T, CoreError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|e| CoreError::Internal(format!("SQLite lock poisoned: {e}")))?;
            f(&guard)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }
}

/// Convert a slice of f32 values to a little-endian byte vector.
fn f32_vec_to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert a byte slice back to a Vec<f32> (little-endian).
fn bytes_to_f32_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Row fetched from the embedding_vectors table for brute-force search.
struct VectorRow {
    segment_id: String,
    content_type: String,
    content_label: Option<String>,
    original_text: String,
    vector: Vec<f32>,
    timestamp: DateTime<Utc>,
}

fn parse_content_type(s: &str) -> EmbeddingContentType {
    match s {
        "SEGMENT_SUMMARY" => EmbeddingContentType::SegmentSummary,
        _ => EmbeddingContentType::ContentActivity,
    }
}

/// Map a single SQLite row (with columns segment_id, content_type, content_label,
/// original_text, vector, timestamp at positions 0..5) to a `VectorRow`.
fn map_vector_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VectorRow> {
    let ts_str: String = row.get(5)?;
    let timestamp = DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let blob: Vec<u8> = row.get(4)?;
    Ok(VectorRow {
        segment_id: row.get(0)?,
        content_type: row.get(1)?,
        content_label: row.get(2)?,
        original_text: row.get(3)?,
        vector: bytes_to_f32_vec(&blob),
        timestamp,
    })
}

fn content_type_to_str(ct: &EmbeddingContentType) -> &'static str {
    match ct {
        EmbeddingContentType::SegmentSummary => "SEGMENT_SUMMARY",
        EmbeddingContentType::ContentActivity => "CONTENT_ACTIVITY",
    }
}

/// Execute brute-force search on rows, applying cosine similarity + time decay.
fn brute_force_search(
    rows: Vec<VectorRow>,
    query_vector: &[f32],
    limit: usize,
    time_decay_hours: f32,
) -> Vec<SearchResult> {
    let now = Utc::now();
    let mut scored: Vec<SearchResult> = rows
        .into_iter()
        .map(|row| {
            let similarity = cosine_similarity(query_vector, &row.vector);
            let age_hours = (now - row.timestamp).num_seconds().max(0) as f32 / 3600.0;
            let time_decay = if time_decay_hours > 0.0 {
                (-age_hours / time_decay_hours).exp()
            } else {
                1.0
            };
            let score = similarity * time_decay;
            SearchResult {
                segment_id: row.segment_id,
                content_type: parse_content_type(&row.content_type),
                content_label: row.content_label,
                score,
                similarity,
                time_decay,
                timestamp: row.timestamp,
                original_text: row.original_text,
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    scored
}

#[async_trait]
impl VectorStore for SqliteVectorStore {
    async fn store(&self, vector: Vec<f32>, metadata: EmbeddingMetadata) -> Result<(), CoreError> {
        let blob = f32_vec_to_bytes(&vector);
        let content_type_str = content_type_to_str(&metadata.content_type).to_string();
        let timestamp_str = metadata.timestamp.to_rfc3339();

        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO embedding_vectors (segment_id, content_type, content_label, original_text, vector, model_id, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    metadata.segment_id,
                    content_type_str,
                    metadata.content_label,
                    metadata.original_text,
                    blob,
                    metadata.model_id,
                    timestamp_str,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("Failed to store embedding vector: {e}")))?;

            debug!(
                "Stored embedding vector for segment {} (type={})",
                metadata.segment_id, content_type_str
            );
            Ok(())
        })
        .await
    }

    async fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
        time_decay_hours: f32,
    ) -> Result<Vec<SearchResult>, CoreError> {
        let qv = query_vector.to_vec();

        self.with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT segment_id, content_type, content_label, original_text, vector, timestamp
                     FROM embedding_vectors
                     WHERE is_stale = 0",
                )
                .map_err(|e| CoreError::Internal(format!("Failed to prepare search query: {e}")))?;

            let rows: Vec<VectorRow> = stmt
                .query_map([], map_vector_row)
                .map_err(|e| CoreError::Internal(format!("Failed to query vectors: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(brute_force_search(rows, &qv, limit, time_decay_hours))
        })
        .await
    }

    async fn search_filtered(
        &self,
        query_vector: &[f32],
        limit: usize,
        time_decay_hours: f32,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, CoreError> {
        let qv = query_vector.to_vec();
        let filters = filters.clone();

        self.with_conn(move |conn| {
            // Build dynamic WHERE clause
            let mut conditions = vec!["is_stale = 0".to_string()];
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(ref after) = filters.after {
                conditions.push(format!("timestamp >= ?{}", param_values.len() + 1));
                param_values.push(Box::new(after.to_rfc3339()));
            }
            if let Some(ref before) = filters.before {
                conditions.push(format!("timestamp <= ?{}", param_values.len() + 1));
                param_values.push(Box::new(before.to_rfc3339()));
            }
            if let Some(ref content_types) = filters.content_types {
                if !content_types.is_empty() {
                    let placeholders: Vec<String> = content_types
                        .iter()
                        .map(|_| {
                            let idx = param_values.len() + 1;
                            format!("?{idx}")
                        })
                        .collect();
                    conditions.push(format!("content_type IN ({})", placeholders.join(", ")));
                    for ct in content_types {
                        param_values.push(Box::new(content_type_to_str(ct).to_string()));
                    }
                }
            }
            // regime_id filter: segment_id based lookup would require a join;
            // for simplicity we skip regime_id in the SQL and could filter post-query.
            if filters.regime_id.is_some() {
                tracing::warn!("regime_id filter not yet implemented, ignoring");
            }

            let where_clause = conditions.join(" AND ");
            let sql = format!(
                "SELECT segment_id, content_type, content_label, original_text, vector, timestamp
                 FROM embedding_vectors
                 WHERE {where_clause}"
            );

            let mut stmt = conn.prepare(&sql).map_err(|e| {
                CoreError::Internal(format!("Failed to prepare filtered query: {e}"))
            })?;

            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            let rows: Vec<VectorRow> = stmt
                .query_map(params_ref.as_slice(), map_vector_row)
                .map_err(|e| CoreError::Internal(format!("Failed to query filtered vectors: {e}")))?
                .filter_map(|r| r.ok())
                .collect();

            Ok(brute_force_search(rows, &qv, limit, time_decay_hours))
        })
        .await
    }

    async fn enforce_retention(&self, max_days: u32) -> Result<u64, CoreError> {
        self.with_conn(move |conn| {
            let cutoff = (Utc::now() - Duration::days(max_days as i64)).to_rfc3339();
            let deleted = conn
                .execute(
                    "DELETE FROM embedding_vectors WHERE timestamp < ?1",
                    params![cutoff],
                )
                .map_err(|e| {
                    CoreError::Internal(format!("Failed to enforce vector retention: {e}"))
                })?;
            debug!("Enforced vector retention: deleted {deleted} rows older than {max_days} days");
            Ok(deleted as u64)
        })
        .await
    }

    async fn mark_stale(&self, old_model_id: &str) -> Result<u64, CoreError> {
        let model_id = old_model_id.to_string();
        self.with_conn(move |conn| {
            let updated = conn
                .execute(
                    "UPDATE embedding_vectors SET is_stale = 1 WHERE model_id = ?1",
                    params![model_id],
                )
                .map_err(|e| CoreError::Internal(format!("Failed to mark vectors stale: {e}")))?;
            debug!("Marked {updated} vectors as stale for model_id={model_id}");
            Ok(updated as u64)
        })
        .await
    }

    async fn get_current_model_id(&self) -> Result<Option<String>, CoreError> {
        self.with_conn(move |conn| {
            let result: Option<String> = conn
                .query_row(
                    "SELECT model_id FROM embedding_vectors WHERE is_stale = 0 ORDER BY id DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .ok();
            Ok(result)
        })
        .await
    }

    async fn get_stale_vectors(&self, limit: usize) -> Result<Vec<(i64, String)>, CoreError> {
        self.with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, original_text FROM embedding_vectors WHERE is_stale = 1 LIMIT ?1",
                )
                .map_err(|e| CoreError::Internal(format!("Failed to prepare stale query: {e}")))?;
            let rows: Vec<(i64, String)> = stmt
                .query_map(params![limit as i64], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| CoreError::Internal(format!("Failed to query stale vectors: {e}")))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(rows)
        })
        .await
    }

    async fn update_vector(
        &self,
        id: i64,
        vector: Vec<f32>,
        model_id: &str,
    ) -> Result<(), CoreError> {
        let blob = f32_vec_to_bytes(&vector);
        let model_id = model_id.to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE embedding_vectors SET vector = ?1, model_id = ?2, is_stale = 0 WHERE id = ?3",
                params![blob, model_id, id],
            )
            .map_err(|e| CoreError::Internal(format!("Failed to update vector: {e}")))?;
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration;

    fn setup_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        migration::run_migrations(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn f32_roundtrip() {
        let original = vec![1.0f32, -2.5, 3.125, 0.0];
        let bytes = f32_vec_to_bytes(&original);
        let restored = bytes_to_f32_vec(&bytes);
        assert_eq!(original, restored);
    }

    #[test]
    fn cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 2.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[tokio::test]
    async fn store_and_search_roundtrip() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        let meta = EmbeddingMetadata {
            segment_id: "seg-001".to_string(),
            content_type: EmbeddingContentType::ContentActivity,
            content_label: Some("VSCode: main.rs".to_string()),
            timestamp: Utc::now(),
            original_text: "VSCode: main.rs".to_string(),
            model_id: "test-model".to_string(),
        };

        store.store(vec![1.0, 0.0, 0.0], meta).await.unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 10, 24.0).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].segment_id, "seg-001");
        assert!(results[0].similarity > 0.99);
    }

    #[tokio::test]
    async fn search_returns_top_k_by_score() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        // Store three vectors: similar, less similar, and orthogonal
        let now = Utc::now();

        store
            .store(
                vec![1.0, 0.0, 0.0],
                EmbeddingMetadata {
                    segment_id: "close".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("close".to_string()),
                    timestamp: now,
                    original_text: "close".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .store(
                vec![0.7, 0.7, 0.0],
                EmbeddingMetadata {
                    segment_id: "medium".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("medium".to_string()),
                    timestamp: now,
                    original_text: "medium".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .store(
                vec![0.0, 0.0, 1.0],
                EmbeddingMetadata {
                    segment_id: "far".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("far".to_string()),
                    timestamp: now,
                    original_text: "far".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 2, 24.0).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].segment_id, "close");
        assert_eq!(results[1].segment_id, "medium");
    }

    #[tokio::test]
    async fn time_decay_reduces_old_scores() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        let now = Utc::now();
        let old = now - Duration::hours(48);

        // Same vector, but one is old
        store
            .store(
                vec![1.0, 0.0, 0.0],
                EmbeddingMetadata {
                    segment_id: "recent".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("recent".to_string()),
                    timestamp: now,
                    original_text: "recent".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .store(
                vec![1.0, 0.0, 0.0],
                EmbeddingMetadata {
                    segment_id: "old".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("old".to_string()),
                    timestamp: old,
                    original_text: "old".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 10, 24.0).await.unwrap();
        assert_eq!(results.len(), 2);
        // Recent should score higher due to time decay
        assert_eq!(results[0].segment_id, "recent");
        assert!(results[0].score > results[1].score);
        // Both have similarity ~1.0
        assert!(results[0].similarity > 0.99);
        assert!(results[1].similarity > 0.99);
        // Old one has lower time_decay
        assert!(results[1].time_decay < results[0].time_decay);
    }

    #[tokio::test]
    async fn enforce_retention_deletes_old() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        let old = Utc::now() - Duration::days(60);
        store
            .store(
                vec![1.0, 0.0],
                EmbeddingMetadata {
                    segment_id: "old-seg".to_string(),
                    content_type: EmbeddingContentType::SegmentSummary,
                    content_label: None,
                    timestamp: old,
                    original_text: "".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .store(
                vec![0.0, 1.0],
                EmbeddingMetadata {
                    segment_id: "new-seg".to_string(),
                    content_type: EmbeddingContentType::SegmentSummary,
                    content_label: None,
                    timestamp: Utc::now(),
                    original_text: "".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        let deleted = store.enforce_retention(30).await.unwrap();
        assert_eq!(deleted, 1);

        let results = store.search(&[1.0, 0.0], 10, 0.0).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].segment_id, "new-seg");
    }

    #[tokio::test]
    async fn mark_stale_excludes_from_search() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        store
            .store(
                vec![1.0, 0.0],
                EmbeddingMetadata {
                    segment_id: "seg-stale".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("stale".to_string()),
                    timestamp: Utc::now(),
                    original_text: "stale".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        // Before marking stale
        let results = store.search(&[1.0, 0.0], 10, 0.0).await.unwrap();
        assert_eq!(results.len(), 1);

        // Mark stale
        let marked = store.mark_stale("test-model").await.unwrap();
        assert_eq!(marked, 1);

        // After marking stale: search should exclude
        let results = store.search(&[1.0, 0.0], 10, 0.0).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_filtered_by_content_type() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        let now = Utc::now();
        store
            .store(
                vec![1.0, 0.0],
                EmbeddingMetadata {
                    segment_id: "s1".to_string(),
                    content_type: EmbeddingContentType::SegmentSummary,
                    content_label: None,
                    timestamp: now,
                    original_text: "".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .store(
                vec![1.0, 0.0],
                EmbeddingMetadata {
                    segment_id: "s2".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("activity".to_string()),
                    timestamp: now,
                    original_text: "activity".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        let filters = SearchFilters {
            content_types: Some(vec![EmbeddingContentType::SegmentSummary]),
            ..Default::default()
        };
        let results = store
            .search_filtered(&[1.0, 0.0], 10, 0.0, &filters)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].segment_id, "s1");
    }

    #[tokio::test]
    async fn search_filtered_by_time_range() {
        let conn = setup_db();
        let store = SqliteVectorStore::new(conn);

        let old = Utc::now() - Duration::days(5);
        let recent = Utc::now();

        store
            .store(
                vec![1.0],
                EmbeddingMetadata {
                    segment_id: "old".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("old".to_string()),
                    timestamp: old,
                    original_text: "old".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        store
            .store(
                vec![1.0],
                EmbeddingMetadata {
                    segment_id: "recent".to_string(),
                    content_type: EmbeddingContentType::ContentActivity,
                    content_label: Some("recent".to_string()),
                    timestamp: recent,
                    original_text: "recent".to_string(),
                    model_id: "test-model".to_string(),
                },
            )
            .await
            .unwrap();

        let filters = SearchFilters {
            after: Some(Utc::now() - Duration::days(1)),
            ..Default::default()
        };
        let results = store
            .search_filtered(&[1.0], 10, 0.0, &filters)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].segment_id, "recent");
    }
}
