# AI Session Manager Phase 2b — HTTP/Ollama Adapters + REST Endpoints

> **Version**: 1.1
> **Date**: 2026-03-25
> **Scope**: HttpApiSession, LocalLlmSession adapters, oneshim-web REST endpoints, ChatMessage type
> **Depends on**: Phase 1 (PR #162, foundation), Phase 2a (Claude subprocess + wiring)
> **Deferred to Phase 3**: Codex resume adapter, Gemini CLI adapter, frontend UI

---

## 1. Goals

1. **HttpApiSession adapter** — Direct API calls (Anthropic/OpenAI/Google) with self-managed conversation history and SSE streaming
2. **LocalLlmSession adapter** — Ollama HTTP with self-managed conversation history and chunked streaming
3. **ChatMessage type** — Unified conversation history model for HTTP-based adapters
4. **oneshim-web REST endpoints** — `/api/ai/sessions` CRUD + SSE streaming for responses
5. **SessionManager trait extension** — Add `get_session` to trait + wire HttpApi/LocalLlm transports
6. **Web AppState integration** — Thread SessionManager from Tauri into oneshim-web (5 edit sites)

---

## 2. Non-Goals

- Codex resume adapter (needs `--last` session-id targeting verification)
- Gemini CLI adapter (not installed)
- Frontend session management UI
- Tool calling within HTTP API sessions (Phase 3)

---

## 3. Architecture

### 3.1 HTTP API Session Strategy

Self-managed conversation history + direct reqwest calls to provider APIs:

```
Turn 1: POST /v1/messages { messages: [{role:"user", content:"hello"}] }
         → SSE stream: content_block_delta events → collect full response
         → Append assistant message to history

Turn 2: POST /v1/messages { messages: [{role:"user",content:"hello"},{role:"assistant",content:"..."},{role:"user",content:"follow-up"}] }
         → SSE stream → append to history

Turn N: ... (history grows, truncated at max_history_turns)
```

**Key design decisions:**
- Reuse `reqwest::Client` from existing provider infrastructure (not `ApiClient` port which is for ONESHIM server)
- History truncation: drop oldest messages when exceeding `max_history_turns` (keep system prompt)
- SSE streaming: parse Anthropic SSE events into `OutboundMessage` in real-time
- Auth: use existing `CredentialSource::resolve_bearer_token()` to obtain API keys (same as `RemoteLlmProvider`)

### 3.2 Local LLM Session Strategy

Same history management pattern, targeting Ollama's `/api/chat` endpoint:

```
Turn 1: POST /api/chat { model: "llama3", messages: [{role:"user",content:"hello"}], stream: true }
         → NDJSON stream: {"message":{"content":"chunk"},"done":false}
         → Collect and append

Turn N: ... (same pattern)
```

### 3.3 ChatMessage Type

Both adapters need a simple role+content history type for API messages arrays:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}
```

This is distinct from `SessionMessage` (inbound JSONL with attachments/context) —
`ChatMessage` is the minimal format that HTTP APIs consume.

### 3.4 Web REST Endpoint Strategy

Add endpoints to existing `oneshim-web` using established patterns:

| Method | Path | Response | Handler |
|--------|------|----------|---------|
| `POST` | `/api/ai/sessions` | `Json<ConversationSessionInfo>` | Create session |
| `GET` | `/api/ai/sessions` | `Json<Vec<ConversationSessionInfo>>` | List sessions |
| `GET` | `/api/ai/sessions/{id}` | `Json<ConversationSessionInfo>` | Get session info |
| `DELETE` | `/api/ai/sessions/{id}` | `()` | Kill session |
| `POST` | `/api/ai/sessions/{id}/messages` | `Sse<impl Stream>` | Send message (SSE stream) |

The message endpoint returns **SSE** using Axum's `Sse` response type — same pattern
as existing `handlers/stream.rs` and `handlers/update.rs`.

### 3.5 Web AppState Threading

`oneshim-web::AppState` needs access to `SessionManagerImpl`.
Following existing pattern (`with_runtime_bindings`):

```rust
// oneshim-web/src/lib.rs — add to AppState
pub session_manager: Option<Arc<dyn SessionManager>>,

