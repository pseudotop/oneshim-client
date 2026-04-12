# Design: Windows Sandbox Real Implementation + Cross-Platform Process Isolation

**Date**: 2026-04-12
**Approach**: B1 — Separate sandbox-worker binary with real action execution inside sandboxed child process

---

## 1. Problem Statement

### 1A. Windows Sandbox Stubs

`WindowsSandbox::create_job_object()` and `create_restricted_token()` at `sandbox/windows.rs:150-168` are log-only stubs. `capabilities()` falsely reports `resource_limits: true` and `process_isolation: true`.

### 1B. Process Isolation — Thread-Pool Leak

`execute_sandboxed()` applies sandbox constraints (Landlock, seccomp, rlimit) inside `tokio::task::spawn_blocking`. These constraints are irreversible per-thread and leak to subsequent unrelated tasks reusing that blocking thread.

### 1C. Carrier Process is Validation-Only

The current carrier process (`/bin/echo` on macOS) only validates that constraints can be applied. The actual automation action runs in the parent, outside the sandbox. No real enforcement.

### 1D. Missing Error Variant

`AutomationError::SandboxEnforcement` is used in 6 locations in `linux.rs` inside `#[cfg(feature = "linux-sandbox")]` blocks but does not exist in `error.rs`. Compiles on macOS CI only because `cfg(target_os = "linux")` excludes the module.

---

## 2. Design Goals

| Goal | Priority |
|------|----------|
| Run automation actions inside sandboxed child process | Must |
| Windows Job Object + Restricted Token real enforcement | Must |
| Fix `SandboxEnforcement` error variant | Must |
| Subprocess model for Linux (avoid thread-pool leak) | Must |
| Subprocess model for Windows (CreateProcessWithTokenW in Job Object) | Must |
| Update macOS to use worker binary instead of `/bin/echo` | Must |
| Accurate `capabilities()` reporting on all platforms | Must |
| Feature-gate behind `windows-sandbox` / `linux-sandbox` | Should |
| Cross-compile safety (compiles on all hosts) | Must |

---

## 3. Architecture

### 3.1 Overview

A new `oneshim-sandbox-worker` binary crate acts as the sandboxed execution host. The parent spawns it as a child, applies platform-specific sandbox constraints at spawn time, and communicates via stdin/stdout JSON pipe.

```
Parent (src-tauri / ActionDispatcher)       Child (oneshim-sandbox-worker)
  |                                           |
  |-- spawn child with sandbox constraints -->|
  |   Linux: pre_exec (Landlock+seccomp+rlimit)
  |   Windows: Job Object + CreateProcessWithTokenW
  |   macOS: sandbox-exec -p <sbpl> wrapper
  |                                           |
  |-- write SandboxRequest JSON to stdin ---->|
  |                                           |-- deserialize request
  |                                           |-- match + run action (enigo/sys)
  |                                           |-- serialize result
  |<-- read SandboxResponse JSON from stdout--|
  |                                           |-- exit(0)
  |-- return CommandResult                    |
```

Key principle: sandbox constraints applied by parent at spawn time. The child only receives and runs the action.

### 3.2 New Crate

```
crates/oneshim-sandbox-worker/
  Cargo.toml
  src/main.rs
```

IPC Protocol (JSON, one line per message, stdin/stdout):

```rust
// Parent -> Child (one JSON line on stdin)
#[derive(Serialize, Deserialize)]
struct SandboxRequest {
    action: AutomationAction,
}

// Child -> Parent (one JSON line on stdout)
#[derive(Serialize, Deserialize)]
struct SandboxResponse {
    success: bool,
    error: Option<String>,
}
```

The worker binary is minimal (~80 lines): read one JSON line from stdin, deserialize SandboxRequest, match on action type, call system API, serialize SandboxResponse to stdout, exit. No async runtime needed.

Dependencies:
```toml
[dependencies]
oneshim-core = { workspace = true }   # AutomationAction model
serde = { workspace = true }
serde_json = { workspace = true }

[features]
enigo = ["dep:enigo"]  # Actual mouse/keyboard input
```

### 3.3 Worker Binary Discovery

```rust
fn resolve_worker_path() -> Result<PathBuf, CoreError> {
    // 1. Tauri sidecar: same dir as main binary, platform-suffixed
    // 2. Same directory as current running binary
    // 3. PATH lookup
    // 4. Error: "sandbox worker binary not found"
}
```

Tauri sidecar config in `tauri.conf.json`:
```json
{ "bundle": { "externalBin": ["oneshim-sandbox-worker"] } }
```

---

## 4. Platform Implementations

### 4.1 Linux

