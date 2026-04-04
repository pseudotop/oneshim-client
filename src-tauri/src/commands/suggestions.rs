use futures::StreamExt;
use serde::Serialize;
use tauri::command;
use tauri::{AppHandle, Emitter};
use tokio::time::{timeout, Duration};

use oneshim_core::models::ai_session::{
    MessageRecord, MessageRole, OutboundMessage, SessionMessage, SessionState,
};
use oneshim_core::ports::conversation_session::SessionManager;

use crate::commands::suggestion_parser::try_extract_suggestions;
use crate::runtime_state::{AiSessionRuntimeState, SuggestionRuntimeState};

#[derive(Serialize)]
pub struct SuggestionViewDto {
    pub id: String,
    pub title: String,
    pub body: String,
    pub priority: String,
    pub category: Option<String>,
    pub source: String,
    pub confidence_score: f64,
    pub created_at: String,
    pub is_read: bool,
    pub reasoning: Option<String>,
}

#[derive(Serialize)]
pub struct SuggestionHistoryDto {
    #[serde(flatten)]
    pub suggestion: SuggestionViewDto,
    pub feedback: Option<String>,
}

fn source_label(source: &oneshim_core::models::suggestion::SuggestionSource) -> &'static str {
    match source {
        oneshim_core::models::suggestion::SuggestionSource::LlmServer => "server",
        oneshim_core::models::suggestion::SuggestionSource::LlmLocal => "local",
        oneshim_core::models::suggestion::SuggestionSource::RuleBased => "local",
    }
}

#[command]
pub async fn get_pending_suggestions(
    state: tauri::State<'_, SuggestionRuntimeState>,
) -> Result<Vec<SuggestionViewDto>, String> {
    let mgr = state.manager().ok_or("Suggestions not available")?;

    // Collect suggestions from queue into a Vec first, then drop the queue lock
    // BEFORE calling is_read() — is_read() acquires its own lock (read_ids),
    // and holding both would cause a nested lock.
    let snapshot: Vec<_> = {
        let queue = mgr.queue().lock().await;
        queue
            .iter()
            .map(|s| {
                (
                    s.suggestion_id.clone(),
                    oneshim_suggestion::presenter::type_to_title(&s.suggestion_type),
                    s.content.clone(),
                    format!("{:?}", s.priority).to_lowercase(),
                    source_label(&s.source).to_string(),
                    s.confidence_score,
                    s.created_at.to_rfc3339(),
                    s.reasoning.clone(),
                )
            })
            .collect()
    }; // queue lock dropped here

    let mut results = Vec::with_capacity(snapshot.len());
    for (id, title, body, priority, source, confidence_score, created_at, reasoning) in snapshot {
        let is_read = mgr.is_read(&id).await;
        results.push(SuggestionViewDto {
            id,
            title,
            body,
            priority,
            category: None, // Suggestion has no category field
            source,
            confidence_score,
            created_at,
            is_read,
            reasoning,
        });
    }
    Ok(results)
}

#[command]
pub async fn get_suggestion_history(
    state: tauri::State<'_, SuggestionRuntimeState>,
    limit: Option<u32>,
) -> Result<Vec<SuggestionHistoryDto>, String> {
    let mgr = state.manager().ok_or("Suggestions not available")?;

    // Snapshot history entries and drop lock before calling is_read()
    let snapshot: Vec<_> = {
        let history = mgr.history().lock().await;
        history
            .recent(limit.unwrap_or(50) as usize)
            .into_iter()
            .cloned()
            .collect()
    }; // history lock dropped here

    let mut results = Vec::with_capacity(snapshot.len());
    for entry in snapshot {
        let is_read = mgr.is_read(&entry.suggestion.suggestion_id).await;
        let feedback = entry.feedback.as_ref().map(|f| match f {
            oneshim_core::models::suggestion::FeedbackType::Accepted => "accepted".to_string(),
            oneshim_core::models::suggestion::FeedbackType::Rejected => "rejected".to_string(),
            oneshim_core::models::suggestion::FeedbackType::Deferred => "deferred".to_string(),
        });
        results.push(SuggestionHistoryDto {
            suggestion: SuggestionViewDto {
                id: entry.suggestion.suggestion_id.clone(),
                title: oneshim_suggestion::presenter::type_to_title(
                    &entry.suggestion.suggestion_type,
                ),
                body: entry.suggestion.content.clone(),
                priority: format!("{:?}", entry.suggestion.priority).to_lowercase(),
                category: None,
                source: source_label(&entry.suggestion.source).to_string(),
                confidence_score: entry.suggestion.confidence_score,
                created_at: entry.suggestion.created_at.to_rfc3339(),
                is_read,
                reasoning: entry.suggestion.reasoning.clone(),
            },
            feedback,
        });
    }
    Ok(results)
}

