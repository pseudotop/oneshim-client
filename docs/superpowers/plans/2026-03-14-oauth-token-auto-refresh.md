# OAuth Token Auto-Refresh Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add proactive background token refresh with failure taxonomy, concurrent guard, retry/backoff, auto-recovery, and UI notifications for OAuth-managed credentials.

**Architecture:** `RefreshResult` enum on `OAuthPort` provides typed refresh outcomes. `TokenRefreshCoordinator` in oneshim-network orchestrates refresh via the port. An `AtomicBool` guard on `OAuthClient` prevents concurrent refreshes. A 10th scheduler loop calls the coordinator every 2 minutes. UI shows expiry badges.

**Tech Stack:** Rust (tokio, chrono, broadcast channel), React (TypeScript), i18n

**Spec:** `docs/superpowers/specs/2026-03-14-oauth-token-auto-refresh-design.md`

**Worktree:** `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/oneshim-core/src/ports/oauth.rs` | Modify | Add `RefreshResult`, `TokenEvent`, `refresh_access_token()` to `OAuthPort`, `has_refresh_token` to `OAuthConnectionStatus` |
| `crates/oneshim-network/src/oauth/refresh_coordinator.rs` | **Create** | Coordinator + RefreshState + RefreshOutcome |
| `crates/oneshim-network/src/oauth/mod.rs` | Modify | AtomicBool guard, `refresh_access_token()` impl with failure taxonomy, export coordinator |
| `src-tauri/src/scheduler/config.rs` | Modify | Add `OAUTH_REFRESH_INTERVAL_SECS` constant |
| `src-tauri/src/scheduler/mod.rs` | Modify | Add coordinator field + builder method |
| `src-tauri/src/scheduler/loops.rs` | Modify | Add `spawn_oauth_refresh_loop` (10th loop) |
| `src-tauri/src/setup.rs` | Modify | Create coordinator, wire into scheduler |
| `crates/oneshim-web/frontend/src/pages/settingSections/OAuthConnectionPanel.tsx` | Modify | Expiry badge + auto-refresh |
| `src-tauri/src/provider_adapters.rs` | Modify | Add `has_refresh_token` + `refresh_access_token()` to FakeOAuthPort |
| `crates/oneshim-web/frontend/src/api/contracts.ts` | Modify | Add `has_refresh_token` to TypeScript type |
| `crates/oneshim-web/frontend/src/i18n/locales/en.json` | Modify | 4 new keys |
| `crates/oneshim-web/frontend/src/i18n/locales/ko.json` | Modify | 4 new keys |

---

## Chunk 1: Port Contract + OAuthClient Changes

### Task 1: Add RefreshResult, TokenEvent, and has_refresh_token (types only)

**Files:**
- Modify: `crates/oneshim-core/src/ports/oauth.rs`

**Note:** This task adds types and the `has_refresh_token` field but does NOT add `refresh_access_token()` to the `OAuthPort` trait. That method is added in Task 3 alongside its implementation, so all OAuthPort implementors (OAuthClient, FakeOAuthPort) can be updated atomically.

- [ ] **Step 1: Add RefreshResult enum after OAuthConnectionStatus (line 38)**

```rust
/// Typed result from an explicit refresh attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshResult {
    /// Token has ≥ min_valid_for_secs remaining — no refresh was attempted.
    AlreadyFresh { expires_at: String },
    /// Token was refreshed successfully.
    Refreshed { expires_at: String },
    /// No stored credentials — user has never authenticated.
    NotAuthenticated,
    /// Refresh token is invalid/revoked — full re-authentication required.
    ReauthRequired { reason: String },
    /// Transient failure (network, server error) — retry later.
    TransientFailure { message: String },
}
```

- [ ] **Step 2: Add `has_refresh_token` field to OAuthConnectionStatus (line 30-38)**

Add after the `api_base_url` field:

```rust
/// Whether a refresh token is stored (indicates background refresh capability).
#[serde(default)]
pub has_refresh_token: bool,
```

- [ ] **Step 3: Add TokenEvent enum after the OAuthPort trait (before `#[cfg(test)]` at line 70)**

```rust
/// Events emitted by the token refresh coordinator.
#[derive(Clone, Debug)]
pub enum TokenEvent {
    /// Token successfully refreshed.
    Refreshed {
        provider_id: String,
        expires_at: String,
    },
    /// Refresh attempt failed, will retry.
    RefreshFailed {
        provider_id: String,
        attempt: u8,
        max_attempts: u8,
    },
    /// All retry attempts exhausted or terminal failure — user must re-authenticate.
    ReauthRequired { provider_id: String },
}
```

- [ ] **Step 4: Update existing tests that construct OAuthConnectionStatus in oauth.rs**

Add `has_refresh_token: true` to the `connection_status_serialization` test at line 103:

```rust
let status = OAuthConnectionStatus {
    provider_id: "openai".to_string(),
    connected: true,
    expires_at: Some("2026-03-14T00:00:00Z".to_string()),
    scopes: vec!["openid".to_string(), "offline_access".to_string()],
    api_base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
    has_refresh_token: true,
};
```

- [ ] **Step 5: Update FakeOAuthPort in provider_adapters.rs**

In `src-tauri/src/provider_adapters.rs`, update the `connection_status` method of `FakeOAuthPort` (line 915-921) to include `has_refresh_token`:

```rust
Ok(OAuthConnectionStatus {
    provider_id: provider_id.to_string(),
    connected: self.connected,
    expires_at: None,
    scopes: vec![],
    api_base_url: None,
    has_refresh_token: false,
})
```

- [ ] **Step 6: Update OAuthConnectionStatus in oneshim-network mod.rs**

In `crates/oneshim-network/src/oauth/mod.rs`, update the `connection_status()` method (line 316-352) to populate `has_refresh_token`. Add after scopes retrieval (line 336):

```rust
let has_refresh_token = self
    .secret_store
    .retrieve(provider_id, KEY_REFRESH_TOKEN)
    .await?
    .is_some();
```

