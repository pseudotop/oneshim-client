# AI Session Manager — Technical Specification

> **Version**: 1.3
> **Date**: 2026-03-24
> **Scope**: Unified AI conversation session management with CLI subprocess optimization
> **Supersedes**: NON-INTERACTIVE-OVERHEAD-OPTIMIZATION-SPEC.md (partially — CLI invocation aspects)

---

## 1. Problem

ONESHIM's current AI integration has two limitations:

1. **Startup overhead**: Every CLI invocation (Claude/Codex/Gemini) spawns a new process with full initialization (config loading, MCP connection, skills parsing, auth check). For proactive calls that happen every few seconds, this overhead is significant.

2. **No interactive sessions**: The current architecture is one-shot only (prompt → JSON → exit). Users cannot continue a conversation after receiving a suggestion — e.g., asking follow-up questions, requesting clarification, or leveraging CLI features like web search and skills.

**Measured impact**: Simple intent planning calls spend ~30-40% of context window on infrastructure metadata (Skills list, MCP tools, deferred tools). Process spawn + CLI initialization adds 2-5 seconds per invocation.

---

## 2. Goals

1. **Unified session interface** — Same user experience regardless of backend (CLI subprocess, direct API, local LLM)
2. **Long-lived CLI processes** — Eliminate repeated startup overhead via persistent subprocesses
3. **Bidirectional conversation** — Support multi-turn dialogue after initial suggestion
4. **Rich attachments** — Files, images, directories, skills, app references in messages
5. **Full audit trail** — All session events recorded for debugging and analysis
6. **One-shot optimization** — Minimize overhead for automated proactive calls
7. **Configurable resource limits** — User-controlled concurrent sessions, timeouts, retention

---

## 3. Non-Goals

- Changing the MCP protocol itself
- Modifying CLI tools (Claude/Codex/Gemini) internals
- Replacing the existing one-shot `SubprocessLlmProvider`/`SubprocessOcrProvider` (they remain for proactive calls)
- Server-side session management (this is client-only)

---

## 4. Architecture

### 4.1 Overview

Two independent tracks coexist:

```
┌──────────────────────────────────────────────────────────────┐
│                       ONESHIM Client                         │
│                                                              │
│  oneshim-core (ports)                                        │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ trait ConversationSession          trait SessionManager │  │
│  │   send_message() → ResponseStream    create_session()  │  │
│  │   get_context() → SessionContext     kill_session()    │  │
│  │   session_id()                       list_sessions()   │  │
│  │   provider_name()                    idle reaping      │  │
│  └────────────────────────────────────────────────────────┘  │
│           ▲              ▲              ▲                     │
│           │              │              │                     │
│  ┌────────┴───┐  ┌───────┴────┐  ┌─────┴──────┐             │
│  │ Subprocess │  │ HttpApi    │  │ LocalLlm   │  adapters   │
│  │ Session    │  │ Session    │  │ Session    │             │
│  │            │  │            │  │            │             │
│  │ claude CLI │  │ Anthropic  │  │ Ollama     │             │
│  │ codex CLI  │  │ OpenAI API │  │ llama.cpp  │             │
│  │ gemini CLI │  │ Google API │  │            │             │
│  │            │  │            │  │            │             │
│  │ stdin/out  │  │ HTTP+hist  │  │ HTTP+hist  │             │
│  │ JSONL      │  │ managed    │  │ managed    │             │
│  └────────────┘  └────────────┘  └────────────┘             │
│                                                              │
│  ┌──────────────┐    ┌──────────────┐                        │
│  │ Context      │    │ oneshim-web  │                        │
│  │ Assembler    │    │ :10090 REST  │                        │
│  │ Push: system │    │ Pull: CLI    │                        │
│  │ prompt build │    │ runtime query│                        │
│  └──────────────┘    └──────────────┘                        │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐    │
│  │ One-shot Providers (existing + optimized flags)       │    │
│  │ SubprocessLlmProvider / SubprocessOcrProvider         │    │
│  │ HttpApiLlmProvider / OllamaOcrProvider               │    │
│  └──────────────────────────────────────────────────────┘    │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐    │
│  │ AuditingSessionDecorator (wraps all sessions)         │    │
│  │ → SqliteAuditSink / BatchAuditSink                    │    │
│  └──────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

### 4.2 Port Definitions (oneshim-core)

```rust
use std::pin::Pin;
use futures_core::Stream;  // requires `futures-core` in oneshim-core/Cargo.toml

