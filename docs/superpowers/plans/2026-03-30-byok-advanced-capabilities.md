# BYOK Advanced Capabilities Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement vision, structured output, thinking, and tool calling for HttpApiSession across all provider shapes.

**Architecture:** Extend `ChatMessage` with optional `content_blocks` for multi-content messages. Add `RequestOptions` to `build_request_body` for response_format/tools injection. Extend SSE parsers for thinking deltas and tool call accumulation. All changes are backward-compatible — plain text sessions work identically.

**Tech Stack:** Rust, serde_json, async_stream, eventsource_stream

---

### Task 1: ContentBlock Model + ChatMessage Extension

**Files:**
- Modify: `crates/oneshim-core/src/models/ai_session.rs`

- [ ] **Step 1: Add ContentBlock enum and extend ChatMessage**

Add after the `Attachment` enum (line 125):

```rust
/// Rich content block for multi-content messages (vision, tool use, thinking).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        data: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
    Thinking {
        thinking: String,
    },
}
```

Add `content_blocks` field to `ChatMessage`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<ContentBlock>>,
}
```

- [ ] **Step 2: Add Thinking and ToolCallDelta variants to OutboundMessage**

Add after the `Control` variant:

```rust
Thinking {
    content: String,
    done: bool,
},
/// Internal: partial tool call argument chunk from SSE stream.
/// Not yielded to consumers — consumed by stream accumulation logic.
ToolCallDelta {
    index: u32,
    id: String,
    name: String,
    arguments_chunk: String,
},
```

- [ ] **Step 3: Add response_format to SessionMessage**

```rust
pub struct SessionMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<MessageContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
}
```

- [ ] **Step 4: Add input_schema to ToolDefinition**

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub endpoint: String,
    #[serde(default = "default_http_method")]
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}
```

- [ ] **Step 5: Add RequestOptions and PartialToolCall to http_api_session.rs**

Add at module level in `crates/oneshim-network/src/http_api_session.rs`:

```rust
use oneshim_core::models::ai_session::{Attachment, ContentBlock, ToolDefinition};

/// Per-request options passed from SessionMessage to build_request_body.
#[derive(Debug, Default)]
struct RequestOptions<'a> {
    response_format: Option<&'a serde_json::Value>,
    tools: Option<&'a [ToolDefinition]>,
}

/// Accumulator for streaming tool call arguments across SSE chunks.
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}
```

Update `build_request_body` signature:
```rust
fn build_request_body(
    &self,
    messages: &[ChatMessage],
    options: &RequestOptions<'_>,
) -> Result<serde_json::Value, CoreError>
```

Update the call in `send_message` to pass `&RequestOptions::default()`.

- [ ] **Step 6: Add thinking to AiSessionConfig**

In `crates/oneshim-core/src/config/sections/ai_session.rs`, add:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub thinking: Option<serde_json::Value>,
```

And in `Default::default()`:
```rust
thinking: None,
```

- [ ] **Step 6: Add serde roundtrip tests**

Add in the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn content_block_text_roundtrip() {
    let block = ContentBlock::Text { text: "hello".to_string() };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains("\"type\":\"text\""));
    let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
    match parsed {
        ContentBlock::Text { text } => assert_eq!(text, "hello"),
        _ => panic!("expected Text"),
    }
}

#[test]
fn content_block_image_roundtrip() {
    let block = ContentBlock::Image {
        media_type: "image/png".to_string(),
        data: "iVBOR...".to_string(),
    };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains("\"type\":\"image\""));
    let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
    match parsed {
        ContentBlock::Image { media_type, .. } => assert_eq!(media_type, "image/png"),
        _ => panic!("expected Image"),
    }
}

#[test]
fn chat_message_backward_compat_no_content_blocks() {
    let json = r#"{"role":"user","content":"hello"}"#;
    let msg: ChatMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.content, "hello");
    assert!(msg.content_blocks.is_none());
}

#[test]
fn chat_message_with_content_blocks() {
    let msg = ChatMessage {
        role: ChatRole::User,
        content: "describe this".to_string(),
        content_blocks: Some(vec![
            ContentBlock::Text { text: "describe this".to_string() },
            ContentBlock::Image {
                media_type: "image/jpeg".to_string(),
                data: "base64data".to_string(),
            },
        ]),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("content_blocks"));
    let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
    assert!(parsed.content_blocks.is_some());
    assert_eq!(parsed.content_blocks.unwrap().len(), 2);
}

#[test]
fn outbound_thinking_serialization() {
    let msg = OutboundMessage::Thinking {
        content: "reasoning...".to_string(),
        done: false,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"thinking\""));
}

#[test]
fn tool_definition_with_schema() {
    let tool = ToolDefinition {
        name: "get_weather".to_string(),
        description: "Get weather".to_string(),
        endpoint: "http://api/weather".to_string(),
        method: "GET".to_string(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "location": { "type": "string" } }
        })),
    };
    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("input_schema"));
}

#[test]
fn tool_definition_without_schema_omits_field() {
    let tool = ToolDefinition {
        name: "ping".to_string(),
        description: "Ping".to_string(),
        endpoint: "http://api/ping".to_string(),
        method: "GET".to_string(),
        input_schema: None,
    };
    let json = serde_json::to_string(&tool).unwrap();
    assert!(!json.contains("input_schema"));
}
```