Update the OAuthConnectionStatus construction (line 345-351) to include `has_refresh_token`.

Also update all test assertions/constructions in `mod.rs` tests that use `OAuthConnectionStatus` (e.g., `connection_status_disconnected` and `connection_status_connected`).

- [ ] **Step 7: Update TypeScript contracts**

In `crates/oneshim-web/frontend/src/api/contracts.ts`, add `has_refresh_token` to the `OAuthConnectionStatus` interface (line 634-640):

```typescript
export interface OAuthConnectionStatus {
  provider_id: string
  connected: boolean
  expires_at: string | null
  scopes: string[]
  api_base_url: string | null
  has_refresh_token?: boolean
}
```

- [ ] **Step 8: Verify it compiles (full workspace)**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo check --workspace`
Expected: compiles with no errors (no trait method added yet, so all implementors compile)

- [ ] **Step 9: Commit**

```bash
git add crates/oneshim-core/src/ports/oauth.rs crates/oneshim-network/src/oauth/mod.rs src-tauri/src/provider_adapters.rs crates/oneshim-web/frontend/src/api/contracts.ts
git commit -m "feat(core): add RefreshResult, TokenEvent, has_refresh_token to OAuthConnectionStatus"
```

---

### Task 2: Add AtomicBool refresh guard to OAuthClient

**Files:**
- Modify: `crates/oneshim-network/src/oauth/mod.rs:43-48` (OAuthClient struct)
- Modify: `crates/oneshim-network/src/oauth/mod.rs:91` (try_refresh method)

- [ ] **Step 1: Write AtomicBool semantics verification test**

Add to the bottom of `crates/oneshim-network/src/oauth/mod.rs` inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn refresh_guard_prevents_concurrent_refresh() {
    use std::sync::atomic::Ordering;
    let guard = std::sync::atomic::AtomicBool::new(false);
    assert!(guard.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok());
    assert!(guard.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err());
    guard.store(false, Ordering::Release);
    assert!(guard.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok());
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo test -p oneshim-network -- refresh_guard_prevents`
Expected: PASS

- [ ] **Step 3: Add AtomicBool field to OAuthClient struct**

In `crates/oneshim-network/src/oauth/mod.rs`, add import and field:

```rust
use std::sync::atomic::{AtomicBool, Ordering};

pub struct OAuthClient {
    http: reqwest::Client,
    secret_store: Arc<dyn SecretStore>,
    providers: HashMap<String, OAuthProviderConfig>,
    active_flows: Arc<Mutex<HashMap<String, ActiveFlow>>>,
    refresh_in_progress: AtomicBool,  // NEW
}
```

Update the constructor (`new()`, line 52-63) to initialize `refresh_in_progress: AtomicBool::new(false)`.

- [ ] **Step 4: Guard try_refresh() with AtomicBool**

In `try_refresh()` (line 91), wrap the refresh logic:

```rust
async fn try_refresh(&self, provider_id: &str) -> Result<bool, CoreError> {
    // Acquire refresh guard — if another refresh is in progress, skip
    if self.refresh_in_progress
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        debug!("Refresh already in progress for {}, skipping", provider_id);
        return Ok(false);
    }

    struct RefreshGuard<'a>(&'a AtomicBool);
    impl<'a> Drop for RefreshGuard<'a> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let _guard = RefreshGuard(&self.refresh_in_progress);

    // ... existing refresh logic unchanged ...
}
```

- [ ] **Step 5: Verify the guard test still passes**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo test -p oneshim-network -- refresh_guard_prevents`

**Note:** Full `cargo test -p oneshim-network` will fail here because OAuthPort now has `has_refresh_token` changes but `refresh_access_token()` hasn't been added to the trait yet. That is expected — full compilation resumes after Task 3.

Expected: PASS (the unit test only exercises AtomicBool logic, not OAuthPort)

**Note on compilation**: If this test fails to compile because the module can't build, combine this commit with Task 3 instead. The guard + impl must go together.

- [ ] **Step 6: Commit** (or defer to Task 3 if compilation fails)

```bash
git add crates/oneshim-network/src/oauth/mod.rs
git commit -m "feat(network): add AtomicBool concurrent refresh guard to OAuthClient"
```

---

### Task 3: Add refresh_access_token() to OAuthPort trait + implement on OAuthClient + update FakeOAuthPort

**Files:**
- Modify: `crates/oneshim-core/src/ports/oauth.rs` (add `refresh_access_token()` to trait)
- Modify: `crates/oneshim-network/src/oauth/mod.rs` (implement `refresh_access_token()`)
- Modify: `src-tauri/src/provider_adapters.rs` (add `refresh_access_token()` to FakeOAuthPort)

**Note:** This task adds the trait method and ALL implementations atomically so the workspace compiles.

- [ ] **Step 1a: Add `refresh_access_token()` method to OAuthPort trait in oauth.rs**

In `crates/oneshim-core/src/ports/oauth.rs`, add after `connection_status` (before the closing `}` of the trait):

```rust
/// Attempt to refresh the access token if it expires within `min_valid_for_secs`.
///
/// Unlike `get_access_token()` (which silently refreshes), this method returns
/// a typed `RefreshResult` allowing callers to distinguish terminal from
/// transient failures and to detect "already fresh" without side effects.
async fn refresh_access_token(
    &self,
    provider_id: &str,
    min_valid_for_secs: i64,
) -> Result<RefreshResult, CoreError>;
```

- [ ] **Step 1b: Write tests for refresh_access_token**

Add to `#[cfg(test)] mod tests` in `crates/oneshim-network/src/oauth/mod.rs`:

```rust
#[tokio::test]
async fn refresh_access_token_returns_not_authenticated_when_no_token() {
    use oneshim_core::ports::oauth::RefreshResult;
    let store = Arc::new(TestSecretStore::new());
    let client = make_client(store);
    let result = client.refresh_access_token("openai", 300).await.unwrap();
    assert_eq!(result, RefreshResult::NotAuthenticated);
}

#[tokio::test]
async fn refresh_access_token_returns_already_fresh_when_not_expiring() {
    use oneshim_core::ports::oauth::RefreshResult;
    let store = Arc::new(TestSecretStore::new());
    let expires = (Utc::now() + chrono::Duration::minutes(30)).to_rfc3339();
    store.store("openai", KEY_ACCESS_TOKEN, "tok").await.unwrap();
    store.store("openai", KEY_EXPIRES_AT, &expires).await.unwrap();
    let client = make_client(store);
    let result = client.refresh_access_token("openai", 300).await.unwrap();
    assert!(matches!(result, RefreshResult::AlreadyFresh { .. }));
}

#[tokio::test]
async fn refresh_access_token_returns_reauth_when_no_refresh_token() {
    use oneshim_core::ports::oauth::RefreshResult;
    let store = Arc::new(TestSecretStore::new());
    let expires = (Utc::now() + chrono::Duration::seconds(60)).to_rfc3339();
    store.store("openai", KEY_ACCESS_TOKEN, "tok").await.unwrap();
    store.store("openai", KEY_EXPIRES_AT, &expires).await.unwrap();
    // No refresh token stored
    let client = make_client(store);
    let result = client.refresh_access_token("openai", 300).await.unwrap();
    assert!(matches!(result, RefreshResult::ReauthRequired { .. }));
}
```

- [ ] **Step 2: Implement refresh_access_token()**

Add to `impl OAuthPort for OAuthClient` block (after `connection_status`, before the closing `}`):

```rust
async fn refresh_access_token(
    &self,
    provider_id: &str,
    min_valid_for_secs: i64,
) -> Result<RefreshResult, CoreError> {
    use oneshim_core::ports::oauth::RefreshResult;

    // 1. Check if token exists
    let has_token = self.secret_store
        .retrieve(provider_id, KEY_ACCESS_TOKEN)
        .await?
        .is_some();
    if !has_token {
        return Ok(RefreshResult::NotAuthenticated);
    }

    // 2. Check if token is still fresh enough
    if let Ok(Some(expires_str)) = self.secret_store
        .retrieve(provider_id, KEY_EXPIRES_AT)
        .await
    {
        if let Ok(expires_at) = chrono::DateTime::parse_from_rfc3339(&expires_str) {
            let remaining = expires_at.signed_duration_since(Utc::now());
            if remaining.num_seconds() > min_valid_for_secs {
                return Ok(RefreshResult::AlreadyFresh {
                    expires_at: expires_str,
                });
            }
        }
    }

    // 3. Get refresh token
    let refresh_tok = self.secret_store
        .retrieve(provider_id, KEY_REFRESH_TOKEN)
        .await?;
    let Some(refresh_tok) = refresh_tok else {
        return Ok(RefreshResult::ReauthRequired {
            reason: "no refresh token stored".into(),
        });
    };

    // 4. Attempt refresh with failure classification
    let config = self.get_provider(provider_id)?;
    match token_exchange::refresh_token(&self.http, config, &refresh_tok).await {
        Ok(result) => {
            let expires_at = result.expires_in
                .map(|s| (Utc::now() + chrono::Duration::seconds(s as i64)).to_rfc3339())
                .unwrap_or_default();
            self.store_tokens(provider_id, &result).await?;
            info!("access token refreshed for {provider_id}");
            Ok(RefreshResult::Refreshed { expires_at })
        }
        Err(e) => {
            let msg = e.to_string();
            // Terminal: server explicitly rejected the refresh token
            if msg.contains("invalid_grant") || msg.contains("invalid_client") {
                warn!("terminal refresh failure for {provider_id}: {msg}");
                Ok(RefreshResult::ReauthRequired { reason: msg })
            } else {
                // Transient: network error, 5xx, timeout, etc.
                warn!("transient refresh failure for {provider_id}: {msg}");
                Ok(RefreshResult::TransientFailure { message: msg })
            }
        }
    }
}
```

- [ ] **Step 3: Add has_refresh_token to connection_status()**

In the `connection_status()` method (line 316-352), add a check for refresh token and include it in the response. After the `scopes` retrieval (line 334-336), add:

```rust
let has_refresh_token = self
    .secret_store
    .retrieve(provider_id, KEY_REFRESH_TOKEN)
    .await?
    .is_some();
```

Update the `OAuthConnectionStatus` construction to include `has_refresh_token`:

```rust
Ok(OAuthConnectionStatus {
    provider_id: provider_id.to_string(),
    connected,
    expires_at,
    scopes,
    api_base_url,
    has_refresh_token,
})
```

- [ ] **Step 4: Update import in mod.rs**

Update the `use` statement at line 20-22 to include `RefreshResult`:

```rust
use oneshim_core::ports::oauth::{
    OAuthConnectionStatus, OAuthFlowHandle, OAuthFlowStatus, OAuthPort, RefreshResult,
};
```

- [ ] **Step 5: Update existing tests that construct OAuthConnectionStatus in mocks**

In the existing tests, update `connection_status_disconnected` and `connection_status_connected` to assert on `has_refresh_token`:

```rust
// In connection_status_connected:
assert!(status.has_refresh_token); // or false, depending on test setup
```

Add `has_refresh_token` field wherever `OAuthConnectionStatus` is constructed.

- [ ] **Step 6: Add refresh_access_token() to FakeOAuthPort in provider_adapters.rs**

In `src-tauri/src/provider_adapters.rs`, add to `impl OAuthPort for FakeOAuthPort` (after `connection_status`, before the closing `}`):

```rust
async fn refresh_access_token(
    &self,
    _provider_id: &str,
    _min_valid_for_secs: i64,
) -> Result<RefreshResult, CoreError> {
    if self.connected {
        Ok(RefreshResult::AlreadyFresh {
            expires_at: chrono::Utc::now().to_rfc3339(),
        })
    } else {
        Ok(RefreshResult::NotAuthenticated)
    }
}
```

Add the required import at the top of the test module or the file's existing `use` block:
```rust
use oneshim_core::ports::oauth::RefreshResult;
```