#[command]
pub async fn submit_suggestion_feedback(
    state: tauri::State<'_, SuggestionRuntimeState>,
    suggestion_id: String,
    action: String,
    snooze_minutes: Option<u32>,
) -> Result<(), String> {
    let mgr = state.manager().ok_or("Suggestions not available")?;

    // Send feedback to server
    match action.as_str() {
        "accept" => mgr
            .feedback()
            .accept(&suggestion_id, None)
            .await
            .map_err(|e| e.to_string())?,
        "reject" => mgr
            .feedback()
            .reject(&suggestion_id, None)
            .await
            .map_err(|e| e.to_string())?,
        "defer" => {
            mgr.feedback()
                .defer(&suggestion_id, None)
                .await
                .map_err(|e| e.to_string())?;

            let (removed, scorer_data) = {
                let mut queue = mgr.queue().lock().await;
                let scorer_data = queue
                    .iter()
                    .find(|s| s.suggestion_id == suggestion_id)
                    .map(|s| (s.suggestion_type.clone(), s.source.clone()));
                let removed = queue.remove_by_id(&suggestion_id);
                (removed, scorer_data)
            }; // queue lock dropped
            if let Some((stype, source)) = scorer_data {
                mgr.scorer().lock().await.record(
                    stype,
                    source,
                    &oneshim_core::models::suggestion::FeedbackType::Deferred,
                );
            }

            if let Some(suggestion) = removed {
                {
                    let mut history = mgr.history().lock().await;
                    history.add(suggestion.clone());
                    history.record_feedback(
                        &suggestion_id,
                        oneshim_core::models::suggestion::FeedbackType::Deferred,
                    );
                }
                let duration_mins = snooze_minutes.unwrap_or(120);
                let duration = chrono::Duration::minutes(duration_mins as i64);
                mgr.deferred().lock().await.defer(suggestion, duration);
            }

            let count = mgr.queue().lock().await.len();
            if let Some(overlay) = state.overlay() {
                overlay.emit_suggestions_changed(count);
            }
            // Don't fall through to the accept/reject history block
            return Ok(());
        }
        _ => return Err(format!("Unknown action: {action}. Use accept/reject/defer")),
    }

    // Move accepted/rejected suggestion from queue to history.
    // Acquire queue lock once to both remove the item and get the remaining count,
    // avoiding a redundant second lock acquisition.
    let (removed, remaining_count) = {
        let mut queue = mgr.queue().lock().await;
        let removed = queue.remove_by_id(&suggestion_id);
        let count = queue.len();
        (removed, count)
    }; // queue lock dropped here

    if let Some(suggestion) = removed {
        let feedback_type = match action.as_str() {
            "accept" => oneshim_core::models::suggestion::FeedbackType::Accepted,
            "reject" => oneshim_core::models::suggestion::FeedbackType::Rejected,
            _ => unreachable!(),
        };
        mgr.scorer().lock().await.record(
            suggestion.suggestion_type.clone(),
            suggestion.source.clone(),
            &feedback_type,
        );
        {
            let mut history = mgr.history().lock().await;
            history.add(suggestion);
            history.record_feedback(&suggestion_id, feedback_type);
        }
    }

    // Notify overlay that suggestions changed (item removed from queue)
    if let Some(overlay) = state.overlay() {
        overlay.emit_suggestions_changed(remaining_count);
    }

    Ok(())
}

