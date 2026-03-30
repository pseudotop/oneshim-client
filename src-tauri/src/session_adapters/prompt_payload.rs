use oneshim_core::models::ai_session::{
    Attachment, ChatMessage, ChatRole, MessageContext, SessionMessage, ToolDefinition,
};

pub(crate) fn render_message_payload(
    message: &SessionMessage,
    fallback_tools: Option<&[ToolDefinition]>,
) -> String {
    let mut sections = vec![message.content.clone()];

    if let Some(context) = message
        .context
        .as_ref()
        .filter(|context| has_meaningful_context(context))
    {
        sections.push(format!(
            "Additional context JSON:\n{}",
            pretty_json(context)
        ));
    }

    let attachment_manifest = attachment_manifest(&message.attachments);
    if !attachment_manifest.is_empty() {
        sections.push(format!(
            "Attachments JSON:\n{}",
            pretty_json(&attachment_manifest)
        ));
    }

    let tools = message
        .tools
        .as_deref()
        .filter(|tools| !tools.is_empty())
        .or_else(|| fallback_tools.filter(|tools| !tools.is_empty()));
    if let Some(tools) = tools {
        sections.push(format!(
            "Available tools JSON:\n{}\nIf you need one of these tools, explain the intended call and arguments explicitly.",
            pretty_json(tools)
        ));
    }

    if let Some(response_format) = message.response_format.as_ref() {
        sections.push(format!(
            "Required response format JSON:\n{}\nReturn the final answer in this format exactly.",
            pretty_json(response_format)
        ));
    }

    sections.join("\n\n")
}

pub(crate) fn render_conversation_prompt(
    system_prompt: Option<&str>,
    history: &[ChatMessage],
) -> String {
    let mut sections = Vec::new();

    if let Some(prompt) = system_prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        sections.push(format!("System instructions:\n{prompt}"));
    }

    if !history.is_empty() {
        let transcript = history
            .iter()
            .map(|message| {
                format!(
                    "{}:\n{}",
                    match message.role {
                        ChatRole::System => "System",
                        ChatRole::User => "User",
                        ChatRole::Assistant => "Assistant",
                    },
                    message.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        sections.push(format!(
            "Conversation transcript:\n{transcript}\n\nRespond as the assistant to the latest user message."
        ));
    }

    sections.join("\n\n")
}

fn has_meaningful_context(context: &MessageContext) -> bool {
    context
        .regime
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        || context
            .active_app
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
}

fn attachment_manifest(attachments: &[Attachment]) -> Vec<serde_json::Value> {
    attachments
        .iter()
        .map(|attachment| match attachment {
            Attachment::Image { mime, path, data } => serde_json::json!({
                "kind": "image",
                "mime": mime,
                "path": path,
                "has_inline_data": data.as_ref().is_some_and(|value| !value.is_empty()),
            }),
            Attachment::File { path, mime, data } => serde_json::json!({
                "kind": "file",
                "path": path,
                "mime": mime,
                "has_inline_data": data.as_ref().is_some_and(|value| !value.is_empty()),
            }),
            Attachment::Directory { path } => serde_json::json!({
                "kind": "directory",
                "path": path,
            }),
            Attachment::Skill {
                skill_id,
                display_name,
            } => serde_json::json!({
                "kind": "skill",
                "skill_id": skill_id,
                "display_name": display_name,
            }),
            Attachment::AppReference {
                app_name,
                window_title,
            } => serde_json::json!({
                "kind": "app_reference",
                "app_name": app_name,
                "window_title": window_title,
            }),
        })
        .collect()
}

fn pretty_json<T: serde::Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::models::ai_session::{MessageRole, ToolDefinition};

    #[test]
    fn render_message_payload_includes_optional_sections() {
        let message = SessionMessage {
            role: MessageRole::User,
            content: "Explain this screenshot".to_string(),
            attachments: vec![Attachment::Image {
                mime: "image/png".to_string(),
                path: None,
                data: Some("abc".to_string()),
            }],
            tools: Some(vec![ToolDefinition {
                name: "get_sessions".to_string(),
                description: "List sessions".to_string(),
                endpoint: "http://localhost/api/sessions".to_string(),
                method: "GET".to_string(),
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                })),
            }]),
            context: Some(MessageContext {
                regime: Some("focus".to_string()),
                active_app: Some("VS Code".to_string()),
            }),
            response_format: Some(serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "answer",
                    "schema": { "type": "object" }
                }
            })),
        };

        let rendered = render_message_payload(&message, None);
        assert!(rendered.contains("Additional context JSON"));
        assert!(rendered.contains("Attachments JSON"));
        assert!(rendered.contains("Available tools JSON"));
        assert!(rendered.contains("Required response format JSON"));
    }

    #[test]
    fn render_conversation_prompt_includes_system_and_history() {
        let prompt = render_conversation_prompt(
            Some("Be concise."),
            &[
                ChatMessage {
                    role: ChatRole::User,
                    content: "Hello".to_string(),
                    content_blocks: None,
                },
                ChatMessage {
                    role: ChatRole::Assistant,
                    content: "Hi".to_string(),
                    content_blocks: None,
                },
            ],
        );

        assert!(prompt.contains("System instructions"));
        assert!(prompt.contains("Conversation transcript"));
        assert!(prompt.contains("User:\nHello"));
        assert!(prompt.contains("Assistant:\nHi"));
    }
}
