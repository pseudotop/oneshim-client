# Phase 3 — YELLOW Domain Stabilization Spec

**Date**: 2026-04-05
**Scope**: `oneshim-automation`, `oneshim-embedding`, `oneshim-network`, `src-tauri`
**Prerequisite**: v0.4.20 + Phase 1/2 gaps completed

## 1. Problem Statement

Four domains are rated YELLOW (infrastructure built, wiring incomplete). Each needs targeted fixes to reach GREEN.

| # | Domain | Current State | Gap |
|---|--------|--------------|-----|
| 9 | Automation confirmation | Modal UI + IPC exist but **execution gate never emits event** | Dead code — modal never shown |
| 10 | Embedding fallback | Local + Remote + NoOp providers exist but **startup-time selection only** | No per-request fallback chain |
| 11 | Cross-device sync | Transport + encryption + auth solid | **Conflict resolution undocumented**, no health check |
| 12 | Auto-update | Check + download + integrity + rollback solid | **No runtime verification beyond mocked tests** |

## 2. Goals

1. **#9**: Wire execution gate to emit `automation:confirm-request` when policy requires confirmation
2. **#10**: Add `FallbackEmbeddingProvider` that chains local → remote → noop at request time
3. **#11**: Add sync health check IPC + document conflict resolution strategy
4. **#12**: Add update dry-run verification IPC that tests download+checksum without installing

### Non-Goals

- New frontend pages (audit log viewer, policy config UI — future work)
- Platform-specific e2e update tests (CI infrastructure, not code)
- LAN mDNS integration tests (requires physical multi-device setup)

## 3. Design

### 3.1 Item #9: Automation Execution Gate — Confirmation Wiring

**Problem**: `AutomationController` has `pending_confirmations` HashMap and the overlay has `AutomationConfirmModal`, but nothing connects them. The execution gate (`controller/mod.rs`) never calls the confirmation path.

**Solution**: Add a `ConfirmationRequirement` enum and wire the gate to check policy before executing.

#### 3.1.1 Add `ConfirmationRequirement` to policy models

**File**: `crates/oneshim-core/src/config/enums.rs`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmationRequirement {
    /// Low-risk: execute immediately without user prompt
    Auto,
    /// Medium-risk: show confirmation modal, wait for user decision
    #[default]
    Confirm,
    /// High-risk: block execution entirely
    Block,
}
```

#### 3.1.2 Add field to `ExecutionPolicy`

**File**: `crates/oneshim-automation/src/policy/models.rs`

Add `confirmation: ConfirmationRequirement` field (default: `Confirm`).

#### 3.1.3 Wire execution gate

**File**: `crates/oneshim-automation/src/controller/gate.rs`

The gate's `execute()` method (gate.rs:88-160) currently does: validate → resolve config → dispatch.
Insert confirmation check BEFORE dispatch:

```rust
match policy.confirmation {
    ConfirmationRequirement::Auto => { /* proceed */ }
    ConfirmationRequirement::Confirm => {
        let approved = self.request_confirmation(&command, &policy).await?;
        if !approved { return Err(AutomationError::UserDenied); }
    }
    ConfirmationRequirement::Block => {
        return Err(AutomationError::PolicyBlocked);
    }
}
```

The `request_confirmation` method:
1. Creates `PendingConfirmation` with nonce
2. Creates oneshot channel
3. Inserts into `pending_confirmations` map
4. Emits `automation:confirm-request` Tauri event via `AppHandle`
5. Awaits oneshot receiver with 30-second timeout
6. Returns `bool`

**Key issue**: `AutomationController` doesn't have `AppHandle` for event emission. Two options:
- A) Pass `AppHandle` to controller (breaks hexagonal — controller is in library crate)
- B) Use a callback `Arc<dyn Fn(PendingConfirmation) + Send + Sync>` set during wiring

**Decision**: Option B — callback. The binary crate sets the callback to emit Tauri events. The library crate stays framework-agnostic.

#### 3.1.4 Add confirmation callback to controller

```rust
// In AutomationController
pub(super) on_confirmation_needed: Option<Arc<dyn Fn(PendingConfirmation) + Send + Sync>>,
```

Set during wiring in `app_runtime_launch.rs`:
```rust
controller.set_confirmation_callback(Arc::new(move |confirmation| {
    let _ = app_handle.emit("automation:confirm-request", &confirmation);
}));
```

### 3.2 Item #10: Embedding Fallback Chain

**Problem**: `embedding_setup.rs` selects one provider at startup. If local ONNX fails to load, it falls to NoOp. No dynamic fallback during operation.

**Solution**: Add `FallbackEmbeddingProvider` wrapper.

#### 3.2.1 New wrapper in `crates/oneshim-embedding/src/lib.rs`

```rust
pub struct FallbackEmbeddingProvider {
    primary: Arc<dyn EmbeddingProvider>,
    fallback: Arc<dyn EmbeddingProvider>,
}