- [ ] **Step 7: Verify full workspace compiles and tests pass**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo test --workspace`
Expected: all tests pass including new `refresh_access_token_*` tests

- [ ] **Step 8: Commit**

```bash
git add crates/oneshim-core/src/ports/oauth.rs crates/oneshim-network/src/oauth/mod.rs src-tauri/src/provider_adapters.rs
git commit -m "feat(network): add refresh_access_token to OAuthPort trait with failure taxonomy"
```

---

## Chunk 2: Coordinator + Tests

### Task 4: Create TokenRefreshCoordinator

**Files:**
- Create: `crates/oneshim-network/src/oauth/refresh_coordinator.rs`
- Modify: `crates/oneshim-network/src/oauth/mod.rs` (add `pub mod refresh_coordinator;`)

- [ ] **Step 1: Create the module file with types**

Create `crates/oneshim-network/src/oauth/refresh_coordinator.rs`:

```rust
//! Proactive OAuth token refresh coordinator.
//!
//! Periodically checks token expiry and triggers refresh before expiration.
//! Manages retry/backoff and emits `TokenEvent`s for UI notification.
//! Uses `RefreshResult` for typed failure classification and auto-recovery.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, Mutex};
use tracing::{debug, info, warn};

use oneshim_core::error::CoreError;
use oneshim_core::ports::oauth::{OAuthPort, RefreshResult, TokenEvent};

/// Outcome of a `check_and_refresh` call.
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshOutcome {
    /// Token is not close to expiry — no action taken.
    NotNeeded,
    /// Token was successfully refreshed.
    Refreshed,
    /// Transient refresh failed, will retry later.
    Failed { attempt: u8 },
    /// Terminal failure or retries exhausted — user must re-authenticate.
    ReauthRequired,
    /// Another refresh is already in progress.
    AlreadyInProgress,
    /// Currently in backoff period.
    BackingOff,
}

/// Internal state tracking refresh attempts and backoff.
struct RefreshState {
    in_progress: bool,
    consecutive_transient_failures: u8,
    last_attempt: Option<Instant>,
    backoff_until: Option<Instant>,
}

impl RefreshState {
    fn new() -> Self {
        Self {
            in_progress: false,
            consecutive_transient_failures: 0,
            last_attempt: None,
            backoff_until: None,
        }
    }

    /// Calculate backoff duration based on transient failure count.
    fn backoff_duration(failures: u8) -> Option<Duration> {
        match failures {
            1 => Some(Duration::from_secs(120)),  // 2 min — skip 1 scheduler tick
            2 => Some(Duration::from_secs(300)),  // 5 min — skip ~2 ticks
            _ => None,                             // 3+ → stop retrying
        }
    }
}

const MAX_CONSECUTIVE_FAILURES: u8 = 3;
/// Proactive refresh threshold — refresh if token expires within this many seconds.
/// Coordinator passes this to `refresh_access_token(provider_id, REFRESH_THRESHOLD_SECS)`.
const REFRESH_THRESHOLD_SECS: i64 = 300; // 5 minutes

/// Orchestrates proactive token refresh with concurrency protection.
///
/// This is an orchestration utility, not a port adapter. The scheduler
/// depends on it directly (concrete type).
pub struct TokenRefreshCoordinator {
    oauth_port: Arc<dyn OAuthPort>,
    state: Mutex<RefreshState>,
    event_tx: broadcast::Sender<TokenEvent>,
}

impl TokenRefreshCoordinator {
    /// Create a new coordinator.
    pub fn new(
        oauth_port: Arc<dyn OAuthPort>,
        event_tx: broadcast::Sender<TokenEvent>,
    ) -> Self {
        Self {
            oauth_port,
            state: Mutex::new(RefreshState::new()),
            event_tx,
        }
    }

    /// Subscribe to token refresh events.
    pub fn subscribe(&self) -> broadcast::Receiver<TokenEvent> {
        self.event_tx.subscribe()
    }