- [ ] **Step 7: Fix existing ChatMessage construction sites**

Every place that creates `ChatMessage { role, content }` must now include `content_blocks: None`. Search with `rg "ChatMessage \{" crates/` and add the field.

Key locations:
- `crates/oneshim-network/src/http_api_session.rs` — `send_message` (user msg + assistant msg)

- [ ] **Step 8: Run tests and verify**

Run: `cargo test -p oneshim-core --lib ai_session`
Expected: All existing tests + 7 new tests pass.

Run: `cargo check --workspace`
Expected: No errors (all ChatMessage construction sites updated).

- [ ] **Step 9: Commit**

```bash
git add crates/oneshim-core/ crates/oneshim-network/src/http_api_session.rs
git commit -m "feat: add ContentBlock model, Thinking outbound, response_format, input_schema"
```

---

### Task 2: Vision — Request Building + Attachment Conversion

**Files:**
- Modify: `crates/oneshim-network/src/http_api_session.rs`

- [ ] **Step 1: Add helper functions for content block serialization**

Add before `build_request_body`:

```rust
/// Serialize content blocks to Anthropic format.
fn serialize_anthropic_content(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => {
                Some(serde_json::json!({"type": "text", "text": text}))
            }
            ContentBlock::Image { media_type, data } => {
                Some(serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": data,
                    }
                }))
            }
            _ => None,
        })
        .collect()
}

/// Serialize content blocks to OpenAI-compatible format.
fn serialize_openai_content(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => {
                Some(serde_json::json!({"type": "text", "text": text}))
            }
            ContentBlock::Image { media_type, data } => {
                Some(serde_json::json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:{media_type};base64,{data}"),
                    }
                }))
            }
            _ => None,
        })
        .collect()
}

/// Serialize content blocks to Google Gemini format.
fn serialize_google_parts(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => {
                Some(serde_json::json!({"text": text}))
            }
            ContentBlock::Image { media_type, data } => {
                Some(serde_json::json!({
                    "inlineData": {
                        "mimeType": media_type,
                        "data": data,
                    }
                }))
            }
            _ => None,
        })
        .collect()
}
```

- [ ] **Step 2: Update Anthropic arm in build_request_body for vision**

Replace the message mapping in the Anthropic arm:

```rust
let api_messages: Vec<serde_json::Value> = messages
    .iter()
    .filter(|m| m.role != ChatRole::System)
    .map(|m| {
        let content = if let Some(ref blocks) = m.content_blocks {
            serde_json::Value::Array(serialize_anthropic_content(blocks))
        } else {
            serde_json::Value::String(m.content.clone())
        };
        serde_json::json!({ "role": m.role, "content": content })
    })
    .collect();
```

- [ ] **Step 3: Update OpenAI arm for vision**

Replace the message mapping in the OpenAI arm:

```rust
let api_messages: Vec<serde_json::Value> = messages
    .iter()
    .map(|m| {
        let content = if let Some(ref blocks) = m.content_blocks {
            serde_json::Value::Array(serialize_openai_content(blocks))
        } else {
            serde_json::Value::String(m.content.clone())
        };
        serde_json::json!({ "role": m.role, "content": content })
    })
    .collect();
```

- [ ] **Step 4: Update Google arm for vision**

Replace the parts mapping in the Google arm:

```rust
let api_contents: Vec<serde_json::Value> = messages
    .iter()
    .filter(|m| m.role != ChatRole::System)
    .map(|m| {
        let parts = if let Some(ref blocks) = m.content_blocks {
            serialize_google_parts(blocks)
        } else {
            vec![serde_json::json!({"text": m.content})]
        };
        serde_json::json!({
            "role": match m.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "model",
                _ => "user",
            },
            "parts": parts,
        })
    })
    .collect();
```

- [ ] **Step 5: Update send_message for attachment→ContentBlock conversion**

In `send_message`, replace the `ChatMessage` construction:

