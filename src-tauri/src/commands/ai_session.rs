//! Tauri IPC commands for AI conversation session management.
//!
//! Provides create/send/kill/list operations. `send_session_message` spawns a
//! background task that streams `OutboundMessage` events to the frontend via
//! Tauri events on the channel `ai-session:<session_id>`.

use std::collections::HashSet;
use std::sync::Arc;

use futures::StreamExt;
use serde::Deserialize;
use tauri::{command, AppHandle, Emitter};

use oneshim_core::models::ai_session::{
    Attachment, ConversationSessionInfo, MessageContext, MessageRecord, MessageRole,
    OutboundMessage, SessionConfig, SessionMessage, SessionRecord, SessionState, ToolDefinition,
};
use oneshim_core::ports::conversation_session::SessionManager;

use crate::runtime_state::AppState;
use crate::session_manager::SessionManagerImpl;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendSessionMessageRequest {
    pub session_id: String,
    pub message: String,
    pub attachments: Option<Vec<Attachment>>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub context: Option<MessageContext>,
    pub response_format: Option<serde_json::Value>,
}

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

    let system_prompt = config.system_prompt.clone();
    let session = mgr
        .create_session(config)
        .await
        .map_err(|e| e.to_string())?;
    let info = session.info();

    // Fire-and-forget: persist session metadata
    if let Some(ref ss) = state.session_storage {
        let record = SessionRecord {
            session_id: info.session_id.clone(),
            provider_name: info.provider_name.clone(),
            model: info.model.clone(),
            transport: info.transport,
            state: info.state,
            system_prompt,
            turn_count: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: info.created_at,
            last_active: info.last_active,
            terminated_at: None,
        };
        if let Err(e) = ss.save_session(&record).await {
            tracing::warn!("failed to persist session metadata: {e}");
        }
    }

    Ok(info)
}