/// Streaming response from a conversation session.
/// Each item is a parsed outbound JSONL message.
pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<OutboundMessage, CoreError>> + Send>>;

/// Transport mechanism for a session (distinct from vendor-level AiProviderType)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionTransport {
    /// Long-lived CLI subprocess (stdin/stdout JSONL)
    Subprocess,
    /// Direct HTTP API calls (conversation history self-managed)
    HttpApi,
    /// Local LLM server (Ollama HTTP, conversation history self-managed)
    LocalLlm,
}

/// Configuration for creating a new session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub transport: SessionTransport,
    pub surface_id: Option<String>,      // e.g. "provider_surface.anthropic.subprocess_cli"
    pub model: Option<String>,           // model override
    pub system_prompt: Option<String>,   // override SessionContextAssembler output
    pub tools_enabled: bool,             // enable Pull API tools
}

/// Session context snapshot — used by both get_context() and list_sessions().
/// Contains all session metadata. `turn_count` is the only field that
/// changes frequently; keeping one type avoids near-duplicate maintenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSessionInfo {
    pub session_id: String,
    pub provider_name: String,
    pub model: String,
    pub state: SessionState,
    pub transport: SessionTransport,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub turn_count: u32,
}

/// Session lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Starting,
    Active,
    Idle,
    Recovering,
    Failed,
    Terminated,
}

#[async_trait]
pub trait ConversationSession: Send + Sync {
    /// Send a message with optional attachments, receive streaming response
    async fn send_message(
        &self,
        message: &SessionMessage,
    ) -> Result<ResponseStream, CoreError>;

    /// Current session info (synchronous — returns locally held state)
    fn info(&self) -> ConversationSessionInfo;

    /// Unique session identifier
    fn session_id(&self) -> &str;

    /// Provider display name
    fn provider_name(&self) -> &str;
}

#[async_trait]
pub trait SessionManager: Send + Sync {
    /// Create a new session with the given provider.
    /// Returns the auditing-wrapped session ready for use.
    async fn create_session(
        &self,
        config: SessionConfig,
    ) -> Result<Arc<dyn ConversationSession>, CoreError>;

    /// Terminate a session
    async fn kill_session(&self, session_id: &str) -> Result<(), CoreError>;

    /// List active sessions
    async fn list_sessions(&self) -> Vec<ConversationSessionInfo>;
}
```

**Dependency note**: `oneshim-core/Cargo.toml` must add `futures-core = "0.3"` (minimal
crate, no full `futures` needed). Alternatively, `tokio-stream` re-exports `Stream` and
is already transitively available via `tokio`.

**Naming note**: `ConversationSessionInfo` avoids collision with the existing
`SessionInfo` in `oneshim-core/src/models/session.rs` (which represents server
connection sessions with `user_id`, `ConnectionHealth`, etc.).

**Note on audit integration**: The `SessionManager` implementation wraps every session
with `AuditingSession` before returning it. The existing `AuditLogPort` trait
(`oneshim-core/src/ports/audit_log.rs`) is extended with a `record_session_event` method
rather than introducing a separate `AuditSink` trait. This reuses the existing
`AuditEntry` model and `AuditLogger` batch infrastructure.

```rust
// Extension to existing AuditLogPort (oneshim-core/src/ports/audit_log.rs)
#[async_trait]
pub trait AuditLogPort: Send + Sync {
    // ... existing methods ...

