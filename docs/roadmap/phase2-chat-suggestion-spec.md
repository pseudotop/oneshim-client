# Phase 2 Spec: Chat + Suggestion Integration

**Date:** 2026-04-04
**Baseline:** v0.4.18 + Phase 1 commit `875ca36d`
**Branch:** `feat/analysis-wiring-v2`
**Scope:** 3 tasks — chat-initiated suggestions, suggestion context in chat, chat response to suggestion conversion

---

## 1. Current State Analysis

### Two Independent Systems

```
Chat System (AI Sessions)              Suggestion System
─────────────────────────               ─────────────────
SessionManagerImpl                      SuggestionReceiver (SSE)
  ├─ create/send/kill sessions          ├─ SuggestionQueue (BTreeSet, max 50)
  ├─ OutboundMessage streaming          ├─ FeedbackScorer
  ├─ Token budget tracking              ├─ DeferredManager (snooze)
  └─ SQLite message persistence         └─ FeedbackRetryQueue

IPC: 8 commands (ai_session.rs)         IPC: 3 commands (suggestions.rs)
Events: ai-session:{session_id}         Events: overlay:suggestions-changed
Frontend: /pages/chat/                  Frontend: overlay SuggestionsPanel

NO CROSS-REFERENCES between the two systems.
```

### Key Constraints

1. **Chat lives in `/pages/chat/`** — a full page, not the overlay. Suggestions live in the overlay.
2. **Chat uses its own streaming protocol** — `OutboundMessage` via `ai-session:{id}` events.
3. **Suggestions arrive via SSE** — server pushes, client queues. Not request-response.
4. **No server API exists for "request suggestions from chat"** — the server generates suggestions based on activity monitoring, not on-demand.
5. **The client has a local LLM pipeline** (`oneshim-analysis`) — can generate suggestions locally without server.

### What Already Exists

| Component | Available | Notes |
|-----------|-----------|-------|
| `MessageContext` | Yes | Sent with each chat message: `{ regime, active_app }` |
| `SendSessionMessageRequest.tools` | Yes | Function calling / tool definitions |
| `SendSessionMessageRequest.response_format` | Yes | JSON schema for structured output |
| `SuggestionViewDto` | Yes | Serialized suggestion for frontend |
| `SuggestionQueue::push()` | Yes | Can programmatically add suggestions |
| `oneshim-analysis::ContextAnalyzer` | Yes | Local LLM summarizer + regime classifier |
| `emit_suggestions_changed()` | Yes | Notify overlay of queue changes |
| `Suggestion.reasoning` field | Yes | Exists in model but NOT exposed to frontend |

---

## 2. Design Decisions

### D1: Where does "suggest from chat" logic live?

**Decision:** Client-side, using the AI session's tool-use capability.

**Rationale:** There is no server API for on-demand suggestion generation. The server generates suggestions from activity monitoring. Adding a new server endpoint is out of scope for v0.4. Instead, we use the chat AI's existing tool-calling mechanism:

1. Define a `generate_suggestions` tool in the chat session
2. The AI model decides when suggestions are relevant based on conversation context
3. Tool results are parsed into `Suggestion` structs and pushed to the queue

This is a client-side integration — no server changes needed.

### D2: How do suggestions become visible from chat?

**Decision:** The overlay `SuggestionsPanel` is the single display surface. Chat can trigger it to open with pre-selected context.

**Rationale:** Building a second suggestion display surface inside the chat page would duplicate logic. Instead:
- Chat generates suggestions → pushes to SuggestionQueue → overlay badge updates
- User can click "View Suggestions" in chat to open the overlay panel
- The overlay panel already handles all CRUD (accept/reject/defer)

### D3: How does "explain this suggestion" work?

**Decision:** A new IPC command `explain_suggestion` sends the suggestion content to the active chat session as a prefilled prompt.

**Rationale:** The chat system already handles message sending. We just need to:
1. Get the suggestion content by ID
2. Compose a prompt: "Explain this suggestion in detail: {content}"
3. Send it to the active chat session via `send_session_message`
4. Navigate the UI to the chat page

### D4: How are chat responses converted to suggestions?

**Decision:** Use `response_format` with a JSON schema to request structured suggestion output when the user asks for suggestions.

When the AI response includes a structured suggestion block (detected by a specific JSON shape), the client parses it and pushes to the `SuggestionQueue`. This happens automatically in the message stream handler.