// Builder method
pub fn with_session_manager(mut self, sm: Arc<dyn SessionManager>) -> Self {
    self.session_manager = Some(sm);
    self
}
```

Use `Arc<dyn SessionManager>` (port trait) not concrete `SessionManagerImpl` — the web
crate depends on `oneshim-core` (ports), not `src-tauri` (concrete types).

**Required edit sites (5):**
1. `oneshim-web/src/lib.rs` — `AppState` struct: add `session_manager` field
2. `oneshim-web/src/lib.rs` — `WebServer::new()`: initialize to `None`
3. `src-tauri/src/web_server_runtime.rs` — `WebServerRuntimeBindings`: add field
4. `oneshim-web/src/lib.rs` — `with_runtime_bindings()`: wire field
5. Test `AppState` constructions in `oneshim-web` — add field to test fixtures

### 3.6 Web Context for AI Session Handlers

```rust
#[derive(Clone)]
pub struct AiSessionWebContext {
    pub session_manager: Option<Arc<dyn SessionManager>>,
}

impl FromRef<AppState> for AiSessionWebContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            session_manager: state.session_manager.clone(),
        }
    }
}
```

---

## 4. Adapter Implementation

### 4.1 HttpApiSession

```rust
// oneshim-network/src/http_api_session.rs
pub struct HttpApiSession {
    session_id: String,
    surface_id: String,                          // REQUIRED for HttpApi transport
    model: String,
    endpoint: String,
    credential: CredentialSource,                    // cloneable enum, per-request token resolution (same as RemoteLlmProvider)
    history: RwLock<Vec<ChatMessage>>,
    system_prompt: Option<String>,
    turn_count: AtomicU32,
    created_at: DateTime<Utc>,
    http_client: reqwest::Client,
    config: Arc<AiSessionConfig>,
}
```

**`surface_id` is required** for HttpApi transport — `SessionConfig.surface_id` must be
`Some(...)` when `transport == HttpApi`. `SessionManagerImpl::create_session` returns
an error if `surface_id` is `None` for HttpApi/LocalLlm transports.

**History truncation preserves system prompt:**
```rust
fn truncate_history(history: &mut Vec<ChatMessage>, max_turns: u32) {
    let max = max_turns as usize;
    if history.len() > max {
        // Keep first message (system prompt) + last (max-1) messages
        let drain_end = history.len() - max + 1;
        history.drain(1..drain_end);
    }
}
```

**`send_message` flow:**
1. Convert `SessionMessage` → `ChatMessage { role: User, content }`
2. Append to history
3. Build provider-specific request body (reuse `request.rs` patterns)
4. POST with `Accept: text/event-stream` (Anthropic) or streaming mode
5. Parse SSE/NDJSON events → yield `OutboundMessage` via `ResponseStream`
6. On completion, append assistant response to history
7. Truncate if `history.len() > max_history_turns`

**Provider-specific request shapes:**

Anthropic:
```json
{ "model": "claude-sonnet-4", "max_tokens": 4096, "stream": true,
  "system": "context...", "messages": [{"role":"user","content":"..."}] }
```

OpenAI:
```json
{ "model": "gpt-5.4", "max_tokens": 4096, "stream": true,
  "messages": [{"role":"system","content":"..."}, {"role":"user","content":"..."}] }
```

### 4.2 LocalLlmSession

```rust
// oneshim-network/src/local_llm_session.rs
pub struct LocalLlmSession {
    session_id: String,
    model: String,
    base_url: String,
    history: RwLock<Vec<ChatMessage>>,
    system_prompt: Option<String>,
    turn_count: AtomicU32,
    created_at: DateTime<Utc>,
    http_client: reqwest::Client,
    config: Arc<AiSessionConfig>,
}
```

**Ollama `/api/chat` request:**
```json
{ "model": "llama3", "stream": true,
  "messages": [{"role":"system","content":"..."}, {"role":"user","content":"..."}] }
```

**Ollama NDJSON response parsing:**
```jsonl
{"model":"llama3","message":{"role":"assistant","content":"chunk"},"done":false}
{"model":"llama3","message":{"role":"assistant","content":""},"done":true,"eval_count":50,"prompt_eval_count":20}

Token usage mapping: `eval_count` → `output_tokens`, `prompt_eval_count` → `input_tokens`.
```

### 4.3 SSE Response Normalization

Anthropic SSE events:
```
event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"chunk"}}

