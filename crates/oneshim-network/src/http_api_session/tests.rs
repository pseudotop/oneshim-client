use super::*;
use oneshim_core::models::ai_session::Attachment;

#[allow(clippy::too_many_arguments)]
fn test_session(
    surface_id: String,
    model: String,
    endpoint: String,
    credential: CredentialSource,
    provider_type: AiProviderType,
    system_prompt: Option<String>,
    config: Arc<AiSessionConfig>,
    default_tools: Option<Vec<ToolDefinition>>,
) -> HttpApiSession {
    HttpApiSession::new(HttpApiSessionInit {
        surface_id,
        model,
        endpoint,
        credential,
        provider_type,
        system_prompt,
        config,
        default_tools,
    })
}

#[test]
fn anthropic_content_block_delta() {
    let data =
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
    let msg = parse_anthropic_sse_event("content_block_delta", data);
    match msg {
        Some(OutboundMessage::Text { content, done }) => {
            assert_eq!(content, "Hello");
            assert!(!done);
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn anthropic_message_stop() {
    let data = r#"{"type":"message_stop"}"#;
    let msg = parse_anthropic_sse_event("message_stop", data);
    match msg {
        Some(OutboundMessage::Result { done, .. }) => {
            assert!(done);
        }
        other => panic!("expected Result with done=true, got {other:?}"),
    }
}

#[test]
fn anthropic_message_delta_with_usage() {
    let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":25,"output_tokens":50}}"#;
    let msg = parse_anthropic_sse_event("message_delta", data);
    match msg {
        Some(OutboundMessage::Result { usage, .. }) => {
            let u = usage.unwrap();
            assert_eq!(u.input_tokens, 25);
            assert_eq!(u.output_tokens, 50);
        }
        other => panic!("expected Result with usage, got {other:?}"),
    }
}

#[test]
fn anthropic_ignores_unknown_event() {
    let msg = parse_anthropic_sse_event("ping", "{}");
    assert!(msg.is_none());
}

#[test]
fn openai_content_delta() {
    let data = r#"{"choices":[{"index":0,"delta":{"content":"world"}}]}"#;
    let msg = parse_openai_chat_sse_event(data);
    match msg {
        Some(OutboundMessage::Text { content, done }) => {
            assert_eq!(content, "world");
            assert!(!done);
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn openai_done_event() {
    let msg = parse_openai_chat_sse_event("[DONE]");
    match msg {
        Some(OutboundMessage::Result { done, .. }) => {
            assert!(done);
        }
        other => panic!("expected Result with done=true, got {other:?}"),
    }
}

#[test]
fn openai_with_usage() {
    let data = r#"{"usage":{"prompt_tokens":10,"completion_tokens":20}}"#;
    let msg = parse_openai_chat_sse_event(data);
    match msg {
        Some(OutboundMessage::Result { usage, .. }) => {
            let u = usage.unwrap();
            assert_eq!(u.input_tokens, 10);
            assert_eq!(u.output_tokens, 20);
        }
        other => panic!("expected Result with usage, got {other:?}"),
    }
}

#[test]
fn google_text_chunk() {
    let data = r#"{"candidates":[{"content":{"parts":[{"text":"Hello from Gemini"}],"role":"model"}}],"modelVersion":"gemini-2.5-flash"}"#;
    let msg = parse_google_sse_event(data);
    match msg {
        Some(OutboundMessage::Text { content, done }) => {
            assert_eq!(content, "Hello from Gemini");
            assert!(!done);
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn google_final_chunk_with_usage() {
    let data = r#"{"candidates":[{"content":{"parts":[{"text":"!"}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":42},"modelVersion":"gemini-2.5-flash"}"#;
    let msg = parse_google_sse_event(data);
    match msg {
        Some(OutboundMessage::Result {
            content,
            done,
            usage,
        }) => {
            assert_eq!(content, "!");
            assert!(done);
            let u = usage.unwrap();
            assert_eq!(u.input_tokens, 10);
            assert_eq!(u.output_tokens, 42);
        }
        other => panic!("expected Result with usage, got {other:?}"),
    }
}

#[test]
fn google_empty_data_ignored() {
    let msg = parse_google_sse_event("");
    assert!(msg.is_none());
}

#[test]
fn openai_empty_content_ignored() {
    let data = r#"{"choices":[{"index":0,"delta":{"content":""}}]}"#;
    let msg = parse_openai_chat_sse_event(data);
    assert!(msg.is_none());
}

#[test]
fn openai_responses_text_delta() {
    let data = r#"{"type":"response.output_text.delta","delta":"hello"}"#;
    let msg = parse_openai_responses_sse_event("response.output_text.delta", data);
    match msg {
        Some(OutboundMessage::Text { content, done }) => {
            assert_eq!(content, "hello");
            assert!(!done);
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn openai_responses_function_call_delta() {
    let data = r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc_123","call_id":"call_123","name":"get_weather","arguments":""}}"#;
    let msg = parse_openai_responses_sse_event("response.output_item.added", data);
    match msg {
        Some(OutboundMessage::ToolCallDelta {
            index, id, name, ..
        }) => {
            assert_eq!(index, 0);
            assert_eq!(id, "call_123");
            assert_eq!(name, "get_weather");
        }
        other => panic!("expected ToolCallDelta, got {other:?}"),
    }
}

#[test]
fn openai_responses_completed_with_usage() {
    let data = r#"{"type":"response.completed","response":{"usage":{"input_tokens":10,"output_tokens":20}}}"#;
    let msg = parse_openai_responses_sse_event("response.completed", data);
    match msg {
        Some(OutboundMessage::Result { done, usage, .. }) => {
            assert!(done);
            let usage = usage.expect("usage should be present");
            assert_eq!(usage.input_tokens, 10);
            assert_eq!(usage.output_tokens, 20);
        }
        other => panic!("expected Result with usage, got {other:?}"),
    }
}

#[test]
fn history_truncation_preserves_system_prompt() {
    let mut history = vec![
        ChatMessage {
            role: ChatRole::System,
            content: "system".to_string(),
            content_blocks: None,
        },
        ChatMessage {
            role: ChatRole::User,
            content: "msg1".to_string(),
            content_blocks: None,
        },
        ChatMessage {
            role: ChatRole::Assistant,
            content: "reply1".to_string(),
            content_blocks: None,
        },
        ChatMessage {
            role: ChatRole::User,
            content: "msg2".to_string(),
            content_blocks: None,
        },
        ChatMessage {
            role: ChatRole::Assistant,
            content: "reply2".to_string(),
            content_blocks: None,
        },
        ChatMessage {
            role: ChatRole::User,
            content: "msg3".to_string(),
            content_blocks: None,
        },
    ];

    // max_turns=4: keep system (index 0) + last 3 messages
    // Before: [system, msg1, reply1, msg2, reply2, msg3] (6 items)
    // drain(1..3) removes msg1, reply1
    // After:  [system, msg2, reply2, msg3] (4 items)
    HttpApiSession::truncate_history(&mut history, 4);
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].role, ChatRole::System);
    assert_eq!(history[0].content, "system");
    assert_eq!(history[1].content, "msg2");
    assert_eq!(history[2].content, "reply2");
    assert_eq!(history[3].content, "msg3");
}

#[test]
fn history_truncation_no_op_when_under_limit() {
    let mut history = vec![
        ChatMessage {
            role: ChatRole::System,
            content: "system".to_string(),
            content_blocks: None,
        },
        ChatMessage {
            role: ChatRole::User,
            content: "hello".to_string(),
            content_blocks: None,
        },
    ];

    HttpApiSession::truncate_history(&mut history, 10);
    assert_eq!(history.len(), 2);
}

#[test]
fn chat_message_from_session_message() {
    let session_msg = SessionMessage {
        role: oneshim_core::models::ai_session::MessageRole::User,
        content: "test question".to_string(),
        attachments: vec![],
        tools: None,
        context: None,
        response_format: None,
    };

    let chat_msg = ChatMessage {
        role: ChatRole::User,
        content: session_msg.content.clone(),
        content_blocks: None,
    };

    assert_eq!(chat_msg.role, ChatRole::User);
    assert_eq!(chat_msg.content, "test question");

    let json = serde_json::to_string(&chat_msg).unwrap();
    assert!(json.contains("\"role\":\"user\""));
    assert!(json.contains("test question"));
}

#[test]
fn new_session_with_system_prompt_initializes_history() {
    let session = test_session(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        Some("You are helpful.".to_string()),
        Arc::new(AiSessionConfig::default()),
        None,
    );

    assert!(!session.session_id.is_empty());
    assert_eq!(session.provider_name(), "anthropic");
    assert_eq!(session.model, "claude-sonnet-4-20250514");

    let info = session.info();
    assert_eq!(info.transport, SessionTransport::HttpApi);
    assert_eq!(info.turn_count, 0);
}

#[test]
fn new_session_without_system_prompt_has_empty_history() {
    let session = test_session(
        "provider_surface.openai.direct_api".to_string(),
        "gpt-5.4".to_string(),
        "https://api.openai.com/v1/chat/completions".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::OpenAi,
        None,
        Arc::new(AiSessionConfig::default()),
        None,
    );

    assert_eq!(session.provider_name(), "openai");
}

// ── Vision Content Block Tests ──────────────────────────────

/// Helper to create a session and build request body with content blocks.
fn build_body_with_blocks(
    provider: AiProviderType,
    surface: &str,
    endpoint: &str,
    blocks: Vec<ContentBlock>,
) -> serde_json::Value {
    let session = test_session(
        surface.to_string(),
        "test-model".to_string(),
        endpoint.to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        provider,
        Some("system prompt".to_string()),
        Arc::new(AiSessionConfig::default()),
        None,
    );

    let messages = vec![
        ChatMessage {
            role: ChatRole::System,
            content: "system prompt".to_string(),
            content_blocks: None,
        },
        ChatMessage {
            role: ChatRole::User,
            content: "Describe this image".to_string(),
            content_blocks: Some(blocks),
        },
    ];

    session
        .build_request_body(&messages, &RequestOptions::default())
        .expect("build_request_body should succeed")
}

fn sample_image_blocks() -> Vec<ContentBlock> {
    vec![
        ContentBlock::Text {
            text: "Describe this image".to_string(),
        },
        ContentBlock::Image {
            media_type: "image/jpeg".to_string(),
            data: "dGVzdA==".to_string(),
        },
    ]
}

fn sample_file_blocks() -> Vec<ContentBlock> {
    vec![
        ContentBlock::Text {
            text: "Summarize this file".to_string(),
        },
        ContentBlock::File {
            media_type: "application/pdf".to_string(),
            data: "JVBERi0xLjQK".to_string(),
            filename: Some("notes.pdf".to_string()),
        },
    ]
}

#[test]
fn anthropic_vision_content_blocks() {
    let body = build_body_with_blocks(
        AiProviderType::Anthropic,
        "provider_surface.anthropic.direct_api",
        "https://api.anthropic.com/v1/messages",
        sample_image_blocks(),
    );

    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 1); // system is excluded
    let content = messages[0]["content"].as_array().expect("content array");
    assert_eq!(content.len(), 2);

    // Text block
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "Describe this image");

    // Image block — Anthropic format
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["type"], "base64");
    assert_eq!(content[1]["source"]["media_type"], "image/jpeg");
    assert_eq!(content[1]["source"]["data"], "dGVzdA==");
}

#[test]
fn openai_vision_content_blocks() {
    let body = build_body_with_blocks(
        AiProviderType::OpenAi,
        "provider_surface.openai.direct_api",
        "https://api.openai.com/v1/responses",
        sample_image_blocks(),
    );

    assert_eq!(body["instructions"], "system prompt");

    let input = body["input"].as_array().expect("input array");
    assert_eq!(input.len(), 1);
    let user_content = input[0]["content"].as_array().expect("input content array");
    assert_eq!(user_content.len(), 2);

    // Text block
    assert_eq!(user_content[0]["type"], "input_text");
    assert_eq!(user_content[0]["text"], "Describe this image");

    // Image block — OpenAI Responses format
    assert_eq!(user_content[1]["type"], "input_image");
    let url = user_content[1]["image_url"].as_str().unwrap();
    assert!(url.starts_with("data:image/jpeg;base64,"));
    assert!(url.ends_with("dGVzdA=="));
}

#[test]
fn google_vision_content_blocks() {
    let body = build_body_with_blocks(
        AiProviderType::Google,
        "provider_surface.google.direct_api",
        "https://generativelanguage.googleapis.com/v1beta/models/test-model:generateContent",
        sample_image_blocks(),
    );

    let contents = body["contents"].as_array().expect("contents array");
    assert_eq!(contents.len(), 1); // system is excluded
    let parts = contents[0]["parts"].as_array().expect("parts array");
    assert_eq!(parts.len(), 2);

    // Text part
    assert_eq!(parts[0]["text"], "Describe this image");

    // Image part — Google format
    assert_eq!(parts[1]["inlineData"]["mimeType"], "image/jpeg");
    assert_eq!(parts[1]["inlineData"]["data"], "dGVzdA==");
}

#[test]
fn anthropic_pdf_file_content_blocks() {
    let body = build_body_with_blocks(
        AiProviderType::Anthropic,
        "provider_surface.anthropic.direct_api",
        "https://api.anthropic.com/v1/messages",
        sample_file_blocks(),
    );

    let messages = body["messages"].as_array().expect("messages array");
    let content = messages[0]["content"].as_array().expect("content array");
    assert_eq!(content[1]["type"], "document");
    assert_eq!(content[1]["source"]["type"], "base64");
    assert_eq!(content[1]["source"]["media_type"], "application/pdf");
    assert_eq!(content[1]["source"]["data"], "JVBERi0xLjQK");
    assert_eq!(content[1]["title"], "notes.pdf");
}

#[test]
fn openai_file_content_blocks() {
    let body = build_body_with_blocks(
        AiProviderType::OpenAi,
        "provider_surface.openai.direct_api",
        "https://api.openai.com/v1/responses",
        sample_file_blocks(),
    );

    let input = body["input"].as_array().expect("input array");
    let user_content = input[0]["content"].as_array().expect("input content array");
    assert_eq!(user_content[1]["type"], "input_file");
    assert_eq!(user_content[1]["file_data"], "JVBERi0xLjQK");
    assert_eq!(user_content[1]["filename"], "notes.pdf");
}

#[test]
fn google_file_content_blocks() {
    let body = build_body_with_blocks(
        AiProviderType::Google,
        "provider_surface.google.direct_api",
        "https://generativelanguage.googleapis.com/v1beta/models/test-model:generateContent",
        sample_file_blocks(),
    );

    let contents = body["contents"].as_array().expect("contents array");
    let parts = contents[0]["parts"].as_array().expect("parts array");
    assert_eq!(parts[1]["inlineData"]["mimeType"], "application/pdf");
    assert_eq!(parts[1]["inlineData"]["data"], "JVBERi0xLjQK");
}

#[test]
fn render_message_content_omits_native_attachment_manifest_entries() {
    let message = SessionMessage {
        role: oneshim_core::models::ai_session::MessageRole::User,
        content: "Summarize these attachments".to_string(),
        attachments: vec![
            Attachment::File {
                path: "/tmp/notes.pdf".to_string(),
                mime: Some("application/pdf".to_string()),
                data: Some("JVBERi0xLjQK".to_string()),
            },
            Attachment::Directory {
                path: "/tmp/workspace".to_string(),
            },
        ],
        tools: None,
        context: None,
        response_format: None,
    };

    let rendered = render_message_content(&message, &ProviderRequestShape::OpenAiResponses);
    assert!(!rendered.contains("/tmp/notes.pdf"));
    assert!(rendered.contains("/tmp/workspace"));
    assert!(rendered.contains("Attachment manifest"));
}

#[test]
fn plain_text_backward_compat() {
    // When content_blocks is None, content should be a plain string
    let session = test_session(
        "provider_surface.anthropic.direct_api".to_string(),
        "test-model".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(AiSessionConfig::default()),
        None,
    );

    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "Hello world".to_string(),
        content_blocks: None,
    }];

    let body = session
        .build_request_body(&messages, &RequestOptions::default())
        .expect("build_request_body should succeed");

    let api_messages = body["messages"].as_array().expect("messages array");
    assert_eq!(api_messages.len(), 1);

    // Content should be a plain string, not an array
    let content = &api_messages[0]["content"];
    assert!(
        content.is_string(),
        "expected string content, got {content}"
    );
    assert_eq!(content.as_str().unwrap(), "Hello world");
}

// ── Structured Output + Thinking Injection Tests ───────────

/// Helper to build a request body with custom RequestOptions.
fn build_body_with_options(
    provider: AiProviderType,
    surface: &str,
    endpoint: &str,
    options: &RequestOptions<'_>,
) -> serde_json::Value {
    let session = test_session(
        surface.to_string(),
        "test-model".to_string(),
        endpoint.to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        provider,
        None,
        Arc::new(AiSessionConfig::default()),
        None,
    );

    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "Hello".to_string(),
        content_blocks: None,
    }];

    session
        .build_request_body(&messages, options)
        .expect("build_request_body should succeed")
}

/// Helper to build a request body with thinking config set on the session.
fn build_body_with_thinking(
    provider: AiProviderType,
    surface: &str,
    endpoint: &str,
    thinking: serde_json::Value,
) -> serde_json::Value {
    let config = AiSessionConfig {
        thinking: Some(thinking),
        ..Default::default()
    };

    let session = test_session(
        surface.to_string(),
        "test-model".to_string(),
        endpoint.to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        provider,
        None,
        Arc::new(config),
        None,
    );

    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "Hello".to_string(),
        content_blocks: None,
    }];

    session
        .build_request_body(&messages, &RequestOptions::default())
        .expect("build_request_body should succeed")
}

#[test]
fn openai_structured_output_injects_response_format() {
    let rf = serde_json::json!({"type": "json_schema", "json_schema": {"name": "result", "schema": {"type": "object"}}});
    let options = RequestOptions {
        response_format: Some(&rf),
        tools: None,
    };
    let body = build_body_with_options(
        AiProviderType::OpenAi,
        "provider_surface.openai.direct_api",
        "https://api.openai.com/v1/responses",
        &options,
    );
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert!(body["text"]["format"]["schema"].is_object());
}

#[test]
fn google_structured_output_sets_response_mime_and_schema() {
    let rf = serde_json::json!({"schema": {"type": "object", "properties": {"name": {"type": "string"}}}});
    let options = RequestOptions {
        response_format: Some(&rf),
        tools: None,
    };
    let body = build_body_with_options(
        AiProviderType::Google,
        "provider_surface.google.direct_api",
        "https://generativelanguage.googleapis.com/v1beta/models/test-model:generateContent",
        &options,
    );
    assert_eq!(
        body["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert_eq!(body["generationConfig"]["responseSchema"]["type"], "object");
}

#[test]
fn anthropic_ignores_response_format() {
    let rf = serde_json::json!({"type": "json_schema"});
    let options = RequestOptions {
        response_format: Some(&rf),
        tools: None,
    };
    let body = build_body_with_options(
        AiProviderType::Anthropic,
        "provider_surface.anthropic.direct_api",
        "https://api.anthropic.com/v1/messages",
        &options,
    );
    assert!(
        body.get("response_format").is_none(),
        "Anthropic body should not contain response_format"
    );
}

#[test]
fn anthropic_thinking_injected() {
    let body = build_body_with_thinking(
        AiProviderType::Anthropic,
        "provider_surface.anthropic.direct_api",
        "https://api.anthropic.com/v1/messages",
        serde_json::json!({"type": "adaptive"}),
    );
    assert_eq!(body["thinking"]["type"], "adaptive");
}

#[test]
fn openai_reasoning_injected() {
    let body = build_body_with_thinking(
        AiProviderType::OpenAi,
        "provider_surface.openai.direct_api",
        "https://api.openai.com/v1/chat/completions",
        serde_json::json!({"effort": "high"}),
    );
    assert_eq!(body["reasoning"]["effort"], "high");
}

#[test]
fn google_thinking_config_injected() {
    let body = build_body_with_thinking(
        AiProviderType::Google,
        "provider_surface.google.direct_api",
        "https://generativelanguage.googleapis.com/v1beta/models/test-model:generateContent",
        serde_json::json!({"thinking_budget": 2048}),
    );
    assert_eq!(
        body["generationConfig"]["thinking_config"]["thinking_budget"],
        2048
    );
}

// ── Thinking SSE Parsing Tests ─────────────────────────────

#[test]
fn anthropic_thinking_delta() {
    let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me reason..."}}"#;
    let msg = parse_anthropic_sse_event("content_block_delta", data);
    match msg {
        Some(OutboundMessage::Thinking { content, done }) => {
            assert_eq!(content, "Let me reason...");
            assert!(!done);
        }
        other => panic!("expected Thinking, got {other:?}"),
    }
}

#[test]
fn anthropic_text_delta_still_works() {
    let data = r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"The answer is 42."}}"#;
    let msg = parse_anthropic_sse_event("content_block_delta", data);
    match msg {
        Some(OutboundMessage::Text { content, done }) => {
            assert_eq!(content, "The answer is 42.");
            assert!(!done);
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn google_thinking_part() {
    let data = r#"{"candidates":[{"content":{"parts":[{"thinking":"Reasoning step..."}],"role":"model"}}]}"#;
    let msg = parse_google_sse_event(data);
    match msg {
        Some(OutboundMessage::Thinking { content, done }) => {
            assert_eq!(content, "Reasoning step...");
            assert!(!done);
        }
        other => panic!("expected Thinking, got {other:?}"),
    }
}

#[test]
fn google_text_after_thinking() {
    let data = r#"{"candidates":[{"content":{"parts":[{"text":"Final answer"}],"role":"model"}}]}"#;
    let msg = parse_google_sse_event(data);
    match msg {
        Some(OutboundMessage::Text { content, done }) => {
            assert_eq!(content, "Final answer");
            assert!(!done);
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

// ── Tool Calling SSE Parsing Tests ────────────────────────────

#[test]
fn anthropic_tool_use_start() {
    let data = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_123","name":"get_weather"}}"#;
    let msg = parse_anthropic_sse_event("content_block_start", data);
    match msg {
        Some(OutboundMessage::ToolCallDelta { id, name, .. }) => {
            assert_eq!(id, "toolu_123");
            assert_eq!(name, "get_weather");
        }
        other => panic!("expected ToolCallDelta, got {other:?}"),
    }
}

#[test]
fn anthropic_input_json_delta() {
    let data = r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"location\":"}}"#;
    let msg = parse_anthropic_sse_event("content_block_delta", data);
    match msg {
        Some(OutboundMessage::ToolCallDelta {
            arguments_chunk, ..
        }) => {
            assert_eq!(arguments_chunk, "{\"location\":");
        }
        other => panic!("expected ToolCallDelta, got {other:?}"),
    }
}

#[test]
fn openai_tool_call_in_delta() {
    let data = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":""}}]}}]}"#;
    let msg = parse_openai_chat_sse_event(data);
    match msg {
        Some(OutboundMessage::ToolCallDelta {
            index, id, name, ..
        }) => {
            assert_eq!(index, 0);
            assert_eq!(id, "call_abc");
            assert_eq!(name, "get_weather");
        }
        other => panic!("expected ToolCallDelta, got {other:?}"),
    }
}

#[test]
fn openai_tool_call_finish() {
    let data = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#;
    let msg = parse_openai_chat_sse_event(data);
    match msg {
        Some(OutboundMessage::Result { done, .. }) => assert!(done),
        other => panic!("expected Result done=true, got {other:?}"),
    }
}

#[test]
fn google_function_call() {
    let data = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_weather","args":{"location":"Tokyo"}}}],"role":"model"}}]}"#;
    let msg = parse_google_sse_event(data);
    match msg {
        Some(OutboundMessage::ToolUse { tool, input, .. }) => {
            assert_eq!(tool, "get_weather");
            assert_eq!(input.unwrap()["location"], "Tokyo");
        }
        other => panic!("expected ToolUse, got {other:?}"),
    }
}

// ── Tool Calling Request Body Tests ───────────────────────────

#[test]
fn anthropic_tools_request_body() {
    let session = test_session(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(AiSessionConfig::default()),
        None,
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "weather?".to_string(),
        content_blocks: None,
    }];
    let tools = vec![ToolDefinition {
        name: "get_weather".to_string(),
        description: "Get weather".to_string(),
        endpoint: String::new(),
        method: "GET".to_string(),
        input_schema: Some(
            serde_json::json!({"type": "object", "properties": {"location": {"type": "string"}}}),
        ),
    }];
    let options = RequestOptions {
        response_format: None,
        tools: Some(&tools),
    };
    let body = session.build_request_body(&messages, &options).unwrap();
    let api_tools = body["tools"].as_array().unwrap();
    assert_eq!(api_tools[0]["name"], "get_weather");
    assert!(api_tools[0]["input_schema"].is_object());
}

#[test]
fn openai_tools_request_body() {
    let session = test_session(
        "provider_surface.openai.direct_api".to_string(),
        "gpt-5.4".to_string(),
        "https://api.openai.com/v1/responses".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::OpenAi,
        None,
        Arc::new(AiSessionConfig::default()),
        None,
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "weather?".to_string(),
        content_blocks: None,
    }];
    let tools = vec![ToolDefinition {
        name: "get_weather".to_string(),
        description: "Get weather".to_string(),
        endpoint: String::new(),
        method: "GET".to_string(),
        input_schema: Some(
            serde_json::json!({"type": "object", "properties": {"location": {"type": "string"}}}),
        ),
    }];
    let options = RequestOptions {
        response_format: None,
        tools: Some(&tools),
    };
    let body = session.build_request_body(&messages, &options).unwrap();
    let api_tools = body["tools"].as_array().unwrap();
    assert_eq!(api_tools[0]["type"], "function");
    assert_eq!(api_tools[0]["name"], "get_weather");
}

#[test]
fn tools_without_schema_receive_default_empty_object_schema() {
    let session = test_session(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(AiSessionConfig::default()),
        None,
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "test".to_string(),
        content_blocks: None,
    }];
    let tools = vec![ToolDefinition {
        name: "ping".to_string(),
        description: "Ping".to_string(),
        endpoint: "http://api/ping".to_string(),
        method: "GET".to_string(),
        input_schema: None,
    }];
    let options = RequestOptions {
        response_format: None,
        tools: Some(&tools),
    };
    let body = session.build_request_body(&messages, &options).unwrap();
    let api_tools = body["tools"].as_array().expect("tools array");
    assert_eq!(api_tools[0]["name"], "ping");
    assert_eq!(api_tools[0]["input_schema"]["type"], "object");
    assert_eq!(api_tools[0]["input_schema"]["additionalProperties"], false);
}