// ── Chat ↔ Suggestion integration ────────────────────────────

const SUGGESTION_PROMPT: &str = r#"Based on our conversation context, generate 1-3 actionable suggestions for the user.
Each suggestion should be specific, practical, and relevant to the current discussion.

Respond ONLY with a JSON object matching this schema:
{"suggestions": [{"type": "<type>", "content": "<text>", "priority": "<priority>", "reasoning": "<why>"}]}

Valid types: work_guidance, email_draft, productivity_tip, workflow_optimization, context_based
Valid priorities: low, medium, high, critical"#;

/// Generate suggestions from an active chat session by sending a structured
/// prompt and parsing the AI response. Returns the number of suggestions added.
#[command]
pub async fn request_chat_suggestions(
    ai_state: tauri::State<'_, AiSessionRuntimeState>,
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    session_id: String,
) -> Result<u32, String> {
    let mgr = ai_state
        .manager_impl()
        .ok_or_else(|| "AI sessions not available".to_string())?;

    let suggestion_mgr = suggestion_state
        .manager()
        .ok_or_else(|| "suggestions not available".to_string())?;

    // Get session and send structured request
    let session = mgr
        .get_session(&session_id)
        .await
        .map_err(|e| format!("session not found: {e}"))?;

    let msg = SessionMessage {
        role: MessageRole::User,
        content: SUGGESTION_PROMPT.to_string(),
        attachments: Vec::new(),
        tools: None,
        context: None,
        response_format: None,
    };

    let mut stream = session
        .send_message(&msg)
        .await
        .map_err(|e| format!("failed to send message: {e}"))?;

    // Drain stream and collect response text with a 60s timeout.
    // ResponseStream yields Result<OutboundMessage, CoreError>.
    let drain_result = timeout(Duration::from_secs(60), async {
        let mut text = String::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(OutboundMessage::Text { content, .. }) => text.push_str(&content),
                Ok(OutboundMessage::Result { content, .. }) => {
                    if !content.is_empty() && text.is_empty() {
                        text = content;
                    }
                }
                Ok(OutboundMessage::Error { message, .. }) => {
                    return Err(format!("AI error: {message}"));
                }
                Err(e) => {
                    return Err(format!("Stream error: {e}"));
                }
                _ => {}
            }
        }
        Ok::<String, String>(text)
    })
    .await;

    let response_text = match drain_result {
        Ok(Ok(text)) => text,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Suggestion generation timed out after 60 seconds".to_string()),
    };

    // Parse suggestions from response
    let suggestions = try_extract_suggestions(&response_text);
    let count = suggestions.len() as u32;

    // Push to queue
    if !suggestions.is_empty() {
        let mut queue = suggestion_mgr.queue().lock().await;
        for suggestion in suggestions {
            queue.push(suggestion);
        }
        let queue_count = queue.len();
        drop(queue);

        if let Some(overlay) = suggestion_state.overlay() {
            overlay.emit_suggestions_changed(queue_count);
        }
    }

    Ok(count)
}

