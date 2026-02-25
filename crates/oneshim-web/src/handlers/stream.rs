use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::Serialize;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum RealtimeEvent {
    #[serde(rename = "metrics")]
    Metrics(MetricsUpdate),
    #[serde(rename = "frame")]
    Frame(FrameUpdate),
    #[serde(rename = "idle")]
    Idle(IdleUpdate),
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsUpdate {
    pub timestamp: String,
    pub cpu_usage: f32,
    pub memory_percent: f32,
    pub memory_used: u64,
    pub memory_total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameUpdate {
    pub id: i64,
    pub timestamp: String,
    pub app_name: String,
    pub window_title: String,
    pub importance: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdleUpdate {
    pub is_idle: bool,
    pub idle_secs: u64,
}

///
/// GET /api/stream
///
pub async fn event_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx);

    let sse_stream = stream.filter_map(|result| {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).ok()?;
                let sse_event = Event::default().event(event_type_name(&event)).data(json);
                Some(Ok(sse_event))
            }
            Err(_) => None, // skip on channel lag
        }
    });

    Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

fn event_type_name(event: &RealtimeEvent) -> &'static str {
    match event {
        RealtimeEvent::Metrics(_) => "metrics",
        RealtimeEvent::Frame(_) => "frame",
        RealtimeEvent::Idle(_) => "idle",
        RealtimeEvent::Ping => "ping",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_metrics_event() {
        let event = RealtimeEvent::Metrics(MetricsUpdate {
            timestamp: "2024-01-30T12:00:00Z".to_string(),
            cpu_usage: 45.5,
            memory_percent: 68.2,
            memory_used: 8_000_000_000,
            memory_total: 16_000_000_000,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"metrics\""));
        assert!(json.contains("\"cpu_usage\":45.5"));
    }

    #[test]
    fn serialize_frame_event() {
        let event = RealtimeEvent::Frame(FrameUpdate {
            id: 123,
            timestamp: "2024-01-30T12:00:00Z".to_string(),
            app_name: "VS Code".to_string(),
            window_title: "main.rs".to_string(),
            importance: 0.85,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"frame\""));
        assert!(json.contains("\"app_name\":\"VS Code\""));
    }

    #[test]
    fn serialize_idle_event() {
        let event = RealtimeEvent::Idle(IdleUpdate {
            is_idle: true,
            idle_secs: 300,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"idle\""));
        assert!(json.contains("\"is_idle\":true"));
    }

    #[test]
    fn event_type_names() {
        assert_eq!(
            event_type_name(&RealtimeEvent::Metrics(MetricsUpdate {
                timestamp: String::new(),
                cpu_usage: 0.0,
                memory_percent: 0.0,
                memory_used: 0,
                memory_total: 0,
            })),
            "metrics"
        );
        assert_eq!(event_type_name(&RealtimeEvent::Ping), "ping");
    }
}