    /// Check token expiry and refresh if needed.
    ///
    /// Uses `oauth_port.refresh_access_token()` for typed results.
    /// Handles auto-recovery: if `AlreadyFresh` is returned while
    /// `consecutive_transient_failures > 0`, the counter resets.
    pub async fn check_and_refresh(
        &self,
        provider_id: &str,
    ) -> Result<RefreshOutcome, CoreError> {
        // Step 1: Check state (short lock)
        {
            let state = self.state.lock().await;

            if state.in_progress {
                return Ok(RefreshOutcome::AlreadyInProgress);
            }

            if let Some(backoff_until) = state.backoff_until {
                if Instant::now() < backoff_until {
                    return Ok(RefreshOutcome::BackingOff);
                }
            }

            // Already exhausted retries — need manual re-auth
            if state.consecutive_transient_failures >= MAX_CONSECUTIVE_FAILURES {
                // But still check: maybe user already re-authed (auto-recovery below)
            }
        }

        // Step 2: Mark in_progress
        {
            let mut state = self.state.lock().await;
            if state.in_progress {
                return Ok(RefreshOutcome::AlreadyInProgress);
            }
            state.in_progress = true;
            state.last_attempt = Some(Instant::now());
        }

        // Step 3: Call refresh_access_token (outside lock — may involve I/O)
        let result = self
            .oauth_port
            .refresh_access_token(provider_id, REFRESH_THRESHOLD_SECS)
            .await;

        // Step 4: Update state based on result
        let mut state = self.state.lock().await;
        state.in_progress = false;

        match result {
            Ok(RefreshResult::AlreadyFresh { expires_at }) => {
                // Auto-recovery: if we had failures, user must have re-authed manually
                if state.consecutive_transient_failures > 0 {
                    info!(
                        provider_id,
                        previous_failures = state.consecutive_transient_failures,
                        "Auto-recovery: token is fresh, resetting failure state"
                    );
                    state.consecutive_transient_failures = 0;
                    state.backoff_until = None;
                }
                debug!(provider_id, expires_at, "Token not expiring, no refresh needed");
                Ok(RefreshOutcome::NotNeeded)
            }

            Ok(RefreshResult::Refreshed { ref expires_at }) => {
                state.consecutive_transient_failures = 0;
                state.backoff_until = None;

                let _ = self.event_tx.send(TokenEvent::Refreshed {
                    provider_id: provider_id.to_string(),
                    expires_at: expires_at.clone(),
                });

                info!(provider_id, expires_at, "Token refreshed successfully");
                Ok(RefreshOutcome::Refreshed)
            }

            Ok(RefreshResult::NotAuthenticated) => {
                debug!(provider_id, "Not authenticated — no credentials stored");
                Ok(RefreshOutcome::ReauthRequired)
            }

            Ok(RefreshResult::ReauthRequired { ref reason }) => {
                // Terminal failure — immediate, no retries
                state.backoff_until = Some(Instant::now() + Duration::from_secs(86400));

                let _ = self.event_tx.send(TokenEvent::ReauthRequired {
                    provider_id: provider_id.to_string(),
                });

                warn!(
                    provider_id,
                    reason, "Terminal refresh failure — reauth required"
                );
                Ok(RefreshOutcome::ReauthRequired)
            }

            Ok(RefreshResult::TransientFailure { ref message }) => {
                state.consecutive_transient_failures += 1;
                let attempt = state.consecutive_transient_failures;

                if attempt >= MAX_CONSECUTIVE_FAILURES {
                    state.backoff_until = Some(Instant::now() + Duration::from_secs(86400));

                    let _ = self.event_tx.send(TokenEvent::ReauthRequired {
                        provider_id: provider_id.to_string(),
                    });

                    warn!(
                        provider_id,
                        attempt, "Transient failures exhausted — reauth required"
                    );
                    Ok(RefreshOutcome::ReauthRequired)
                } else {
                    if let Some(backoff) = RefreshState::backoff_duration(attempt) {
                        state.backoff_until = Some(Instant::now() + backoff);
                    }

                    let _ = self.event_tx.send(TokenEvent::RefreshFailed {
                        provider_id: provider_id.to_string(),
                        attempt,
                        max_attempts: MAX_CONSECUTIVE_FAILURES,
                    });

                    warn!(
                        provider_id,
                        attempt,
                        max_attempts = MAX_CONSECUTIVE_FAILURES,
                        message,
                        "Transient refresh failure, will retry"
                    );
                    Ok(RefreshOutcome::Failed { attempt })
                }
            }

            Err(e) => {
                // CoreError from the port — treat as transient
                state.consecutive_transient_failures += 1;
                let attempt = state.consecutive_transient_failures;
                warn!(provider_id, error = %e, attempt, "Refresh port error");

                if attempt >= MAX_CONSECUTIVE_FAILURES {
                    state.backoff_until = Some(Instant::now() + Duration::from_secs(86400));
                    let _ = self.event_tx.send(TokenEvent::ReauthRequired {
                        provider_id: provider_id.to_string(),
                    });
                    Ok(RefreshOutcome::ReauthRequired)
                } else {
                    if let Some(backoff) = RefreshState::backoff_duration(attempt) {
                        state.backoff_until = Some(Instant::now() + backoff);
                    }
                    let _ = self.event_tx.send(TokenEvent::RefreshFailed {
                        provider_id: provider_id.to_string(),
                        attempt,
                        max_attempts: MAX_CONSECUTIVE_FAILURES,
                    });
                    Ok(RefreshOutcome::Failed { attempt })
                }
            }
        }
    }
}
```

- [ ] **Step 2: Add module export in mod.rs**

In `crates/oneshim-network/src/oauth/mod.rs`, add near the top with other `mod` declarations (after line 9):

```rust
pub mod refresh_coordinator;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo check -p oneshim-network`
Expected: compiles (may need to add `chrono` to oneshim-network's Cargo.toml if not already present — it should already be there since `OAuthClient` uses `chrono::Utc`)

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-network/src/oauth/refresh_coordinator.rs crates/oneshim-network/src/oauth/mod.rs
git commit -m "feat(network): add TokenRefreshCoordinator with failure taxonomy and auto-recovery"
```

---

### Task 5: Unit tests for TokenRefreshCoordinator