```rust
use oneshim_core::models::ai_session::{Attachment, ContentBlock};

let content_blocks = {
    let mut blocks = vec![ContentBlock::Text {
        text: message.content.clone(),
    }];
    for att in &message.attachments {
        if let Attachment::Image {
            mime,
            data: Some(b64),
            ..
        } = att
        {
            blocks.push(ContentBlock::Image {
                media_type: mime.clone(),
                data: b64.clone(),
            });
        }
    }
    if blocks.len() > 1 {
        Some(blocks)
    } else {
        None
    }
};

let user_msg = ChatMessage {
    role: ChatRole::User,
    content: message.content.clone(),
    content_blocks,
};
```

- [ ] **Step 6: Add vision request build tests**

```rust
#[test]
fn anthropic_vision_content_blocks() {
    let session = HttpApiSession::new(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(AiSessionConfig::default()),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "describe".to_string(),
        content_blocks: Some(vec![
            ContentBlock::Text { text: "describe".to_string() },
            ContentBlock::Image {
                media_type: "image/png".to_string(),
                data: "iVBOR".to_string(),
            },
        ]),
    }];
    let body = session.build_request_body(&messages, &RequestOptions::default()).unwrap();
    let content = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["media_type"], "image/png");
}

#[test]
fn openai_vision_content_blocks() {
    let session = HttpApiSession::new(
        "provider_surface.openai.direct_api".to_string(),
        "gpt-5.4".to_string(),
        "https://api.openai.com/v1/chat/completions".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::OpenAi,
        None,
        Arc::new(AiSessionConfig::default()),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "describe".to_string(),
        content_blocks: Some(vec![
            ContentBlock::Text { text: "describe".to_string() },
            ContentBlock::Image {
                media_type: "image/jpeg".to_string(),
                data: "/9j/4AAQ".to_string(),
            },
        ]),
    }];
    let body = session.build_request_body(&messages, &RequestOptions::default()).unwrap();
    let content = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image_url");
    assert!(content[1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
}

#[test]
fn google_vision_content_blocks() {
    let session = HttpApiSession::new(
        "provider_surface.google.direct_api".to_string(),
        "gemini-2.5-flash".to_string(),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent".to_string(),
        CredentialSource::ApiKey("test-key".to_string()),
        AiProviderType::Google,
        None,
        Arc::new(AiSessionConfig::default()),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "describe".to_string(),
        content_blocks: Some(vec![
            ContentBlock::Text { text: "describe".to_string() },
            ContentBlock::Image {
                media_type: "image/png".to_string(),
                data: "iVBOR".to_string(),
            },
        ]),
    }];
    let body = session.build_request_body(&messages, &RequestOptions::default()).unwrap();
    let parts = body["contents"][0]["parts"].as_array().unwrap();
    assert!(parts[0].get("text").is_some());
    assert_eq!(parts[1]["inlineData"]["mimeType"], "image/png");
}

#[test]
fn plain_text_backward_compat() {
    let session = HttpApiSession::new(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(AiSessionConfig::default()),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "hello".to_string(),
        content_blocks: None,
    }];
    let body = session.build_request_body(&messages, &RequestOptions::default()).unwrap();
    assert_eq!(body["messages"][0]["content"], "hello");
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p oneshim-network --lib http_api_session`
Expected: All existing 16 + 4 new tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/oneshim-network/src/http_api_session.rs
git commit -m "feat: vision content block serialization for Anthropic/OpenAI/Google"
```

---

### Task 3: RequestOptions + Structured Output + Thinking Injection

**Files:**
- Modify: `crates/oneshim-network/src/http_api_session.rs`
- Modify: `crates/oneshim-api-contracts/src/settings.rs`
- Modify: `crates/oneshim-web/src/services/settings_assembler.rs`
- Modify: `crates/oneshim-web/src/services/settings_config_mutation.rs`

- [ ] **Step 1: Add RequestOptions struct and update build_request_body signature**

```rust
use oneshim_core::models::ai_session::ToolDefinition;

