//! SSE(Server-Sent Events) 스트림 클라이언트.
//!
//! `SseClient` 포트 구현. 자동 재연결 + exponential backoff.

use async_trait::async_trait;
use futures::stream::StreamExt;
use oneshim_core::error::CoreError;
use oneshim_core::models::suggestion::Suggestion;
use oneshim_core::ports::api_client::{SseClient, SseEvent};
use reqwest_eventsource::{Event, EventSource};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::auth::TokenManager;

/// SSE 스트림 클라이언트 — `SseClient` 포트 구현
pub struct SseStreamClient {
    base_url: String,
    token_manager: Arc<TokenManager>,
    max_retry_secs: u64,
    http_client: reqwest::Client,
}

impl SseStreamClient {
    /// 새 SSE 클라이언트 생성
    pub fn new(base_url: &str, token_manager: Arc<TokenManager>, max_retry_secs: u64) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token_manager,
            max_retry_secs,
            http_client: reqwest::Client::new(),
        }
    }

    /// SSE 이벤트 데이터를 SseEvent로 파싱
    fn parse_event(event_type: &str, data: &str) -> Option<SseEvent> {
        match event_type {
            "connection" => {
                let val: serde_json::Value = serde_json::from_str(data).ok()?;
                let session_id = val.get("session_id")?.as_str()?.to_string();
                Some(SseEvent::Connected { session_id })
            }
            "suggestion" => {
                let suggestion: Suggestion = serde_json::from_str(data).ok()?;
                Some(SseEvent::Suggestion(suggestion))
            }
            "update" => {
                let val: serde_json::Value = serde_json::from_str(data).ok()?;
                Some(SseEvent::Update(val))
            }
            "heartbeat" => {
                let val: serde_json::Value = serde_json::from_str(data).ok()?;
                let ts_str = val.get("timestamp")?.as_str()?;
                let timestamp = chrono::DateTime::parse_from_rfc3339(ts_str)
                    .ok()?
                    .with_timezone(&chrono::Utc);
                Some(SseEvent::Heartbeat { timestamp })
            }
            "error" => {
                let msg = data.to_string();
                Some(SseEvent::Error(msg))
            }
            "close" => Some(SseEvent::Close),
            // 기본 이벤트 타입 "message" 처리
            "message" => {
                // 일반 메시지는 JSON으로 파싱 시도
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                    Some(SseEvent::Update(val))
                } else {
                    debug!("일반 메시지 수신: {data}");
                    None
                }
            }
            _ => {
                debug!("알 수 없는 SSE 이벤트 타입: {event_type}");
                None
            }
        }
    }
}

#[async_trait]
impl SseClient for SseStreamClient {
    async fn connect(&self, session_id: &str, tx: mpsc::Sender<SseEvent>) -> Result<(), CoreError> {
        let url = format!(
            "{}/user_context/sessions/stream?session_id={}",
            self.base_url, session_id
        );
        let max_retry = self.max_retry_secs;

        info!("SSE 연결 시작: {url}");

        let mut retry_delay = 1u64;

        loop {
            let token = self.token_manager.get_token().await?;

            // reqwest-eventsource로 SSE 연결 생성
            let request = self
                .http_client
                .get(&url)
                .header("Authorization", format!("Bearer {token}"));

            let mut es = EventSource::new(request)
                .map_err(|e| CoreError::Internal(format!("SSE 연결 생성 실패: {e}")))?;

            loop {
                match es.next().await {
                    Some(Ok(event)) => match event {
                        Event::Open => {
                            debug!("SSE 연결 수립됨");
                            // 연결 성공 시 재시도 지연 리셋
                            retry_delay = 1;
                        }
                        Event::Message(msg) => {
                            let event_type = if msg.event.is_empty() {
                                "message"
                            } else {
                                &msg.event
                            };

                            if let Some(sse_event) = Self::parse_event(event_type, &msg.data) {
                                if tx.send(sse_event).await.is_err() {
                                    info!("SSE 이벤트 채널 닫힘, 연결 종료");
                                    es.close();
                                    return Ok(());
                                }
                            }
                        }
                    },
                    Some(Err(e)) => {
                        warn!("SSE 스트림 에러: {e}");
                        es.close();
                        break;
                    }
                    None => {
                        info!("SSE 스트림 종료");
                        break;
                    }
                }
            }

            // 채널이 닫혔으면 종료
            if tx.is_closed() {
                return Ok(());
            }

            // exponential backoff 재연결
            warn!("SSE 재연결 대기: {retry_delay}초");
            tokio::time::sleep(Duration::from_secs(retry_delay)).await;
            retry_delay = (retry_delay * 2).min(max_retry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_connection_event() {
        let data = r#"{"session_id": "sess_123"}"#;
        let event = SseStreamClient::parse_event("connection", data);
        assert!(
            matches!(event, Some(SseEvent::Connected { session_id }) if session_id == "sess_123")
        );
    }

    #[test]
    fn parse_suggestion_event() {
        let data = r#"{
            "suggestion_id": "sug_001",
            "suggestion_type": "WORK_GUIDANCE",
            "content": "테스트 제안",
            "priority": "HIGH",
            "confidence_score": 0.95,
            "relevance_score": 0.88,
            "is_actionable": true,
            "created_at": "2026-01-28T10:00:00Z"
        }"#;
        let event = SseStreamClient::parse_event("suggestion", data);
        assert!(matches!(event, Some(SseEvent::Suggestion(_))));
    }

    #[test]
    fn parse_heartbeat_event() {
        let data = r#"{"timestamp": "2026-01-28T10:00:00Z"}"#;
        let event = SseStreamClient::parse_event("heartbeat", data);
        assert!(matches!(event, Some(SseEvent::Heartbeat { .. })));
    }

    #[test]
    fn parse_error_event() {
        let event = SseStreamClient::parse_event("error", "서버 에러");
        assert!(matches!(event, Some(SseEvent::Error(_))));
    }

    #[test]
    fn parse_close_event() {
        let event = SseStreamClient::parse_event("close", "");
        assert!(matches!(event, Some(SseEvent::Close)));
    }

    #[test]
    fn parse_unknown_event() {
        let event = SseStreamClient::parse_event("unknown_type", "data");
        assert!(event.is_none());
    }

    #[test]
    fn parse_message_event_json() {
        let data = r#"{"key": "value"}"#;
        let event = SseStreamClient::parse_event("message", data);
        assert!(matches!(event, Some(SseEvent::Update(_))));
    }

    #[test]
    fn parse_message_event_non_json() {
        let event = SseStreamClient::parse_event("message", "plain text");
        assert!(event.is_none());
    }
}
