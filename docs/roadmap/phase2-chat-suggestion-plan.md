# Phase 2: Chat + Suggestion Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bridge the chat and suggestion systems — generate suggestions from chat, explain suggestions in chat, auto-extract suggestions from AI responses.

**Architecture:** Add 2 new IPC commands (`request_chat_suggestions`, `explain_suggestion_in_chat`), a shared `try_extract_suggestions` parser, and cross-window navigation via Tauri events. No new crates.

**Tech Stack:** Rust (tokio, serde_json, futures), TypeScript/React (Tauri IPC/events)

**Spec:** `docs/roadmap/phase2-chat-suggestion-spec.md`

---

## File Structure

### New Files
| File | Responsibility |
|------|---------------|
| `src-tauri/src/commands/suggestion_parser.rs` | `try_extract_suggestions` — shared JSON parser for suggestion extraction |

### Modified Files
| File | Changes |
|------|---------|
| `src-tauri/src/commands/suggestions.rs` | Add `request_chat_suggestions`, `explain_suggestion_in_chat` commands; add `reasoning` to DTOs |
| `src-tauri/src/commands/ai_session.rs` | Pass `SuggestionManager` to streaming task for auto-extraction |
| `src-tauri/src/commands/mod.rs` | Export `suggestion_parser` module |
| `src-tauri/src/main.rs` | Register 2 new IPC commands |
| `crates/oneshim-web/frontend/src/pages/chat/ChatInput.tsx` | "Get Suggestions" button |
| `crates/oneshim-web/frontend/src/pages/chat/index.tsx` | Wire button handler, listen for extraction events |
| `crates/oneshim-web/frontend/src/pages/chat/hooks/useMessageStream.ts` | Listen for `chat:suggestions-extracted` |
| `crates/oneshim-web/frontend/src/overlay/components/SuggestionItem.tsx` | "Explain" button |
| `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx` | Handle explain action |

---

## Task 1: Shared Suggestion Parser (`try_extract_suggestions`)

**Files:**
- Create: `src-tauri/src/commands/suggestion_parser.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write failing tests for the parser**

Create `src-tauri/src/commands/suggestion_parser.rs`:

```rust
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
        let text =
            r#"{"suggestions": [{"type": "unknown_type", "content": "Test", "priority": "medium"}]}"#;
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
```

- [ ] **Step 2: Add module export**

In `src-tauri/src/commands/mod.rs`, add:
```rust
pub mod suggestion_parser;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p oneshim-app suggestion_parser -- --nocapture`
Expected: 7 tests PASS

Note: `uuid` crate is needed. Check if it's already a dependency of `src-tauri`. If not, add `uuid = { version = "1", features = ["v4"] }` to `src-tauri/Cargo.toml`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/suggestion_parser.rs src-tauri/src/commands/mod.rs
git commit -m "feat(suggestion): add try_extract_suggestions parser for chat integration"
```

---

## Task 2: `request_chat_suggestions` IPC Command

**Files:**
- Modify: `src-tauri/src/commands/suggestions.rs`
- Modify: `src-tauri/src/main.rs` (command registration)

- [ ] **Step 1: Add the command**

In `src-tauri/src/commands/suggestions.rs`, add imports:
```rust
use crate::commands::suggestion_parser::try_extract_suggestions;
use crate::runtime_state::AiSessionRuntimeState;
use futures::StreamExt;
use oneshim_core::models::ai_session::{MessageRole, OutboundMessage, SessionMessage};
use oneshim_core::ports::conversation_session::SessionManager;
```

Add the command:

```rust
const SUGGESTION_PROMPT: &str = r#"Based on our conversation context, generate 1-3 actionable suggestions for the user.
Each suggestion should be specific, practical, and relevant to the current discussion.

Respond ONLY with a JSON object matching this schema:
{"suggestions": [{"type": "<type>", "content": "<text>", "priority": "<priority>", "reasoning": "<why>"}]}

Valid types: work_guidance, email_draft, productivity_tip, workflow_optimization, context_based
Valid priorities: low, medium, high, critical"#;

#[command]
pub async fn request_chat_suggestions(
    ai_state: tauri::State<'_, AiSessionRuntimeState>,
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    session_id: String,
) -> Result<u32, String> {
    let mgr = ai_state
        .manager_impl()
        .ok_or_else(|| "AI sessions not available".to_string())?;

    let suggestion_mgr = suggestion_state
        .manager()
        .ok_or_else(|| "suggestions not available".to_string())?;

    // Get session and send structured request
    let session = mgr
        .get_session(&session_id)
        .await
        .map_err(|e| format!("session not found: {e}"))?;

    let msg = SessionMessage {
        role: MessageRole::User,
        content: SUGGESTION_PROMPT.to_string(),
        attachments: Vec::new(),
        tools: None,
        context: None,
        response_format: None,
    };

    let mut stream = session
        .send_message(&msg)
        .await
        .map_err(|e| format!("failed to send message: {e}"))?;

    // Drain stream and collect response text
    // Note: ResponseStream yields Result<OutboundMessage, CoreError>, not bare OutboundMessage
    let mut response_text = String::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(OutboundMessage::Text { content, .. }) => response_text.push_str(&content),
            Ok(OutboundMessage::Result { content, .. }) => {
                if !content.is_empty() && response_text.is_empty() {
                    response_text = content;
                }
            }
            Ok(OutboundMessage::Error { message, .. }) => {
                return Err(format!("AI error: {message}"));
            }
            Err(e) => {
                return Err(format!("Stream error: {e}"));
            }
            _ => {}
        }
    }

    // Parse suggestions
    let suggestions = try_extract_suggestions(&response_text);
    let count = suggestions.len() as u32;

    // Push to queue
    if !suggestions.is_empty() {
        let mut queue = suggestion_mgr.queue().lock().await;
        for suggestion in suggestions {
            queue.push(suggestion);
        }
        let queue_count = queue.len();
        drop(queue);

        if let Some(overlay) = suggestion_state.overlay() {
            overlay.emit_suggestions_changed(queue_count);
        }
    }

    Ok(count)
}
```

- [ ] **Step 2: Register in main.rs**

Find the `generate_handler!` macro invocation in `src-tauri/src/main.rs` and add `request_chat_suggestions` to the list.

- [ ] **Step 3: Build check**

Run: `cargo check -p oneshim-app`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/suggestions.rs src-tauri/src/main.rs
git commit -m "feat(suggestion): add request_chat_suggestions IPC command"
```

---

## Task 3: `explain_suggestion_in_chat` IPC Command

**Files:**
- Modify: `src-tauri/src/commands/suggestions.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add the command**

