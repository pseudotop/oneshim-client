# AI Session Manager Phase 3 — State Machine, Regime Sharing, Auto Mode, Cleanup

> **Version**: 1.1
> **Date**: 2026-03-25
> **Scope**: Session state machine, SharedRegimeState sharing, auto permission mode, dead_code cleanup, idle reaper wiring
> **Depends on**: Phase 1 (PR #162), Phase 2a (Claude subprocess), Phase 2b (HTTP/Ollama adapters)

---

## 1. Goals

1. **Session state machine** — `ManagedSession.state` transitions (Active→Idle→Terminated), update on send_message/idle/kill
2. **Idle reaper wiring** — Background task that periodically calls `reap_idle_sessions()`, state-aware reaping
3. **SharedRegimeState sharing** — Single instance shared between scheduler and SessionManager (fix "always unknown" regime)
4. **Auto permission mode** — `--permission-mode auto` support for Claude interactive sessions via config
5. **dead_code cleanup** — Remove `#[allow(dead_code)]` annotations from wired code
6. **Session catalog flags** — Populate `session_flags` in provider-surface-catalog.json

---

## 2. Non-Goals

- Computer Use integration (separate feature)
- Dispatch pattern (separate feature)
- Codex resume adapter (deferred, needs CLI verification)
- Web handler tests (follows project pattern of no handler-level tests)

---

## 3. Session State Machine

### 3.1 State Transitions

```
create_session() → Active
send_message() → Active (reset idle timer)
idle_timeout elapsed → Idle
send_message() on Idle → Active
kill_session() → Terminated (removed from map)
reap_idle_sessions() on Idle → Terminated
```

### 3.2 Implementation

`ManagedSession.state` is currently set to `Active` at creation and never updated.

Changes needed in `SessionManagerImpl`:

```rust
// In create_session — already sets Active (no change)

// New: update_last_active() — called by send_message IPC command
pub async fn touch_session(&self, session_id: &str) {
    if let Some(managed) = self.sessions.write().await.get_mut(session_id) {
        managed.last_active = Instant::now();
        managed.state = SessionState::Active;
    }
}

// In reap_idle_sessions — change condition from state==Idle to elapsed > timeout
pub async fn reap_idle_sessions(&self) {
    let idle_timeout = Duration::from_secs(self.config.idle_timeout_secs);
    let mut to_reap = vec![];
    {
        let mut sessions = self.sessions.write().await;
        for (id, managed) in sessions.iter_mut() {
            if managed.last_active.elapsed() > idle_timeout {
                if managed.state == SessionState::Active {
                    managed.state = SessionState::Idle;
                    info!(session_id = %id, "session marked idle");
                } else if managed.state == SessionState::Idle {
                    to_reap.push(id.clone());
                }
            }
        }
    }
    for id in to_reap {
        let _ = self.kill_session(&id).await;
    }
}
```

**Two-phase idle**: First timeout marks Active→Idle (warning). Second timeout reaps Idle→Terminated.
This gives users a grace period.

### 3.3 Idle Reaper Background Task

Spawn in `app_runtime_launch.rs` alongside the scheduler:

```rust
// Spawn idle reaper loop
let sm_clone = session_manager.clone();
let shutdown_rx_clone = shutdown_rx.clone();
tokio::spawn(async move {
    let interval = Duration::from_secs(sm_clone.config.health_check_interval_secs);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                sm_clone.reap_idle_sessions().await;
            }
            _ = shutdown_rx_clone.changed() => break,
        }
    }
});
```

---

## 4. SharedRegimeState Sharing

### 4.1 Problem

Currently two separate instances exist:
- **Scheduler** (`sync.rs:239`): `SharedRegimeState::new()` — receives regime updates from analysis pipeline
- **SessionManager** (`app_runtime_launch.rs:306`): `SharedRegimeState::new()` — always returns "unknown"

### 4.2 Solution

Create `SharedRegimeState` **once** in `app_runtime_launch.rs` and pass it to both:

```rust
// app_runtime_launch.rs — create shared instance
let shared_regime_state = Arc::new(SharedRegimeState::new());

// Pass to SessionContextAssembler
let context_assembler = SessionContextAssembler::new(
    storage.clone(), config.clone(), shared_regime_state.clone()
);

// Pass to SessionManager (via context_assembler)
let session_manager = SessionManagerImpl::new(..., Some(Arc::new(context_assembler)));

// Pass to scheduler (needs modification to accept external SharedRegimeState)
```

The scheduler's `sync.rs` currently creates its own `SharedRegimeState::new()` internally.
It needs to accept an `Option<Arc<SharedRegimeState>>` parameter and use the passed-in
instance if present, falling back to a new one for backward compatibility.

### 4.3 Threading Chain (4 Edit Sites)

The `SharedRegimeState` must be threaded through 4 layers:

| # | File | Change |
|---|------|--------|
| 1 | `src-tauri/src/app_runtime_launch.rs` | Create `Arc<SharedRegimeState>` once, pass to both `SessionContextAssembler` AND `AgentRuntimeBundle` |
| 2 | `src-tauri/src/agent_runtime.rs` (`AgentRuntimeBundle`) | Add `shared_regime: Option<Arc<SharedRegimeState>>` field + `with_shared_regime()` builder |
| 3 | `src-tauri/src/scheduler/mod.rs` (`Scheduler`) | Add `shared_regime: Option<Arc<SharedRegimeState>>` field + `with_shared_regime()` builder |
| 4 | `src-tauri/src/scheduler/loops/sync.rs` (`run_scheduler_loops`) | Use `self.shared_regime.clone().unwrap_or_else(\|\| Arc::new(SharedRegimeState::new()))` instead of `Arc::new(SharedRegimeState::new())` |

This ensures the scheduler's regime updates (from `monitor.rs:437 shared_regime.update(...)`)
are visible to `SessionContextAssembler` via the shared instance.

---

## 5. Auto Permission Mode

### 5.1 Config Addition

Add `permission_mode` to `AiSessionConfig`:

```rust
pub struct AiSessionConfig {
    // ... existing fields ...

    /// Permission mode for Claude interactive sessions (default: "dontAsk")
    /// Options: "default", "acceptEdits", "dontAsk", "auto"
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
}

fn default_permission_mode() -> String { "dontAsk".to_string() }
```

### 5.2 Claude Session Usage

In `ClaudeSubprocessSession::build_command()`, use the config value instead of hardcoded `"dontAsk"`:

```rust
// Before:
cmd.arg("--permission-mode").arg("dontAsk");

// After:
cmd.arg("--permission-mode").arg(&self.permission_mode);
```

The `permission_mode` comes from `AiSessionConfig` at session creation.

### 5.3 Catalog session_flags Update

The catalog already has `session_flags: ["--output-format", "stream-json", "--permission-mode", "dontAsk"]`.
**Remove** `"--permission-mode", "dontAsk"` from `session_flags` since permission mode is now config-driven:

```json
"session_flags": [
    "--output-format", "stream-json"
]
```

`--permission-mode` is set by `ClaudeSubprocessSession` from `AiSessionConfig.permission_mode`.

---

## 6. Dead Code Cleanup

Remove `#[allow(dead_code)]` from Phase 1/2 items that are now wired:

| File | Item | Wired By |
|------|------|----------|
| `session_manager.rs` | `ManagedSession` | Used by `create_session`, `get_session`, `kill_session` |
| `session_manager.rs` | `SessionManagerImpl` | Wired in `app_runtime_launch.rs` |
| `session_manager.rs` | `reap_idle_sessions()` | Wired by idle reaper background task (this phase) |
| `session_context.rs` | `SessionContextAssembler` | Wired in `app_runtime_launch.rs` |
| `auditing_session.rs` | `AuditingSession` | Used by `SessionManagerImpl::create_session` |
Items that remain `#[allow(dead_code)]` (not yet wired — Phase 4+):
- `ManagedSession.retry_count` — crash recovery (future)
- `context_assembler` field in `SessionManagerImpl` — system prompt integration (future)
- `with_secret_store()` builder in `SessionManagerImpl` — not called in production wiring

**Note**: `append_oneshot_flags` is already used in production — no dead_code annotation exists.

---

## 7. Crate Placement

| Component | Location | Change Type |
|-----------|----------|-------------|
| `AiSessionConfig.permission_mode` | `oneshim-core/config/sections/ai_session.rs` | Modify |
| `SessionManagerImpl` state machine | `src-tauri/src/session_manager.rs` | Modify |
| `ClaudeSubprocessSession` permission_mode | `src-tauri/src/session_adapters/claude_session.rs` | Modify |
| Idle reaper background task | `src-tauri/src/app_runtime_launch.rs` | Modify |
| SharedRegimeState sharing | `src-tauri/src/app_runtime_launch.rs` + `scheduler/loops/sync.rs` | Modify |
| `send_session_message` touch_session | `src-tauri/src/commands/ai_session.rs` | Modify (call `touch_session` before `send_message`) |
| Catalog session_flags | `specs/providers/provider-surface-catalog.json` | Modify (remove `--permission-mode dontAsk` — now config-driven) |

---

## 8. Testing Strategy

| What | How |
|------|-----|
| State transitions | Create session → verify Active, wait → verify Idle, send → verify Active again |
| Idle reaper | Create session, set short timeout, verify reaping after double-timeout |
| `touch_session` | Send message → verify last_active updated, state reset to Active |
| Permission mode config | Create session with "auto" → verify `build_command` uses "auto" |
| SharedRegimeState | Update regime → verify SessionContextAssembler returns correct label |