/// Per-request options passed from SessionMessage to build_request_body.
#[derive(Debug, Default)]
struct RequestOptions<'a> {
    response_format: Option<&'a serde_json::Value>,
    tools: Option<&'a [ToolDefinition]>,
}
```

Change `build_request_body` signature:
```rust
fn build_request_body(
    &self,
    messages: &[ChatMessage],
    options: &RequestOptions<'_>,
) -> Result<serde_json::Value, CoreError>
```

Update the call site in `send_message`:
```rust
let options = RequestOptions {
    response_format: message.response_format.as_ref(),
    tools: message.tools.as_deref(),
};
let request_body = self.build_request_body(&messages_snapshot, &options)?;
```

- [ ] **Step 2: Inject response_format per provider shape**

At the end of each match arm, before `Ok(body)`:

**Anthropic arm:** (silently ignored — no native support)

**OpenAI arm:**
```rust
if let Some(rf) = options.response_format {
    body["response_format"] = rf.clone();
}
```

**Google arm:**
```rust
if let Some(rf) = options.response_format {
    if let Some(schema) = rf.get("schema").or_else(|| rf.get("json_schema").and_then(|js| js.get("schema"))) {
        body["generationConfig"]["responseMimeType"] = serde_json::json!("application/json");
        body["generationConfig"]["responseSchema"] = schema.clone();
    }
}
```

- [ ] **Step 3: Inject thinking config per provider shape**

At the end of each match arm, before `Ok(body)`:

**Anthropic arm:**
```rust
if let Some(ref thinking) = self.config.thinking {
    body["thinking"] = thinking.clone();
}
```

**OpenAI arm:**
```rust
if let Some(ref thinking) = self.config.thinking {
    body["reasoning"] = thinking.clone();
}
```

**Google arm:**
```rust
if let Some(ref thinking) = self.config.thinking {
    body["generationConfig"]["thinking_config"] = thinking.clone();
}
```

- [ ] **Step 4: Wire thinking through settings**

In `crates/oneshim-api-contracts/src/settings.rs`, add to `AiSessionSettings`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub thinking: Option<serde_json::Value>,
```

In `Default::default()`:
```rust
thinking: None,
```

In `crates/oneshim-web/src/services/settings_assembler.rs`, add:
```rust
thinking: config.ai_session.thinking.clone(),
```

In `crates/oneshim-web/src/services/settings_config_mutation.rs`, add:
```rust
config.ai_session.thinking = settings.ai_session.thinking.clone();
```

- [ ] **Step 5: Add structured output tests**

```rust
#[test]
fn openai_structured_output_injects_response_format() {
    let session = HttpApiSession::new(
        "provider_surface.openai.direct_api".to_string(),
        "gpt-5.4".to_string(),
        "https://api.openai.com/v1/chat/completions".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::OpenAi,
        None,
        Arc::new(AiSessionConfig::default()),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "extract data".to_string(),
        content_blocks: None,
    }];
    let rf = serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "Output",
            "schema": {"type": "object", "properties": {"name": {"type": "string"}}}
        }
    });
    let options = RequestOptions {
        response_format: Some(&rf),
        tools: None,
    };
    let body = session.build_request_body(&messages, &options).unwrap();
    assert_eq!(body["response_format"]["type"], "json_schema");
}

#[test]
fn google_structured_output_sets_response_mime_and_schema() {
    let session = HttpApiSession::new(
        "provider_surface.google.direct_api".to_string(),
        "gemini-2.5-flash".to_string(),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent".to_string(),
        CredentialSource::ApiKey("test-key".to_string()),
        AiProviderType::Google,
        None,
        Arc::new(AiSessionConfig::default()),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "extract".to_string(),
        content_blocks: None,
    }];
    let rf = serde_json::json!({
        "schema": {"type": "object", "properties": {"name": {"type": "string"}}}
    });
    let options = RequestOptions {
        response_format: Some(&rf),
        tools: None,
    };
    let body = session.build_request_body(&messages, &options).unwrap();
    assert_eq!(body["generationConfig"]["responseMimeType"], "application/json");
    assert!(body["generationConfig"]["responseSchema"].get("type").is_some());
}

#[test]
fn anthropic_ignores_response_format() {
    let session = HttpApiSession::new(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(AiSessionConfig::default()),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "test".to_string(),
        content_blocks: None,
    }];
    let rf = serde_json::json!({"type": "json_schema"});
    let options = RequestOptions {
        response_format: Some(&rf),
        tools: None,
    };
    let body = session.build_request_body(&messages, &options).unwrap();
    assert!(body.get("response_format").is_none());
}
```

- [ ] **Step 6: Add thinking injection tests**

