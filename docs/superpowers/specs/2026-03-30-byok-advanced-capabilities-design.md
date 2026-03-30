# BYOK Provider Advanced Capabilities

> Date: 2026-03-30
> Status: Draft v2 (post-review)
> Scope: Vision, Structured Output, Thinking/Reasoning, Tool Calling for HttpApiSession

## Problem

HttpApiSession supports only basic text messages across 10 BYOK providers. Four advanced
capabilities — vision, structured output, thinking, and tool calling — are declared in the
provider catalog but not implemented. This blocks multimodal AI features, structured data
extraction, chain-of-thought reasoning, and agentic workflows.

## Scope

**In scope:** HttpApiSession streaming path only (the user-facing chat session).
**Out of scope:** Non-streaming LlmProvider (automation intent interpreter), Subprocess/CLI transport.

### Provider Matrix

| Feature | Anthropic | OpenAI | Google | OpenAI-Compatible (7) |
|---------|-----------|--------|--------|----------------------|
| Vision | base64 content block | image_url (URL/b64) | inlineData (b64) | image_url (URL/b64) |
| Structured Output | tool_use workaround | json_schema | responseMimeType + schema | json_object / json_schema |
| Thinking | adaptive / budget | reasoning.effort | thinking_config | N/A (most) |
| Tool Calling | tools + input_schema | tools + function | functionDeclarations | OpenAI-compatible |

"OpenAI-Compatible (7)" = Groq, DeepSeek, Mistral, xAI, OpenRouter, NVIDIA, Ollama.

## Design

### 1. Model Layer Changes (oneshim-core)

#### 1.1 ChatMessage — Multi-content support

Current `ChatMessage` has `content: String`. Vision and tool calling require mixed content
blocks (text + image + tool_use in one message).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    /// Plain text content. Always populated (empty string for non-text messages).
    /// Used for history truncation, logging, and fallback.
    pub content: String,
    /// Rich content blocks for vision/tool messages.
    /// When present, build_request_body uses this instead of content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Image { media_type: String, data: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String },
    Thinking { thinking: String },
}
```

**Backward compatibility:** `#[serde(default)]` on `content_blocks` means old JSON without
this field deserializes to `None`. New JSON with `content_blocks: [...]` will fail on old
code, but ChatMessage is only used in-memory (conversation history in `Arc<RwLock<Vec>>>`).
It is NOT persisted to SQLite or disk — no migration needed.

**Tagged serde for ContentBlock:** Required because content blocks are serialized to JSON in
JSONL audit logs. Tagged format ensures correct round-trip deserialization.

#### 1.2 OutboundMessage — Thinking variant

Add `Thinking` variant to stream thinking tokens to the frontend:

```rust
pub enum OutboundMessage {
    // ... existing variants unchanged
    Thinking { content: String, done: bool },
}
```

**Streaming semantics:** `done: false` for each thinking delta chunk. `done: true` when
thinking block completes (Anthropic `content_block_stop`, Google final thinking part).

**Existing ToolUse variant:** The current `OutboundMessage::ToolUse` has fields
`{ tool, input, status, result }` designed for tool execution reporting. For SSE-parsed
tool calls from providers, we reuse this existing variant:
- `tool` = function name
- `input` = parsed JSON arguments
- `status` = `ToolUseStatus::Started` (provider requested tool call)
- `result` = `None` (not yet executed)

No new variant needed for tool calling — the existing shape is compatible.

#### 1.3 SessionMessage — Advanced feature configs

```rust
pub struct SessionMessage {
    // ... existing fields unchanged (role, content, attachments, tools, context)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
}
```

`response_format` is passed through to `build_request_body` as-is. The frontend or caller
is responsible for building the correct provider-specific format.

**Note:** `tools: Option<Vec<ToolDefinition>>` already exists on SessionMessage.

#### 1.4 ToolDefinition — Add input_schema for provider injection

Current `ToolDefinition` has `endpoint` and `method` for HTTP tool execution but lacks
the JSON Schema needed for provider API injection:

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub endpoint: String,
    #[serde(default = "default_http_method")]
    pub method: String,
    /// JSON Schema for tool parameters. Required for provider tool calling.
    /// Anthropic uses this as input_schema, OpenAI as function.parameters,
    /// Google as functionDeclarations[].parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}
