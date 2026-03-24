use serde::Serialize;
use tauri::command;

use crate::runtime_state::AppState;

#[derive(Serialize)]
pub struct SuggestionViewDto {
    pub id: String,
    pub title: String,
    pub body: String,
    pub priority: String,
    pub category: Option<String>,
    pub source: String,
    pub created_at: String,
    pub is_read: bool,
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
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SuggestionViewDto>, String> {
    let mgr = state
        .suggestion_manager
        .as_ref()
        .ok_or("Suggestions not available")?;

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
                    s.created_at.to_rfc3339(),
                )
            })
            .collect()
    }; // queue lock dropped here

    let mut results = Vec::with_capacity(snapshot.len());
    for (id, title, body, priority, source, created_at) in snapshot {
        let is_read = mgr.is_read(&id).await;
        results.push(SuggestionViewDto {
            id,
            title,
            body,
            priority,
            category: None, // Suggestion has no category field
            source,
            created_at,
            is_read,
        });
    }
    Ok(results)
}

#[command]
pub async fn get_suggestion_history(
    state: tauri::State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<SuggestionViewDto>, String> {
    let mgr = state
        .suggestion_manager
        .as_ref()
        .ok_or("Suggestions not available")?;

    let history = mgr.history().lock().await;
    let entries = history.recent(limit.unwrap_or(20) as usize);
    let results = entries
        .into_iter()
        .map(|entry| SuggestionViewDto {
            id: entry.suggestion.suggestion_id.clone(),
            title: oneshim_suggestion::presenter::type_to_title(&entry.suggestion.suggestion_type),
            body: entry.suggestion.content.clone(),
            priority: format!("{:?}", entry.suggestion.priority).to_lowercase(),
            category: None,
            source: source_label(&entry.suggestion.source).to_string(),
            created_at: entry.suggestion.created_at.to_rfc3339(),
            is_read: true, // history items are implicitly read
        })
        .collect();
    Ok(results)
}

#[command]
pub async fn submit_suggestion_feedback(
    state: tauri::State<'_, AppState>,
    suggestion_id: String,
    action: String,
) -> Result<(), String> {
    let mgr = state
        .suggestion_manager
        .as_ref()
        .ok_or("Suggestions not available")?;

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
            // Notify overlay that suggestions changed (defer keeps item in queue)
            if let Some(ref overlay) = state.magic_overlay {
                let count = mgr.queue().lock().await.len();
                overlay.emit_suggestions_changed(count);
            }
            return Ok(()); // defer keeps item in queue, no history transfer
        }
        _ => return Err(format!("Unknown action: {action}. Use accept/reject/defer")),
    }

    // Move accepted/rejected suggestion from queue to history.
    let removed = mgr.queue().lock().await.remove_by_id(&suggestion_id);
    if let Some(suggestion) = removed {
        mgr.history().lock().await.add(suggestion);
    }

    // Notify overlay that suggestions changed (item removed from queue)
    if let Some(ref overlay) = state.magic_overlay {
        let count = mgr.queue().lock().await.len();
        overlay.emit_suggestions_changed(count);
    }

    Ok(())
}