/// Explain a suggestion in a chat session. Finds the suggestion from the queue
/// or history, sends an explain prompt to the session, and spawns a streaming
/// task that emits events. Emits `navigate:chat` for overlay navigation.
/// Returns the session ID used.
#[command]
pub async fn explain_suggestion_in_chat(
    app: AppHandle,
    ai_state: tauri::State<'_, AiSessionRuntimeState>,
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    suggestion_id: String,
    session_id: Option<String>,
) -> Result<String, String> {
    let suggestion_mgr = suggestion_state
        .manager()
        .ok_or_else(|| "suggestions not available".to_string())?;

    let ai_mgr = ai_state
        .manager_impl()
        .ok_or_else(|| "AI sessions not available".to_string())?;

    // Find suggestion from queue or history.
    // Two-phase lookup: check queue first, then fall back to history.
    let from_queue = {
        let queue = suggestion_mgr.queue().lock().await;
        let found = queue
            .iter()
            .find(|s| s.suggestion_id == suggestion_id)
            .map(|s| (s.content.clone(), s.reasoning.clone()));
        found
    }; // queue lock dropped

    let (content, reasoning) = if let Some(pair) = from_queue {
        pair
    } else {
        let history = suggestion_mgr.history().lock().await;
        let entry = history
            .recent(100)
            .into_iter()
            .find(|e| e.suggestion.suggestion_id == suggestion_id);
        match entry {
            Some(e) => (e.suggestion.content.clone(), e.suggestion.reasoning.clone()),
            None => return Err(format!("Suggestion {suggestion_id} not found")),
        }
    };

    // Find or validate session
    let sid = match session_id {
        Some(id) => id,
        None => {
            // Find most recent active/idle session
            let sessions = ai_mgr.list_sessions().await;
            sessions
                .into_iter()
                .filter(|s| s.state == SessionState::Active || s.state == SessionState::Idle)
                .max_by_key(|s| s.last_active)
                .map(|s| s.session_id)
                .ok_or_else(|| "No active chat session — open a chat first".to_string())?
        }
    };

    // Validate session state
    let sessions = ai_mgr.list_sessions().await;
    let session_info = sessions.iter().find(|s| s.session_id == sid);
    match session_info {
        Some(info) if info.state == SessionState::Active || info.state == SessionState::Idle => {}
        Some(info) => {
            return Err(format!(
                "Session {} is not active (state: {:?})",
                sid, info.state
            ))
        }
        None => return Err(format!("Session {sid} not found")),
    }

    // Compose explain message
    let mut prompt = format!(
        "Explain this suggestion in detail and help me understand how to act on it:\n\n{}",
        content
    );
    if let Some(reasoning) = reasoning {
        prompt.push_str(&format!("\n\nReasoning provided: {reasoning}"));
    }

    // Call session.send_message() directly and spawn a streaming task
    // that emits OutboundMessage events — replicating the pattern from ai_session.rs.
    let session = ai_mgr
        .get_session(&sid)
        .await
        .map_err(|e| format!("session error: {e}"))?;

    let user_content = prompt.clone();
    let msg = SessionMessage {
        role: MessageRole::User,
        content: prompt,
        attachments: Vec::new(),
        tools: None,
        context: None,
        response_format: None,
    };

    let session_storage = ai_state.session_storage();
    let stream = session
        .send_message(&msg)
        .await
        .map_err(|e| format!("failed to send: {e}"))?;

    // Spawn streaming task to emit events + persist messages
    // (same pattern as send_session_message in ai_session.rs)
    let event_name = format!("ai-session:{sid}");
    let session_id = sid.clone();
    let app_clone = app.clone();
    tokio::spawn(async move {
        tokio::pin!(stream);
        let mut assistant_content = String::new();
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
                        OutboundMessage::Result {
                            usage: Some(ref u), ..
                        } => {
                            total_input = u.input_tokens;
                            total_output = u.output_tokens;
                        }
                        _ => {}
                    }
                    let _ = app_clone.emit(&event_name, &outbound);
                }
                Err(e) => {
                    let err_msg = OutboundMessage::Error {
                        code: "stream_error".to_string(),
                        message: e.to_string(),
                        retryable: false,
                    };
                    let _ = app_clone.emit(&event_name, &err_msg);
                    break;
                }
            }
        }

        // Persist user + assistant messages after stream completes
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
                    thinking: None,
                    tool_use: None,
                    usage_input: Some(total_input),
                    usage_output: Some(total_output),
                    created_at: now,
                    seq: seq + 1,
                };
                if let Err(e) = ss
                    .save_messages(&session_id, &[user_msg, assistant_msg])
                    .await
                {
                    tracing::warn!("failed to persist explain messages: {e}");
                }
                let _ = ss
                    .update_session_usage(&session_id, total_input, total_output)
                    .await;
            }
        }
    });

    // Emit navigation event for overlay -> chat
    let _ = app.emit("navigate:chat", serde_json::json!({ "sessionId": sid }));

    Ok(sid)
}