```rust
#[test]
fn anthropic_thinking_injected() {
    let mut config = AiSessionConfig::default();
    config.thinking = Some(serde_json::json!({"type": "adaptive"}));
    let session = HttpApiSession::new(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(config),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "think hard".to_string(),
        content_blocks: None,
    }];
    let body = session.build_request_body(&messages, &RequestOptions::default()).unwrap();
    assert_eq!(body["thinking"]["type"], "adaptive");
}

#[test]
fn openai_reasoning_injected() {
    let mut config = AiSessionConfig::default();
    config.thinking = Some(serde_json::json!({"effort": "high"}));
    let session = HttpApiSession::new(
        "provider_surface.openai.direct_api".to_string(),
        "gpt-5.4".to_string(),
        "https://api.openai.com/v1/chat/completions".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::OpenAi,
        None,
        Arc::new(config),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "reason".to_string(),
        content_blocks: None,
    }];
    let body = session.build_request_body(&messages, &RequestOptions::default()).unwrap();
    assert_eq!(body["reasoning"]["effort"], "high");
}

#[test]
fn google_thinking_config_injected() {
    let mut config = AiSessionConfig::default();
    config.thinking = Some(serde_json::json!({"thinking_budget": 2048}));
    let session = HttpApiSession::new(
        "provider_surface.google.direct_api".to_string(),
        "gemini-2.5-flash".to_string(),
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent".to_string(),
        CredentialSource::ApiKey("test-key".to_string()),
        AiProviderType::Google,
        None,
        Arc::new(config),
    );
    let messages = vec![ChatMessage {
        role: ChatRole::User,
        content: "think".to_string(),
        content_blocks: None,
    }];
    let body = session.build_request_body(&messages, &RequestOptions::default()).unwrap();
    assert_eq!(body["generationConfig"]["thinking_config"]["thinking_budget"], 2048);
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p oneshim-network --lib http_api_session && cargo check --workspace`
Expected: All pass, no compile errors.

- [ ] **Step 8: Commit**

```bash
git add crates/
git commit -m "feat: RequestOptions + structured output + thinking injection for all providers"
```

---

### Task 4: Thinking SSE Parsing

**Files:**
- Modify: `crates/oneshim-network/src/http_api_session.rs`

- [ ] **Step 1: Extend parse_anthropic_sse_event for thinking deltas**

In `parse_anthropic_sse_event`, add handling for `content_block_delta` with thinking:

```rust
"content_block_delta" => {
    let val: serde_json::Value = serde_json::from_str(data).ok()?;
    let delta = val.get("delta")?;
    let delta_type = delta.get("type")?.as_str()?;
    match delta_type {
        "text_delta" => {
            let text = delta.get("text")?.as_str()?.to_string();
            Some(OutboundMessage::Text { content: text, done: false })
        }
        "thinking_delta" => {
            let thinking = delta.get("thinking")?.as_str()?.to_string();
            Some(OutboundMessage::Thinking { content: thinking, done: false })
        }
        _ => None,
    }
}
```

Add handling for `content_block_stop` (thinking completion):
```rust
"content_block_stop" => {
    // Thinking blocks complete; emit done signal
    // NOTE: We can't distinguish text vs thinking block stop without tracking state.
    // For simplicity, we don't emit a separate done signal here — the stream
    // switches from Thinking to Text naturally as the model moves on.
    None
}
```

- [ ] **Step 2: Extend parse_google_sse_event for thinking parts**

Update `parse_google_sse_event` to check for both `text` and `thinking` in parts:

```rust
pub fn parse_google_sse_event(data: &str) -> Option<OutboundMessage> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return None;
    }

    let val: serde_json::Value = serde_json::from_str(trimmed).ok()?;

    let parts = val
        .get("candidates")?
        .get(0)?
        .get("content")?
        .get("parts")?
        .as_array()?;

    let usage = val.get("usageMetadata").and_then(|u| {
        let input = u.get("promptTokenCount")?.as_u64()?;
        let output = u.get("candidatesTokenCount")?.as_u64()?;
        Some(TokenUsage { input_tokens: input, output_tokens: output })
    });

    // Process the first meaningful part
    for part in parts {
        if let Some(thinking) = part.get("thinking").and_then(|t| t.as_str()) {
            if !thinking.is_empty() {
                return Some(OutboundMessage::Thinking {
                    content: thinking.to_string(),
                    done: usage.is_some(),
                });
            }
        }
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            if !text.is_empty() {
                if usage.is_some() {
                    return Some(OutboundMessage::Result {
                        content: text.to_string(),
                        done: true,
                        usage,
                    });
                }
                return Some(OutboundMessage::Text {
                    content: text.to_string(),
                    done: false,
                });
            }
        }
    }

    // Usage-only chunk (no text/thinking content)
    if let Some(usage) = usage {
        return Some(OutboundMessage::Result {
            content: String::new(),
            done: true,
            usage: Some(usage),
        });
    }

    None
}
```

- [ ] **Step 3: Update stream accumulation for thinking**

In the `try_stream!` block, update the match to NOT accumulate thinking content into the
text history (thinking is transient, not part of the conversation):

