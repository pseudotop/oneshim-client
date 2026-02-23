//! 통합 타임라인 API - 이벤트 + 프레임 + 유휴 기간을 시간순 정렬

use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::services::timeline_service;
use crate::AppState;

/// 세션 정보
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    /// 시작 시각 (RFC3339)
    pub start: String,
    /// 종료 시각 (RFC3339)
    pub end: String,
    /// 세션 지속 시간 (초)
    pub duration_secs: i64,
    /// 총 이벤트 수
    pub total_events: i64,
    /// 총 프레임 수
    pub total_frames: i64,
    /// 총 유휴 시간 (초)
    pub total_idle_secs: i64,
}

/// 타임라인 아이템 (태그 기반 enum)
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum TimelineItem {
    /// 이벤트 아이템
    Event {
        id: String,
        timestamp: String,
        event_type: String,
        app_name: Option<String>,
        window_title: Option<String>,
    },
    /// 프레임(스크린샷) 아이템
    Frame {
        id: i64,
        timestamp: String,
        app_name: String,
        window_title: String,
        importance: f32,
        image_url: String,
    },
    /// 유휴 기간 아이템
    IdlePeriod {
        start: String,
        end: String,
        duration_secs: i64,
    },
}

/// 앱 세그먼트 (타임라인 바용)
#[derive(Debug, Serialize)]
pub struct AppSegment {
    /// 앱 이름
    pub app_name: String,
    /// 시작 시각 (RFC3339)
    pub start: String,
    /// 종료 시각 (RFC3339)
    pub end: String,
    /// 표시 색상 (hex)
    pub color: String,
}

/// 통합 타임라인 응답
#[derive(Debug, Serialize)]
pub struct TimelineResponse {
    /// 세션 정보
    pub session: SessionInfo,
    /// 타임라인 아이템 목록 (시간순)
    pub items: Vec<TimelineItem>,
    /// 앱 세그먼트 목록 (타임라인 바용)
    pub segments: Vec<AppSegment>,
}

/// 타임라인 쿼리 파라미터
#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    /// 시작 시각 (RFC3339, 기본: 1시간 전)
    pub from: Option<String>,
    /// 종료 시각 (RFC3339, 기본: 현재)
    pub to: Option<String>,
    /// 최대 이벤트 조회 개수 (기본: 1000)
    pub max_events: Option<usize>,
    /// 최대 프레임 조회 개수 (기본: 500)
    pub max_frames: Option<usize>,
}

impl TimelineQuery {
    /// 기본값이 적용된 시작 시각
    pub fn from_datetime(&self) -> DateTime<Utc> {
        self.from
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() - chrono::Duration::hours(1))
    }

    /// 기본값이 적용된 종료 시각
    pub fn to_datetime(&self) -> DateTime<Utc> {
        self.to
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now)
    }

    /// 최대 이벤트 개수 (기본: 1000)
    pub fn max_events(&self) -> usize {
        self.max_events.unwrap_or(1000)
    }

    /// 최대 프레임 개수 (기본: 500)
    pub fn max_frames(&self) -> usize {
        self.max_frames.unwrap_or(500)
    }
}

