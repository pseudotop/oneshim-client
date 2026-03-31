# Error Strategy ADR-001 §1 Compliance — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give all 8 non-compliant library crates their own `thiserror` error enums with bidirectional `CoreError` conversion.

**Architecture:** Each crate gets `error.rs` with a `Core(#[from] CoreError)` variant for port call propagation and domain-specific variants. `From<CrateError> for CoreError` uses exhaustive match (no catch-all). Internal functions return `CrateError`; port trait impls stay `CoreError`.

**Tech Stack:** `thiserror` 2.x (already in workspace dependencies)

**Spec:** `docs/superpowers/specs/2026-03-31-error-strategy-compliance-design.md`

---

## File Map

Each crate gets one new file and modifications to existing files:

| Crate | Create | Modify | Tests to update |
|-------|--------|--------|-----------------|
| oneshim-storage | `src/error.rs` | `src/lib.rs` + 27 source files | 4 assertion patterns |
| oneshim-analysis | `src/error.rs` | `src/lib.rs` + 5 source files | 3 test mocks |
| oneshim-network | `src/error.rs` | `src/lib.rs` + 30+ source files | 10 assertion patterns |
| oneshim-automation | `src/error.rs` | `src/lib.rs` + 19 source files | 6 assertion patterns |
| oneshim-monitor | `src/error.rs` | `src/lib.rs` + 16 source files | 0 assertions |
| oneshim-embedding | `src/error.rs` | `src/lib.rs` + 1 source file | 0 assertions |
| oneshim-suggestion | `src/error.rs` | `src/lib.rs` + 5 source files | 0 assertions |
| oneshim-vision | `src/error.rs` | `src/lib.rs` + 24 source files | 1 assertion pattern |

---

### Task 1: oneshim-storage — StorageError

**Files:**
- Create: `crates/oneshim-storage/src/error.rs`
- Modify: `crates/oneshim-storage/src/lib.rs`
- Modify: All 27 source files in `crates/oneshim-storage/src/`
- Tests: `integration_state_store/tests.rs`, `temp_file_projection.rs`, `env_secret_store.rs`, `process_env_projection.rs`

- [ ] **Step 1: Create `error.rs` with StorageError enum**

```rust
// crates/oneshim-storage/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("secret store error: {0}")]
    SecretStore(String),

    #[error("{resource_type} not found: {id}")]
    NotFound { resource_type: String, id: String },

    #[error("encryption error: {0}")]
    Encryption(String),

    #[error("validation failed — {field}: {message}")]
    Validation { field: String, message: String },

    #[error("configuration error: {0}")]
    Config(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<StorageError> for CoreError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::Core(e) => e,
            StorageError::Sqlite(e) => CoreError::Storage(e.to_string()),
            StorageError::Io(e) => CoreError::Io(e),
            StorageError::SecretStore(msg) => CoreError::SecretStoreError(msg),
            StorageError::NotFound { resource_type, id } => {
                CoreError::NotFound { resource_type, id }
            }
            StorageError::Encryption(msg) => CoreError::Internal(msg),
            StorageError::Validation { field, message } => {
                CoreError::Validation { field, message }
            }
            StorageError::Config(msg) => CoreError::Config(msg),
            StorageError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/oneshim-storage/src/lib.rs`:
```rust
pub mod error;
pub use error::StorageError;
```

- [ ] **Step 3: Verify the new module compiles**

Run: `cargo check -p oneshim-storage`
Expected: PASS (no existing code changed yet)

- [ ] **Step 4: Migrate internal functions from CoreError to StorageError**

Systematic replacement across all 27 source files. The pattern:

```rust
// BEFORE
use oneshim_core::error::CoreError;

pub fn some_fn() -> Result<T, CoreError> {
    do_thing().map_err(|e| CoreError::Internal(format!("failed: {e}")))?;
    Ok(result)
}

// AFTER
use crate::error::StorageError;

pub fn some_fn() -> Result<T, StorageError> {
    do_thing().map_err(|e| StorageError::Internal(format!("failed: {e}")))?;
    Ok(result)
}
```

For functions implementing port traits, keep `Result<T, CoreError>` — the `?` on `StorageError` auto-converts via `From`.