    /// Record a session audit event (best-effort: logs warning on failure, never propagates error)
    async fn record_session_event(&self, entry: SessionAuditEntry);
}
```

### 4.3 Session Isolation

Each conversation session runs in its own OS subprocess (for CLI backends) or its own HTTP connection context (for API backends). This provides:

- **Memory isolation**: No shared state between sessions
- **Crash isolation**: One session failure does not affect others
- **Data isolation**: Conversation context stays within the process boundary

Shared data (conversation history, action events, user profile, regime state) is accessed through:
- **Push**: ContextAssembler injects into system prompt at session creation
- **Pull**: CLI tools query `localhost:10090` REST API at runtime

---

## 5. JSONL Protocol

All session communication uses a unified JSONL (newline-delimited JSON) format. Each adapter normalizes its backend's native output to this format.

### 5.1 Inbound Messages (ONESHIM → Session)

```jsonl
{"type":"message","role":"system","content":"...","attachments":[],"tools":[...],"context":null}
{"type":"message","role":"user","content":"이 화면 분석해줘","attachments":[{"kind":"image","mime":"image/png","path":"/tmp/screen.png","data":"base64..."}],"context":{"regime":"deep_work","active_app":"VSCode"}}
{"type":"control","action":"cancel"}
{"type":"control","action":"ping"}
```

### 5.2 Outbound Messages (Session → ONESHIM)

```jsonl
{"type":"text","content":"화면을 분석하겠습니다.","done":false}
{"type":"tool_use","tool":"web_search","input":{"query":"rust patterns"},"status":"started"}
{"type":"tool_use","tool":"web_search","input":null,"status":"completed","result":"..."}
{"type":"tool_use","tool":"oneshim_api","input":{"endpoint":"/api/suggestions"},"status":"completed","result":"..."}
{"type":"result","content":"분석 완료입니다.","done":true,"usage":{"input_tokens":1200,"output_tokens":340}}
{"type":"error","code":"rate_limit","message":"API rate limit exceeded","retryable":true}
{"type":"control","action":"pong"}
```

### 5.3 Message Types

| Direction | Type | Purpose |
|-----------|------|---------|
| Inbound | `message` | User/system message with attachments |
| Inbound | `control` | cancel, ping |
| Outbound | `text` | Streaming text chunk |
| Outbound | `tool_use` | CLI internal tool call status |
| Outbound | `result` | Response complete (with usage) |
| Outbound | `error` | Error (retryable flag) |
| Outbound | `control` | pong |

### 5.4 Attachment Types

| Kind | Required Fields | Optional Fields | Adapter Conversion |
|------|----------------|-----------------|-------------------|
| `image` | `mime`, `path` or `data` | `data` (base64) | CLI: `--image`, API: content block |
| `file` | `path` | `mime`, `data` | CLI: content injection/workdir, API: prompt |
| `directory` | `path` | — | CLI: `--cwd`/tree, API: tree in prompt |
| `skill` | `skill_id` | `display_name` | CLI: `--tools` definition, API: tools param |
| `app_reference` | `app_name` | `window_title` | Prompt context injection |

### 5.5 Protocol Rules

| Rule | Description |
|------|-------------|
| Message boundary | `\n` delimiter, one line = one JSON object |
| Response completion | `done:true` or `type:"result"` |
| Error handling | `retryable:true` → auto retry, `false` → user notification |
| Health check | `ping`/`pong` for process liveness |
| Attachment size | `image` data max 10MB, `file` prefers path over data |
| Normalization | Each adapter converts native output → unified format |

### 5.6 Adapter Normalization

```
Claude stream-json event  → normalize_claude()  ─┐
Codex JSON stdout          → normalize_codex()   ─┼─→ Unified JSONL
Gemini stdout              → normalize_gemini()  ─┤
Ollama /api/chat response  → normalize_ollama()  ─┤
Direct API HTTP response   → normalize_http()    ─┘
```

---

## 6. Context Assembly (Push + Pull)

### 6.1 Push: SessionContextAssembler

Assembles system prompt at session creation. Lives in `src-tauri/` (binary crate)
because it requires concrete infrastructure types for DI wiring. This is separate from
the existing `ContextAssembler` in `oneshim-analysis/src/assembler.rs` which handles
frame-level context assembly with PII filtering.

```rust
// src-tauri/src/session_context.rs
pub struct SessionContextAssembler {
    // Uses concrete Arc<SqliteStorage> per accepted deviation in CLAUDE.md
    // (SqliteStorage implements 10+ disjoint port traits — single trait object cannot represent this)
    storage: Arc<SqliteStorage>,
    config: Arc<AppConfig>,
    regime_state: Arc<SharedRegimeState>,
}