Add imports (in addition to Task 2's imports):
```rust
use tauri::{AppHandle, Emitter};
```

```rust
#[command]
pub async fn explain_suggestion_in_chat(
    app: AppHandle,
    ai_state: tauri::State<'_, AiSessionRuntimeState>,
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    suggestion_id: String,
    session_id: Option<String>,
) -> Result<String, String> {
    let suggestion_mgr = suggestion_state
        .manager()
        .ok_or_else(|| "suggestions not available".to_string())?;

    let ai_mgr = ai_state
        .manager_impl()
        .ok_or_else(|| "AI sessions not available".to_string())?;

    // Find suggestion from queue or history
    let (content, reasoning) = {
        let queue = suggestion_mgr.queue().lock().await;
        if let Some(s) = queue.iter().find(|s| s.suggestion_id == suggestion_id) {
            (s.content.clone(), s.reasoning.clone())
        } else {
            drop(queue);
            let history = suggestion_mgr.history().lock().await;
            let entry = history
                .recent(100)
                .into_iter()
                .find(|e| e.suggestion.suggestion_id == suggestion_id);
            match entry {
                Some(e) => (e.suggestion.content.clone(), e.suggestion.reasoning.clone()),
                None => return Err(format!("Suggestion {suggestion_id} not found")),
            }
        }
    };

    // Find or validate session
    let sid = match session_id {
        Some(id) => id,
        None => {
            // Find most recent active/idle session
            let sessions = ai_mgr.list_sessions().await;
            sessions
                .into_iter()
                .filter(|s| {
                    s.state == oneshim_core::models::ai_session::SessionState::Active
                        || s.state == oneshim_core::models::ai_session::SessionState::Idle
                })
                .max_by_key(|s| s.last_active)
                .map(|s| s.session_id)
                .ok_or_else(|| "No active chat session — open a chat first".to_string())?
        }
    };

    // Compose explain message
    let mut prompt = format!(
        "Explain this suggestion in detail and help me understand how to act on it:\n\n{}",
        content
    );
    if let Some(reasoning) = reasoning {
        prompt.push_str(&format!("\n\nReasoning provided: {reasoning}"));
    }

    // Cannot call #[command] send_session_message directly (tauri::State not Clone).
    // Instead, call session.send_message() directly and spawn a streaming task
    // that emits OutboundMessage events — replicating the pattern from ai_session.rs.
    let session = ai_mgr
        .get_session(&sid)
        .await
        .map_err(|e| format!("session error: {e}"))?;

    let msg = SessionMessage {
        role: MessageRole::User,
        content: prompt,
        attachments: Vec::new(),
        tools: None,
        context: None,
        response_format: None,
    };

    let stream = session
        .send_message(&msg)
        .await
        .map_err(|e| format!("failed to send: {e}"))?;

    // Spawn streaming task to emit events (same pattern as send_session_message)
    let event_name = format!("ai-session:{sid}");
    let app_clone = app.clone();
    tokio::spawn(async move {
        use futures::StreamExt;
        tokio::pin!(stream);
        while let Some(item) = stream.next().await {
            match item {
                Ok(outbound) => {
                    let _ = app_clone.emit(&event_name, &outbound);
                }
                Err(e) => {
                    let err_msg = OutboundMessage::Error {
                        code: "stream_error".to_string(),
                        message: e.to_string(),
                        retryable: false,
                    };
                    let _ = app_clone.emit(&event_name, &err_msg);
                    break;
                }
            }
        }
    });

    // Emit navigation event for overlay → chat
    let _ = app.emit("navigate:chat", serde_json::json!({ "sessionId": sid }));

    Ok(sid)
}
```

- [ ] **Step 2: Add `reasoning` to SuggestionViewDto**

In `src-tauri/src/commands/suggestions.rs`, add to `SuggestionViewDto`:
```rust
pub reasoning: Option<String>,
```

Update `get_pending_suggestions` mapping to include:
```rust
reasoning: s.reasoning.clone(),
```

Update `get_suggestion_history` mapping similarly.

- [ ] **Step 3: Register in main.rs**

Add `explain_suggestion_in_chat` to `generate_handler!`.

- [ ] **Step 4: Build check**

Run: `cargo check -p oneshim-app`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/suggestions.rs src-tauri/src/main.rs
git commit -m "feat(suggestion): add explain_suggestion_in_chat + expose reasoning field"
```

---

## Task 4: Auto-Extract Suggestions from Chat Responses

**Files:**
- Modify: `src-tauri/src/commands/ai_session.rs`

- [ ] **Step 1: Add SuggestionRuntimeState to send_session_message**

Add import:
```rust
use crate::runtime_state::SuggestionRuntimeState;
```

Add state parameter to `send_session_message`:
```rust
#[command]
pub async fn send_session_message(
    app: AppHandle,
    state: tauri::State<'_, AiSessionRuntimeState>,
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    request: SendSessionMessageRequest,
) -> Result<(), String> {
```

Clone the suggestion manager for the spawned task:
```rust
let suggestion_mgr = suggestion_state.manager();
```

Move it into the spawned task and add extraction after the stream loop:
```rust
// After the while let Some(item) = stream.next().await loop ends:

// Auto-extract suggestions from AI response
if let Some(ref sgn_mgr) = suggestion_mgr {
    let extracted = crate::commands::suggestion_parser::try_extract_suggestions(&assistant_content);
    if !extracted.is_empty() {
        let count = extracted.len();
        let mut queue = sgn_mgr.queue().lock().await;
        for suggestion in extracted {
            queue.push(suggestion);
        }
        let queue_count = queue.len();
        drop(queue);

        let _ = app_clone.emit(
            "chat:suggestions-extracted",
            serde_json::json!({ "count": count, "sessionId": session_id }),
        );

        // Also notify overlay
        let _ = app_clone.emit(
            "overlay:suggestions-changed",
            serde_json::json!({ "count": queue_count }),
        );

        debug!(count, session_id = %session_id, "auto-extracted suggestions from chat response");
    }
}
```

- [ ] **Step 2: Build check**

Run: `cargo check -p oneshim-app`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/ai_session.rs
git commit -m "feat(chat): auto-extract suggestions from AI responses into queue"
```

---

## Task 5: Frontend — "Get Suggestions" Button in Chat

**Files:**
- Modify: `crates/oneshim-web/frontend/src/pages/chat/ChatInput.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/chat/index.tsx`

- [ ] **Step 1: Add button to ChatInput**

In `ChatInput.tsx`, add a new prop:
```typescript
  onRequestSuggestions?: () => void
  requestingSuggestions?: boolean
```

Add a button before the submit button (around line 195):
```tsx
{onRequestSuggestions && (
  <Button
    variant="ghost"
    size="sm"
    type="button"
    disabled={sendDisabled || requestingSuggestions}
    onClick={onRequestSuggestions}
    title="Get AI suggestions"
    className="shrink-0"
  >
    {requestingSuggestions ? (
      <Loader2 className={cn(iconSize.sm, 'animate-spin')} />
    ) : (
      <Lightbulb className={iconSize.sm} />
    )}
  </Button>
)}
```

Add `Lightbulb` to the lucide-react imports at the top.

- [ ] **Step 2: Wire handler in index.tsx**

Add state:
```typescript
const [requestingSuggestions, setRequestingSuggestions] = useState(false)
```

Add handler:
```typescript
const handleRequestSuggestions = useCallback(async () => {
  if (!activeId) return
  setRequestingSuggestions(true)
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const count = await invoke<number>('request_chat_suggestions', { sessionId: activeId })
    addToast({ type: 'success', message: `${count} suggestion${count !== 1 ? 's' : ''} generated` })
  } catch (e) {
    addToast({ type: 'error', message: `Failed to get suggestions: ${e}` })
  } finally {
    setRequestingSuggestions(false)
  }
}, [activeId])
```

Pass to ChatInput:
```tsx
<ChatInput
  {...existingProps}
  onRequestSuggestions={activeId ? handleRequestSuggestions : undefined}
  requestingSuggestions={requestingSuggestions}
/>
```

- [ ] **Step 3: Build check**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-web/frontend/src/pages/chat/ChatInput.tsx crates/oneshim-web/frontend/src/pages/chat/index.tsx
git commit -m "feat(chat): add Get Suggestions button to chat input"
```

---

## Task 6: Frontend — "Explain" Button on Suggestions

**Files:**
- Modify: `crates/oneshim-web/frontend/src/overlay/components/SuggestionItem.tsx`
- Modify: `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx`
- Modify: `crates/oneshim-web/frontend/src/overlay/types.ts`

- [ ] **Step 1: Add reasoning to SuggestionViewDto type**

In `types.ts`, add to `SuggestionViewDto`:
```typescript
  reasoning: string | null
```

- [ ] **Step 2: Add "Explain" button to SuggestionItem**

Update `onAction` prop type:
```typescript
  onAction: (id: string, action: 'accept' | 'reject' | 'defer' | 'explain', snoozeMinutes?: number) => void
```

Add an "Explain" button next to the existing action buttons:
```tsx
<button
  type="button"
  className="px-2 py-1 rounded text-xs bg-brand/10 text-brand hover:bg-brand/20 transition-colors"
  onClick={() => onAction(item.id, 'explain')}
>
  Explain
</button>
```

- [ ] **Step 3: Handle explain action in SuggestionsPanel**

Update `handleAction` to handle 'explain':
```typescript
if (action === 'explain') {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('explain_suggestion_in_chat', { suggestionId: id })
    showToast('Opening in chat...', 'info')
  } catch (e) {
    showToast(`${e}`, 'error')
  }
  return
}
```

- [ ] **Step 4: Build check**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-web/frontend/src/overlay/components/SuggestionItem.tsx crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx crates/oneshim-web/frontend/src/overlay/types.ts
git commit -m "feat(overlay): add Explain button on suggestions for chat integration"
```

---

## Task 7: Frontend — Chat Extraction Notification

**Files:**
- Modify: `crates/oneshim-web/frontend/src/pages/chat/index.tsx`

- [ ] **Step 1: Add listener for `chat:suggestions-extracted`**

In `index.tsx`, add a `useEffect` after the existing hook calls:

```typescript
useEffect(() => {
  if (!activeId) return
  let unlisten: (() => void) | null = null
  ;(async () => {
    const { listen } = await import('@tauri-apps/api/event')
    unlisten = await listen<{ count: number; sessionId: string }>(
      'chat:suggestions-extracted',
      ({ payload }) => {
        if (payload.sessionId === activeId) {
          addToast({
            type: 'info',
            message: `${payload.count} suggestion${payload.count !== 1 ? 's' : ''} added from this conversation`,
          })
        }
      },
    )
  })()
  return () => { unlisten?.() }
}, [activeId])
```

- [ ] **Step 2: Build check**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/pages/chat/index.tsx
git commit -m "feat(chat): show toast when suggestions auto-extracted from AI response"
```

---

## Task 8: Full Build Verification

- [ ] **Step 1: cargo check --workspace**
Expected: PASS

- [ ] **Step 2: cargo test --workspace**
Expected: ALL tests PASS (including ~7 new parser tests)

- [ ] **Step 3: cargo clippy --workspace**
Expected: 0 warnings

- [ ] **Step 4: cargo fmt --check**
Expected: PASS

- [ ] **Step 5: pnpm build (frontend)**
Expected: PASS

- [ ] **Step 6: Commit any fixes**