```rust
if let Some(msg) = parsed {
    match &msg {
        OutboundMessage::Text { content, .. } => {
            accumulated.push_str(content);
        }
        OutboundMessage::Thinking { .. } => {
            // Thinking tokens are streamed to frontend but not accumulated
            // into conversation history.
        }
        OutboundMessage::Result { .. } => {
            let assistant_msg = ChatMessage {
                role: ChatRole::Assistant,
                content: accumulated.clone(),
                content_blocks: None,
            };
            let mut hist = history.write().await;
            hist.push(assistant_msg);
            truncate_chat_history(&mut hist, max_turns);
        }
        _ => {}
    }
    yield msg;
}
```

- [ ] **Step 4: Add thinking SSE tests**

```rust
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
    let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
    let msg = parse_anthropic_sse_event("content_block_delta", data);
    match msg {
        Some(OutboundMessage::Text { content, .. }) => {
            assert_eq!(content, "Hello");
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn google_thinking_part() {
    let data = r#"{"candidates":[{"content":{"parts":[{"thinking":"Reasoning here..."}],"role":"model"}}]}"#;
    let msg = parse_google_sse_event(data);
    match msg {
        Some(OutboundMessage::Thinking { content, done }) => {
            assert_eq!(content, "Reasoning here...");
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
        Some(OutboundMessage::Text { content, .. }) => {
            assert_eq!(content, "Final answer");
        }
        other => panic!("expected Text, got {other:?}"),
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p oneshim-network --lib http_api_session`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-network/src/http_api_session.rs
git commit -m "feat: thinking/reasoning SSE parsing for Anthropic + Google"
```

---

### Task 5: Tool Calling — Request Building + SSE Parsing

**Files:**
- Modify: `crates/oneshim-network/src/http_api_session.rs`

- [ ] **Step 1: Add tool injection helpers**

```rust
/// Build Anthropic tool definitions from ToolDefinitions.
fn build_anthropic_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .filter_map(|t| {
            let schema = t.input_schema.as_ref()?;
            Some(serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": schema,
            }))
        })
        .collect()
}

/// Build OpenAI-compatible tool definitions from ToolDefinitions.
fn build_openai_tools(tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .filter_map(|t| {
            let schema = t.input_schema.as_ref()?;
            Some(serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": schema,
                }
            }))
        })
        .collect()
}

/// Build Google function declarations from ToolDefinitions.
fn build_google_tools(tools: &[ToolDefinition]) -> serde_json::Value {
    let declarations: Vec<serde_json::Value> = tools
        .iter()
        .filter_map(|t| {
            let schema = t.input_schema.as_ref()?;
            Some(serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": schema,
            }))
        })
        .collect();
    serde_json::json!([{"function_declarations": declarations}])
}
```

- [ ] **Step 2: Inject tools in build_request_body**

At the end of each match arm, before `Ok(body)`:

**Anthropic:**
```rust
if let Some(tools) = options.tools {
    let tool_defs = build_anthropic_tools(tools);
    if !tool_defs.is_empty() {
        body["tools"] = serde_json::Value::Array(tool_defs);
    }
}
```

**OpenAI:**
```rust
if let Some(tools) = options.tools {
    let tool_defs = build_openai_tools(tools);
    if !tool_defs.is_empty() {
        body["tools"] = serde_json::Value::Array(tool_defs);
    }
}
```

**Google:**
```rust
if let Some(tools) = options.tools {
    let tool_defs = build_google_tools(tools);
    if let Some(arr) = tool_defs.as_array() {
        if !arr.is_empty() && !arr[0]["function_declarations"].as_array().map_or(true, |a| a.is_empty()) {
            body["tools"] = tool_defs;
        }
    }
}
```

- [ ] **Step 3: Add PartialToolCall and extend Anthropic SSE parser**

```rust
/// Accumulator for streaming tool call arguments.
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}
```

Update `parse_anthropic_sse_event` to handle tool_use events:

```rust
"content_block_start" => {
    let val: serde_json::Value = serde_json::from_str(data).ok()?;
    let block = val.get("content_block")?;
    let block_type = block.get("type")?.as_str()?;
    if block_type == "tool_use" {
        let id = block.get("id")?.as_str()?.to_string();
        let name = block.get("name")?.as_str()?.to_string();
        Some(OutboundMessage::ToolCallDelta {
            index: 0,
            id,
            name,
            arguments_chunk: String::new(),
        })
    } else {
        None
    }
}
```

For `input_json_delta`, we handle accumulation in the stream loop, not in the parser.
Update `content_block_delta` to also detect `input_json_delta`:

```rust
"content_block_delta" => {
    let val: serde_json::Value = serde_json::from_str(data).ok()?;
    let delta = val.get("delta")?;
    let delta_type = delta.get("type")?.as_str()?;
    match delta_type {
        "text_delta" => {
            let text = delta.get("text")?.as_str()?.to_string();
            Some(OutboundMessage::Text { content: text, done: false })
        }
        "thinking_delta" => {
            let thinking = delta.get("thinking")?.as_str()?.to_string();
            Some(OutboundMessage::Thinking { content: thinking, done: false })
        }
        "input_json_delta" => {
            let partial = delta.get("partial_json")?.as_str()?.to_string();
            Some(OutboundMessage::ToolCallDelta {
                index: 0,
                id: String::new(),
                name: String::new(),
                arguments_chunk: partial,
            })
        }
        _ => None,
    }
}
```

- [ ] **Step 4: Add tool_calls handling to OpenAI SSE parser**

Extend `parse_openai_sse_event` to detect tool_calls in delta:

After the existing content extraction, add:

```rust
// Check for tool_calls in delta
if let Some(tool_calls) = val
    .get("choices")
    .and_then(|c| c.get(0))
    .and_then(|c| c.get("delta"))
    .and_then(|d| d.get("tool_calls"))
    .and_then(|tc| tc.as_array())
{
    for tc in tool_calls {
        let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
        let name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("").to_string();
        let args = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()).unwrap_or("").to_string();
        let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0);

        if !id.is_empty() || !name.is_empty() || !args.is_empty() {
            return Some(OutboundMessage::ToolCallDelta {
                index: index as u32,
                id,
                name,
                arguments_chunk: args,
            });
        }
    }
}