---

## 3. Task Specifications

### 3.1 Chat-Initiated Suggestions (Task 2.1)

**Goal:** Users can request suggestions from the chat AI. The AI generates context-aware suggestions that appear in the suggestion queue.

#### Design

**New IPC command:** `request_chat_suggestions`

```rust
#[command]
pub async fn request_chat_suggestions(
    ai_state: tauri::State<'_, AiSessionRuntimeState>,
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    session_id: String,
) -> Result<u32, String>  // Returns number of suggestions generated
```

**Key architectural detail:** This command does NOT use the `send_session_message` IPC (which is fire-and-forget — spawns a background streaming task). Instead, it calls `ConversationSession::send_message()` directly on the session port and drains the `ResponseStream` inline to collect the full response text. This is a synchronous-await pattern within the async command — no Tauri event streaming needed.

This command:
1. Validates the AI session exists and is active via `manager.get_session(session_id)`
2. Gets the current suggestion queue state for context (what's already suggested)
3. Builds a `SessionMessage` with a prompt that instructs the AI to generate suggestions in structured JSON
4. Calls `session.send_message(&msg)` directly (not via IPC `send_session_message`)
5. Drains the `ResponseStream` collecting `OutboundMessage::Text` chunks into a `String`
6. Parses the accumulated response JSON into `Vec<Suggestion>` via `try_extract_suggestions()`
7. Pushes valid suggestions to the `SuggestionQueue`
8. Emits `suggestions-changed` event
9. Returns the count of new suggestions

**Structured output format (JSON schema for AI response):**

```json
{
  "suggestions": [
    {
      "type": "productivity_tip",
      "content": "Consider batching similar tasks...",
      "priority": "medium",
      "reasoning": "Based on your current workflow pattern..."
    }
  ]
}
```

**Message sent to AI:**
```
Based on our conversation context, generate 1-3 actionable suggestions for the user.
Each suggestion should be specific, practical, and relevant to the current discussion.

Respond ONLY with a JSON object matching this schema:
{suggestions: [{type, content, priority, reasoning}]}

Valid types: work_guidance, email_draft, productivity_tip, workflow_optimization, context_based
Valid priorities: low, medium, high, critical
```

**Frontend integration:** Add a "Get Suggestions" button in the chat input area or sidebar. Clicking it calls `request_chat_suggestions` with the active session ID.

#### Files Modified
- `src-tauri/src/commands/suggestions.rs` — Add `request_chat_suggestions` command
- `src-tauri/src/main.rs` (or command registration) — Register the new command
- `crates/oneshim-web/frontend/src/pages/chat/ChatInput.tsx` — Add "Get Suggestions" button
- `crates/oneshim-web/frontend/src/pages/chat/index.tsx` — Handler for the button

#### Acceptance Criteria
- [ ] "Get Suggestions" button visible in chat UI when a session is active
- [ ] Clicking the button sends a structured request to the AI
- [ ] AI response parsed into Suggestion structs
- [ ] Valid suggestions pushed to SuggestionQueue
- [ ] Overlay badge updates with new count
- [ ] Invalid/malformed AI responses handled gracefully (toast error)
- [ ] Button shows loading state during generation
- [ ] Works with all transport types (subprocess, http_api, local_llm)

---

### 3.2 Suggestion Context in Chat (Task 2.2)

**Goal:** Users can click a suggestion to get more context/explanation in the chat.

#### Design

**New IPC command:** `explain_suggestion_in_chat`

```rust
#[command]
pub async fn explain_suggestion_in_chat(
    ai_state: tauri::State<'_, AiSessionRuntimeState>,
    suggestion_state: tauri::State<'_, SuggestionRuntimeState>,
    suggestion_id: String,
    session_id: Option<String>,  // If None, use the most recent active session
) -> Result<String, String>  // Returns the session_id used
```

This command:
1. Looks up the suggestion by ID from the queue (or history)
2. Validates `session_id` refers to an active AI session (if `None`, find the most recent active one; if none active, return error "No active chat session — open a chat first")
3. Composes a message: "Explain this suggestion in detail and help me understand how to act on it: [suggestion content]. Reasoning: [reasoning if available]"
4. Calls the existing `send_session_message` IPC command internally (NOT `session.send_message()` directly — unlike Task 2.1). This reuses the existing fire-and-forget streaming pattern: the spawned task drains the `ResponseStream` and emits `OutboundMessage` chunks via `ai-session:{session_id}` Tauri events. The chat page's `useMessageStream` hook automatically receives and displays the streaming response. This avoids duplicating the stream-to-event wiring logic.
5. Returns the session_id so the frontend can navigate to it

**No auto-create:** If no active session exists, return an error. The user must have opened the chat page and created a session first. This avoids the under-specified `SessionConfig` problem (transport, model, surface selection).

**Frontend integration:** Add an "Explain" action button on each `SuggestionItem` in the overlay panel. Clicking it:
1. Calls `explain_suggestion_in_chat` IPC
2. Receives the session_id (or error "No active chat session")
3. Emits Tauri event `navigate:chat` with `{ sessionId }` — the main window listens for this and navigates its router to the chat page
4. The chat page reads the `sessionId` from a new Tauri event listener (or a global navigation store) and selects that session

**Cross-window navigation:** Since overlay and chat page are in separate Tauri WebView windows, navigation uses Tauri events (NOT URL query params):
- Overlay emits: `navigate:chat { sessionId: string }`
- Main window listens: on `navigate:chat`, set active route to `/chat` and pass `sessionId` to the chat component via state
- Chat page: on mount or on receiving the event, if `sessionId` is provided, call `handleSelectSession(sessionId)`
- No new window management needed — the main window already exists

**Exposing `reasoning` field:** Currently `SuggestionViewDto` does not include `reasoning`. Add it so the AI has full context:

```rust
pub struct SuggestionViewDto {
    // ... existing fields
    pub reasoning: Option<String>,  // NEW
}
```

#### Files Modified
- `src-tauri/src/commands/suggestions.rs` — Add `explain_suggestion_in_chat` command, add `reasoning` to `SuggestionViewDto`
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionItem.tsx` — Add "Explain" button
- `crates/oneshim-web/frontend/src/overlay/types.ts` — Add `reasoning` to `SuggestionViewDto`
- `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx` — Handle explain action, navigate/emit

#### Acceptance Criteria
- [ ] "Explain" button on each suggestion item in overlay
- [ ] Clicking sends suggestion content to AI chat session
- [ ] If no active session, returns descriptive error ("No active chat session")
- [ ] User is directed to the chat page with the correct session
- [ ] `reasoning` field exposed in SuggestionViewDto
- [ ] Works for both queued and history suggestions

---

### 3.3 Chat Response to Suggestion Conversion (Task 2.3)

**Goal:** When the AI provides actionable suggestions in normal conversation (not via the explicit "Get Suggestions" flow), they are automatically detected and added to the suggestion queue.

#### Design

**Auto-detection in message stream:** After the AI response stream is fully drained in `send_session_message`'s spawned task, check the accumulated `assistant_content` (local `String` variable) for structured suggestion JSON.

**Key constraint:** The spawned streaming task in `send_session_message` does NOT have access to `SuggestionRuntimeState` or `SuggestionQueue`. To solve this:

1. Add `suggestion_state: Option<Arc<SuggestionManager>>` as a parameter to `send_session_message` (or pass it via the spawned task closure)
2. In `send_session_message`, clone the `SuggestionManager` Arc from `SuggestionRuntimeState` and move it into the spawned task
3. After the stream loop exits (all chunks drained into `assistant_content`), call `try_extract_suggestions(&assistant_content)`
4. If suggestions found, push to queue via the cloned `SuggestionManager`

**Detection strategy:** Look for a JSON block in the AI response that matches the suggestion schema. Best-effort heuristic — not all responses are checked.

**Implementation:** Add a `try_extract_suggestions` function (shared between Task 2.1 and 2.3):

```rust
fn try_extract_suggestions(response_text: &str) -> Vec<ParsedSuggestion> {
    // 1. Try to find JSON block: look for ```json ... ``` fences, or bare { "suggestions": [...] }
    // 2. Try to parse as { "suggestions": [...] }
    // 3. Validate each entry has: type (string), content (string), priority (string)
    // 4. Return valid entries (empty vec if nothing found or invalid)
}
```

**Where to call it:** In the `send_session_message` spawned task, **after the `while let Some(item) = stream.next().await` loop exits** (NOT on `OutboundMessage::Result` — the accumulated text is the local `assistant_content` variable, not the Result variant's content field). Extract from `assistant_content`.

**Tauri event:** Emit `chat:suggestions-extracted { count, session_id }` via the `AppHandle` (already available in the spawned task) so the frontend can show a notification.

**Frontend notification:** In the chat page, listen for `chat:suggestions-extracted` and show an inline notification "N suggestions added".

#### Files Modified
- `src-tauri/src/commands/ai_session.rs` — Add `SuggestionManager` Arc to spawned task, extract after stream loop
- `src-tauri/src/commands/suggestions.rs` — Add shared `try_extract_suggestions()` function (used by Task 2.1 too)
- `crates/oneshim-web/frontend/src/pages/chat/hooks/useMessageStream.ts` — Listen for `chat:suggestions-extracted` event
- `crates/oneshim-web/frontend/src/pages/chat/index.tsx` — Show notification when suggestions extracted

#### Acceptance Criteria
- [ ] AI responses containing structured suggestion JSON are auto-detected
- [ ] Detected suggestions pushed to SuggestionQueue
- [ ] Overlay badge updates
- [ ] Chat page shows inline notification "N suggestions added"
- [ ] Non-suggestion responses are not affected (no false positives)
- [ ] Graceful handling of malformed JSON (silent skip, no crash)
- [ ] Extraction is opt-in (only when response contains suggestion-shaped JSON)

---

## 4. Cross-Cutting Concerns

### 4.1 Navigation Between Chat and Overlay

The chat page and overlay are separate UI surfaces in separate Tauri WebView windows:
- Chat: `/pages/chat/` — full-page React app in the main window
- Overlay: Tauri webview overlay — separate transparent window

**Navigation strategy:** Use Tauri events for cross-window communication (NEW — these events don't exist yet):
- `navigate:chat { sessionId }` — emitted from overlay Rust backend, main window listens and routes to chat page with session pre-selected
- `overlay:toggle-suggestions` — already exists, chat page can trigger it to open suggestions panel

**Main window event listener:** The main window app (not overlay) needs a new `navigate:chat` event listener that updates its router state. This requires adding an event listener in the main window's root component.

### 4.2 Session Discovery

Task 2.2 needs to find an "active" chat session. The `AiSessionRuntimeState` provides `manager_impl()` which has session listing. In the Rust IPC command, call `manager.list_sessions()`, filter by `state == Active || state == Idle`, sort by `last_active` descending, use the first one. If none found, return error — do NOT auto-create (to avoid the under-specified SessionConfig problem).

### 4.3 Suggestion Source Tracking

Suggestions generated from chat should have `source: SuggestionSource::LlmLocal` (generated locally by the chat AI, not by server activity monitoring). This distinguishes them from SSE-delivered server suggestions.

### 4.4 No New Crate Dependencies

All features implementable with existing infrastructure:
- Chat: existing AI session system
- Suggestions: existing queue + manager
- JSON parsing: `serde_json` (already available)
- Events: Tauri event emission (already used)

### 4.5 Thread Safety

`request_chat_suggestions` accesses both `AiSessionRuntimeState` and `SuggestionRuntimeState`. These are separate Tauri managed states, each with their own locks. No new shared state or cross-state locking needed — each state is accessed independently.

### 4.6 Testing Strategy

| Layer | Test Type | Scope |
|-------|-----------|-------|
| `src-tauri` | Unit | `try_extract_suggestions` parser (various JSON formats) |
| `src-tauri` | Unit | Suggestion creation from parsed chat response |
| Frontend | Manual | Button interactions, navigation, notifications |

Estimated new test count: ~10-15 Rust tests (parser + integration).

---

## 5. Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| AI doesn't follow JSON schema | Medium | Best-effort parsing, fallback to empty, error toast |
| Token budget exhausted during suggestion request | Low | Check budget before sending, show error if insufficient |
| Session terminated mid-request | Low | Handle error in IPC command, return descriptive error |
| False positive extraction (normal response looks like suggestion JSON) | Medium | Require exact schema match with `"suggestions"` key + type field |
| Navigation between overlay and chat page unreliable | Low | Fallback: show toast with "Open chat to see response" |
| Large AI response causes parsing delay | Low | Run extraction in background task, not blocking |

---

## 6. Out of Scope

- Server-side suggestion generation API (would require server changes)
- Suggestion ranking based on chat context (future enhancement)
- Multi-turn suggestion refinement in chat
- Persistent association between chat sessions and suggestions
