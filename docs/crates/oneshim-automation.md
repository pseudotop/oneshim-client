[English](./oneshim-automation.md) | [한국어](./oneshim-automation.ko.md)

# oneshim-automation

The automation control crate. Handles policy-based command execution, audit logging, OS native sandbox, UI automation intent resolution, and workflow presets.

## Overview

Executes automation commands received from the server after policy token verification, with all commands recorded in audit logs.
Uses a 2-layer action model: **AutomationIntent** (server→client high-level intent) → **AutomationAction** (client internal low-level action).

## Directory Structure

```
oneshim-automation/src/
├── lib.rs              # Crate root (9 modules)
├── audit.rs            # AuditLogger — audit logging (14 methods)
├── controller.rs       # AutomationController — policy verification + command execution
├── input_driver.rs     # NoOpInputDriver — test/default input driver
├── intent_resolver.rs  # IntentResolver + IntentExecutor — intent resolution + execution
├── local_llm.rs        # LocalLlmProvider — local LLM (rule-based)
├── policy.rs           # PolicyClient — server policy sync + verification
├── presets.rs          # builtin_presets() — 10 builtin workflows
├── resolver.rs         # Policy → sandbox profile resolver (3 pure functions)
└── sandbox/            # OS native kernel sandbox
    ├── mod.rs          # create_platform_sandbox() factory
    ├── noop.rs         # NoOpSandbox — passthrough when disabled
    ├── linux.rs        # LinuxSandbox — seccomp + namespaces
    ├── macos.rs        # MacOsSandbox — sandbox-exec + App Sandbox
    └── windows.rs      # WindowsSandbox — Job Objects + AppContainers
```

## Modules

### `controller.rs` — AutomationController

Central controller for policy verification + command execution + audit logging + sandbox management.

- `AutomationController::new(sandbox, sandbox_config)` — Constructor (`Arc<dyn Sandbox>` + `SandboxConfig`)
- `set_intent_executor(executor)` — Inject IntentExecutor
- `execute_command(command)` — Policy verification → audit log → action dispatch → return result
- `execute_intent(intent, config)` — Execute high-level intent (delegates to IntentExecutor)
- `resolve_for_command(command)` — Determine dynamic SandboxConfig based on policy
- `dispatch_action_with_config(action, config)` — Execute action with timeout
- Disabled by default (`enabled: false`), activate via `set_enabled()`
- Execution timeout based on `tokio::time::timeout`

### `policy.rs` — PolicyClient

Server policy synchronization + command verification + process permission management.

- `ExecutionPolicy` — Policy ID, process name, binary hash, argument patterns, sudo required, audit level
  - `sandbox_profile: Option<SandboxProfile>` — Server override
  - `allowed_paths: Vec<String>` — Allowed paths per policy
  - `allow_network: Option<bool>` — Network override
- `AuditLevel` enum: None, Basic, Detailed, Full
- `PolicyCache` — Policy list + TTL cache (default 5 minutes)
- `validate_command()` — Cache validity + non-empty token verification
- `validate_args()` — Glob pattern-based argument validation (`*` wildcard)
- `is_process_allowed()` — Fast process permission lookup via HashSet

### `audit.rs` — AuditLogger

Local VecDeque buffer + batched transmission audit log. Includes non-destructive query methods.

#### Types

```rust
pub enum AuditStatus { Started, Completed, Failed, Denied, Timeout }

pub struct AuditEntry {
    pub entry_id: String,
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub command_id: String,
    pub action_type: String,
    pub status: AuditStatus,
    pub details: Option<String>,
    pub execution_time_ms: Option<u64>,
}
```

#### Methods (14)

| Category | Method | Description |
|----------|--------|-------------|
| Basic Logging | `log_start()` | Record command start |
| | `log_complete()` | Record command completion |
| | `log_denied()` | Record policy denial |
| | `log_failed()` | Record execution failure |
| Conditional Logging | `log_start_if(level, ...)` | Skip if AuditLevel::None |
| | `log_complete_with_time(level, ..., ms)` | Record with execution time |
| | `log_timeout(...)` | Record timeout |
| Batch Management | `has_pending_batch()` | Whether ready for transmission |
| | `pending_count()` | Number of pending entries |
| | `drain_batch()` | Extract batch-sized chunk |
| | `drain_all()` | Extract all (for shutdown) |
| Non-destructive Query | `recent_entries(limit)` | Query latest N entries (for API) |
| | `entries_by_status(status, limit)` | Filter by status |
| | `stats()` | Aggregate statistics (total, success, failed, denied, timeout) |

- Oldest entries are automatically removed on buffer overflow
- Default settings: max 1000 buffer entries, batch size of 50

### `resolver.rs` — Policy → Sandbox Resolver

3 pure functions (stateless, easy to test):

| Function | Description |
|----------|-------------|
| `resolve_sandbox_profile(policy)` | AuditLevel → SandboxProfile cascading mapping |
| `resolve_sandbox_config(policy, base)` | Generate dynamic SandboxConfig based on policy |
| `default_strict_config(base)` | Strict settings for commands without policies |

#### AuditLevel → SandboxProfile Mapping

