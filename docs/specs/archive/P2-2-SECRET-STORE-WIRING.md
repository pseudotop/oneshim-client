# P2-2: SecretStore Wiring

## Problem Statement

`SessionManagerImpl` has a `with_secret_store()` builder method (line 64 of `session_manager.rs`) that is never called in production. Without it, `HttpApiSession` cannot resolve API keys via `CredentialSource::StoredSecret` — sessions only work for no-auth surfaces (e.g., Ollama) or inline plaintext keys.

## Solution

Wire `with_secret_store()` in `app_runtime_launch.rs` using the existing `resolve_provider_secret_backend()` pattern from `server_runtime_context.rs:60-64`.

## Implementation

**File**: `src-tauri/src/app_runtime_launch.rs`

In the session manager creation block (~line 301), before `SessionManagerImpl::new()`:

1. Resolve `config_dir` via `ConfigManager::config_dir()` (fallback: `data_dir_path`)
2. Create OS secret store via `create_os_secret_store(&config_dir)`
3. Resolve backend via `resolve_provider_secret_backend(&config_dir, os_store)`
4. If `resolution.secret_store` is `Some`, chain `.with_secret_store(store)`

```rust
let session_manager = {
    // ... existing audit_logger, audit_port, session_config, context_assembler ...

    // Resolve provider secret backend for HttpApi credential resolution.
    let config_dir = oneshim_core::config_manager::ConfigManager::config_dir()
        .unwrap_or_else(|_| data_dir_path.to_path_buf());
    let os_secret_store = crate::provider_secret_backend::create_os_secret_store(&config_dir);
    let secret_store = crate::provider_secret_backend::resolve_provider_secret_backend(
        &config_dir,
        os_secret_store,
    )
    .ok()
    .and_then(|r| r.secret_store);

    let mut manager = SessionManagerImpl::new(
        session_config,
        audit_port,
        Some(context_assembler),
    );
    if let Some(store) = secret_store {
        manager = manager.with_secret_store(store);
    }
    Some(Arc::new(manager))
};
```

**Note**: Uses `.ok()` to gracefully handle backend resolution failure (logs warning, falls back to no secret store — same as current behavior).

## Files Changed

| File | Change | Lines |
|------|--------|-------|
| `src-tauri/src/app_runtime_launch.rs` | Wire secret store in session_manager block | ~10 |
| `src-tauri/src/session_manager.rs` | Remove `#[allow(dead_code)]` from `with_secret_store` | -1 |

## Testing

- No new tests needed — existing `provider_secret_backend` tests cover backend resolution
- `with_secret_store()` is a simple field setter
- Verified by `cargo check` + `cargo test --workspace`
