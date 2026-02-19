//! 검색 API 핸들러.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::AppState;

/// 태그 정보 (검색 결과에 포함)
#[derive(Debug, Serialize, Clone)]
pub struct TagInfo {
    pub id: i64,
    pub name: String,
    pub color: String,
}

/// 검색 쿼리 파라미터
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// 검색어 (빈 문자열 허용 - 태그만으로 검색 가능)
    #[serde(default)]
    pub q: String,
    /// 검색 대상: all, frames, events (기본: all)
    #[serde(default = "default_search_type")]
    pub search_type: String,
    /// 태그 ID 목록 (쉼표 구분)
    pub tag_ids: Option<String>,
    /// 최대 결과 수 (기본: 50)
    pub limit: Option<usize>,
    /// 시작 오프셋 (기본: 0)
    pub offset: Option<usize>,
}

fn default_search_type() -> String {
    "all".to_string()
}

/// 검색 결과 항목
#[derive(Debug, Serialize)]
pub struct SearchResult {
    /// 결과 유형: frame, event
    pub result_type: String,
    /// 항목 ID
    pub id: String,
    /// 타임스탬프 (RFC3339)
    pub timestamp: String,
    /// 앱 이름
    pub app_name: Option<String>,
    /// 창 제목
    pub window_title: Option<String>,
    /// 매칭된 텍스트 (OCR 텍스트 또는 이벤트 데이터)
    pub matched_text: Option<String>,
    /// 이미지 URL (프레임인 경우)
    pub image_url: Option<String>,
    /// 중요도 (프레임인 경우)
    pub importance: Option<f32>,
    /// 태그 목록 (프레임인 경우)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<TagInfo>>,
}

/// 검색 응답
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// 검색어
    pub query: String,
    /// 전체 결과 수
    pub total: u64,
    /// 현재 오프셋
    pub offset: usize,
    /// 요청한 limit
    pub limit: usize,
    /// 검색 결과
    pub results: Vec<SearchResult>,
}

/// 프레임의 태그 목록 조회
fn get_frame_tags(
    conn: &std::sync::MutexGuard<rusqlite::Connection>,
    frame_id: i64,
) -> Vec<TagInfo> {
    let mut stmt = match conn.prepare(
        "SELECT t.id, t.name, t.color
         FROM tags t
         INNER JOIN frame_tags ft ON t.id = ft.tag_id
         WHERE ft.frame_id = ?1
         ORDER BY t.name",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([frame_id], |row| {
        Ok(TagInfo {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
        })
    })
    .map(|iter| iter.flatten().collect())
    .unwrap_or_default()
}

