# P2: Health Flag, Automation Status, Suggestion Reception — Spec Document

**Status**: Analysis Complete
**Severity**: P2 — Feature completeness
**Branch**: `fix/dmg-background`

---

## Issue 1: CLI Health Flag Not Wired

### Problem
The `AutomationController` has `with_health_flag()` method (line 78) and the builder creates `cli_health_flag` (line 92 of `app_runtime_launch.rs`), but the flag is never passed to the controller. A TODO comment at `automation_controller_builder.rs:215` explicitly notes this gap.

### Root Cause
`AutomationControllerBuilder` has no `cli_health_flag` field. The builder creates the controller and calls `controller.set_enabled(true)` but skips `controller.with_health_flag(flag)`.

### Impact
- CLI Bridge health status in tray menu shows "disconnected" permanently
- Health check loop (`health.rs`) reads `cli_ok = false` every 5 seconds
- `overlay:connection-changed` events never report CLI as connected

### Fix (3-step wiring)

**Step A**: `WebServerSupportContext` (web_server_runtime.rs:105)
- Add `cli_health_flag: Option<Arc<AtomicBool>>` field
- Add `with_cli_health_flag()` builder method
- Pass through `configure_automation_builder()` to the controller builder

**Step B**: `AutomationControllerBuilder` (automation_controller_builder.rs:28)
- Add `cli_health_flag: Option<Arc<AtomicBool>>` field
- Add `with_cli_health_flag()` builder method
- In `build()` at line 214, chain: `AutomationController::new(...).with_health_flag(flag)` before `set_enabled(true)`

**Step C**: `app_runtime_launch.rs` (line 162)
- After `WebServerSupportContext::new(...)`, chain `.with_cli_health_flag(cli_health_flag.clone())`

Note: `AutomationController.with_health_flag(self, flag)` uses builder pattern (takes `self`), so it must be called during construction, not after `set_enabled(&mut self)`.

### Files
| File | Change |
|------|--------|
| `src-tauri/src/web_server_runtime.rs` | Add field + method to `WebServerSupportContext`, pass through `configure_automation_builder` |
| `src-tauri/src/automation_controller_builder.rs` | Add field + method, wire in `build()` |
| `src-tauri/src/app_runtime_launch.rs` | Chain `.with_cli_health_flag(cli_health_flag.clone())` |

### Verification
- `cli_health_flag` is passed through builder → controller
- After automation command execution, `last_command_ok` flag updates
- Health check loop reflects CLI status in tray menu

---

## Issue 2: Automation Runtime Status (Partially Resolved)

### Problem (Original)
Settings toggle doesn't take effect without restart.

### Current State (Already Fixed)
The IPC command `get_automation_status` (`commands/system.rs:48-49`) already reads from `config_manager.get().automation.enabled`, NOT from `controller.is_some()`. This was fixed in commit `ca364f8`.

The REST endpoint `/automation/status` also uses `AutomationQueryService` which checks config.

### Remaining Gap
The `AutomationController` itself still runs with startup-time settings. If user disables automation in settings:
- UI correctly shows "disabled" (reads config)
- But the controller object still exists and could theoretically execute commands if called directly

### Practical Impact: **Low**
Since all automation endpoints (REST + IPC) check config before forwarding to the controller, the mismatch only matters if something bypasses the status check. This is defensive, not critical.

### Fix (Minimal — defensive check)
Add a config-aware guard in `AutomationController`'s execute methods, or have the REST/IPC handlers gate on `config.automation.enabled` before calling the controller. The handlers already do this for some endpoints.

**Decision**: Defer to a future session. The status reporting is correct. The runtime behavior gap is theoretical.

---

## Issue 3: SuggestionReceiver Instantiation

### Problem
The suggestion reception infrastructure is fully scaffolded but no `SuggestionReceiver` instance is ever created:
- `spawn_suggestion_loop()` exists (`scheduler/loops/suggestions.rs`)
- Scheduler has `suggestion_receiver` field + `with_suggestion_receiver()` builder method
- `AgentRuntimeBuilder` has `suggestion_receiver` field + builder method
- `run_scheduler_loops()` calls `spawn_suggestion_loop()` if receiver is `Some`
- But `AgentRuntimeBuilder.suggestion_receiver` is always `None`

### Root Cause
In `agent_runtime_support.rs`, `build_server_transports()` creates `TokenManager` and `BatchUploader` but does NOT create `SseStreamClient` or `SuggestionReceiver`. Nobody calls `builder.with_suggestion_receiver()`.

### Required Instantiation Chain
```
1. TokenManager (already exists in build_server_transports)
   ↓
2. SseStreamClient::new_with_tls(base_url, token_manager, max_retry_secs, tls)
   ↓
3. SuggestionQueue::new(max_suggestions)
   ↓
4. mpsc::channel::<Suggestion>(buffer_size)
   ↓
5. SuggestionReceiver::new(sse_client, notifier, queue, suggestion_tx)
   ↓
6. AgentRuntimeBuilder.with_suggestion_receiver(receiver)
   ↓
7. Scheduler.with_suggestion_receiver(receiver) [automatic via AgentRuntimeBundle]
   ↓
8. run_scheduler_loops → spawn_suggestion_loop [already implemented]
```

Steps 1, 6, 7, 8 already exist. Steps 2-5 need implementation.

### Dependencies
- `TokenManager` — shared with `HttpApiClient`, must be `Arc<TokenManager>` (already is)
- `SseStreamClient` — needs `base_url`, `token_manager`, `max_retry_secs`, `tls` from config
- `SuggestionQueue` — needs `max_suggestions` (default 50 from config)
- `DesktopNotifier` — optional, already available as `notification_manager` in support context
- `SuggestionConfig.enabled` — already wired to `AgentRuntimeBuilder.suggestions_enabled`

### Fix
1. In `build_server_transports()`, create `SseStreamClient` alongside `HttpApiClient` (reuse `TokenManager`)
2. Return the SSE client from the function
3. In `AgentSupportContextBuilder::build()`, create `SuggestionReceiver` using the SSE client
4. In `AgentRuntimeBundle::run()`, wire `receiver` via `builder.with_suggestion_receiver()`

### Files
| File | Change |
|------|--------|
| `src-tauri/src/agent_runtime_support.rs` | Create `SseStreamClient` + `SuggestionReceiver` |
| `src-tauri/src/agent_runtime.rs` | Wire receiver from support context into scheduler |

### Feature Gate
All suggestion code is behind `#[cfg(feature = "server")]`. The `spawn_suggestion_loop` already has this gate. The new code must also be gated.

### Verification
- With `server` feature enabled and `suggestions.enabled = true`:
  - `SuggestionReceiver` is instantiated
  - `spawn_suggestion_loop` is called in scheduler
  - SSE stream connects to server (may fail if server is down — that's expected)
- With `suggestions.enabled = false`:
  - No receiver created, no loop spawned
- Without `server` feature:
  - No receiver created (compile-time exclusion)