Key mappings for this crate:
- `CoreError::Internal(msg)` → `StorageError::Internal(msg)`
- `CoreError::SecretStoreError(msg)` → `StorageError::SecretStore(msg)`
- `CoreError::NotFound { .. }` → `StorageError::NotFound { .. }`
- `CoreError::Config(msg)` → `StorageError::Config(msg)`
- `CoreError::Validation { .. }` → `StorageError::Validation { .. }`
- `CoreError::Storage(msg)` → `StorageError::Internal(msg)`
- `CoreError::Auth(msg)` → `StorageError::Config(msg)` (used for missing auth config)
- `CoreError::InvalidArguments(msg)` → `StorageError::Config(msg)` (used for invalid config args)

Where `rusqlite::Error` is currently wrapped via `CoreError::Internal(e.to_string())`, replace with `StorageError::Sqlite` using `?` directly (since `#[from]` handles it).

Where `std::io::Error` is currently wrapped via `CoreError::Internal(e.to_string())` or `CoreError::Io`, replace with `StorageError::Io` using `?` directly.

- [ ] **Step 5: Update test assertions**

```rust
// integration_state_store/tests.rs
// BEFORE
assert!(matches!(err, CoreError::Validation { .. }));
// AFTER
assert!(matches!(err, StorageError::Validation { .. }));

// temp_file_projection.rs, process_env_projection.rs
// BEFORE
assert!(matches!(err, CoreError::Config(_)));
// AFTER
assert!(matches!(err, StorageError::Config(_)));

// env_secret_store.rs
// BEFORE
assert!(matches!(err, CoreError::SecretStoreError(_)));
// AFTER
assert!(matches!(err, StorageError::SecretStore(_)));
```

- [ ] **Step 6: Verify storage crate**

Run: `cargo check -p oneshim-storage && cargo test -p oneshim-storage && cargo clippy -p oneshim-storage`
Expected: All PASS

- [ ] **Step 7: Verify workspace still compiles**

Run: `cargo check --workspace`
Expected: PASS (src-tauri calls StorageError constructors via `?` into `anyhow::Result`)

- [ ] **Step 8: Commit**

```bash
git add crates/oneshim-storage/
git commit -m "refactor(storage): introduce StorageError per ADR-001 §1"
```

---

### Task 2: oneshim-analysis — AnalysisError

**Files:**
- Create: `crates/oneshim-analysis/src/error.rs`
- Modify: `crates/oneshim-analysis/src/lib.rs`
- Modify: `hnsw_adapter.rs`, `gmm_detector.rs`, `hdbscan_detector.rs`, `adaptive_search.rs`, `daily_insight_generator.rs`, `llm_segment_summarizer.rs`
- Tests: inline mocks in `adaptive_search.rs`, `daily_insight_generator.rs`, `llm_segment_summarizer.rs`

- [ ] **Step 1: Create `error.rs` with AnalysisError enum**

```rust
// crates/oneshim-analysis/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("vector index error: {0}")]
    VectorIndex(String),

    #[error("clustering failed: {0}")]
    Clustering(String),

    #[error("LLM service error: {0}")]
    LlmService(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<AnalysisError> for CoreError {
    fn from(err: AnalysisError) -> Self {
        match err {
            AnalysisError::Core(e) => e,
            AnalysisError::VectorIndex(msg) => CoreError::Internal(msg),
            AnalysisError::Clustering(msg) => CoreError::Analysis(msg),
            AnalysisError::LlmService(msg) => CoreError::Analysis(msg),
            AnalysisError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/oneshim-analysis/src/lib.rs`:
```rust
pub mod error;
pub use error::AnalysisError;
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p oneshim-analysis`

- [ ] **Step 4: Migrate internal functions**

Key mappings:
- `CoreError::Internal("HNSW ...")` → `AnalysisError::VectorIndex("...")`
- `CoreError::Analysis("GMM ...")` → `AnalysisError::Clustering("...")`
- `CoreError::Analysis("HDBSCAN ...")` → `AnalysisError::Clustering("...")`
- `CoreError::Analysis("LLM ...")` → `AnalysisError::LlmService("...")`

- [ ] **Step 5: Update test mocks**

```rust
// adaptive_search.rs — MockAnnIndex
// BEFORE
Err(CoreError::Internal("mock HNSW search failure".into()))
// AFTER
Err(AnalysisError::VectorIndex("mock HNSW search failure".into()))

// llm_segment_summarizer.rs — FailingAnalysisProvider
// BEFORE
Err(CoreError::Analysis("mock failure".into()))
// AFTER
Err(AnalysisError::LlmService("mock failure".into()))

// daily_insight_generator.rs — mock
// BEFORE
Err(CoreError::Analysis("LLM unavailable".into()))
// AFTER
Err(AnalysisError::LlmService("LLM unavailable".into()))
```

