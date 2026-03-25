# P2-P3 Batch — CLI Update, Context Wiring, Codex Adapter, Crash Recovery, README TOC

> **Version**: 1.1
> **Date**: 2026-03-25
> **Scope**: 5 independent features batched for efficiency
> **Depends on**: AI Session Manager Phase 1-3 (PR #162)

---

## Feature A: CLI Startup Update Check

### Problem
GUI has `UpdateCoordinator` + `Updater::check_for_updates()` with full lifecycle
(Checking→Available→Downloading→Installing), but there is no startup check — updates
are only detected on user action or configured interval.

### Solution
Add a **non-blocking startup check** that runs immediately on app launch:

1. On startup, spawn a `tokio::spawn` task that:
   - Calls existing `Updater::check_for_updates()` (GitHub Releases API, 3s timeout)
   - If new version available → emit Tauri event `update-available` with version info
   - Frontend shows notification banner (non-blocking, no interruption)
   - If error or timeout → silently ignore (don't block startup)

2. Existing `UpdateCoordinator` interval check continues as-is for runtime polling.

### Implementation
- Modify: `src-tauri/src/update_runtime.rs` — add startup check in `build_and_spawn()`
- The check reuses `Updater::check_for_updates()` (already handles GitHub API, version comparison)
- No new crate dependencies needed (`self_update`, `semver` already present)

### Files
| File | Change |
|------|--------|
| `src-tauri/src/update_runtime.rs` | Add startup check task before coordinator loop |

---

## Feature B: Context Assembler Wiring in SessionManager

### Problem
`SessionManagerImpl` stores `context_assembler: Option<Arc<SessionContextAssembler>>`
but never uses it. `create_session` does not inject a system prompt from local context.

### Solution
In `SessionManagerImpl::create_session`, when creating a session:

1. If `config.system_prompt` is `Some(...)`, use the user-provided prompt
2. Otherwise, if `context_assembler` is available, call `build_system_message().await`
   to generate a context-aware system prompt from local data (regime, activity, suggestions)
3. Pass the system prompt to the adapter's constructor

For `ClaudeSubprocessSession`, the system prompt is already accepted via `config.system_prompt`.
For `HttpApiSession` and `LocalLlmSession`, the system prompt initializes the first
`ChatMessage { role: System, ... }` in history.

### Files
| File | Change |
|------|--------|
| `src-tauri/src/session_manager.rs` | Use `context_assembler` in `create_session`, remove `#[allow(dead_code)]` from field |

---

## Feature C: Codex Resume Adapter

### Problem
`SessionTransport::Subprocess` only creates `ClaudeSubprocessSession`. Codex CLI
(`codex resume SESSION_ID`) supports session-specific resume — previously thought to
be limited to `--last` only.

### Solution
Add `CodexSubprocessSession` adapter:

```
Turn 1: codex exec --sandbox read-only --skip-git-repo-check -C <workdir> \
          --output-last-message <output> - < prompt
         → reads output file → process exits, session saved

Turn 2: codex resume <session-id> "follow-up prompt" \
          --output-last-message <output>
         → reads output file → process exits, continues session
```

Key differences from Claude adapter:
- No `--output-format stream-json` — Codex uses `--output-last-message` file output
- First turn uses `exec`, subsequent turns use `resume <session-id>`
- Response is read from file, not stdout streaming
- `ResponseStream` yields a single `OutboundMessage::Result` (no streaming chunks)

### Surface Selection
`SessionManagerImpl::create_session` for `SessionTransport::Subprocess`:
- Detect available CLI surfaces
- If `config.surface_id` matches Codex surface → create `CodexSubprocessSession`
- If matches Claude surface → create `ClaudeSubprocessSession` (existing)
- If no `surface_id` specified → prefer Claude, fallback to Codex

### Files
| File | Change |
|------|--------|
| `src-tauri/src/session_adapters/codex_session.rs` | NEW: `CodexSubprocessSession` adapter |
| `src-tauri/src/session_adapters/codex_normalizer.rs` | NEW: Codex output → `OutboundMessage` |
| `src-tauri/src/session_adapters/mod.rs` | Add modules |
| `src-tauri/src/session_manager.rs` | Route to Codex adapter based on surface_id |

---

## Feature D: Crash Recovery (retry_count)

### Problem
`ManagedSession.retry_count` exists but is never used. When a subprocess crashes
mid-response, the session is lost.

### Solution
Add crash recovery to `SessionManagerImpl`:

1. When `send_message` stream yields an error with `retryable: true`:
   - Increment `retry_count`
   - If `retry_count < max_retries` (config: 3):
     - Mark state as `Recovering`
     - Re-create the adapter (for subprocess: new process with `--continue`)
     - Re-send the failed message
   - If `retry_count >= max_retries`:
     - Mark state as `Failed`
     - Return error to caller

2. Add `max_retries` to `AiSessionConfig`:
   ```rust
   pub max_retries: u32,  // default: 3
   ```

3. Recovery is handled **in the Tauri IPC command** (`send_session_message`),
   not in the adapter itself — the adapter just reports errors, the manager retries.

### Files
| File | Change |
|------|--------|
| `crates/oneshim-core/src/config/sections/ai_session.rs` | Add `max_retries` field |
| `src-tauri/src/session_manager.rs` | Add `recover_session` method, use `retry_count` |
| `src-tauri/src/commands/ai_session.rs` | Add retry loop in `send_session_message` |

---

## ~~Feature E: README Table of Contents~~ — DROPPED

Already implemented in main branch. No action needed.

---

## Testing Strategy

| Feature | Tests |
|---------|-------|
| A: Update check | Verify startup task spawns, handles timeout gracefully |
| B: Context wiring | Mock assembler, verify system prompt injected into session config |
| C: Codex adapter | Codex output normalization, session resume command building |
| D: Crash recovery | Simulate retryable error, verify retry_count increment + re-creation |
| ~~E: README TOC~~ | Dropped — already exists |

---

## Implementation Priority

| # | Feature | Complexity | Dependencies |
|---|---------|------------|-------------|
| 1 | B: Context wiring | Small | None |
| 2 | A: CLI update check | Small | None |
| 3 | C: Codex adapter | Medium | None |
| 4 | D: Crash recovery | Medium | None |
