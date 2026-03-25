# P2: Health Flag, Automation Status, Suggestion Reception — Spec Document

**Status**: Verified (2026-03-25)
**Severity**: P2 — Feature completeness
**Scope**: Wire DesktopNotifier into SuggestionReceiver + clean up vestigial channel

---

## Issue 1: CLI Health Flag — RESOLVED

**Verified 2026-03-25**: Full wiring chain is in place.

```
cli_health_flag (app_runtime_launch.rs:218)
  → WebServerSupportContext.with_cli_health_flag() (app_runtime_launch.rs:373)
  → configure_automation_builder() (web_server_runtime.rs:153)
  → AutomationControllerBuilder.with_cli_health_flag() (automation_controller_builder.rs:65)
  → AutomationController.with_health_flag() (controller/mod.rs:78)
```

No action needed.

---

## Issue 2: Automation Runtime Status — DEFERRED

Settings toggle UI correctly reads config. Runtime gap is theoretical (all endpoints
check config before forwarding). Defer to future session.

---

## Issue 3: SuggestionReceiver DesktopNotifier Not Wired

### Problem

`SuggestionReceiver` is fully instantiated and wired into the scheduler loop, but
receives `None` for the `notifier` parameter. When SSE suggestions arrive:

- Pushed to shared queue (SuggestionManager reads it) — **works**
- Desktop notification shown — **silent** (notifier is None)
- Channel send to `suggestion_tx` — **always fails** (_suggestion_rx dropped immediately)

### Root Cause

In `agent_runtime_support.rs` lines 211-216:

```rust
oneshim_suggestion::receiver::SuggestionReceiver::new(
    sse_client,
    None, // ← TauriNotifier exists but not passed
    queue,
    suggestion_tx,
)
```

`TauriNotifier` is created at line 180-185, **before** `SuggestionReceiver` (line 201-224).
It's passed to `NotificationManager` but not to `SuggestionReceiver`.

### Secondary Issue: Vestigial mpsc Channel

```rust
let (suggestion_tx, _suggestion_rx) = tokio::sync::mpsc::channel(64);  // line 210
```

`_suggestion_rx` is dropped immediately. Every `suggestion_tx.send()` in
`SuggestionReceiver::handle_suggestion()` fails silently with a debug log.
The channel was intended for real-time push but has no consumer.

### Impact

| Path | Status |
|------|--------|
| SSE → Queue → SuggestionManager → IPC query | Working |
| SSE → Desktop notification | Silent (notifier = None) |
| SSE → mpsc channel → (subscriber) | Dead (rx dropped) |

### Fix

**Fix A — Wire notifier (1 line change)**

`src-tauri/src/agent_runtime_support.rs` line 214:

```diff
 oneshim_suggestion::receiver::SuggestionReceiver::new(
     sse_client,
-    None, // desktop notifier wired separately via notification_manager
+    Some(notifier.clone()),
     queue,
     suggestion_tx,
 )
```

`notifier` (Arc<dyn DesktopNotifier>) is already in scope — created at line 180.

**Fix B — Remove vestigial channel**

Option B1 (minimal — keep interface, drop sender):
Remove `suggestion_tx` from `SuggestionReceiver::new()` signature and the
`handle_suggestion()` send call. This changes the library crate API.

Option B2 (preserve interface — store rx for future use):
Store `_suggestion_rx` in `AgentSupportContext` for future real-time push consumers.

**Recommended**: B1. The channel has no consumer and the queue is the authoritative
path. Removing dead code is cleaner than carrying vestigial state.

### Files

| File | Change | Lines |
|------|--------|-------|
| `src-tauri/src/agent_runtime_support.rs` | Pass `notifier.clone()` instead of `None` | ~1 line |
| `crates/oneshim-suggestion/src/receiver.rs` | Remove `suggestion_tx` field + send call (Fix B1) | ~10 lines |
| `src-tauri/src/agent_runtime_support.rs` | Remove `mpsc::channel` creation (Fix B1) | ~1 line |

### Verification

1. `cargo check --workspace` — no compile errors
2. `cargo test -p oneshim-suggestion` — existing tests pass
3. `cargo clippy --workspace` — no new warnings
4. Manual: with `suggestions.enabled = true` + server running, SSE suggestion
   triggers desktop notification

### Risk Assessment

- **Fix A**: Zero risk — adds a clone of an already-existing Arc
- **Fix B1**: Low risk — removes dead code path, existing tests cover queue path