- [ ] **Step 6: Verify analysis crate**

Run: `cargo check -p oneshim-analysis && cargo test -p oneshim-analysis && cargo clippy -p oneshim-analysis`

- [ ] **Step 7: Commit**

```bash
git add crates/oneshim-analysis/
git commit -m "refactor(analysis): introduce AnalysisError per ADR-001 §1"
```

---

### Task 3: oneshim-network — NetworkError

**Files:**
- Create: `crates/oneshim-network/src/error.rs`
- Modify: `crates/oneshim-network/src/lib.rs`
- Modify: 30+ source files across `src/`, `src/grpc/`, `src/sync/`, `src/integration/`, `src/oauth/`, `src/ai_llm_client/`, `src/ai_ocr_client/`
- Tests: `http_client.rs`, `grpc/error_mapping.rs`, `analysis_client.rs`, `integration/policy_egress.rs`, `batch_uploader.rs`, `resilience.rs`, `integration/http_transport/tests.rs`, `integration/auth/tests.rs`

- [ ] **Step 1: Create `error.rs` with NetworkError enum**

```rust
// crates/oneshim-network/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("request timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("OAuth error for {provider}: {message}")]
    OAuth { provider: String, message: String },

    #[error("OAuth refresh failed for {provider}: {message}")]
    OAuthRefresh { provider: String, message: String },

    #[error("{resource_type} not found: {id}")]
    NotFound { resource_type: String, id: String },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("analysis API error: {0}")]
    Analysis(String),

    #[error("policy denied: {0}")]
    PolicyDenied(String),

    #[error("OCR error: {0}")]
    Ocr(String),

    #[error("secret store error: {0}")]
    SecretStore(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<NetworkError> for CoreError {
    fn from(err: NetworkError) -> Self {
        match err {
            NetworkError::Core(e) => e,
            NetworkError::Http(msg) => CoreError::Network(msg),
            NetworkError::Timeout { timeout_ms } => CoreError::RequestTimeout { timeout_ms },
            NetworkError::RateLimited { retry_after_secs } => {
                CoreError::RateLimit { retry_after_secs }
            }
            NetworkError::ServiceUnavailable(msg) => CoreError::ServiceUnavailable(msg),
            NetworkError::Auth(msg) => CoreError::Auth(msg),
            NetworkError::OAuth { provider, message } => {
                CoreError::OAuthError { provider, message }
            }
            NetworkError::OAuthRefresh { provider, message } => {
                CoreError::OAuthRefreshError {
                    provider,
                    kind: oneshim_core::ports::oauth::OAuthErrorKind::ServerError,
                    message,
                }
            }
            NetworkError::NotFound { resource_type, id } => {
                CoreError::NotFound { resource_type, id }
            }
            NetworkError::Serialization(msg) => {
                CoreError::Internal(format!("serialization: {msg}"))
            }
            NetworkError::Config(msg) => CoreError::Config(msg),
            NetworkError::Validation(msg) => CoreError::Validation {
                field: String::new(),
                message: msg,
            },
            NetworkError::Analysis(msg) => CoreError::Analysis(msg),
            NetworkError::PolicyDenied(msg) => CoreError::PolicyDenied(msg),
            NetworkError::Ocr(msg) => CoreError::OcrError(msg),
            NetworkError::SecretStore(msg) => CoreError::SecretStoreError(msg),
            NetworkError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/oneshim-network/src/lib.rs`:
```rust
pub mod error;
pub use error::NetworkError;
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p oneshim-network`

- [ ] **Step 4: Migrate internal functions**

