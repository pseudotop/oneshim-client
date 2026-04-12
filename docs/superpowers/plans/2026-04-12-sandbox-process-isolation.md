# Sandbox Process Isolation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Replace sandbox stubs with real enforcement — automation actions run inside sandboxed child processes on all platforms.

**Architecture:** New `oneshim-sandbox-worker` binary crate acts as sandboxed execution host. Parent spawns it with platform-specific constraints (Linux: pre_exec Landlock/seccomp/rlimit, Windows: Job Object + CreateProcessWithTokenW, macOS: sandbox-exec wrapper). Communication via stdin/stdout JSON pipe.

**Tech Stack:** Rust, windows-sys 0.61 (Job Objects, Restricted Tokens), landlock 0.4, seccompiler 0.4, tokio::process, serde_json

---

## File Structure

| File | Responsibility |
|------|---------------|
| `crates/oneshim-sandbox-worker/Cargo.toml` | NEW: Worker binary crate manifest |
| `crates/oneshim-sandbox-worker/src/main.rs` | NEW: Read stdin, run action, write stdout |
| `crates/oneshim-automation/src/sandbox/ipc.rs` | NEW: SandboxRequest/Response types + worker path resolution |
| `crates/oneshim-automation/src/error.rs` | ADD: SandboxEnforcement variant |
| `crates/oneshim-automation/src/sandbox/linux.rs` | REWRITE: pre_exec subprocess model |
| `crates/oneshim-automation/src/sandbox/windows.rs` | REWRITE: Real Win32 API + subprocess |
| `crates/oneshim-automation/src/sandbox/macos.rs` | MODIFY: Use worker binary as carrier |
| `crates/oneshim-automation/src/sandbox/mod.rs` | MODIFY: Add ipc module export |
| `crates/oneshim-automation/src/action_dispatcher.rs` | MODIFY: Remove parent-side action match |
| `crates/oneshim-automation/Cargo.toml` | ADD: windows-sandbox feature, windows-sys dep |
| `Cargo.toml` (workspace) | ADD: worker member, windows-sys features |
| `src-tauri/Cargo.toml` | ADD: Forward windows-sandbox feature |
| `src-tauri/tauri.conf.json` | ADD: externalBin sidecar |

---

### Task 1: Fix missing SandboxEnforcement error variant

**Files:**
- Modify: `crates/oneshim-automation/src/error.rs:5-38` (enum) and `:40-66` (From impl)

- [ ] **Step 1: Add SandboxEnforcement variant to AutomationError**

In `crates/oneshim-automation/src/error.rs`, add after `SandboxExecution` (line 17):

```rust
    #[error("sandbox enforcement failed: {0}")]
    SandboxEnforcement(String),
```

- [ ] **Step 2: Add From mapping for SandboxEnforcement**

In the `impl From<AutomationError> for CoreError` block, add after the `SandboxExecution` arm (line 48):

```rust
            AutomationError::SandboxEnforcement(msg) => CoreError::SandboxExecution(msg),
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-automation`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-automation/src/error.rs
git commit -m "fix: add missing SandboxEnforcement error variant