// Check for finish_reason: tool_calls
if let Some(finish) = val
    .get("choices")
    .and_then(|c| c.get(0))
    .and_then(|c| c.get("finish_reason"))
    .and_then(|f| f.as_str())
{
    if finish == "tool_calls" {
        return Some(OutboundMessage::Result {
            content: String::new(),
            done: true,
            usage: None,
        });
    }
}
```

- [ ] **Step 5: Add Google tool call parsing**

In `parse_google_sse_event`, add function_call detection before text/thinking:

```rust
// Check for function call
for part in parts {
    if let Some(fc) = part.get("functionCall") {
        let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
        let args = fc.get("args").cloned().unwrap_or(serde_json::json!({}));
        return Some(OutboundMessage::ToolUse {
            tool: name,
            input: Some(args),
            status: ToolUseStatus::Started,
            result: None,
        });
    }
}
```

- [ ] **Step 6: Update stream for tool accumulation**

In the `try_stream!` block, add `PartialToolCall` tracking and filter out internal
`__tool_json_delta` messages:

```rust
let mut tool_calls: Vec<PartialToolCall> = Vec::new();

// Inside the event loop, after parsing:
if let Some(msg) = parsed {
    match &msg {
        OutboundMessage::ToolCallDelta { index, id, name, arguments_chunk } => {
            let idx = *index as usize;
            if !id.is_empty() || !name.is_empty() {
                // New tool call or first chunk with id/name
                while tool_calls.len() <= idx {
                    tool_calls.push(PartialToolCall {
                        id: String::new(), name: String::new(), arguments: String::new(),
                    });
                }
                if !id.is_empty() { tool_calls[idx].id = id.clone(); }
                if !name.is_empty() { tool_calls[idx].name = name.clone(); }
            }
            if !arguments_chunk.is_empty() {
                if let Some(tc) = tool_calls.get_mut(idx) {
                    tc.arguments.push_str(arguments_chunk);
                } else if let Some(last) = tool_calls.last_mut() {
                    // Anthropic: index is always 0, append to last
                    last.arguments.push_str(arguments_chunk);
                }
            }
            // ToolCallDelta is internal — don't yield
            continue;
        }
        OutboundMessage::Text { content, .. } => {
            accumulated.push_str(content);
        }
        OutboundMessage::Thinking { .. } => {
            // Stream to frontend but don't accumulate in history
        }
        OutboundMessage::Result { .. } => {
            // Emit accumulated tool calls first
            for tc in tool_calls.drain(..) {
                if tc.name.is_empty() { continue; }
                let parsed_args = serde_json::from_str(&tc.arguments)
                    .unwrap_or(serde_json::json!({}));
                yield OutboundMessage::ToolUse {
                    tool: tc.name,
                    input: Some(parsed_args),
                    status: ToolUseStatus::Started,
                    result: None,
                };
            }
            // Save text history
            if !accumulated.is_empty() {
                let assistant_msg = ChatMessage {
                    role: ChatRole::Assistant,
                    content: accumulated.clone(),
                    content_blocks: None,
                };
                let mut hist = history.write().await;
                hist.push(assistant_msg);
                truncate_chat_history(&mut hist, max_turns);
            }
        }
        _ => {}
    }
    yield msg;
}
```

- [ ] **Step 7: Add tool calling tests**

```rust
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
        Some(OutboundMessage::ToolCallDelta { arguments_chunk, .. }) => {
            assert_eq!(arguments_chunk, r#"{"location":"#);
        }
        other => panic!("expected ToolCallDelta, got {other:?}"),
    }
}