**Files:**
- Modify: `crates/oneshim-network/src/oauth/refresh_coordinator.rs` (add `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write tests at the bottom of refresh_coordinator.rs**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use oneshim_core::ports::oauth::{
        OAuthConnectionStatus, OAuthFlowHandle, OAuthFlowStatus,
    };
    use std::sync::atomic::{AtomicU8, Ordering as AtomicOrdering};

    /// Mock OAuthPort that returns configurable RefreshResult.
    struct MockOAuthPort {
        call_count: AtomicU8,
        refresh_result: tokio::sync::Mutex<RefreshResult>,
    }

    impl MockOAuthPort {
        fn new(result: RefreshResult) -> Self {
            Self {
                call_count: AtomicU8::new(0),
                refresh_result: tokio::sync::Mutex::new(result),
            }
        }

        fn calls(&self) -> u8 {
            self.call_count.load(AtomicOrdering::Relaxed)
        }

        async fn set_result(&self, result: RefreshResult) {
            *self.refresh_result.lock().await = result;
        }
    }

    #[async_trait]
    impl OAuthPort for MockOAuthPort {
        async fn start_flow(&self, _: &str) -> Result<OAuthFlowHandle, CoreError> {
            unimplemented!()
        }
        async fn flow_status(&self, _: &str) -> Result<OAuthFlowStatus, CoreError> {
            unimplemented!()
        }
        async fn cancel_flow(&self, _: &str) -> Result<(), CoreError> {
            unimplemented!()
        }
        async fn get_access_token(&self, _: &str) -> Result<Option<String>, CoreError> {
            unimplemented!()
        }
        async fn revoke(&self, _: &str) -> Result<(), CoreError> {
            unimplemented!()
        }
        async fn connection_status(&self, _: &str) -> Result<OAuthConnectionStatus, CoreError> {
            unimplemented!()
        }
        async fn refresh_access_token(
            &self,
            _provider_id: &str,
            _min_valid_for_secs: i64,
        ) -> Result<RefreshResult, CoreError> {
            self.call_count.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(self.refresh_result.lock().await.clone())
        }
    }

    fn expiry_in_secs(secs: i64) -> String {
        (Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339()
    }

    #[tokio::test]
    async fn not_needed_when_already_fresh() {
        let port = Arc::new(MockOAuthPort::new(RefreshResult::AlreadyFresh {
            expires_at: expiry_in_secs(600),
        }));
        let (tx, _rx) = broadcast::channel(16);
        let coord = TokenRefreshCoordinator::new(port.clone(), tx);

        let result = coord.check_and_refresh("openai").await.unwrap();
        assert_eq!(result, RefreshOutcome::NotNeeded);
        assert_eq!(port.calls(), 1); // port was called (coordinator delegates check to port)
    }

    #[tokio::test]
    async fn refreshes_when_token_expiring() {
        let port = Arc::new(MockOAuthPort::new(RefreshResult::Refreshed {
            expires_at: expiry_in_secs(600),
        }));
        let (tx, mut rx) = broadcast::channel(16);
        let coord = TokenRefreshCoordinator::new(port.clone(), tx);

        let result = coord.check_and_refresh("openai").await.unwrap();
        assert_eq!(result, RefreshOutcome::Refreshed);
        assert_eq!(port.calls(), 1);

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, TokenEvent::Refreshed { .. }));
    }

    #[tokio::test]
    async fn transient_failure_increments_counter() {
        let port = Arc::new(MockOAuthPort::new(RefreshResult::TransientFailure {
            message: "network timeout".into(),
        }));
        let (tx, mut rx) = broadcast::channel(16);
        let coord = TokenRefreshCoordinator::new(port, tx);

        let result = coord.check_and_refresh("openai").await.unwrap();
        assert_eq!(result, RefreshOutcome::Failed { attempt: 1 });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            TokenEvent::RefreshFailed { attempt: 1, max_attempts: 3, .. }
        ));
    }

    #[tokio::test]
    async fn terminal_failure_immediate_reauth() {
        let port = Arc::new(MockOAuthPort::new(RefreshResult::ReauthRequired {
            reason: "invalid_grant: token revoked".into(),
        }));
        let (tx, mut rx) = broadcast::channel(16);
        let coord = TokenRefreshCoordinator::new(port, tx);

        let result = coord.check_and_refresh("openai").await.unwrap();
        assert_eq!(result, RefreshOutcome::ReauthRequired);

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, TokenEvent::ReauthRequired { .. }));
    }

    #[tokio::test]
    async fn three_transient_failures_triggers_reauth() {
        let port = Arc::new(MockOAuthPort::new(RefreshResult::TransientFailure {
            message: "connection refused".into(),
        }));
        let (tx, mut rx) = broadcast::channel(16);
        let coord = TokenRefreshCoordinator::new(port, tx);

        // Failure 1
        coord.check_and_refresh("openai").await.unwrap();
        let _ = rx.try_recv();
        // Clear backoff
        { coord.state.lock().await.backoff_until = None; }
        // Failure 2
        coord.check_and_refresh("openai").await.unwrap();
        let _ = rx.try_recv();
        // Clear backoff
        { coord.state.lock().await.backoff_until = None; }
        // Failure 3
        let result = coord.check_and_refresh("openai").await.unwrap();
        assert_eq!(result, RefreshOutcome::ReauthRequired);

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, TokenEvent::ReauthRequired { .. }));
    }

    #[tokio::test]
    async fn auto_recovery_resets_failure_count() {
        let port = Arc::new(MockOAuthPort::new(RefreshResult::TransientFailure {
            message: "timeout".into(),
        }));
        let (tx, _rx) = broadcast::channel(16);
        let coord = TokenRefreshCoordinator::new(port.clone(), tx);

        // Fail twice
        coord.check_and_refresh("openai").await.unwrap();
        { coord.state.lock().await.backoff_until = None; }
        coord.check_and_refresh("openai").await.unwrap();
        { coord.state.lock().await.backoff_until = None; }

        assert_eq!(coord.state.lock().await.consecutive_transient_failures, 2);

        // Simulate user re-auth → port now returns AlreadyFresh
        port.set_result(RefreshResult::AlreadyFresh {
            expires_at: expiry_in_secs(600),
        }).await;

        let result = coord.check_and_refresh("openai").await.unwrap();
        assert_eq!(result, RefreshOutcome::NotNeeded);
        assert_eq!(coord.state.lock().await.consecutive_transient_failures, 0);
    }

    #[tokio::test]
    async fn backoff_prevents_immediate_retry() {
        let port = Arc::new(MockOAuthPort::new(RefreshResult::TransientFailure {
            message: "error".into(),
        }));
        let (tx, _rx) = broadcast::channel(16);
        let coord = TokenRefreshCoordinator::new(port.clone(), tx);

        // Fail once — sets 2-min backoff
        coord.check_and_refresh("openai").await.unwrap();

        // Second call should be in backoff
        let result = coord.check_and_refresh("openai").await.unwrap();
        assert_eq!(result, RefreshOutcome::BackingOff);
        assert_eq!(port.calls(), 1);
    }

    #[test]
    fn backoff_duration_schedule() {
        assert_eq!(RefreshState::backoff_duration(1), Some(Duration::from_secs(120)));
        assert_eq!(RefreshState::backoff_duration(2), Some(Duration::from_secs(300)));
        assert_eq!(RefreshState::backoff_duration(3), None);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo test -p oneshim-network -- refresh_coordinator`
Expected: all 8 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-network/src/oauth/refresh_coordinator.rs
git commit -m "test(network): unit tests for TokenRefreshCoordinator including auto-recovery"
```

---

## Chunk 3: Scheduler Integration + DI Wiring

### Task 6: Add scheduler config constant

**Files:**
- Modify: `src-tauri/src/scheduler/config.rs:40` (add constant after `REDACTED_WINDOW_TITLE`)

- [ ] **Step 1: Add the constant**

After line 40 (`REDACTED_WINDOW_TITLE`), add:

```rust
/// OAuth token refresh check interval (seconds).
pub(super) const OAUTH_REFRESH_INTERVAL_SECS: u64 = 120;
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo check -p oneshim-app`
Expected: pass

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/scheduler/config.rs
git commit -m "feat(scheduler): add OAUTH_REFRESH_INTERVAL_SECS constant"
```

---

### Task 7: Add coordinator field and builder to Scheduler

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs:24-41` (Scheduler struct)
- Modify: `src-tauri/src/scheduler/mod.rs:88-91` (builder methods)

- [ ] **Step 1: Add import and field**

Add import at the top of `src-tauri/src/scheduler/mod.rs`:

```rust
use oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator;
```

Add field to `Scheduler` struct (after `focus_analyzer` field at line 40):

```rust
pub(super) oauth_coordinator: Option<Arc<TokenRefreshCoordinator>>,
```

Initialize as `None` in `new()` constructor (add after `focus_analyzer: None,` at line 74).

- [ ] **Step 2: Add builder method**

After `with_focus_analyzer()` (line 88-91), add:

```rust
pub fn with_oauth_coordinator(mut self, coordinator: Arc<TokenRefreshCoordinator>) -> Self {
    self.oauth_coordinator = Some(coordinator);
    self
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo check -p oneshim-app`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/scheduler/mod.rs
git commit -m "feat(scheduler): add oauth_coordinator field and builder method"
```

---

### Task 8: Add spawn_oauth_refresh_loop

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs` (add loop function + wire into run_scheduler_loops)

- [ ] **Step 1: Add the loop method to `impl Scheduler`**

Before `run_scheduler_loops()` (line 647), add this `&self` method:

```rust
/// Periodically check and refresh OAuth tokens.
pub(super) fn spawn_oauth_refresh_loop(
    &self,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Option<tokio::task::JoinHandle<()>> {
    use super::config::OAUTH_REFRESH_INTERVAL_SECS;
    use oneshim_core::ports::oauth::TokenEvent;
    use std::time::Duration;

    let coordinator = self.oauth_coordinator.as_ref()?.clone();

    Some(tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(Duration::from_secs(OAUTH_REFRESH_INTERVAL_SECS));
        let mut event_rx = coordinator.subscribe();
        let mut last_reauth_notify: Option<tokio::time::Instant> = None;
        let provider_id = "openai".to_string();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match coordinator.check_and_refresh(&provider_id).await {
                        Ok(outcome) => {
                            debug!(provider_id = %provider_id, ?outcome, "OAuth refresh tick");
                        }
                        Err(e) => {
                            warn!(provider_id = %provider_id, error = %e, "OAuth refresh error");
                        }
                    }
                }
                event = event_rx.recv() => {
                    if let Ok(TokenEvent::ReauthRequired { ref provider_id }) = event {
                        let should_notify = last_reauth_notify
                            .map_or(true, |t| t.elapsed() > Duration::from_secs(300));
                        if should_notify {
                            warn!(
                                provider_id = %provider_id,
                                "OAuth re-authentication required — user must reconnect"
                            );
                            // TODO: emit Tauri event `oauth-reauth-required` when AppHandle
                            // is threaded into the scheduler (follow-up improvement)
                            last_reauth_notify = Some(tokio::time::Instant::now());
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    debug!("OAuth refresh loop shutting down");
                    break;
                }
            }
        }
    }))
}
```

- [ ] **Step 2: Wire the loop into run_scheduler_loops()**

In `run_scheduler_loops()` (line 647), after the last `spawn` call (event_snapshot, ~line 701), add:

```rust
// 10. OAuth token refresh (conditional — returns None if no coordinator)
let oauth_task = self.spawn_oauth_refresh_loop(shutdown_rx.clone());
```

In the shutdown abort section (where all tasks are aborted, ~lines 717-725), add:

```rust
if let Some(task) = oauth_task {
    task.abort();
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo check -p oneshim-app`
Expected: pass

- [ ] **Step 4: Run all tests**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo test --workspace`
Expected: all pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scheduler/loops.rs
git commit -m "feat(scheduler): add spawn_oauth_refresh_loop as 10th loop"
```

---

### Task 9: Wire coordinator in setup.rs

**Files:**
- Modify: `src-tauri/src/setup.rs:223` (after oauth_port creation)
- Modify: `src-tauri/src/setup.rs:297-307` (`run_agent()` call site)
- Modify: `src-tauri/src/setup.rs:588-596` (`run_agent()` function signature)
- Modify: `src-tauri/src/setup.rs:700-701` (scheduler builder in `run_agent()`)

- [ ] **Step 1: Create coordinator in setup() after oauth_port**

After `let oauth_port = create_oauth_port(&config_dir);` (line 223, inside `#[cfg(feature = "server")]`), add:

```rust
#[cfg(feature = "server")]
let oauth_coordinator = {
    use oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator;
    use oneshim_core::config::AiAccessMode;

    if matches!(config.ai_provider.access_mode, AiAccessMode::ProviderOAuth) {
        oauth_port.as_ref().map(|port| {
            let (token_event_tx, _) = tokio::sync::broadcast::channel(32);
            Arc::new(TokenRefreshCoordinator::new(
                Arc::clone(port),
                token_event_tx,
            ))
        })
    } else {
        None
    }
};
```

**Note:** No `#[cfg(not(feature = "server"))]` fallback is needed here. The `run_agent` function uses `Option<Arc<TokenRefreshCoordinator>>` which the scheduler handles as `None` gracefully (the loop simply doesn't spawn). We use a cfg-gated type alias to avoid referencing `oneshim_network` types when the feature is disabled:

```rust
// Type alias to avoid referencing oneshim_network when server feature is off
#[cfg(feature = "server")]
type OAuthCoordinator = Option<Arc<oneshim_network::oauth::refresh_coordinator::TokenRefreshCoordinator>>;
#[cfg(not(feature = "server"))]
type OAuthCoordinator = Option<()>;  // Never Some — satisfies the type system
```

- [ ] **Step 2: Add coordinator parameter to run_agent()**

Update the `run_agent` function signature (line 588) to accept the coordinator. Use the cfg-gated type alias:

```rust
async fn run_agent(
    storage: Arc<dyn StorageService>,
    scheduler_storage: Arc<dyn SchedulerStorage>,
    focus_storage: Arc<dyn FocusStorage>,
    data_dir: PathBuf,
    config: AppConfig,
    offline_mode: bool,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    event_tx: Option<broadcast::Sender<RealtimeEvent>>,
    oauth_coordinator: OAuthCoordinator,
) -> Result<()> {
```

- [ ] **Step 3: Pass coordinator at the call site**

Update the `run_agent()` call in `setup()` (line 298) to pass `oauth_coordinator`:

```rust
#[cfg(feature = "server")]
let agent_oauth_coordinator = oauth_coordinator.clone();
#[cfg(not(feature = "server"))]
let agent_oauth_coordinator: OAuthCoordinator = None;
// ... inside handle.spawn:
if let Err(e) = run_agent(
    agent_storage,
    agent_scheduler_storage,
    agent_focus_storage,
    agent_data_dir,
    agent_config,
    false,
    shutdown_rx,
    agent_event_tx,
    agent_oauth_coordinator,
)
```

- [ ] **Step 4: Wire coordinator into scheduler builder**

In `run_agent()`, after the scheduler builder chain (line 701, after `.with_focus_analyzer(focus_analyzer)`), add:

```rust
#[cfg(feature = "server")]
if let Some(coord) = oauth_coordinator {
    scheduler = scheduler.with_oauth_coordinator(coord);
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo check -p oneshim-app`
Expected: pass

- [ ] **Step 6: Run all tests**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo test --workspace`
Expected: all pass

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/setup.rs
git commit -m "feat(setup): wire TokenRefreshCoordinator into scheduler DI"
```

---

## Chunk 4: Frontend UI

### Task 10: Add i18n keys

**Files:**
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/en.json`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/ko.json`

- [ ] **Step 1: Add English keys**

In `en.json`, inside the `"settingsOAuth": { ... }` nested object (at line 611), before its closing `}` (at line 632), add:

```json
"statusExpiringSoon": "Token expiring soon",
"statusExpired": "Token expired",
"reauthRequired": "Re-authentication required",
"reauthDescription": "Your OAuth session has expired. Please reconnect."
```

- [ ] **Step 2: Add Korean keys**

In `ko.json`, inside the `"settingsOAuth": { ... }` nested object, before its closing `}`, add:

```json
"statusExpiringSoon": "토큰 만료 임박",
"statusExpired": "토큰 만료됨",
"reauthRequired": "재인증 필요",
"reauthDescription": "OAuth 세션이 만료되었습니다. 다시 연결해 주세요."
```

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/i18n/locales/en.json crates/oneshim-web/frontend/src/i18n/locales/ko.json
git commit -m "feat(i18n): add OAuth token expiry and reauth strings (en/ko)"
```

---

### Task 11: Add expiry badge to OAuthConnectionPanel

**Files:**
- Modify: `crates/oneshim-web/frontend/src/pages/settingSections/OAuthConnectionPanel.tsx`

- [ ] **Step 1: Add expiry badge helper function**

After the imports (line 6) and before the component, add:

```typescript
type ExpiryLevel = 'ok' | 'warning' | 'critical' | 'none';

function getExpiryLevel(expiresAt: string | null | undefined): ExpiryLevel {
  if (!expiresAt) return 'none';
  const remaining = new Date(expiresAt).getTime() - Date.now();
  const minutes = remaining / 60_000;
  if (minutes < 1) return 'critical';
  if (minutes <= 5) return 'warning';
  return 'ok';
}

const EXPIRY_BADGE_STYLES: Record<ExpiryLevel, string> = {
  ok: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
  warning: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200',
  critical: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
  none: 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400',
};
```

- [ ] **Step 2: Add auto-refresh useEffect**

After the existing `useEffect` (after line 80, before `handleConnect`), add:

```typescript
// Auto-refresh status every 60s to update expiry badge
useEffect(() => {
  if (state.phase !== 'connected') return;
  const timer = setInterval(() => refreshStatus(), 60_000);
  return () => clearInterval(timer);
}, [state.phase, refreshStatus]);
```

- [ ] **Step 3: Add expiry badge in the connected state block**

In the connected state rendering section (lines 189-204, the `state.phase === 'connected'` block), after the existing `expires_at` text (line 197, after the `</p>` for expires_at), add:

```tsx
{state.status.expires_at && (() => {
  const level = getExpiryLevel(state.status.expires_at);
  if (level === 'ok') return null;
  const label = level === 'critical'
    ? t('settingsOAuth.statusExpired')
    : t('settingsOAuth.statusExpiringSoon');
  return (
    <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${EXPIRY_BADGE_STYLES[level]}`}>
      {label}
    </span>
  );
})()}
```

- [ ] **Step 4: Build frontend to verify**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh/crates/oneshim-web/frontend && pnpm build`
Expected: builds without errors

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-web/frontend/src/pages/settingSections/OAuthConnectionPanel.tsx
git commit -m "feat(web): add OAuth token expiry badge to connection panel"
```

---

### Task 12: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Run full workspace check**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo check --workspace`
Expected: pass

- [ ] **Step 2: Run full test suite**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo test --workspace`
Expected: all tests pass, including new coordinator and OAuthClient tests

- [ ] **Step 3: Run clippy**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo clippy --workspace`
Expected: no warnings

- [ ] **Step 4: Run fmt check**

Run: `cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.config/superpowers/worktrees/client-rust/oauth-token-refresh && cargo fmt --check`
Expected: pass

- [ ] **Step 5: Verify commit history**

Run: `git log --oneline feat/oauth-token-refresh ^main`
Expected: 11-12 commits, all clean
