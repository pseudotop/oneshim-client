//! 통합 타임라인 API - 이벤트 + 프레임 + 유휴 기간을 시간순 정렬

use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use oneshim_core::ports::storage::{MetricsStorage, StorageService};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
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

/// 앱 이름 → 색상 매핑 (해시 기반)
const APP_COLORS: &[&str] = &[
    "#3B82F6", // blue
    "#10B981", // green
    "#F59E0B", // amber
    "#EF4444", // red
    "#8B5CF6", // purple
    "#EC4899", // pink
    "#06B6D4", // cyan
    "#84CC16", // lime
];

/// 앱 이름을 해시하여 색상 반환
fn app_to_color(app_name: &str) -> String {
    let hash = app_name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    APP_COLORS[hash % APP_COLORS.len()].to_string()
}

/// TimelineItem에서 타임스탬프 추출 (정렬용)
fn get_timestamp(item: &TimelineItem) -> &str {
    match item {
        TimelineItem::Event { timestamp, .. } => timestamp,
        TimelineItem::Frame { timestamp, .. } => timestamp,
        TimelineItem::IdlePeriod { start, .. } => start,
    }
}

/// 앱 세그먼트 계산 (연속적인 앱 사용 기간)
fn calculate_app_segments(items: &[TimelineItem]) -> Vec<AppSegment> {
    let mut segments: Vec<AppSegment> = Vec::new();

    for item in items {
        let (app_name, timestamp) = match item {
            TimelineItem::Event {
                app_name: Some(name),
                timestamp,
                ..
            } => (name.clone(), timestamp.clone()),
            TimelineItem::Frame {
                app_name,
                timestamp,
                ..
            } => (app_name.clone(), timestamp.clone()),
            _ => continue,
        };

        // 마지막 세그먼트와 같은 앱이면 연장
        if let Some(last) = segments.last_mut() {
            if last.app_name == app_name {
                last.end = timestamp;
                continue;
            }
        }

        // 새 세그먼트 시작
        segments.push(AppSegment {
            color: app_to_color(&app_name),
            app_name,
            start: timestamp.clone(),
            end: timestamp,
        });
    }

    segments
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

    // 1. 이벤트, 프레임, 유휴 기간 조회
    let events = state.storage.get_events(from, to, max_events).await?;
    let frames = state.storage.get_frames(from, to, max_frames)?;
    let idle_periods = state.storage.get_idle_periods(from, to).await?;

    // 2. TimelineItem으로 변환
    let mut items: Vec<TimelineItem> = Vec::new();

    // 이벤트 변환
    for event in &events {
        let (event_id, event_type, timestamp, app_name, window_title) = match event {
            oneshim_core::models::event::Event::User(e) => (
                e.event_id.to_string(),
                format!("{:?}", e.event_type),
                e.timestamp.to_rfc3339(),
                Some(e.app_name.clone()),
                Some(e.window_title.clone()),
            ),
            oneshim_core::models::event::Event::System(e) => (
                e.event_id.to_string(),
                format!("{:?}", e.event_type),
                e.timestamp.to_rfc3339(),
                None,
                None,
            ),
            oneshim_core::models::event::Event::Context(e) => (
                format!("ctx_{}", uuid::Uuid::new_v4()),
                "ContextChange".to_string(),
                e.timestamp.to_rfc3339(),
                Some(e.app_name.clone()),
                Some(e.window_title.clone()),
            ),
            oneshim_core::models::event::Event::Input(e) => (
                format!("input_{}", uuid::Uuid::new_v4()),
                "InputActivity".to_string(),
                e.timestamp.to_rfc3339(),
                Some(e.app_name.clone()),
                None,
            ),
            oneshim_core::models::event::Event::Process(e) => (
                format!("proc_{}", uuid::Uuid::new_v4()),
                "ProcessSnapshot".to_string(),
                e.timestamp.to_rfc3339(),
                None,
                None,
            ),
            oneshim_core::models::event::Event::Window(e) => (
                format!("win_{}", uuid::Uuid::new_v4()),
                format!("{:?}", e.event_type),
                e.timestamp.to_rfc3339(),
                Some(e.window.app_name.clone()),
                Some(e.window.window_title.clone()),
            ),
        };

        items.push(TimelineItem::Event {
            id: event_id,
            timestamp,
            event_type,
            app_name,
            window_title,
        });
    }

    // 프레임 변환
    for frame in &frames {
        items.push(TimelineItem::Frame {
            id: frame.id,
            timestamp: frame.timestamp.clone(),
            app_name: frame.app_name.clone(),
            window_title: frame.window_title.clone(),
            importance: frame.importance,
            image_url: format!("/api/frames/{}/image", frame.id),
        });
    }

    // 유휴 기간 변환
    for idle in &idle_periods {
        if let Some(end_time) = idle.end_time {
            if let Some(duration) = idle.duration_secs {
                items.push(TimelineItem::IdlePeriod {
                    start: idle.start_time.to_rfc3339(),
                    end: end_time.to_rfc3339(),
                    duration_secs: duration as i64,
                });
            }
        }
    }

    // 3. 시간순 정렬
    items.sort_by(|a, b| get_timestamp(a).cmp(get_timestamp(b)));

    // 4. 앱 세그먼트 계산
    let segments = calculate_app_segments(&items);

    // 5. 세션 정보 계산
    let total_idle_secs: i64 = idle_periods
        .iter()
        .filter_map(|i| i.duration_secs.map(|d| d as i64))
        .sum();

    let session = SessionInfo {
        start: from.to_rfc3339(),
        end: to.to_rfc3339(),
        duration_secs: (to - from).num_seconds(),
        total_events: events.len() as i64,
        total_frames: frames.len() as i64,
        total_idle_secs,
    };

    Ok(Json(TimelineResponse {
        session,
        items,
        segments,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_to_color_is_consistent() {
        // 같은 앱 이름은 항상 같은 색상
        assert_eq!(app_to_color("Chrome"), app_to_color("Chrome"));
        assert_eq!(app_to_color("Code"), app_to_color("Code"));
    }

    #[test]
    fn app_to_color_varies() {
        // 다른 앱은 보통 다른 색상 (충돌 가능하지만 확률 낮음)
        let chrome_color = app_to_color("Chrome");
        let code_color = app_to_color("Code");
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

        let segments = calculate_app_segments(&items);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].app_name, "Chrome");
        assert_eq!(segments[0].start, "2024-01-01T10:00:00Z");
        assert_eq!(segments[0].end, "2024-01-01T10:05:00Z");
        assert_eq!(segments[1].app_name, "Code");
    }
}