/// Data model (oneshim-core/src/models/session.rs)
pub struct SystemPromptContext {
    pub user_profile: UserProfileSummary,       // config → preferences
    pub current_regime: String,                 // regime_state → deep_work, browsing, etc.
    pub recent_activity: ActivitySummary,        // storage → last 30min activity
    pub suggestion_history: SuggestionPatterns,  // storage → recent accept/reject patterns
    pub available_skills: Vec<SkillInfo>,        // config → enabled skills
    pub system_info: SystemInfo,                 // OS, active app, timezone
}
```

**Injection timing:**
- Session creation → `type:"message", role:"system"`
- Regime change → additional system message with updated context

### 6.2 Pull: oneshim-web REST API

CLI sessions query runtime data via tools mapped to existing endpoints:

| Existing Endpoint | Purpose | CLI Tool Name |
|-------------------|---------|---------------|
| `GET /api/metrics` | System metrics (CPU, memory, disk) | `oneshim_metrics` |
| `GET /api/sessions/{id?}` | Work session history | `oneshim_sessions` |
| `GET /api/suggestions` | Suggestion history | `oneshim_suggestions` |
| `GET /api/frames` | Captured frames | `oneshim_frames` |
| `GET /api/events` | Event log | `oneshim_events` |
| `GET /api/stats/summary` | Activity statistics summary | `oneshim_stats` |
| `GET /api/stats/apps` | Per-app usage statistics | `oneshim_app_stats` |
| `GET /api/stats/heatmap` | Activity heatmap data | `oneshim_heatmap` |
| `GET /api/focus/metrics` | Focus metrics | `oneshim_focus_metrics` |
| `GET /api/focus/sessions` | Focus session history | `oneshim_focus_sessions` |
| `GET /api/focus/interruptions` | Interruption log | `oneshim_interruptions` |
| `GET /api/tags` | Tags | `oneshim_tags` |

Tool definitions are sent with the initial system message at session creation only
(not per-message). When the CLI invokes a tool, ONESHIM proxies the request to
`localhost:10090` and returns the result to the session.

**Inbound JSONL clarification**: The `tools` field in Section 5.1 appears only in
the initial `role:"system"` message. Subsequent `role:"user"` messages do not
include `tools`.

---

## 7. One-shot Optimization

Existing `SubprocessLlmProvider`/`SubprocessOcrProvider` remain for proactive automated calls, with additional flags to minimize overhead.

### 7.1 Optimized CLI Flags

#### Claude CLI
```bash
# Current
claude -p --permission-mode dontAsk --tools "" --no-session-persistence \
  --output-format text --json-schema <schema> "<prompt>"

# Optimized (added flags)
claude -p --permission-mode dontAsk --tools "" --no-session-persistence \
  --output-format stream-json --json-schema <schema> \
  --no-mcp --no-hooks --max-turns 1 "<prompt>"
```

#### Codex CLI
```bash
# Current (already well-optimized)
codex exec --sandbox read-only --skip-git-repo-check --color never \
  -C <workdir> --output-schema <schema> --output-last-message <output> -

# Optimized (added flag)
codex exec --sandbox read-only --skip-git-repo-check --color never \
  --no-browser-login -C <workdir> \
  --output-schema <schema> --output-last-message <output> -