Key mappings:
- `CoreError::Network(msg)` → `NetworkError::Http(msg)`
- `CoreError::RequestTimeout { timeout_ms }` → `NetworkError::Timeout { timeout_ms }`
- `CoreError::RateLimit { retry_after_secs }` → `NetworkError::RateLimited { retry_after_secs }`
- `CoreError::ServiceUnavailable(msg)` → `NetworkError::ServiceUnavailable(msg)`
- `CoreError::Auth(msg)` → `NetworkError::Auth(msg)`
- `CoreError::OAuthError { provider, message }` → `NetworkError::OAuth { provider, message }`
- `CoreError::OAuthRefreshError { provider, message, .. }` → `NetworkError::OAuthRefresh { provider, message }`
- `CoreError::NotFound { .. }` → `NetworkError::NotFound { .. }`
- `CoreError::Config(msg)` → `NetworkError::Config(msg)`
- `CoreError::Validation { field, message }` → `NetworkError::Validation(format!("{field}: {message}"))`
- `CoreError::Analysis(msg)` → `NetworkError::Analysis(msg)`
- `CoreError::PolicyDenied(msg)` → `NetworkError::PolicyDenied(msg)`
- `CoreError::OcrError(msg)` → `NetworkError::Ocr(msg)`
- `CoreError::Internal(msg)` → `NetworkError::Internal(msg)`
- `CoreError::InvalidArguments(msg)` → `NetworkError::Config(msg)`

Also update `grpc/error_mapping.rs`:
```rust
// BEFORE
pub fn map_grpc_status_error(operation: &str, status: Status) -> CoreError { ... }
// AFTER
pub fn map_grpc_status_error(operation: &str, status: Status) -> NetworkError { ... }
```

- [ ] **Step 5: Update test assertions**

```rust
// http_client.rs tests
// BEFORE
assert!(matches!(err, CoreError::RateLimit { .. }));
// AFTER
assert!(matches!(err, NetworkError::RateLimited { .. }));

// grpc/error_mapping.rs tests
// BEFORE
assert!(matches!(result, CoreError::Auth(_)));
// AFTER
assert!(matches!(result, NetworkError::Auth(_)));

// integration/http_transport/tests.rs
// BEFORE
assert!(matches!(err, CoreError::InvalidArguments(_)));
// AFTER
assert!(matches!(err, NetworkError::Config(_)));
```

- [ ] **Step 6: Verify network crate**

Run: `cargo check -p oneshim-network && cargo test -p oneshim-network && cargo clippy -p oneshim-network`

- [ ] **Step 7: Verify workspace**

Run: `cargo check --workspace`

- [ ] **Step 8: Commit**

```bash
git add crates/oneshim-network/
git commit -m "refactor(network): introduce NetworkError per ADR-001 §1"
```

---

### Task 4: oneshim-automation — AutomationError

**Files:**
- Create: `crates/oneshim-automation/src/error.rs`
- Modify: `crates/oneshim-automation/src/lib.rs`
- Modify: 19 source files in `src/`, `src/controller/`, `src/policy/`, `src/gui_interaction/`, `src/sandbox/`
- Tests: `controller/tests.rs`, `policy/mod.rs`, `intent_resolver.rs`

- [ ] **Step 1: Create `error.rs` with AutomationError enum**

```rust
// crates/oneshim-automation/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AutomationError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("policy denied: {0}")]
    PolicyDenied(String),

    #[error("sandbox not supported: {0}")]
    SandboxUnsupported(String),

    #[error("sandbox init failed: {0}")]
    SandboxInit(String),

    #[error("sandbox execution failed: {0}")]
    SandboxExecution(String),

    #[error("execution timeout after {timeout_ms}ms")]
    ExecutionTimeout { timeout_ms: u64 },

    #[error("element not found: {0}")]
    ElementNotFound(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("privacy denied: {0}")]
    PrivacyDenied(String),

    #[error("invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<AutomationError> for CoreError {
    fn from(err: AutomationError) -> Self {
        match err {
            AutomationError::Core(e) => e,
            AutomationError::Io(e) => CoreError::Io(e),
            AutomationError::PolicyDenied(msg) => CoreError::PolicyDenied(msg),
            AutomationError::SandboxUnsupported(msg) => CoreError::SandboxUnsupported(msg),
            AutomationError::SandboxInit(msg) => CoreError::SandboxInit(msg),
            AutomationError::SandboxExecution(msg) => CoreError::SandboxExecution(msg),
            AutomationError::ExecutionTimeout { timeout_ms } => {
                CoreError::ExecutionTimeout { timeout_ms }
            }
            AutomationError::ElementNotFound(msg) => CoreError::ElementNotFound(msg),
            AutomationError::Config(msg) => CoreError::Config(msg),
            AutomationError::ServiceUnavailable(msg) => CoreError::ServiceUnavailable(msg),
            AutomationError::PrivacyDenied(msg) => CoreError::PrivacyDenied(msg),
            AutomationError::InvalidArguments(msg) => CoreError::InvalidArguments(msg),
            AutomationError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/oneshim-automation/src/lib.rs`:
```rust
pub mod error;
pub use error::AutomationError;
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p oneshim-automation`

- [ ] **Step 4: Migrate internal functions**

Direct 1:1 variant name mappings. `CoreError::PolicyDenied` → `AutomationError::PolicyDenied`, etc.

- [ ] **Step 5: Update test assertions**

```rust
// controller/tests.rs
// BEFORE
assert!(matches!(err, CoreError::PolicyDenied(_)));
// AFTER
assert!(matches!(err, AutomationError::PolicyDenied(_)));

// intent_resolver.rs tests
// BEFORE
assert!(matches!(result.unwrap_err(), CoreError::ElementNotFound(_)));
// AFTER
assert!(matches!(result.unwrap_err(), AutomationError::ElementNotFound(_)));

// policy/mod.rs tests
// BEFORE
assert!(matches!(result, Err(CoreError::PolicyDenied(_))));
// AFTER
assert!(matches!(result, Err(AutomationError::PolicyDenied(_))));
```

- [ ] **Step 6: Verify automation crate**

Run: `cargo check -p oneshim-automation && cargo test -p oneshim-automation && cargo clippy -p oneshim-automation`

- [ ] **Step 7: Commit**

```bash
git add crates/oneshim-automation/
git commit -m "refactor(automation): introduce AutomationError per ADR-001 §1"
```

---

### Task 5: oneshim-monitor — MonitorError

**Files:**
- Create: `crates/oneshim-monitor/src/error.rs`
- Modify: `crates/oneshim-monitor/src/lib.rs`
- Modify: 16 source files in `src/`

- [ ] **Step 1: Create `error.rs` with MonitorError enum**

```rust
// crates/oneshim-monitor/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MonitorError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<MonitorError> for CoreError {
    fn from(err: MonitorError) -> Self {
        match err {
            MonitorError::Core(e) => e,
            MonitorError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/oneshim-monitor/src/lib.rs`:
```rust
pub mod error;
pub use error::MonitorError;
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p oneshim-monitor`

- [ ] **Step 4: Migrate internal functions**

All 10 occurrences: `CoreError::Internal(msg)` → `MonitorError::Internal(msg)`

- [ ] **Step 5: Verify monitor crate**

