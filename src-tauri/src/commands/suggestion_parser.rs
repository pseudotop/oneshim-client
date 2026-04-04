use chrono::Utc;
use oneshim_core::models::suggestion::{Priority, Suggestion, SuggestionSource, SuggestionType};
use serde::Deserialize;
use tracing::debug;

#[derive(Debug, Deserialize)]
struct ParsedSuggestion {
    #[serde(rename = "type")]
    suggestion_type: String,
    content: String,
    priority: String,
    #[serde(default)]
    reasoning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SuggestionResponse {
    suggestions: Vec<ParsedSuggestion>,
}

fn parse_type(s: &str) -> Option<SuggestionType> {
    match s.to_lowercase().replace(' ', "_").as_str() {
        "work_guidance" => Some(SuggestionType::WorkGuidance),
        "email_draft" => Some(SuggestionType::EmailDraft),
        "productivity_tip" => Some(SuggestionType::ProductivityTip),
        "workflow_optimization" => Some(SuggestionType::WorkflowOptimization),
        "context_based" => Some(SuggestionType::ContextBased),
        _ => None,
    }
}

fn parse_priority(s: &str) -> Priority {
    match s.to_lowercase().as_str() {
        "critical" => Priority::Critical,
        "high" => Priority::High,
        "low" => Priority::Low,
        _ => Priority::Medium,
    }
}

/// Extract suggestion JSON from AI response text.
/// Looks for `{"suggestions": [...]}` pattern — either bare or inside ```json fences.
/// Returns empty vec if nothing found or parsing fails.
pub fn try_extract_suggestions(response_text: &str) -> Vec<Suggestion> {
    // Try to find JSON block
    let json_str = extract_json_block(response_text);
    let json_str = match json_str {
        Some(s) => s,
        None => return Vec::new(),
    };

    // Parse as SuggestionResponse
    let parsed: SuggestionResponse = match serde_json::from_str(&json_str) {
        Ok(r) => r,
        Err(e) => {
            debug!("suggestion extraction parse error: {e}");
            return Vec::new();
        }
    };

    // Convert to Suggestion structs
    parsed
        .suggestions
        .into_iter()
        .filter_map(|p| {
            let stype = parse_type(&p.suggestion_type)?;
            Some(Suggestion {
                suggestion_id: format!("chat-{}", uuid::Uuid::new_v4()),
                suggestion_type: stype,
                content: p.content,
                priority: parse_priority(&p.priority),
                confidence_score: 0.7,
                relevance_score: 0.8,
                is_actionable: true,
                created_at: Utc::now(),
                expires_at: None,
                source: SuggestionSource::LlmLocal,
                reasoning: p.reasoning,
            })
        })
        .collect()
}

/// Extract a JSON object from text. Handles:
/// 1. ```json\n{...}\n``` fenced blocks
/// 2. Bare JSON starting with `{"suggestions"`
fn extract_json_block(text: &str) -> Option<String> {
    // Try fenced block first
    if let Some(start) = text.find("```json") {
        let after_fence = &text[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return Some(after_fence[..end].trim().to_string());
        }
    }
    // Try bare JSON with "suggestions" key
    if let Some(start) = text.find("{\"suggestions\"") {
        // Find matching closing brace
        let mut depth = 0;
        for (i, ch) in text[start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(text[start..start + i + 1].to_string());
                    }
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fenced_json() {
        let text = r#"Here are some suggestions:

```json
{"suggestions": [{"type": "productivity_tip", "content": "Try batching similar tasks", "priority": "high", "reasoning": "Based on your workflow"}]}
```

Hope that helps!"#;

        let results = try_extract_suggestions(text);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].suggestion_type, SuggestionType::ProductivityTip);
        assert_eq!(results[0].content, "Try batching similar tasks");
        assert_eq!(results[0].priority, Priority::High);
        assert_eq!(
            results[0].reasoning.as_deref(),
            Some("Based on your workflow")
        );
        assert_eq!(results[0].source, SuggestionSource::LlmLocal);
    }

    #[test]
    fn parse_bare_json() {
        let text = r#"{"suggestions": [{"type": "work_guidance", "content": "Focus on the report", "priority": "medium"}]}"#;
        let results = try_extract_suggestions(text);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].suggestion_type, SuggestionType::WorkGuidance);
        assert!(results[0].reasoning.is_none());
    }

    #[test]
    fn parse_multiple_suggestions() {
        let text = r#"{"suggestions": [
            {"type": "productivity_tip", "content": "Tip 1", "priority": "low"},
            {"type": "email_draft", "content": "Draft email", "priority": "high"},
            {"type": "context_based", "content": "Context suggestion", "priority": "critical"}
        ]}"#;
        let results = try_extract_suggestions(text);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].priority, Priority::Low);
        assert_eq!(results[1].suggestion_type, SuggestionType::EmailDraft);
        assert_eq!(results[2].priority, Priority::Critical);
    }

    #[test]
    fn invalid_type_filtered_out() {
        let text = r#"{"suggestions": [{"type": "unknown_type", "content": "Test", "priority": "medium"}]}"#;
        let results = try_extract_suggestions(text);
        assert!(results.is_empty());
    }

    #[test]
    fn no_json_returns_empty() {
        let results = try_extract_suggestions("Just a normal response with no JSON.");
        assert!(results.is_empty());
    }

    #[test]
    fn malformed_json_returns_empty() {
        let text = r#"{"suggestions": [{"type": "work_guidance", "content": broken}]}"#;
        let results = try_extract_suggestions(text);
        assert!(results.is_empty());
    }

    #[test]
    fn empty_suggestions_array() {
        let text = r#"{"suggestions": []}"#;
        let results = try_extract_suggestions(text);
        assert!(results.is_empty());
    }
}
