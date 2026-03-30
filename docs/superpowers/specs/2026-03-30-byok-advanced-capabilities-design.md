# BYOK Provider Advanced Capabilities

> Date: 2026-03-30
> Status: Draft
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
    /// Plain text content (used when content_blocks is None).
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

**Backward compatibility:** When `content_blocks` is `None`, behavior is identical to current.
Existing code that reads `content` field continues to work. Only `build_request_body` checks
for `content_blocks` presence and switches serialization format.

#### 1.2 OutboundMessage — Thinking variant

Add `Thinking` variant to stream thinking tokens to the frontend:

```rust
pub enum OutboundMessage {
    // ... existing variants unchanged
    Thinking { content: String, done: bool },
}
```

#### 1.3 SessionMessage — Advanced feature configs

```rust
pub struct SessionMessage {
    // ... existing fields unchanged
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
}
```

`response_format` is passed through to `build_request_body` as-is. The frontend or caller
is responsible for building the correct provider-specific format. This avoids a complex
abstraction layer that would need constant updates as provider APIs evolve.

#### 1.4 AiSessionConfig — Thinking/reasoning config

```rust
pub struct AiSessionConfig {
    // ... existing fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,
}
```

Provider-specific thinking config (Anthropic `{"type":"adaptive"}`, OpenAI
`{"effort":"high"}`, Google `{"thinking_budget":1024}`) is stored as opaque JSON.
`build_request_body` injects it into the correct location per provider shape.

### 2. Request Building (http_api_session.rs)

**Signature change:** `build_request_body` gains an optional `RequestOptions` parameter:

```rust
struct RequestOptions {
    response_format: Option<serde_json::Value>,
    tools: Option<Vec<ToolDefinition>>,
}

fn build_request_body(
    &self,
    messages: &[ChatMessage],
    options: &RequestOptions,
) -> Result<serde_json::Value, CoreError>
```

`send_message` builds `RequestOptions` from the incoming `SessionMessage` before calling
`build_request_body`. Thinking config comes from `self.config.thinking` (session-level, not
per-message).

#### 2.1 Vision — Content block serialization

**Anthropic shape:**
```json
{"role": "user", "content": [
  {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "..."}},
  {"type": "text", "text": "Describe this"}
]}
```

**OpenAI-compatible shape:**
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

**Ollama exception:** Ollama uses a separate `images` array on the message object instead of
content blocks: `{"role": "user", "content": "Describe this", "images": ["base64..."]}`.
For Ollama, extract image data from `content_blocks` into the `images` field.

When `content_blocks` is `Some`, `build_request_body` serializes each block according to
the resolved `ProviderRequestShape`. When `None`, uses plain `content` string as today.

#### 2.2 Structured Output — response_format injection

If `SessionMessage.response_format` is `Some`:
- **Anthropic:** Ignored (not supported natively; use tool_use workaround via tools field).
- **OpenAI/OpenAI-Compatible:** Inject `response_format` at request body top level.
- **Google:** Extract `schema` → `generationConfig.responseSchema`, set
  `generationConfig.responseMimeType = "application/json"`.

#### 2.3 Thinking — Config injection

If `AiSessionConfig.thinking` is `Some`:
- **Anthropic:** Inject as top-level `thinking` field.
- **OpenAI:** Inject as top-level `reasoning` field.
- **Google:** Inject into `generationConfig.thinking_config`.
- **Others:** Silently ignored (no native support).

#### 2.4 Tool Calling — Tools injection

If `SessionMessage.tools` is `Some` and non-empty, inject tool definitions:
- **Anthropic:** `tools` array with `name`, `description`, `input_schema`.
- **OpenAI/Compatible:** `tools` array with `type: "function"`, `function: {name, description, parameters}`.
- **Google:** `tools: [{function_declarations: [{name, description, parameters}]}]`.

### 3. Response Parsing (SSE)

#### 3.1 Vision — No parsing change needed

Vision affects request format only. Responses remain text streams.

#### 3.2 Structured Output — No parsing change needed

Structured output content arrives as regular text (JSON string). Parsing is unchanged;
the caller validates the JSON against the expected schema.