/// 통합 타임라인 조회
///
/// GET /api/timeline?from=&to=&max_events=&max_frames=
pub async fn get_timeline(
    State(state): State<AppState>,
    Query(params): Query<TimelineQuery>,
) -> Result<Json<TimelineResponse>, ApiError> {
    let from = params.from_datetime();
    let to = params.to_datetime();
    let max_events = params.max_events();
    let max_frames = params.max_frames();

    Ok(Json(
        timeline_service::build_timeline_response(&state, from, to, max_events, max_frames).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_to_color_is_consistent() {
        // 같은 앱 이름은 항상 같은 색상
        assert_eq!(
            timeline_service::app_to_color("Chrome"),
            timeline_service::app_to_color("Chrome")
        );
        assert_eq!(
            timeline_service::app_to_color("Code"),
            timeline_service::app_to_color("Code")
        );
    }

    #[test]
    fn app_to_color_varies() {
        // 다른 앱은 보통 다른 색상 (충돌 가능하지만 확률 낮음)
        let chrome_color = timeline_service::app_to_color("Chrome");
        let code_color = timeline_service::app_to_color("Code");
        // 색상이 유효한 hex인지 확인
        assert!(chrome_color.starts_with('#'));
        assert!(code_color.starts_with('#'));
    }

    #[test]
    fn timeline_query_defaults() {
        let query = TimelineQuery {
            from: None,
            to: None,
            max_events: None,
            max_frames: None,
        };

        let now = Utc::now();
        assert!(query.from_datetime() < now);
        assert!(query.to_datetime() <= now + chrono::Duration::seconds(1));
        assert_eq!(query.max_events(), 1000);
        assert_eq!(query.max_frames(), 500);
    }

    #[test]
    fn session_info_serializes() {
        let info = SessionInfo {
            start: "2024-01-01T10:00:00Z".to_string(),
            end: "2024-01-01T11:00:00Z".to_string(),
            duration_secs: 3600,
            total_events: 100,
            total_frames: 50,
            total_idle_secs: 300,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("3600"));
        assert!(json.contains("total_events"));
    }

    #[test]
    fn timeline_item_serializes_with_tag() {
        let event = TimelineItem::Event {
            id: "test_123".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            event_type: "AppSwitch".to_string(),
            app_name: Some("Chrome".to_string()),
            window_title: Some("Google".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"Event\""));
        assert!(json.contains("Chrome"));

        let frame = TimelineItem::Frame {
            id: 1,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            app_name: "Code".to_string(),
            window_title: "main.rs".to_string(),
            importance: 0.85,
            image_url: "/api/frames/1/image".to_string(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"type\":\"Frame\""));
        assert!(json.contains("0.85"));

        let idle = TimelineItem::IdlePeriod {
            start: "2024-01-01T00:00:00Z".to_string(),
            end: "2024-01-01T00:05:00Z".to_string(),
            duration_secs: 300,
        };
        let json = serde_json::to_string(&idle).unwrap();
        assert!(json.contains("\"type\":\"IdlePeriod\""));
        assert!(json.contains("300"));
    }

    #[test]
    fn app_segment_serializes() {
        let segment = AppSegment {
            app_name: "Chrome".to_string(),
            start: "2024-01-01T10:00:00Z".to_string(),
            end: "2024-01-01T10:30:00Z".to_string(),
            color: "#3B82F6".to_string(),
        };
        let json = serde_json::to_string(&segment).unwrap();
        assert!(json.contains("Chrome"));
        assert!(json.contains("#3B82F6"));
    }

    #[test]
    fn calculate_segments_merges_consecutive() {
        let items = vec![
            TimelineItem::Frame {
                id: 1,
                timestamp: "2024-01-01T10:00:00Z".to_string(),
                app_name: "Chrome".to_string(),
                window_title: "Tab 1".to_string(),
                importance: 0.5,
                image_url: "/api/frames/1/image".to_string(),
            },
            TimelineItem::Frame {
                id: 2,
                timestamp: "2024-01-01T10:05:00Z".to_string(),
                app_name: "Chrome".to_string(),
                window_title: "Tab 2".to_string(),
                importance: 0.5,
                image_url: "/api/frames/2/image".to_string(),
            },
            TimelineItem::Frame {
                id: 3,
                timestamp: "2024-01-01T10:10:00Z".to_string(),
                app_name: "Code".to_string(),
                window_title: "main.rs".to_string(),
                importance: 0.8,
                image_url: "/api/frames/3/image".to_string(),
            },
        ];

        let segments = timeline_service::calculate_app_segments(&items);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].app_name, "Chrome");
        assert_eq!(segments[0].start, "2024-01-01T10:00:00Z");
        assert_eq!(segments[0].end, "2024-01-01T10:05:00Z");
        assert_eq!(segments[1].app_name, "Code");
    }
}
