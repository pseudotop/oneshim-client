# Error Strategy ADR-001 §1 Compliance

**Date**: 2026-03-31
**Status**: Approved
**Scope**: 8 library crates — single PR

## Problem

ADR-001 §1 specifies: "Library crates: `thiserror` — specific error enums."

Currently 8 of 12 library crates return `Result<T, CoreError>` directly, violating this rule. Only `oneshim-web` (ApiError) and `oneshim-core` (CoreError) are compliant.

| Crate | CoreError refs | Domain |
|-------|---------------|--------|
| oneshim-storage | 216 | SQLite, keychain, crypto, file I/O |
| oneshim-analysis | 156 | LLM analysis, vector search, coaching |
| oneshim-network | 152 | HTTP, SSE, gRPC, auth, sync |
| oneshim-automation | 53 | sandbox, GUI interaction, policy |
| oneshim-monitor | 28 | sysinfo, platform-specific window detection |
| oneshim-embedding | 19 | fastembed vector embedding |
| oneshim-suggestion | 17 | SSE reception, priority queue, feedback |
| oneshim-vision | ~20 | capture, delta, OCR, privacy |

## Solution

Each library crate gets its own `thiserror` error enum in `error.rs`.

### Conversion Pattern

Port traits in `oneshim-core` return `Result<T, CoreError>`. This does NOT change. Adapter crates convert at the port boundary:

```rust
// oneshim-network/src/error.rs
#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("Connection timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    // ...
}

// Orphan rule allows this — NetworkError is local
impl From<NetworkError> for CoreError {
    fn from(err: NetworkError) -> Self {
        match err {
            NetworkError::Core(e) => e,  // unwrap identity
            NetworkError::Http(msg) => CoreError::Network(msg),
            NetworkError::Timeout { timeout_ms } => CoreError::RequestTimeout { timeout_ms },
            // ... (exhaustive — no catch-all arm)
        }
    }
}
```

**Exhaustive match required**: `From<CrateError> for CoreError` must NOT use a catch-all `_ =>` arm. Since `CrateError` is local, adding a new variant should produce a compiler error forcing the mapping to be updated. (This differs from `From<CoreError> for ApiError` in oneshim-web, which correctly uses a catch-all because CoreError is foreign.)

### Bidirectional Conversion via Core Variant

Adapter crates hold port trait references (`Arc<dyn PortTrait>`) and call their methods, which return `CoreError`. When an internal function returns `CrateError`, the `?` operator on port calls needs `From<CoreError> for CrateError`.

**Solution**: Every crate error enum includes a `Core(CoreError)` variant with `#[from]`:

```rust
#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Core(#[from] CoreError),    // wraps port trait errors via ?

    #[error("embedding failed: {0}")]
    Embedding(String),          // crate-specific failure
    // ...
}
```

This enables:

```rust
// Internal function returns AnalysisError
pub async fn process_activities(&self) -> Result<usize, AnalysisError> {
    // Port call returns CoreError — ? wraps into AnalysisError::Core
    let vectors = self.embedding_provider.embed_batch(&texts).await?;
    Ok(vectors.len())
}

// Port trait impl returns CoreError — ? unwraps AnalysisError::Core back
impl SomePort for AnalysisService {
    async fn analyze(&self) -> Result<(), CoreError> {
        self.process_activities().await?;  // AnalysisError → CoreError via From
        Ok(())
    }
}
```

The `Core` variant round-trips cleanly: `CoreError → AnalysisError::Core(e) → CoreError` (identity unwrap in the `From<AnalysisError> for CoreError` match arm).

- **Internal functions** return `Result<T, CrateError>` (domain-specific)
- **Port trait impls** return `Result<T, CoreError>` (unchanged), `?` on CrateError works via `From`
- **No circular deps** — `oneshim-core` never depends on adapter crates

### Variant Design Principle

Variants mirror actual failure modes of the crate, not CoreError's variant list.

```rust
// Good: reflects what actually fails
pub enum StorageError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Schema migration failed at v{version}: {reason}")]
    Migration { version: u32, reason: String },
}

// Bad: thin mirror of CoreError
pub enum StorageError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Internal error: {0}")]
    Internal(String),
}
```

External crate errors use `#[from]` for automatic wrapping where a 1:1 mapping exists (e.g., `rusqlite::Error`, `reqwest::Error`).

### Per-Crate Error Types