```

#### Gemini CLI
```bash
# Current
gemini -p "<prompt>" --output-format json

# Optimized (added flags)
gemini -p "<prompt>" --output-format json --non-interactive --sandbox
```

### 7.2 Catalog Schema Extension

`provider-surface-catalog.json` `subprocess_transport` gains mode-specific flags:

```json
{
  "subprocess_transport": {
    "tool_id": "claude-code",
    "executable_candidates": ["claude", "claude-code"],
    "invocation_mode": "claude_print_json",
    "model_flag": "--model",
    "json_output_supported": true,
    "oneshot_flags": [
      "--no-session-persistence", "--no-mcp", "--no-hooks",
      "--max-turns", "1", "--permission-mode", "dontAsk",
      "--tools=", "--output-format", "stream-json"
    ],
    "session_flags": [
      "--output-format", "stream-json",
      "--permission-mode", "dontAsk"
    ]
  }
}
```

### 7.3 Interactive Session Flags (Comparison)

| Flag | One-shot | Interactive |
|------|----------|-------------|
| `--tools` | `""` (disabled) | oneshim tool definitions |
| `--no-session-persistence` | Yes | No (session maintained) |
| `--no-mcp` | Yes | Optional |
| `--no-hooks` | Yes | Optional |
| `--output-format` | `stream-json` | `stream-json` |
| `--max-turns` | `1` | Unlimited |

---

## 8. Auditing

All session events are recorded via a decorator pattern that wraps every session transparently.

### 8.1 Audit Categories

| Category | Events | Recorded Data |
|----------|--------|---------------|
| **Session lifecycle** | create, terminate, crash, idle_timeout | session_id, cli_type, provider, start/end time, exit reason, exit_code |
| **Messages** | inbound, outbound | session_id, role, content summary/hash, attachment metadata, timestamp |
| **Tool calls** | tool_use started/completed/failed | session_id, tool_name, input summary, result summary, duration |
| **Attachments** | attachment resolved/failed | session_id, kind, path, mime, size, conversion result |
| **Errors** | error, retry, fallback | session_id, error_code, message, retryable, retry count |
| **Process** | spawn, health_check, kill | PID, memory usage, startup duration |
| **Token usage** | usage per turn | session_id, input_tokens, output_tokens, provider, model |
| **Context assembly** | context_assembled | session_id, included sections, prompt token count |
| **Pull API** | api_proxy_request | session_id, endpoint, status_code, duration |

### 8.2 Decorator Pattern

Uses `Arc<dyn ConversationSession>` directly (not generic `<S>`) to match the project's
DI pattern. Audit recording is **best-effort**: failures are logged as warnings but never
propagate to the caller, following the convention set by `AuditLogger` in
`oneshim-automation/audit.rs`.

```rust
// src-tauri/src/auditing_session.rs
pub struct AuditingSession {
    inner: Arc<dyn ConversationSession>,
    audit: Arc<dyn AuditLogPort>,
}

#[async_trait]
impl ConversationSession for AuditingSession {
    async fn send_message(&self, message: &SessionMessage) -> Result<ResponseStream, CoreError> {
        let start = Instant::now();
        // Best-effort audit — log warning on failure, never propagate
        self.audit.record_session_event(SessionAuditEntry::message_sent(
            self.session_id(), message
        )).await;

        let result = self.inner.send_message(message).await;

        self.audit.record_session_event(SessionAuditEntry::message_result(
            self.session_id(), start.elapsed(), &result
        )).await;
        result
    }

    fn info(&self) -> ConversationSessionInfo {
        self.inner.info()
    }

    fn session_id(&self) -> &str {
        self.inner.session_id()
    }

    fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }
}
```

SessionManager automatically wraps every session with `AuditingSession` at creation — no adapter code changes required.

### 8.3 Storage

- **SQLite**: New `session_audit_log` table (migration **V20** — V18=trigram FTS5, V19=app_meta)
- **Batch sink**: Reuses existing `AuditLogger` from `oneshim-automation/audit.rs` for server transmission
- **Retention**: Follows existing 30-day / 500MB policy

---

## 9. Error Recovery + Process Lifecycle

### 9.1 Session State Machine

```
     spawn()          first message       idle_timeout
  ┌──────────┐     ┌──────────┐      ┌──────────────┐
  │ Starting │────▶│  Active  │─────▶│    Idle      │
  └──────────┘     └──────────┘      └──────────────┘
       │                │                    │
       │ spawn fail     │ crash/error        │ timeout expired
       ▼                ▼                    ▼
  ┌──────────┐     ┌──────────┐      ┌──────────────┐
  │  Failed  │     │Recovering│─────▶│ Terminated   │
  └──────────┘     └──────────┘      └──────────────┘
                     retry OK │
                        │
                        ▼
                   ┌──────────┐
                   │  Active  │
                   └──────────┘
```

### 9.2 Error Scenarios

| Scenario | Detection | Recovery | Max Retries |
|----------|-----------|----------|-------------|
| Process crash | `wait()` abnormal exit code | Respawn + conversation history re-injection | 3 |
| Response timeout | `tokio::time::timeout` | cancel → resend | 2 |
| Stdin pipe broken | `BrokenPipe` IO error | Terminate + respawn | 3 |
| Stdout EOF | `read_line` → `None` | Confirm termination + respawn | 3 |
| Rate limit | `error.code == "rate_limit"` | Exponential backoff (1s→30s) | 5 |
| Auth expired | `error.code == "auth_expired"` | Auth re-probe + reconnect | 1 |
| OOM kill | exit code 137 (SIGKILL) | Warning log + respawn (suggest memory limit adjustment) | 1 |
| Ping unresponsive | No `pong` within 5s | Force kill + respawn | 2 |

### 9.3 Crash Recovery Flow

```
Process crash detected
    │
    ▼
Audit record (crash, exit_code, last 100 lines stderr)
    │
    ▼
retry_count < max_retries?
    ├── Yes: Spawn new process
    │         ├── ContextAssembler rebuilds system prompt
    │         ├── conversation_history re-injected (summarized)
    │         ├── User notified: "session recovering"
    │         └── state → Recovering → Active
    │
    └── No: state → Failed
            ├── User notified: "session recovery failed"
            └── Prompt to create new session
```

### 9.4 Resource Protection

| Protection | Mechanism |
|------------|-----------|
| Concurrent session cap | `max_concurrent_sessions` exceeded → terminate oldest Idle session |
| Memory | Per-process RSS monitoring (sysinfo), warning at threshold |
| Conversation history | LruCache(100 turns), excess → summary compression |
| Pipe buffer | BufReader/BufWriter, 64KB buffer |
| Idle reaping | Background task checks `last_active` every 30s |

### 9.5 SessionManager Implementation

```rust
// src-tauri/src/session_manager.rs
pub struct SessionManagerImpl {
    sessions: RwLock<HashMap<String, ManagedSession>>,
    config: Arc<AiSessionConfig>,
    audit: Arc<dyn AuditLogPort>,
    context_assembler: Arc<SessionContextAssembler>,
}

