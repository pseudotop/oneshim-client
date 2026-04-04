use serde::Serialize;
use tauri::command;

use crate::runtime_state::SuggestionRuntimeState;

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
                )
            })
            .collect()
    }; // queue lock dropped here

    let mut results = Vec::with_capacity(snapshot.len());
    for (id, title, body, priority, source, confidence_score, created_at) in snapshot {
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
        });
    }
    Ok(results)
}

#[command]
pub async fn get_suggestion_history(
    state: tauri::State<'_, SuggestionRuntimeState>,
    limit: Option<u32>,
) -> Result<Vec<SuggestionViewDto>, String> {
    let mgr = state.manager().ok_or("Suggestions not available")?;

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
            confidence_score: entry.suggestion.confidence_score,
            created_at: entry.suggestion.created_at.to_rfc3339(),
            is_read: true, // history items are implicitly read
        })
        .collect();
    Ok(results)
}

#[command]
pub async fn submit_suggestion_feedback(
    state: tauri::State<'_, SuggestionRuntimeState>,
    suggestion_id: String,
    action: String,
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
            // Record deferred feedback for scoring.
            // FeedbackSender::defer() does not lock queue — safe to acquire here.
            let (suggestion_snapshot, count) = {
                let queue = mgr.queue().lock().await;
                let snapshot = queue
                    .iter()
                    .find(|s| s.suggestion_id == suggestion_id)
                    .cloned();
                let count = queue.len();
                (snapshot, count)
            }; // queue lock dropped
            if let Some(suggestion) = suggestion_snapshot {
                mgr.scorer().lock().await.record(
                    suggestion.suggestion_type,
                    suggestion.source,
                    &oneshim_core::models::suggestion::FeedbackType::Deferred,
                );
            }
            if let Some(overlay) = state.overlay() {
                overlay.emit_suggestions_changed(count);
            }
            return Ok(()); // defer keeps item in queue, no history transfer
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
        // Record feedback for relevance scoring
        let feedback_type = match action.as_str() {
            "accept" => oneshim_core::models::suggestion::FeedbackType::Accepted,
            "reject" => oneshim_core::models::suggestion::FeedbackType::Rejected,
            _ => unreachable!(), // defer returns early above
        };
        mgr.scorer().lock().await.record(
            suggestion.suggestion_type.clone(),
            suggestion.source.clone(),
            &feedback_type,
        );
        mgr.history().lock().await.add(suggestion);
    }

    // Notify overlay that suggestions changed (item removed from queue)
    if let Some(overlay) = state.overlay() {
        overlay.emit_suggestions_changed(remaining_count);
    }

    Ok(())
}
