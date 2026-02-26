use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub start: String,
    pub end: String,
    pub duration_secs: i64,
    pub total_events: i64,
    pub total_frames: i64,
    pub total_idle_secs: i64,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum TimelineItem {
    Event {
        id: String,
        timestamp: String,
        event_type: String,
        app_name: Option<String>,
        window_title: Option<String>,
    },
    Frame {
        id: i64,
        timestamp: String,
        app_name: String,
        window_title: String,
        importance: f32,
        image_url: String,
    },
    IdlePeriod {
        start: String,
        end: String,
        duration_secs: i64,
    },
}

#[derive(Debug, Serialize)]
pub struct AppSegment {
    pub app_name: String,
    pub start: String,
    pub end: String,
    pub color: String,
}

#[derive(Debug, Serialize)]
pub struct TimelineResponse {
    pub session: SessionInfo,
    pub items: Vec<TimelineItem>,
    pub segments: Vec<AppSegment>,
}

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub max_events: Option<usize>,
    pub max_frames: Option<usize>,
}

impl TimelineQuery {
    pub fn from_datetime(&self) -> DateTime<Utc> {
        self.from
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() - Duration::hours(1))
    }

    pub fn to_datetime(&self) -> DateTime<Utc> {
        self.to
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now)
    }

    pub fn max_events(&self) -> usize {
        self.max_events.unwrap_or(1000)
    }

    pub fn max_frames(&self) -> usize {
        self.max_frames.unwrap_or(500)
    }
}