struct ManagedSession {
    /// The auditing-wrapped session — stored as Arc<dyn ConversationSession>
    /// following the project's DI convention (Arc<dyn T>, never concrete types)
    session: Arc<dyn ConversationSession>,
    state: SessionState,
    pid: Option<u32>,                          // For subprocess sessions
    created_at: Instant,
    last_active: Instant,
    retry_count: u32,
    conversation_history: Vec<SessionMessage>,  // For crash recovery
}
```

### 9.6 Graceful Shutdown

On application shutdown (integrated with `src-tauri/src/lifecycle.rs`):

1. Send `control:cancel` to all Active sessions
2. Wait up to 5 seconds for in-flight responses to complete
3. Force-kill remaining subprocess processes
4. Audit record all terminations with reason `"app_shutdown"`

---

## 10. Configuration

New config section at `oneshim-core/src/config/sections/ai_session.rs`.
Must be registered in `sections/mod.rs` and added as a field to `AppConfig` in `config/mod.rs`
(following the directory module pattern per ADR-003):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSessionConfig {
    /// Maximum concurrent sessions (default: 3)
    pub max_concurrent_sessions: u32,
    /// Idle session timeout in seconds (default: 300)
    pub idle_timeout_secs: u64,
    /// One-shot invocation timeout in seconds (default: 60)
    pub oneshot_timeout_secs: u64,
    /// Interactive session response timeout in seconds (default: 600)
    pub session_timeout_secs: u64,
    /// Audit log retention in days (default: 30)
    pub audit_retention_days: u32,
    /// Maximum attachment size in bytes (default: 10MB)
    pub max_attachment_bytes: u64,
    /// Health check interval in seconds (default: 30)
    pub health_check_interval_secs: u64,
    /// Maximum conversation history turns kept for crash recovery (default: 100)
    pub max_history_turns: u32,
}
```

---

## 11. Crate Placement

Following Hexagonal Architecture, new code is placed as:

| Component | Crate | Reason |
|-----------|-------|--------|
| `ConversationSession` trait | `oneshim-core/ports/` | Port definition |
| `SessionManager` trait | `oneshim-core/ports/` | Port definition |
| `AuditLogPort` extension | `oneshim-core/ports/` | Extends existing port (no new `AuditSink` trait) |
| JSONL message types | `oneshim-core/models/` | Shared domain models |
| `SessionMessage`, `Attachment` | `oneshim-core/models/` | Shared domain models |
| `ConversationSessionInfo`, `SessionConfig`, `SessionState`, `SessionTransport` | `oneshim-core/models/` | Shared domain models |
| `SystemPromptContext`, `UserProfileSummary`, `ActivitySummary`, `SuggestionPatterns`, `SkillInfo`, `SystemInfo` | `oneshim-core/models/` | Context assembly data models |
| `ResponseStream` type alias | `oneshim-core/models/` | Shared type |
| `AiSessionConfig` | `oneshim-core/config/sections/` | Config section |
| `SubprocessSession` adapter | `src-tauri/subprocess_provider/` | Process management belongs in binary crate |
| `HttpApiSession` adapter | `oneshim-network/` | HTTP communication is network adapter responsibility |
| `LocalLlmSession` adapter | `oneshim-network/` | Ollama uses HTTP — same as other HTTP adapters |
| `SessionContextAssembler` | `src-tauri/` | Concrete wiring with infra types |
| `SessionManagerImpl` | `src-tauri/` | Binary crate wiring |
| `AuditingSession` decorator | `src-tauri/` | Binary crate wiring |
| `session_audit_log` migration V20 | `oneshim-storage/migration.rs` | Schema change |
| Catalog schema extension | `specs/providers/provider-surface-catalog.json` | Data file |

**Placement rationale for adapter split:**
- `SubprocessSession` → `src-tauri/`: Requires `tokio::process::Command` and OS-level process
  management. The binary crate already owns process lifecycle (`subprocess_provider/`).
- `HttpApiSession`/`LocalLlmSession` → `oneshim-network/`: Both use HTTP clients (reqwest).
  `oneshim-network` already owns HTTP communication (`http_client.rs`, `ai_llm_client/`).
  This follows the existing pattern where HTTP-based adapters live in `oneshim-network`.

---

## 12. Migration from Existing Code

### 12.1 Preserved (No Changes)

- `SubprocessLlmProvider` / `SubprocessOcrProvider` — one-shot invocation (add optimized flags only)
- `provider-surface-catalog.json` — additive schema extension only
- `oneshim-web` REST API — no changes, reused as Pull target
- `AuditLogger` in `oneshim-automation` — reused for batch sink