Run: `cargo check -p oneshim-monitor && cargo test -p oneshim-monitor && cargo clippy -p oneshim-monitor`

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-monitor/
git commit -m "refactor(monitor): introduce MonitorError per ADR-001 §1"
```

---

### Task 6: oneshim-embedding — EmbeddingError

**Files:**
- Create: `crates/oneshim-embedding/src/error.rs`
- Modify: `crates/oneshim-embedding/src/lib.rs`

- [ ] **Step 1: Create `error.rs` with EmbeddingError enum**

```rust
// crates/oneshim-embedding/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<EmbeddingError> for CoreError {
    fn from(err: EmbeddingError) -> Self {
        match err {
            EmbeddingError::Core(e) => e,
            EmbeddingError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/oneshim-embedding/src/lib.rs`:
```rust
pub mod error;
pub use error::EmbeddingError;
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p oneshim-embedding`

- [ ] **Step 4: Migrate internal functions**

All 10 occurrences: `CoreError::Internal(msg)` → `EmbeddingError::Internal(msg)`

- [ ] **Step 5: Verify embedding crate**

Run: `cargo check -p oneshim-embedding && cargo test -p oneshim-embedding && cargo clippy -p oneshim-embedding`

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-embedding/
git commit -m "refactor(embedding): introduce EmbeddingError per ADR-001 §1"
```

---

### Task 7: oneshim-suggestion — SuggestionError

**Files:**
- Create: `crates/oneshim-suggestion/src/error.rs`
- Modify: `crates/oneshim-suggestion/src/lib.rs`
- Modify: `feedback.rs`, `history.rs`, `receiver.rs`, `queue.rs`, `presenter.rs`

- [ ] **Step 1: Create `error.rs` with SuggestionError enum**

```rust
// crates/oneshim-suggestion/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SuggestionError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<SuggestionError> for CoreError {
    fn from(err: SuggestionError) -> Self {
        match err {
            SuggestionError::Core(e) => e,
            SuggestionError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/oneshim-suggestion/src/lib.rs`:
```rust
pub mod error;
pub use error::SuggestionError;
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p oneshim-suggestion`

- [ ] **Step 4: Migrate internal functions**

This crate has 0 direct CoreError variant creation — it receives CoreError from port calls and passes it through. After adding `Core(#[from] CoreError)`, internal functions that currently return `CoreError` switch to `SuggestionError`. Port calls returning `CoreError` automatically wrap into `SuggestionError::Core` via `?`.

- [ ] **Step 5: Verify suggestion crate**

Run: `cargo check -p oneshim-suggestion && cargo test -p oneshim-suggestion && cargo clippy -p oneshim-suggestion`

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-suggestion/
git commit -m "refactor(suggestion): introduce SuggestionError per ADR-001 §1"
```

---

### Task 8: oneshim-vision — VisionError

**Files:**
- Create: `crates/oneshim-vision/src/error.rs`
- Modify: `crates/oneshim-vision/src/lib.rs`
- Modify: 24 source files in `src/`, `src/accessibility/`, `src/gui_detector/`
- Remove: internal `OcrError` from `ocr.rs` (absorbed into `VisionError`)
- Tests: `accessibility/macos/tests.rs`

- [ ] **Step 1: Create `error.rs` with VisionError enum**

```rust
// crates/oneshim-vision/src/error.rs
use oneshim_core::error::CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VisionError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("OCR error: {0}")]
    Ocr(String),

    #[error("element not found: {0}")]
    ElementNotFound(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<VisionError> for CoreError {
    fn from(err: VisionError) -> Self {
        match err {
            VisionError::Core(e) => e,
            VisionError::PermissionDenied(msg) => CoreError::PermissionDenied(msg),
            VisionError::Ocr(msg) => CoreError::OcrError(msg),
            VisionError::ElementNotFound(msg) => CoreError::ElementNotFound(msg),
            VisionError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/oneshim-vision/src/lib.rs`:
```rust
pub mod error;
pub use error::VisionError;
```

- [ ] **Step 3: Verify compiles**

Run: `cargo check -p oneshim-vision`

- [ ] **Step 4: Absorb OcrError into VisionError**

In `ocr.rs`, the internal `OcrError` enum (5 variants) is used only within that module. Replace with `VisionError::Ocr`:

```rust
// BEFORE (ocr.rs)
#[derive(Debug, Error)]
pub enum OcrError {
    #[error("OCR initialize failure: {0}")]
    Init(String),
    // ...
}

// AFTER — remove OcrError, use VisionError
use crate::error::VisionError;
// Replace OcrError::Init(msg) → VisionError::Ocr(format!("init: {msg}"))
```

- [ ] **Step 5: Migrate internal functions**

Key mappings:
- `CoreError::Internal(msg)` → `VisionError::Internal(msg)`
- `CoreError::PermissionDenied(msg)` → `VisionError::PermissionDenied(msg)`
- `CoreError::OcrError(msg)` → `VisionError::Ocr(msg)`
- `CoreError::ElementNotFound(msg)` → `VisionError::ElementNotFound(msg)`

- [ ] **Step 6: Update test assertions**

```rust
// accessibility/macos/tests.rs
// BEFORE
assert!(matches!(err, CoreError::PermissionDenied(_)));
// AFTER
assert!(matches!(err, VisionError::PermissionDenied(_)));
```

- [ ] **Step 7: Verify vision crate**

Run: `cargo check -p oneshim-vision && cargo test -p oneshim-vision && cargo clippy -p oneshim-vision`

- [ ] **Step 8: Commit**

```bash
git add crates/oneshim-vision/
git commit -m "refactor(vision): introduce VisionError per ADR-001 §1"
```

---

### Task 9: Final Workspace Verification

- [ ] **Step 1: Full workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 2: Full workspace tests**

Run: `cargo test --workspace`
Expected: All tests PASS

- [ ] **Step 3: Full workspace clippy**

Run: `cargo clippy --workspace`
Expected: No new warnings

- [ ] **Step 4: Format check**

Run: `cargo fmt --check`
Expected: PASS

- [ ] **Step 5: Verify no remaining CoreError in internal functions**

Run: `grep -r "-> Result<.*CoreError>" crates/ --include="*.rs" | grep -v "test" | grep -v "mod.rs"` — review output. Only port trait impls should return `CoreError`.

- [ ] **Step 6: Final commit (if any fixups needed)**

```bash
git add -A
git commit -m "refactor: final fixups for error strategy compliance"
```