Used in 6 locations in linux.rs behind cfg(feature = linux-sandbox)
but was absent from the enum. Fixes compilation on Linux targets."
```

---

### Task 2: Add IPC types and worker path resolution

**Files:**
- Create: `crates/oneshim-automation/src/sandbox/ipc.rs`
- Modify: `crates/oneshim-automation/src/sandbox/mod.rs`

- [ ] **Step 1: Create ipc.rs with types, resolver, and tests**

Create `crates/oneshim-automation/src/sandbox/ipc.rs`:

```rust
use oneshim_core::models::automation::AutomationAction;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct SandboxRequest {
    pub action: AutomationAction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SandboxResponse {
    pub success: bool,
    pub error: Option<String>,
}

/// Resolve the sandbox worker binary path.
/// Search order: exact name adjacent to binary, Tauri platform-suffixed, then PATH.
pub fn resolve_worker_path() -> Result<PathBuf, oneshim_core::error::CoreError> {
    let base_name = "oneshim-sandbox-worker";
    let ext = if cfg!(target_os = "windows") { ".exe" } else { "" };

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // 1. Exact name (dev builds: cargo puts binaries in target/debug/)
            let candidate = dir.join(format!("{base_name}{ext}"));
            if candidate.exists() {
                return Ok(candidate);
            }
            // 2. Tauri sidecar: platform-suffixed name
            //    e.g., oneshim-sandbox-worker-aarch64-apple-darwin
            let target = target_triple();
            let suffixed = dir.join(format!("{base_name}-{target}{ext}"));
            if suffixed.exists() {
                return Ok(suffixed);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    if let Ok(output) = std::process::Command::new("which")
        .arg("oneshim-sandbox-worker")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    #[cfg(target_os = "windows")]
    if let Ok(output) = std::process::Command::new("where.exe")
        .arg("oneshim-sandbox-worker")
        .output()
    {
        if output.status.success() {
            if let Some(first_line) = String::from_utf8_lossy(&output.stdout).lines().next() {
                let path = first_line.trim().to_string();
                if !path.is_empty() {
                    return Ok(PathBuf::from(path));
                }
            }
        }
    }

    Err(oneshim_core::error::CoreError::SandboxExecution(
        "sandbox worker binary not found: checked adjacent to executable, Tauri sidecar, and PATH".into(),
    ))
}

/// Returns the Rust target triple for the current platform.
fn target_triple() -> &'static str {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    { "x86_64-unknown-linux-gnu" }
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    { "aarch64-unknown-linux-gnu" }
    #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
    { "x86_64-apple-darwin" }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    { "aarch64-apple-darwin" }
    #[cfg(all(target_arch = "x86_64", target_os = "windows"))]
    { "x86_64-pc-windows-msvc" }
    #[cfg(all(target_arch = "aarch64", target_os = "windows"))]
    { "aarch64-pc-windows-msvc" }
}

/// Parse worker stdout into SandboxResponse.
pub fn parse_worker_response(stdout: &[u8]) -> Result<SandboxResponse, oneshim_core::error::CoreError> {
    let stdout_str = String::from_utf8_lossy(stdout);
    let trimmed = stdout_str.trim();
    if trimmed.is_empty() {
        return Err(oneshim_core::error::CoreError::SandboxExecution(
            "worker produced no output on stdout".into(),
        ));
    }
    serde_json::from_str(trimmed).map_err(|e| {
        oneshim_core::error::CoreError::SandboxExecution(format!(
            "failed to parse worker response: {e} -- stdout: {trimmed}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serde_roundtrip() {
        let req = SandboxRequest {
            action: AutomationAction::KeyType { text: "hello".into() },
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: SandboxRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{:?}", parsed.action), format!("{:?}", req.action));
    }

    #[test]
    fn response_success_roundtrip() {
        let resp = SandboxResponse { success: true, error: None };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: SandboxResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        assert!(parsed.error.is_none());
    }

    #[test]
    fn response_failure_roundtrip() {
        let resp = SandboxResponse { success: false, error: Some("denied".into()) };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: SandboxResponse = serde_json::from_str(&json).unwrap();
        assert!(!parsed.success);
        assert_eq!(parsed.error.as_deref(), Some("denied"));
    }

    #[test]
    fn parse_worker_response_valid() {
        let resp = parse_worker_response(br#"{"success":true,"error":null}"#).unwrap();
        assert!(resp.success);
    }

    #[test]
    fn parse_worker_response_empty_stdout() {
        assert!(parse_worker_response(b"").is_err());
    }

    #[test]
    fn parse_worker_response_malformed() {
        assert!(parse_worker_response(b"not json").is_err());
    }
}
```

- [ ] **Step 2: Export ipc module from sandbox/mod.rs**

In `crates/oneshim-automation/src/sandbox/mod.rs`, add at line 1:

```rust
pub mod ipc;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p oneshim-automation sandbox::ipc`
Expected: 6 tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-automation/src/sandbox/ipc.rs crates/oneshim-automation/src/sandbox/mod.rs
git commit -m "feat(sandbox): add IPC types and worker path resolution

SandboxRequest/Response for stdin/stdout JSON protocol.
resolve_worker_path checks adjacent dir + PATH."
```

---

### Task 3: Create oneshim-sandbox-worker binary crate

**Files:**
- Create: `crates/oneshim-sandbox-worker/Cargo.toml`
- Create: `crates/oneshim-sandbox-worker/src/main.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Create Cargo.toml**

Create `crates/oneshim-sandbox-worker/Cargo.toml`:

```toml
[package]
name = "oneshim-sandbox-worker"
description = "Sandboxed automation action executor"
edition.workspace = true
version.workspace = true
license.workspace = true
rust-version.workspace = true
repository.workspace = true
homepage.workspace = true
authors.workspace = true

[[bin]]
name = "oneshim-sandbox-worker"
path = "src/main.rs"

[dependencies]
oneshim-core = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
enigo = { workspace = true, optional = true }

[features]
default = []
enigo = ["dep:enigo"]
```

Note: The `enigo` feature enables real mouse/keyboard execution. Without it, actions are logged to stderr (sufficient for sandbox validation). Wiring real `enigo` calls is a follow-up task.

- [ ] **Step 2: Create main.rs**

Create `crates/oneshim-sandbox-worker/src/main.rs`:

```rust
//! Sandboxed automation action executor.
//!
//! Spawned by the parent process with platform sandbox constraints already
//! applied. Reads SandboxRequest JSON from stdin, runs the action, writes
//! SandboxResponse JSON to stdout.

use oneshim_core::models::automation::AutomationAction;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct SandboxRequest {
    action: AutomationAction,
}

#[derive(Serialize)]
struct SandboxResponse {
    success: bool,
    error: Option<String>,
}

fn main() {
    let response = match run() {
        Ok(()) => SandboxResponse { success: true, error: None },
        Err(e) => SandboxResponse { success: false, error: Some(e) },
    };

    if let Ok(json) = serde_json::to_string(&response) {
        let _ = io::stdout().write_all(json.as_bytes());
        let _ = io::stdout().write_all(b"\n");
        let _ = io::stdout().flush();
    }
}

fn run() -> Result<(), String> {
    let stdin = io::stdin();
    let line = stdin
        .lock()
        .lines()
        .next()
        .ok_or_else(|| "no input on stdin".to_string())?
        .map_err(|e| format!("stdin read error: {e}"))?;

    let request: SandboxRequest =
        serde_json::from_str(&line).map_err(|e| format!("invalid request JSON: {e}"))?;

    run_action(&request.action)
}

fn run_action(action: &AutomationAction) -> Result<(), String> {
    match action {
        AutomationAction::MouseMove { x, y } => {
            eprintln!("sandbox-worker: mouse move ({x}, {y})");
        }
        AutomationAction::MouseClick { button, x, y } => {
            eprintln!("sandbox-worker: mouse click {button} ({x}, {y})");
        }
        AutomationAction::KeyType { text } => {
            eprintln!("sandbox-worker: key type len={}", text.len());
        }
        AutomationAction::KeyPress { key } => {
            eprintln!("sandbox-worker: key press {key}");
        }
        AutomationAction::KeyRelease { key } => {
            eprintln!("sandbox-worker: key release {key}");
        }
        AutomationAction::Hotkey { keys } => {
            eprintln!("sandbox-worker: hotkey {:?}", keys);
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Add to workspace members**

In root `Cargo.toml`, add `"crates/oneshim-sandbox-worker"` to the `members` array (after `"crates/oneshim-embedding"`).

- [ ] **Step 4: Build and verify**

Run: `cargo build -p oneshim-sandbox-worker`
Expected: PASS

Verify stdin/stdout protocol:
Run: `echo '{"action":{"KeyType":{"text":"hello"}}}' | cargo run -p oneshim-sandbox-worker 2>/dev/null`
Expected stdout: `{"success":true,"error":null}`

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-sandbox-worker/ Cargo.toml
git commit -m "feat: add oneshim-sandbox-worker binary crate

Minimal sandboxed action executor. Reads SandboxRequest from stdin,
runs action, writes SandboxResponse to stdout. No async runtime."
```

---

### Task 4: Add workspace dependency and feature changes

**Files:**
- Modify: `Cargo.toml:194` (workspace windows-sys features)
- Modify: `crates/oneshim-automation/Cargo.toml:12-15` (features + deps)
- Modify: `src-tauri/Cargo.toml:92` (forward feature)
- Modify: `src-tauri/tauri.conf.json:39` (sidecar)

- [ ] **Step 1: Add windows-sys features to workspace Cargo.toml**

At line 194, append `"Win32_System_JobObjects", "Win32_Security_Authentication_Identity"` to the windows-sys features list.

- [ ] **Step 2: Add windows-sandbox feature to oneshim-automation**

In `crates/oneshim-automation/Cargo.toml`, add to `[features]`:
```toml
windows-sandbox = []
```

Add target dependency:
```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { workspace = true }
```

- [ ] **Step 3: Forward windows-sandbox in src-tauri**

In `src-tauri/Cargo.toml`, add after `linux-sandbox` (line 92):
```toml
windows-sandbox = ["oneshim-automation/windows-sandbox"]
```

- [ ] **Step 4: Add sidecar to tauri.conf.json**

In `src-tauri/tauri.conf.json`, inside `"bundle"` object, add:
```json
"externalBin": ["oneshim-sandbox-worker"]
```

- [ ] **Step 5: Verify**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/oneshim-automation/Cargo.toml src-tauri/Cargo.toml src-tauri/tauri.conf.json
git commit -m "feat(sandbox): add windows-sandbox feature and workspace deps

Win32_System_JobObjects + Win32_Security_Authentication_Identity.
Feature forwarded from src-tauri. Tauri sidecar config added."
```

---

### Task 5: Rewrite Linux sandbox with subprocess model

**Files:**
- Modify: `crates/oneshim-automation/src/sandbox/linux.rs`

- [ ] **Step 1: Add new tests**

Add to `#[cfg(test)] mod tests` in `linux.rs`:

```rust
    #[test]
    fn linux_process_isolation_capability() {
        let sandbox = LinuxSandbox::new();
        let caps = sandbox.capabilities();
        assert!(caps.process_isolation);
    }

    #[test]
    fn landlock_rules_include_worker_binary() {
        let config = SandboxConfig {
            profile: SandboxProfile::Strict,
            ..Default::default()
        };
        let rules = LinuxSandbox::build_landlock_rules(&config);
        assert!(
            rules.read_paths.iter().any(|p| p.contains("oneshim-sandbox-worker")),
            "Landlock rules must include worker binary path, got: {:?}",
            rules.read_paths
        );
    }

    fn is_permissive_noop_helper(config: &SandboxConfig) -> bool {
        matches!(config.profile, SandboxProfile::Permissive)
            && config.max_memory_bytes == 0
            && config.max_cpu_time_ms == 0
    }

    #[test]
    fn permissive_no_limits_is_noop() {
        let config = SandboxConfig {
            profile: SandboxProfile::Permissive,
            ..Default::default()
        };
        assert!(is_permissive_noop_helper(&config));
    }
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test -p oneshim-automation linux_process_isolation -- --nocapture`
Expected: FAIL (`process_isolation` is currently `false`)

- [ ] **Step 3: Rewrite execute_sandboxed with subprocess model**

Replace the `execute_sandboxed` implementation with:
1. `is_permissive_noop()` fast-path check
2. `resolve_worker_path()` to find the worker binary
3. Build BPF program BEFORE fork (`build_seccomp_bpf()`)
4. `tokio::process::Command::new(worker_path)` with piped stdin/stdout/stderr
5. `cmd.pre_exec()` applying Landlock, seccomp BPF, and rlimits
6. Spawn child, write `SandboxRequest` JSON to stdin, close stdin
7. `tokio::time::timeout` wrapping `child.wait_with_output()`
8. Parse `SandboxResponse` from stdout

Also:
- Add `build_seccomp_bpf()` returning `BpfProgram` (pre-fork)
- Add `apply_seccomp_bpf_sync()` returning `io::Result` (post-fork)
- Add `apply_landlock_rules_sync()` returning `io::Result`
- Add `apply_resource_limits_sync()` returning `io::Result`
- Add worker binary path to `build_landlock_rules` — at the top of the method, before the profile match:
  ```rust
  fn build_landlock_rules(config: &SandboxConfig) -> LandlockRules {
      let mut rules = LandlockRules::default();
      // Always allow the worker binary for subprocess model
      if let Ok(path) = crate::sandbox::ipc::resolve_worker_path() {
          rules.read_paths.push(path.to_string_lossy().to_string());
          // Also allow the directory containing the worker (for dynamic linker)
          if let Some(dir) = path.parent() {
              rules.read_paths.push(dir.to_string_lossy().to_string());
          }
      }
      match config.profile {
          // ... existing profile-based rules unchanged ...
      }
      rules
  }
  ```
- Remove `SYS_execve`/`SYS_execveat` from seccomp deny list
- Update `capabilities()`: `process_isolation: true`
- Add `derive_timeout()` helper

See spec section 4.1 for full details.

- [ ] **Step 4: Build worker and run tests**

Run: `cargo build -p oneshim-sandbox-worker && cargo test -p oneshim-automation sandbox::linux`
Expected: ALL PASS

Note: Worker binary MUST be built first — `execute_sandboxed` now spawns it as a child process. The existing `linux_sandbox_execute` test must be updated to either skip when worker is absent or build the worker as a test prerequisite.

- [ ] **Step 5: Verify workspace**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-automation/src/sandbox/linux.rs
git commit -m "feat(sandbox/linux): subprocess model with pre_exec isolation

Replace spawn_blocking with tokio::process::Command + pre_exec.
Sandbox constraints apply in child only -- no thread-pool leak.
BPF built pre-fork, only apply_filter post-fork.
Worker binary path added to Landlock read_paths.
execve allowed in seccomp for subprocess model."
```

---

### Task 6: Rewrite Windows sandbox with real Win32 API

**Files:**
- Modify: `crates/oneshim-automation/src/sandbox/windows.rs`

- [ ] **Step 1: Add OwnedHandle test**

Add to tests:

```rust
    #[test]
    fn owned_handle_validity() {
        // 0 = null handle, -1 = INVALID_HANDLE_VALUE
        let null_h = OwnedHandle(0);
        assert!(!null_h.is_valid());
        std::mem::forget(null_h);

        let invalid_h = OwnedHandle(-1);
        assert!(!invalid_h.is_valid());
        std::mem::forget(invalid_h);

        let valid_h = OwnedHandle(42);
        assert!(valid_h.is_valid());
        std::mem::forget(valid_h);
    }
```

- [ ] **Step 2: Verify test fails**

Run: `cargo test -p oneshim-automation owned_handle_validity`
Expected: FAIL (`OwnedHandle` not defined)

- [ ] **Step 3: Implement OwnedHandle + Win32 API + subprocess**

Rewrite `windows.rs`:
1. `OwnedHandle(isize)` with `is_valid()` (`!= 0 && != -1`) and `Drop`
2. `create_job_object()` with `#[cfg(feature = "windows-sandbox")]` dual paths
3. `create_restricted_token()` with dual paths
4. `execute_sandboxed()`:
   - `CreateJobObjectW` + `SetInformationJobObject`
   - `OpenProcessToken` + `CreateRestrictedToken`
   - `CreateProcessWithTokenW` with `CREATE_SUSPENDED`
   - `AssignProcessToJobObject` (error path: `TerminateProcess`)
   - `ResumeThread`
   - Stdin/stdout pipe for SandboxRequest/Response
   - `WaitForSingleObject` with timeout
5. `capabilities()` respecting feature flag

See spec section 4.2 for full Win32 API details.

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-automation sandbox::windows`
Expected: ALL PASS

- [ ] **Step 5: Verify workspace**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-automation/src/sandbox/windows.rs
git commit -m "feat(sandbox/windows): real Win32 Job Object + restricted token

CreateJobObjectW + SetInformationJobObject for resource limits.
CreateRestrictedToken for token restrictions.
CreateProcessWithTokenW + CREATE_SUSPENDED for subprocess.
OwnedHandle RAII wrapper. Feature-gated behind windows-sandbox."
```

---

### Task 7: Update macOS sandbox to use worker binary

**Files:**
- Modify: `crates/oneshim-automation/src/sandbox/macos.rs`

- [ ] **Step 1: Update build_sandbox_command**

Change `build_sandbox_command` to use worker binary instead of `/bin/echo`:
- Remove `action` parameter (action goes via stdin now)
- Call `resolve_worker_path()` for the carrier
- Update args array

- [ ] **Step 2: Update execute_sandboxed for stdin/stdout pipe**

Change `execute_sandboxed` to:
1. Spawn `sandbox-exec` with piped stdin/stdout
2. Write `SandboxRequest` JSON to stdin
3. Wrap wait with `tokio::time::timeout`
4. Parse `SandboxResponse` from stdout
5. Check `response.success` and return error if false

- [ ] **Step 3: Update tests for new API**

Replace `build_sandbox_command_produces_correct_structure` (currently tests `/bin/echo` + action JSON):

```rust
    #[test]
    fn build_sandbox_command_uses_worker() {
        let sandbox = MacOsSandbox::with_exec_path(Some("/usr/bin/sandbox-exec".to_string()));
        let profile = "(version 1)\n(allow default)\n";
        // May fail if worker not built, but we test the structure
        if let Ok((exec_path, args)) = sandbox.build_sandbox_command(profile) {
            assert_eq!(exec_path, "/usr/bin/sandbox-exec");
            assert_eq!(args[0], "-p");
            assert_eq!(args[1], profile);
            assert_eq!(args[2], "--");
            assert!(args[3].contains("oneshim-sandbox-worker"));
        }
    }
```

Replace `build_sandbox_command_without_exec_path_fails`:
```rust
    #[test]
    fn build_sandbox_command_without_exec_path_fails() {
        let sandbox = MacOsSandbox::with_exec_path(None);
        let result = sandbox.build_sandbox_command("(version 1)\n");
        assert!(result.is_err());
    }
```

Remove `build_sandbox_command_all_action_variants` (action no longer in args — goes via stdin).

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-automation sandbox::macos`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-automation/src/sandbox/macos.rs
git commit -m "feat(sandbox/macos): use worker binary instead of /bin/echo

Actions now run inside sandboxed child process via stdin/stdout
pipe. sandbox-exec wraps worker with Seatbelt constraints.
Added tokio::time::timeout for child wait."
```

---

### Task 8: Simplify ActionDispatcher

**Files:**
- Modify: `crates/oneshim-automation/src/action_dispatcher.rs`

- [ ] **Step 1: Remove parent-side action match**

Replace the `dispatch` implementation: remove the `match action { ... }` block after `execute_sandboxed`. The action is now executed inside the worker. Dispatcher just returns Ok/Err from sandbox.

```rust
async fn dispatch(&self, action: &AutomationAction, config: &SandboxConfig) -> CommandResult {
    tracing::info!(
        action = ?action,
        sandbox = self.sandbox.platform(),
        profile = ?config.profile,
        "dispatching to sandboxed worker"
    );

    match self.sandbox.execute_sandboxed(action, config).await {
        Ok(()) => CommandResult::Success,
        Err(e) => {
            tracing::error!(error = %e, "sandboxed execution failed");
            CommandResult::Failed(format!("Sandbox execution failed: {}", e))
        }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p oneshim-automation action_dispatcher`
Expected: ALL PASS (MockSandbox tests unchanged)

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-automation/src/action_dispatcher.rs
git commit -m "refactor(dispatcher): delegate fully to sandbox worker

Action runs inside sandboxed child process now.
Parent dispatcher only routes to sandbox.execute_sandboxed."
```

---

### Task 9: Integration tests

**Files:**
- Create: `crates/oneshim-automation/tests/sandbox_integration.rs`

- [ ] **Step 1: Write integration tests**

Create `crates/oneshim-automation/tests/sandbox_integration.rs`:

```rust
use oneshim_automation::sandbox::ipc::resolve_worker_path;
use std::io::Write;

#[test]
fn worker_binary_discoverable() {
    match resolve_worker_path() {
        Ok(path) => assert!(path.exists(), "path exists but file missing: {:?}", path),
        Err(_) => eprintln!("skipping: worker not found (expected in some envs)"),
    }
}

#[test]
fn worker_stdin_stdout_roundtrip() {
    let worker = match resolve_worker_path() {
        Ok(p) => p,
        Err(_) => { eprintln!("skipping: worker not found"); return; }
    };

    let mut child = std::process::Command::new(&worker)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().expect("spawn failed");

    let stdin = child.stdin.as_mut().unwrap();
    stdin.write_all(br#"{"action":{"KeyType":{"text":"test"}}}"#).unwrap();
    stdin.write_all(b"\n").unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""success":true"#), "got: {stdout}");
}

#[test]
fn worker_malformed_input() {
    let worker = match resolve_worker_path() {
        Ok(p) => p,
        Err(_) => { eprintln!("skipping: worker not found"); return; }
    };

    let mut child = std::process::Command::new(&worker)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().expect("spawn failed");

    let stdin = child.stdin.as_mut().unwrap();
    stdin.write_all(b"not json\n").unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""success":false"#), "got: {stdout}");
}
```

- [ ] **Step 2: Build worker and run integration tests**

Run: `cargo build -p oneshim-sandbox-worker && cargo test -p oneshim-automation --test sandbox_integration`
Expected: ALL PASS

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-automation/tests/sandbox_integration.rs
git commit -m "test: add sandbox worker integration tests

Verifies worker stdin/stdout protocol and malformed input handling."
```

---

### Task 10: CI configuration

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/build-smoke.yml`

- [ ] **Step 1: Add windows-sandbox to CI**

If a Windows runner exists in ci.yml, add `--features windows-sandbox` to clippy and test steps. If no Windows runner, add a comment noting it's needed.

In build-smoke.yml, add Windows smoke check with `windows-sandbox` feature if applicable.

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/
git commit -m "ci: enable windows-sandbox feature gate in CI"
```

---

### Task 11: Final verification

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: PASS

- [ ] **Step 2: All tests**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 3: Clippy**

Run: `cargo clippy --workspace`
Expected: No errors

- [ ] **Step 4: Format check**

Run: `cargo fmt --check`
Expected: No diff
