//! Tauri IPC commands for AI conversation session management.
//!
//! Provides create/send/kill/list operations. `send_session_message` spawns a
//! background task that streams `OutboundMessage` events to the frontend via
//! Tauri events on the channel `ai-session:<session_id>`.

use std::sync::Arc;

use futures::StreamExt;
use tauri::{command, AppHandle, Emitter};

use oneshim_core::models::ai_session::{
    ConversationSessionInfo, MessageRole, SessionConfig, SessionMessage, SessionState,
};
use oneshim_core::ports::conversation_session::SessionManager;

use crate::runtime_state::AppState;
use crate::session_manager::SessionManagerImpl;

/// Create a new AI conversation session.
#[command]
pub async fn create_ai_session(
    state: tauri::State<'_, AppState>,
    config: SessionConfig,
) -> Result<ConversationSessionInfo, String> {
    let mgr = state
        .session_manager
        .as_ref()
        .ok_or_else(|| "session manager not available".to_string())?;

    let session = mgr
        .create_session(config)
        .await
        .map_err(|e| e.to_string())?;
    Ok(session.info())
}

/// Send a message to an existing session. Spawns a background task that emits
/// `ai-session:<session_id>` Tauri events as `OutboundMessage` chunks arrive.
#[command]
pub async fn send_session_message(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    message: String,
) -> Result<(), String> {
    let mgr = state
        .session_manager
        .as_ref()
        .ok_or_else(|| "session manager not available".to_string())?;

    let session = mgr
        .get_session(&session_id)
        .await
        .map_err(|e| e.to_string())?;

    // Reset idle timer — keeps the session in Active state.
    mgr.touch_session(&session_id).await;

    let msg = SessionMessage {
        role: MessageRole::User,
        content: message,
        attachments: vec![],
        tools: None,
        context: None,
    };

    let mgr_clone: Arc<SessionManagerImpl> = mgr.clone();
    let mut stream = match session.send_message(&msg).await {
        Ok(s) => s,
        Err(err) => {
            mgr_clone.report_failure(&session_id, &err).await;
            return Err(err.to_string());
        }
    };

    let event_name = format!("ai-session:{session_id}");

    // Spawn a background task to drain the stream and emit events.
    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            match item {
                Ok(outbound) => {
                    if let Err(e) = app.emit(&event_name, &outbound) {
                        tracing::warn!(
                            session_id = %session_id,
                            "failed to emit ai-session event: {e}"
                        );
                        break;
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        session_id = %session_id,
                        "stream error: {err}"
                    );
                    let new_state = mgr_clone.report_failure(&session_id, &err).await;
                    let retryable = new_state == SessionState::Active;
                    let error_msg = oneshim_core::models::ai_session::OutboundMessage::Error {
                        code: "stream_error".to_string(),
                        message: err.to_string(),
                        retryable,
                    };
                    let _ = app.emit(&event_name, &error_msg);
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Terminate an active AI session.
#[command]
pub async fn kill_ai_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let mgr = state
        .session_manager
        .as_ref()
        .ok_or_else(|| "session manager not available".to_string())?;

    mgr.kill_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Retry (recover) a failed or errored session. Increments retry_count and
/// returns the session info if successful. Fails when max retries exceeded.
#[command]
pub async fn retry_ai_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<ConversationSessionInfo, String> {
    let mgr = state
        .session_manager
        .as_ref()
        .ok_or_else(|| "session manager not available".to_string())?;

    let session = mgr
        .recover_session(&session_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(session.info())
}

/// List all active AI sessions.
#[command]
pub async fn list_ai_sessions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ConversationSessionInfo>, String> {
    let mgr = state
        .session_manager
        .as_ref()
        .ok_or_else(|| "session manager not available".to_string())?;

    Ok(mgr.list_sessions().await)
}
