# AI Session Manager Phase 2a — Claude Subprocess + One-shot Wiring

> **Version**: 1.1
> **Date**: 2026-03-25
> **Scope**: Claude SubprocessSession adapter, one-shot flag wiring, context assembler real data, DI wiring
> **Depends on**: AI-SESSION-MANAGER-SPEC.md v1.3 (Phase 1, PR #162)
> **Deferred to Phase 2b**: HttpApiSession, LocalLlmSession, oneshim-web REST endpoints, Codex resume adapter

---

## 1. Blocking Prerequisites — Resolution

Phase 1 spec Section 14 identified 5 blocking prerequisites. Results:

| # | Question | Result | Impact |
|---|----------|--------|--------|
| 1 | Claude CLI `stream-json` interactive mode | **Supported** — `--print --input-format stream-json --output-format stream-json` with `--continue`/`--session-id` for multi-turn. | Serial `-p` calls with CLI-managed session resume |
| 2 | Codex CLI persistent mode | **Partial** — `resume`/`fork` exist but `--last` targets most recent session only (no session-id targeting). | **Defer to Phase 2b** — needs further verification |
| 3 | Gemini CLI | **Not installed** | Defer to Phase 3 |
| 4 | CLI flag availability | **Verified** — `--bare` (skips hooks/LSP/MCP/memory/CLAUDE.md), `--system-prompt`, `--tools`, `--allowed-tools` | `--bare` for one-shot; selective for sessions |
| 5 | Pull API auth | **Deferred** — localhost-only acceptable | No auth needed |

---

## 2. Goals (Phase 2a)

1. **SubprocessSession adapter (Claude only)** — Serial `-p` calls with `--session-id` + `--continue` for multi-turn
2. **Wire `append_oneshot_flags`** — Replace hardcoded CLI flags with catalog-driven `oneshot_flags`
3. **Update catalog `oneshot_flags`** — Add `--bare` for maximum startup optimization
4. **SessionContextAssembler real data** — Query SQLite for actual activity/suggestion history
5. **SessionManager DI wiring** — Connect to Tauri AppState + IPC commands
6. **Tauri event streaming** — Use Tauri events to push response chunks to frontend

---

## 3. Non-Goals (Phase 2a)

- Codex resume adapter (deferred to Phase 2b — `--last` lacks session-id targeting)
- Gemini CLI adapter (deferred to Phase 3)
- HttpApiSession / LocalLlmSession (deferred to Phase 2b)
- oneshim-web REST endpoints (deferred to Phase 2b)
- Frontend UI for session management

---

## 4. Architecture

### 4.1 Claude Subprocess Session Strategy

**Serial `-p` calls with CLI-managed session resume**, not a long-lived process:

```
Turn 1: claude -p --output-format stream-json --session-id <uuid> \
          --system-prompt "context..." --bare "user message"
         → stdout: stream-json events → process exits, session saved to disk

Turn 2: claude -p --output-format stream-json --continue --session-id <uuid> \
          --bare "follow-up message"
         → stdout: stream-json events → process exits, continues session

Turn N: ... (same pattern with --continue)
```

**Advantages:**
- No crash recovery needed (process exits after each turn)
- CLI manages conversation history (no ONESHIM-side duplication)
- `--bare` minimizes startup overhead (skips hooks/LSP/MCP/memory/CLAUDE.md)
- No memory leak risk (process memory freed on exit)
- Session persistence controlled by CLI (survives app restart)

**Disadvantage:**
- Process spawn per turn (~1-2s overhead, mitigated by `--bare`)

### 4.2 SubprocessSession Implementation

```rust
// src-tauri/src/session_adapters/claude_session.rs
pub struct ClaudeSubprocessSession {
    session_id: String,
    cli_session_id: String,      // UUID for --session-id
    surface: DetectedSubprocessCli,
    model: String,
    system_prompt: Option<String>,
    turn_count: AtomicU32,
    last_active: Mutex<Instant>,
    config: Arc<AiSessionConfig>,
}
```

The `ClaudeSubprocessSession` implements `ConversationSession`:
- `send_message()` spawns a claude process with stream-json output, parses stdout line-by-line into `OutboundMessage` events, and returns them as a `ResponseStream`.
- First turn uses `--session-id <uuid> --system-prompt "..."`.
- Subsequent turns add `--continue`.
- `info()` returns session metadata from local fields (no CLI call).

### 4.3 Claude stream-json Normalization

CLI output format (verified from `--output-format stream-json`):

```jsonl
{"type":"assistant","subtype":"text","cost_usd":0.001,"duration_ms":100,"session_id":"uuid","text":"chunk"}
{"type":"result","subtype":"success","cost_usd":0.005,"duration_ms":500,"session_id":"uuid","result":"full response"}
```

Normalization to Phase 1's `OutboundMessage`:

```rust
fn normalize_claude_stream_event(line: &str) -> Option<OutboundMessage> {
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
                content: event.get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                done: true,
                usage,
            })
        }
        _ => None, // Skip unknown event types
    }
}
```

**Note**: The exact field names in stream-json output must be verified against real
CLI output. The `usage` field may be nested or absent. The normalization handles
both cases gracefully with `Option` chaining.

### 4.4 Streaming to Frontend via Tauri Events

`ConversationSession::send_message()` returns `ResponseStream` (async stream).
Tauri IPC commands cannot return streams. Solution: **Tauri event emission**.

```rust
#[tauri::command]
pub async fn send_session_message(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    message: SessionMessage,
) -> Result<(), String> {
    let session = state.session_manager.get_session(&session_id)
        .map_err(|e| e.to_string())?;
    let mut stream = session.send_message(&message).await
        .map_err(|e| e.to_string())?;

    // Spawn background task to emit events as chunks arrive
    tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => {
                    let _ = app.emit(&format!("ai-session:{session_id}"), &msg);
                }
                Err(err) => {
                    let _ = app.emit(&format!("ai-session:{session_id}"),
                        &OutboundMessage::Error {
                            code: "stream_error".to_string(),
                            message: err.to_string(),
                            retryable: false,
                        });
                    break;
                }
            }
        }
    });

    Ok(())
}
```

Frontend listens: `listen("ai-session:<id>", callback)`.

---

## 5. One-shot Flag Wiring

### 5.1 Replace Hardcoded Flags with Catalog

In `run_claude()` and `run_claude_ocr()`, replace hardcoded flags with
`append_oneshot_flags()`:

```rust
// Before (llm_provider.rs run_claude)
command
    .arg("-p")
    .arg("--permission-mode").arg("dontAsk")
    .arg("--tools").arg("")
    .arg("--no-session-persistence")
    .arg("--output-format").arg("text")
    .arg("--json-schema").arg(ACTION_SCHEMA_JSON)
    .arg(prompt);

// After
command.arg("-p");
append_oneshot_flags(&mut command, &self.surface.surface_id);
command
    .arg("--json-schema").arg(ACTION_SCHEMA_JSON)
    .arg(prompt);
```

### 5.2 Update Catalog `oneshot_flags` with `--bare`

`--bare` replaces the need for individual `--no-mcp`, `--no-hooks` flags
(it skips hooks, LSP, plugin sync, MCP auto-discovery, memory, CLAUDE.md all at once):

```json
"oneshot_flags": [
  "--bare",
  "--no-session-persistence",
  "--permission-mode", "dontAsk",
  "--tools="
]
```

**Note**: `--bare` is used for **one-shot** calls only. Session calls use selective
flags (no `--bare`) so that `--system-prompt` and tools are available.

---

## 6. SessionContextAssembler Real Data

Phase 1's sync `build_system_prompt()` changes to **async** to support SQLite queries:

```rust
impl SessionContextAssembler {
    pub async fn build_system_prompt(&self) -> SystemPromptContext {
        SystemPromptContext {
            user_profile: self.build_user_profile(),
            current_regime: self.current_regime(),
            recent_activity: self.query_recent_activity().await,
            suggestion_history: self.query_suggestion_history().await,
            available_skills: vec![],
            system_info: self.build_system_info(),
        }
    }

    async fn query_recent_activity(&self) -> ActivitySummary {
        // Query activity_segments for last 30 minutes
        // Uses tokio::task::block_in_place for sync SQLite calls
        let storage = self.storage.clone();
        tokio::task::spawn_blocking(move || {
            // storage.query_recent_segments(Duration::from_secs(1800))
            ActivitySummary::default() // Fallback on error
        }).await.unwrap_or_default()
    }

    async fn query_suggestion_history(&self) -> SuggestionPatterns {
        // Query local_suggestions table
        let storage = self.storage.clone();
        tokio::task::spawn_blocking(move || {
            // storage.count_suggestions_by_status()
            SuggestionPatterns::default()
        }).await.unwrap_or_default()
    }
}
```

**Breaking change from Phase 1**: `build_system_prompt` was sync, now async.
Phase 1 callers: none (the method was never wired). Safe to change.

---

## 7. SessionManager DI Wiring

### 7.1 AppState Integration

Add to `src-tauri/src/runtime_state.rs`:

```rust
pub struct AppState {
    // ... existing fields ...
    pub session_manager: Arc<SessionManagerImpl>,
}
```

### 7.2 SessionManagerImpl Modifications

Add `get_session()` method (needed by Tauri commands):

```rust
impl SessionManagerImpl {
    pub async fn get_session(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn ConversationSession>, CoreError> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id)
            .map(|m| m.session.clone())
            .ok_or_else(|| CoreError::Internal(format!("session not found: {session_id}")))
    }
}
```

Wire `create_session` to create `ClaudeSubprocessSession`:

```rust
async fn create_session(&self, config: SessionConfig) -> Result<Arc<dyn ConversationSession>, CoreError> {
    // ... cap check ...
    match config.transport {
        SessionTransport::Subprocess => {
            let surface = self.detect_claude_surface()?;
            let inner: Arc<dyn ConversationSession> = Arc::new(
                ClaudeSubprocessSession::new(surface, &config, self.config.clone())
            );
            let audited = Arc::new(AuditingSession::new(inner.clone(), self.audit.clone()));
            // Store in sessions map
            // Return audited session
        }
        _ => Err(CoreError::Internal("adapter not yet implemented".into())),
    }
}
```

### 7.3 `shutdown_all` is on `SessionManagerImpl` directly

`shutdown_all()` is already defined on `SessionManagerImpl` (Phase 1), not on
the `SessionManager` port trait. The Tauri `RunEvent::Exit` handler uses the
concrete type via `AppState.session_manager` — no trait method needed.

### 7.4 Tauri IPC Commands

New file `src-tauri/src/commands/ai_session.rs`:

```rust
#[tauri::command]
pub async fn create_ai_session(
    state: tauri::State<'_, AppState>,
    config: SessionConfig,
) -> Result<ConversationSessionInfo, String> { ... }

#[tauri::command]
pub async fn send_session_message(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    message: SessionMessage,
) -> Result<(), String> { ... }  // streams via Tauri events

#[tauri::command]
pub async fn kill_ai_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> { ... }

#[tauri::command]
pub async fn list_ai_sessions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ConversationSessionInfo>, String> { ... }
```

---

## 8. Crate Placement

| Component | Crate | Reason |
|-----------|-------|--------|
| `ClaudeSubprocessSession` | `src-tauri/src/session_adapters/claude_session.rs` | Process management in binary crate |
| `normalize_claude_stream_event` | `src-tauri/src/session_adapters/claude_normalizer.rs` | Claude-specific parsing |
| AI session Tauri commands | `src-tauri/src/commands/ai_session.rs` | IPC commands (directory module pattern) |
| `SessionManagerImpl` modifications | `src-tauri/src/session_manager.rs` | Wire adapter creation |
| `SessionContextAssembler` modifications | `src-tauri/src/session_context.rs` | Add async queries |

**Note**: `session_adapters/` is a new directory module, separate from
`subprocess_provider/` (which handles one-shot LLM/OCR calls). This maintains
single-responsibility: `subprocess_provider` = one-shot, `session_adapters` = sessions.

---

## 9. Testing Strategy

| Layer | What | How |
|-------|------|-----|
| **Unit** | Claude stream-json normalization | Feed sample JSONL events, assert `OutboundMessage` variants |
| **Unit** | `ClaudeSubprocessSession::info()` | Verify metadata fields |
| **Unit** | `SessionContextAssembler` async queries | Mock storage, verify data |
| **Unit** | Tauri IPC command routing | Mock `SessionManager`, verify command behavior |
| **Unit** | One-shot flag application | Verify `append_oneshot_flags` produces correct args |
| **Integration** | Full session lifecycle | Create → send → list → kill → verify audit entries |

---

## 10. Phase 2b Scope (Deferred)

| Task | Rationale for Deferral |
|------|----------------------|
| `HttpApiSession` | Needs streaming HTTP (SSE) design decision |
| `LocalLlmSession` | Depends on `HttpApiSession` pattern |
| Codex `resume` adapter | `--last` lacks session-id targeting, needs verification |
| oneshim-web REST endpoints | Needs both AppState types wired (Tauri + web) |
| `ChatMessage` type | Only needed for HTTP/Ollama adapters (self-managed history) |