### 12.2 Extended

- `oneshim-core/Cargo.toml` — add `futures-core = "0.3"` dependency (for `Stream` trait in `ResponseStream`)
- `AppConfig` (`oneshim-core/src/config/mod.rs`) — add `AiSessionConfig` field + re-export
- `oneshim-core/src/config/sections/mod.rs` — add `pub mod ai_session;` entry
- `AuditLogPort` (`oneshim-core/src/ports/audit_log.rs`) — add `record_session_event` method
- `SubprocessTransportSpec` (`oneshim-api-contracts/src/provider_specs/models.rs`) — add `oneshot_flags: Vec<String>` and `session_flags: Vec<String>` fields
- `oneshim-storage/migration.rs` — add V20 for `session_audit_log` table
- `src-tauri/subprocess_provider/` — add `SubprocessSession` alongside existing providers
- `src-tauri/main.rs` — wire SessionManager in DI
- `src-tauri/src/lifecycle.rs` — add session shutdown integration

### 12.3 New

- Port traits in `oneshim-core/ports/` (`ConversationSession`, `SessionManager`)
- JSONL model types in `oneshim-core/models/` (session messages, attachments, outbound types)
- `SessionContextAssembler` in `src-tauri/`
- Session adapters (Subprocess in `src-tauri/`, HttpApi and LocalLlm in `oneshim-network/`)
- `AuditingSession` decorator in `src-tauri/`
- `SessionManagerImpl` in `src-tauri/`

---

## 13. Testing Strategy

| Layer | What | How |
|-------|------|-----|
| **Unit** | JSONL parsing/serialization | `#[cfg(test)]` in model modules |
| **Unit** | SessionContextAssembler output | Mock storage/config, verify prompt structure |
| **Unit** | Adapter normalization | Feed native CLI output, assert unified JSONL |
| **Unit** | AuditingSession decorator | Mock inner session + mock sink, verify audit entries |
| **Unit** | Error recovery logic | Simulate crash scenarios, verify state transitions |
| **Integration** | SubprocessSession with real CLI | Spawn actual CLI, exchange messages, verify JSONL |
| **Integration** | SessionManager lifecycle | Create/list/kill sessions, verify cleanup |
| **Integration** | Idle reaper | Create session, wait for timeout, verify termination |
| **E2E** | Full conversation flow | Tauri command → SessionManager → CLI → response → audit |

---

## 14. Blocking Prerequisites

These must be resolved **before implementation begins**. Each item determines whether
the `SubprocessSession` adapter can use persistent mode or must fall back to serial
one-shot calls with history re-injection.

| # | Question | Impact if Unsupported | Fallback |
|---|----------|----------------------|----------|
| 1 | **Claude CLI `stream-json` in interactive mode** — Does `--output-format stream-json` work with interactive (non-`-p`) mode for long-lived sessions? | Cannot parse streaming response in persistent subprocess | Serial `-p` calls with `--continue` / `--session-id` |
| 2 | **Codex CLI interactive mode** — Codex's `exec` is one-shot; is there a persistent mode or session resume? | No persistent subprocess possible for Codex | Serial `exec` calls with conversation history in prompt |
| 3 | **Gemini CLI interactive mode** — Does Gemini CLI support stdin-based multi-turn interaction? | No persistent subprocess for Gemini | Serial `-p` calls with history in prompt |
| 4 | **CLI flag availability** — Do `--no-mcp`, `--no-hooks`, `--max-turns` exist in current Claude CLI versions? | One-shot optimization flags unavailable | Use only known flags, skip unavailable ones |
| 5 | **Pull API authentication** — `localhost:10090` has no auth; should CLI tool calls use a session token? | Unauthorized local access risk (low — localhost only) | Add optional bearer token to oneshim-web |

**Resolution plan**: Verify items 1-4 by testing each CLI tool locally before writing adapter code.
Item 5 is deferred — localhost-only access is acceptable for initial implementation.