```
AuditLevel::None     → SandboxProfile::Permissive
AuditLevel::Basic    → SandboxProfile::Standard
AuditLevel::Detailed → SandboxProfile::Strict
AuditLevel::Full     → SandboxProfile::Strict
```

- `requires_sudo=true` promotes Permissive → Standard
- Server `sandbox_profile` override takes priority

### `intent_resolver.rs` — IntentResolver + IntentExecutor

Converts high-level intents (AutomationIntent) into low-level action (AutomationAction) sequences and executes them.

- `IntentResolver` — UI element discovery → coordinate calculation → action conversion
  - OCR-based element discovery (`ElementFinder`)
  - LLM-based intent interpretation (`LlmProvider`)
  - Confidence verification + retry logic (`IntentConfig`)
- `IntentExecutor` — Sequential execution of converted actions + result verification
  - `execute_intent(intent, config)` → `IntentResult`
  - Post-execution text verification (`verify_after_action`)
  - Retry (`max_retries`, `retry_interval_ms`)

### `presets.rs` — Builtin Workflow Presets

The `builtin_presets()` function returns 10 builtin presets. Platform-specific key mapping is applied automatically.

#### Productivity Presets (4)

| ID | Name | Steps |
|----|------|-------|
| `save-file` | Save File | `ExecuteHotkey(["Cmd/Ctrl", "S"])` |
| `undo` | Undo | `ExecuteHotkey(["Cmd/Ctrl", "Z"])` |
| `select-all-copy` | Select All and Copy | `Cmd/Ctrl+A` → 200ms → `Cmd/Ctrl+C` |
| `find-replace` | Find/Replace | `ExecuteHotkey(["Cmd/Ctrl", "H"])` |

#### App Management Presets (3)

| ID | Name | Steps |
|----|------|-------|
| `switch-next-app` | Switch to Next App | `Cmd/Alt+Tab` |
| `close-window` | Close Current Window | `Cmd/Ctrl+W` |
| `minimize-all` | Minimize All | macOS: `Cmd+Option+H+M` / Win: `Win+D` |

#### Workflow Presets (3)

| ID | Name | Steps |
|----|------|-------|
| `morning-routine` | Morning Routine | `ActivateApp(Mail)` → 2s → `Calendar` → 2s → `VSCode` |
| `meeting-prep` | Meeting Preparation | `ActivateApp(Zoom)` → 1s → `Notes` |
| `end-of-day` | End of Day | `Cmd/Ctrl+S` → 1s → `Cmd/Ctrl+Q` |

**Helper Functions:**
- `platform_modifier()` — macOS: `"Cmd"`, others: `"Ctrl"`
- `platform_alt_modifier()` — macOS: `"Cmd"`, others: `"Alt"`

### `sandbox/` — OS Native Kernel Sandbox

The `create_platform_sandbox()` factory function creates platform-specific sandboxes.

| Platform | Implementation | Technology |
|----------|---------------|------------|
| `config.enabled=false` | `NoOpSandbox` | Passthrough (no restrictions) |
| Linux | `LinuxSandbox` | seccomp + namespaces |
| macOS | `MacOsSandbox` | sandbox-exec + App Sandbox |
| Windows | `WindowsSandbox` | Job Objects + AppContainers |
| (unsupported) | `NoOpSandbox` (fallback) | Warning log + passthrough |

### `input_driver.rs` — NoOpInputDriver

Test/default input driver. `InputDriver` trait implementation that logs all actions and ignores them.

### `local_llm.rs` — LocalLlmProvider

Local LLM/rule-based intent interpretation. `LlmProvider` trait implementation. Operates via rule matching without external APIs.

## Dependencies

```
oneshim-automation → oneshim-core (CoreError, models, port traits)
```

## Security

- **Policy token required**: All automation commands require a server-issued policy token
- **Binary hash verification**: Tamper detection via `ExecutionPolicy.process_hash`
- **Argument pattern restriction**: Allowed arguments restricted via glob patterns
- **OS native sandbox**: Kernel-level isolation (seccomp, sandbox-exec, Job Objects)
- **Policy → Sandbox auto binding**: SandboxProfile automatically determined by AuditLevel
- **Execution timeout**: Forced termination based on `tokio::time::timeout`
- **Audit log recording**: All executions/denials/failures/timeouts recorded in audit log
- **Disabled by default**: `AutomationController` is disabled by default
- **Privacy Gateway**: PII filtering + sensitive app blocking + consent verification for external data transmission

## Tests

| Module | Test Count | Description |
|--------|-----------|-------------|
| controller | 6 | Action/result serialization, intent execution, timeout |
| policy | 7 | Policy serialization, argument validation, policy update, sandbox fields |
| audit | 7 | Log/drain, buffer overflow, partial batch extraction, serialization, non-destructive query, statistics |
| resolver | 5 | Profile mapping, sudo promotion, path merging, strict default, server override |
| presets | 3 | Preset loading, platform key mapping, step verification |
| sandbox | 3 | Factory creation, NoOp passthrough, capability reporting |
| intent_resolver | 2 | Intent resolution, action conversion |
| **Total** | **33** | - |
