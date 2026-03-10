use axum::extract::{Query, State};
use axum::Json;
use oneshim_api_contracts::events::EventResponse;
use oneshim_core::models::event::Event;

use crate::error::ApiError;
use crate::AppState;

use super::{PaginatedResponse, PaginationMeta, TimeRangeQuery};

fn event_response_from_event(event: Event) -> EventResponse {
    match event {
        Event::User(e) => EventResponse {
            event_id: e.event_id.to_string(),
            event_type: "User".to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            app_name: Some(e.app_name.clone()),
            window_title: Some(e.window_title.clone()),
            data: serde_json::json!({
                "event_type": format!("{:?}", e.event_type),
                "app_name": e.app_name,
                "window_title": e.window_title,
            }),
        },
        Event::System(e) => EventResponse {
            event_id: e.event_id.to_string(),
            event_type: "System".to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            app_name: None,
            window_title: None,
            data: serde_json::json!({
                "event_type": format!("{:?}", e.event_type),
                "data": e.data,
            }),
        },
        Event::Context(e) => EventResponse {
            event_id: format!("ctx_{}", uuid::Uuid::new_v4()),
            event_type: "Context".to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            app_name: Some(e.app_name.clone()),
            window_title: Some(e.window_title.clone()),
            data: serde_json::json!({
                "app_name": e.app_name,
                "window_title": e.window_title,
                "prev_app_name": e.prev_app_name,
            }),
        },
        Event::Input(e) => EventResponse {
            event_id: format!("input_{}", uuid::Uuid::new_v4()),
            event_type: "Input".to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            app_name: Some(e.app_name.clone()),
            window_title: None,
            data: serde_json::json!({
                "period_secs": e.period_secs,
                "mouse": {
                    "click_count": e.mouse.click_count,
                    "move_distance": e.mouse.move_distance,
                    "scroll_count": e.mouse.scroll_count,
                    "double_click_count": e.mouse.double_click_count,
                    "right_click_count": e.mouse.right_click_count,
                },
                "keyboard": {
                    "keystrokes_per_min": e.keyboard.keystrokes_per_min,
                    "total_keystrokes": e.keyboard.total_keystrokes,
                    "typing_bursts": e.keyboard.typing_bursts,
                    "shortcut_count": e.keyboard.shortcut_count,
                    "correction_count": e.keyboard.correction_count,
                },
            }),
        },
        Event::Process(e) => EventResponse {
            event_id: format!("proc_{}", uuid::Uuid::new_v4()),
            event_type: "Process".to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            app_name: None,
            window_title: None,
            data: serde_json::json!({
                "total_process_count": e.total_process_count,
                "processes": e.processes.iter().map(|p| serde_json::json!({
                    "name": p.name,
                    "pid": p.pid,
                    "cpu_percent": p.cpu_percent,
                    "memory_mb": p.memory_mb,
                    "window_count": p.window_count,
                    "is_foreground": p.is_foreground,
                })).collect::<Vec<_>>(),
            }),
        },
        Event::Window(e) => EventResponse {
            event_id: format!("win_{}", uuid::Uuid::new_v4()),
            event_type: "Window".to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            app_name: Some(e.window.app_name.clone()),
            window_title: Some(e.window.window_title.clone()),
            data: serde_json::json!({
                "event_type": format!("{:?}", e.event_type),
                "position": e.window.position,
                "size": e.window.size,
                "screen_ratio": e.window.screen_ratio,
                "is_fullscreen": e.window.is_fullscreen,
                "z_order": e.window.z_order,
                "screen_resolution": e.screen_resolution,
                "monitor_index": e.monitor_index,
            }),
        },
        Event::Clipboard(e) => EventResponse {
            event_id: format!("clip_{}", uuid::Uuid::new_v4()),
            event_type: "Clipboard".to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            app_name: None,
            window_title: None,
            data: serde_json::json!({
                "content_type": format!("{:?}", e.content_type),
                "char_count": e.char_count,
                "preview": e.preview,
            }),
        },
        Event::FileAccess(e) => EventResponse {
            event_id: format!("fa_{}", uuid::Uuid::new_v4()),
            event_type: "FileAccess".to_string(),
            timestamp: e.timestamp.to_rfc3339(),
            app_name: None,
            window_title: None,
            data: serde_json::json!({
                "event_type": format!("{:?}", e.event_type),
                "relative_path": e.relative_path.display().to_string(),
                "extension": e.extension,
            }),
        },
    }
}

/// GET /api/events?from=&to=&limit=&offset=
pub async fn get_events(
    State(state): State<AppState>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<Json<PaginatedResponse<EventResponse>>, ApiError> {
    let from = params.from_datetime();
    let to = params.to_datetime();
    let limit = params.limit_or_default();
    let offset = params.offset_or_default();

    let total = state
        .storage
        .count_events_in_range(&from.to_rfc3339(), &to.to_rfc3339())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let fetch_limit = limit + offset;
    let events = state.storage.get_events(from, to, fetch_limit).await?;

    let data: Vec<EventResponse> = events
        .into_iter()
        .skip(offset)
        .map(event_response_from_event)
        .collect();

    let has_more = (offset + data.len()) < total as usize;

    Ok(Json(PaginatedResponse {
        data,
        pagination: PaginationMeta {
            total,
            offset,
            limit,
            has_more,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_response_serializes() {
        let response = EventResponse {
            event_id: "test_123".to_string(),
            event_type: "User".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            app_name: Some("Code".to_string()),
            window_title: Some("main.rs".to_string()),
            data: serde_json::json!({"event_type": "WindowChange"}),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("WindowChange"));
    }
}