Replace `spawn_blocking` with `tokio::process::Command` + `pre_exec` on the sandbox-worker binary.

Key changes from current implementation:
- Build BPF program BEFORE fork (heap alloc unsafe post-fork)
- `cmd.as_std_mut().pre_exec()` applies Landlock + seccomp + rlimit in child only
- seccomp allows `execve` (needed for carrier binary to start). Mitigation: Landlock restricts reachable executables
- Add worker binary path to Landlock read_paths explicitly
- `tokio::time::timeout` wraps child wait
- Permissive fast-path: skip subprocess when no constraints apply

Separated build/apply for seccomp:
- `build_seccomp_bpf()` returns `BpfProgram` (safe pre-fork, heap allocation)
- `apply_seccomp_bpf_sync()` calls `seccompiler::apply_filter` (safe post-fork, just prctl syscall)
- `apply_landlock_rules_sync()` and `apply_resource_limits_sync()` return `std::io::Result` for pre_exec

Write SandboxRequest to child stdin, read SandboxResponse from child stdout.

Known limitation: `/usr/lib` in Landlock read_paths contains the dynamic linker and other executables reachable via allowed `execve`. Accepted tradeoff: network and fork still blocked by seccomp, resources constrained by rlimit.

Updated capabilities:
```rust
process_isolation: true,  // NOW TRUE: child process model
```

### 4.2 Windows

OwnedHandle RAII wrapper (`HANDLE` is `isize` in windows-sys 0.61):
```rust
struct OwnedHandle(isize);
impl OwnedHandle {
    fn is_valid(&self) -> bool { self.0 != 0 && self.0 != -1 }
}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.is_valid() { unsafe { CloseHandle(self.0); } }
    }
}
```

Execution flow:
1. `CreateJobObjectW(null, null)` — configure memory/CPU/process limits via `SetInformationJobObject`
2. `OpenProcessToken` + `CreateRestrictedToken` — create restricted token
3. `CreateProcessWithTokenW(restricted_token, ..., CREATE_SUSPENDED, ...)` — spawn child with restricted token as primary token. This is the correct API for desktop apps (requires `SE_IMPERSONATE_NAME`, available to interactive users). NOT `CreateProcessAsUserW` (requires `SE_ASSIGNPRIMARYTOKEN_NAME`, system services only). NOT `ImpersonateLoggedOnUser` + `CreateProcessW` (child ignores thread impersonation token).
4. `AssignProcessToJobObject(job, process)` — on failure: `TerminateProcess(process, 1)` before unwinding (prevents zombie suspended process)
5. `ResumeThread(thread)`
6. Write SandboxRequest to child stdin pipe
7. `WaitForSingleObject(process, timeout_ms)` — `WAIT_TIMEOUT` triggers `TerminateProcess`
8. Read SandboxResponse from child stdout pipe
9. OwnedHandle Drop auto-closes all handles

API error check rules:
- `CreateJobObjectW` returns 0 on failure (NOT `INVALID_HANDLE_VALUE`)
- `CreateProcessWithTokenW` returns 0 (FALSE) on failure
- `OpenProcessToken` returns 0 (FALSE) on failure
- `WaitForSingleObject` returns `WAIT_FAILED` (0xFFFFFFFF) on failure

Feature-disabled fallback (same dual `#[cfg]` pattern as linux.rs):
```rust
#[cfg(not(feature = "windows-sandbox"))]
{
    tracing::debug!("Windows sandbox (enforcement requires windows-sandbox feature)");
    Ok(())
}
```

Updated capabilities (feature enabled):
```rust
SandboxCapabilities {
    filesystem_isolation: false,  // Job Objects don't isolate filesystem
    syscall_filtering: false,     // Windows has no syscall filtering
    network_isolation: false,     // Would need WFP (out of scope)
    resource_limits: true,        // ENFORCED via Job Object
    process_isolation: true,      // ENFORCED via restricted token + Job Object
}
// Feature disabled: all false
```

Note: `process_isolation: true` does NOT imply UI isolation. Child inherits the desktop.

### 4.3 macOS

Minimal change: replace `/bin/echo` carrier with the sandbox-worker binary.

```rust
fn build_sandbox_command(&self, profile: &str) -> Result<(String, Vec<String>), CoreError> {
    let worker_path = resolve_worker_path()?;
    let args = vec![
        "-p".into(), profile.into(), "--".into(),
        worker_path.to_string_lossy().to_string(),
    ];
    Ok((self.sandbox_exec_path.clone().unwrap(), args))
}
```

Write SandboxRequest to child stdin, read SandboxResponse from stdout. Wrap child wait with `tokio::time::timeout` (same as Linux). The `sandbox-exec` wrapper applies Seatbelt SBPL constraints.