#[async_trait]
impl EmbeddingProvider for FallbackEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
        match self.primary.embed(text).await {
            Ok(v) => Ok(v),
            Err(e) => {
                tracing::warn!("primary embedding failed, trying fallback: {e}");
                self.fallback.embed(text).await
            }
        }
    }
    // Same pattern for embed_batch, dimensions delegates to primary
}
```

**Dimension handling**: `dimensions()` and `model_id()` delegate to primary. All providers currently use 384 dimensions (local ONNX default, remote configured, NoOp hardcoded). If dimensions mismatch, `FallbackEmbeddingProvider` logs a warning but proceeds — the vector store already handles dimension validation on insert.

#### 3.2.2 Wire in `embedding_setup.rs`

Current setup (lines 39-144) tries local first, falls to NoOp if it fails, then separately tries remote. Change to chain: build primary+fallback pair, then wrap in `FallbackEmbeddingProvider`.

```rust
// Build candidates
let local_result = try_build_local(&config);
let remote_result = try_build_remote(&config);
let noop = Arc::new(NoOpEmbeddingProvider::new(384));

// Chain: best available → next → noop
let provider: Arc<dyn EmbeddingProvider> = match (local_result, remote_result) {
    (Ok(l), Ok(r)) => Arc::new(FallbackEmbeddingProvider::new(l, Arc::new(FallbackEmbeddingProvider::new(r, noop)))),
    (Ok(l), Err(_)) => Arc::new(FallbackEmbeddingProvider::new(l, noop)),
    (Err(_), Ok(r)) => Arc::new(FallbackEmbeddingProvider::new(r, noop)),
    (Err(_), Err(_)) => noop,
};
```

### 3.3 Item #11: Sync Health Check

**Problem**: No way to verify sync is working without actually syncing.

**Solution**: Add `get_sync_health` IPC command that returns transport status.

#### 3.3.1 Add health check to SyncEngine

**File**: `src-tauri/src/sync_engine.rs`

```rust
pub struct SyncHealthStatus {
    pub transport_type: String,    // "lan" | "remote" | "file"
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub peer_count: usize,
    pub consent_granted: bool,
}

pub async fn health(&self) -> SyncHealthStatus { ... }
```

#### 3.3.2 Extend existing `get_sync_status` IPC

**File**: `src-tauri/src/commands/sync.rs`

`get_sync_status` already exists (returns enabled, device_id, device_name). Extend its return DTO with health fields rather than adding a new command:

```rust
pub struct SyncStatusDto {
    // existing fields...
    pub enabled: bool,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    // NEW health fields
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub peer_count: usize,
}
```

#### 3.3.3 Document conflict resolution

**File**: `docs/guides/sync-conflict-resolution.md` (NEW)

Document the current strategy:
- Push returns 409 → SyncEngine re-pulls and re-merges
- Last-write-wins for individual records
- GDPR deletion events are never overwritten

### 3.4 Item #12: Update Dry-Run Verification

**Problem**: Update mechanism is mocked in tests. No way to verify download+checksum without installing.

**Solution**: Add `verify_update` IPC command that downloads and checksums but doesn't install.

#### 3.4.1 Add verification to updater

**File**: `src-tauri/src/updater/install.rs`

Current code: `download_update()` downloads and returns `PathBuf`. Checksum verification is built into the download flow via `fetch_expected_sha256()`. Signature verification via `verify_signature()` (pub(super)).

Extract a standalone dry-run function that reuses existing internals:

```rust
pub async fn verify_update_integrity(release: &UpdateRelease) -> Result<VerifyResult, UpdateError> {
    let path = download_update(release).await?; // downloads to temp
    let sig_ok = verify_signature(&path, release).await.unwrap_or(false);
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    std::fs::remove_file(&path).ok(); // cleanup — don't install
    Ok(VerifyResult { checksum_ok: true, signature_ok: sig_ok, size_bytes: size })
    // checksum_ok is always true here because download_update already validates it
}
```

#### 3.4.2 Add IPC command

**File**: `src-tauri/src/commands/system.rs`

```rust
#[command]
pub async fn verify_update(...) -> Result<VerifyResult, String>
```

## 4. File Change Summary

| # | File | Change |
|---|------|--------|
| 1 | `crates/oneshim-core/src/config/enums.rs` | +ConfirmationRequirement enum |
| 2 | `crates/oneshim-automation/src/policy/models.rs` | +confirmation field |
| 3 | `crates/oneshim-automation/src/controller/gate.rs` | Wire confirmation check before dispatch |
| 3b | `crates/oneshim-automation/src/controller/mod.rs` | Add confirmation callback field + setter |
| 4 | `src-tauri/src/app_runtime_launch.rs` | Set confirmation callback |
| 5 | `crates/oneshim-embedding/src/lib.rs` | +FallbackEmbeddingProvider |
| 6 | `src-tauri/src/agent_runtime/embedding_setup.rs` | Chain fallback providers |
| 7 | `src-tauri/src/sync_engine.rs` | +health() method |
| 8 | `src-tauri/src/commands/sync.rs` | Extend get_sync_status with health fields |
| 9 | `src-tauri/src/updater/install.rs` | +verify_update() |
| 10 | `src-tauri/src/commands/system.rs` | +verify_update IPC |
| 11 | `docs/guides/sync-conflict-resolution.md` | NEW — conflict resolution docs |

## 5. Testing Strategy

- `ConfirmationRequirement`: enum serde round-trip test
- `FallbackEmbeddingProvider`: primary-succeeds, primary-fails-fallback-succeeds, both-fail tests
- Execution gate: mock controller with Confirm policy → verify callback invoked
- Sync health: verify status fields populated correctly

## 6. Prioritization

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| P1 | #9 Confirmation wiring | Medium | Dead code → functional feature |
| P1 | #10 Embedding fallback | Small | Resilience improvement |
| P2 | #11 Sync health + docs | Small | Observability + documentation |
| P2 | #12 Update verification | Small | Safety verification |