event: message_stop
data: {"type":"message_stop"}
```

→ `OutboundMessage::Text { content: "chunk", done: false }`
→ `OutboundMessage::Result { content: accumulated, done: true, usage }`

OpenAI SSE events:
```
data: {"choices":[{"delta":{"content":"chunk"}}]}
data: [DONE]
```

→ Same normalization pattern.

---

## 5. Required Dependency Additions

| Crate | Addition | Reason |
|-------|----------|--------|
| `oneshim-network/Cargo.toml` | `async-stream = { workspace = true }` | `ResponseStream` construction in adapters |
| `oneshim-network/Cargo.toml` | `uuid = { workspace = true }` | Session ID generation |

## 6. SessionManager Trait Extension

Add `get_session` to the `SessionManager` trait in `oneshim-core/src/ports/conversation_session.rs`:

```rust
#[async_trait]
pub trait SessionManager: Send + Sync {
    async fn create_session(&self, config: SessionConfig) -> Result<Arc<dyn ConversationSession>, CoreError>;
    async fn get_session(&self, session_id: &str) -> Result<Arc<dyn ConversationSession>, CoreError>;
    async fn kill_session(&self, session_id: &str) -> Result<(), CoreError>;
    async fn list_sessions(&self) -> Vec<ConversationSessionInfo>;
}
```

Move `get_session` from `SessionManagerImpl` concrete method to the trait so
`oneshim-web` can access it via `Arc<dyn SessionManager>`.

## 7. SSE Error Handling

The web message endpoint returns `Sse<impl Stream<Item = Result<Event, Infallible>>>`.
`ResponseStream` items are `Result<OutboundMessage, CoreError>`. The handler must convert:

```rust
let sse_stream = response_stream.map(|item| {
    let event = match item {
        Ok(msg) => Event::default().json_data(&msg).unwrap_or_else(|_| Event::default()),
        Err(err) => {
            let error_msg = OutboundMessage::Error {
                code: "stream_error".to_string(),
                message: err.to_string(),
                retryable: false,
            };
            Event::default().json_data(&error_msg).unwrap_or_else(|_| Event::default())
        }
    };
    Ok::<_, Infallible>(event)
});
Sse::new(sse_stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
```

## 8. Crate Placement (updated)

| Component | Crate | Reason |
|-----------|-------|--------|
| `ChatMessage`, `ChatRole` | `oneshim-core/models/ai_session.rs` | Shared model |
| `HttpApiSession` | `oneshim-network/src/http_api_session.rs` | HTTP adapter |
| `LocalLlmSession` | `oneshim-network/src/local_llm_session.rs` | HTTP adapter |
| Anthropic SSE normalizer | `oneshim-network/src/http_api_session.rs` | Provider-specific |
| Ollama NDJSON normalizer | `oneshim-network/src/local_llm_session.rs` | Provider-specific |
| AI session web handlers | `oneshim-web/src/handlers/ai_session.rs` | REST handlers |
| AI session web context | `oneshim-web/src/services/web_contexts/mod.rs` | Context extraction |
| Web route registration | `oneshim-web/src/routes.rs` | Route table |
| `SessionManagerImpl` update | `src-tauri/src/session_manager.rs` | Wire new transports |
| Web AppState threading | `src-tauri/src/web_server_runtime.rs` | DI wiring |

---

## 9. Testing Strategy

| Layer | What | How |
|-------|------|-----|
| **Unit** | ChatMessage serialization | Roundtrip JSON tests |
| **Unit** | Anthropic SSE → OutboundMessage | Feed sample SSE, assert variants |
| **Unit** | OpenAI SSE → OutboundMessage | Feed sample SSE, assert variants |
| **Unit** | Ollama NDJSON → OutboundMessage | Feed sample lines, assert variants |
| **Unit** | History truncation | Create session, add messages, verify truncation |
| **Unit** | Web handler routing | Mock SessionManager, verify response shapes |
| **Integration** | Full HTTP API session (if API key) | Create → send → list → kill |
| **Integration** | Web REST endpoints | HTTP client → oneshim-web → verify JSON/SSE |

---

## 10. Implementation Priority

| # | Task | Rationale |
|---|------|-----------|
| 1 | ChatMessage type | Foundation for all adapters |
| 2 | HttpApiSession (Anthropic) | Primary direct API path |
| 3 | HttpApiSession (OpenAI) | Second provider |
| 4 | LocalLlmSession (Ollama) | Local LLM users |
| 5 | SessionManager transport wiring | Connect adapters |
| 6 | Web handlers + routes | Dashboard access |
| 7 | Web AppState threading | DI completion |
| 8 | Workspace verification | Clean build |
