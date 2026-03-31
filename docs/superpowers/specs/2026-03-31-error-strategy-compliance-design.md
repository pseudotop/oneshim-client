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
            NetworkError::Http(msg) => CoreError::Network(msg),
            NetworkError::Timeout { timeout_ms } => CoreError::RequestTimeout { timeout_ms },
            // ...
        }
    }
}
```

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
| 1 | oneshim-storage | `StorageError` | `rusqlite::Error`, `std::io::Error` |
| 2 | oneshim-analysis | `AnalysisError` | — |
| 3 | oneshim-network | `NetworkError` | `reqwest::Error`, `std::io::Error` |
| 4 | oneshim-automation | `AutomationError` | `std::io::Error` |
| 5 | oneshim-monitor | `MonitorError` | — |
| 6 | oneshim-embedding | `EmbeddingError` | — |
| 7 | oneshim-suggestion | `SuggestionError` | — |
| 8 | oneshim-vision | `VisionError` | (absorbs existing internal `OcrError`) |

Exact variants per crate are determined during implementation by analyzing actual `.map_err()` and `CoreError::Xxx` usage.

### Per-Crate Work

For each crate:

1. Create `src/error.rs` with crate-specific `thiserror` enum
2. Add `impl From<CrateError> for CoreError` mapping
3. Update internal (non-port) functions: `CoreError` → `CrateError`
4. Port trait impls stay `Result<T, CoreError>` — `?` auto-converts via `From`
5. Export error type from `lib.rs`
6. Verify `cargo check -p <crate>` and `cargo test -p <crate>`

## What Does NOT Change

- Port trait signatures in `oneshim-core` (stay `CoreError`)
- `src-tauri` binary crate (already `anyhow`)
- `oneshim-web` (already has `ApiError`)
- `oneshim-api-contracts` (pure DTOs, no errors)
- `CoreError` itself (remains the port-level error lingua franca)

## Execution

- **Single PR** — mechanically related changes, same pattern across all 8 crates
- **Order**: storage → analysis → network → automation → monitor → embedding → suggestion → vision (by ref count)
- **Validation**: `cargo check --workspace && cargo test --workspace && cargo clippy --workspace` must pass after all changes

## Success Criteria

- All 8 library crates define their own `thiserror` error enum
- No internal function returns `CoreError` directly (only port trait impls)
- `From<CrateError> for CoreError` implemented for all 8 types
- All existing tests pass
- No new `#[allow(dead_code)]` on error variants