/// Send a message to an existing session. Spawns a background task that emits
/// `ai-session:<session_id>` Tauri events as `OutboundMessage` chunks arrive.
#[command]
pub async fn send_session_message(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    request: SendSessionMessageRequest,
) -> Result<(), String> {
    let mgr = state
        .session_manager
        .as_ref()
        .ok_or_else(|| "session manager not available".to_string())?;

    // Check daily token budget before sending
    if !mgr.check_token_budget(&request.session_id).await {
        return Err("Daily token budget exhausted".to_string());
    }

    let session = mgr
        .get_session(&request.session_id)
        .await
        .map_err(|e| e.to_string())?;

    // Reset idle timer — keeps the session in Active state.
    mgr.touch_session(&request.session_id).await;

    let user_content = request.message.clone();
    let session_info = session.info(); // capture turn_count before spawn
    let msg = SessionMessage {
        role: MessageRole::User,
        content: request.message,
        attachments: request.attachments.unwrap_or_default(),
        tools: request.tools,
        context: request.context,
        response_format: request.response_format,
    };

    let mgr_clone: Arc<SessionManagerImpl> = mgr.clone();
    let session_storage = state.session_storage.clone();
    let mut stream = match session.send_message(&msg).await {
        Ok(s) => s,
        Err(err) => {
            mgr_clone.report_failure(&request.session_id, &err).await;
            return Err(err.to_string());
        }
    };

    let event_name = format!("ai-session:{}", request.session_id);
    let session_id = request.session_id;

    // Spawn a background task to drain the stream and emit events.
    tokio::spawn(async move {
        let mut assistant_content = String::new();
        let mut assistant_thinking: Option<String> = None;
        let mut assistant_tool_use: Option<String> = None;
        let mut total_input: u64 = 0;
        let mut total_output: u64 = 0;

        while let Some(item) = stream.next().await {
            match item {
                Ok(outbound) => {
                    // Accumulate for persistence
                    match &outbound {
                        OutboundMessage::Text { content, .. } => {
                            assistant_content.push_str(content);
                        }
                        OutboundMessage::Thinking { content, .. } => {
                            assistant_thinking
                                .get_or_insert_with(String::new)
                                .push_str(content);
                        }
                        OutboundMessage::ToolUse { tool, status, .. } => {
                            assistant_tool_use = Some(
                                serde_json::json!({
                                    "tool": tool,
                                    "status": status,
                                })
                                .to_string(),
                            );
                        }
                        OutboundMessage::Result {
                            usage: Some(ref u), ..
                        } => {
                            total_input = u.input_tokens;
                            total_output = u.output_tokens;
                            mgr_clone
                                .accumulate_tokens(&session_id, u.input_tokens, u.output_tokens)
                                .await;
                        }
                        _ => {}
                    }

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
                    let error_msg = OutboundMessage::Error {
                        code: "stream_error".to_string(),
                        message: err.to_string(),
                        retryable,
                    };
                    let _ = app.emit(&event_name, &error_msg);
                    break;
                }
            }
        }

        // Persist messages after stream completes
        if let Some(ref ss) = session_storage {
            if let Ok(seq) = ss.next_seq(&session_id).await {
                let now = chrono::Utc::now();
                let user_msg = MessageRecord {
                    id: None,
                    session_id: session_id.clone(),
                    role: "user".to_string(),
                    content: user_content,
                    thinking: None,
                    tool_use: None,
                    usage_input: None,
                    usage_output: None,
                    created_at: now,
                    seq,
                };
                let assistant_msg = MessageRecord {
                    id: None,
                    session_id: session_id.clone(),
                    role: "assistant".to_string(),
                    content: assistant_content,
                    thinking: assistant_thinking,
                    tool_use: assistant_tool_use,
                    usage_input: Some(total_input),
                    usage_output: Some(total_output),
                    created_at: now,
                    seq: seq + 1,
                };
                if let Err(e) = ss.save_messages(&session_id, &[user_msg, assistant_msg]).await {
                    tracing::warn!("failed to persist messages: {e}");
                }
                // Update session usage
                let _ = ss
                    .update_session_usage(
                        &session_id,
                        total_input,
                        total_output,
                        session_info.turn_count + 1,
                    )
                    .await;
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
        .map_err(|e| e.to_string())?;

    // Fire-and-forget: mark terminated in DB
    if let Some(ref ss) = state.session_storage {
        let _ = ss.terminate_session(&session_id).await;
    }

    Ok(())
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

/// List all AI sessions (active + persisted historical).
#[command]
pub async fn list_ai_sessions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ConversationSessionInfo>, String> {
    let mut result = vec![];

    if let Some(mgr) = &state.session_manager {
        result.extend(mgr.list_sessions().await);
    }

    // Merge persisted (historical) sessions.
    // Reuse max_history_turns (default 100) as the session list limit.
    if let Some(ref ss) = state.session_storage {
        let limit = state.config.ai_session.max_history_turns;
        if let Ok(persisted) = ss.list_sessions(limit).await {
            let active_ids: HashSet<String> =
                result.iter().map(|s| s.session_id.clone()).collect();
            for record in &persisted {
                if !active_ids.contains(&record.session_id) {
                    result.push(ConversationSessionInfo::from(record));
                }
            }
        }
    }

    Ok(result)
}

/// Load conversation history for a session (active or persisted).
#[command]
pub async fn load_session_messages(
    state: tauri::State<'_, AppState>,
    session_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<MessageRecord>, String> {
    let ss = state
        .session_storage
        .as_ref()
        .ok_or_else(|| "session storage not available".to_string())?;

    ss.load_messages(&session_id, limit.unwrap_or(100), offset.unwrap_or(0))
        .await
        .map_err(|e| e.to_string())
}

/// Delete a persisted session and all its messages.
#[command]
pub async fn delete_session_history(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let ss = state
        .session_storage
        .as_ref()
        .ok_or_else(|| "session storage not available".to_string())?;

    ss.delete_session(&session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get token usage for the current day across all sessions.
#[command]
pub async fn get_token_usage(
    state: tauri::State<'_, AppState>,
) -> Result<TokenUsageResponse, String> {
    let mgr = state
        .session_manager
        .as_ref()
        .ok_or_else(|| "session manager not available".to_string())?;

    let (input, output) = mgr.get_global_token_usage().await;
    let budget = mgr.config.daily_token_budget;
    Ok(TokenUsageResponse {
        total_input_tokens: input,
        total_output_tokens: output,
        daily_budget: budget,
        budget_remaining: if budget == 0 {
            None
        } else {
            Some(budget.saturating_sub(input + output))
        },
    })
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageResponse {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub daily_budget: u64,
    pub budget_remaining: Option<u64>,
}