/// GET /api/search - 통합 검색 (태그 필터 지원)
pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, ApiError> {
    let query = params.q.trim();

    // 태그 ID 파싱
    let tag_ids: Vec<i64> = params
        .tag_ids
        .as_ref()
        .map(|s| {
            s.split(',')
                .filter_map(|id| id.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default();

    // 검색어나 태그 중 하나는 필요
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

    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    let mut results = Vec::new();
    let mut total: u64 = 0;

    // LIKE 패턴 생성
    let pattern = format!("%{}%", query);

    // 프레임 검색 (태그 필터는 프레임에만 적용)
    if search_type == "all" || search_type == "frames" {
        // 동적 쿼리 구성
        let (count_sql, select_sql) = build_frame_queries(has_text_query, has_tag_filter, &tag_ids);

        // 프레임 전체 개수
        let frame_count: u64 = if has_text_query && has_tag_filter {
            conn.query_row(&count_sql, [&pattern], |row| row.get(0))
        } else if has_text_query {
            conn.query_row(&count_sql, [&pattern], |row| row.get(0))
        } else {
            conn.query_row(&count_sql, [], |row| row.get(0))
        }
        .unwrap_or(0);
        total += frame_count;

        // 프레임 결과 조회
        let mut stmt = conn
            .prepare(&select_sql)
            .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let frame_results: Vec<SearchResult> = if has_text_query {
            stmt.query_map([&pattern, &limit.to_string(), &offset.to_string()], |row| {
                let id: i64 = row.get(0)?;
                let file_path: Option<String> = row.get(6)?;
                let image_url = file_path
                    .as_ref()
                    .map(|_| format!("/api/frames/{}/image", id));

                Ok((
                    id,
                    SearchResult {
                        result_type: "frame".to_string(),
                        id: id.to_string(),
                        timestamp: row.get(1)?,
                        app_name: row.get(2)?,
                        window_title: row.get(3)?,
                        matched_text: row.get(4)?,
                        image_url,
                        importance: row.get(5)?,
                        tags: None,
                    },
                ))
            })
            .map_err(|e| ApiError::Internal(format!("프레임 검색 실패: {e}")))?
            .flatten()
            .map(|(id, mut result)| {
                result.tags = Some(get_frame_tags(&conn, id));
                result
            })
            .collect()
        } else {
            // 태그만으로 검색 (텍스트 검색 없음)
            stmt.query_map([&limit.to_string(), &offset.to_string()], |row| {
                let id: i64 = row.get(0)?;
                let file_path: Option<String> = row.get(6)?;
                let image_url = file_path
                    .as_ref()
                    .map(|_| format!("/api/frames/{}/image", id));

                Ok((
                    id,
                    SearchResult {
                        result_type: "frame".to_string(),
                        id: id.to_string(),
                        timestamp: row.get(1)?,
                        app_name: row.get(2)?,
                        window_title: row.get(3)?,
                        matched_text: row.get(4)?,
                        image_url,
                        importance: row.get(5)?,
                        tags: None,
                    },
                ))
            })
            .map_err(|e| ApiError::Internal(format!("프레임 검색 실패: {e}")))?
            .flatten()
            .map(|(id, mut result)| {
                result.tags = Some(get_frame_tags(&conn, id));
                result
            })
            .collect()
        };

        results.extend(frame_results);
    }

    // 이벤트 검색 (태그 필터가 없을 때만)
    if (search_type == "all" || search_type == "events") && has_text_query && !has_tag_filter {
        // 이벤트 전체 개수
        let event_count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events
                 WHERE app_name LIKE ?1
                    OR window_title LIKE ?1
                    OR data LIKE ?1",
                [&pattern],
                |row| row.get(0),
            )
            .unwrap_or(0);
        total += event_count;

        // 프레임 결과가 limit 미만이면 이벤트도 조회
        let remaining = limit.saturating_sub(results.len());
        if remaining > 0 {
            let event_offset = if search_type == "all" {
                offset.saturating_sub(results.len())
            } else {
                offset
            };

            let mut stmt = conn
                .prepare(
                    "SELECT event_id, timestamp, event_type, app_name, window_title, data
                     FROM events
                     WHERE app_name LIKE ?1
                        OR window_title LIKE ?1
                        OR data LIKE ?1
                     ORDER BY timestamp DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .map_err(|e| ApiError::Internal(format!("쿼리 준비 실패: {e}")))?;

            let event_results = stmt
                .query_map(
                    [&pattern, &remaining.to_string(), &event_offset.to_string()],
                    |row| {
                        Ok(SearchResult {
                            result_type: "event".to_string(),
                            id: row.get(0)?,
                            timestamp: row.get(1)?,
                            app_name: row.get(3)?,
                            window_title: row.get(4)?,
                            matched_text: row.get(5)?,
                            image_url: None,
                            importance: None,
                            tags: None,
                        })
                    },
                )
                .map_err(|e| ApiError::Internal(format!("이벤트 검색 실패: {e}")))?;

            for result in event_results.flatten() {
                results.push(result);
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

/// 프레임 검색 쿼리 동적 생성
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
        // 안전성 설명: tag_ids는 search() 함수에서 .parse::<i64>().ok()로 파싱되어
        // 이미 유효한 i64 값만 포함됨. rusqlite가 배열 바인딩을 지원하지 않아
        // format!을 사용하지만, i64::to_string()은 숫자와 '-'만 생성하므로
        // SQL 인젝션이 불가능함. 방어적 검증으로 debug_assert 추가.
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
        // tags가 None이면 JSON에 포함되지 않음
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