---

## 5. Error Variant Fix

File: `crates/oneshim-automation/src/error.rs`

```rust
// Add to AutomationError enum:
#[error("sandbox enforcement failed: {0}")]
SandboxEnforcement(String),

// Add to From<AutomationError> for CoreError:
AutomationError::SandboxEnforcement(msg) => CoreError::SandboxExecution(msg),
```

---

## 6. Feature Gating

| Feature Flag | Platform | What It Gates |
|---|---|---|
| `linux-sandbox` (existing) | Linux | Landlock + seccomp real enforcement in pre_exec |
| `windows-sandbox` (new) | Windows | Win32 Job Object + Restricted Token + CreateProcessWithTokenW |

Without feature flags: log-only stub behavior with dual `#[cfg(feature)]` / `#[cfg(not(feature))]` code paths.

Workspace `Cargo.toml` additions for windows-sys:
- `Win32_System_JobObjects` — Job Object APIs
- `Win32_Security_Authentication_Identity` — CreateRestrictedToken, CreateProcessWithTokenW

---

## 7. Files Changed

| File | Change |
|------|--------|
| `crates/oneshim-sandbox-worker/` | NEW binary crate |
| `crates/oneshim-automation/src/error.rs` | Add SandboxEnforcement variant |
| `crates/oneshim-automation/src/sandbox/windows.rs` | Real Win32 API + subprocess |
| `crates/oneshim-automation/src/sandbox/linux.rs` | pre_exec subprocess model |
| `crates/oneshim-automation/src/sandbox/macos.rs` | Use worker binary as carrier |
| `crates/oneshim-automation/src/sandbox/mod.rs` | Worker path resolution |
| `crates/oneshim-automation/src/action_dispatcher.rs` | Pipe-based dispatch to worker |
| `crates/oneshim-automation/Cargo.toml` | windows-sandbox feature + deps |
| `Cargo.toml` (workspace) | Add worker crate + windows-sys features |
| `src-tauri/Cargo.toml` | Forward windows-sandbox feature |
| `src-tauri/tauri.conf.json` | Sidecar configuration |
| CI workflows | Enable windows-sandbox on Windows runner |

---

## 8. Test Strategy

### Unit Tests (all platforms)
- `build_job_limits_profiles` — existing
- `build_token_restrictions_profiles` — existing
- `error_variant_sandbox_enforcement` — new: verify error conversion
- `permissive_fast_path_skips_subprocess` — new: Permissive with no limits returns early
- `landlock_rules_include_worker_binary` — new: worker path in read_paths
- `worker_request_response_roundtrip` — new: SandboxRequest/Response serde
- `worker_malformed_input_returns_error` — new: invalid JSON handling

### Integration Tests (platform-specific)
- `job_object_limits_enforced` — Windows: verify limits via QueryInformationJobObject
- `restricted_token_created` — Windows: verify token restrictions
- `child_timeout_terminates` — both: verify timeout kills child
- `linux_parent_thread_not_contaminated` — Linux: call twice, second succeeds
- `linux_carrier_missing_returns_error` — Linux: clear error when worker absent
- `assign_job_failure_terminates_child` — Windows: suspended child killed on error
- `worker_executes_action` — all: end-to-end dispatch through worker

### Cross-Platform Safety
- `cargo check --workspace` on all 3 OS runners
- Feature-disabled compilation on non-target platforms

---

## 9. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| Win32 HANDLE leak | High | OwnedHandle RAII with isize comparison |
| seccomp blocks execve in pre_exec | High | Allow execve; Landlock restricts reachable binaries |
| Landlock blocks worker binary path | High | Add worker path to read_paths explicitly |
| `/usr/lib` contains reachable executables | Medium | Accepted: network + fork blocked, rlimit constrains |
| Child process hangs | High | tokio::time::timeout (Linux/macOS) + WaitForSingleObject + TerminateProcess (Windows) |
| Worker binary not found at runtime | Medium | Multi-path resolution + clear error message |
| Heap alloc in post-fork context | Medium | Build BPF before fork, only apply_filter post-fork |
| Restricted token too restrictive | Medium | Test on real Windows; Permissive only disables admin SID |
| Zombie suspended process on error | Medium | Explicit TerminateProcess in step 4 error path |

---

## 10. Out of Scope

- macOS resource limits (no pre-exec hook in sandbox-exec)
- Windows network isolation (requires WFP kernel driver)
- Windows filesystem isolation (Job Objects do not provide this)
- Windows UI isolation (restricted tokens do not prevent desktop access)