#[test]
fn openai_tool_call_in_delta() {
    let data = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":""}}]}}]}"#;
    let msg = parse_openai_sse_event(data);
    match msg {
        Some(OutboundMessage::ToolCallDelta { index, id, name, .. }) => {
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
    let msg = parse_openai_sse_event(data);
    match msg {
        Some(OutboundMessage::Result { done, .. }) => {
            assert!(done);
        }
        other => panic!("expected Result with done=true, got {other:?}"),
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

#[test]
fn anthropic_tools_request_body() {
    let session = HttpApiSession::new(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(AiSessionConfig::default()),
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
        input_schema: Some(serde_json::json!({"type": "object", "properties": {"location": {"type": "string"}}})),
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
    let session = HttpApiSession::new(
        "provider_surface.openai.direct_api".to_string(),
        "gpt-5.4".to_string(),
        "https://api.openai.com/v1/chat/completions".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::OpenAi,
        None,
        Arc::new(AiSessionConfig::default()),
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
        input_schema: Some(serde_json::json!({"type": "object", "properties": {"location": {"type": "string"}}})),
    }];
    let options = RequestOptions {
        response_format: None,
        tools: Some(&tools),
    };
    let body = session.build_request_body(&messages, &options).unwrap();
    let api_tools = body["tools"].as_array().unwrap();
    assert_eq!(api_tools[0]["type"], "function");
    assert_eq!(api_tools[0]["function"]["name"], "get_weather");
}

#[test]
fn tools_without_schema_are_skipped() {
    let session = HttpApiSession::new(
        "provider_surface.anthropic.direct_api".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "https://api.anthropic.com/v1/messages".to_string(),
        CredentialSource::ApiKey("sk-test".to_string()),
        AiProviderType::Anthropic,
        None,
        Arc::new(AiSessionConfig::default()),
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
    assert!(body.get("tools").is_none());
}
```

- [ ] **Step 8: Run all tests**

Run: `cargo test -p oneshim-network --lib http_api_session`
Expected: All existing + new tests pass.

Run: `cargo clippy -p oneshim-network -- -D warnings`
Expected: No warnings.

- [ ] **Step 9: Commit**

```bash
git add crates/oneshim-network/src/http_api_session.rs
git commit -m "feat: tool calling request build + SSE parsing for Anthropic/OpenAI/Google"
```

---

### Task 6: Provider Catalog Update + Final Verification

**Files:**
- Modify: `specs/providers/provider-surface-catalog.json`

- [ ] **Step 1: Update structured_output flags**

Run:
```python
python3 -c "
import json
path = 'specs/providers/provider-surface-catalog.json'
with open(path) as f:
    data = json.load(f)
fix = {
    'provider_surface.openai.direct_api': True,
    'provider_surface.google.direct_api': True,
    'provider_surface.ollama.local_http': True,
    'provider_surface.generic.local_openai_compatible': True,
}
for s in data['surfaces']:
    sid = s.get('surface_id', '')
    if sid in fix:
        s.setdefault('llm_capabilities', {})['structured_output'] = fix[sid]
with open(path, 'w') as f:
    json.dump(data, f, indent=2, ensure_ascii=False)
    f.write('\n')
print('Updated structured_output for', list(fix.keys()))
"
```

- [ ] **Step 2: Update the catalog test assertion**

In `crates/oneshim-api-contracts/src/provider_specs/tests.rs`, change:
```rust
// structured_output is false until client-side implementation is complete
assert!(!openai.llm_capabilities.structured_output);
```
Back to:
```rust
assert!(openai.llm_capabilities.structured_output);
```

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass.

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -5`
Expected: No warnings.

- [ ] **Step 4: Commit**

```bash
git add specs/ crates/oneshim-api-contracts/
git commit -m "feat: re-enable structured_output in catalog after implementation"
```

---

### Task Summary

| Task | Feature | Tests Added | Files |
|------|---------|-------------|-------|
| 1 | ContentBlock model + config | 7 | ai_session.rs, ai_session config |
| 2 | Vision request building | 4 | http_api_session.rs |
| 3 | Structured output + thinking request | 6 | http_api_session.rs, settings |
| 4 | Thinking SSE parsing | 4 | http_api_session.rs |
| 5 | Tool calling request + SSE parsing | 8 | http_api_session.rs |
| 6 | Catalog update + verification | 0 | catalog, tests |
| **Total** | | **~29 new tests** | |