| # | Crate | Error Type | `#[from]` candidates |
|---|-------|-----------|---------------------|
| 1 | oneshim-storage | `StorageError` | `CoreError`, `rusqlite::Error`, `std::io::Error` |
| 2 | oneshim-analysis | `AnalysisError` | `CoreError` |
| 3 | oneshim-network | `NetworkError` | `CoreError`, `reqwest::Error`, `std::io::Error` |
| 4 | oneshim-automation | `AutomationError` | `CoreError`, `std::io::Error` |
| 5 | oneshim-monitor | `MonitorError` | `CoreError` |
| 6 | oneshim-embedding | `EmbeddingError` | `CoreError` |
| 7 | oneshim-suggestion | `SuggestionError` | `CoreError` |
| 8 | oneshim-vision | `VisionError` | `CoreError` (absorbs existing internal `OcrError`) |

All crate errors include `Core(#[from] CoreError)` for port trait call propagation.

Exact variants per crate are determined during implementation by analyzing actual `.map_err()` and `CoreError::Xxx` usage.

### Per-Crate Work

For each crate:

1. Create `src/error.rs` with crate-specific `thiserror` enum
2. Add `impl From<CrateError> for CoreError` mapping
3. Update internal (non-port) functions: `CoreError` → `CrateError`
4. Port trait impls stay `Result<T, CoreError>` — `?` auto-converts via `From`
5. Export error type from `lib.rs`
6. Verify `cargo check -p <crate>` and `cargo test -p <crate>`

### Constructor & Builder Functions

Constructors (e.g., `HttpApiClient::new()`, `GrpcSessionClient::connect()`) are public non-port functions called by `src-tauri`. These SHOULD return `CrateError`, not `CoreError`:

- `src-tauri` uses `anyhow::Result`, so `?` on any `thiserror` type auto-converts via `std::error::Error` trait
- Keeping constructors on `CoreError` while internals use `CrateError` creates inconsistency
- If a constructor fails, the error is crate-specific (e.g., TLS config, connection refused)

```rust
// src-tauri/src/main.rs (before)
let client = HttpApiClient::new(&url, token_mgr, timeout)?; // CoreError → anyhow

// src-tauri/src/main.rs (after — same callsite, no change needed)
let client = HttpApiClient::new(&url, token_mgr, timeout)?; // NetworkError → anyhow
```

### Test Migration Strategy

15+ tests across crates match on specific `CoreError` variants (e.g., `matches!(err, CoreError::PolicyDenied(_))`). These must be updated:

**Rule**: Tests for internal functions should assert on `CrateError` variants. Tests for port trait impls continue to assert on `CoreError` variants (since ports return `CoreError`).

```rust
// Before: test for internal function
assert!(matches!(result.unwrap_err(), CoreError::PolicyDenied(_)));

// After: test asserts on crate-specific error
assert!(matches!(result.unwrap_err(), AutomationError::PolicyDenied(_)));
```

Tests that validate port trait behavior (e.g., gRPC error mapping) stay on `CoreError` — they test the `From<CrateError> for CoreError` conversion boundary.

### Information Loss at Conversion Boundary

Some CoreError variants are `String`-only (e.g., `Network(String)`, `Storage(String)`). Crate-specific errors may have structured fields (e.g., `Http { status: u16, message: String }`). The `From` conversion flattens these into strings:

```rust
NetworkError::Http { status, message } => CoreError::Network(format!("{status}: {message}"))
```

This is acceptable — port-level consumers (src-tauri, oneshim-web) need error category, not internal detail. The structured detail is available in logs via `tracing::error!` before conversion.

## What Does NOT Change

- Port trait signatures in `oneshim-core` (stay `CoreError`)
- `src-tauri` binary crate (already `anyhow`)
- `oneshim-web` (already has `ApiError`)
- `oneshim-api-contracts` (pure DTOs, no errors)
- `CoreError` itself (remains the port-level error lingua franca)
- `GuiInteractionError` in `oneshim-core` — already compliant (defined in core, re-exported by automation as a core type, not an adapter error)

## Execution

- **Single PR** — mechanically related changes, same pattern across all 8 crates
- **Order**: storage → analysis → network → automation → monitor → embedding → suggestion → vision (by ref count)
- **Validation**: `cargo check --workspace && cargo test --workspace && cargo clippy --workspace` must pass after all changes

## Success Criteria

- All 8 library crates define their own `thiserror` error enum with `Core(#[from] CoreError)` variant
- No internal function returns `CoreError` directly (only port trait impls)
- `From<CrateError> for CoreError` implemented for all 8 types (exhaustive match, no catch-all)
- All existing tests pass
- No new `#[allow(dead_code)]` on error variants
