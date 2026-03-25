//! Claude stream-json output -> OutboundMessage normalization.

use oneshim_core::models::ai_session::{OutboundMessage, TokenUsage};

/// Parse a single line of Claude `--output-format stream-json` output into an
/// [`OutboundMessage`].  Returns `None` for unrecognised event types or
/// malformed JSON so callers can silently skip non-content lines.
pub(crate) fn normalize_claude_stream_event(line: &str) -> Option<OutboundMessage> {
    let event: serde_json::Value = serde_json::from_str(line).ok()?;
    match event.get("type")?.as_str()? {
        "assistant" => Some(OutboundMessage::Text {
            content: event.get("text")?.as_str()?.to_string(),
            done: false,
        }),
        "result" => {
            let usage = event.get("usage").and_then(|u| {
                Some(TokenUsage {
                    input_tokens: u.get("input_tokens")?.as_u64()?,
                    output_tokens: u.get("output_tokens")?.as_u64()?,
                })
            });
            Some(OutboundMessage::Result {
                content: event
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                done: true,
                usage,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_assistant_text_event() {
        let line = r#"{"type":"assistant","subtype":"text","text":"hello world"}"#;
        let msg = normalize_claude_stream_event(line).unwrap();
        match msg {
            OutboundMessage::Text { content, done } => {
                assert_eq!(content, "hello world");
                assert!(!done);
            }
            _ => panic!("expected Text variant"),
        }
    }

    #[test]
    fn normalizes_result_event() {
        let line = r#"{"type":"result","subtype":"success","result":"final answer","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let msg = normalize_claude_stream_event(line).unwrap();
        match msg {
            OutboundMessage::Result {
                content,
                done,
                usage,
            } => {
                assert_eq!(content, "final answer");
                assert!(done);
                let u = usage.unwrap();
                assert_eq!(u.input_tokens, 100);
                assert_eq!(u.output_tokens, 50);
            }
            _ => panic!("expected Result variant"),
        }
    }

    #[test]
    fn returns_none_for_unknown_event() {
        let line = r#"{"type":"system","message":"loading"}"#;
        assert!(normalize_claude_stream_event(line).is_none());
    }

    #[test]
    fn returns_none_for_invalid_json() {
        assert!(normalize_claude_stream_event("not json").is_none());
    }

    #[test]
    fn handles_result_without_usage() {
        let line = r#"{"type":"result","subtype":"success","result":"answer"}"#;
        let msg = normalize_claude_stream_event(line).unwrap();
        match msg {
            OutboundMessage::Result { usage, .. } => assert!(usage.is_none()),
            _ => panic!("expected Result variant"),
        }
    }
}
