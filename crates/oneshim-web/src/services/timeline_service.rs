use chrono::{DateTime, Utc};
use oneshim_core::models::event::Event;

use crate::error::ApiError;
use crate::handlers::timeline::{AppSegment, SessionInfo, TimelineItem, TimelineResponse};
use crate::AppState;

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

pub async fn build_timeline_response(
    state: &AppState,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    max_events: usize,
    max_frames: usize,
) -> Result<TimelineResponse, ApiError> {
    let events = state.storage.get_events(from, to, max_events).await?;
    let frames = state.storage.get_frames(from, to, max_frames)?;
    let idle_periods = state.storage.get_idle_periods(from, to).await?;

    let mut items: Vec<TimelineItem> = Vec::new();

    for event in &events {
        let (event_id, event_type, timestamp, app_name, window_title) = match event {
            Event::User(e) => (
                e.event_id.to_string(),
                format!("{:?}", e.event_type),
                e.timestamp.to_rfc3339(),
                Some(e.app_name.clone()),
                Some(e.window_title.clone()),
            ),
            Event::System(e) => (
                e.event_id.to_string(),
                format!("{:?}", e.event_type),
                e.timestamp.to_rfc3339(),
                None,
                None,
            ),
            Event::Context(e) => (
                format!("ctx_{}", uuid::Uuid::new_v4()),
                "ContextChange".to_string(),
                e.timestamp.to_rfc3339(),
                Some(e.app_name.clone()),
                Some(e.window_title.clone()),
            ),
            Event::Input(e) => (
                format!("input_{}", uuid::Uuid::new_v4()),
                "InputActivity".to_string(),
                e.timestamp.to_rfc3339(),
                Some(e.app_name.clone()),
                None,
            ),
            Event::Process(e) => (
                format!("proc_{}", uuid::Uuid::new_v4()),
                "ProcessSnapshot".to_string(),
                e.timestamp.to_rfc3339(),
                None,
                None,
            ),
            Event::Window(e) => (
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

    items.sort_by(|a, b| get_timestamp(a).cmp(get_timestamp(b)));
    let segments = calculate_app_segments(&items);

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

    Ok(TimelineResponse {
        session,
        items,
        segments,
    })
}

pub(crate) fn app_to_color(app_name: &str) -> String {
    let hash = app_name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    APP_COLORS[hash % APP_COLORS.len()].to_string()
}

pub(crate) fn calculate_app_segments(items: &[TimelineItem]) -> Vec<AppSegment> {
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

        if let Some(last) = segments.last_mut() {
            if last.app_name == app_name {
                last.end = timestamp;
                continue;
            }
        }

        segments.push(AppSegment {
            color: app_to_color(&app_name),
            app_name,
            start: timestamp.clone(),
            end: timestamp,
        });
    }

    segments
}

fn get_timestamp(item: &TimelineItem) -> &str {
    match item {
        TimelineItem::Event { timestamp, .. } => timestamp,
        TimelineItem::Frame { timestamp, .. } => timestamp,
        TimelineItem::IdlePeriod { start, .. } => start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use oneshim_storage::sqlite::SqliteStorage;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    fn test_state() -> AppState {
        let storage = Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory sqlite"));
        let (event_tx, _) = broadcast::channel(8);
        AppState {
            storage,
            frames_dir: None,
            event_tx,
            config_manager: None,
            audit_logger: None,
            automation_controller: None,
            ai_runtime_status: None,
            update_control: None,
        }
    }

    #[tokio::test]
    async fn build_timeline_response_returns_empty_payload_for_empty_store() {
        let state = test_state();
        let from = Utc::now() - chrono::Duration::minutes(30);
        let to = Utc::now();

        let response = build_timeline_response(&state, from, to, 100, 50)
            .await
            .expect("timeline response");

        assert_eq!(response.session.total_events, 0);
        assert_eq!(response.session.total_frames, 0);
        assert_eq!(response.session.total_idle_secs, 0);
        assert!(response.items.is_empty());
        assert!(response.segments.is_empty());
    }
}