```

**Dual use:** `endpoint`/`method` are for tool execution orchestration (out of scope).
`input_schema` is for declaring tool capabilities to the LLM provider (in scope).

#### 1.5 AiSessionConfig — Thinking/reasoning config

```rust
pub struct AiSessionConfig {
    // ... existing fields
    /// Provider-specific thinking/reasoning config. Opaque JSON injected into requests.
    /// Anthropic: {"type":"adaptive"}, OpenAI: {"effort":"high"},
    /// Google: {"thinking_budget":1024}
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,
}
```

**Granularity:** This is a global setting shared by all HttpApiSession instances via
`Arc<AiSessionConfig>`. This is intentional — thinking config is a user preference
(e.g., "always use extended thinking"), not a per-message toggle. The user can change it
via Settings UI at any time; active sessions pick up the change on next request.

### 2. Request Building (http_api_session.rs)

**Signature change:** `build_request_body` gains a `RequestOptions` parameter:

```rust
struct RequestOptions<'a> {
    response_format: Option<&'a serde_json::Value>,
    tools: Option<&'a [ToolDefinition]>,
}

fn build_request_body(
    &self,
    messages: &[ChatMessage],
    options: &RequestOptions<'_>,
) -> Result<serde_json::Value, CoreError>
```

**Attachment→ContentBlock conversion in send_message:**
Before calling `build_request_body`, `send_message` converts `SessionMessage.attachments`
to `ContentBlock` entries on the `ChatMessage`:

```rust
let mut blocks = vec![ContentBlock::Text { text: message.content.clone() }];
for att in &message.attachments {
    if let Attachment::Image { mime, data: Some(b64), .. } = att {
        blocks.push(ContentBlock::Image {
            media_type: mime.clone(),
            data: b64.clone(),
        });
    }
}
let user_msg = ChatMessage {
    role: ChatRole::User,
    content: message.content.clone(),
    content_blocks: if blocks.len() > 1 { Some(blocks) } else { None },
};
```

#### 2.1 Vision — Content block serialization

Each `build_request_body` match arm checks `content_blocks`. When `Some`, serialize
per-provider format instead of plain string:

**Anthropic shape:**
```json
{"role": "user", "content": [
  {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "..."}},
  {"type": "text", "text": "Describe this"}
]}
```

**OpenAI-compatible shape (including Ollama via openai_responses):**
```json
{"role": "user", "content": [
  {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,..."}},
  {"type": "text", "text": "Describe this"}
]}
```

**Google shape:**
```json
{"role": "user", "parts": [
  {"inlineData": {"mimeType": "image/jpeg", "data": "..."}},
  {"text": "Describe this"}
]}
```

**Note on Ollama:** The provider catalog maps Ollama to `openai_responses` shape.
Ollama's OpenAI-compatible endpoint (`/v1/chat/completions`) accepts the standard
OpenAI vision format. No special `images` array handling needed.

When `content_blocks` is `None`, uses plain `content` string as today (backward compatible).

#### 2.2 Structured Output — response_format injection

If `options.response_format` is `Some`:
- **Anthropic:** Silently ignored (not supported natively).
- **OpenAI/OpenAI-Compatible:** Inject `response_format` at request body top level.
- **Google:** Extract `schema` → `generationConfig.responseSchema`, set
  `generationConfig.responseMimeType = "application/json"`.

#### 2.3 Thinking — Config injection

If `self.config.thinking` is `Some`:
- **Anthropic:** Inject as top-level `thinking` field.
- **OpenAI:** Inject as top-level `reasoning` field.
- **Google:** Inject into `generationConfig.thinking_config`.
- **Others:** Silently ignored (no native support).

#### 2.4 Tool Calling — Tools injection

If `options.tools` is `Some` and non-empty, build provider-specific tool definitions from
each `ToolDefinition` that has `input_schema`:

- **Anthropic:** `tools: [{ name, description, input_schema }]`
- **OpenAI/Compatible:** `tools: [{ type: "function", function: { name, description, parameters } }]`
- **Google:** `tools: [{ function_declarations: [{ name, description, parameters }] }]`

Tools without `input_schema` are skipped (they're HTTP-execution-only tools).

### 3. Response Parsing (SSE)

#### 3.1 Vision / Structured Output — No parsing change needed

Vision affects request format only. Structured output arrives as regular text.

#### 3.2 Thinking — New SSE event handling

**Anthropic:** Event type `content_block_delta` with `delta.type == "thinking_delta"`:
```json
{"type": "content_block_delta", "delta": {"type": "thinking_delta", "thinking": "..."}}
```
Emit `OutboundMessage::Thinking { content, done: false }`.
On `content_block_stop` for a thinking block: emit `Thinking { content: "", done: true }`.

**Google:** Parts with `thinking` key instead of `text`:
```json
{"candidates": [{"content": {"parts": [{"thinking": "..."}]}}]}
```
Emit `OutboundMessage::Thinking` for thinking parts, `OutboundMessage::Text` for text parts.

**OpenAI:** Reasoning tokens are internal; not streamed. No parsing change needed.

#### 3.3 Tool Calling — SSE stateful accumulation

Tool call arguments are streamed as partial JSON across multiple SSE chunks. This requires
accumulation state in the `try_stream!` closure.

**New state variables in stream:**
```rust
let mut tool_calls: Vec<PartialToolCall> = Vec::new();

struct PartialToolCall {
    id: String,
    name: String,
    arguments: String, // Accumulated JSON string
}
```

**Anthropic flow:**
1. `content_block_start` with `type: "tool_use"` → push new `PartialToolCall { id, name }`
2. `content_block_delta` with `type: "input_json_delta"` → append to `arguments`
3. `content_block_stop` → parse accumulated `arguments` as JSON, emit `OutboundMessage::ToolUse`

**OpenAI/Compatible flow:**
1. `choices[0].delta.tool_calls[i]` with `id` + `function.name` → push new `PartialToolCall`
2. Subsequent chunks with same index → append to `arguments`
3. `finish_reason: "tool_calls"` → emit all accumulated `PartialToolCall`s as `OutboundMessage::ToolUse`

**Google flow:**
1. `functionCall` in parts → emit `OutboundMessage::ToolUse` immediately (no accumulation
   needed — Google sends complete function calls, not partial chunks)

**Bounded state:** `tool_calls: Vec` is bounded by the number of parallel tool calls per
response (typically 1-5). No unbounded growth risk.

### 4. Provider Catalog Updates

After implementation, update `structured_output` back to `true` for providers where it's
now actually implemented:
- OpenAI, Google, Mistral, xAI: `json_schema` support
- Groq, DeepSeek, Ollama: `json_object` support

### 5. File Changes Summary

| File | Change |
|------|--------|
| `oneshim-core/src/models/ai_session.rs` | Add `ContentBlock`, `content_blocks` to `ChatMessage`, `Thinking` to `OutboundMessage`, `response_format` to `SessionMessage`, `input_schema` to `ToolDefinition` |
| `oneshim-core/src/config/sections/ai_session.rs` | Add `thinking` field |
| `oneshim-network/src/http_api_session.rs` | Add `RequestOptions`, extend `build_request_body` for vision/structured/thinking/tools per shape; extend parsers for thinking + tool_use streaming; add `PartialToolCall` state |
| `oneshim-api-contracts/src/settings.rs` | Add `thinking` + `max_output_tokens` to `AiSessionSettings` |
| `oneshim-web/src/services/settings_assembler.rs` | Map thinking config |
| `oneshim-web/src/services/settings_config_mutation.rs` | Apply thinking config |
| `specs/providers/provider-surface-catalog.json` | Flip `structured_output` back to `true` post-implementation |

### 6. Testing Strategy

**Unit tests per feature per shape (target: ~45 tests):**

| Feature | Anthropic | OpenAI | Google | Total |
|---------|-----------|--------|--------|-------|
| Vision request build | 2 | 2 | 2 | 6 |
| Vision attachment conversion | - | - | - | 2 |
| Structured output request | 1 | 2 | 2 | 5 |
| Thinking request build | 2 | 2 | 2 | 6 |
| Thinking SSE parse | 3 | 1 | 2 | 6 |
| Tool calling request | 2 | 2 | 2 | 6 |
| Tool calling SSE parse (accumulation) | 3 | 3 | 1 | 7 |
| Backward compat (plain text, no options) | 1 | 1 | 1 | 3 |
| ContentBlock serde roundtrip | - | - | - | 3 |
| ToolDefinition with/without schema | - | - | - | 2 |
| **Total** | | | | **~46** |

**Integration test:** Build a request with all 4 features enabled simultaneously for each
provider shape. Verify JSON output matches expected format.

### 7. Non-Goals

- **Tool execution loop:** This spec covers tool *definition* passthrough and tool_use
  *response parsing*. Actual tool execution (HTTP call → tool_result → re-send) is a
  separate orchestration concern.
- **Provider-specific model validation:** We don't validate whether a specific model
  supports vision/thinking. The provider returns an error if unsupported.
- **Streaming tool result injection:** Re-sending tool results mid-stream requires session
  orchestration changes beyond HttpApiSession scope.
- **Ollama special image format:** Ollama uses OpenAI-compatible vision via catalog-mapped
  `openai_responses` shape. No special handling.

### 8. Implementation Order

1. **ContentBlock model + attachment conversion + vision request building** — Foundation
2. **Structured output (response_format injection)** — Request-only, no parsing
3. **Thinking (request injection + SSE parsing)** — 3 provider formats
4. **Tool calling (request injection + stateful SSE parsing)** — Most complex

Each step is independently testable and deployable.