#### 3.3 Thinking — New SSE event handling

**Anthropic:** Event type `content_block_delta` with `delta.type == "thinking_delta"`:
```json
{"type": "content_block_delta", "delta": {"type": "thinking_delta", "thinking": "..."}}
```
Emit `OutboundMessage::Thinking { content, done: false }`.

**Google:** Parts with `thinking` key instead of `text`:
```json
{"candidates": [{"content": {"parts": [{"thinking": "..."}]}}]}
```
Emit `OutboundMessage::Thinking` for thinking parts, `OutboundMessage::Text` for text parts.

**OpenAI:** Reasoning tokens are internal; not streamed to client. Only
`reasoning_tokens` count is available in usage. No parsing change needed.

#### 3.4 Tool Calling — New SSE event handling

**Anthropic:** Event type `content_block_start` with `content_block.type == "tool_use"`:
```json
{"type": "content_block_start", "content_block": {"type": "tool_use", "id": "...", "name": "..."}}
```
Then `content_block_delta` with `delta.type == "input_json_delta"` accumulates JSON input.
Emit `OutboundMessage::ToolUse` on `content_block_stop`.

**OpenAI/Compatible:** `choices[0].delta.tool_calls` array in streaming chunks:
```json
{"choices": [{"delta": {"tool_calls": [{"index": 0, "id": "...", "function": {"name": "...", "arguments": "..."}}]}}]}
```
Accumulate `arguments` across chunks, emit `OutboundMessage::ToolUse` on `finish_reason: "tool_calls"`.

**Google:** `function_call` in parts:
```json
{"candidates": [{"content": {"parts": [{"functionCall": {"name": "...", "args": {...}}}]}}]}
```
Emit `OutboundMessage::ToolUse` immediately.

### 4. Provider Catalog Updates

After implementation, update `structured_output` back to `true` for providers where it's
now actually implemented:
- OpenAI, Google, Mistral, xAI: `json_schema` support
- Groq, DeepSeek, Ollama: `json_object` support

### 5. File Changes Summary

| File | Change |
|------|--------|
| `oneshim-core/src/models/ai_session.rs` | Add `ContentBlock`, `content_blocks` to `ChatMessage`, `Thinking` to `OutboundMessage`, `response_format` to `SessionMessage`, `thinking` to `AiSessionConfig` |
| `oneshim-network/src/http_api_session.rs` | Extend `build_request_body` for vision/structured/thinking/tools per shape; extend `parse_*_sse_event` for thinking + tool_use streaming |
| `oneshim-api-contracts/src/settings.rs` | Add `thinking` to `AiSessionSettings` |
| `oneshim-web/src/services/settings_assembler.rs` | Map thinking config |
| `oneshim-web/src/services/settings_config_mutation.rs` | Apply thinking config |
| `specs/providers/provider-surface-catalog.json` | Flip `structured_output` back to `true` post-implementation |
| `oneshim-core/src/config/sections/ai_session.rs` | Add `thinking` field |

### 6. Testing Strategy

**Unit tests per feature per shape (target: ~40 tests):**

| Feature | Anthropic | OpenAI | Google | Total |
|---------|-----------|--------|--------|-------|
| Vision request build | 2 | 2 | 2 | 6 |
| Structured output request | 1 | 2 | 2 | 5 |
| Thinking request build | 2 | 2 | 2 | 6 |
| Thinking SSE parse | 3 | 1 | 2 | 6 |
| Tool calling request | 2 | 2 | 2 | 6 |
| Tool calling SSE parse | 3 | 2 | 2 | 7 |
| Backward compat (plain text) | 1 | 1 | 1 | 3 |
| ContentBlock serde roundtrip | - | - | - | 3 |
| **Total** | | | | **~42** |

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

### 8. Implementation Order

1. **ContentBlock model + vision** — Foundation for all other features
2. **Structured output** — Simplest (request-only change, no parsing)
3. **Thinking** — Request + parse change, 3 distinct formats
4. **Tool calling** — Most complex (request + stateful streaming parse)

Each step is independently testable and deployable.
