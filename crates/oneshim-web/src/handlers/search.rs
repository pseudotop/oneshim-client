use axum::extract::{Query, State};
use axum::Json;
use oneshim_api_contracts::search::{SearchQuery, SearchResponse, SearchResult, TagInfo};

use crate::error::ApiError;
use crate::AppState;

pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, ApiError> {
    let query = params.q.trim();

    let tag_ids: Vec<i64> = params
        .tag_ids
        .as_ref()
        .map(|s| {
            s.split(',')
                .filter_map(|id| id.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default();

    if query.is_empty() && tag_ids.is_empty() {
        return Err(ApiError::BadRequest(
            "검색어 또는 태그가 필요합니다".to_string(),
        ));
    }

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let search_type = params.search_type.as_str();
    let has_text_query = !query.is_empty();
    let has_tag_filter = !tag_ids.is_empty();

    let mut results = Vec::new();
    let mut total: u64 = 0;

    let pattern = format!("%{}%", query);

    if search_type == "all" || search_type == "frames" {
        let (count_sql, select_sql) = build_frame_queries(has_text_query, has_tag_filter, &tag_ids);

        let frame_count = state
            .storage
            .count_search_frames(
                &count_sql,
                if has_text_query { Some(&pattern) } else { None },
            )
            .unwrap_or(0);
        total += frame_count;

        let frame_rows = state
            .storage
            .search_frames_with_sql(
                &select_sql,
                if has_text_query { Some(&pattern) } else { None },
                limit,
                offset,
            )
            .map_err(|e| ApiError::Internal(e.to_string()))?;

        let mut frame_results = Vec::with_capacity(frame_rows.len());
        for row in frame_rows {
            let image_url = row
                .file_path
                .as_ref()
                .map(|_| format!("/api/frames/{}/image", row.id));

            let tags = state
                .storage
                .get_tags_for_frame(row.id)
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .into_iter()
                .map(|tag| TagInfo {
                    id: tag.id,
                    name: tag.name,
                    color: tag.color,
                })
                .collect();

            frame_results.push(SearchResult {
                result_type: "frame".to_string(),
                id: row.id.to_string(),
                timestamp: row.timestamp,
                app_name: row.app_name,
                window_title: row.window_title,
                matched_text: row.matched_text,
                image_url,
                importance: row.importance,
                tags: Some(tags),
            });
        }

        results.extend(frame_results);
    }

    if (search_type == "all" || search_type == "events") && has_text_query && !has_tag_filter {
        let event_count = state.storage.count_search_events(&pattern).unwrap_or(0);
        total += event_count;

        let remaining = limit.saturating_sub(results.len());
        if remaining > 0 {
            let event_offset = if search_type == "all" {
                offset.saturating_sub(results.len())
            } else {
                offset
            };

            let event_rows = state
                .storage
                .search_events(&pattern, remaining, event_offset)
                .map_err(|e| ApiError::Internal(e.to_string()))?;

            for row in event_rows {
                results.push(SearchResult {
                    result_type: "event".to_string(),
                    id: row.event_id,
                    timestamp: row.timestamp,
                    app_name: row.app_name,
                    window_title: row.window_title,
                    matched_text: row.data,
                    image_url: None,
                    importance: None,
                    tags: None,
                });
            }
        }
    }

    Ok(Json(SearchResponse {
        query: query.to_string(),
        total,
        offset,
        limit,
        results,
    }))
}

fn build_frame_queries(has_text: bool, has_tags: bool, tag_ids: &[i64]) -> (String, String) {
    let tag_ids_str = tag_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let text_condition = if has_text {
        "(app_name LIKE ?1 OR window_title LIKE ?1 OR ocr_text LIKE ?1)"
    } else {
        "1=1"
    };

    let tag_condition = if has_tags {
        debug_assert!(
            tag_ids.iter().all(|id| id
                .to_string()
                .chars()
                .all(|c| c.is_ascii_digit() || c == '-')),
            "tag_ids에 예상치 못한 문자가 포함됨"
        );
        format!(
            "EXISTS (SELECT 1 FROM frame_tags ft WHERE ft.frame_id = frames.id AND ft.tag_id IN ({}))",
            tag_ids_str
        )
    } else {
        "1=1".to_string()
    };

    let where_clause = format!("{} AND {}", text_condition, tag_condition);

    let count_sql = format!("SELECT COUNT(*) FROM frames WHERE {}", where_clause);

    let select_sql = if has_text {
        format!(
            "SELECT id, timestamp, app_name, window_title, ocr_text, importance, file_path
             FROM frames
             WHERE {}
             ORDER BY timestamp DESC
             LIMIT ?2 OFFSET ?3",
            where_clause
        )
    } else {
        format!(
            "SELECT id, timestamp, app_name, window_title, ocr_text, importance, file_path
             FROM frames
             WHERE {}
             ORDER BY timestamp DESC
             LIMIT ?1 OFFSET ?2",
            where_clause
        )
    };

    (count_sql, select_sql)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_defaults() {
        let json = r#"{"q": "test"}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.q, "test");
        assert_eq!(query.search_type, "all");
        assert!(query.limit.is_none());
        assert!(query.tag_ids.is_none());
    }

    #[test]
    fn search_query_with_tags() {
        let json = r#"{"q": "test", "tag_ids": "1,2,3"}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.q, "test");
        assert_eq!(query.tag_ids, Some("1,2,3".to_string()));
    }

    #[test]
    fn search_query_empty_q_allowed() {
        let json = r#"{"tag_ids": "1,2"}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.q, "");
        assert_eq!(query.tag_ids, Some("1,2".to_string()));
    }

    #[test]
    fn search_result_serializes() {
        let result = SearchResult {
            result_type: "frame".to_string(),
            id: "123".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            app_name: Some("VS Code".to_string()),
            window_title: Some("main.rs".to_string()),
            matched_text: Some("fn main()".to_string()),
            image_url: Some("/api/frames/123/image".to_string()),
            importance: Some(0.85),
            tags: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("VS Code"));
        assert!(json.contains("frame"));
        assert!(!json.contains("tags"));
    }

    #[test]
    fn search_result_with_tags_serializes() {
        let result = SearchResult {
            result_type: "frame".to_string(),
            id: "123".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            app_name: Some("VS Code".to_string()),
            window_title: None,
            matched_text: None,
            image_url: None,
            importance: None,
            tags: Some(vec![TagInfo {
                id: 1,
                name: "work".to_string(),
                color: "#3b82f6".to_string(),
            }]),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("tags"));
        assert!(json.contains("work"));
        assert!(json.contains("#3b82f6"));
    }

    #[test]
    fn search_response_serializes() {
        let response = SearchResponse {
            query: "test".to_string(),
            total: 10,
            offset: 0,
            limit: 50,
            results: vec![],
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"total\":10"));
    }

    #[test]
    fn build_frame_queries_text_only() {
        let (count, select) = build_frame_queries(true, false, &[]);
        assert!(count.contains("app_name LIKE"));
        assert!(select.contains("LIMIT ?2 OFFSET ?3"));
        assert!(!count.contains("frame_tags"));
    }

    #[test]
    fn build_frame_queries_tags_only() {
        let (count, select) = build_frame_queries(false, true, &[1, 2]);
        assert!(count.contains("frame_tags"));
        assert!(count.contains("tag_id IN (1,2)"));
        assert!(select.contains("LIMIT ?1 OFFSET ?2"));
    }

    #[test]
    fn build_frame_queries_both() {
        let (count, select) = build_frame_queries(true, true, &[5]);
        assert!(count.contains("app_name LIKE"));
        assert!(count.contains("frame_tags"));
        assert!(count.contains("tag_id IN (5)"));
        assert!(select.contains("LIMIT ?2 OFFSET ?3"));
    }
}
