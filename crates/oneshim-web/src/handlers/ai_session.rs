//! AI session REST handlers — session CRUD and SSE message streaming.

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures::stream::Stream;
use futures::StreamExt;
use std::convert::Infallible;
use std::time::Duration;

use oneshim_api_contracts::sessions::{AiSendMessageRequest, AiSessionPath};
use oneshim_core::models::ai_session::{
    ConversationSessionInfo, MessageRole, OutboundMessage, SessionConfig, SessionMessage,
};

use crate::error::ApiError;
use crate::services::web_contexts::AiSessionWebContext;

/// POST /api/ai/sessions — create a new AI conversation session.
pub async fn create_session(
    State(context): State<AiSessionWebContext>,
    Json(config): Json<SessionConfig>,
) -> Result<Json<ConversationSessionInfo>, ApiError> {
    let session_manager = context.session_manager.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("AI session manager is not configured".to_string())
    })?;

    let session = session_manager.create_session(config).await?;
    Ok(Json(session.info()))
}

/// GET /api/ai/sessions — list all active sessions.
pub async fn list_sessions(
    State(context): State<AiSessionWebContext>,
) -> Result<Json<Vec<ConversationSessionInfo>>, ApiError> {
    let session_manager = context.session_manager.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("AI session manager is not configured".to_string())
    })?;

    let sessions = session_manager.list_sessions().await;
    Ok(Json(sessions))
}

/// GET /api/ai/sessions/{id} — get a single session by ID.
pub async fn get_session(
    State(context): State<AiSessionWebContext>,
    Path(path): Path<AiSessionPath>,
) -> Result<Json<ConversationSessionInfo>, ApiError> {
    let session_manager = context.session_manager.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("AI session manager is not configured".to_string())
    })?;

    let session = session_manager.get_session(&path.id).await?;
    Ok(Json(session.info()))
}

/// DELETE /api/ai/sessions/{id} — terminate and remove a session.
pub async fn delete_session(
    State(context): State<AiSessionWebContext>,
    Path(path): Path<AiSessionPath>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session_manager = context.session_manager.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("AI session manager is not configured".to_string())
    })?;

    session_manager.kill_session(&path.id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// POST /api/ai/sessions/{id}/messages — send a message and stream the response via SSE.
pub async fn send_message(
    State(context): State<AiSessionWebContext>,
    Path(path): Path<AiSessionPath>,
    Json(req): Json<AiSendMessageRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let session_manager = context.session_manager.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable("AI session manager is not configured".to_string())
    })?;

    let session = session_manager.get_session(&path.id).await?;

    // Keep session alive — matches Tauri path behavior.
    session_manager.touch_session(&path.id).await;

    let message = SessionMessage {
        role: MessageRole::User,
        content: req.content,
        attachments: req.attachments,
        tools: req.tools,
        context: req.context,
        response_format: req.response_format,
    };

    let mgr_for_stream = session_manager.clone();
    let session_id_for_stream = path.id.clone();
    let response_stream = match session.send_message(&message).await {
        Ok(s) => s,
        Err(err) => {
            session_manager.report_failure(&path.id, &err).await;
            return Err(err.into());
        }
    };

    // Convert ResponseStream items to SSE Events (async for report_failure).
    let sse_stream = response_stream.then(move |item| {
        let mgr = mgr_for_stream.clone();
        let sid = session_id_for_stream.clone();
        async move {
            Ok::<Event, Infallible>(match item {
                Ok(outbound) => match &outbound {
                    OutboundMessage::Text { .. } => Event::default()
                        .event("text")
                        .json_data(&outbound)
                        .unwrap_or_else(|_| {
                            Event::default().event("error").data("serialize error")
                        }),
                    OutboundMessage::Result { .. } => Event::default()
                        .event("result")
                        .json_data(&outbound)
                        .unwrap_or_else(|_| {
                            Event::default().event("error").data("serialize error")
                        }),
                    OutboundMessage::ToolUse { .. } => Event::default()
                        .event("tool_use")
                        .json_data(&outbound)
                        .unwrap_or_else(|_| {
                            Event::default().event("error").data("serialize error")
                        }),
                    OutboundMessage::Error { .. } => Event::default()
                        .event("error")
                        .json_data(&outbound)
                        .unwrap_or_else(|_| Event::default().event("error").data("unknown error")),
                    OutboundMessage::Control { .. } => Event::default()
                        .event("control")
                        .json_data(&outbound)
                        .unwrap_or_else(|_| {
                            Event::default().event("error").data("serialize error")
                        }),
                    OutboundMessage::Thinking { .. } => Event::default()
                        .event("thinking")
                        .json_data(&outbound)
                        .unwrap_or_else(|_| {
                            Event::default().event("error").data("serialize error")
                        }),
                    OutboundMessage::ToolCallDelta { .. } => Event::default()
                        .event("tool_call_delta")
                        .json_data(&outbound)
                        .unwrap_or_else(|_| {
                            Event::default().event("error").data("serialize error")
                        }),
                },
                Err(err) => {
                    let new_state = mgr.report_failure(&sid, &err).await;
                    let retryable =
                        new_state == oneshim_core::models::ai_session::SessionState::Active;
                    let error_msg = OutboundMessage::Error {
                        code: "stream".to_string(),
                        message: err.to_string(),
                        retryable,
                    };
                    Event::default()
                        .event("error")
                        .data(serde_json::to_string(&error_msg).unwrap_or_default())
                }
            })
        }
    });

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}
